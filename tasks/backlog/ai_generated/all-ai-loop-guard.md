# Guard the all-AI auto-play loop

Status: pending

## Goal

After a human's `end_turn` (or a `new`/`load` that lands on an AI turn),
AI turns auto-play until a human is on turn or the scenario scores. A
scenario where every faction is AI-controlled and no `last_turn` is set
would loop forever. Every shipped scenario has a human seat, so this is a
latent trap, not a live bug — but it should be closed.

## Mechanics

To be specified by the author.

## Acceptance criteria

- To be specified with the Mechanics.

## Open questions

- Refuse such a scenario at load-time validation, cap the auto-play run,
  or make all-AI a supported spectate mode?
