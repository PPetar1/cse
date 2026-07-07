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
- Read existing files before writing. Don't re-read unless changed.
- Thorough in reasoning, concise in output.
- Skip files over 100KB unless required.
- No sycophantic openers or closing fluff.
- No emojis or em-dashes.
- Do not guess APIs, versions, flags, commit SHAs, or package names. Verify by reading code or docs before asserting.
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
cargo build          # first build is slow; incremental is fast
cargo run            # opens the GUI (main thread) with a terminal (background
                     # thread, reads stdin) alongside it — see "Two interfaces,
                     # one game" below
```

Dependencies build without debug info (`[profile.dev.package."*"] debug = false`
in Cargo.toml) — keeps target/ small.

```
cargo test           # run the test suite
```

Tests are in-crate unit tests (`#[cfg(test)] mod tests` per module) since most types
are crate-private. Fixtures are inline TOML strings — the shared scenario snippets
live in `game/test_support.rs` — and a few tests load the real
`scenarios/*.scen` / `maps/basic_map.map` via
`concat!(env!("CARGO_MANIFEST_DIR"), ...)` so shipped-config drift breaks the build.
There is no CI.

## Two interfaces, one game

`cargo run` starts both interfaces at once, sharing one game: `main.rs`'s
`main` builds a `SharedGame` (`lib.rs`, `Arc<Mutex<Option<Game>>>` —
`None` until a game is started or loaded, from either side), spawns a
background thread running the terminal loop against it, and runs the GUI
(`gui.rs`) on the main thread (winit event loops must run there). A command
from either side is visible to the other immediately, since there's only
ever one `Game` behind the mutex — see "GUI" under Architecture below for
how the window picks up terminal-driven changes. Closing the GUI window or
typing `exit`/Ctrl-D/Ctrl-C in the terminal ends the whole process (the
terminal thread can't hand off control gracefully, so it calls
`std::process::exit` directly rather than just returning).

The terminal thread reads lines from stdin and passes them to
`cse::run_shared(command, &shared)` (a thin lock-and-call wrapper around
`cse::run(command, current_game)` in `lib.rs`, which parses via `command.rs`
and dispatches). Commands:

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
- `air_support <x1> <y1> <x2> <y2> <unit name>` — flies one owned unit's
  elements into that attack as extra firers, for that battle only; same
  report as `attack`, but the unit never advances and stays wherever it
  started (see "Air support" below)
- `interdict <x> <y> <unit name>` — a fighter-capable unit covers that hex
  this turn (up to 3 hexes per unit); any battle there this turn or the
  opponent's next automatically pulls it in (see "Interdiction" below)
- `interdiction` — show which units are covering which hexes
- `simulate <x1> <y1> <x2> <y2> <n>` — fight that attack n times state-untouched,
  print hold/retreat rates + average losses (the balance-tuning tool)
- `end_turn` — pass control to the next player (IGO-UGO); when every player has
  moved, the turn counter and in-game date advance (`turn_length` days). The
  faction coming on turn gets turn-start effects (`Game::begin_turn`): MP
  refill + morale drift toward the faction default
- `status` — scenario name, turn, date, faction to move
- `help` — print the command list (HELP_TEXT in command.rs)
- `exit`

When adding a command, update all three of: `Command::parse`, `HELP_TEXT`, and
`COMMAND_KEYWORDS` — all in command.rs (a test there guards keyword drift) —
plus the dispatch match in `run` (lib.rs).

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

Layering: `core/` is the data model, `procedures/` are pure algorithms on
snapshots (no `Game`/`State` access), `game/` is the orchestration layer
(turn flow, orders, scenario content — one module per concern; `Game` keeps
its fields in `game/mod.rs`, submodules add `impl Game` blocks and mark
what crosses module lines `pub(super)`), and the interface layer
(`command.rs`/`ai.rs`/`gui.rs`) talks to `Game`'s public API. Future systems
follow the pattern: supply = `procedures/supply.rs` + a `game/` hook; the AI
consumes `Game` like another front-end (`ai.rs`, next to `command.rs`, not a
`game/` submodule); the real UI does too (`gui.rs`, Phase 6 — see "GUI"
below) — and now shares that one `Game` live with the terminal instead of
each owning a separate one (see "Two interfaces, one game" above).

