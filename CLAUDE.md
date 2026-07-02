# CSE — Combat Simulation Engine

A WW2 operational wargame engine in Rust, inspired by Gary Grigsby's *War in the East*.
Design goal: scalable engine-first architecture — terminal-driven simulation core now,
richer frontends later. Unit/TOE/element attributes come from easy-to-edit TOML config
files, so scenarios are data, not code.

**Standing rules from the author:**
- After every code change, update README.md and this file in the same pass
  (commands/usage in README; architecture, conventions, gotchas, roadmap here).
- Prioritize clean, simple, reviewable code — the author reviews everything and may
  pick the project up solo later. Work in small chunks rather than big diffs.
  Modularity/expandability matter (the end goal is a complex game), but get them
  from clean seams and data-driven design, not speculative abstraction.
- The project doubles as a learning exercise in AI-driven development; the README
  carries a disclaimer that code is heavily AI-generated.

## Build & run

```
cargo build          # first build is slow (Bevy); incremental is fast
cargo run            # starts the interactive command loop (reads stdin)
```

```
cargo test           # run the test suite
```

Tests are in-crate unit tests (`#[cfg(test)] mod tests` per module) since most types
are crate-private. Fixtures are inline TOML strings; a few tests load the real
`scenarios/basic_scenario.scen` / `maps/basic_map.map` via
`concat!(env!("CARGO_MANIFEST_DIR"), ...)` so shipped-config drift breaks the build.
There is no CI.

## Command loop

`main.rs` reads lines from stdin and passes them to `cse::run(command, current_game)`
in `lib.rs`, which parses and dispatches. Commands:

- `new <path.scen>` — start a game from a scenario file (e.g. `new scenarios/basic_scenario.scen`)
- `load <path.sav>` / `save <path.sav>` — postcard (binary serde) save/load of the whole `Game`
- `inspect <x> <y>` or `inspect <offmap name>` — show location + units there with
  their element rosters (ready/damaged per element)
- `units` / `units detail` — list all units
- `move <x1> <y1> <x2> <y2> <unit_index>` — teleport-style move (no distance/cost checks yet)
- `attack <x1> <y1> <x2> <y2>` — units at hex 1 attack units at hex 2; prints a battle
  report and persists losses (retreat outcomes reported but not executed yet)
- `view` — open the Bevy map window in a detached subprocess (terminal stays usable;
  Esc closes the window; can be called repeatedly)
- `help` — print the command list (HELP_TEXT in lib.rs)
- `exit`

When adding a command, update all three of: `Command::parse`, `HELP_TEXT`, and
`COMMAND_KEYWORDS` (drives tab completion; a lib.rs test guards keyword drift).

## Design docs

- `docs/combat_design.md` — living combat design doc: WitE2 findings (rules readable at
  dornshuld.com/rules/wite2), the current resolution model, deliberate deviations,
  open questions. Update it whenever the combat engine changes.
- `docs/ideas.md` — parking lot for future game ideas; save new ideas there instead
  of losing them in conversation.

## Architecture

```
src/
  main.rs        — stdin loop + `--view <snapshot>` subprocess entry
  lib.rs         — command parsing/dispatch, save/load, view subprocess
                   spawning (spawn_view_subprocess/run_view_subprocess), Error type
  game/mod.rs    — Game (state + players + turn/phase), Scenario TOML schema, move_unit,
                   attack (builds combat snapshots, applies losses back)
  core/mod.rs    — State::build: resolves a Scenario into runtime State (units get
                   their element rosters instantiated from their TOE here)
  core/map.rs    — Map: HashMap<(u32,u32), Location> + offmap locations; TOML map parsing
  core/location.rs — Location wraps Option<hexx::Hex> (None = offmap), Terrain enum
  core/unit.rs   — Unit, Toe, Element, ElementClass, Size + config structs
  visualiser.rs  — self-contained Bevy 0.15 debug map view (see below)
  procedures/combat.rs — pure fires-based battle engine: CombatElement snapshots in,
                   BattleReport out; never touches Game/State (see docs/combat_design.md)
  utils/         — empty
```

