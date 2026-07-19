# CSE architecture — technical reference

Everything a coding agent (or future maintainer) needs to work on the
codebase: layout, conventions, per-system implementation notes, testing,
and hard-won gotchas. Each system's *behavior* is documented in
`docs/manual.md` — this file covers where it lives in the code and how.
Update both in the same pass as any code change.

## Build, run, test

```
cargo build          # first build is slow; incremental is fast
cargo run            # GUI (main thread) + terminal (background thread, stdin)
cargo test
cargo clippy --all-targets   # must stay clean, like the build warnings
```

Dependencies build without debug info (`[profile.dev.package."*"] debug =
false` in Cargo.toml) — keeps target/ small. There is no CI.

## Two interfaces, one game

`cargo run` starts both interfaces against one shared game: `main.rs` is
wiring only — it builds a `SharedGame` (`session.rs`,
`Arc<Mutex<Option<Game>>>` — `None` until a game is started or loaded from
either side), spawns a background thread running `cse::run_terminal`
(`terminal::run_loop`), and runs the GUI (`cse::run_gui`, `gui::run`) on
the main thread (winit event loops must run there). There's only ever one
`Game` behind the mutex, so each side sees the other's commands
immediately; the GUI polls a few times a second (`request_repaint_after`)
so terminal-driven changes surface without a window event. Closing the
window or typing `exit`/Ctrl-D/Ctrl-C ends the whole process (the terminal
thread can't hand control back to the main thread, so it calls
`std::process::exit`).

`terminal::run_loop` reads stdin lines (rustyline: tab completion via
`COMMAND_KEYWORDS` + a filename completer, arrow-key history) into
`run_shared(command, &shared)` — a lock-and-call wrapper around
`run(command, current_game)`, which parses via `terminal/command.rs` and
dispatches. `run` returns `Result<Option<Game>, Error>` (`Some` = a new
game replaces the current one).

`session.rs` hosts what both interfaces share: `new_game`/`load_game`/
`save_game` (`pub(crate)`; the GUI's menu calls them directly since it
builds/loads outside the mutex), `report_turn_transition`/
`play_pending_ai_turns` (`pub(crate)`, returning `Vec<String>` — the
terminal prints them, the GUI logs them), and `activate_game` — the
post-new/load ritual (auto-play any AI faction already on turn, then drain
turn-1 event messages), called from both `terminal::run`'s tail and
`gui::menu::adopt_game` so the two can't drift apart. AI auto-play runs
after a human's `end_turn` and after `new`/`load` (the first or restored
on-turn player can be AI), stopping when a human is on turn or the
scenario scores. Known gap, not guarded: an all-AI scenario with no
`last_turn` would loop forever.

### Adding a command

Update all three of `Command::parse`, `HELP_TEXT` and `COMMAND_KEYWORDS`
in `terminal/command.rs` (a test there guards keyword drift), the dispatch
match in `run` (`terminal/mod.rs`), and the README command table.

A command needing more than one line of input (`reassign_leader`: unit name,
then a second prompt for the leader's name) can't go through `Command`/`run`
at all — that path is one shared string in, one `Result` out, with no room
for a follow-up prompt. It's special-cased in `terminal::run_loop` instead,
ahead of the `run_shared` call, and stays out of the `Command` enum; it's
still in `COMMAND_KEYWORDS` for tab completion, and the keyword-drift test
in `terminal/command.rs` excludes it by name (same as `exit`).

## Layering and file map

`core/` is the data model; `procedures/` are pure algorithms on snapshots
(no `Game`/`State` access — worth the split only when the algorithm is
meaty and independently testable, like combat, the supply flood fill, or
pathfinding);
`game/` is the orchestration layer (one module per concern; `Game` keeps
its fields in `game/mod.rs`, submodules add `impl Game` blocks and mark
what crosses module lines `pub(super)`); `session.rs` is the application
layer both frontends share (persistence, shared-game plumbing, turn-flow
orchestration); the interface layer (`terminal/`/`ai.rs`/`gui/`) talks to
`Game`'s public API. New systems follow the pattern: a pure procedure (if
warranted) + a `game/` hook; the AI and GUI consume `Game` like any other
front-end (`ai.rs` sits next to `terminal/`, not under `game/`).

