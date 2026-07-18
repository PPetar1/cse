# Sweep for remaining layering violations and audit the docs against code

Status: pending

## Goal

arch-1 through arch-8 each found and fixed one distinct kind of
layering/boundary violation — state leaking through fields (arch-4),
rule logic duplicated in `gui/` instead of asked from `Game` (arch-6),
pathfinding logic living outside `procedures/` (arch-5), `core::`
methods called directly from `ai.rs` (arch-7). Every one of these was
found by deliberately digging into a specific file, not by a systematic
sweep — there's no reason to assume they're the only instances left.
This task is a dedicated pass to find what's still there, across the
whole codebase, and propose fixes — it does not implement anything.

Separately: `docs/architecture.md` and `docs/manual.md` are themselves
AI-written, across many sessions, and have already been caught being
wrong once this session — `game/victory.rs`'s `VictoryHexInfo` doc
comment references a `MapSnapshot` type as a parallel example that
doesn't exist anywhere in the codebase (confirmed by grep). Treat both
docs as claims to verify against the actual code, not as ground truth —
they may describe intentions that were never finished, behavior that's
since changed, or conventions that were never actually enforced
everywhere they claim to be.

## Mechanics

Code sweep — for each of the following, check the *whole* codebase, not
just files already touched by arch-1..8:

- **core:: boundary** (arch-7's rule, once documented): anywhere outside
  `game/`/`procedures/` calling a method on a `core::` type, rather than
  reading its data or going through a `Game` query.
- **Rule derivation outside `game/`** (arch-6's class of bug): anywhere
  outside `game/` computing a game rule from data it read, instead of
  asking `Game` for the answer.
- **Algorithmic logic outside `procedures/`/`game/`** (arch-5's class):
  real branching/looping game logic (pathfinding-like, combat-like)
  living in `ai.rs`, `gui/`, or `terminal/` instead of `procedures/` or
  `game/`.
- **Direct `game.state.*` access outside `game/`** that arch-4 might
  have missed, and any `Game` query that hands back something broader
  than its caller needs (violates "queries stay narrow").
- **Wrong-direction dependencies**: confirm `procedures/` never calls
  back into `game/`, and that the one-way layering (`core` ←
  `procedures` ← `game` ← interfaces) holds everywhere it's supposed to,
  not just where it's already been checked.

Docs audit:

- Read `docs/architecture.md` in full against current code — Layering,
  Conventions, and every Per-system implementation note. Flag anything
  inaccurate, stale, or describing something not actually implemented,
  including the known dead `MapSnapshot` reference.
- Read `docs/manual.md` in full against actual game behavior where
  feasible. Flag anything describing behavior the code doesn't actually
  have (or has changed since).
- `docs/roadmap.md` is lower priority (it's direction, not a technical
  claim) — flag anything there that's clearly contradicted by current
  code, but don't do a line-by-line audit of it.

## Deliverable

A written findings report — a new file (e.g. `tasks/arch-9-findings.md`,
or whatever's clearest) listing each finding with its location
(file/line), what expectation or rule it violates, and a proposed fix.
Frame every proposal as something for the author to accept, adjust, or
reject — not as a decision already made. **Do not modify any source
file, docs file, or existing task file as part of this task** beyond
adding the findings report itself.

## Acceptance criteria

- Every violation class already fixed in arch-1 through arch-8 has been
  explicitly checked for recurrence elsewhere in the codebase, not just
  spot-checked.
- `docs/architecture.md` and `docs/manual.md` have been read in full
  against current code, with every discrepancy found listed in the
  report — including the known dead `MapSnapshot` reference.
- The report contains no source code changes — proposals only.
- Each finding is concrete enough to become its own task file without
  further investigation: a real file/line reference and a proposed fix,
  not a vague impression.

## Implementation notes

- This task's output is a research/writing pass, not a code change —
  `cargo test`/`cargo clippy` aren't relevant acceptance gates here.
- Worth specifically re-checking `terminal/` and `session.rs` (both
  confirmed clean earlier this session, but more tasks have landed
  since) and any `game/` submodule added or changed after arch-6.
- Queued after arch-7 and arch-8 so this sweep isn't re-flagging work
  already in flight — it should surface *other* instances, not repeat
  those two.

## Open questions

None.
