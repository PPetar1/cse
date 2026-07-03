# CSE — Combat Simulation Engine

An operational wargame engine in Rust, inspired by Gary Grigsby's *War in the East*;
the end goal is a finished game that improves on it (see `docs/roadmap.md`).
Engine-first architecture — terminal-driven simulation core now, richer frontends
later. Unit/TOE/element attributes come from easy-to-edit TOML config files, so
scenarios are data, not code.

WW2 is the first target, not the boundary: the engine should model any fires-based
conflict from roughly Napoleonic to modern, with air — and possibly naval — warfare
eventually. Keep design choices era-agnostic: mechanics generic, era flavor in
scenario data. (`ElementClass` is the main code-side taxonomy to watch as eras
multiply.)

**Standing rules from the author:**
- After every code change, update README.md and this file in the same pass
  (commands/usage in README; architecture, conventions, gotchas, current focus here).
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

Dependencies build without debug info (`[profile.dev.package."*"] debug = false`
in Cargo.toml) — keeps target/ at ~2 GB instead of >10 GB. Avoid `--release`
builds on this machine; they compile a second full Bevy tree.

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
  their element rosters (ready/damaged/morale/experience per element)
- `units` / `units detail` — list all units
- `move <x1> <y1> <x2> <y2> <unit_index>` — single-hex move to an adjacent hex;
  costs MP per destination terrain (`Terrain::movement_cost`, Water impassable);
  only the on-turn faction's units
- `attack <x1> <y1> <x2> <y2>` — units at hex 1 attack units at the adjacent hex 2
  (on-turn faction only; `simulate` is exempt from both gates); prints a battle
  report and persists losses + experience gain; beaten defenders retreat (with
  attrition), rout, shatter, or surrender, and the attackers advance into the
  vacated hex (free, automatic)
- `simulate <x1> <y1> <x2> <y2> <n>` — fight that attack n times state-untouched,
  print hold/retreat rates + average losses (the balance-tuning tool)
- `end_turn` — pass control to the next player (IGO-UGO); when every player has
  moved, the turn counter and in-game date advance (`turn_length` days)
- `status` — scenario name, turn, date, faction to move
- `view` — open the Bevy map window in a detached subprocess (terminal stays usable;
  Esc closes the window; can be called repeatedly)
- `help` — print the command list (HELP_TEXT in lib.rs)
- `exit`

When adding a command, update all three of: `Command::parse`, `HELP_TEXT`, and
`COMMAND_KEYWORDS` (drives tab completion; a lib.rs test guards keyword drift).

## Design docs

- `docs/roadmap.md` — the long-term compass: Part 1 is the phased path to a
  playable prototype; Part 2 the (non-exhaustive) feature areas separating the
  prototype from the finished game; the "better than WitE" pillars are design
  tiebreakers. Update it when direction changes, not per-feature.
- `docs/combat_design.md` — living combat design doc: WitE2 findings (rules readable at
  dornshuld.com/rules/wite2), the current resolution model, deliberate deviations,
  open questions. Update it whenever the combat engine changes.
- `docs/ideas.md` — parking lot for future game ideas; save new ideas there instead
  of losing them in conversation.

## Architecture

```
src/
  main.rs        — rustyline prompt loop (tab completion via COMMAND_KEYWORDS +
                   FilenameCompleter, history) + `--view <snapshot>` subprocess entry
  lib.rs         — command parsing/dispatch, save/load, view subprocess
                   spawning (spawn_view_subprocess/run_view_subprocess), Error type
  game/mod.rs    — Game (state + players + turn/phase/date + TurnSystem), Scenario TOML
                   schema, end_turn/status, move_unit, attack (builds combat snapshots,
                   applies losses back, executes retreats/surrenders — destination +
                   attrition rules live here)
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
- **Element** — a weapon system type (rifle squad, Pz IV…): `class`, `cv`,
  `vulnerability` (armor for vehicles, exposure otherwise) and a non-empty list of
  **devices** (validated by `State::build`)
- **Device** — one weapon of an element (rifle/LMG volley, tank main gun, coax MG):
  `accuracy` (to-hit), `range` (meters), `rate_of_fire` (shots per combat round),
  `soft_attack`/`hard_attack` (hit effect vs unarmored/armored — targets are engaged
  with the value matching their hardness, per `ElementClass::is_armored`)
- **TOE** — table of equipment: named list of (element, amount), with validity dates
  and `mp` — the per-turn movement budget of units on this TOE. The runtime
  `Unit.mp_left` counts it down; `Game::begin_turn` refills it (and hosts all
  future turn-start effects) when the owning faction comes on turn
- **Unit** — a division etc.: points at a TOE by name; holds live `ElementInUnit`
  buckets (`ready`/`damaged` counts plus per-element `morale`/`experience`);
  location is the `UnitLocation` enum (`OnMap(coords)` / `Offmap(name)`);
  scenario TOML writes it as `location = { x = 3, y = 3 }` or `location = "GE Reserve"`
- **Morale/experience** (0–100) live on the elements; the scenario sets them at
  any granularity, most specific wins: `[[units.elements]]` override → `[[units]]`
  → faction default on `[[players]]` (stored on the runtime `Player` so events can
  shift it later) → 50. In combat, experience gates element commitment, both scale
  element CV ×(1 + mor/100 + exp/100) (`morexp_modifier` in combat.rs), and the
  unit's strength-weighted `average_morale()` gates routs. Battles shift both:
  everyone gains experience, winners rally / losers sag morale (routs doubly)
- **Map files** (`maps/*.map`) — TOML: per-hex terrain + named offmap boxes ("GE Reserve")
- **Turn system** — scenario-selectable via `turn_system = "IgoUgo"` (optional,
  the default). Only IGO-UGO exists; the matches on `TurnSystem` are the seam
  where a future WEGO mode (order queue, simultaneous resolution) plugs in

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

## Current focus

The full phased plan lives in `docs/roadmap.md`. Phase 0 (combat core) is done:
fires-based battles with device-level weapons, morale/experience with battle
feedback, routs/shatters/surrenders, and the `simulate` tuning tool
(`docs/combat_design.md` is the spec).

**Now: Phase 1 — the game loop.** The turn clock is in (`end_turn`, alternating
players IGO-UGO, real dates advancing by `turn_length`, scenario-selectable
`TurnSystem`), move/attack are gated to the on-turn faction with attacks
adjacency-checked, and movement is real: single-hex moves, terrain entry costs,
TOE MP budgets refilled at turn start, and attackers advance after a won
battle. Remaining: morale recovery over time. Combat knob retuning via
`simulate` continues alongside.
