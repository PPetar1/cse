# CSE manual — how the game's systems work

The behavior of every system in the game, written for a player or designer —
no code references (those live in `docs/architecture.md`). When a system
changes, this file changes with it; as the game matures it grows into the
player-facing manual.

Everything here is prototype-grade by design (`docs/roadmap.md`, Stage 1):
good enough to play and learn from, with the real versions coming in Stage 2
and beyond. Each section ends with its known simplifications and open
questions.

## The world

The game is played on a hex map. Every hex has a terrain type — Plains,
Forest, Hills, Swamp, Desert, Mountain, Urban, Water — shaping movement
cost, combat ranges, cover and defense. A map may also define named
**offmap boxes** (e.g. "GE Reserve"): holding areas where units wait before
arriving or after withdrawing; nothing happens to a unit in one.

A scenario is entirely data in editable text files — map, factions and
their controllers, units, equipment tables, weapons, terrain costs, victory
conditions, reinforcement schedules, events, supply sources, optional rules
like fog of war. Scenarios are content, not code: a motivated player can
create one with a text editor.

### Units, TOEs, elements, devices

- A **unit** is a maneuver formation (division, brigade, regiment): it
  occupies one hex or offmap box, has a movement-point budget, and is what
  you give orders to. Friendly units stack freely.
- Its **TOE** (table of equipment) prescribes its composition — element
  types and full-strength counts — plus the per-turn movement allowance
  and, for air units, a mission range.
- An **element** is one squad, gun or vehicle type — a rifle squad, a 45mm
  AT gun, a Pz IV, a Stuka. Elements do the fighting and take the losses.
  Each carries a combat value, a vulnerability rating (armor for vehicles,
  exposure otherwise) and its own morale and experience, and is classed by
  role (infantry, tanks, artillery, AT guns, ground-attack aircraft,
  fighters) — the class decides whether it's armored, an aircraft, and
  what it may shoot at.
- A **device** is one weapon of an element — a rifle/LMG volley, a tank's
  main gun, its machine guns — with accuracy (chance a shot hits), range in
  meters, rate of fire (shots per combat round), and three hit-effect
  values: soft (vs unarmored), hard (vs armor), air (vs aircraft). A Pz IV
  sprays MG fire at infantry while taking aimed cannon shots at a gun line
  because those are different devices on one element.

### Leaders

