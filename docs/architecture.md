# CSE architecture — technical reference

Everything a coding agent (or future maintainer) needs to work on the
codebase: layout, conventions, per-system implementation notes, testing,
and hard-won gotchas. The *behavior* of each system is documented in
`docs/manual.md` — this file covers where it lives in the code and how.
Update both in the same pass as any code change.

## Build, run, test

```
cargo build          # first build is slow; incremental is fast
cargo run            # opens the GUI (main thread) with a terminal (background
                     # thread, reads stdin) alongside it
cargo test           # run the test suite
cargo clippy --all-targets   # must stay clean, like the build warnings
```

Dependencies build without debug info (`[profile.dev.package."*"] debug =
false` in Cargo.toml) — keeps target/ small. There is no CI.

## Two interfaces, one game

`cargo run` starts both interfaces at once, sharing one game: `main.rs`'s
`main` builds a `SharedGame` (`lib.rs`, `Arc<Mutex<Option<Game>>>` — `None`
until a game is started or loaded, from either side), spawns a background
thread running the terminal loop against it, and runs the GUI (`gui/`) on
the main thread (winit event loops must run there). A command from either
side is visible to the other immediately, since there's only ever one
`Game` behind the mutex; the GUI polls a few times a second
(`request_repaint_after`) so terminal-driven changes surface without a
window event. Closing the GUI window or typing `exit`/Ctrl-D/Ctrl-C in the
terminal ends the whole process (the terminal thread can't hand control
back to the main thread, so it calls `std::process::exit` directly).

The terminal thread reads lines from stdin (rustyline: tab completion via
`COMMAND_KEYWORDS` + a filename completer, arrow-key history) and passes
them to `cse::run_shared(command, &shared)` — a thin lock-and-call wrapper
around `cse::run(command, current_game)` in `lib.rs`, which parses via
`command.rs` and dispatches. `run` returns `Result<Option<Game>, Error>`
(`Some` = a new game was created and should replace the current one).

`lib.rs` also hosts the pieces both interfaces share:
`new_game`/`load_game`/`save_game` (`pub(crate)`; the GUI's menu calls them
directly since it builds/loads outside the mutex), and
`report_turn_transition`/`play_pending_ai_turns` (`pub(crate)`, returning
`Vec<String>` so the terminal prints them and the GUI logs them). AI
auto-play runs after a human's `end_turn` and after `new`/`load` (the first
or restored on-turn player can be AI), stopping when a human is on turn or
the scenario scores. Known gap, not guarded: an all-AI scenario with no
`last_turn` would loop forever.

### Adding a command

Update all three of `Command::parse`, `HELP_TEXT` and `COMMAND_KEYWORDS` —
all in command.rs (a test there guards keyword drift) — plus the dispatch
match in `run` (lib.rs), plus the README command table.

## Layering and file map

`core/` is the data model; `procedures/` are pure algorithms on snapshots
(no `Game`/`State` access — the split only pays for itself when the
algorithm is meaty and independently testable, like combat resolution or
the supply flood fill); `game/` is the orchestration layer (turn flow,
orders, scenario content — one module per concern; `Game` keeps its fields
in `game/mod.rs`, submodules add `impl Game` blocks and mark what crosses
module lines `pub(super)`); the interface layer (`command.rs`/`ai.rs`/
`gui/`) talks to `Game`'s public API. Future systems follow the pattern: a
new mechanic = a pure procedure (if warranted) + a `game/` hook; the AI
consumes `Game` like another front-end (`ai.rs`, next to `command.rs`, not
a `game/` submodule); so does the GUI.