```
src/
  main.rs        — wiring only: builds a SharedGame, spawns the terminal
                   thread, runs the GUI on the main thread
  lib.rs         — module declarations plus the re-exports main.rs and the
                   frontends need; no function bodies of its own
  session.rs     — the application layer both frontends share: SharedGame/
                   new_shared_game, new_game/load_game/save_game,
                   report_turn_transition/play_pending_ai_turns,
                   activate_game (the post-new/load ritual)
  ai.rs          — the AI opponent: take_turn per faction via Game's public
                   API (no own pathfinding or combat logic)
  terminal/      — the terminal frontend, mirrors gui/'s layout:
    mod.rs         — run/run_shared: command dispatch against a SharedGame;
                     run_loop: the rustyline read-eval-print loop
                     (CommandHelper tab completion) plus the two-step
                     reassign_leader prompt
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
  game/mod.rs    — Game (state + players + turn/phase/date + schedules;
                   `state` is private — see "Sealed state" below),
                   Game::build, InspectTarget, read queries (location/
                   offmap_location/map_locations/unit/units_at_location/
                   units_of_faction/units_not_of_faction/units_by_name/
                   adjacent/distance/units_summary/inspect_summary),
                   check_mission_range
  game/scenario.rs — the whole game-level .scen TOML schema + parse,
                   load-time validation, and build_state (resolves a
                   Scenario into runtime State, reading the map file);
                   domain types stay in their domains
  game/turn.rs   — end_turn/begin_turn/status, TurnPhase, TurnSystem (the
                   WEGO seam), turn-start morale drift
  game/orders/   — player orders, one module each: movement.rs (move_unit,
                   MP charging), attack.rs (attack/air_support/simulate via
                   prepare_battle, battle orchestration, retreat/rout/
                   shatter/surrender aftermath, AttackReport); a future
                   WEGO order queue plugs in here
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
                   covering_fighter_units
  game/leaders.rs — leaders_of_faction/leaders_summary/leader_detail/
                   reassign_leader; assignment lives on Unit.leader, not on
                   Leader (see below)
  game/doctrine.rs — faction-wide doctrine: doctrine_of (feeds CV scaling in
                   prepare_battle), apply_doctrine_battle_result (per-battle
                   personal doctrine gain/loss), apply_doctrine_turn_start
                   (leader/faction feedback, called from begin_turn)
  game/detection.rs — is_visible_to/is_unit_visible_to, the fog-of-war
                   display gate
  game/entrenchment.rs — apply_entrenchment (turn-start fort_level tick +
                   MAX_FORT_LEVEL cap)
  game/test_support.rs — shared #[cfg(test)] scenario fixtures
  core/mod.rs    — State: the runtime data model (map, terrain_costs, units,
                   toe, elements, leaders, supply_sources,
                   starting_strength); a true leaf, no game:: or file I/O —
                   assembly lives in game/scenario.rs::build_state
  core/map.rs    — Map: HashMap<(u32,u32), Location> + offmap locations;
                   TOML map parsing (pathfinding lives in
                   procedures/pathfinding.rs)
  core/location.rs — Location wraps Option<hexx::Hex> (None = offmap),
                   Terrain, TerrainCosts (scenario overrides over code
                   defaults; 0 = impassable)
  core/unit.rs   — Unit (mp_left, fort_level, elements, leader), Toe (mp,
                   range), Element/Device/ElementClass, Size
  core/leader.rs — Leader (name, faction, stats, doctrine), LeaderStats (the
                   seven WitE2 ratings); the runtime type `LeaderConfig`
                   (game/scenario.rs) resolves into
  core/supply.rs — SupplySource (faction, x, y): a faction's supply-source
                   hexes; deserialized straight from TOML, same reasoning
                   as Leader
  procedures/combat.rs — the pure battle engine: CombatElement snapshots in,
                   BattleReport out; never touches Game/State
  procedures/pathfinding.rs — cheapest_path_cost (hexx a_star over a Map;
                   start hex is free)
  procedures/supply.rs — pure multi-source flood fill (reachable_hexes)
```