Key data model (all TOML-configurable, see `scenarios/basic_scenario.scen`):
- **Element** — a weapon system type (rifle squad, Pz IV…): `class`, `cv`, `accuracy`,
  `range` (meters), `v_inf`, `v_arm` (vulnerability vs inf/armor fire)
- **TOE** — table of equipment: named list of (element, amount), with validity dates
- **Unit** — a division etc.: points at a TOE by name; holds live `ElementInUnit`
  counts (`ready`/`damaged`); location is the `UnitLocation` enum (`OnMap(coords)` /
  `Offmap(name)`); scenario TOML writes it as `location = { x = 3, y = 3 }` or
  `location = "GE Reserve"`
- **Map files** (`maps/*.map`) — TOML: per-hex terrain + named offmap boxes ("GE Reserve")

Conventions used throughout:
- Hex coords: offset coordinates, `OffsetHexMode::Even`, `HexOrientation::Pointy`
  (conversion happens inside `Location`; the rest of the code speaks (x, y) u32)
- Lookups by name: `State` keeps `HashMap<String, _>` registries for units/toe/elements
- Errors: crate-local `Error { error_message }` with `From` impls for io/toml/postcard;
  command handlers return `Result<Option<Game>, Error>` (Some = a new game was created)
- Randomness: anything that rolls dice takes `&mut impl rand::Rng` from the caller —
  the command loop passes `rand::rng()`, tests pass `StdRng::seed_from_u64(...)`.
  For seed-reproducibility, never iterate a HashMap where order reaches the RNG
  (unit lists feeding battles are sorted by name first).

## Gotchas

- **glam version split**: hexx 0.23 uses glam 0.30, Bevy 0.15 uses glam 0.29. Their
  `Vec2`s are incompatible. In `visualiser.rs`, hexx's re-exported `Vec2` is aliased
  as `HexVec2` for `HexLayout`; positions cross into Bevy as plain f32 x/y. Keep any
  new hexx↔bevy math on this pattern (or unify versions when upgrading).
- **winit event loops can only be created once per process**, so `view` re-invokes
  the binary as `cse --view <snapshot-file>` (postcard temp file, deleted by the
  child after reading; child stdout/stderr nulled; a reaper thread `wait()`s the
  child to avoid zombies). Never call `visualiser::launch` directly from the
  command loop — a second call would crash the process.
- `.map`/`.scen`/`.sav` are all project file formats: TOML, TOML, postcard binary.
- **`#[serde(untagged)]` breaks postcard** (it needs self-describing formats), so
  TOML-facing config types (`UnitLocationConfig`) are separate from runtime types
  (`UnitLocation`, normally tagged). Keep any new untagged enums on the config side
  only, with a `From` impl to the runtime type.
- The visualiser gets a `MapSnapshot` (plain serde data, no game references) — keep
  it decoupled; don't hand it `&State`.
- Fields deserialized from config but not yet read (`Scenario.start_date`,
  `MapFile.width`…) carry `#[allow(dead_code)]`; remove the attribute when a
  system starts using them. The build is warning-free and `cargo clippy` is
  clean — keep both that way.
- Scenario element names must match TOE element names exactly — `State::build`
  validates this (errors on unknown TOE and on TOE entries referencing undefined
  elements), and `builds_the_real_basic_scenario` guards the shipped scenario.

## Roadmap (agreed with the author)

1. **Combat resolution** — v1 implemented (fires-based, closing range bands,
   disrupt/damage/destroy hits, CV odds outcome; docs/combat_design.md is the
   spec). Next combat steps, in rough order: retreat execution, a `simulate`
   command for tuning, rate of fire + dual AP/HE fire values, morale/experience.
2. Turn/phase system — `end_turn`, alternating players, date advancement
   (`turn_length` exists in scenarios but is unused).
3. Movement rules — adjacency/cost/MP budget on the hex grid.
4. Supply system — later, WitE-style depth.
5. Visualiser growth — the Bevy `MapViewPlugin` is meant to accrete systems
   (hover-inspect, selection) and maybe become the real frontend eventually.
