# arch-9 findings: boundary sweep and docs audit

Status: report delivered (this file is the deliverable; no source or docs
were modified)

Method: every `.rs` file under `src/` was read (all of `game/`, `core/`,
`procedures/`, `gui/`, `terminal/`, plus `ai.rs`, `session.rs`, `main.rs`,
`lib.rs`, `error.rs`), backed by grep sweeps for each violation class;
`docs/architecture.md` and `docs/manual.md` were read in full against that
code, `docs/roadmap.md` skimmed for contradictions. Every proposal below is
for the author to accept, adjust, or reject.

## 1. Code sweep — the arch-1..8 violation classes

Each class was checked across the whole codebase, not just files touched by
earlier arch tasks.

- **`core::` boundary (arch-7's class): clean.** Every method defined on a
  `core::` type (`get_location`, `get_offmap_location`, `all_locations`,
  `map_from_string`, `neighbour_coords`, `distance_to`, `hex`, `cost`,
  `average_morale`, `average_experience`, `is_armored`, `is_air_domain`,
  `can_target_ground`, `Location::new`, `TerrainCosts::new`,
  `OffmapLocations::*`) was grepped for call sites; none is called outside
  `game/`/`procedures/` (non-test code). The `core::` imports that do exist
  outside (`ai.rs:16`, `gui/map_view.rs:8-9`) are data-only
  (`UnitLocation`/`Terrain` matching), which the documented rule allows.
- **Rule derivation outside `game/` (arch-6's class): clean.** The GUI's
  Move/Attack gate is `Game::hex_controlled_by` (`gui/inspector.rs:25`),
  map markers use the detection-gated `units_by_name`
  (`gui/map_view.rs:49`), and the terminal computes nothing — every rule
  answer is asked from `Game`.
- **Algorithmic logic outside `procedures/`/`game/` (arch-5's class):
  clean, one borderline noted.** `ai.rs::move_toward` (ai.rs:118-133)
  picks a fallback step by sorting neighbours on `game.distance` — a
  greedy one-step heuristic, not a pathfinder; all validation and costing
  is delegated to `move_unit`, and architecture.md documents exactly this
  ("full jump first, best single step if that errs"). Read as acceptable
  AI decision logic, flagged here only so the author can confirm.
- **`game.state.*` access outside `game/` (arch-4's class): clean.** Grep
  for `.state.` outside `src/game/` finds nothing (non-test). Query
  narrowness holds: no `Game` query returns `State`, a raw collection, or
  more than its caller needs.
- **Dependency direction: clean.** `procedures/` and `core/` import
  nothing from `game/`/`session`/`ai`/`gui`/`terminal` (grep-verified);
  `session.rs` sits between `game` and the frontends as documented;
  `main.rs`/`lib.rs` are wiring only.

`terminal/` and `session.rs` were re-checked in full as the task requested:
still clean, including the newer `reassign_leader` two-step path
(`terminal/mod.rs:84-130`), which uses only `Game::unit`/
`leaders_of_faction`/`reassign_leader`.

## 2. Behavior findings — code vs. manual (the substantive ones)

### F1. Entrenchment ticks offmap units too

`game/entrenchment.rs:19-26`: `apply_entrenchment` increments `fort_level`
for **every** unit of the faction on turn — there is no on-map check.
Contradicts `docs/manual.md`:

- line 79-80: step 4 is "every **on-map** unit that hasn't relocated digs
  in one level";
- line 18-19: "nothing happens to a unit in one [offmap box]".

Consequence: a unit parked in a reserve box climbs to fort level 5 while
"waiting". Mostly masked because arrival resets the level to 0
(`game/reinforcements.rs:41`) — but not entirely, see F2.

Proposed fix: skip `UnitLocation::Offmap` units in `apply_entrenchment`
(mirroring `apply_refit`'s existing on-map filter at `game/refit.rs:34-36`).

### F2. Interdicting fighters' fort levels leak into the ground defense CV

`game/orders/attack.rs:218-224` extends the defender snapshot with covering
fighter units; `procedures/combat.rs:108-110` copies each unit's
`fort_level` into its snapshot instances; `combat.rs:414-420` averages fort
level over **every** defending instance. So a covering fighter's fort level
(which, per F1, can be a phantom 5 for an offmap wing, or a legitimate 5
for a stationary on-map one) shifts the ground defenders' entrenchment
average up or down.

Contradicts `docs/manual.md:339-341`: the fort bonus is described as "each
average fort level **across the defending stack**" — the interdictor is
explicitly not part of the stack (manual line 246-248, and
`defender_names` excludes it by design).

Proposed fix (author's pick): zero `fort_level` on covering-unit snapshots
in `prepare_battle`, or exclude non-stack units from `average_fort_level`
via a snapshot flag. The first is the smaller change.

### F3. A reinforcement arriving mid-game lands with fort level 1, not 0

`game/turn.rs:50-54`: `begin_turn` runs `apply_scheduled_arrivals` (which
sets the arriving unit's `fort_level = 0`, reinforcements.rs:41) **before**
`apply_entrenchment`, which then immediately ticks the same unit to 1 in
the same turn start. Turn-1 arrivals applied from `Game::build`
(game/mod.rs:121-124) get no entrenchment pass, so they land at 0 —
inconsistent with later arrivals.

Contradicts `docs/manual.md:334-337`: "Relocating for any reason — …
arriving as a reinforcement — resets it to zero", and "one fort level per
**stationary** turn" (the unit was not stationary; it just arrived).

Proposed fix: have `apply_entrenchment` skip units relocated by this turn's
`apply_scheduled_arrivals` (e.g. arrivals return the affected names, or
entrenchment runs before arrivals — the latter also changes withdrawal
behavior, so the skip-list is the more surgical option). Note F1's fix
alone does not fix F3 (the arrival is on-map by then).

### F4. "Harmless bystander" fighter can in fact be shot at

`docs/manual.md:246-248` says a fighter pulled into a battle with no enemy
aircraft "can neither shoot nor be shot at". Not quite: an attacking ground
element flagged `anti_air` may target it (`procedures/combat.rs:281`,
`can_target_air = air_domain || anti_air` at combat.rs:108) — consistent
with the manual's own anti-air rule three paragraphs earlier
(manual.md:228-229). The "cannot shoot" half is correct (fighters have
`can_target_ground = false`).

Proposed fix: soften the manual sentence to "…cannot shoot, and only
anti-air-capable ground elements can shoot at it", or accept the
simplification as intended and adjust the wording either way. Docs-only.

### F5. A stuck lead unit blocks the rest of its stack's AI movement

`ai.rs:41-45` iterates a stack's unit names, but `move_toward`
(ai.rs:118-133) always issues `move_unit(..., 0)` — index 0. When index 0
moves away that's self-correcting (the sorted index shifts), but if the
first unit is stuck (no MP), every later iteration re-addresses the same
stuck unit, so followers that could move don't. Weakly contradicts
`docs/manual.md:415-417` ("move **the stack** toward the nearest
objective…").

Proposed fix: resolve each name to its current index via
`units_at_location` before ordering, or accept as a known prototype-AI
simplification and note it in the manual's AI caveats. Low priority.

## 3. docs/architecture.md audit

### F6. `State` field list omits `terrain_costs`

architecture.md:155-158 lists `State` as "(map, units, toe, elements,
leaders, supply_sources, starting_strength)"; `core/mod.rs:17` also has
`terrain_costs: TerrainCosts` (used by movement, pathfinding cost closure,
and supply). Proposed fix: add it to the list.

### F7. `core/unit.rs` entry claims "+ config structs"

architecture.md:165-166: "core/unit.rs — Unit …, Toe …,
Element/Device/ElementClass, Size **+ config structs**". No config struct
lives in `core/unit.rs`; `UnitLocationConfig`, `ScheduledArrivalConfig`,
`UnitConfig`, `ElementStatsConfig` are all in `game/scenario.rs`
(scenario.rs:363-414), matching the "all TOML schema in scenario.rs"
convention. Proposed fix: drop "+ config structs" from the entry.

### F8. Dead-config-field list still names `VictoryHex.name` — which is read

architecture.md:223-227 lists `VictoryHex.name` among "fields deserialized
but not yet read". It is read: `game/victory.rs:90`
(`victory_conditions_summary`) and `victory.rs:113` (`victory_hexes`).
Correspondingly, the `#[allow(dead_code)]` on `game/scenario.rs:453` is now
unnecessary (the convention itself says to remove the attribute when a
system starts using the field). Proposed fix: drop `VictoryHex.name` from
the doc's list and remove the attribute. (`Scenario.game_version` and
`MapFile.width/height` were verified still dead — those entries are
accurate.)

Everything else in architecture.md checked out against code: the session/
terminal/GUI wiring description, the file map (minus F6/F7), all
conventions (randomness, errors, hex coordinates, name registries,
summaries, sealed state, core boundary, file formats), and every
per-system implementation note, including the begin_turn ordering, the
combat aftermath ordering, the interdiction double-count exclusion, and the
detection gating list.

## 4. docs/manual.md audit

Beyond F1-F5 above, the manual verified accurate against code on every
numeric and structural claim checked: turn-start sequence, MP/terrain
costs and cheapest-path movement, range bands and opening bands, cover and
terrain-defense tables, severity split (50/35/15), commit-by-experience,
domain-restricted targeting, CV modifier curve, 2:1 retreat odds, fort
+15%/level, retreat/rout/shatter/surrender mechanics and their constants,
experience/morale shift math (steps 10/20), morale drift, refit steps
(1/4, 1/8), supply flood-fill semantics (blocked sources supply no one),
fog-of-war gating (display-only, exactly the listed surfaces), victory
scoring, schedules, AI rules (60% threshold over 20 runs), and save
round-tripping.

### F9. Intro still frames the whole manual as Stage-1 prototype

manual.md:8-11 says everything described is prototype-grade "with the real
versions coming in Stage 2" — but Stage 2 is current and the manual already
documents a Stage-2 system (Leaders, added by the leaders task). Proposed
fix: reword the intro to date the prototype framing per system rather than
blanket-labeling the file, or simply note Stage 2 is underway. Docs-only,
cosmetic.

## 5. Stale references in code doc comments

Same class as the known `MapSnapshot` case: comments naming things that do
not exist or no longer live where claimed. All proposals are one-line
comment edits.

- **F10 (the known one).** `game/victory.rs:120-121`: `VictoryHexInfo` is
  "decoupled … the same way `MapSnapshot` is decoupled from `State`" — no
  `MapSnapshot` exists anywhere (grep-confirmed). Propose deleting the
  parallel-example clause.
- **F11.** `core/unit.rs:137, 164, 194` reference
  `ElementClass::can_target_air` — no such method exists; the real thing
  is the derived `CombatElement::can_target_air` field
  (`procedures/combat.rs:56,108`). Propose pointing at
  `procedures::combat` (or rephrasing without a code reference).
- **F12.** `ai.rs:2`: "see `Command::EndTurn` in lib.rs" — `Command` lives
  in `terminal/command.rs`, and the AI is invoked from
  `session::play_pending_ai_turns`. `ai.rs:139-140`: `AiTurnReport` is
  "printed by `lib.rs`" — printed by the terminal's `run` and logged by
  the GUI, via session.rs. lib.rs has no function bodies. Propose
  correcting both to `session.rs`.
- **F13.** `game/leaders.rs:67`: `reassign_leader` "backs the terminal's
  … prompt (see `main.rs`)" — the prompt lives in `terminal/mod.rs`
  (`run_reassign_leader`). Propose correcting the pointer.
- **F14.** `game/mod.rs:49-51` and `game/events.rs:29`: event messages
  are drained "via `take_event_messages`" by "`run`" — the drain sites
  are `session::report_turn_transition`/`activate_game` (both frontends),
  not the terminal's `run` specifically. Propose naming session.rs.
- **F15.** `gui/mod.rs:17`: "`SharedGame` (`Arc<Mutex<Option<Game>>>`,
  see lib.rs)" — `SharedGame` is defined in `session.rs` (lib.rs only
  re-exports it). Propose correcting the pointer.
- **F16.** `game/detection.rs:6-7`: garbled doubled reference — 'See "Fog
  of war / detection" in docs/manual.md ("Fog of war and detection")'.
  The manual heading is "Fog of war and detection"; propose collapsing to
  one correct reference.

## 6. docs/roadmap.md (contradiction check only)

### F17. "Every command has a clickable equivalent" is overstated

roadmap.md:62-64 (Stage 1 inventory): the GUI has no equivalent for
`simulate`, `units`, `leaders`, `leader`, or `reassign_leader`
(grep-confirmed: `gui/` references leaders only as an inspector label, and
`simulate` not at all). `simulate` and `units` predate Stage 2, so the
claim was already inexact for the prototype; the leader commands widened
the gap. Proposed fix: soften to "every *order* has a clickable
equivalent" or list the exceptions — author's wording call. (The rest of
the roadmap checked out; its known-gaps list at roadmap.md:75-78 matches
the code.)

## Suggested triage

F1-F3 are the only findings with gameplay effect and would make one small
combined task (entrenchment scope + interdictor fort leak + arrival tick
order). F6-F8 are a single architecture.md/scenario.rs cleanup pass.
F10-F16 are a single comment-hygiene pass. F4, F5, F9, F17 are independent
one-liners the author may also judge not worth fixing.
