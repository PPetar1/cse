# CSE manual — how the game's systems work

Living document: the behavior of every system in the game, written for a
player (or a designer), not a programmer — no code references. When a system
changes, this file changes with it; the technical counterpart (where each
system lives in the code and how) is `docs/architecture.md`. As the game
matures this document grows into the player-facing manual.

The systems described here are prototype-grade by design (see
`docs/roadmap.md`, Stage 1): good enough to play and learn from, with the
real versions coming in Stage 2 and beyond. Each section ends with its known
simplifications and open questions.

## The world

The game is played on a hex map. Every hex has a terrain type — Plains,
Forest, Hills, Swamp, Desert, Mountain, Urban, Water — that shapes movement
cost, combat ranges, cover and defense. A map may also define named **offmap
boxes** (e.g. "GE Reserve"): holding areas where units can wait before
arriving or after withdrawing; nothing happens to a unit sitting in one.

Everything a scenario needs is data in editable text files: the map, the
factions and who controls them, the units, their equipment tables, the
weapons, terrain costs, victory conditions, reinforcement schedules, events,
supply sources, and optional rules like fog of war. Scenarios are content,
not code — a motivated player can edit or create one with a text editor.

### Units, TOEs, elements, devices

- A **unit** is a maneuver formation — a division, brigade or regiment. It
  occupies one hex (or an offmap box), has a movement-point budget, and is
  the thing you give orders to. Multiple friendly units may stack in a hex.
- A unit's **TOE** (table of equipment) prescribes what it's made of: a
  named list of element types and how many of each a full-strength unit
  fields, plus the unit's per-turn movement allowance and, for air units,
  its mission range.
- An **element** is one squad, gun or vehicle type — a rifle squad, a 45mm
  AT gun, a Pz IV, a Stuka. Elements are what actually fight and take
  losses. Each carries a combat value, a vulnerability rating (armor for
  vehicles, exposure for everything else), and its own morale and
  experience. Elements are classed by role (infantry, tanks, artillery, AT
  guns, ground-attack aircraft, fighters), which decides whether they're
  armored, whether they're aircraft, and what they may shoot at.
- A **device** is one weapon of an element — a rifle/LMG volley, a tank's
  main gun, its machine guns. Each has accuracy (chance a shot hits), range
  in meters, rate of fire (shots per combat round), and three attack values
  for how devastating a hit is: soft (against unarmored targets), hard
  (against armor), and air (against aircraft). A Pz IV sprays MG fire at
  infantry while taking aimed cannon shots at a gun line because those are
  different devices on the same element.

## The turn system

Play alternates player by player ("IGO-UGO"): each player moves and fights
with all their units, then passes. When every player has moved, the turn
counter advances and the in-game date jumps forward by the scenario's turn
length in days.

When a faction comes on turn, in order:

1. **Scheduled arrivals** — its reinforcements/withdrawals due this turn
   step on or off the map.
