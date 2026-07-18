# Split battle aftermath out of orders/attack.rs (refactor proposal)

Status: pending

## Goal

`src/game/orders/attack.rs` is the largest file in the repo (~1300 lines,
roughly 500 of them code, the rest tests): order validation and battle
orchestration share the file with the retreat/rout/shatter/surrender
aftermath. During the Stage 1→2 cleanup this was deliberately *not* split:
the aftermath reads and writes the same battle snapshot state the
orchestration builds, so a mechanical split would have created an awkward
seam rather than a clean one.

## Mechanics

Proposal, for the author to accept, reshape, or fold into other work: when
Stage 2 reworks retreat/rout mechanics, do the split then — the rework has
to open this code anyway, and the new design can define the seam
(orchestration vs. aftermath) instead of inheriting the current tangle.

## Acceptance criteria

- To be specified when the task is taken up.

## Open questions

- Standalone refactor, or absorbed into a Stage 2 combat/retreat rework
  task when one is written?
