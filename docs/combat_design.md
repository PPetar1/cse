# Combat resolution design

Living document. Records what we learned from WitE2's ground combat system, what
CSE's resolution model does now, where it deliberately deviates, and what's still
open. Update this whenever the combat engine changes.

Source studied: the WitE2 rules, chapter 23 (Ground Combat) plus chapters 12
(Morale), 15 (Leaders), 21 (Ground Units) — readable at
<https://dornshuld.com/rules/wite2/1-0.html>. Note the manual describes the
*structure* of resolution but never publishes the inner to-hit formulas, so all
concrete math below is ours.

## How WitE2 does it (summary of findings)

Battle sequence: initiate → terrain/fort modifiers → commit support/reserve
units → air missions → **multi-round fire phase** → final CV computation →
outcome by odds ratio → retreat consequences.

- **Fire phase**: a variable number of rounds at *closing range*. Opening range
  depends on defending terrain (urban starts closer). Long-range elements
  (artillery, tanks) fire in early rounds; infantry joins as the range closes.
  Attacker fires first at ≥3,000 yards, defender first below that.
- **Element fire**: eligibility checks (morale, supply, fatigue, ammo, leader
  rolls), then shots based on rate of fire and ammo state, then to-hit from
  device accuracy vs target speed/size, then damage vs armor/toughness.
- **Hit outcomes**: nothing / **disrupted** (stops firing, excluded from final
  CV, recovers after battle with extra fatigue) / **damaged** (out of action,
  may recover or be captured after) / **destroyed** (gone; ~1/25 men captured).
- **Outcome**: final CV = sum of still-effective elements' CVs, heavily
  modified (terrain density doubles/quadruples infantry CV and halves/quarters
  AFV CV; fort levels; leader rolls that can double or halve unit CV; CPP,
  weather, HQ command chain). **Modified odds ≥ 2:1 → defender retreats**,
  otherwise holds. Rout if odds exceed unit morale (morale 40 unit routs at
  >40:1); shatter/surrender for weak or isolated units. Retreat attrition
  (worse over rivers, damaged elements likely captured).

## CSE v1 model (implemented)

Scope: an `attack <x1> <y1> <x2> <y2>` command. All units in the source hex
attack all units in the target hex. Pure resolution engine in
`src/procedures/combat.rs`; the `Game` layer builds battle snapshots, calls the
engine, applies losses back, executes retreats, and prints the report. An
attack order must come from the faction on turn and target an adjacent hex;
`simulate` obeys the same rules (shared validation in `prepare_battle`), so a
simulation is always of an attack the player could actually order — and any
future order logic keyed on the source hex or the turn (reserve activation…)
automatically covers both.

### Snapshot seam (the load-bearing design decision)

The engine never touches `Game`/`State`. Each individual element instance
(`ready` count expanded, one struct per squad/gun/tank) becomes a
`CombatElement` carrying everything resolution needs. Results flow back by
reading each element's final state. Future stats (experience, morale, fatigue,
ammo, leaders) extend the snapshot builder and the modifier math — not the
engine's control flow. Damaged elements do not participate.

### Round structure

Range bands: **3000 → 1500 → 800 → 400 → 100 meters**, one round per band.
Defender terrain sets the opening band: Plains/Desert/Water start at 3000,
Hills/Forest/Swamp/Mountain at 800, Urban at 400. An element fires in a round
iff at least one of its devices' `range` covers the current band. Fire within
a round is simultaneous: all shots resolved against the round's starting
states, then effects applied — no first-strike advantage from code ordering.

### Morale & experience (per element)

Every element carries `morale` and `experience` (0–100). The scenario sets
them at whatever granularity is convenient — the most specific setting wins:

1. `[[units.elements]]` override on a single element type of a unit,
2. `morale`/`experience` on the `[[units]]` entry (all its elements inherit),
3. faction-wide defaults on `[[players]]` (kept on the runtime player so
   future events can shift them over time),
4. absent everywhere: 50.