A **leader** is a named commander, defined per faction in the scenario with
the same seven ratings *War in the East 2* uses (rules at
<https://dornshuld.com/rules/wite2/15-0.html>, chapter 15): Political,
Morale, Initiative, Administration, Mechanized, Infantry and Air, each 1-9.
A leader can be assigned to any one unit at a time — the scenario can start
one there, and the `reassign_leader` command can move one between units
later, unassigning it from wherever it was. A unit can go without a leader
entirely.

For now leaders are purely informational: their stats have no gameplay
effect. They're groundwork for a future command/effectiveness system, and
today's blanket "leaders can command any unit" will narrow once unit types
and HQs exist — a Corps or Army leader restricted to the HQ level they fit,
the way WitE2 does it.

## The turn system

Play alternates player by player ("IGO-UGO"): each moves and fights with
all their units, then passes. When everyone has moved, the turn counter
advances and the date jumps by the scenario's turn length in days.

When a faction comes on turn, in order:

1. **Scheduled arrivals** — its reinforcements/withdrawals due this turn
   step on or off the map.
2. **Events** — its scenario events due this turn fire (a message plus any
   morale/experience nudge to the faction defaults).
3. **Refit** — its supplied units repair and receive replacements.
4. **Entrenchment** — every on-map unit that hasn't relocated digs in one
   level.
5. **Housekeeping** — fresh movement points from each unit's TOE, the
   faction's interdiction declarations reset, and every element's morale
   drifts one step toward the faction default.

A simultaneous ("WEGO") mode, where all players issue orders that resolve
together, is a planned alternative — scenarios already choose their turn
system; there's just only one choice so far.

## Movement

A unit spends **movement points (MP)** from its per-turn budget. Entering a
hex costs MP by terrain (scenario-overridable; cost zero = impassable —
Water by default). A move can cross any number of hexes: the game charges
the cheapest path's total cost, routing around impassable terrain and enemy
hexes automatically. Only entering costs — leaving is free — so a unit can
path out of terrain it couldn't path into.

Enemy-occupied hexes can be neither entered nor crossed; taking ground is
what attacking is for. Only the player on turn may move, and unspent MP
doesn't carry over.

## Combat

### What WitE2 does (the studied reference)

The combat model is built from a study of *War in the East 2* (rules at
<https://dornshuld.com/rules/wite2/1-0.html>, chapters 23, 12, 15, 21),
whose manual describes the *structure* of resolution but not the inner
formulas — all concrete math in CSE is original. WitE2's sequence: initiate
→ terrain/fort modifiers → commit support/reserves → air missions → a
multi-round fire phase at closing range → final combat values → outcome by
odds ratio → retreat consequences. Element fire runs eligibility checks
(morale, supply, fatigue, ammo, leaders), then shots by rate of fire,
to-hit from device accuracy vs target speed/size, damage vs armor/
toughness; hits leave an element untouched, **disrupted** (stops firing,
recovers after), **damaged** (out of action, may recover or be captured) or
**destroyed**. Final CVs are heavily modified (terrain density, forts,
leaders, weather, command chain); modified odds of 2:1 or better force the
defender out, with routs, shatters and surrenders for broken or trapped
units.

### How a CSE battle runs

An attack sends every unit in one hex against every unit in an adjacent
hex; only the faction on turn may attack. The battle plays out in **rounds
at closing range** — 3000 → 1500 → 800 → 400 → 100 m, one round per band.
The defender's terrain sets the opening range: open ground 3000 m, close
terrain (Hills, Forest, Swamp, Mountain) 800 m, Urban 400 m. An element
fires in a round if any of its devices reaches that range, so artillery and
tank guns dominate early and infantry joins as the range closes. Fire
within a round is simultaneous — every shot resolves against the situation
at the round's start, no first-strike advantage. Damaged elements sit
battles out.

Each ready, in-range element must first **commit**: it fires only on a roll
under its experience — green troops often fail to fire at all. Then every
in-range device fires its rate-of-fire worth of shots. Per shot:

1. **Target**: random among the enemy's ready elements (restricted by
   domain — see Air combat). Every individual squad/gun/vehicle is a
   separate target, so numerous element types soak proportionally more
   fire — an emergent screening effect with no extra rules.
2. **To hit**: the device's accuracy, scaled by the defender's terrain
   cover when firing *at* the defender (Plains/Desert 1.0, Hills 0.8,
   Swamp 0.7, Forest 0.6, Mountain 0.5, Urban 0.4). Attackers are assumed
   to be advancing in the open — no cover.
3. **Effect**: the attack value matching the target — hard (AP) against
   armored classes, air against aircraft, soft (small arms/HE) against the
   rest — scaled by the target's vulnerability. A target only ever receives
   the fire kind matching what it is, so one vulnerability stat suffices.
4. **Severity**: 50% disrupted, 35% damaged, 15% destroyed. Hit twice in a
   round, an element keeps the worse result.

### Loss states

- **Disrupted**: out of the rest of this battle and the final combat value;
  fully recovers afterwards, leaving no trace.
- **Damaged**: persists on the unit after the battle; repairs over time via
  refit, if the unit is in supply.
- **Destroyed**: gone for good; replaced over time via refit, if in supply.

### Outcome

When the rounds end, each side's **final combat value (CV)** is the sum of
its still-ready elements' CVs, each scaled by morale and experience: ×1 at
0/0, ×2 at the 50/50 baseline, ×3 at 100/100. The additive form is
deliberate: outcomes ride on the odds ratio, and additive scaling lets
elite-vs-green tilt the odds ~1.4× on stats alone where multiplicative
would hand them 3.5×, letting stats dwarf equipment.

The defender's final CV is then multiplied by terrain (Plains/Desert/Water
1.0, Hills 1.5, Forest/Swamp 2.0, Mountain/Urban 3.0) and by entrenchment
(+15% per fort level). **Modified odds of 2:1 or better force the defender
to retreat**; anything less holds.

