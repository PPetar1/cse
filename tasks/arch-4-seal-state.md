# Seal Game.state behind read queries

Status: pending

## Goal

`Game.state` is `pub` (game/mod.rs:32) and `State`'s collections are all
`pub`, so any holder of `&mut Game` can mutate internals with no invariant
enforcement, and the GUI/AI read `game.state.map` / `game.state.units`
directly instead of going through `Game`. Make `state` private: the
compiler then enforces that everything outside `game/` uses `Game`'s
methods, which becomes the one true API surface.

## Mechanics

- Add the read queries the frontends actually need, next to the existing
  ones (`units_of_faction`, `units_at_location`, `victory_hexes` model the
  style):
  - `location(x, y) -> Option<&Location>` (wrapping
    `state.map.get_location`),
  - `offmap_location(name) -> Option<&Location>`,
  - `map_locations() -> impl Iterator<Item = &Location>` (wrapping
    `state.map.all_locations`, for map rendering),
  - `unit(name) -> Option<&Unit>` (for the GUI's fort-level lookup and
    session/terminal's `unit_leader_context`).
  Exact names are the implementer's choice; keep them consistent with the
  existing query vocabulary.
- Migrate every outside reader off `game.state.*`:
  - `ai.rs` (`get_location` at ~8 sites, `units.values()` once),
  - `gui/map_view.rs` (`all_locations`, `units.values()` — replaceable
    with the existing `units_of_faction`/queries — and `units.get` for
    `fort_level`),
  - `gui/inspector.rs` (`get_location` x3),
  - `unit_leader_context` (session/terminal after arch-3, lib.rs before).
- Then remove `pub` from `Game`'s `state` field. `game/` submodules keep
  `self.state` access (field privacy is per module tree); nothing outside
  `game/` compiles if it touches it.
- `State`'s own fields may stay as they are for now: with no `&mut State`
  escaping `Game`, outsiders only ever hold `&` references obtained from
  queries.

## Acceptance criteria

- `Game`'s `state` field is not `pub`; `grep -rn "\.state\." src/gui
  src/ai.rs src/session.rs src/terminal` (paths as they exist at
  implementation time) finds nothing.
- No new mutation paths added — every new query returns `&`/owned data.
- Behavior identical: map rendering, inspector panel, AI turns, and the
  reassign-leader prompt work as before. GUI verified live (map +
  inspector render, an order round-trips) per "Manual GUI verification" in
  docs/architecture.md.
- `cargo test` / `cargo clippy --all-targets` clean; `docs/architecture.md`
  updated (conventions + per-system notes that mention `game.state`).

## Implementation notes

- Do arch-1, arch-2, arch-3 first: they remove or relocate several
  `game.state` readers (State assembly, `inspect`, `unit_leader_context`),
  shrinking this task's migration list.
- Tests inside `game/` modules that poke `self.state` directly are fine
  and unaffected. If any test outside `game/` needs state access, route it
  through a `#[cfg(test)]` helper in `game/test_support.rs` rather than
  widening visibility.
- Borrow-checker note: a `&Location` from a query borrows `game`; the GUI
  already handles this by re-fetching reads in small scopes
  (docs/architecture.md, Gotchas) — keep new call sites on that pattern.