Effects: experience gates commitment (below), both scale the element's CV
contribution (see Outcome), and the unit's strength-weighted average morale
gates routs (see Retreat execution).

### Devices: one shot, three rolls

An element fights with its **devices** — a rifle/LMG volley, a tank's main
gun, its coaxial MG — each carrying `accuracy` (chance a shot hits), `range`,
`rate_of_fire` (shots per round) and `soft_attack`/`hard_attack` (how
devastating a hit is). This is what lets a Pz IV spray MG fire at infantry
while taking two aimed cannon shots at a gun line.

Each Ready element with a device in range first **commits**: it fires only
when a d100 roll is under its `experience` — WitE's "green units fail to
commit" (one roll for the whole element). Then every in-range device fires
`rate_of_fire` shots; per shot:

1. **Target**: uniform random over enemy Ready elements. Because the snapshot
   is per-instance, numerous element types soak proportionally more fire — an
   emergent screening effect, no extra rules.
2. **To hit**: `accuracy × cover(defender terrain)` as a percentage, cover
   applying only to shots *at the defender* (attacker is assumed advancing in
   the open). Cover: Plains/Desert/Water 1.0, Hills 0.8, Swamp 0.7, Forest 0.6,
   Mountain 0.5, Urban 0.4.
3. **Effect**: the device engages with the fire value matching the target's
   hardness — `hard_attack` (AP) against armored classes (LightTank/MedTank),
   `soft_attack` (small arms/HE) against everything else — scaled by the
   target's element-level `vulnerability` (armor for vehicles, exposure for
   the rest): effect chance = `attack × vulnerability / 100`, one d100 roll.
   Because a target only ever receives the fire kind matching its hardness,
   one vulnerability stat per element suffices (no v_inf/v_arm pair), and
   small arms need no category of their own — they are simply the
   soft-attack value of an infantry device.
4. **Severity** on a failed save: 50% disrupted, 35% damaged, 15% destroyed.

### Loss flow

- **Disrupted**: out of the rest of this battle and the final CV; fully
  recovers afterwards (nothing persisted).
- **Damaged**: persisted `ready → damaged` on the owning unit. Repairs at
  turn-start via `game::refit`, gated on supply connectivity (see below).
- **Destroyed**: `ready` decremented, element gone; replaced over time via
  `game::refit`, also gated on supply.
- **Experience gain**: every element bucket that fielded instances in the
  battle gains `ceil((100 − experience) / 10)` experience, winners and losers
  alike — green troops learn fast (+7 at 35), veterans barely (+1 at 90),
  100 caps itself. Side effect worth knowing: repeatedly battering a
  defender also trains them.
- **Morale shift**: settles last, once routs are known, and is collective —
  every bucket of a participating *unit* shifts, whether or not it could
  fight (unlike experience, which only those who fought earn). The winning
  side (a hold counts as a defender win) gains `ceil((100 − morale) / 20)`,
  the losing side loses `ceil(morale / 20)`, and a routed unit takes the
  loss a second time — tapering toward the 0/100 bounds like experience.
  This is a deliberate feedback loop: repulsed attacks rally the defender
  (higher CV, steadier under rout checks) and discourage the attacker, so
  grinding a position has a real psychological price.
- **Morale recovery** (turn system, not the battle engine): at its faction's
  turn start every element bucket drifts toward the faction default morale by
  `ceil(|gap| / 10)`, from both sides — rest heals battered units, battle
  euphoria fades. Gentler than the battle shifts, so combat outcomes dominate;
  battle-earned experience, by contrast, is permanent. Mirrors WitE's
  national-morale anchor, and events can move the anchor itself (the faction
  default lives on the runtime player).

### Outcome

Final CV per side = Σ `cv × (1 + morale/100 + experience/100)` of elements
still Ready: ×1 at 0/0, ×2 at the 50/50 baseline, ×3 at 100/100. The additive
form (WitE-style) was chosen over the multiplicative `(mor/100 × exp/100)`
because outcomes ride on the odds *ratio*: additive, elite (80/70) vs green
(45/35) tilts the odds ×1.39 on stats alone; multiplicative it would be ×3.5,
letting stats dwarf equipment. One function (`morexp_modifier`) to swap if
tuning wants a different curve.