### Retreats, routs, shatters, surrenders

A beaten defending stack must leave its hex:

- **Destination**: an adjacent on-map, non-Water hex free of enemies,
  preferring the farthest from the attacker (deterministic tie-break); the
  whole stack retreats together.
- **Retreat attrition**: each ready element has a 10% chance to arrive
  damaged; each damaged element — hard to drag along — a 25% chance to be
  lost for good.
- **Rout**: a retreating unit routs when a roll beats its strength-weighted
  average morale — it takes the retreat attrition twice, and the
  post-battle morale loss twice.
- **Shatter**: a routing unit whose ready strength is below half its TOE
  disintegrates if a second roll also beats its morale — removed from the
  game. Fresh units never shatter off one lost battle; worn-down ones can.
- **Surrender**: no valid destination — the defenders are cut off and
  removed from the game.
- **Advance after combat**: the attackers advance into the vacated hex
  automatically, at no MP cost — the battle paid for the ground.

### Morale and experience

Every element carries morale and experience (0–100), set by the scenario at
whatever granularity is convenient — element of a unit → unit → faction
default → 50, most specific wins. Experience gates commitment, both scale
CV, and a unit's strength-weighted average morale gates routs.

Battles move both. **Experience**: every element type that fielded troops
gains it, winners and losers alike, tapering as it rises (+7 around 35, +1
at 90) — so repeatedly battering a defender also trains them. Experience is
permanent. **Morale** shifts after the outcome and is collective — every
element of a participating unit shifts, fought or not: winners rally toward
100, losers sag toward 0 (routed units doubly), both tapering near the
bounds. A deliberate feedback loop: repulsed attacks rally the defender and
discourage the attacker, so grinding a position has a psychological price.

