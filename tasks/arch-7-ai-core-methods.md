# Stop ai.rs calling core:: methods directly

Status: pending

## Goal

`ai.rs` calls `core::location::Location` methods directly
(`neighbour_coords()`, `distance_to()`) and duplicates a rule `Game`
already exposes (`controls()` reimplements `hex_controlled_by`). Sealing
`Game.state` (arch-4) stopped the interface layer from reading state
directly, but it didn't stop it from calling behavior on the `core::`
types a `Game` query hands back — a future `core::` change (e.g. a new
location kind) would force changes in `ai.rs` even though `ai.rs` never
touches `game.state`. This session settled the general rule with the
author: reading a `core::` type's data (fields, enum variants) from
outside `game/` is fine, since a shape change surfaces as a compiler
error at the exact spot needing an update; *calling* a `core::` method
from outside `game/` is not, since that's behavior that belongs behind a
`Game` query. `ai.rs`'s two geometry calls and its duplicate `controls()`
are the concrete case; fix them and write the rule down.

## Mechanics

- Add `Game::adjacent(&self, x: u32, y: u32) -> Vec<(u32, u32)>` —
  looks up the hex via `self.state.map`, returns its neighbour
  coordinates (empty if the hex doesn't exist or is offmap). Delegates
  to `Location::neighbour_coords()` internally; the caller never sees a
  `Location`.
- Add `Game::distance(&self, a: (u32, u32), b: (u32, u32)) -> Option<u32>`
  — looks up both hexes, returns `Location::distance_to()`'s result (or
  `None` if either hex doesn't exist). Same delegate pattern as the
  existing `hex_controlled_by` in `game/victory.rs`.
- In `ai.rs`:
  - `best_attack`: replace `from_location.neighbour_coords()` with
    `game.adjacent(from.0, from.1)`.
  - `priority_target`: replace both `from_location.distance_to(location)`
    calls with `game.distance(from, (x, y))` (adjust the two call sites'
    coordinate pairs accordingly).
  - `move_toward`: replace `from_location.neighbour_coords()` with
    `game.adjacent(from.0, from.1)`, and the `distance_to()` call inside
    the sort with `game.distance((x, y), target)`. **Preserve the
    existing early-return if `target` isn't a valid hex** — today's code
    bails out of the whole function via `game.location(target...)?`
    before trying any neighbours; keep that behavior (e.g. an explicit
    `game.location(target.0, target.1)?;` validity check up front, or
    equivalent) rather than letting an invalid target silently fall
    through to "try every neighbour in arbitrary order."
  - Delete `controls()` entirely; its two call sites become
    `game.hex_controlled_by(faction, x, y)`.
  - The `UnitLocation::OnMap`/`Offmap` matches in `faction_stacks` and
    `priority_target` stay as they are — they read data, they don't call
    a method, so they're fine under the confirmed rule.
- Update `ai.rs`'s tests (the two that call `.distance_to()` directly to
  compute a baseline distance) to use `game.distance(...)` instead, for
  consistency with the rule the production code now follows.
- Document the rule in `docs/architecture.md`'s Conventions section,
  next to the existing "Sealed state" bullet: `core::` types may be read
  (fields, enum variants) from anywhere; `core::` methods/functions may
  only be called from inside `game/` (or `procedures/`, which is exempt
  as pure algorithm code `game/` calls into — not part of the interface
  layer); `Game` queries return narrowly-scoped results, never the whole
  `State` or an unfiltered internal collection. Add `adjacent`/`distance`
  to the list of query methods already named in that bullet.

## Acceptance criteria

- `ai.rs` production code contains no direct calls to any `core::`
  method (`neighbour_coords`, `distance_to`, or otherwise) — only `Game`
  calls remain.
- `controls()` is gone from `ai.rs`; its callers use
  `Game::hex_controlled_by`.
- AI behavior is unchanged: existing `ai.rs` tests pass (adjusted only
  to call `Game::distance` where they previously called
  `Location::distance_to` for a baseline), including the target-invalid
  early return in `move_toward`.
- `cargo test` / `cargo clippy --all-targets` clean.
- `docs/architecture.md`'s Conventions section documents the three-clause
  `core::` boundary rule and lists `adjacent`/`distance` among `Game`'s
  query methods.

## Implementation notes

- `Game::adjacent`/`Game::distance` fit naturally in `game/mod.rs` next
  to the other query methods (`location`, `units_at_location`, etc.); no
  new module needed.
- `gui/map_view.rs` and `gui/inspector.rs` are unaffected — they already
  only read `core::` type data (`Terrain`, `UnitLocation` fields), never
  call a `core::` method, so they're already compliant with the
  confirmed rule and don't need touching in this task.

## Open questions

None — scope and the rule's wording were confirmed with the author
before this task was written.
