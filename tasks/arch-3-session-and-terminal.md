# Dissolve lib.rs into session.rs and terminal/

Status: pending

## Goal

lib.rs (~400 lines) is a grab-bag application layer: terminal command
dispatch, persistence, shared turn-flow orchestration, and thread glue.
Give each concern a named home so lib.rs shrinks to module declarations
plus re-exports, and so the post-new/load ritual — currently duplicated
between `run` and `gui/menu.rs::adopt_game` — exists exactly once.

## Mechanics

- New `src/session.rs` — the application layer every frontend shares:
  - `SharedGame` type alias + `new_shared_game`.
  - `new_game` / `load_game` / `save_game` (file I/O + postcard), moved
    verbatim from lib.rs.
  - `report_turn_transition` and `play_pending_ai_turns`, moved from
    lib.rs (this is where `ai::take_turn` gets driven from).
  - New `activate_game(game: &mut Game) -> Vec<String>`: the
    post-new/load ritual — play pending AI turns, then drain
    `take_event_messages` — extracted from `run`'s tail (lib.rs:132-147)
    and `menu.rs::adopt_game`. Both call it; the duplication goes away.
- New `src/terminal/` — the terminal frontend, mirroring `gui/` as a
  directory:
  - `terminal/command.rs` — `command.rs` moved unchanged (Command enum,
    parse, HELP_TEXT, COMMAND_KEYWORDS, tests).
  - `terminal/mod.rs` — `run` and `run_shared` (moved from lib.rs, still
    print via `println!`), `require_game`, the rustyline repl loop moved
    out of `main.rs` (including the `reassign_leader` two-prompt special
    case and its helpers `unit_leader_context` /
    `reassign_leader_shared`).
- `main.rs` becomes wiring only: build the shared game, spawn the
  terminal thread (`terminal::run_loop(shared)` or similar), run the GUI
  on the main thread.
- `gui/menu.rs` switches from `crate::new_game`/`load_game`/`save_game`
  and `crate::play_pending_ai_turns` to the `session::` equivalents;
  `adopt_game` keeps only its GUI-specific part (logging the returned
  lines) around a call to `session::activate_game`.
- lib.rs afterward: module declarations and the re-exports the binary and
  frontends need — nothing else.

## Acceptance criteria

- lib.rs contains no function bodies beyond trivial re-export glue.
- The post-new/load AI-auto-play + event-drain logic exists in exactly one
  place (`session::activate_game`), called from both frontends.
- Behavior unchanged: terminal commands, tab completion, history, the
  reassign-leader prompt, GUI New/Load/Save, and end-turn AI auto-play all
  work as before (GUI paths get a live manual check).
- `cargo test` / `cargo clippy --all-targets` clean; `docs/architecture.md`
  layout, file map, and "Adding a command" section updated; README updated
  if any commands/paths it mentions changed.

## Implementation notes

- Pure code movement plus the one extraction (`activate_game`) — no
  behavior changes. Keep the moves reviewable: if one commit gets large,
  split session extraction and terminal move into two commits.
- `run`'s signature (`Result<Option<Game>, Error>`) and the lib.rs tests
  move to `terminal/mod.rs` with it.