2. **Events** — its scenario events due this turn fire (a message, plus any
   morale/experience nudge to the faction's defaults).
3. **Refit** — its supplied units repair damaged elements and receive
   replacements (see Supply and refit).
4. **Entrenchment** — every on-map unit that hasn't relocated digs in one
   more level (see Entrenchment).
5. **Fresh movement points** from each unit's TOE, the faction's own
   interdiction declarations reset, and every element's morale drifts one
   step back toward the faction default (see Morale and experience).

A simultaneous ("WEGO") turn mode, where all players issue orders and they
resolve together, is a planned alternative — the scenario already chooses
its turn system, there's just only one choice so far.

## Movement

A unit spends **movement points (MP)** from its per-turn budget to move.
Entering a hex costs MP by terrain (the scenario can override the defaults;
a cost of zero makes a terrain impassable — Water is impassable by
default). A move can cross any number of hexes: the game finds the cheapest
path and charges its total cost, routing around impassable terrain and
enemy-occupied hexes automatically. Leaving a hex is free — only entering
costs — so a unit can path out of terrain it couldn't path into.

Enemy-occupied hexes can be neither entered nor crossed; taking ground held
by the enemy is what attacking is for. Friendly units stack freely. Only the
player on turn may move their units, and unspent MP does not carry over.

## Combat

### What WitE2 does (the studied reference)

CSE's combat model is built from a study of Gary Grigsby's *War in the
East 2* (rules readable at <https://dornshuld.com/rules/wite2/1-0.html>,
chapters 23, 12, 15, 21). The manual there describes the *structure* of
resolution but never publishes the inner to-hit formulas, so all concrete
math in CSE is original. WitE2's sequence: initiate → terrain/fort modifiers
→ commit support/reserve units → air missions → a multi-round fire phase at
closing range → final combat-value computation → outcome by odds ratio →
retreat consequences. Element fire runs eligibility checks (morale, supply,
fatigue, ammo, leaders), then shots by rate of fire, to-hit from device
accuracy vs target speed/size, damage vs armor/toughness. Hits leave an
element untouched, **disrupted** (stops firing, recovers after), **damaged**
(out of action, may recover or be captured), or **destroyed**. Final combat
values are heavily modified (terrain density, fort levels, leader rolls,
weather, command chain); modified odds of 2:1 or better force the defender
to retreat, with routs, shatters and surrenders for broken or trapped units.

### How a CSE battle runs

An attack sends every unit in one hex against every unit in an adjacent
hex; only the faction on turn may attack. The battle then plays out in
**rounds at closing range**: 3000 → 1500 → 800 → 400 → 100 meters, one round
per band. The defender's terrain sets the opening range — open ground starts
at 3000 m, close terrain (Hills, Forest, Swamp, Mountain) at 800 m, Urban at
400 m. An element fires in a round if at least one of its devices reaches
that range, so artillery and tank guns dominate the early rounds and
infantry joins as the range closes. Fire within a round is simultaneous:
every shot resolves against the situation at the start of the round, so
neither side gets a first-strike advantage. Damaged elements sit battles
out; each battle is fought by the ready ones.

Each ready, in-range element must first **commit**: it fires this round only
on a roll under its experience — green troops often fail to fire at all.
Then every in-range device fires its rate-of-fire worth of shots. Per shot:

1. **Target**: picked at random from the enemy's ready elements (restricted
   by domain — see Air combat below). Because every individual squad/gun/
   vehicle is a separate target, numerous element types soak proportionally
   more fire: an emergent screening effect, with no extra rules.
2. **To hit**: the device's accuracy, scaled down by the defender's terrain
   cover when firing *at* the defender (Plains/Desert 1.0, Hills 0.8, Swamp
   0.7, Forest 0.6, Mountain 0.5, Urban 0.4). Attackers are assumed to be
   advancing in the open — no cover for them.
3. **Effect**: the device engages with the attack value matching the
   target — hard (AP) against armored classes, air against aircraft, soft
   (small arms/HE) against everything else — scaled by the target's
   vulnerability. Because a target only ever receives the fire kind
   matching what it is, one vulnerability stat per element suffices.
4. **Severity** of an effective hit: 50% disrupted, 35% damaged, 15%
   destroyed. An element hit twice in a round keeps the worse result.

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
deliberate — outcomes ride on the odds ratio, and an additive modifier lets
elite-vs-green tilt the odds ~1.4× on stats alone where a multiplicative one
would hand them 3.5×, letting stats dwarf equipment.

The defender's final CV is then multiplied by terrain (Plains/Desert/Water
1.0, Hills 1.5, Forest/Swamp 2.0, Mountain/Urban 3.0) and by entrenchment
(+15% per fort level — see Entrenchment). **Modified odds of 2:1 or better
force the defender to retreat**; anything less and the defender holds.

### Retreats, routs, shatters, surrenders

A beaten defending stack must leave its hex:

- **Destination**: an adjacent on-map, non-Water hex free of enemy units,
  preferring the one farthest from the attacker (deterministic
  tie-breaking). The whole stack retreats to the same hex.
- **Retreat attrition**: each ready element has a 10% chance to end up
  damaged on the way; each damaged element — hard to drag along — has a 25%
  chance to be lost for good.