```
src/
  main.rs        — spawns the terminal thread (rustyline prompt loop: tab completion
                   via COMMAND_KEYWORDS + FilenameCompleter, history) against a
                   SharedGame, then runs the GUI on the main thread
  lib.rs         — SharedGame/new_shared_game/run_shared (the terminal thread's
                   entry point); run(): command dispatch, save/load/inspect helpers
                   (new_game/load_game/save_game are pub(crate) — gui.rs's main menu
                   calls them directly, see "GUI"); report_turn_transition/
                   play_pending_ai_turns (pub(crate), return Vec<String>) are shared
                   with gui.rs, not just the terminal
  ai.rs          — the AI opponent: take_turn per faction, consuming Game's public API
                   the same way command.rs does (attack/move_unit/simulate, no new
                   pathfinding or combat logic)
  gui.rs         — the real interface (Phase 6): an eframe/egui window over a
                   SharedGame, main menu when it's empty, map/orders once it isn't
                   (see "GUI" below)
  command.rs     — the command language: Command enum + parse, COMMAND_KEYWORDS, HELP_TEXT
  error.rs       — crate-wide Error type + From impls (io/toml/postcard)
  game/mod.rs    — Game (state + players + turn/phase/date), Game::build, unit queries,
                   check_mission_range (shared by air_support/interdict, see "Airfields")
  game/scenario.rs — the whole game-level .scen TOML schema (Scenario, Player, UnitConfig,
                   ScheduledArrivalConfig, ScenarioEvent, VictoryConditions…) + parse and
                   load-time validation; domain types it references stay in their domains
  game/turn.rs   — end_turn/begin_turn/status, TurnPhase, TurnSystem (the WEGO seam),
                   turn-start morale drift, interdiction-coverage reset
  game/orders/   — player orders, one module each: movement.rs (move_unit, MP charging),
                   attack.rs (attack/air_support/simulate validation, battle orchestration,
                   retreat/rout/shatter/surrender aftermath, AttackReport); a future WEGO
                   order queue plugs in here
  game/victory.rs — victory scoring/report + the `victory` summary and map-view hex feed
  game/reinforcements.rs — runtime ScheduledArrival + arrival application and summary
  game/events.rs — event firing (morale/experience nudges, message queue) and summary
  game/supply.rs — on-demand supply status query (supply_status_summary);
                   faction_supplied_hexes also backs game/refit.rs
  game/refit.rs  — turn-start repair (damaged -> ready) and replacements
                   (missing -> ready), gated on supply, see "Refit" below
  game/interdiction.rs — interdict/interdiction_summary, covering_fighter_units (used by
                   game/orders/attack.rs), see "Interdiction" below
  game/test_support.rs — shared #[cfg(test)] scenario fixtures for the game test suites
  core/mod.rs    — State::build: resolves a Scenario into runtime State (units get
                   their element rosters instantiated from their TOE here)
  core/map.rs    — Map: HashMap<(u32,u32), Location> + offmap locations; TOML map
                   parsing; cheapest_path_cost (hexx a_star; start hex is free)
  core/location.rs — Location wraps Option<hexx::Hex> (None = offmap), Terrain enum
  core/unit.rs   — Unit, Toe (mp, range), Element, ElementClass, Size + config structs
  procedures/combat.rs — pure fires-based battle engine: CombatElement snapshots in,
                   BattleReport out; never touches Game/State (see docs/combat_design.md)
  procedures/supply.rs — pure multi-source flood fill (reachable_hexes) over the
                   map, blocked by enemy-occupied hexes and impassable terrain
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
  time; `Game::victory_hexes` exists for a map view to flag those hexes, but
  no view does yet — the old Bevy visualiser did before it was retired
  (Phase 6 slice 3), and porting flag markers to the egui GUI is still open
- **Reinforcements/withdrawals** — `[[reinforcements]]` and `[[withdrawals]]`
  (each entry: `unit`, `turn`, `location` — reuses `UnitLocationConfig`, so a
  hex or an offmap box name both work either direction) are mechanically
  identical: at scenario load both parse into a single `Vec<ScheduledArrival>`
  on `Game` (`ScheduledArrivalConfig` -> `ScheduledArrival` via `From`, same
  config/runtime split as `UnitLocationConfig`/`UnitLocation`, needed because
  postcard save files must carry the still-pending schedule forward). Unit
  names and location targets are validated against the built `State` the same
  way victory hexes are. `Game::begin_turn` applies any entry whose `turn`
  matches the current turn and whose unit belongs to the faction coming on
  turn, before the MP refill/morale drift — since `begin_turn` only fires from
  `end_turn`, turn-1 arrivals for the very first mover are applied once more
  explicitly right after `Game::build`. The `reinforcements` command
  (`Game::reinforcement_schedule_summary`) lists every entry with a
  pending/arrived status inferred from `turn` vs. the current turn (no
  separate "already executed" flag needed, since each (unit, turn) pair's
  owning faction gets exactly one `begin_turn` call for that turn number)
- **Scenario events** — `[[events]]` (`turn`, `faction`, `message`, optional
  `morale_delta`/`experience_delta`, both 0 by default). No config/runtime
  split needed (unlike locations, nothing here is TOML-only), so `Scenario`
  parses straight into `Vec<ScenarioEvent>`. `event.faction` is validated
  against `scenario.players` at load. `Game::begin_turn` calls
  `apply_scheduled_events` before capturing the morale drift target, so an
  event's `morale_delta` on a faction's default lands in time to steer that
  same turn's drift; deltas are applied via `clamp_percent` to keep morale/
  experience in 0-100. Fired messages queue in `Game::pending_event_messages`
  (`#[serde(skip)]` — transient, so it's fine that a save/load drops anything
  not yet printed) until `run` drains them with `take_event_messages` — once
  after `end_turn`, and once right after building a fresh game, since
  `Game::build` also runs the turn-1 explicit pass described above. The
  `events` command (`Game::event_schedule_summary`) lists the schedule with a
  pending/fired status, same heuristic and same caveat as reinforcements'
