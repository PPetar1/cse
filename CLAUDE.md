# CSE — Combat Simulation Engine

An operational wargame engine in Rust, inspired by Gary Grigsby's *War in
the East*; the end goal is a finished game that improves on it. Engine-first:
a simulation core driven by a terminal and an egui window at the same time,
with scenarios/maps/TOEs/elements as easy-to-edit TOML — scenarios are data,
not code. Era-agnostic by design: mechanics generic, era flavor in scenario
data (WW2 is the first target, not the boundary).

## Documentation map

Each kind of information lives in exactly one place. Read what the task
needs; update the same doc in the same pass as the change it describes.

- `README.md` — what the project is, commands, usage. Update when the
  user-facing surface changes.
- `docs/manual.md` — how every game system *behaves*, strictly
  non-technical (no file paths, function or type names). Update whenever a
  system's behavior changes. Grows toward the player-facing manual.
- `docs/architecture.md` — the technical reference: layout, conventions,
  per-system implementation notes, testing, gotchas. Update with the code.
- `docs/roadmap.md` — direction: the five development stages. Update when
  direction changes, not per task.
- `docs/ideas.md` — parking lot for future game ideas; save new ideas
  there instead of losing them in conversation.
- `tasks/` — the work queue (see "Development workflow" below).
- This file — agent guidelines only. Update it only when the guidelines
  themselves change.

## Build & test

```
cargo build                  # first build is slow; incremental is fast
cargo run                    # GUI window + terminal on stdin, one shared game
cargo test
cargo clippy --all-targets   # keep it clean, and the build warning-free
```

## Development workflow

The prototype (Stage 1) was built with the AI designing freely under loose
direction. That mode is over. From Stage 2 on the author does the design
and the agent does focused, completion-oriented implementation:

- When the author says to continue working (or similar): read
  `tasks/task_priorities.md`, take the topmost pending task, and implement
  exactly what its task file specifies.
- Work down the priorities list until it is exhausted or a task is blocked
  on the author's answer.
- If a task spec is ambiguous or underspecified, **ask how to implement
  it — never assume or invent the design.** Design is the author's job in
  this stage.
- No unsolicited design or feature suggestions. When the author wants
  planning, they will explicitly say so beforehand.
- Per task: implement; keep `cargo test` and `cargo clippy --all-targets`
  clean; update the affected docs in the same pass (see the map above);
  make one commit, which also moves the task file to `tasks/done/` and
  ticks it off in `task_priorities.md`.
- Changes with a GUI surface get verified live, not just by unit tests —
  see "Manual GUI QA" in docs/architecture.md.

`tasks/README.md` covers the task file format and queue mechanics.

## Standing rules

- Read existing files before writing. Don't re-read unless changed.
- Thorough in reasoning, concise in output.
- Skip files over 100KB unless required.
- No sycophantic openers or closing fluff. No emojis or em-dashes.
- Do not guess APIs, versions, flags, commit SHAs, or package names.
  Verify by reading code or docs before asserting.
- Prioritize clean, simple, reviewable code — the author reviews everything
  and may pick the project up solo later. Work in small chunks rather than
  big diffs. Modularity/expandability come from clean seams and data-driven
  design, not speculative abstraction.
- Commit once a change is implemented and verified; no need to ask first.
- The project doubles as a learning exercise in AI-driven development; the
  README carries a disclaimer that code is heavily AI-generated.
