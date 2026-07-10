# Teach the AI to use air operations

Status: pending

## Goal

The AI never issues `air_support` or `interdict` — in
`frontline_sector.scen` its Stuka wing never flies, and it never declares
fighter coverage. Only a human player can use the air war today; the AI
should be able to as well.

## Mechanics

To be specified by the author. Known surface: `ai::take_turn` consumes
`Game`'s public order API only; `Game::air_support` folds an air unit into
a ground attack, `Game::interdict` declares coverage of a hex (up to 3 per
unit), and both respect the TOE's mission range.

## Acceptance criteria

- To be specified with the Mechanics.

## Implementation notes

Keep the existing `ai.rs` rule: no new pathfinding or combat logic in the
AI — every decision bottoms out in `Game`'s existing public methods.

## Open questions

- When should the AI commit ground support: every attack it makes, only
  marginal ones (per `simulate`), or something else?
- What is the interdiction policy — cover its own threatened hexes, likely
  enemy attack targets, or objectives?
- Should this wait until Stage 2 reworks the air-war systems themselves?