- **Supply** — the scenario's `[[supply_sources]]` (`faction`, `x`, `y`)
  declare where each faction's connectivity is traced back to; validated at
  load like victory hexes (hex on the map) and events (`faction` known).
  `procedures::supply::reachable_hexes` is a pure multi-source flood fill
  from those hexes — `Location::neighbour_coords` expands the frontier,
  `TerrainCosts::cost` stops it at impassable terrain, a caller-supplied
  `blocked` set (the enemy's on-map hexes) stops it the same way
  `move_unit`'s pathfinding already does; a source hex the enemy currently
  holds doesn't seed the flood. `Game::supply_status_summary` (the `supply`
  command) computes this fresh per faction on every call — nothing persists.
  Deliberately no degradation or surrender for cut-off units ("a pocket
  starves and surrenders", the roadmap's stated landmark, was dropped by
  design) — `game::refit` is the one thing that reads supply status.
- **Refit** — turn-start repair and replacement, combined into one mechanic
  (not a separate "pulled from the line" state). For every on-map unit of
  the faction coming on turn that `faction_supplied_hexes` says is connected
  to supply: each element bucket repairs `ceil(damaged / REPAIR_STEP)` back
  to ready, then gains `ceil(missing / REPLACEMENT_STEP)` replacements where
  `missing` is the gap to the TOE-prescribed count for that element type —
  both tapering the same way morale/experience recovery already does, and
  both naturally bounded (`div_ceil(x, step) <= x`) so neither needs a
  separate cap. Cut-off units get neither. Runs in `Game::begin_turn` after
  scheduled arrivals/events, before the MP refill/morale-drift loop.