- **Rout**: a retreating unit routs when a roll beats its strength-weighted
  average morale. A routed unit suffers the retreat attrition twice, and
  the post-battle morale loss twice.
- **Shatter**: a routing unit whose ready strength has fallen below half
  its TOE disintegrates outright if a second roll also beats its morale —
  removed from the game. Fresh units never shatter from one lost battle;
  worn-down ones can.
- **Surrender**: with no valid destination, the defenders are cut off and
  surrender — removed from the game.
- **Advance after combat**: the attacking stack advances into the vacated
  hex automatically and at no MP cost — the battle already paid for the
  ground.

### Morale and experience

Every element carries morale and experience (0–100). The scenario sets them
at whatever granularity is convenient, most specific wins: a single element
type of a unit → the unit → the faction default → 50. Their effects:
experience gates commitment in battle, both scale CV, and a unit's
strength-weighted average morale gates routs.

Battles move both. **Experience**: every element type that fielded troops
in a battle gains experience, winners and losers alike, tapering as it
rises — green troops learn fast (+7 around 35), veterans barely (+1 at 90).
A side effect worth knowing: repeatedly battering a defender also trains
them. Experience is permanent. **Morale** settles after the outcome and is
collective — every element of a participating unit shifts, fought or not.
Winners rally toward 100, losers sag toward 0 (routed units sag twice),
both tapering near the bounds. This is a deliberate feedback loop: repulsed
attacks rally the defender and discourage the attacker, so grinding a
position has a real psychological price.

