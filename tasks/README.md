# Task workflow

The agent contract — take the topmost queued task, implement exactly what
it specifies, ask on ambiguity instead of assuming, one commit per task —
lives in CLAUDE.md ("Development workflow"). This file covers the queue
mechanics.

- **One task = one file** in this folder, following `TEMPLATE.md`. The
  author writes and edits task files; the Mechanics section is the design,
  and it is the author's.
- **`task_priorities.md`** is the ordered queue: the agent takes the
  topmost unchecked task. A task file *not* listed there should not
  be picked up until the author queues it.
- **`backlog/`** holds unrefined tasks for the future that the author still 
  needs to work on, there is no need to read those every time
- **`done/`** holds completed task files.
- Lifecycle: `Status: pending` → `in progress` → `done` (moved to
  `done/`). Whether a pending task is actionable is decided by
  `task_priorities.md`, not its Status line.
- Unresolved "Open questions" that affect the implementation are blockers:
  ask the author, don't proceed — and don't skip past a blocked task
  without asking.