```
src/
  main.rs        — spawns the terminal thread against a SharedGame, then runs
                   the GUI on the main thread
  lib.rs         — SharedGame/new_shared_game/run_shared; run(): command
                   dispatch; new_game/load_game/save_game; inspect;
                   report_turn_transition/play_pending_ai_turns
  ai.rs          — the AI opponent: take_turn per faction, consuming Game's
                   public API the same way command.rs does (no new pathfinding
                   or combat logic; simulate powers its attack judgment)
  command.rs     — the command language: Command enum + parse,
                   COMMAND_KEYWORDS, HELP_TEXT
  error.rs       — crate-wide Error type + From impls (io/toml/postcard)
  gui/           — the egui/eframe window. Mirrors game/'s layout: GuiApp's
                   fields in mod.rs (plus render_playing/end_turn/
                   resolve_order and the eframe::App impl), submodules add
                   impl GuiApp blocks:
    menu.rs        — main menu, mid-game Save/Load/New dialogs, MenuAction
                     deferral (see Gotchas), adopt_game
    map_view.rs    — MapView (hex-to-screen mapping + zoom/pan), render_map,
                     terrain/unit/flag/entrenchment-pip drawing,
                     assign_stack_slots
    inspector.rs   — the hex side panel: rosters, Move/Attack buttons,
                     air-operations block
    file_picker.rs — the Browse popup (std::fs::read_dir in an egui::Window)
    test_support.rs— shared test fixtures (cf. game/test_support.rs)
  game/mod.rs    — Game (state + players + turn/phase/date + schedules),
                   Game::build, unit queries (units_at_location/
                   units_of_faction/units_summary), check_mission_range
  game/scenario.rs — the whole game-level .scen TOML schema + parse and
                   load-time validation; domain types it references stay in
                   their domains
  game/turn.rs   — end_turn/begin_turn/status, TurnPhase, TurnSystem (the
                   WEGO seam), turn-start morale drift
  game/orders/   — player orders, one module each: movement.rs (move_unit,
                   MP charging), attack.rs (attack/air_support/simulate
                   validation via prepare_battle, battle orchestration,
                   retreat/rout/shatter/surrender aftermath, AttackReport);
                   a future WEGO order queue plugs in here
  game/victory.rs — score_victory/VictoryReport, victory summary,
                   victory_hexes (feeds the GUI's flag markers)
  game/reinforcements.rs — runtime ScheduledArrival + arrival application
                   and summary
  game/events.rs — event firing (morale/experience nudges, message queue)
                   and summary
  game/supply.rs — faction_supplied_hexes + supply_status_summary (computed
                   fresh per call, nothing persisted)
  game/refit.rs  — turn-start repair/replacements, gated on supply
  game/interdiction.rs — interdict/interdiction_summary,
                   covering_fighter_units (used by game/orders/attack.rs)
  game/detection.rs — is_visible_to/is_unit_visible_to, the fog-of-war
                   display gate used by inspect/units (lib.rs) and the GUI
  game/entrenchment.rs — apply_entrenchment (turn-start fort_level tick +
                   MAX_FORT_LEVEL cap)
  game/test_support.rs — shared #[cfg(test)] scenario fixtures
  core/mod.rs    — State + State::build: resolves a Scenario into runtime
                   State (element rosters instantiated from TOEs here, with
                   validation), starting_strength baseline
  core/map.rs    — Map: HashMap<(u32,u32), Location> + offmap locations;
                   TOML map parsing; cheapest_path_cost (hexx a_star; start
                   hex is free)
  core/location.rs — Location wraps Option<hexx::Hex> (None = offmap),
                   Terrain, TerrainCosts (scenario overrides over code
                   defaults; 0 = impassable)
  core/unit.rs   — Unit (mp_left, fort_level, elements), Toe (mp, range),
                   Element/Device/ElementClass, Size + config structs
  procedures/combat.rs — the pure battle engine: CombatElement snapshots in,
                   BattleReport out; never touches Game/State
  procedures/supply.rs — pure multi-source flood fill (reachable_hexes)
```

## Conventions

