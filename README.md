# CSE — Combat Simulation Engine

A combat simulation engine/wargame written in Rust, inspired by Gary Grigsby's War in the East series. WW2 is the first target; the long-term goal is an engine generic enough to model fires-based conflicts of any era (Napoleonic through modern), eventually including air — and possibly naval — warfare.

Side project, worked on in free time.

## ⚠️ AI-generated code disclaimer

This project doubles as a learning exercise in AI-driven development: the large majority of the code is written by AI (Claude Code) under human direction. Most of it has not been reviewed line-by-line. Treat the code accordingly — don't assume it reflects best practices or that every path has been exercised, and review before reusing any of it in your own projects.

## What's in the game

The engine simulates battles between units on a hex map. Unit attributes (elements, TOEs) and maps are easy-to-edit TOML (`scenarios/*.scen`, `maps/*.map`), so scenarios are data, not code. The prototype — stage 1 of the five development stages in `docs/roadmap.md` — is complete:

- **Combat** — fires-based resolution at the weapon-device level: rounds at closing range bands, every in-range weapon of every committed element fires, hits damage or destroy elements. Beaten defenders retreat with attrition, rout, shatter, or surrender if surrounded; the attackers advance. A `simulate` command fights any attack repeatedly without touching the game, for balance tuning.
- **Morale and experience** — set in the scenario at any granularity (element, unit, or faction default); both scale combat value, experience gates commitment, morale gates routs. Battles shift both; morale drifts back toward the faction default between turns.
- **Turns and movement** — IGO-UGO turns advancing a real in-game date; per-TOE movement point budgets, terrain entry costs, cheapest-path movement around enemy-held and impassable hexes.
- **Scenario machinery** — victory conditions (objective hexes plus points for enemy strength destroyed and lost, scored at a final turn), scheduled reinforcements and withdrawals, scripted events with morale/experience nudges.
- **Supply and refit** — units trace connectivity to their faction's supply sources; supplied units repair damaged elements and receive replacements each turn, cut-off units get neither.
- **Air war** — air units fly ground support into ongoing battles, declare interdiction coverage over hexes (covering fighters join any battle there), fight under domain-restricted targeting (fighters only engage aircraft; ground elements can't hit aircraft unless flagged anti-air), and operate within their TOE's mission range of their base.
- **AI opponent** — a scenario faction can be AI-controlled: it attacks when simulated odds look favorable, otherwise advances on objectives, playing its turns automatically.
- **Fog of war and entrenchment** — an optional per-scenario detection range hides enemy units none of your units can spot; units that hold still dig in for a defensive bonus, reset the moment they relocate.
- **Two interfaces, one game** — `cargo run` opens a GUI window (egui/eframe) and a terminal at the same time, both driving the same session; every terminal command has a clickable equivalent. Save/load to compact binary files.

Two scenarios ship: `scenarios/basic_scenario.scen` (a small dev/test sandbox) and `scenarios/frontline_sector.scen` (a division-scale slice of the front — a continuous 10-hex Soviet line under a German push, with reinforcements, a withdrawal, events and victory conditions all exercised).

Docs live in `docs/`: `manual.md` (how every system behaves), `architecture.md` (the technical reference), `roadmap.md` (the five development stages), `ideas.md` (the idea backlog).

## Building and running

```
cargo build   # incremental builds are fast
cargo run     # opens the GUI (main menu first) with a terminal alongside it
cargo test    # run the test suite
```

## Commands

`cargo run` opens the GUI window and a terminal at once, sharing one game session — a command in either is immediately reflected in the other. The window opens on a main menu (scenario path + "New Game", save path + "Load Game", "Quit"; every path field has a "Browse…" button opening a small directory listing). In game it shows the map (scroll to zoom, drag to pan; objective hexes carry flag markers, stacked units offset sideways), a header with End Turn and Save/Load/New/Quit (a path popup where needed), a Reports row (Victory/Reinforcements/Events/Supply/Interdiction summaries), and a click-to-inspect side panel with Move/Attack buttons plus an "Air operations" block (pick a unit, then Air Support or Interdict). The terminal supports every command below, with tab completion and arrow-key history; `exit` (or Ctrl-C/Ctrl-D) closes the whole program, same as closing the window.

System behavior behind these commands — battle resolution, retreats, supply, fog of war and the rest — is documented in `docs/manual.md`.

| Command | Description |
|---|---|
| `new <path.scen>` | start a new game from a scenario file, e.g. `new scenarios/basic_scenario.scen` |
| `inspect <x> <y>` | show the hex's terrain and each unit there with its TOE, leader, entrenchment level, and per-element ready/damaged counts and morale/experience; prints "Unknown" if fog of war hides the hex |
| `inspect <name>` | inspect the offmap location with the given name |
| `units` | list units visible to the faction on turn (`units detail` for full rosters) |
| `move <x1> <y1> <x2> <y2> <unit_index>` | move a unit to any reachable destination; the cheapest path's cost is charged against its movement points. Stacked units are indexed in alphabetical order, matching `inspect`'s listing |
| `attack <x1> <y1> <x2> <y2>` | all units at the first hex attack all units at the second (adjacent) hex; prints a battle report. Losses and experience persist; beaten defenders retreat/rout/shatter/surrender and the attackers advance |
| `air_support <x1> <y1> <x2> <y2> <unit name>` | fly one owned unit's elements into that attack as extra firers, for that battle only; the unit stays at its base and never advances. Covering enemy fighters (`interdict`) join automatically |
| `interdict <x> <y> <unit name>` | a fighter-capable unit declares coverage of that hex (up to 3 per unit, within mission range); it automatically joins any battle there as an extra defender, through the opponent's next turn |
| `interdiction` | list every unit currently covering hexes, and which ones |
| `simulate <x1> <y1> <x2> <y2> <n>` | fight that attack n times without changing the game; prints hold/retreat rates, average losses, mean final CVs — the balance-tuning tool. Only legal attacks (adjacent, on turn) can be simulated |
| `end_turn` | pass control to the next player; when everyone has moved, the turn and date advance. The faction coming on turn gets its turn-start sequence (arrivals, events, refit, entrenchment, MP refill, morale drift). Completing the scenario's last turn prints the victory report; AI factions then play automatically until a human is on turn |
| `status` | show the scenario name, turn number, date and whose move it is |
| `victory` | show the victory conditions and each objective hex's current holder |
| `reinforcements` | list every scheduled reinforcement/withdrawal and whether it has arrived |
| `events` | list every scheduled scenario event and whether it has fired |
| `supply` | list every on-map unit as supplied or cut off |
| `leaders <faction>` | list that faction's leaders and which unit (if any) each commands |
| `leader <name>` | show one leader's stats and current assignment |
| `reassign_leader <unit name>` | assign a leader to that unit; prompts for the leader's name (tab-completed) on a second line |
| `help` | list all commands |
| `save <path>` | save the game state to a file |
| `load <path>` | load a game state from a file |
| `exit` | quit |