Between battles morale **recovers**: at its faction's turn start every
element drifts one tapering step toward the faction default, from either
side — rest heals battered units, battle euphoria fades. The drift is
gentler than battle shifts, so combat dominates. Events can move the
faction default itself, shifting what everyone drifts toward (mirroring
WitE's national-morale anchor).

### Air combat

Air warfare is not a separate engine: aircraft are elements, and
air-vs-ground is one unified battle with **domain-restricted targeting**:

- **Fighters** only ever engage other aircraft — never a ground target.
- **Ground-attack aircraft** (bombers/CAS) engage ground targets normally
  and aircraft weakly (a rear-gunner potshot, not their job).
- **Ground elements** can't engage aircraft unless flagged **anti-air**
  (dual-purpose flak), which adds air fire to their normal ground fire.

With nothing airborne present, targeting works exactly as if the rule
didn't exist.

The missions:

- **Air support**: one owned air unit's elements join an ongoing ground
  attack as extra firers, for that battle only. The unit must belong to the
  attacking faction, not already be in the ground stack, and be within
  mission range. It fights, takes losses and gains experience like anyone
  else — but never advances into a vacated hex, never shares the ground
  stack's morale shift, and stays at its base whatever the outcome.
- **Interdiction**: a fighter-capable unit *declares* coverage of a hex —
  up to 3 per unit at a time. Any battle at a covered hex, ground or
  air-supported, automatically pulls the covering unit in as an extra
  defender. Pulled into a battle with no enemy aircraft, a fighter is a
  harmless bystander (can neither shoot nor be shot at) but still gains
  experience for being fielded. Coverage lasts through the opponent's next
  turn, then must be redeclared — a scarce commitment, not a blanket air
  umbrella.
- **Airfields**: an air unit's base is wherever it sits — on the map or in
  an offmap box. Its TOE may cap mission reach in hexes from that base; a
  unit still offmap has no position to measure from, so no limit applies.
  An air unit could in principle redeploy by ordinary movement if its TOE
  gave it MP; the shipped scenarios keep theirs stationary.

Not yet modeled: escort (attacker-side fighters protecting their own CAS),
stand-alone air strikes without a ground attack, and AI use of air missions
(see The AI opponent).

### Deliberate deviations from WitE2 (for now)

- Flat defender terrain CV multiplier instead of WitE2's per-class density
  rules (infantry ×2 / AFV ×0.5 in dense terrain). Worth adopting once
  battles are tunable via simulation.
- Fixed one round per range band; WitE2 has a variable number and can break
  off battles early on bad odds.
- No fatigue, ammo or leader checks, no support/reserve commitment, no CPP
  or weather.

### The simulation tool

`simulate` fights the same attack any number of times against copies of the
real situation — the game is untouched — and reports hold/retreat rates,
average losses and mean final CVs; a thousand runs resolve in under a
second. It's the balance-tuning loop (change a knob, re-run, compare), the
planned basis for a low-randomness battle mode (resolve as the average of
many runs), and exactly what the AI uses to judge attacks.

### Balance history (pre-tuning)

The current knobs were reached stepwise, judged by simulation runs on the
development scenario:

- Raw CVs only: a panzer division frontally attacking forest-dug riflemen
  ground *itself* down (CV 984 → 148 over twelve attacks) while barely
  denting the defender — tanks fired only AP at near-immune squads, forest
  cover shielded the defender, its howitzers fired every round. Plausible
  flavor, but proof that dual soft/hard fire had to come first.
- Experience (veteran panzers vs green infantry) moved the same runs the
  right way: the forest attack traded ~evenly, the green counter-attack
  lost ~1.7:1. Still 0% retreats.
- CV modifiers put the stat gap in the odds too — retreats still 0%; the
  soft/hard split remained the lever.
- Soft/hard fire values (tanks finally hurt infantry) made it click: the
  forest attack inflicts ~2.5:1 losses but the defender holds 100% at 1.2:1
  odds, while the same attack on open plains forces a retreat 100% of the
  time at 2.8:1 — dug-in forest infantry bleeding but holding, and getting
  thrown back in the open, is the intended flavor.
- Devices and rate of fire keep that shape but bloodier (MGs joining at
  close range): forest holds 100% at ~3:1 defender losses, plains collapse
  100% at 3.5:1. If attrition feels too high, rate of fire and accuracy are
  the knobs.

Open questions: retune the numeric knobs (severity split, cover table, CV
multipliers, retreat attrition) with simulation evidence; longer term, the
2D tactical battlefield experiment from `docs/ideas.md` could swap in
behind the same battle interface and be compared via the simulator.

## Supply and refit

A scenario declares **supply sources** — hexes each faction traces
connectivity to. A unit is **in supply** if passable terrain free of enemy
hexes connects it to one of its faction's sources (movement's rules;
distance is irrelevant, only connectivity). A source the enemy holds
supplies no one. Supply is computed fresh whenever asked; the `supply`
command reports every on-map unit as supplied or cut off.

Its one gameplay effect is **refit**, each turn start, for every supplied
on-map unit of the faction coming on turn:

- **Repair**: a quarter of its damaged elements (rounded up) return to
  ready per turn, tapering as the pool shrinks.
- **Replacements**: for each element type below TOE strength, an eighth of
  the shortfall (rounded up) arrives ready — tapering likewise, never
  exceeding the TOE.

A cut-off unit gets neither — deliberately encirclement's whole consequence
in the prototype: pockets wither, they don't starve and surrender (lethal
encirclement, if it returns, belongs to the detailed logistics rework).
Offmap units don't trace supply and don't refit.

## Entrenchment

