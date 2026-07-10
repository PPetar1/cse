# Make victory scoring end the game

Status: pending

## Goal

When a scenario's `last_turn` completes, the final score and winner are
printed — but nothing stops play. Orders keep working, turns keep passing;
scoring is report-only. Decide what "the game is over" means and implement
it.

## Mechanics

To be specified by the author.

## Acceptance criteria

- To be specified with the Mechanics.

## Open questions

- Hard gate (further orders refused, only inspection/save allowed) or soft
  (a persistent "game over" banner, sandbox play still possible)?
- What does the GUI show — an end screen, or just the report in the log?
- Should `end_turn` past the last turn be possible at all?
