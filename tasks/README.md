# Task workflow

How Stage 2+ development runs. CLAUDE.md's "Development workflow" section
carries the summary; this file is the full contract.

## The pieces

- **One task = one file** in this folder, following `TEMPLATE.md`. The
  author writes and edits task files — the Mechanics section is the design,
  and it is the author's.
- **`task_priorities.md`** is the ordered queue. The agent always takes the
  topmost unchecked task. A task file *not* listed there is **backlog**:
  visible, but not to be picked up until the author adds it to the queue.
- **`done/`** holds completed task files.

## The agent contract

- When the author says to continue working (or similar): read
  `task_priorities.md`, take the topmost unchecked task, and implement
  exactly what its file specifies.
- Work down the queue until it is exhausted or a task is blocked on the
  author's answer. Don't skip ahead past a blocked task without asking.
- If the spec is ambiguous or underspecified, **ask how to implement it —
  never assume or invent the design.** Unresolved "Open questions" that
  affect the implementation are blockers, not invitations.
- No unsolicited design or feature suggestions. When the author wants
  planning, they will say so explicitly beforehand.
- Per task, in one commit:
  - the implementation, with `cargo test` and `cargo clippy --all-targets`
    clean;
  - the matching doc updates (behavior changes → `docs/manual.md`,
    implementation → `docs/architecture.md`, user-facing surface →
    `README.md`);
  - the task file moved to `done/` (Status set to done) and its line
    checked off in `task_priorities.md`.
- Changes with a GUI surface get verified live, not just by unit tests —
  see "Manual GUI QA" in `docs/architecture.md`.

## Task file lifecycle

`Status: pending` → `Status: in progress` (while being worked) →
`Status: done` (moved to `done/`). Whether a pending task is actionable is
decided by `task_priorities.md`, not by its Status line.