- **Config/runtime type split**: `#[serde(untagged)]` breaks postcard (it
  needs self-describing formats), so TOML-facing config types
  (`UnitLocationConfig`, `ScheduledArrivalConfig`) are separate from
  runtime types (`UnitLocation`, `ScheduledArrival` — normally tagged),
  with `From` impls across. Keep any new untagged enums on the config side
  only. Types that are the same shape in TOML and at runtime
  (`ScenarioEvent`, `SupplySource`) need no split.
- **Randomness**: anything that rolls dice takes `&mut impl rand::Rng` from
  the caller — the command loop passes `rand::rng()`, tests pass
  `StdRng::seed_from_u64(...)`. Never iterate a HashMap where order reaches
  the RNG: unit lists feeding battles are sorted by name first
  (`units_at_location`), and the AI walks its stacks via a BTreeMap.
- **Errors**: crate-local `Error { error_message }` with `From` impls for
  io/toml/postcard; command handlers return `Result<Option<Game>, Error>`.
- **Hex coordinates**: offset coordinates, `OffsetHexMode::Even`,
  `HexOrientation::Pointy`. Conversion happens inside `Location` (and
  `gui/map_view.rs` for drawing); the rest of the code speaks (x, y) u32.
- **Name registries**: `State` keeps `HashMap<String, _>` for
  units/toe/elements; units are addressed by name everywhere past the
  initial index-based `move` command.
- **Summaries return Strings**: every report (`victory`, `supply`, `units`,
  schedules...) is a `Game` method returning `String`; interfaces print or
  log it. The game layer does no I/O.
- **Dead config fields**: fields deserialized but not yet read
  (`Scenario.game_version`, `MapFile.width/height`, `VictoryHex.name` in
  places) carry `#[allow(dead_code)]`; remove the attribute when a system
  starts using them. The build and clippy are warning-free — keep both so.
- **File formats**: `.map`/`.scen` are TOML, `.sav` is postcard (binary
  serde) of the whole `Game`. Transient fields (`pending_event_messages`)
  are `#[serde(skip)]`.
- **Docs split**: behavior → `docs/manual.md`; implementation →
  this file; agent guidelines only → `CLAUDE.md`; direction →
  `docs/roadmap.md`; parked ideas → `docs/ideas.md`. Update manual +
  architecture in the same pass as the code they describe.

## Per-system implementation notes

What's non-obvious per system, beyond the file map above. Behavior details
live in the manual.

- **Scenario loading**: `game/scenario.rs` owns the schema and validation
  (players non-empty, victory hexes/supply sources on the map, event and
  supply-source factions known, arrival units/destinations real).
  `State::build` (`core/mod.rs`) validates element/TOE referential
  integrity (elements non-empty devices, TOEs reference defined elements,
  units reference defined TOEs/factions, stat overrides name TOE members)
  and computes `starting_strength` per faction (the victory baseline).
  Morale/experience inheritance (element override → unit → faction default
  → 50) resolves here, at build time.
