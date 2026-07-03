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
engine, applies losses back, and prints the report. No unit movement yet — a
"defender retreats" result is reported but not executed (needs
adjacency/stacking rules first).

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
- **Damaged**: persisted `ready → damaged` on the owning unit. No repair system
  yet (comes with the turn system).
- **Destroyed**: `ready` decremented, element gone.

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
  twice.
- **Surrender**: if no valid destination exists, the defenders are cut off and
  surrender — the units are removed from the game.

Attackers stay put; advancing into the vacated hex is an open question.

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
2. Morale/experience effects beyond commit/CV/rout: shatter, experience gain
   from battles, morale drops from lost battles (the event hook for shifting
   faction defaults over time exists on the runtime player).
3. Attacker advance into the vacated hex after a retreat?
4. Adjacency requirement for `attack` (arrives with movement rules).
5. Longer term: swap-in experiment — 2D tactical battlefield with generated
   terrain and LOS (see docs/ideas.md) behind the same snapshot/report
   interface.
