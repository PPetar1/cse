# CSE — Combat Simulation Engine

A combat simulation engine/wargame written in Rust, inspired by Gary Grigsby's War in the East series. WW2 is the first target; the long-term goal is an engine generic enough to model fires-based conflicts of any era (Napoleonic through modern), eventually including air — and possibly naval — warfare.

Side project, worked on in free time.

## ⚠️ AI-generated code disclaimer

This project doubles as a learning exercise in AI-driven development: the large majority of the code is written by AI (Claude Code) under human direction. Most of it has not been reviewed line-by-line. Treat the code accordingly — don't assume it reflects best practices or that every path has been exercised, and review before reusing any of it in your own projects.

## What's in the game

The engine simulates battles between units on a hex map. Unit attributes (elements, TOEs) and maps are read from easy-to-edit TOML config files (`scenarios/*.scen`, `maps/*.map`), so scenarios are data, not code. The prototype — stage 1 of the five development stages in `docs/roadmap.md` — is complete:

- **Combat** — fires-based resolution at the weapon-device level: battles run in rounds at closing range bands, every in-range weapon of every committed element fires, and hits damage or destroy elements. Beaten defenders retreat with attrition, rout, shatter, or surrender if surrounded; the attackers advance into the vacated hex. A `simulate` command fights any attack repeatedly without touching the game, for balance tuning.
- **Morale and experience** — set in the scenario at any granularity (element, unit, or faction default); both scale combat value, experience gates whether an element commits each round, morale gates routs. Battles shift both, and morale drifts back toward the faction default between turns.
- **Turns and movement** — IGO-UGO turns advancing a real in-game date; per-TOE movement point budgets, terrain entry costs, cheapest-path movement around enemy-held and impassable hexes.
- **Scenario machinery** — victory conditions (objective hexes plus points for enemy strength destroyed and lost, scored at a final turn), scheduled reinforcements and withdrawals, and scripted events that fire messages and morale/experience nudges on a given turn.
- **Supply and refit** — every unit traces connectivity back to its faction's supply sources around enemy hexes and impassable terrain; supplied units repair damaged elements and receive replacements each turn, cut-off units get neither.
- **Air war** — air units fly ground support into ongoing battles as extra firers, declare interdiction coverage over hexes (covering fighters automatically join any battle fought there), fight under domain-restricted targeting (fighters only engage aircraft; ground elements can't hit aircraft unless flagged anti-air), and operate within their TOE's mission range of wherever they're based.
- **AI opponent** — a scenario faction can be marked AI-controlled: it attacks adjacent enemies when simulated odds look favorable, otherwise advances on objectives, playing its turns automatically after a human ends theirs.
- **Fog of war and entrenchment** — an optional per-scenario detection range hides enemy units none of your units are close enough to spot; units that hold still dig in turn by turn for a defensive bonus, reset the moment they relocate.
- **Two interfaces, one game** — `cargo run` opens a GUI window (egui/eframe) and a terminal at the same time, both driving the same session; every terminal command has a clickable equivalent. Save/load to compact binary files.

Two scenarios ship: `scenarios/basic_scenario.scen` (a small dev/test sandbox) and `scenarios/frontline_sector.scen` (a division-scale slice of the front — a continuous 10-hex Soviet line under a German push, mixed infantry and Panzer divisions on the Axis side, reinforcements, a withdrawal, scenario events and victory conditions all exercised together).

Docs live in `docs/`: `manual.md` (how every system behaves), `architecture.md` (the technical reference), `roadmap.md` (the five development stages), `ideas.md` (the idea backlog).

## Building and running

```
cargo build   # incremental builds are fast
cargo run     # opens the GUI (main menu first) with a terminal alongside it
cargo test    # run the test suite
```

## Commands

`cargo run` opens the GUI window and a terminal at the same time, sharing one game session — a command in the terminal is reflected in the window and vice versa. The window opens on a main menu (a scenario-path field and "New Game", a save-path field and "Load Game", and "Quit" — each path field also has a "Browse…" button that opens a small directory listing instead of requiring the path to be typed); once a game is active it shows the map (scroll to zoom, drag to pan; objective hexes carry a small flag marker, and multiple units on one hex offset sideways instead of overlapping), a header with an End Turn button, Save/Load/New/Quit buttons (each Save/Load/New opens a small popup asking for a path, with the same Browse… option), and a Reports row (Victory/Reinforcements/Events/Supply/Interdiction, each logging that summary), plus a click-to-inspect side panel with Move/Attack buttons and an "Air operations" block (pick a unit from the dropdown, then Air Support or Interdict). The terminal supports every command below, plus tab completion (command names, and file paths for `new`/`load`/`save`) and arrow-key history; typing `exit` (or Ctrl-C/Ctrl-D) there closes the whole program, same as closing the window.

| Command | Description |
|---|---|
| `new <path.scen>` | start a new game from a scenario file, e.g. `new scenarios/basic_scenario.scen` or `new scenarios/frontline_sector.scen` |
| `inspect <x> <y>` | inspect the location at (x, y): terrain plus each unit there with its TOE, entrenchment level, and per-element ready/damaged counts and morale/experience. If the scenario has `[fog_of_war]` on and the hex is outside the on-turn faction's detection range, prints "Unknown" instead |
| `inspect <name>` | inspect the offmap location with the given name |
| `units` | list units visible to the faction on turn — your own always, enemy ones only within detection range if `[fog_of_war]` is on (`units detail` for more detail) |
| `move <x1> <y1> <x2> <y2> <unit_index>` | move the unit with the given index to any reachable destination: the engine finds the cheapest path (terrain entry costs summed, impassable and enemy-occupied hexes routed around) and charges it against the unit's movement points (refilled each turn from the TOE's `mp`); only the player on turn can move their units; stacked units are indexed in alphabetical order, matching the order `inspect` lists them |
| `attack <x1> <y1> <x2> <y2>` | all units at the first hex attack all units at the second (adjacent) hex; only the faction on turn can attack. Prints a battle report (rounds at closing range, losses, final CV odds, outcome). Losses and experience gain persist; beaten defenders retreat to an adjacent free hex with extra attrition — routing if morale breaks, shattering if also badly depleted — or surrender if surrounded; the attackers then advance into the vacated hex for free. See `docs/manual.md` |
| `air_support <x1> <y1> <x2> <y2> <unit name>` | fly one owned unit's elements into that attack as extra firers, for that battle only — same report as `attack`, but the unit never advances into a vacated hex and stays at its base regardless of outcome (it must belong to the attacking faction and not already be part of the ground stack, and within its TOE's mission range, if any, of an on-map unit's current hex). If the defending faction has fighters covering the target hex (`interdict`), they automatically join the fight |
| `interdict <x> <y> <unit name>` | a fighter-capable unit declares coverage of that hex (up to 3 hexes per unit at a time, and within its TOE's mission range if it's based on the map); if the enemy later attacks that hex — `attack` or `air_support` — the covering unit automatically joins as an extra defender. Coverage lasts through the opponent's next turn and must be redeclared after that |
| `interdiction` | list every unit currently covering hexes, and which ones |
| `simulate <x1> <y1> <x2> <y2> <n>` | fight that attack n times without changing the game and print statistics: hold/retreat rates, average losses, mean final CVs. For balance tuning; only legal attacks (adjacent, on turn) can be simulated |
| `end_turn` | pass control to the next player; once every player has moved, the turn counter and the in-game date advance. The faction coming on turn refills its movement points, its element morale drifts one step toward the faction default, any of its units scheduled to arrive or leave this turn do so, any of its scenario events scheduled for this turn fire (printing their message and nudging its morale/experience default before the drift above uses it), and every one of its on-map units still connected to supply repairs some damaged elements and receives some replacements for destroyed ones (units cut off from supply get neither). If the scenario's `[victory_conditions] last_turn` has just been completed, prints the final score for every faction and the winner (or a draw). Any faction marked `controller = "Ai"` then plays its own turn automatically (and the next, and the next…) until control reaches a human player or the scenario ends, printing a report of what it did |
| `status` | show the scenario name, turn number, date and whose move it is |
| `victory` | show this scenario's victory conditions: the last turn, each objective hex with points and its current holder, and the enemy-destruction/own-loss point multipliers |
| `reinforcements` | list every scheduled reinforcement/withdrawal (turn, unit, destination) and whether it has arrived yet |
| `events` | list every scheduled scenario event (turn, faction, message, morale/experience deltas) and whether it has fired yet |
| `supply` | list every on-map unit as supplied or cut off, based on whether it can currently trace a path back to one of its faction's `[[supply_sources]]` without crossing enemy-held hexes or impassable terrain |
| `help` | list all commands |
| `save <path>` | save the game state to a file |
| `load <path>` | load a game state from a file |
| `exit` | quit |
