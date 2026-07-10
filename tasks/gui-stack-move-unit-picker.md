# GUI: pick which stacked unit moves

Status: pending

## Goal

The GUI's Move order always moves the first unit (index 0, alphabetical)
of the inspected hex's stack; the terminal's `move` takes an explicit unit
index. A multi-unit stack can't be split with the mouse.

## Mechanics

To be specified by the author.

## Acceptance criteria

- To be specified with the Mechanics.

## Open questions

- Selection UI: click a unit's roster entry in the inspector to select it,
  per-unit Move buttons, or a checkbox set for moving part of a stack in
  one order?
- Does Attack need the same treatment, or does whole-stack-attacks stay
  the rule (as it is in the terminal too)?
