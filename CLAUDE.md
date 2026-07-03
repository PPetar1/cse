## Approach
- Read existing files before writing. Don't re-read unless changed.
- Thorough in reasoning, concise in output.
- Skip files over 100KB unless required.
- No sycophantic openers or closing fluff.
- No emojis or em-dashes.
- Do not guess APIs, versions, flags, commit SHAs, or package names. Verify by reading code or docs before asserting.

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
- `move <x1> <y1> <x2> <y2> <unit_index>` — move to any reachable hex: charges
  the cheapest-path cost (`Map::cheapest_path_cost`, hexx a_star; terrain entry
  costs from scenario `[terrain_costs]`, impassable and enemy hexes block);
  enemy-occupied destinations are rejected (that's `attack`); only the on-turn
  faction's units
- `attack <x1> <y1> <x2> <y2>` — units at hex 1 attack units at the adjacent hex 2
  (on-turn faction only; `simulate` shares this validation via `prepare_battle`);
  prints a battle report and persists losses + experience gain; beaten defenders
  retreat (with attrition), rout, shatter, or surrender, and the attackers
  advance into the vacated hex (free, automatic)
- `simulate <x1> <y1> <x2> <y2> <n>` — fight that attack n times state-untouched,
  print hold/retreat rates + average losses (the balance-tuning tool)
- `end_turn` — pass control to the next player (IGO-UGO); when every player has
  moved, the turn counter and in-game date advance (`turn_length` days). The
  faction coming on turn gets turn-start effects (`Game::begin_turn`): MP
  refill + morale drift toward the faction default
- `status` — scenario name, turn, date, faction to move
- `view` — open the Bevy map window in a detached subprocess (terminal stays usable;
  Esc closes the window; can be called repeatedly); the window auto-updates as
  commands change the game (it polls the session's snapshot file, which `run`
  rewrites after every successful command)
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
                   spawning (spawn_view_subprocess/run_view_subprocess) + live
                   snapshot refresh (refresh_view/write_view_snapshot), Error type
  game/mod.rs    — Game (state + players + turn/phase/date + TurnSystem), Scenario TOML
                   schema, end_turn/status, move_unit, attack (builds combat snapshots,
                   applies losses back, executes retreats/surrenders — destination +
                   attrition rules live here)
  core/mod.rs    — State::build: resolves a Scenario into runtime State (units get
                   their element rosters instantiated from their TOE here)
  core/map.rs    — Map: HashMap<(u32,u32), Location> + offmap locations; TOML map
                   parsing; cheapest_path_cost (hexx a_star; start hex is free)
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
  everyone gains experience, winners rally / losers sag morale (routs doubly).
  At its faction's turn start, morale also drifts toward the faction default
  (`MORALE_RECOVERY_STEP`, gentler than battle shifts); experience is permanent
- **Map files** (`maps/*.map`) — TOML: per-hex terrain + named offmap boxes ("GE Reserve")
- **Terrain costs** — the scenario's `[terrain_costs]` table (terrain name → MP
  to enter, 0 = impassable) layered over code defaults
  (`Terrain::default_movement_cost`); runtime lookup via `State.terrain_costs`
  (`TerrainCosts::cost`)
- **Turn system** — scenario-selectable via `turn_system = "IgoUgo"` (optional,
  the default). Only IGO-UGO exists; the matches on `TurnSystem` are the seam
  where a future WEGO mode (order queue, simultaneous resolution) plugs in
- **Victory conditions** — the scenario's optional `[victory_conditions]` table:
  `last_turn` (the last turn played; absent = the scenario never scores itself),
  `[[victory_conditions.hexes]]` (x, y, points, optional name — flat points to
  whoever holds the hex when scoring happens), `points_per_percent_enemy_destroyed`
  / `points_per_percent_own_lost` (multipliers on % of starting element strength
  gone, per faction). `State::build` computes `starting_strength` (ready +
  damaged elements per faction, onmap and offmap, at scenario load) as the
  baseline destruction/loss percentages are measured against; hex coordinates
  are validated against the map at load time. `Game::end_turn` returns
  `Some(VictoryReport)` the moment `last_turn` is completed, and `run` prints
  it (scores per faction, then the winner or a draw on a tie) — nothing yet
  stops further commands afterward, so this is scoring/reporting only, not a
  hard game-over gate. The `victory` command (`Game::victory_conditions_summary`)
  shows the same conditions and each objective hex's current holder at any
  time; `Game::victory_hexes` feeds the hexes to the map view as flag markers
  (see visualiser gotcha below)

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
  the binary as `cse --view <snapshot-file>` (child stdout/stderr nulled; a reaper
  thread `wait()`s the child to avoid zombies). Never call `visualiser::launch`
  directly from the command loop — a second call would crash the process.
- **The view snapshot file is a live channel**, not a one-shot handoff: one postcard
  file per game session (`cse_view_<pid>.snapshot` in the temp dir), created by
  `view`, rewritten by `run` after every successful command (write-to-.tmp +
  rename so the child never sees a partial file), polled twice a second by the
  view window (byte-compare, then despawn/respawn all `MapEntity` entities),
  deleted by `main` on exit — the child must never delete it itself, but a
  missing/unreadable file is its cue to close (`reload_on_change` sends
  `AppExit`), so the view window doesn't outlive the process that opened it.
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

The full phased plan lives in `docs/roadmap.md`. Phase 0 (combat core) is done
(`docs/combat_design.md` is the spec). Phase 1 (the game loop) is done: turn
clock (`end_turn`/`status`, IGO-UGO, real dates, scenario-selectable
`TurnSystem`), move/attack gated to the on-turn faction, adjacency-checked
attacks, MP budgets with terrain costs, attacker advance after retreat, and
turn-start morale recovery.

**Now: Phase 2 — the first winnable scenario.** Victory conditions are done:
objective hexes with flat points, plus points for enemy strength destroyed and
a penalty for strength lost, scored at a scenario's `last_turn` (`end_turn`
prints the result); the `victory` command shows the conditions and current
hex holders at any time, and the map view flags objective hexes with their
point value. `basic_scenario.scen` carries a `[victory_conditions]` table and
8 units (up from 3) spread across a larger map (10x8, up from 6x6) — untuned,
for exercising the new mechanics. Still open: scheduled reinforcements/
withdrawals from offmap boxes and first scenario events.
Landmark: win — or lose — a game of CSE. Combat knob retuning via `simulate`
continues alongside.
