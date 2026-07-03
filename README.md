# CSE — Combat Simulation Engine

A combat simulation engine/wargame written in Rust, inspired by Gary Grigsby's War in the East series. WW2 is the first target; the long-term goal is an engine generic enough to model fires-based conflicts of any era (Napoleonic through modern), eventually including air — and possibly naval — warfare.

Side project, worked on in free time.

## ⚠️ AI-generated code disclaimer

This project doubles as a learning exercise in AI-driven development: the large majority of the code is written by AI (Claude Code) under human direction. Most of it has not been reviewed line-by-line. Treat the code accordingly — don't assume it reflects best practices or that every path has been exercised, and review before reusing any of it in your own projects.

The engine runs in the terminal and simulates battles between units on a hex map. Unit attributes (elements, TOEs) and maps are read from easy-to-edit TOML config files (`scenarios/*.scen`, `maps/*.map`), so scenarios are data, not code. A simple Bevy-based map visualiser is included for debugging.

Currently implemented: scenario/map loading, unit inspection, simple movement, save/load, map visualiser, and a fires-based combat resolution (`attack`) with retreats/routs/shatters, morale/experience (shifting with battle outcomes: everyone gains experience, winners rally and losers sag), device-level weapons (accuracy, rate of fire, soft/hard attack) and a `simulate` tuning tool. Next up: the turn/phase system.

Design notes live in `docs/` (`combat_design.md` for the battle model, `ideas.md` for the idea backlog).

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
| `move <x1> <y1> <x2> <y2> <unit_index>` | move the unit with the given index from the start hex to the destination hex; stacked units are indexed in alphabetical order, matching the order `inspect` lists them |
| `attack <x1> <y1> <x2> <y2>` | all units at the first hex attack all units at the second hex; prints a battle report (rounds at closing range, losses, final CV odds, outcome). Losses and experience gain persist; beaten defenders retreat to an adjacent free hex with extra attrition — routing if morale breaks, shattering if also badly depleted — or surrender if surrounded. See `docs/combat_design.md` |
| `simulate <x1> <y1> <x2> <y2> <n>` | fight that attack n times without changing the game and print statistics: hold/retreat rates, average losses, mean final CVs. For balance tuning |
| `view` | open a window visualising the map and unit positions; the terminal stays usable while the window is open, and `view` can be called again after closing it (Esc or close the window to dismiss) |
| `help` | list all commands |
| `save <path>` | save the game state to a file |
| `load <path>` | load a game state from a file |
| `exit` | quit |