## Conventions

- **Config/runtime type split**: `#[serde(untagged)]` breaks postcard (it
  needs self-describing formats), so TOML-facing config types
  (`UnitLocationConfig`, `ScheduledArrivalConfig`) are separate from
  runtime types (`UnitLocation`, `ScheduledArrival` — normally tagged),
  with `From` impls across. Keep new untagged enums on the config side
  only. Types the same shape in TOML and at runtime (`ScenarioEvent`,
  `SupplySource`) need no split. A second, unrelated reason for the same
  split: a field whose default depends on other scenario data, resolved in
  `build_state` rather than by a static `#[serde(default = "fn")]` — see
  `UnitConfig`/`ElementStatsConfig` (morale/experience) and
  `LeaderConfig`/`Leader` (doctrine).
- **Randomness**: anything that rolls dice takes `&mut impl rand::Rng`
  from the caller — the command loop passes `rand::rng()`, tests pass
  `StdRng::seed_from_u64(...)`. Never iterate a HashMap where order
  reaches the RNG: unit lists feeding battles are sorted by name
  (`units_at_location`), the AI walks its stacks via a BTreeMap.
- **Errors**: crate-local `Error { error_message }` with `From` impls for
  io/toml/postcard; command handlers return `Result<Option<Game>, Error>`.
- **Hex coordinates**: offset coordinates, `OffsetHexMode::Even`,
  `HexOrientation::Pointy`. Conversion happens inside `Location` (and
  `gui/map_view.rs` for drawing); the rest of the code speaks (x, y) u32.
- **Name registries**: `State` keeps `HashMap<String, _>` for
  units/toe/elements/leaders; units are addressed by name everywhere past
  the index-based `move` command.
- **Summaries return Strings**: every report (`victory`, `supply`,
  `units`, schedules, `leaders`/`leader`...) is a `Game` method returning
  `String`; interfaces print or log it. The game layer does no I/O.
- **Sealed state**: `Game.state` is private — everything outside `game/`
  reads through `Game`'s query methods (`location`/`offmap_location`/
  `map_locations`/`unit`/`units_at_location`/`units_of_faction`/
  `units_not_of_faction`/`units_by_name`/`adjacent`/`distance`), never
  `game.state.*` directly. `game/` submodules still use `self.state` freely
  (field privacy is per module tree, not per file). A new frontend need
  means a new `&`-returning query next to the existing ones, not widened
  field visibility.
- **The `core::` boundary**: a `core::` type's data (fields, enum variants)
  may be read from anywhere — a shape change there surfaces as a compiler
  error at the exact spot needing an update, so it's cheap to allow.
  Calling a `core::` *method* is different: it's behavior, and it may only
  be called from inside `game/` (or `procedures/`, exempt as pure algorithm
  code `game/` calls into, not part of the interface layer). Outside
  `game/`, that behavior must go through a `Game` query instead
  (`adjacent`/`distance` wrap `Location::neighbour_coords`/`distance_to`
  this way). `Game` queries themselves stay narrowly scoped — they never
  return the whole `State` or an unfiltered internal collection.
- **Dead config fields**: fields deserialized but not yet read
  (`Scenario.game_version`, `MapFile.width/height`) carry
  `#[allow(dead_code)]`; remove the attribute when a system starts using
  them.
- **File formats**: `.map`/`.scen` are TOML, `.sav` is postcard (binary
  serde) of the whole `Game`. Transient fields (`pending_event_messages`)
  are `#[serde(skip)]`.
- **Docs split**: behavior → `docs/manual.md`; implementation → this file;
  agent guidelines → `CLAUDE.md`; direction → `docs/roadmap.md`; parked
  ideas → `docs/ideas.md`.

## Per-system implementation notes

What's non-obvious per system, beyond the file map. Behavior lives in the
manual.

