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
iff its `range` stat ≥ the current band. Fire within a round is simultaneous:
all shots resolved against the round's starting states, then effects applied —
no first-strike advantage from code ordering.

### One shot, three rolls

Each Ready, in-range element fires one shot per round (rate of fire: later):

1. **Target**: uniform random over enemy Ready elements. Because the snapshot
   is per-instance, numerous element types soak proportionally more fire — an
   emergent screening effect, no extra rules.
2. **To hit**: `accuracy × cover(defender terrain)` as a percentage, cover
   applying only to shots *at the defender* (attacker is assumed advancing in
   the open). Cover: Plains/Desert/Water 1.0, Hills 0.8, Swamp 0.7, Forest 0.6,
   Mountain 0.5, Urban 0.4.
3. **Damage save**: hit takes effect if a d100 roll is under the target's
   vulnerability — `v_arm` if the firer's class shoots armor-piercing
   (AtGun/LightTank/MedTank), `v_inf` otherwise (Inf/MotInf/LightArt).
4. **Severity** on a failed save: 50% disrupted, 35% damaged, 15% destroyed.

### Loss flow

- **Disrupted**: out of the rest of this battle and the final CV; fully
  recovers afterwards (nothing persisted).
- **Damaged**: persisted `ready → damaged` on the owning unit. No repair system
  yet (comes with the turn system).
- **Destroyed**: `ready` decremented, element gone.

### Outcome

Final CV per side = Σ `cv` of elements still Ready. Defender CV is multiplied
by a flat terrain defense factor (Plains/Desert/Water 1.0, Hills 1.5,
Forest/Swamp 2.0, Mountain/Urban 3.0). **Odds ≥ 2:1 → defender retreats**,
else defender holds. No rout/shatter/surrender yet (need morale).

### Randomness & testing

All rolls go through a `rand::Rng` passed in by the caller — the command loop
passes a fresh thread RNG, tests pass `StdRng::seed_from_u64(...)` for exact
reproducibility. This is also what will power the planned `simulate` command
(run a matchup N times, print outcome/loss distributions) and the
low-randomness battle setting idea (see docs/ideas.md).

## Deliberate deviations from WitE2 (for now)

- Flat defender terrain CV multiplier instead of WitE2's per-class density
  rules (infantry ×2 / AFV ×0.5 in dense terrain). Worth adopting once battles
  are tunable via `simulate`.
- Single fire type per element class. WitE2 devices carry both AP and HE
  effect; our tanks always fire AP, so they are weak vs infantry (v_arm 3 on
  squads). Likely the first data-model extension combat needs.
- Fixed one round per range band; WitE2 has a variable number and can break
  off battles early on bad odds.
- No morale/experience/fatigue/ammo/leader checks, no fort levels, no support
  or reserve commitment, no retreat execution or attrition.

## Open questions / next steps (rough order)

1. Retreat execution: move retreating defenders to an adjacent hex (needs
   neighbor logic + stacking decisions). Attacker occupies on empty hex?
2. `simulate <battle> <n>` dev command for tuning; then revisit the numeric
   knobs (severity split, cover table, CV multipliers) with evidence.
3. Rate of fire + dual AP/HE fire values per element.
4. Morale & experience (enables rout/shatter outcomes and eligibility checks).
5. Adjacency requirement for `attack` (arrives with movement rules).
6. Longer term: swap-in experiment — 2D tactical battlefield with generated
   terrain and LOS (see docs/ideas.md) behind the same snapshot/report
   interface.
