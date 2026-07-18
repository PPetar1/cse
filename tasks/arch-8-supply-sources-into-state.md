# Move supply_sources into State's resolution pipeline

Status: pending

## Goal

`supply_sources` lives on `Game` as a bare `Vec<SupplySource>`, carried
straight over from the parsed `Scenario` — the only piece of
scenario-derived map content that stops one layer short of `State`'s
resolution pipeline. `units`/`toe`/`elements`/`leaders`/
`starting_strength` are all resolved into `State` via `build_state`
(`game/scenario.rs`); `supply_sources` should follow the same pattern.
This matters ahead of the roadmap's Stage 2 pool (depots, industry,
forts, ports, AA, and supply sources themselves eventually becoming
buildable depots) adding more hex-scoped content that should land in the
same place from the start, rather than repeating today's inconsistency.

`victory_conditions.hexes` is explicitly out of scope for this task —
leave it where it is.

## Mechanics

- Move `supply_sources: Vec<SupplySource>` from the `Game` struct
  (`game/mod.rs`) into `State` (`src/core/mod.rs`).
- Resolve it inside `build_state` (`game/scenario.rs`), the same way
  `units`/`toe`/`elements`/`leaders` are resolved today.
- Update every `self.supply_sources` reference (`game/supply.rs` and
  anywhere else) to `self.state.supply_sources`.
- `scenario::validate_supply_sources` currently runs after `build_state`,
  against the already-built `state`. Check whether it still needs to run
  at that point or should move, now that `supply_sources` is resolved as
  part of `state`'s own construction — don't assume, verify against how
  the equivalent unit/toe/element validation is sequenced.
- `Game`'s `serde` round-trip (`.sav` files, postcard binary) must keep
  working. Moving a field into a different struct can change a
  postcard-serialized layout; if this changes the save format in a way
  that breaks loading existing `.sav` files, stop and ask rather than
  silently accepting the break.

## Acceptance criteria

- `SupplySource` data lives on `State`, resolved during `build_state`,
  matching the units/toe/elements/leaders pattern.
- `game/supply.rs` and any other consumer read it via
  `self.state.supply_sources`. (This stays internal to `game/`, so it's
  not a sealed-state violation — `game/` submodules already use
  `self.state` freely.)
- `cargo test` / `cargo clippy --all-targets` clean.
- `docs/architecture.md`'s per-system Supply notes updated to reflect
  where the data now lives.

## Implementation notes

- `victory_conditions.hexes` is a known, separate case of the same
  "scenario-derived data not fully resolved into State" pattern — left
  alone here per the author's explicit call; a future task if it becomes
  worth doing.

## Open questions

None currently — flag here if `validate_supply_sources` sequencing or
the save-format check surfaces something that needs a design call.