Between battles, morale **recovers**: at its faction's turn start, every
element drifts one tapering step toward the faction's default morale, from
either side — rest heals battered units, battle euphoria fades. The drift
is gentler than battle shifts, so combat outcomes dominate. Scenario events
can move the faction default itself, which shifts what everyone drifts
toward (mirroring WitE's national-morale anchor).

### Air combat

Air warfare is not a separate engine: aircraft are elements like any other,
and air-vs-ground combat is one unified battle with **domain-restricted
targeting**:

- **Fighters** only ever engage other aircraft. They cannot touch a ground
  target, no matter what's in range.
- **Ground-attack aircraft** (bombers/CAS) engage ground targets normally
  and aircraft weakly (a rear-gunner potshot, not their job).
- **Ground elements** cannot engage aircraft at all — unless flagged as
  **anti-air** (dual-purpose flak), which lets them fire at aircraft while
  keeping their normal ground fire.

When nothing airborne is present, targeting works exactly as if the rule
didn't exist.

The air missions:

- **Air support**: one owned air unit's elements join an ongoing ground
  attack as extra firers, for that battle only. The unit must belong to the
  attacking faction, must not already be part of the ground stack, and must
  be within mission range. It fights, takes losses and gains experience
  like anyone else — but it never advances into a vacated hex, never shifts
  morale with the ground stack's outcome, and stays at its base regardless
  of how the battle goes.
- **Interdiction**: a fighter-capable unit *declares* coverage of a hex, up
  to 3 hexes per unit at a time. Any battle fought at a covered hex —
  ground attack or air-supported — automatically pulls the covering unit in
  as an extra defender. A fighter pulled into a battle with no aircraft on
  the other side is a harmless bystander (it can neither shoot nor be shot
  at), though it still gains experience for being fielded. Coverage lasts
  through the opponent's next turn and must be redeclared every time the
  covering faction acts again — a scarce, deliberate commitment rather than
  a blanket air umbrella.
- **Airfields**: an air unit's base is simply wherever it currently sits —
  on the map like any ground unit, or in an offmap box. Its TOE may cap how
  many hexes from its base its missions (air support, interdiction) can
  reach; a unit still offmap has no position to measure from, so no limit
  applies. In principle an air unit could redeploy by ordinary movement if
  its TOE gave it movement points; the shipped scenarios keep their air
  bases stationary.

Not yet modeled: escort (attacker-side fighters protecting their own CAS),
stand-alone air strikes without a ground attack, and any AI use of air
missions (see The AI opponent).

### Deliberate deviations from WitE2 (for now)

- Flat defender terrain CV multiplier instead of WitE2's per-class density
  rules (infantry ×2 / AFV ×0.5 in dense terrain). Worth adopting once
  battles are tunable via simulation.
- Fixed one round per range band; WitE2 has a variable number and can break
  off battles early on bad odds.
- No fatigue, ammo or leader checks, no support/reserve commitment, no CPP
  or weather.

### The simulation tool

The `simulate` command fights the same attack any number of times against
copies of the real situation — the game itself is untouched — and reports
hold/retreat rates, average losses per side and mean final CVs. This is the
balance-tuning loop: change a knob, re-run, compare distributions. A
thousand runs resolve in under a second. It also underpins a planned
low-randomness battle mode (resolve each battle as the average of many
simulated runs), and it is exactly what the AI uses to judge its attacks.

### Balance history (pre-tuning)

The current knob settings were reached step by step, using simulation runs
on the development scenario as the yardstick:

- Raw CVs only: repeated frontal attacks by a panzer division into a
  forest-defending rifle division ground the *attacker* down (CV 984 → 148
  over twelve attacks) while the defender barely dented — tanks fired only
  AP at near-immune squads, forest cover protected the defender, and the
  defender's howitzers fired every round. Plausible flavor, but it proved
  dual soft/hard fire values had to come first.
- With experience in play (veteran panzers vs green infantry), the same
  runs moved the right way: the forest attack traded ~evenly, the green
  counter-attack lost ~1.7:1. Still 0% retreats anywhere.
- With CV modifiers on top, the stat gap showed in the odds too — but
  retreats stayed at 0%; the soft/hard split was still the lever.
- With soft/hard fire values (tanks can finally hurt infantry), the picture
  clicked: the panzer attack into forest inflicts ~2.5:1 losses but the
  defender holds 100% at 1.2:1 odds, while the same attack against infantry
  caught on open plains forces a retreat 100% of the time at 2.8:1. Dug-in
  forest infantry holding a frontal armor attack while bleeding, and
  getting thrown back in the open, is the intended flavor.
- Devices and rate of fire keep that shape but make battles bloodier (MGs
  joining at close range): forest holds 100% with ~3:1 defender losses,
  plains collapse 100% at 3.5:1. If attrition feels too high, rate of fire
  and accuracy are the knobs.

Open questions: retune the numeric knobs (severity split, cover table, CV
multipliers, retreat attrition) with simulation evidence; longer term, the
2D tactical battlefield experiment from `docs/ideas.md` could swap in
behind the same battle interface and be compared via the simulator.

## Supply and refit

A scenario declares **supply sources** — hexes each faction traces its
connectivity back to. A unit is **in supply** if a path of passable terrain
free of enemy-occupied hexes connects its hex to one of its faction's
sources (the same rules movement follows; distance doesn't matter, only
connectivity). A source hex the enemy currently holds supplies no one.
Supply is computed fresh whenever it's asked for; the `supply` command
reports every on-map unit as supplied or cut off.

Supply's one gameplay effect is **refit**, at every turn start, for each
on-map unit of the faction coming on turn that is in supply:

- **Repair**: a fraction of its damaged elements return to ready — a
  quarter of the damaged pool (rounded up) per turn, tapering as the pool
  shrinks.
- **Replacements**: for each element type below its TOE-prescribed count,
  an eighth of the shortfall (rounded up) arrives as fresh ready elements —
  tapering the same way, and never exceeding the TOE.

A cut-off unit gets neither. That is deliberately encirclement's whole
consequence in the prototype: pockets wither, they don't starve and
surrender (lethal encirclement, if it returns, belongs to the detailed
logistics rework — see the roadmap). Offmap units don't trace supply and
don't refit.

## Entrenchment

A unit that stays put digs in: one **fort level** per turn spent
stationary, up to level 5. Relocating for any reason — moving, retreating,
advancing into a vacated hex, arriving as a reinforcement — resets it to
zero; holding in place, or losing a battle without being forced back, does
not.

Entrenchment is purely defensive: each average fort level across the
defending stack adds +15% to the defenders' final CV (so a fully dug-in
stack defends at +75%), stacking with terrain. It never helps an attacker.
The dig-in rate and cap are universal rules, not scenario settings.