- **Scenario loading**: `game/scenario.rs` owns schema and validation
  (players non-empty, victory hexes on the map, event factions known,
  arrival units/destinations real). `build_state` (`game/scenario.rs`)
  validates element/TOE referential integrity (non-empty devices, TOEs
  reference defined elements, units reference defined TOEs/factions, stat
  overrides name TOE members, leader factions are known, unit leader
  assignments name a real same-faction leader with no leader claimed
  twice), supply sources sit on the map and name a known faction, and
  computes `starting_strength` per faction (the victory baseline).
  Morale/experience inheritance (element override → unit → faction
  default → 50) resolves here, at build time.
- **Turn flow**: `Game::end_turn` advances `TurnPhase`/turn/date and
  triggers `begin_turn` for the faction coming on turn:
  `apply_scheduled_arrivals` → `apply_scheduled_events` → `apply_refit` →
  `apply_entrenchment` → MP refill + `reset_interdiction_coverage` +
  morale drift + `apply_doctrine_turn_start`, in that order (events land
  before the drift so a delta steers the same turn's drift target).
  `begin_turn` only fires from `end_turn`, so turn-1 arrivals/events for
  the very first mover get an explicit pass at the end of `Game::build`.
  Each faction gets exactly one `begin_turn` per turn number, so the
  schedule summaries infer pending/fired status from `turn` alone — no
  "executed" flag. `apply_doctrine_turn_start` runs per faction here (not
  once per full game turn) purely by following this existing pattern — see
  "Command & doctrine" in docs/ideas.md for the open question of whether
  that's the right cadence long-term.
- **Combat orchestration** (`game/orders/attack.rs`): `prepare_battle`
  validates (adjacency, single factions per side, turn ownership), looks up
  each side's faction doctrine (`Game::doctrine_of`) and builds the
  `CombatElement` snapshots with it; `attack`/`air_support` persist results
  afterwards, `simulate` never does — all three share it. The aftermath
  order matters: experience gain (before losses reshape rosters), losses,
  `apply_doctrine_battle_result` (leader lookup by unit name, so it must
  still run before a beaten defender can be removed), retreat execution,
  advance, then morale shifts (once routs are known). `BattlePlan.attacker_names`
  is ground-only — an air-support unit joins the snapshot but not the name
  lists, which keeps it out of the advance, morale shift and doctrine
  attribution while its element losses/experience (snapshot-driven) still
  persist.
- **Combat engine** (`procedures/combat.rs`): per-instance snapshots (one
  `CombatElement` per ready squad/gun/vehicle; damaged sit out) carry
  everything resolution needs — cv, morale, experience, doctrine,
  vulnerability, armored/air-domain/targeting flags, devices, fort_level —
  so future stats extend the snapshot builder and modifier math, not the
  control flow. `doctrine` is a single value stamped uniformly across every
  element on a side (one faction per side, always), unlike the per-element
  morale/experience. Rounds fire simultaneously (hits collected, then
  applied); severity keeps the worse of double hits. The constants at the
  top of the file are the tuning knobs (RANGE_BANDS, DISRUPT/DAMAGE_CHANCE,
  RETREAT_ODDS, FORT_CV_BONUS_PER_LEVEL); `stat_modifier` is the single
  function to swap for a different stat curve. Doctrine also gates
  commitment in `fire_round`, as a second roll independent of experience's.
- **Interdiction**: coverage lives on `Game` (`interdiction_coverage:
  HashMap<unit name, Vec<hex>>`), not on `Unit` — same separation as
  `scheduled_arrivals`/`events`. `prepare_battle` extends the defender
  snapshot with `covering_fighter_units(defender_faction, to)`, *excluding
  any unit already present as a ground defender at that hex* — without the
  exclusion, a unit interdicting its own hex was double-counted and could
  underflow its `ready` bucket in `apply_battle_losses` (a debug-build
  panic; see the `_is_not_double_counted_as_a_defender` test).
- **Leaders**: the opposite split from interdiction — assignment lives on
  `Unit.leader: Option<String>` (the leader's name), not on `Leader`, so
  `State`'s leader roster (`leaders: HashMap<String, Leader>`) carries no
  back-reference; `Game::unit_led_by` does the reverse lookup by scanning
  units. `build_state` enforces the invariants a scenario can violate that
  `reassign_leader` can't (it always clears the old unit first): a unit's
  `leader` must name a real `[[leaders]]` entry of its own faction, and no
  two units may claim the same leader. Stats (`LeaderStats`, the seven
  WitE2 ratings) are read-only for now — no per-roll combat or
  command-radius effect yet; `game::doctrine` reads `initiative`/`political`
  for the drift formulas and averages the rest (`average_leader_value`).
- **Doctrine** (`game/doctrine.rs`): a faction-wide rating (`Player::doctrine`)
  plus a personal one per leader (`Leader::doctrine`), the latter resolved
  from an optional scenario field to the faction default at build time (see
  `LeaderConfig` above). Two independent, currently one-way flows: battles
  shift a leader's personal doctrine (see the module doc for the formula and
  the `LAV * 10` ceiling/floor a leader's own rating caps it at — the cap is
  provably unreachable by a single battle, since `FBO * LOS` never exceeds
  2.0 against a `10 * (LAV - DOC/10)` gap, but stays as the spec's explicit
  safety net and is unit-tested directly by calling past it), and turn start
  runs every faction leader's contribution to the faction value (a two-pass
  snapshot — computed from `Player::doctrine` before any leader's delta is
  applied — then drift back toward the updated value). Only one leader per
  side of a battle is credited: the one with the highest
  `average_leader_value` among that side's participating (ground, named)
  units — see "Command & doctrine" in docs/ideas.md for the deliberately
  deferred question of whether others should share in it.
