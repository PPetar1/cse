# Fix AI stack movement: a stuck lead unit blocks its followers

Status: pending

## Goal

Finding F5 of `tasks/arch-9-findings.md`: `ai.rs::move_toward`
(`ai.rs:118-133`) always issues `move_unit(..., 0)` — unit index 0 at the
source hex. When the alphabetically-first unit of a stack is stuck (no MP,
nowhere passable), every later iteration of the stack loop
(`ai.rs:41-45`) re-addresses that same stuck unit, so followers that could
move don't. The manual says the AI "move[s] the stack" toward its target;
make that true per unit.

## Mechanics

When the AI moves a stack, each iteration must order the specific unit it
is iterating for, not whatever currently sits at index 0. Resolve the
unit's name to its current index at the source hex (the index order is
`units_at_location`'s name sort — the same contract `move_unit` documents)
immediately before issuing the order. A unit that cannot move is skipped
and the loop continues with the next unit; the reported log line then
always names the unit that actually moved.

No other AI behavior changes: attack decisions, target selection, and the
full-jump-then-best-step movement per unit stay exactly as they are.

## Acceptance criteria

- New test: a stack of two units where the alphabetically-first has 0 MP
  and the second has MP — after `take_turn`, the second unit has moved
  toward the target and the first has not.
- Every log line's unit name matches the unit whose location changed.
- Existing AI tests pass; `cargo test` and `cargo clippy --all-targets`
  clean.
- No docs changes needed (`docs/manual.md`'s AI description already says
  "move the stack"); confirm that holds.

## Implementation notes

- `move_toward` currently takes only coordinates; the natural shape is
  passing the unit name in and looking up its index via
  `Game::units_at_location` (through the existing `location` query) right
  before each `move_unit` call — including re-resolving between the
  full-jump attempt and the single-step fallbacks if a prior call could
  have moved it (it can't within one `move_toward`, but the index must be
  fresh per call site since earlier stack members may have left).

## Open questions

None.
