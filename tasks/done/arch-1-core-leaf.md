# Make core/ a true leaf: move State assembly into game/scenario.rs

Status: done

## Goal

`core/mod.rs` imports `crate::game::{Player, Scenario}` because
`State::build(scenario)` lives there — so `core` and `game` depend on each
other and `core` is not the independent foundation its name claims. Break
the cycle so the dependency chain runs strictly downward
(`game` → `core`, never back).

## Mechanics

- Move the body of `State::build` (core/mod.rs) into `game/scenario.rs` as
  a free function `build_state(scenario: Scenario) -> Result<State, Error>`,
  next to the schema and the other `validate_*` functions it belongs with.
  The map-file reading (`File::open` on `scenario.map`) moves with it, so
  `core` ends up with no file I/O at all (`Map::map_from_string` stays in
  core — it parses a string, no I/O).
- `Game::parse_scen_from_toml` calls `scenario::build_state(scenario)`
  instead of `State::build(scenario)`.
- All validation behavior and error messages stay exactly as they are;
  this is a move, not a rewrite.
- Tests covering the assembly/validation move from `core/mod.rs` to
  `game/scenario.rs` with it. Tests of core data types stay in core.

## Acceptance criteria

- No `use crate::game` (or any `game::` path) anywhere under `src/core/`.
- No `std::fs`/`std::io` use anywhere under `src/core/`.
- `cargo test` and `cargo clippy --all-targets` clean; the shipped-scenario
  build tests (`builds_the_real_basic_scenario`,
  `builds_the_real_frontline_sector_scenario`) still pass unchanged.
- `docs/architecture.md` file map updated in the same pass.

## Implementation notes

- `State` itself (the struct) stays in `core/mod.rs` — only the
  constructor logic moves. `build_state` can construct it directly since
  `State`'s fields are currently public; task arch-4 revisits visibility.
- `core/unit.rs:16` mentions `game::entrenchment::MAX_FORT_LEVEL` in a doc
  comment only — reword it so core's docs don't reference game either.