- **Airfields**: `Toe.range: Option<u32>` (`None` = unlimited, every
  pre-existing TOE's behavior). `Game::check_mission_range` is a no-op
  for an offmap unit or a range-less TOE; otherwise compares
  `Location::distance_to`. Called from `prepare_battle`'s air_support
  branch and `Game::interdict`.
- **Detection**: `is_visible_to`/`is_unit_visible_to` are pure queries —
  no persisted state, no `procedures/` split (a handful of lines over
  `Location::distance_to`). Display-only by design: they gate
  `units_summary`'s `units_by_name`, `inspect_summary` (game/mod.rs), and the GUI's
  map markers/inspector roster — never
  `units_at_location`/`units_of_faction`, which order validation, the AI
  and the GUI's buttons rely on.
- **Supply**: `State::supply_sources` (`core::supply::SupplySource`,
  resolved by `build_state` like units/toe/elements/leaders) is each
  faction's supply-source hexes. `procedures::supply::reachable_hexes` is
  a pure multi-source flood fill (`Location::neighbour_coords` frontier,
  `TerrainCosts::cost` stops at impassable, a caller-supplied blocked set
  stops at enemy hexes). `game/supply.rs` assembles the inputs from
  `self.state.supply_sources`; nothing persists beyond that; `game/refit.rs`
  is the one consumer with gameplay effect.
- **Victory**: `end_turn` returns `Some(VictoryReport)` once `last_turn`
  completes; `run`/the GUI print it. Scoring only — nothing gates further
  commands afterwards.
- **AI** (`ai.rs`): per stack (BTreeMap of hex → unit names), attack the
  best adjacent enemy hex if `simulate` predicts ≥
  `ATTACK_RETREAT_THRESHOLD` (0.6) retreat rate over `SIMULATION_RUNS`
  (20), else `move_unit` toward the nearest unheld victory hex (fallback:
  nearest enemy) — full jump first, best single step if that errs.
- **GUI**: `PendingOrder` arms Move/Attack/AirSupport from the inspector;
  the next map click resolves it (`resolve_order` — unit index 0 always;
  stack-picking is a known deferred nicety). Interdict applies
  immediately to the inspected hex. Save/Load/New/Quit all defer through
  `pending_menu_action: Option<MenuAction>`, applied only after `ui()`
  drops its lock (see Gotchas). `adopt_game` gives a fresh New/Load the
  same AI-auto-play + turn-1-event-drain treatment `run` gives one.
  `MapView` folds zoom into its `HexLayout` scale and pan into
  screen-space offsets; every draw method scales sizes off `MapView.size`
  (`HEX_SIZE * zoom`) so everything grows/shrinks together;
  `assign_stack_slots` (pure, tested) offsets stacked units sideways.
  `gui/` renders answers, never derives rules: `render_inspector`'s
  Move/Attack gate is `Game::hex_controlled_by` (victory.rs, wraps the
  same `controlling_faction` `score_victory` uses), not an inline
  ownership check — one rule, one home, so the sim and the display can't
  drift apart.

## Testing

In-crate unit tests (`#[cfg(test)] mod tests` per module), since most
types are crate-private. Fixtures are inline TOML strings — shared
snippets in `game/test_support.rs` and `gui/test_support.rs` — and a few
tests load the real `scenarios/*.scen` / `maps/basic_map.map` via
`concat!(env!("CARGO_MANIFEST_DIR"), ...)` so shipped-config drift breaks
the build. Battle tests seed `StdRng` for exact reproducibility;
`three_vs_one(morale)` is the standard "defender surely loses" fixture
for retreat-path tests.

### Manual GUI verification

GUI changes need a live check — this sandbox has no input-injection tool,
so the technique is:

1. `mkfifo` a pipe in the scratchpad; hold its write end open from one
   backgrounded shell (`exec 3>pipe && sleep 600`) so the terminal
   thread's stdin doesn't EOF.
2. Launch `./target/debug/cse < pipe > stdout.log 2>&1` backgrounded,
   then drive it by echoing commands into the pipe — the terminal thread
   applies them to the shared game the window is rendering.
3. Screenshot with `spectacle` (`grim` does not work here — "compositor
   doesn't support the screen capture protocol"). The CSE window must be
   visible/on top.
4. `exit` through the pipe shuts everything down; kill the sleep holder.

Temporary debug hacks to force a state worth screenshotting are fine —
always reverted immediately after the screenshot confirms.

## Gotchas

- **eframe's default `wgpu` backend renders nothing in this project's
  sandboxed dev VM** — the window opens, the UI callback runs every frame
  (confirmed with debug logging), but no frame ever visibly presents.
  `glow` (OpenGL, via Mesa/llvmpipe here) works. Cargo.toml disables
  eframe's default features and enables `glow` explicitly — don't switch
  back to (or add) `wgpu` without re-confirming rendering via screenshot.
- **winit event loops can only be created once per process** — `gui::run`
  holds the codebase's only `eframe::run_native` call, made once from
  `main`'s main thread. Don't add a second one (e.g. a respawned/child
  window); route "another window" needs through the existing `GuiApp`.
- **Locking `SharedGame` in a method that also needs `&mut self`**:
  `self.shared.lock()` borrows the field, but a later `self.method(...)`
  needs all of `self` — they conflict while the guard lives. `GuiApp::ui`
  locks a clone of the `Arc` (`self.shared.clone()`), which doesn't
  borrow `self` at all. Keep new guard-across-`&mut self` code on this
  pattern.
- **`std::sync::Mutex` is not reentrant**: `MenuAction`s confirmed inside
  `render_playing`/`render_main_menu` (which run under `ui()`'s lock) are
  only *armed* there and applied after the guard drops
  (`apply_pending_menu_action`) — `Load`/`New` lock `shared` themselves
  in `adopt_game` and would deadlock otherwise.
- **Same shape of problem inside one `&mut Game` call**: a `&Location`/
  `Vec<&Unit>` read borrows `game` as long as the binding lives, so
  holding one across a later `game.interdict(...)` (`&mut Game`) won't
  compile. `render_inspector` re-fetches each read in its own small scope
  right before it's needed instead of binding once at the top.
- Scenario element names must match TOE element names exactly —
  `build_state` validates this, and `builds_the_real_basic_scenario` /
  `builds_the_real_frontline_sector_scenario` guard the shipped
  scenarios.
