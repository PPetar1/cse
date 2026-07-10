# CSE roadmap — from prototype to finished game

The destination is a **finished game that is a real improvement over Gary
Grigsby's *War in the East***. Development runs in five stages. Stage 1 —
the prototype — is done; the rest are intent, not schedule (timescale is
deliberately ignored — years are fine).

From Stage 2 on, work is authored as task files in `tasks/` (the author
writes detailed specs, the agent implements them — see CLAUDE.md,
"Development workflow"). This file records direction only: update it when
direction changes, not per task.

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
   editor on day one; validation at scenario load is the safety net.
5. **Modern multiplayer.** Beyond hot-seat/PBEM — a server-mediated or
   simultaneous-turn mode designed in once the turn structure is proven, with
   multiple players per faction (see ideas.md, faction/player separation).

## Stage 1 — the prototype (done)

Built fast, with the AI designing freely under loose direction; every system
deliberately simple and shallow. The point was structure and a playable
basis for the real work, and it delivered one. What it contains:

- **Combat core** — fires-based battle engine: rounds at closing range
  bands, device-level weapons, morale/experience with battle feedback,
  retreats/routs/shatters/surrenders, the `simulate` tuning tool.
- **Game loop** — IGO-UGO turns with real dates, MP budgets and terrain
  costs, adjacency-gated attacks, attacker advance after retreat,
  turn-start morale recovery.
- **Scenario machinery** — victory conditions (objective hexes plus
  strength destroyed/lost, scored at a final turn), scheduled
  reinforcements/withdrawals, scripted events. Two scenarios ship: a dev
  sandbox and `frontline_sector.scen`, a division-scale slice of the front.
- **Supply and refit** — connectivity traced from per-faction supply
  sources; supplied units repair and receive replacements each turn,
  cut-off units get neither.
- **AI opponent** — rule-based: attacks when simulated odds look good,
  otherwise advances on objectives; plays whole turns automatically.
- **Air war** — ground support flown into ongoing battles, domain-restricted
  air-to-air targeting, interdiction coverage, airfield range limits.
- **Interface** — an egui/eframe window and a terminal driving one shared
  game; every command has a clickable equivalent (map with zoom/pan,
  inspector, order buttons, reports, save/load with a file browser).
- **Operational depth, started** — fog of war (detection ranges) and
  entrenchment.

Deliberately *not* built, by author's call — record these so they aren't
mistaken for gaps: pocket starvation/surrender (encirclement only stalls
refit; lethal encirclement belongs with Stage 2's deeper logistics if it
returns), and the era-proofing spike (a second-era mini-scenario to flush
out WW2 assumptions — folded into Stage 4's content work, cheap insurance
for pillar 1 whenever it's motivated).

Known prototype gaps are seeded as backlog task files in `tasks/` (the AI
never uses air missions, victory scoring doesn't stop the game, the GUI's
Move order can't pick a unit within a stack, no CI, and so on) for the
author to triage into the priorities list.

## Stage 2 — systems building and polish (current)

The working mode changes here: the author does the design, the agent does
the implementation. Each system gets specified in detail as a task file —
mechanics, acceptance criteria — and rebuilt or deepened from its
prototype version. Less AI design, more AI coding.

The pool this stage draws from (unordered — `tasks/task_priorities.md`
decides order, not this list):

- The prototype systems, each revisited to its real design: combat
  (AP penetration, AoE fire, possibly a deep rework), supply and logistics
  (depots, rail conversion and capacity, truck/horse pools, ports),
  refit/replacements, air war, fog of war and detection.
- The systems the prototype never had: weather and ground conditions,
  leaders/staffs/command, industry and production (factories, resources,
  production queues, evacuation), faction-level state (manpower, national
  will, politics, doctrine), a low-randomness resolution mode, a first
  multiplayer mode (faction/player separation).
- AI improvement as the systems it plays with deepen.
- The prototype gap backlog above.

## Stage 3 — GUI

Same task-driven mode, aimed at the interface: from "every command has a
clickable equivalent" to an interface that is actually good to play.
Panels and information density, battle-report browsing, in-game
transparency tooling (pillar 2), map and counter presentation, usability
polish. Cross-platform CI builds and packaging naturally start here.

## Stage 4 — the scenario

The big one: scenario complexity is a major part of what makes the genre —
and this game — worth playing. Historical research (orders of battle,
TOEs, maps), campaign-scale scenario construction, editor tooling beyond a
text editor, systematic balance and tuning campaigns with real play, and a
starter scenario that teaches the basics. Era packs beyond WW2 are content
work of the same kind, whenever they earn their place.

## Stage 5 — testing and final polish

The design docs graduate into a reference-grade, player-facing manual
versioned with the rules it describes; systematic testing; release
packaging; feedback rounds with real players. From "works for the author"
to "works in a stranger's hands."
