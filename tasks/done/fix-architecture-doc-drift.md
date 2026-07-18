# Fix architecture.md drift found by arch-9 (plus one stale attribute)

Status: pending

## Goal

Findings F6-F8 of `tasks/arch-9-findings.md`: three places where
`docs/architecture.md` no longer matches the code, one of which also
leaves a stale `#[allow(dead_code)]` in the source. Docs-accuracy pass;
no behavior changes.

## Mechanics

1. **F6** — in the file map's `core/mod.rs` entry
   (architecture.md:155-158), add `terrain_costs` to the listed `State`
   fields (it sits between `map` and `units` in `core/mod.rs:17`).
2. **F7** — in the file map's `core/unit.rs` entry
   (architecture.md:165-166), drop the trailing "+ config structs": all
   scenario config structs (`UnitConfig`, `ElementStatsConfig`,
   `UnitLocationConfig`, `ScheduledArrivalConfig`) live in
   `game/scenario.rs`, per the schema-in-scenario.rs convention.
3. **F8** — in the "Dead config fields" convention
   (architecture.md:223-227), remove `VictoryHex.name` from the list: it
   is read by `victory_conditions_summary` (`game/victory.rs:90`) and
   `victory_hexes` (`game/victory.rs:113`). Accordingly remove the now
   unnecessary `#[allow(dead_code)]` from `VictoryHex.name` in
   `game/scenario.rs:452-454` — the convention itself mandates removing
   the attribute once the field is used. `Scenario.game_version` and
   `MapFile.width/height` stay listed; they are still unread.

## Acceptance criteria

- The three architecture.md corrections applied, wording consistent with
  the surrounding entries.
- `#[allow(dead_code)]` gone from `VictoryHex.name`; `cargo build`,
  `cargo test`, and `cargo clippy --all-targets` stay clean (the field is
  read, so no dead-code warning may appear).
- No other content changes to architecture.md.

## Implementation notes

Line numbers reference the state of the tree at commit d67cd83; re-locate
by content if the file has shifted.

## Open questions

None.
