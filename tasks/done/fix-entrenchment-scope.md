# Fix entrenchment scope: offmap ticks, interdictor fort leak, arrival tick

Status: pending

## Goal

Findings F1-F3 of `tasks/arch-9-findings.md` — the three arch-9 findings
with gameplay effect, all in the entrenchment system. In each case
`docs/manual.md` already describes the intended behavior; the code
deviates. This task makes the code match the manual, so no docs change is
part of it.

## Mechanics

1. **Only on-map units entrench** (F1). `apply_entrenchment`
   (`game/entrenchment.rs:19-26`) must skip units whose location is
   `UnitLocation::Offmap` — same filter `apply_refit` already applies
   (`game/refit.rs:34-36`). A unit sitting in a reserve box gains no fort
   levels, per manual.md ("every on-map unit … digs in", "nothing happens
   to a unit in one [offmap box]").

2. **A unit relocated by this turn's scheduled arrivals gets no
   entrenchment tick that same turn start** (F2 ordering bug, finding F3).
   Today `begin_turn` (`game/turn.rs:50-54`) runs
   `apply_scheduled_arrivals` (which resets the unit to fort 0) and then
   `apply_entrenchment` ticks it straight back to 1. After the fix a
   mid-game reinforcement lands at fort level 0 — matching both the manual
   ("arriving as a reinforcement resets it to zero") and what turn-1
   arrivals applied from `Game::build` already do. It then ticks to 1 at
   its faction's *next* turn start as normal.

3. **The defender-CV entrenchment multiplier counts the ground defending
   stack only** (F2). Units pulled into the defender snapshot by
   interdiction (`game/orders/attack.rs:218-224`) must neither raise nor
   lower the fort average used by `fort_defense_modifier`
   (`procedures/combat.rs:414-426`) — they are not part of the stack and
   don't retreat with it. Note plain zeroing of their `fort_level` is NOT
   enough: `average_fort_level` divides by every defending instance, so a
   zeroed fighter would still dilute a dug-in stack's average. The
   covering unit's instances must be excluded from both the numerator and
   the denominator. An air unit physically stacked at the defended hex is
   already excluded from the covering pull-in (existing double-count
   guard) and keeps counting as ordinary stack.

## Acceptance criteria

- An offmap unit's `fort_level` stays 0 across any number of turn starts
  (new test).
- A reinforcement arriving mid-game has `fort_level` 0 on its arrival
  turn and 1 after its faction's next turn start (new test).
- A battle at an interdiction-covered hex computes the same
  `fort_defense_modifier` input as the identical battle without the
  covering unit, for a dug-in ground stack (new test — e.g. assert the
  defender CV's fort component is unchanged by adding coverage).
- All existing tests still pass; `cargo test` and
  `cargo clippy --all-targets` clean.
- No docs changes — `docs/manual.md` already states this behavior; confirm
  that remains true after the change.

## Implementation notes

- For (2): the least invasive shape is `apply_scheduled_arrivals`
  returning the names it relocated and `begin_turn` passing them to
  `apply_entrenchment` as a skip set. Reordering the two calls instead
  would change withdrawal semantics — avoid.
- For (3): a boolean on `CombatElement` (set false for interdiction
  pull-ins in `prepare_battle`, true everywhere else) that
  `average_fort_level` filters on is the expected shape; `combat.rs`'s
  purity (no `Game`/`State` access) must be preserved. Attacker-side
  snapshots never feed the fort average, so their flag value is inert.
- The existing test at `procedures/combat.rs:856` (attacker fort has no
  effect) and the interdiction double-count test must keep passing.

## Open questions

None.
