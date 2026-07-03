# CSE roadmap — from engine to finished game

The destination is a **finished game that is a real improvement over Gary
Grigsby's *War in the East***. That road is longer than any honest list of
phases can capture, so this document works in two horizons:

- **Part 1 — the prototype.** Concrete and phased, landmark by landmark. The
  result is a playable game that clearly shows the game's spirit — not at the
  complexity level of the final product.
- **Part 2 — the game.** Abstract and non-exhaustive: the major feature areas
  that separate the prototype from a finished product. Unordered, no landmarks;
  each gets its own plan when its time comes.

Timescale is deliberately ignored — years are fine. When a phase begins, it
gets broken down into concrete work in CLAUDE.md ("Current focus") and the
design docs (`combat_design.md`, `ideas.md`). When direction changes, change
this file.

## What "better than WitE" means

WitE is the benchmark for operational depth. CSE keeps the depth and beats it on
five fronts — these pillars are the tiebreaker whenever a design decision is
unclear:

1. **One engine, any era.** WitE hardcodes 1941–45. CSE mechanics are generic
   (fires, devices, elements, TOEs); eras are data packs. Napoleonic to modern
   conflicts from the same binary, eventually with air — and possibly naval —
   warfare as additional domains, not parallel engines.
2. **Transparency.** WitE's combat is famously a black box. CSE's battle engine
   is inspectable by design (battle reports, `simulate`); the player must always
   be able to answer *"why did I lose that battle?"* — and tooling for that
   question should eventually be in the game itself, not just the dev console.
3. **Easy to start, deep to master.** These games are made for the people who
   love them — mastery can legitimately take a 400-page manual, and that depth
   is the point, not a flaw. The improvement over WitE is the on-ramp: small
   scenarios first, a clean UI, mechanics explained where they act — the manual
   is where mastery deepens, not the toll booth at the entrance.
4. **Moddability.** Everything is data: scenarios, maps, TOEs, elements,
   devices are TOML a motivated player can edit. The scenario editor is a text
   editor on day one; validation (`State::build`) is the safety net.
5. **Modern multiplayer.** Beyond hot-seat/PBEM — a server-mediated or
   simultaneous-turn mode designed in once the turn structure is proven, with
   multiple players per faction (see ideas.md, faction/player separation).

## Part 1 — the prototype

Each phase ends at a **landmark** — a moment the project is visibly more of a
game. Cross-cutting work (combat knob retuning, AP penetration, AoE fire,
refactors) threads through whichever phase strains it first. Systems built here
are prototype-grade on purpose: good enough to play and to learn from, with the
real versions coming in Part 2.

### Phase 0 — Combat core ✅ (done)

Fires-based battle engine: range bands, device-level weapons, morale/experience
with battle feedback, routs/shatters/surrenders, `simulate` tuning tool.

### Phase 1 — The game loop

Turn/phase system fused with movement rules: `end_turn`, alternating players,
date advancement, MP budgets, terrain costs, adjacency-gated attacks. Mops up
everything waiting on the turn clock: morale recovery, attacker advance after
retreat, real dates. **Landmark: sit down and play a full turn.**

### Phase 2 — The first winnable scenario

Victory conditions (hold these hexes by turn N), scheduled reinforcements and
withdrawals from offmap boxes, first scenario events. A small historical
scenario at division scale. **Landmark: win — or lose — a game of CSE.**

### Phase 3 — The living army

Supply traced through the hex grid, units degrading when cut off, replacements
and repair during turn changeover, refit. Encirclement becomes lethal — the
East Front's defining mechanic. **Landmark: a pocket starves and surrenders.**

### Phase 4 — An opponent

A first AI: move toward objectives, attack at favorable odds (`simulate` can
literally power its judgment). Doesn't need to be good; needs to fight back.
**Landmark: lose to the machine.**

### Phase 5 — Proof of eras

A second-era mini-scenario (Napoleonic or WW1) built purely from TOML — no code
changes beyond, at most, new `ElementClass` variants. Flushes out every WW2
assumption that leaked into code. Cheap insurance for pillar 1; can be pulled
earlier as a spike. **Landmark: the same binary fights Borodino.**

### Phase 6 — Combined arms

Air warfare as data + mission procedures: ground support first (extra firers in
the existing battle), then air superiority, interdiction, airfields. Naval, if
it happens, follows the pattern set here. **Landmark: the first air mission
flies.**

### Phase 7 — The real interface

The egui (or Bevy-hybrid) UI from ideas.md: hex map, unit counters, panels,
battle reports — the full game playable without a terminal. Cross-platform CI
builds start here. **Landmark: a whole game played without typing a command.**

### Phase 8 — Operational depth

The systems that make campaigns breathe: fog of war/detection, weather, ground
conditions, entrenchment and fortification, leaders/command, deeper logistics,
the combat refinements from ideas.md (AP penetration, AoE fire). The WitE
comparison becomes fair here. **Landmark: a full campaign scenario —
Barbarossa '41 at division scale, start to finish.**

### Phase 9 — The pillars, in miniature

Prototype-scale versions of the pillar features: in-game battle transparency
tools, the low-randomness resolution mode, a first multiplayer mode
(faction/player separation lands here at the latest), a starter scenario that
teaches the basics. **Landmark: a stranger learns the basics without the author
in the room.**

### Phase 10 — Prototype release

Polish, a handful of scenarios, written rules complete enough to be learnable,
packaging for friends and fellow grognards. Explicitly not v1.0 — a working
game that shows the spirit and generates feedback. **Landmark: the prototype in
someone else's hands, and their feedback in the backlog.**

### Open ordering questions (Part 1)

- **Supply (3) vs AI (4):** supply-first because it changes the data model and
  the AI doesn't — but an opponent makes supply testing far more interesting.
  Swap if motivation demands it.
- **Era proof (5):** could run right after Phase 2 as a spike, before the
  codebase grows; catching era-leaks is cheaper early.
- **UI (7):** deliberately after the game is playable end-to-end in the
  terminal, per ideas.md — but nothing stops the debug visualiser accreting
  features the whole time.

## Part 2 — from prototype to the game

The distance between the prototype and a finished product, in feature areas.
**Non-exhaustive by design** — this list grows as playing the prototype teaches
us what the game needs — and deliberately unordered.

- **Industry and production** — factories, resources, production queues,
  factory evacuation; equipment pools fed by a modeled economy instead of
  scenario scripts.
- **Faction state as a whole** — the faction as a modeled entity: manpower,
  national will, politics, doctrine (see ideas.md), theater-level decisions.
- **Detailed logistics** — the prototype supply system is a sketch; the real
  one has WitE-style depth: depots, rail conversion and capacity, truck/horse
  pools, ports.
- **Leaders, staffs and command** — command chains, leader checks, staff
  quality feeding recovery and logistics, doctrine drift (see ideas.md).
- **Combat engine refinement and rework** — prototype play will expose what the
  battle model gets wrong; expect at least one deep rework (AP penetration,
  AoE fire, possibly the 2D tactical battlefield experiment from ideas.md).
- **Scenario building at scale** — historical OOB and unit research is a huge
  topic in its own right; real campaign scenarios are archival projects and
  need editor tooling beyond a text editor.
- **Balance campaigns** — systematic tuning passes with real players over real
  campaigns, not just `simulate` runs.
- **Rules formalized into a manual** — the design docs graduate into a
  player-facing manual: reference-grade, versioned together with the rules it
  describes.
- **AI and multiplayer maturity** — from "fights back" to "plays well"; from a
  working multiplayer mode to one people actually use.
- **More eras and domains** — era packs as real content, naval warfare if it
  earns its place.
