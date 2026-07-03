# CSE — Combat Simulation Engine

A combat simulation engine/wargame written in Rust, inspired by Gary Grigsby's War in the East series. WW2 is the first target; the long-term goal is an engine generic enough to model fires-based conflicts of any era (Napoleonic through modern), eventually including air — and possibly naval — warfare.

Side project, worked on in free time.

## ⚠️ AI-generated code disclaimer

This project doubles as a learning exercise in AI-driven development: the large majority of the code is written by AI (Claude Code) under human direction. Most of it has not been reviewed line-by-line. Treat the code accordingly — don't assume it reflects best practices or that every path has been exercised, and review before reusing any of it in your own projects.

The engine runs in the terminal and simulates battles between units on a hex map. Unit attributes (elements, TOEs) and maps are read from easy-to-edit TOML config files (`scenarios/*.scen`, `maps/*.map`), so scenarios are data, not code. A simple Bevy-based map visualiser is included for debugging.

Currently implemented: scenario/map loading, unit inspection, simple movement, save/load, map visualiser, a fires-based combat resolution (`attack`) with retreats/routs/shatters, morale/experience (shifting with battle outcomes: everyone gains experience, winners rally and losers sag), device-level weapons (accuracy, rate of fire, soft/hard attack), a `simulate` tuning tool, the turn system (`end_turn` passes control between players IGO-UGO and advances the turn counter and in-game date, with MP budgets, terrain costs and turn-start morale recovery), and victory conditions: a scenario's optional `[victory_conditions]` table scores every faction at a fixed final turn — flat points for holding named objective hexes, points for enemy strength destroyed, a penalty for strength lost — and `end_turn` prints the result once the last turn completes; scheduled reinforcements and withdrawals move a scenario's units on or off the map (typically to/from an offmap reserve box) the moment their faction's turn reaches the scheduled turn number, even the very first turn; and scenario events fire the same way, printing a message and optionally nudging a faction's default morale/experience. Phase 2 (the first winnable scenario) is now feature-complete; next up is Phase 3 (supply and the living army).

Design notes live in `docs/` (`roadmap.md` for the long-term plan, `combat_design.md` for the battle model, `ideas.md` for the idea backlog).

## Building and running

```
cargo build   # first build is slow due to Bevy; incremental builds are fast
cargo run
cargo test    # run the test suite
```

## Commands

The prompt supports tab completion (command names, and file paths for `new`/`load`/`save`) and arrow-key history.

| Command | Description |
|---|---|
| `new <path.scen>` | start a new game from a scenario file, e.g. `new scenarios/basic_scenario.scen` |
| `inspect <x> <y>` | inspect the location at (x, y): terrain plus each unit there with its TOE and per-element ready/damaged counts and morale/experience |
| `inspect <name>` | inspect the offmap location with the given name |
| `units` | list all units (`units detail` for more detail) |
| `move <x1> <y1> <x2> <y2> <unit_index>` | move the unit with the given index to any reachable destination: the engine finds the cheapest path (terrain entry costs summed, impassable and enemy-occupied hexes routed around) and charges it against the unit's movement points (refilled each turn from the TOE's `mp`); only the player on turn can move their units; stacked units are indexed in alphabetical order, matching the order `inspect` lists them |
| `attack <x1> <y1> <x2> <y2>` | all units at the first hex attack all units at the second (adjacent) hex; only the faction on turn can attack. Prints a battle report (rounds at closing range, losses, final CV odds, outcome). Losses and experience gain persist; beaten defenders retreat to an adjacent free hex with extra attrition — routing if morale breaks, shattering if also badly depleted — or surrender if surrounded; the attackers then advance into the vacated hex for free. See `docs/combat_design.md` |
| `simulate <x1> <y1> <x2> <y2> <n>` | fight that attack n times without changing the game and print statistics: hold/retreat rates, average losses, mean final CVs. For balance tuning; only legal attacks (adjacent, on turn) can be simulated |
| `end_turn` | pass control to the next player; once every player has moved, the turn counter and the in-game date advance. The faction coming on turn refills its movement points, its element morale drifts one step toward the faction default, any of its units scheduled to arrive or leave this turn do so, and any of its scenario events scheduled for this turn fire (printing their message and nudging its morale/experience default before the drift above uses it). If the scenario's `[victory_conditions] last_turn` has just been completed, prints the final score for every faction and the winner (or a draw) |
| `status` | show the scenario name, turn number, date and whose move it is |
| `victory` | show this scenario's victory conditions: the last turn, each objective hex with points and its current holder, and the enemy-destruction/own-loss point multipliers |
| `reinforcements` | list every scheduled reinforcement/withdrawal (turn, unit, destination) and whether it has arrived yet |
| `events` | list every scheduled scenario event (turn, faction, message, morale/experience deltas) and whether it has fired yet |
| `view` | open a window visualising the map and unit positions, with a flag and its point value on each objective hex; it follows the game automatically as commands change the state (moves, retreats, even loading a save); the terminal stays usable while the window is open, and `view` can be called again after closing it (Esc or close the window to dismiss); the window also closes itself when the main program exits |
| `help` | list all commands |
| `save <path>` | save the game state to a file |
| `load <path>` | load a game state from a file |
| `exit` | quit |