- **Turn flow**: `Game::end_turn` advances `TurnPhase`/turn/date and
  triggers `begin_turn` for the faction coming on turn:
  `apply_scheduled_arrivals` → `apply_scheduled_events` → `apply_refit` →
  `apply_entrenchment` → MP refill + `reset_interdiction_coverage` +
  morale drift, in that order (events land before the drift so a delta
  steers the same turn's drift target). `begin_turn` only fires from
  `end_turn`, so turn-1 arrivals/events for the very first mover get an
  explicit pass at the end of `Game::build`. Every (unit, turn) pair's
  owning faction gets exactly one `begin_turn` per turn number — the
  schedule summaries infer pending/fired status from `turn` alone, no
  "executed" flag needed.
- **Combat orchestration** (`game/orders/attack.rs`): `prepare_battle`
  validates (adjacency, single factions per side, turn ownership) and
  builds the `CombatElement` snapshots; `attack`/`air_support` persist
  results afterwards, `simulate` never does — all three share it. The
  aftermath order matters: experience gain (before losses reshape
  rosters), losses, retreat execution, advance, then morale shifts (once
  routs are known). `BattlePlan.attacker_names` is ground-only — an
  air-support unit joins the snapshot but not the name lists, which is
  what keeps it out of the advance and morale shift while its element
  losses/experience (snapshot-driven) still persist.
- **Combat engine** (`procedures/combat.rs`): per-instance snapshots (one
  `CombatElement` per ready squad/gun/vehicle; damaged sit out) carry
  everything resolution needs — cv, morale, experience, vulnerability,
  armored/air-domain/targeting flags, devices, fort_level — so future
  stats extend the snapshot builder and modifier math, not the control
  flow. Rounds fire simultaneously (hits collected, then applied).
  Severity worst-of on double hits. Constants at the top of the file are
  the tuning knobs (RANGE_BANDS, DISRUPT/DAMAGE_CHANCE, RETREAT_ODDS,
  FORT_CV_BONUS_PER_LEVEL); `morexp_modifier` is the single function to
  swap for a different stat curve.
- **Interdiction**: coverage lives on `Game`
  (`interdiction_coverage: HashMap<unit name, Vec<hex>>`), not on `Unit` —
  same separation as `scheduled_arrivals`/`events`/`supply_sources`.
  `prepare_battle` extends the defender snapshot with
  `covering_fighter_units(defender_faction, to)`, *excluding any unit
  already present as a ground defender at that hex* — without the
  exclusion a unit interdicting its own hex was double-counted and could
  underflow its `ready` bucket in `apply_battle_losses` (a debug-build
  panic that killed the whole process; see the
  `_is_not_double_counted_as_a_defender` test).
- **Airfields**: `Toe.range: Option<u32>` (`None` = unlimited, every
  pre-existing TOE's behavior). `Game::check_mission_range` is a no-op for
  an offmap unit or a range-less TOE; otherwise compares
  `Location::distance_to`. Called from `prepare_battle`'s air_support
  branch and `Game::interdict`.
- **Detection**: `is_visible_to`/`is_unit_visible_to` are pure queries
  (like victory's holder checks), no persisted state, no `procedures/`
  split (a handful of lines over `Location::distance_to`). Display-only by
  design: they gate `units_summary`'s `units_by_name`, `inspect` (lib.rs),
  and the GUI's map markers/inspector roster — never
  `units_at_location`/`units_of_faction`, which order validation, the AI
  and the GUI's buttons rely on.
- **Supply**: `procedures::supply::reachable_hexes` is a pure multi-source
  flood fill (`Location::neighbour_coords` frontier, `TerrainCosts::cost`
  stops at impassable, a caller-supplied blocked set stops at enemy
  hexes). `game/supply.rs` assembles the inputs; nothing persists;
  `game/refit.rs` is the one consumer with gameplay effect.
- **Victory**: `end_turn` returns `Some(VictoryReport)` once `last_turn`
  completes; `run`/the GUI print it. Scoring only — nothing gates further
  commands afterwards. `victory_hexes` feeds the GUI's flag markers.
- **AI** (`ai.rs`): per stack (BTreeMap of hex → unit names), attack the
  best adjacent enemy hex if `simulate` predicts ≥
  `ATTACK_RETREAT_THRESHOLD` (0.6) retreat rate over `SIMULATION_RUNS`
  (20), else `move_unit` toward the nearest unheld victory hex (fallback:
  nearest enemy) — full jump first, best single step if that errs. Every
  decision bottoms out in `Game`'s public order methods.
- **GUI**: `PendingOrder` arms Move/Attack/AirSupport from the inspector;
  the next map click resolves it (`resolve_order` — unit index 0 always;
  stack-picking is a known deferred nicety). Interdict applies immediately
  to the inspected hex. Save/Load/New/Quit all defer through
  `pending_menu_action: Option<MenuAction>`, applied only after `ui()`
  drops its lock (see Gotchas). `adopt_game` gives a fresh New/Load the
  same AI-auto-play + turn-1-event-drain treatment `run` gives one.
  `MapView` folds zoom into its `HexLayout` scale and pan into
  screen-space offsets; every draw method scales sizes off `MapView.size`
  (`HEX_SIZE * zoom`) so everything grows/shrinks together;
  `assign_stack_slots` (pure, tested) offsets stacked units sideways.

## Testing

Tests are in-crate unit tests (`#[cfg(test)] mod tests` per module) since
most types are crate-private. Fixtures are inline TOML strings — shared
scenario snippets live in `game/test_support.rs` and `gui/test_support.rs`
— and a few tests load the real `scenarios/*.scen` / `maps/basic_map.map`
via `concat!(env!("CARGO_MANIFEST_DIR"), ...)` so shipped-config drift
breaks the build. Battle tests seed `StdRng` for exact reproducibility;
`three_vs_one(morale)` is the standard "defender surely loses" fixture for
retreat-path tests.

### Manual GUI verification

GUI changes need a live check — this sandbox has no input-injection tool,
so the technique is:

1. `mkfifo` a pipe in the scratchpad; hold its write end open from one
   backgrounded shell (`exec 3>pipe && sleep 600`) so the terminal thread's
   stdin doesn't EOF.
2. Launch `./target/debug/cse < pipe > stdout.log 2>&1` backgrounded, then
   drive it by echoing commands into the pipe (`new scenarios/...`,
   `status`, ...) — the terminal thread applies them to the shared game the
   window is rendering.
3. Screenshot with `spectacle` (`grim` does not work here — "compositor
   doesn't support the screen capture protocol"). Note the CSE window must
   be visible/on top for the screenshot to show it.
4. `exit` through the pipe shuts everything down; kill the sleep holder.

Temporary debug hacks to force a state worth screenshotting are fine —
always reverted immediately after the screenshot confirms.

## Gotchas

- **eframe's default `wgpu` backend renders nothing in this project's
  sandboxed dev VM** — the window opens, the UI callback runs every frame
  (confirmed with debug logging), but no frame ever visibly presents.
  `glow` (OpenGL, via Mesa/llvmpipe here) works correctly in the same
  environment. Cargo.toml disables eframe's default features and enables
  `glow` explicitly for this reason — don't switch it back to (or add)
  `wgpu` without re-confirming rendering actually shows up via screenshot.
- **winit event loops can only be created once per process** — `gui::run`
  is the only `eframe::run_native` call in the codebase, made once from
  `main`'s main thread. Don't add a second one (e.g. a respawned/child
  window); route any future "another window" need through the existing
  `GuiApp`.
- **Locking `SharedGame` inside a method that also needs `&mut self`**:
  `self.shared.lock()` borrows the field, but a later `self.method(...)`
  needs all of `self` — they conflict while the guard lives. `GuiApp::ui`
  locks a clone of the `Arc` (`self.shared.clone()`), which doesn't borrow
  `self` at all. Keep new guard-across-`&mut self` code on this pattern.
- **`std::sync::Mutex` is not reentrant**: `MenuAction`s confirmed inside
  `render_playing`/`render_main_menu` (which run under `ui()`'s lock) are
  only *armed* there and applied after the guard drops
  (`apply_pending_menu_action`) — `Load`/`New` lock `shared` themselves in
  `adopt_game` and would deadlock otherwise.
- **Same shape of problem inside one `&mut Game` call**: a `&Location`/
  `Vec<&Unit>` read borrows `game` as long as the binding lives, so holding
  one across a later `game.interdict(...)` (`&mut Game`) won't compile.
  `render_inspector` re-fetches each read in its own small scope right
  before it's needed instead of binding once at the top.
- Scenario element names must match TOE element names exactly —
  `State::build` validates this, and `builds_the_real_basic_scenario` /
  `builds_the_real_frontline_sector_scenario` guard the shipped scenarios.