- **AI opponent** — a scenario's `[[players]]` entry gets an optional
  `controller` (`Human`/`Ai`, default `Human`, `PlayerController` in
  scenario.rs); `Game::current_player_is_ai`/`current_faction` (turn.rs) read
  it. `ai::take_turn` (top-level `ai.rs`, not a `game/` submodule — see
  Architecture above) plays one AI faction's turn: per on-map stack
  (`Game::units_of_faction` grouped by hex), attack the best adjacent enemy
  hex if `Game::simulate` predicts a defender-retreat rate clearing
  `ATTACK_RETREAT_THRESHOLD`, otherwise move every unit in the stack toward
  the nearest victory hex this faction doesn't hold (falling back to the
  nearest enemy unit if the scenario has none) — trying the full
  `Game::move_unit` jump first and a single best neighbouring step if that
  errs. No new pathfinding or combat logic anywhere in `ai.rs`; every
  decision bottoms out in `Game`'s existing public order methods, the same
  ones `command.rs` calls. `lib.rs`'s `run()` auto-plays consecutive
  AI-controlled turns (`play_pending_ai_turns`) after a human's `end_turn`
  and after `new`/`load` (covers the case where the first or restored
  on-turn player is AI), stopping once a human is on turn or the scenario's
  score fires. Known gap, not guarded against: an all-AI scenario with no
  `last_turn` would loop here forever — every scenario so far assumes at
  least one human seat.
- **Air support** — the first Phase 5 mechanic: `Game::air_support`
  (`game/orders/attack.rs`) folds one owned unit's elements into an ongoing
  ground attack's attacker snapshot for that battle only, via a
  `prepare_battle` parameter — same validation as `attack` (adjacency, turn
  ownership, a ground stack must already be present at the source hex),
  plus checks that the air unit belongs to the attacking faction and isn't
  already part of that ground stack. Deliberately: the air unit never
  advances into a vacated hex and doesn't share in the post-battle morale
  shift (both use the ground-only `attacker_names`/`defender_names`), but
  its element losses and experience gain persist automatically, since those
  read the full `CombatElement` snapshot directly.
- **Air superiority** — domain-restricted targeting, not a separate
  air-to-air phase: `ElementClass::GroundAttack`/`Fighter` are air-domain
  (`is_air_domain`), `Element.anti_air` lets a *ground* element also engage
  air, and `Device.air_attack` is the attack value used against an
  air-domain target. `CombatElement` precomputes `air_domain`/
  `can_target_ground`/`can_target_air` once per snapshot; `fire_round`
  (`procedures/combat.rs`) filters each firer's target pool to
  domain-compatible Ready targets before picking one — a strict
  generalization, since that pool equals the old shared one whenever
  nothing air-domain/anti-air is present. Fighters only ever hit air;
  bombers hit both (weakly against air); ordinary ground elements hit
  ground only unless flagged `anti_air`.
- **Interdiction** — `Game::interdict(unit, target)` (`game/
  interdiction.rs`) declares that a fighter-capable unit covers a hex, up
  to `INTERDICTION_HEX_LIMIT` (3) hexes at a time; `interdiction_coverage`
  (a `HashMap<unit name, Vec<hex>>` on `Game`, not on `Unit` — same
  separation-of-concerns as `scheduled_arrivals`/`events`/
  `supply_sources`) tracks it. `prepare_battle` unconditionally extends the
  defender snapshot with `Game::covering_fighter_units(defender_faction,
  to)` — whatever's covering the target hex, for *any* battle there, not
  just `air_support` ones (this replaced slice 2's `air_support.is_some()`
  gated, unconditional `faction_fighter_units`, now deleted). Coverage
  clears at the covering faction's own next turn start
  (`reset_interdiction_coverage`, called from `Game::begin_turn`), so a
  declaration survives exactly through the opponent's next turn and must
  be redeclared every time the covering faction acts again. The
  `interdiction` command shows current coverage. See "Air support"/"Air
  superiority"/"Interdiction" in docs/combat_design.md.
- **Airfields** — `Toe.range: Option<u32>` (`None` = unlimited, every TOE's
  behavior before this field existed) caps how many hexes
  `air_support`/`interdict` can reach from a unit's current on-map
  location — its "airfield" is just wherever it sits, `UnitLocation::OnMap`
  like any ground unit, no new type needed. `Game::check_mission_range`
  (`game/mod.rs`) is a no-op for a still-offmap unit (nothing to measure a
  distance from) or a TOE with no range set; otherwise it compares
  `Location::distance_to` against the range, called from both
  `prepare_battle`'s `air_support` branch and `Game::interdict`. See
  "Airfields" in docs/combat_design.md.