Fort levels are visible everywhere the unit is: inspection text and, on the
map, one small pip per level under the unit's marker.

Open questions: no way for a scenario to start units pre-entrenched (a
historically prepared defensive line); no per-TOE variation (engineers
faster, armor slower); no attacker-side "prepared position" benefit such as
defensive first fire — deliberately, for now.

## Fog of war and detection

Off by default; a scenario opts in by setting a **detection range** in
hexes. With fog of war on, a faction sees an enemy unit only if one of its
own on-map units is within detection range of that unit's hex — pure
distance, no line-of-sight or terrain blocking. Your own units are always
visible; an enemy unit still in an offmap reserve box is never visible.

Fog of war is **information denial only** — it changes what you're told,
never what's legal. Unit listings show your own units plus only detected
enemies; inspecting an unseen hex reports it as unknown; the map draws no
marker for an undetected enemy. But orders validate exactly as before: you
can attack a hex you can't fully see into, just without knowing what's
there. Deliberately not hidden: victory-hex holders (scoreboard
information, like the turn counter), reinforcement and event schedules,
and supply status. The AI ignores fog of war entirely and plays with full
information — a known, accepted simplification.

Open questions: line-of-sight/terrain blocking (today a unit sees straight
through a mountain), contact persistence (a "last known position, fading
over turns" instead of a hard binary), per-TOE detection ranges (recon),
and eventually teaching the AI to respect it.

## Victory conditions

A scenario may define how it's scored: a **last turn**, **objective hexes**
each worth flat points to whoever holds them at the end, and per-faction
points for the percentage of enemy starting strength destroyed minus a
penalty for the percentage of own strength lost (measured in elements,
against each faction's strength at scenario start). When the last turn
completes, every faction's score prints and the highest total wins — a tie
is a draw.

The `victory` command shows the conditions and each objective hex's current
holder at any time, and objective hexes carry a flag marker with their
point value on the map.

Two honest caveats: a scenario with no last turn never scores or ends on
its own, and scoring is currently report-only — nothing stops further
orders after the final score prints. A hard game-over gate is a known gap.

## Reinforcements, withdrawals and events

A scenario can schedule **reinforcements** (a unit steps onto the map at a
given turn, typically from an offmap box) and **withdrawals** (the
reverse). Both fire the moment the owning faction's turn reaches the
scheduled number — including the very first turn — and both are the same
mechanism: a relocation at a scheduled time. The `reinforcements` command
lists the schedule with each entry's pending/arrived status.

**Events** fire the same way: at a scheduled turn, for a scheduled faction,
a message prints and the faction's default morale and/or experience shift
by the event's deltas (staying within 0–100). Because events land before
the same turn's morale drift, an event's morale nudge immediately steers
what that faction's elements drift toward. The `events` command lists the
schedule with pending/fired status.

## The AI opponent

A scenario can hand any faction to the AI, whose turns then play themselves
whenever control reaches it, with a printed report of everything it did (you
should always be able to see *why* the AI did what it did — transparency is
a design pillar). Its rules, deliberately simple:

- For each stack of its units: simulate an attack against each adjacent
  enemy hex, and attack the best one if the predicted retreat rate clears
  60% — it only takes clearly winning fights.
- Otherwise, move the stack toward the nearest objective hex it doesn't
  hold, or the nearest enemy unit if the scenario has no objectives —
  jumping as far as movement allows, or a single best step if the full move
  fails.

Known, accepted simplifications: the AI never uses air support or
interdiction (its air units sit idle), it plays with full information under
fog of war, and its strength is not the point — it exists to fight back, and
a stronger AI later replaces its decision-making without changing how it's
plugged in.

## Saved games

The game can be saved to and loaded from a file at any point, from the
terminal or the window; everything that defines the session — units,
losses, morale, schedules still pending, whose turn it is — survives the
round trip. Pending schedule entries are carried in the save, so a
reinforcement due on turn 5 still arrives if you save on turn 3 and load
later.
