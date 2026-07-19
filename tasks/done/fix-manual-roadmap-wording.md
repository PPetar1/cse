# Fix manual/roadmap wording inaccuracies found by arch-9

Status: pending

## Goal

Findings F4, F9 and F17 of `tasks/arch-9-findings.md`: three places where
`docs/manual.md`/`docs/roadmap.md` overstate or misframe actual behavior.
Docs-only pass; the code is correct as is.

## Mechanics

1. **F4** — `docs/manual.md:246-248` (interdiction): a fighter pulled into
   a battle with no enemy aircraft "can neither shoot nor be shot at" is
   half wrong — anti-air-flagged ground elements on the attacking side can
   shoot it (consistent with the manual's own anti-air rule). Reword to:
   it cannot shoot, and only anti-air-capable ground elements can shoot at
   it; it still gains experience for being fielded.
2. **F9** — `docs/manual.md:8-11` (intro): stop blanket-labeling the whole
   manual as Stage-1 prototype-grade. Reword to say the systems originate
   from the Stage-1 prototype and are being rebuilt to their real designs
   in Stage 2 (now underway) — Stage-2 systems (Leaders, and whatever
   lands after) are documented here as they arrive. Keep the per-section
   "known simplifications" convention statement.
3. **F17** — `docs/roadmap.md:62-64` (Stage 1, Interface bullet): "every
   command has a clickable equivalent" is false for `simulate`, `units`,
   `leaders`, `leader` and `reassign_leader`. Reword to claim clickable
   equivalents for every *order* (move, attack, air support, interdict,
   end turn, save/load/new) and name the report/inspection commands that
   remain terminal-only, so the Stage-3 GUI stage inherits an honest
   baseline.

## Acceptance criteria

- The three passages reworded as specified; surrounding text untouched.
- No claims introduced that the code doesn't back (re-check each new
  sentence against behavior).
- No code changes.

## Implementation notes

Line numbers reference commit d67cd83; re-locate by content if shifted.
Keep the manual's no-code-references rule: no file, function or type names
in manual.md wording.

## Open questions

None.