- **GUI** — `gui.rs` (Phase 6, launched by `cargo run` — see "Two
  interfaces, one game" above) opens an eframe/egui window over a
  `SharedGame`. `GuiApp::ui` locks a clone of the `Arc` (not `self.shared`
  directly — locking the field itself would borrow all of `self` for the
  guard's lifetime and conflict with the `&mut self` render calls) and
  polls it a few times a second (`request_repaint_after`) so terminal-driven
  changes surface without needing a window event to trigger a redraw. `None`
  renders a main menu (`GuiApp::render_main_menu`): a scenario-path field and
  "New Game" button, a save-path field and "Load Game" button, and "Quit"
  (`std::process::exit`). Both New/Load call `crate::new_game`/
  `crate::load_game` directly (the same `pub(crate)` helpers `lib.rs::run`
  uses for the terminal's `new`/`load`) rather than routing through `run`,
  since there's nothing to lock yet; `GuiApp::adopt_game` then gives the
  result the same auto-play-pending-AI-turns-and-drain-turn-1-events
  treatment `run`'s post-build block gives a freshly built/loaded game,
  before publishing it into `shared`.
  `Some(game)` renders the map/orders (`GuiApp::render_playing`): `MapView`
  (hex layout + panel/map centering) maps hex coordinates to `egui::Pos2`
  for drawing and back for click hit-testing
  (`hexx::HexLayout::world_pos_to_hex`); clicking a hex sets `selected_hex`,
  which drives a side panel listing that hex's units and rosters (the same
  information `inspect` prints), plus Move/Attack buttons if the hex holds
  a unit of `Game::current_faction()`. Clicking either arms
  `GuiApp.pending_order`; the *next* map click resolves it
  (`GuiApp::resolve_order`) via `move_unit`/`attack`/`air_support` instead
  of just re-selecting, logging the result (or error) to `GuiApp.log`,
  shown in a bottom panel. The inspector's "Air operations" block (visible
  regardless of who holds the inspected hex, since interdiction covers
  hexes you don't occupy) is a unit combo box
  (`game.units_of_faction(game.current_faction())`, into `GuiApp
  .selected_air_unit` by name) plus two buttons: "Air Support" arms a
  `PendingOrder::AirSupport { air_unit }` the same way Move/Attack do
  (needs the inspected hex to hold your ground units, same as Attack);
  "Interdict" needs no second click — the target hex is already the
  inspected one — so it calls `Game::interdict` immediately. Each of that
  method's hex/unit lookups is scoped tightly (see the `SharedGame` locking
  gotcha below for the general pattern) so no shared borrow of `game`
  is still alive when `interdict` needs `&mut Game` partway through
  rendering the panel. An "End Turn" button calls `Game::end_turn` and the
  same `report_turn_transition`/`play_pending_ai_turns` (`lib.rs`,
  `pub(crate)`, returning `Vec<String>` so both the terminal and the GUI
  can consume them) the terminal's `end_turn` command uses. Mid-game
  save/load still needs the terminal — no in-app file dialogs beyond the
  main menu's New/Load.
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

- **eframe's default `wgpu` backend renders nothing in this project's
  sandboxed dev VM** — the window opens, `App::ui` runs every frame
  (confirmed with debug logging), but no frame ever visibly presents.
  `glow` (OpenGL, via Mesa/llvmpipe here) works correctly in the same
  environment. `Cargo.toml` disables eframe's default features and enables
  `glow` explicitly for this reason — don't switch it back to (or add)
  `wgpu` without re-confirming rendering actually shows up, e.g. via a
  screenshot tool (`spectacle` worked in this environment; `grim` didn't —
  "compositor doesn't support the screen capture protocol").
- **winit event loops can only be created once per process** — `gui::run`
  is the only call to `eframe::run_native` anywhere in the codebase, made
  once from `main`'s main thread. Don't add a second one (e.g. a
  respawned/child-process GUI); route any future need for "another window"
  through the existing `GuiApp` instead.
- **Locking `SharedGame` inside a method that also needs `&mut self`**:
  `self.shared.lock()` borrows `self.shared` specifically (a field
  projection), but a later `self.some_method(...)` call needs to borrow all
  of `self` — the two conflict if the lock guard is still alive. `GuiApp::ui`
  works around this by locking a clone of the `Arc` (`self.shared.clone()`),
  which doesn't borrow `self` at all. Keep new code that holds a guard
  across `&mut self` calls on this pattern.
- **Same shape of problem inside one `&mut Game` call**: a `&Location`/
  `Vec<&Unit>` read (`game.state.map.get_location`/`game.units_at_location`)
  borrows `game` for as long as that binding is used, so holding one across
  a later `game.interdict(...)` (needs `&mut Game`) in the same function
  won't compile. `GuiApp::render_inspector` avoids this by re-fetching each
  read in its own small scope right before it's needed, rather than binding
  `location`/`units` once at the top and reusing them across the whole
  function.
- `.map`/`.scen`/`.sav` are all project file formats: TOML, TOML, postcard binary.
- **`#[serde(untagged)]` breaks postcard** (it needs self-describing formats), so
  TOML-facing config types (`UnitLocationConfig`) are separate from runtime types
  (`UnitLocation`, normally tagged). Keep any new untagged enums on the config side
  only, with a `From` impl to the runtime type.
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

**Phase 2 — the first winnable scenario — is now feature-complete.** Victory
conditions: objective hexes with flat points, plus points for enemy strength
destroyed and a penalty for strength lost, scored at a scenario's `last_turn`
(`end_turn` prints the result); the `victory` command shows the conditions
and current hex holders at any time, and the map view flags objective hexes
with their point value. Scheduled reinforcements/withdrawals:
`[[reinforcements]]`/`[[withdrawals]]` move a unit on/off the map at a given
turn; the `reinforcements` command shows the schedule and its pending/arrived
status. Scenario events: `[[events]]` fire a message plus an optional
morale/experience nudge to a faction's default at a given turn (feeding the
same turn's morale drift target); the `events` command shows the schedule and
its pending/fired status. All three share one mechanism — apply at
`Game::begin_turn` for the faction/turn they're keyed to, with an explicit
first pass at `Game::build` for turn-1 entries, since `begin_turn` otherwise
only fires from `end_turn`. `basic_scenario.scen` exercises all three (two
reinforcements, a withdrawal, two events, `[victory_conditions]`) as a dev/
test sandbox — untuned, 8 units on a 10x8 map (up from 3 on 6x6).
`scenarios/frontline_sector.scen` is the actual landmark deliverable: a
division-scale scenario with a continuous 10-hex Soviet line (one rifle
division per hex along y=4, so there's no gap to walk through unopposed), a
mixed German attack (infantry — the new `GE_inf_squad`/`GE_37mm_pak`/
`GE_105mm_lefh` elements and `GE_inf_div_41` TOE, plus Panzer divisions
concentrated at two breakthrough points), reinforcements feeding both sides,
a withdrawal, three narrative events, and three objective hexes. Landmark
reached: win — or lose — a game of CSE. Combat knob retuning via `simulate`
continues alongside.

**Phase 3 — the living army — scope settled, two of three slices landed.**
By author's call, this phase deliberately drops the roadmap's unit
degradation/surrender (encirclement kills a pocket outright); the prototype
stops at repair/replacement stalling for cut-off units, so the stated
landmark ("a pocket starves and surrenders") will not be hit — that's
intentional, not unfinished. Supply connectivity tracing:
`[[supply_sources]]` (per faction, on-map hexes) plus `procedures::supply::
reachable_hexes` (a pure flood fill blocked by enemy hexes and impassable
terrain, mirroring `move_unit`'s pathfinding rules) give
`Game::supply_status_summary` (the `supply` command) a live supplied/cut-off
read on every on-map unit; both shipped scenarios declare sources
(`frontline_sector.scen`'s German source doubles as the "Rear supply depot"
victory hex). Refit: every turn, units connected to supply repair damaged
elements and receive replacements for missing ones (both tapering, capped
by construction); cut-off units get neither — supply's one gameplay effect
in this prototype. Still open: replacements/repair could later gate on more
than raw connectivity (distance, throughput) if Part 2's detailed logistics
ever revisits this; not planned now.

**Phase 4 — an opponent — done.** A scenario faction can be marked
`controller = "Ai"`; `ai.rs`'s simple rule-based `take_turn` attacks
adjacent enemies at favorable `simulate`-predicted odds and otherwise
advances toward the nearest unclaimed victory hex or enemy, using only
`Game`'s existing public order methods (see "AI opponent" above).
`lib.rs::run` auto-plays every AI faction's turn in sequence after a human's
`end_turn` (and after `new`/`load`, in case the game opens on an AI turn)
until a human is on turn or the scenario scores. `frontline_sector.scen`'s
German side is now AI-controlled — the landmark scenario for "lose to the
machine." The AI's decision-making is intentionally simple by design (the
point was proving the seam, not strength); a stronger AI later replaces
`take_turn`'s internals without touching how it's invoked.

**Phase 5 — combined arms — done, all four slices landed.** Ground
support: the `air_support` command flies one owned unit's elements into an
ongoing ground attack as extra firers for that battle (see "Air support"
above). Air superiority: domain-restricted targeting (see "Air
superiority" above) means fighters only ever fight other air-domain
elements, bombers fight both (weakly against air), and only `anti_air`-
flagged ground elements (both scenarios' 45mm/37mm AT guns are now
dual-purpose) can hit air-domain targets at all. Interdiction: a fighter
unit must `interdict` a hex (up to 3 at a time) before it's pulled into
any battle there. Airfields: both scenarios' air units (a Stuka wing, a
Soviet fighter regiment) now sit on real map hexes — their old offmap
supply/rear-depot hexes — instead of an offmap reserve box, and their TOEs
cap `air_support`/`interdict` missions to 9 hexes from wherever they
currently are (see "Airfields" above); testing values, like the rest of
both scenarios. The AI still doesn't know `air_support`/`interdict` exist,
so in `frontline_sector.scen` (Axis played by the AI) its Stuka wing
currently never flies and its opponent's fighters never get declared —
only a human player can order either today.

**Phase 6 — the real interface — four slices landed.** `cargo run` (see
"Two interfaces, one game" above) opens a real egui/eframe window: the map
(terrain-colored hexes, coordinate labels, faction-colored unit markers)
plus a status header (`Game::status()`) with an End Turn button, click a
hex to select it and see its units/rosters in a side panel. Slice 2 added
order issuing: the inspector's Move/Attack buttons arm a `PendingOrder`,
resolved by the next map click (`GuiApp::resolve_order` — `move_unit`/
`attack`, unit index 0 always, picking which unit in a multi-unit stack
moves is a deferred nicety); a bottom log panel shows status/event/battle-
report/error lines, the window's equivalent of the terminal's scrollback.
Slice 3 replaced the old `cse --gui <scenario.scen>`/`--view` split with a
single `cargo run`: the window now opens on a main menu (New/Load/Quit,
see "GUI" above) instead of requiring a scenario path up front, the
terminal moved to a background thread so both interfaces run at once
against one `SharedGame`, and the old Bevy-based `view` command/
`visualiser.rs`/`view.rs` are gone entirely (dropping the `bevy` dependency
along with them — a much lighter build). `lib.rs`'s
`report_turn_transition`/`play_pending_ai_turns` (`pub(crate)`, returning
`Vec<String>`) are still what let the terminal and the GUI share the exact
same AI-auto-play logic. Slice 4 added the named-unit orders: the
inspector's "Air operations" block (see "GUI" above) picks a unit from a
combo box and offers "Air Support" (arms a `PendingOrder`, resolved by the
next map click, same as Move/Attack) and "Interdict" (applies immediately
to the inspected hex, no second click needed). Still open, in rough order:
mid-game save/load and an in-app scenario picker (only available from the
main menu so far), victory-hex flag markers and multi-unit stacking
offsets (both existed in the old `visualiser.rs`, not yet ported), and
pan/zoom.