Defender CV is further multiplied by a flat terrain defense factor
(Plains/Desert/Water 1.0, Hills 1.5, Forest/Swamp 2.0, Mountain/Urban 3.0).
**Odds ≥ 2:1 → defender retreats**, else defender holds. No shatter yet.

### Retreat execution

Implemented in the game layer (`Game::execute_retreat`), not the engine — it
needs the map and unit occupancy. On a retreat outcome:

- **Destination**: an adjacent on-map, non-Water hex with no enemy units,
  preferring the hex farthest from the attacker (ties break on lowest (x, y)
  so retreats are deterministic). The whole defending stack goes to the same
  hex.
- **Retreat attrition**: each ready element of a retreating unit has a 10%
  chance to end up damaged; each damaged element (hard to drag along, per
  WitE's capture rule) has a 25% chance to be lost for good.
- **Rout**: a retreating unit routs when a d100 roll beats its
  strength-weighted average element morale — the attrition rolls then run
  twice, and the unit takes the post-battle morale loss twice.
- **Shatter**: a routing unit whose ready strength has fallen below half of
  its TOE (SHATTER_STRENGTH_FRACTION) disintegrates outright when a second
  roll also beats its morale — the unit is removed from the game. Fresh
  units never shatter from a single lost battle; worn-down ones can.
- **Surrender**: if no valid destination exists, the defenders are cut off and
  surrender — the units are removed from the game.

- **Advance after combat**: a beaten defender always clears its hex, and the
  whole attacking stack advances into it automatically, at no MP cost — the
  battle already paid for the ground (WitE-style). Reported in the attack
  output.

### Randomness & testing

All rolls go through a `rand::Rng` passed in by the caller — the command loop
passes a fresh thread RNG, tests pass `StdRng::seed_from_u64(...)` for exact
reproducibility.

The `simulate <x1> <y1> <x2> <y2> <n>` command fights the same attack n times
against snapshot copies (game state untouched) and prints hold/retreat rates,
average losses per side, and mean final CVs. This is the tuning loop: change a
knob, re-run, compare distributions. 1000 runs resolve in well under a second
even in debug builds. It also underpins the low-randomness battle setting idea
(see docs/ideas.md).

### Air support (Phase 5, slice 1)

`Game::air_support(air_unit, from, to, rng)` flies one owned unit's elements
into an ongoing ground attack as extra firers, for that battle only — the
first piece of "combined arms" per the roadmap, and a direct test of
`docs/ideas.md`'s claim that air warfare should slot into the existing
engine as data (a new `ElementClass::GroundAttack`) rather than a parallel
system.

- The air unit's `CombatElement`s join the attacker snapshot in
  `prepare_battle` (an added `air_support: Option<&str>` parameter), so the
  whole existing round/CV/severity machinery just runs — nothing in
  `procedures/combat.rs` changed. `attack`'s validation is unchanged and
  fully reused: the air unit's faction must match the attacking faction and
  a ground attacker must already be present at `from` (this mechanic only
  augments an existing ground battle — a stand-alone air strike is a later
  slice, along with air superiority, interdiction and airfields).
- Slice 1 let any Ready element on the attacker side be targeted uniformly,
  so ordinary ground fire could shoot down supporting aircraft. Slice 2
  (below) replaced that with domain-restricted targeting — ordinary ground
  fire no longer can, only AA-flagged ground elements or enemy fighters can.
- Two deliberate simplifications for this slice: the air unit never
  advances into a vacated hex (it stays at its home location, whether or not
  that's an on-map airfield — see slice 4), and its own morale does not
  shift from the battle's outcome (only the ground attacker/defender names
  feed the post-battle morale shift).
  Experience gain and element losses *do* persist for it, since those flow
  from the generic per-`CombatElement` snapshot, not from the
  ground-only name lists. Revisit both if it turns out to matter once the
  mechanic gets played with.
- `simulate` is untouched — no air-support preview yet; a natural follow-up
  once the mechanic is proven.

### Air superiority (Phase 5, slice 2)

Rather than a separate air-to-air "interception" phase, air and ground
combat stay one unified battle with **domain-restricted targeting** — the
shape you get when you take `docs/ideas.md`'s "more element classes,
devices and mission procedures, not parallel engines" claim at face value.

- Every element is either ground-domain or air-domain
  (`ElementClass::is_air_domain`: true for `GroundAttack` and the new
  `Fighter`). A firer's eligible targets depend on its own class plus an
  `Element.anti_air` flag:
  - **Fighter**: air-domain targets only — it can never touch a ground
    target, no matter what's in range.
  - **GroundAttack** (bomber): both domains — ground normally
    (`soft_attack`/`hard_attack` as before), air weakly via a new
    `air_attack` device stat (a rear gunner potshot at an intercepting
    fighter, not its real job).
  - **Ground elements**: ground only, unless `anti_air = true` (dual-purpose
    flak), in which case they can also engage air-domain targets via
    `air_attack` — while keeping their normal ground-fire capability.
- `CombatElement` carries three fields computed once per snapshot
  (`combat_elements`, not per shot): `air_domain` (is this element itself an
  air target), `can_target_ground`, `can_target_air`. `fire_round` filters
  each firer's target pool down to Ready-and-domain-compatible before
  picking uniformly, and picks `air_attack` instead of `hard_attack`/
  `soft_attack` when the chosen target is air-domain. When nothing air-domain
  or `anti_air` is involved, a firer's eligible pool is exactly the old
  shared Ready-target pool — a strict generalization, confirmed by the full
  pre-existing test suite passing unchanged.
- AA-flagged ground elements need no extra wiring to contest an incoming
  CAS mission — they're already part of the ordinary defender snapshot, and
  the targeting rules above just let them shoot back. Fighters needed one
  more piece, superseded by slice 3 below.
- Not modeled yet: escort (an attacker-side fighter protecting its own CAS).

### Interdiction (Phase 5, slice 3)

Slice 2 gave every fighter unit a faction owns a blanket, unconditional
pass into any `air_support`-augmented battle, anywhere. Slice 3 replaces
that with a declared, scarce resource: `Game::interdict(unit, target)`
marks a hex as covered by a fighter-capable unit (up to
`INTERDICTION_HEX_LIMIT` = 3 hexes per unit at a time); only a battle that
actually happens at a covered hex pulls that unit's elements into the
defender snapshot — and it does so for *any* battle there, not just ones
using `air_support`.

- Coverage is tracked on `Game` (`interdiction_coverage: HashMap<unit name,
  Vec<hex>>`, `game/interdiction.rs`), not on `Unit` — matching how other
  scheduling mechanics (`scheduled_arrivals`, `events`, `supply_sources`)
  live as their own `Game` fields rather than being bolted onto the domain
  types they act on.
- `prepare_battle` (`game/orders/attack.rs`) unconditionally extends
  `defenders` with `Game::covering_fighter_units(defender_faction, to)` —
  whatever units of the defending faction currently cover the target hex.
  This replaced slice 2's `air_support.is_some()`-gated, unconditional
  `faction_fighter_units` call (now deleted). A plain ground `attack`
  against a covered hex now pulls fighters in too; against an uncovered
  hex, or with nothing covering it, nothing changes from before. Since a
  fighter with no air-domain target present simply never fires and can
  never be fired on (domain-restricted targeting again), a fighter pulled
  into a battle with no air element on the attacker's side is a harmless
  bystander — it still gains experience just for being fielded (same as
  any element bucket in any battle), but never fights.
- Coverage clears at the *covering faction's own* next turn start
  (`reset_interdiction_coverage`, called from `Game::begin_turn` alongside
  the MP refill), so a declaration made on your turn survives exactly
  through the opponent's next turn — the only window in which it can
  matter under IGO-UGO — and must be redeclared every time you act again.
- **Bug fixed:** nothing stopped a unit from interdicting the very hex it's
  already physically stacked on (e.g. an air unit based right on the front
  line, defending alongside the ground troops there — a natural first
  thing to try from the GUI's inspector, since its Interdict button works
  on any hex, including the one you're looking at). `prepare_battle` used
  to add `covering_fighter_units` to the defender snapshot unconditionally,
  so that unit's elements were counted twice: once as an ordinary ground
  defender (`units_at_location`), once again as a covering fighter. Against
  an attacker that could actually hit it (an anti-air-flagged element, or
  an air-domain attacker), `apply_battle_losses` could then try to charge
  more losses against its `ready` bucket than it actually had — a `u32`
  underflow, which panics in a debug build (`cargo run`'s default) and
  takes down the whole process, GUI included. Fixed by excluding, from the
  covering-units list, any unit whose name is already in the hex's
  `defender_units` — see the `_is_not_double_counted_as_a_defender` test.

### Airfields (Phase 5, slice 4)

Every slice above explicitly deferred the same gap: an air unit had no
on-map base or range limit. This slice closes it without any new type —
an air unit's "airfield" is just wherever it currently sits.

- `Toe` gains `range: Option<u32>` (`#[serde(default)]`, so `None` —
  unlimited — is every TOE's behavior before this field existed, and every
  ground TOE's forever, since only air-mission code ever reads it).
- `Game::check_mission_range(unit, target)` (`game/mod.rs`) is `Ok(())`
  immediately if the unit is still `Offmap` (no coordinate to measure a
  distance from — an offmap air unit keeps slices 1–3's original unlimited
  behavior) or its TOE sets no `range`; otherwise it compares
  `Location::distance_to` from the unit's current on-map hex against the
  TOE's range, erroring with both numbers if it's out of reach. One
  function, called from both mission orders: `prepare_battle`'s
  `air_support` branch (`game/orders/attack.rs`) and `Game::interdict`
  (`game/interdiction.rs`).
- Basing is just `UnitLocation::OnMap` — the same enum every ground unit
  already uses — so an air unit could in principle redeploy to a new
  airfield with an ordinary `move_unit`, if its TOE ever gave it nonzero
  `mp`. Not exercised yet; both shipped scenarios keep `mp = 0` (stationary
  bases), only adding `range` and an on-map location.

### Fog of war / detection (Phase 7, slice 1)

Phase 7's first slice: whether a faction can see an enemy unit at all now
depends on how close its own units are. Deliberately narrow scope — this
is a first cut, not the whole of "operational depth."

- Opt-in per scenario via an optional `[fog_of_war]` table (`detection_range:
  u32`), mirroring `[victory_conditions]`/`[[supply_sources]]`'s "absent =
  feature off" pattern — every scenario shipped before this existed keeps
  behaving exactly as before (full visibility), so nothing needed
  retuning. `basic_scenario.scen` (the dev/test sandbox) turns it on at
  `detection_range = 3`; `frontline_sector.scen` (the tuned AI-vs-human
  landmark) is untouched, since its own AI wouldn't react to being seen or
  unseen anyway (see below).
- `Game::is_visible_to(viewer, x, y)`/`Game::is_unit_visible_to(unit,
  viewer)` (`game/detection.rs`) are the whole model: a hex is visible to
  `viewer` iff some on-map unit of `viewer`'s is within `detection_range`
  hexes of it (`Location::distance_to`, the same query `check_mission_range`
  already uses) — pure hex distance, no line-of-sight or terrain blocking.
  A unit is visible if it's the viewer's own, or (without fog of war) always,
  or its on-map hex is visible; an enemy unit still sitting offmap (a
  reserve box) is never visible under fog of war — this first slice can't
  reveal what's waiting in reserve at all.
- **Display-only, not a mechanics change.** `units_at_location`/
  `units_of_faction` — read by move/attack validation, `ai.rs`, and the
  GUI's Move/Attack button logic — are untouched; you can still legally
  attack a hex without full information about who's defending it, same as
  before. Only the *telling* is gated: the terminal's `inspect`/`units`
  (via `Game::list_units`/`list_units_detail`'s shared `units_by_name`) and
  the GUI's map markers (`GuiApp::render_map`) and inspector roster
  (`GuiApp::render_inspector`) now show the on-turn faction's own units
  always, plus only enemy units/hexes `is_visible_to`/`is_unit_visible_to`
  currently allows.
- **Deliberately not gated in this slice**: victory-hex holders
  (`victory_conditions_summary`, the map's flag markers) — treated as
  scoreboard/meta information, the same category as the turn counter, not
  battlefield awareness. Reinforcement/event schedules and supply status
  are untouched too. `ai.rs` keeps its existing full-information queries —
  the AI stays omniscient, a new instance of the same already-documented
  category of simplification as "the AI doesn't know air_support/interdict
  exist" (see Airfields above).
- Open questions, in the same rough-order spirit as the list below: LOS/
  terrain blocking (right now a unit sees straight through a mountain),
  contact persistence (a WitE-style "last known position, fading over
  turns" instead of a hard binary visible/not), recon-specific detection
  ranges (one scenario-wide number today, not per-TOE), and eventually
  teaching the AI to respect it at all.

### Entrenchment (Phase 7, slice 2)

A unit that stays put gains a defensive fort level over time; moving,
retreating, or advancing throws it away. No scenario config here — unlike
fog of war's `detection_range`, the numbers are code constants (like
`RETREAT_ODDS`/`MORALE_SHIFT_STEP`), since this is a universal combat rule
rather than a per-scenario toggle.

- `Unit.fort_level: u32` (`core/unit.rs`), 0 by default, capped at
  `game::entrenchment::MAX_FORT_LEVEL` (5). `Game::apply_entrenchment`
  (`game/entrenchment.rs`) adds one level, at turn start, to every on-map
  unit of the faction coming on turn — called from `begin_turn` alongside
  refit, so a unit's dig-in tick lands the turn *after* it last relocated,
  the same lag turn-start morale drift and refit already have.
- Reset to 0 wherever a unit's `location` actually changes to a new hex:
  `move_unit` (`game/orders/movement.rs`), the post-battle advance into a
  vacated hex and a beaten defender's retreat (both in
  `game/orders/attack.rs`), and a scheduled reinforcement/withdrawal
  (`game/reinforcements.rs`). Attacking without moving (holding, or
  advancing having *lost*) doesn't reset it — only an actual relocation
  does.
- **Purely defensive, like `terrain_defense`.** `CombatElement` carries the
  owning unit's `fort_level` (set once in `combat_elements`, read-only
  through the battle); `resolve_battle` multiplies `defender_cv` — never
  `attacker_cv` — by `fort_defense_modifier(average_fort_level(defenders))`
  (`procedures/combat.rs`): `1.0 + FORT_CV_BONUS_PER_LEVEL (0.15) *
  average_fort_level`, so level 5 is a flat +75% defensive CV, stacking
  multiplicatively with terrain the same way morale/experience stack
  additively into `morexp_modifier`. The average is a simple mean over
  every defending instance (not filtered to currently-Ready, mirroring how
  `terrain_defense` itself isn't either) — a flat environmental property of
  the defending stack, not a per-round combat outcome.
- **Display**: the terminal's `inspect`/unit `Display` impl and `units
  detail`'s `Debug` output show `Entrenchment: level N`; the GUI's
  inspector roster shows the same line, and the map draws one small pip
  per fort level under each unit's marker (`MapView::draw_unit`,
  `UnitMarker.fort_level`) — a glance at who's dug in without opening the
  inspector.
- Open questions: no way for a scenario to start a unit pre-entrenched
  (a historically dug-in defensive line at scenario start); no per-TOE
  variation (engineers digging in faster, armor slower); attacking from
  an entrenched position doesn't currently do anything special for the
  *attacker's* own CV (deliberately — see "purely defensive" above) but a
  future "prepared position" mechanic might want to let a defender fire
  first from cover, which this doesn't attempt.

## Deliberate deviations from WitE2 (for now)

- Flat defender terrain CV multiplier instead of WitE2's per-class density
  rules (infantry ×2 / AFV ×0.5 in dense terrain). Worth adopting once battles
  are tunable via `simulate`.
- Fixed one round per range band; WitE2 has a variable number and can break
  off battles early on bad odds.
- No morale/experience/fatigue/ammo/leader checks, no fort levels, no support
  or reserve commitment, no retreat execution or attrition.

## Observed balance (pre-tuning)

Repeated frontal attacks by the panzer division into the forest-defending
Soviet infantry division grind the *attacker* down (CV 984 → 148 over twelve
attacks) while the defender barely dents. Causes: tanks fire only AP (squads
have v_arm 3, near-immune), forest cover 0.6 protects the defender while the
attacker is hit at full accuracy, and the defender's howitzers fire in every
round. Plausible flavor (frontal attacks into forests *should* be bad), but
confirms dual AP/HE fire values are the next combat priority.

`simulate` quantifies it (1000 runs each, basic scenario): panzers attacking
the infantry division in forest — 0% retreats, attacker loses ~1.7 elements
per defender element. Soviets counter-attacking the panzers on plains — also
0% retreats, roughly even losses. Baseline to beat when tuning.

With experience in play (panzers 70, Soviet infantry 35), the same runs move
the right way: the forest attack now trades ~evenly (green defenders fail to
commit most shots), the Soviet counter-attack loses ~1.7:1. Still 0% retreats
everywhere — dislodging a dug-in division needs the AP/HE fix and probably
odds/CV work.

With the CV modifiers added on top (panzers ×2.5 vs Soviet infantry ×1.8),
the stat gap now shows in the odds too: the forest attack averages 1.1:1
(was ~0.5:1 on raw CV — forest ×2 still holds the line), the Soviet
counter-attack 0.4:1 with ~2.2:1 losses against it. Retreats remain at 0%;
the AP/HE fire-value split is still the lever that has to move first.

With soft/hard fire values (tanks can finally hurt infantry), the picture
clicks into place: the panzer attack into the forest inflicts ~2.5:1 losses
but the defender still holds 100% at 1.2:1 odds — while the same attack
against infantry caught on open plains forces a retreat 100% of the time at
2.8:1. The Soviet counter-attack on the panzers loses ~3.9:1. Dug-in forest
infantry holding a frontal armor attack while bleeding, and getting thrown
back in the open, is the intended flavor — a solid baseline for future
tuning.

Devices + rate of fire keep that shape but make battles bloodier (more shots
per round, MGs joining at close range): forest attack holds 100% at 1.4:1
with ~3:1 defender losses (~62 of 260 elements per attack), plains defense
collapses 100% at 3.5:1, the Soviet counter-attack loses ~5:1. If attrition
feels too high once turns exist, `rate_of_fire`/`accuracy` are the knobs.

## Open questions / next steps (rough order)

1. Retune the numeric knobs (severity split, cover table, CV multipliers,
   retreat attrition) with `simulate` evidence.
2. Longer term: swap-in experiment — 2D tactical battlefield with generated
   terrain and LOS (see docs/ideas.md) behind the same snapshot/report
   interface.

Resolved in Phase 1: adjacency requirement for `attack` (attacks must target
an adjacent hex; `simulate` shares all attack validation); attacker advance
after retreat (automatic, free — see Retreat execution); morale recovery over
time (turn-start drift toward the faction default — see Morale recovery above).

Resolved in Phase 3: damaged-element repair and replacement of destroyed
elements, both at turn-start and both gated on supply connectivity (`game::
refit`, `game::supply`) — see Loss flow above. Deliberately no unit
degradation or surrender for units cut off from supply; the prototype treats
"repair/replace stops" as encirclement's whole consequence.
