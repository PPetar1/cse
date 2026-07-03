# CSE — Combat Simulation Engine

A combat simulation engine/wargame written in Rust, inspired by Gary Grigsby's War in the East series. WW2 is the first target; the long-term goal is an engine generic enough to model fires-based conflicts of any era (Napoleonic through modern), eventually including air — and possibly naval — warfare.

Side project, worked on in free time.

## ⚠️ AI-generated code disclaimer

This project doubles as a learning exercise in AI-driven development: the large majority of the code is written by AI (Claude Code) under human direction. Most of it has not been reviewed line-by-line. Treat the code accordingly — don't assume it reflects best practices or that every path has been exercised, and review before reusing any of it in your own projects.

The engine runs in the terminal and simulates battles between units on a hex map. Unit attributes (elements, TOEs) and maps are read from easy-to-edit TOML config files (`scenarios/*.scen`, `maps/*.map`), so scenarios are data, not code. A simple Bevy-based map visualiser is included for debugging.

Currently implemented: scenario/map loading, unit inspection, simple movement, save/load, map visualiser, a fires-based combat resolution (`attack`) with retreats/routs/shatters, morale/experience (shifting with battle outcomes: everyone gains experience, winners rally and losers sag), device-level weapons (accuracy, rate of fire, soft/hard attack), a `simulate` tuning tool, and the turn system: `end_turn` passes control between players (IGO-UGO) and advances the turn counter and in-game date (`start_date` + `turn_length` from the scenario), with movement rules on top — each TOE sets a movement-point budget (`mp`), moving goes hex by hex, terrain entry costs come from the scenario's `[terrain_costs]` table (0 = impassable, engine defaults fill the gaps), and budgets refill when a faction comes on turn, when element morale also drifts back toward the faction default. Next up: victory conditions and the first winnable scenario.

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
| `move <x1> <y1> <x2> <y2> <unit_index>` | move the unit with the given index one hex, to an adjacent destination; costs movement points by terrain (refilled each turn from the TOE's `mp`), Water and enemy-occupied hexes are off limits, and only the player on turn can move their units; stacked units are indexed in alphabetical order, matching the order `inspect` lists them |
| `attack <x1> <y1> <x2> <y2>` | all units at the first hex attack all units at the second (adjacent) hex; only the faction on turn can attack. Prints a battle report (rounds at closing range, losses, final CV odds, outcome). Losses and experience gain persist; beaten defenders retreat to an adjacent free hex with extra attrition — routing if morale breaks, shattering if also badly depleted — or surrender if surrounded; the attackers then advance into the vacated hex for free. See `docs/combat_design.md` |
| `simulate <x1> <y1> <x2> <y2> <n>` | fight that attack n times without changing the game and print statistics: hold/retreat rates, average losses, mean final CVs. For balance tuning; only legal attacks (adjacent, on turn) can be simulated |
| `end_turn` | pass control to the next player; once every player has moved, the turn counter and the in-game date advance. The faction coming on turn refills its movement points and its element morale drifts one step toward the faction default |
| `status` | show the scenario name, turn number, date and whose move it is |
| `view` | open a window visualising the map and unit positions; the terminal stays usable while the window is open, and `view` can be called again after closing it (Esc or close the window to dismiss) |
| `help` | list all commands |
| `save <path>` | save the game state to a file |
| `load <path>` | load a game state from a file |
| `exit` | quit |
