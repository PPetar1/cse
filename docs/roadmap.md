# CSE roadmap — from engine to finished game

This is the long-term compass: it ends at a **shipped, finished game that is a
real improvement over Gary Grigsby's *War in the East***, not a tech demo or a
clone with fewer features. Timescale is deliberately ignored — years are fine.
Phases are ordered but abstract; when one begins, it gets broken down into
concrete work in CLAUDE.md ("Current focus") and the design docs
(`combat_design.md`, `ideas.md`). When direction changes, change this file.

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
3. **Approachability.** WitE buries a great game under a hostile UI and a
   400-page manual as the only teacher. CSE ramps players in: small scenarios
   first, a clean UI, mechanics explained where they act.
4. **Moddability.** Everything is data: scenarios, maps, TOEs, elements,
   devices are TOML a motivated player can edit. The scenario editor is a text
   editor on day one; validation (`State::build`) is the safety net.
5. **Modern multiplayer.** Beyond hot-seat/PBEM — a server-mediated or
   simultaneous-turn mode designed in once the turn structure is proven, with
   multiple players per faction (see ideas.md, faction/player separation).

## Phases

Each phase ends at a **landmark** — a moment the project is visibly more of a
game. Cross-cutting work (combat knob retuning, AP penetration, AoE fire,
refactors) threads through whichever phase strains it first.

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

### Phase 9 — Beyond WitE

The pillars become features: in-game battle transparency tools, the
low-randomness resolution mode, multiplayer (faction/player separation lands
here at the latest), onboarding scenarios and in-game teaching, modding docs.
**Landmark: a stranger learns the game without the author in the room.**

### Phase 10 — Ship it

Polish, a scenario library spanning at least two eras, a manual that is a
reference rather than a requirement, packaging and distribution (itch/Steam),
support cadence. **Landmark: v1.0 in someone else's hands.**

## Open ordering questions

- **Supply (3) vs AI (4):** supply-first because it changes the data model and
  the AI doesn't — but an opponent makes supply testing far more interesting.
  Swap if motivation demands it.
- **Era proof (5):** could run right after Phase 2 as a spike, before the
  codebase grows; catching era-leaks is cheaper early.
- **UI (7):** deliberately after the game is playable end-to-end in the
  terminal, per ideas.md — but nothing stops the debug visualiser accreting
  features the whole time.