A unit that stays put digs in: one **fort level** per stationary turn, up
to 5. Relocating for any reason — moving, retreating, advancing, arriving
as a reinforcement — resets it to zero; holding in place, or losing a
battle without being forced back, does not.

Purely defensive: each average fort level across the defending stack adds
+15% to the defenders' final CV (a fully dug-in stack defends at +75%),
stacking with terrain; it never helps an attacker. The rate and cap are
universal rules, not scenario settings. Fort levels show in inspection text
and as one pip per level under the unit's map marker.

Open questions (deliberate, for now): no pre-entrenched scenario starts (a
prepared defensive line), no per-TOE variation (engineers faster, armor
slower), no attacker-side prepared-position benefits such as defensive
first fire.

## Fog of war and detection

Off by default; a scenario opts in with a **detection range** in hexes.
With it on, a faction sees an enemy unit only if one of its own on-map
units is within range of that unit's hex — pure distance, no line-of-sight
or terrain blocking. Own units are always visible; an enemy still in an
offmap box never is.

Fog of war is **information denial only** — it changes what you're told,
never what's legal. Unit listings show only detected enemies, inspecting an
unseen hex reports unknown, the map draws no marker for an undetected
enemy — but orders validate exactly as before: you can attack into a hex
you can't see, just blind. Deliberately not hidden: victory-hex holders
(scoreboard information, like the turn counter), reinforcement/event
schedules, and supply status. The AI ignores fog of war and plays with full
information — a known, accepted simplification.

Open questions: line-of-sight/terrain blocking (today a unit sees straight
through a mountain), contact persistence ("last known position, fading"
instead of a hard binary), per-TOE detection ranges (recon), teaching the
AI to respect it.

## Victory conditions

A scenario may define its scoring: a **last turn**, **objective hexes**
worth flat points to whoever holds them at the end, and per-faction points
for the percentage of enemy starting strength destroyed minus a penalty for
own strength lost (measured in elements against each faction's strength at
scenario start). When the last turn completes, every faction's score prints
and the highest total wins; a tie is a draw.

The `victory` command shows the conditions and each objective's current
holder at any time; objective hexes carry a flag marker with their point
value on the map.

Two honest caveats: a scenario with no last turn never scores or ends on
its own, and scoring is report-only — nothing stops further orders after
the final score prints. A hard game-over gate is a known gap.

## Reinforcements, withdrawals and events

A scenario can schedule **reinforcements** (a unit steps onto the map at a
given turn, typically from an offmap box) and **withdrawals** (the
reverse) — the same mechanism, a relocation at a scheduled time, firing the
moment the owning faction's turn reaches the scheduled number, including
the very first turn. The `reinforcements` command lists the schedule with
pending/arrived status.

**Events** fire the same way: at a scheduled turn, for a faction, a message
prints and the faction's default morale and/or experience shift by the
event's deltas (staying within 0–100). Events land before the same turn's
morale drift, so a morale nudge immediately steers what that faction's
elements drift toward. The `events` command lists the schedule with
pending/fired status.

## The AI opponent

A scenario can hand any faction to the AI; its turns then play themselves
whenever control reaches it, with a printed report of everything it did
(you should always see *why* — transparency is a design pillar). Its rules,
deliberately simple:

- Per stack: simulate an attack on each adjacent enemy hex, and attack the
  best if the predicted retreat rate clears 60% — only clearly winning
  fights.
- Otherwise move the stack toward the nearest objective hex it doesn't hold
  (or the nearest enemy if the scenario has none) — as far as movement
  allows, or a single best step if the full move fails.

Known, accepted simplifications: it never uses air support or interdiction
(its air units sit idle), it ignores fog of war, and its strength isn't the
point — it exists to fight back; a stronger AI later replaces the
decision-making without changing how it's plugged in.

## Saved games

The game saves to and loads from a file at any point, from terminal or
window; everything defining the session — units, losses, morale, pending
schedules, whose turn it is — survives the round trip (a reinforcement due
on turn 5 still arrives if you save on turn 3 and load later).
