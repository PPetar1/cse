# Move pathfinding into procedures/

Status: done

## Goal

`procedures/` is the home for meaty, pure, independently testable
algorithms (combat resolution, the supply flood fill) — but the third such
algorithm, A* pathfinding (`cheapest_path_cost`), sits in `core/map.rs`.
Reconcile the inconsistency so all pure algorithms live in one place and
`core` stays plain data + geometry.

## Mechanics

- New `procedures/pathfinding.rs`: move `Map::cheapest_path_cost`
  (core/map.rs:55) there as a free function taking `&Map` plus the same
  arguments it has today (start, end, the `enter_cost` closure the game
  layer uses to inject terrain costs and blocking). Semantics unchanged,
  including "start hex is free".
- Update the one production caller, `game/orders/movement.rs:52`, and the
  module doc there that names `core::map` as the algorithm's home.
- Move the pathfinding tests from `core/map.rs` alongside the function.

## Acceptance criteria

- `core/map.rs` contains no pathfinding; `procedures/pathfinding.rs` does,
  with its tests.
- Movement behaves identically (`cargo test` clean, including movement's
  MP-charging tests).
- `cargo clippy --all-targets` clean; `docs/architecture.md` file map and
  the "Layering and file map" description updated.

## Implementation notes

- The hexx `a_star` call needs `Location`'s `pub(crate) fn hex` — fine
  from `procedures/` (same crate); keep hexx contained to
  `Location`/pathfinding as today.
