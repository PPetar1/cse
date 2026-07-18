# Pull game rules out of the GUI view code

Status: done

## Goal

The GUI computes game rules inline that belong to the game layer:
`gui/inspector.rs:25-28` derives `owns_hex` (a turn/ownership rule) from
raw state, duplicating logic `victory.rs` already has
(`controlling_faction`, victory.rs:49, currently private). View code
should render answers the game layer provides, never derive rules itself
— otherwise rules drift between the sim and the display.

## Mechanics

- Expose hex ownership as a `Game` query — either widen
  `controlling_faction(x, y) -> Option<String>` to `pub(crate)`, or add a
  thin `pub` wrapper if a friendlier name/shape is wanted (e.g.
  `hex_controlled_by(faction, x, y) -> bool`). One rule, one home:
  the inspector's inline derivation is deleted and calls the query;
  make sure the query's semantics match what the inspector needs (whose
  units are present vs. victory.rs's control notion — if they differ,
  stop and ask, don't pick one silently).
- Sweep the rest of `gui/` for rule logic while there. Known-clean today
  and staying in gui/: `assign_stack_slots`, `map_center`,
  `terrain_color`/`faction_color` (pure presentation), and visibility
  filtering (already calls `Game::is_unit_visible_to`). Anything else
  found that decides a rule (not a pixel) moves behind a `Game` query the
  same way.

## Acceptance criteria

- `gui/` contains no rule derivations over game data — ownership,
  visibility, and any newly found rules are all `Game` calls.
- Inspector behavior unchanged: order buttons appear exactly when they did
  before (own hex on turn), verified live per "Manual GUI verification"
  in docs/architecture.md.
- `cargo test` / `cargo clippy --all-targets` clean; `docs/architecture.md`
  GUI notes updated.

## Implementation notes

- Best done after arch-4 (sealed state), which already forces the GUI
  through queries — this task then only covers derived *rules*, not raw
  reads.
- Semantics already checked: `owns_hex` ("any unit of the on-turn faction
  present") and `controlling_faction == current faction` are equivalent,
  because mixed stacks cannot occur (see `controlling_faction`'s doc,
  victory.rs:46-48). Unifying on `controlling_faction` is safe; re-verify
  only if stacking rules have changed by then.
