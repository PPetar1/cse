# Fix stale references in code doc comments found by arch-9

Status: pending

## Goal

Findings F10-F16 of `tasks/arch-9-findings.md`: doc comments naming types
that don't exist or pointing at files where the referenced thing no longer
lives. Comment-only pass — no behavior, signature, or docs-file changes.

## Mechanics

1. **F10** — `game/victory.rs:120-121` (`VictoryHexInfo` doc): delete the
   clause "the same way `MapSnapshot` is decoupled from `State`" — no
   `MapSnapshot` type exists. Keep the point that `VictoryHexInfo` is
   decoupled from the scenario-parsed `VictoryHex`.
2. **F11** — `core/unit.rs:137, 164, 194`: replace the references to
   `ElementClass::can_target_air` (no such method) with a pointer to the
   derived `can_target_air` snapshot logic in `procedures::combat`
   (`combat_elements`, combat.rs:108) or rephrase without naming a
   nonexistent item.
3. **F12** — `ai.rs:2`: "(see `Command::EndTurn` in lib.rs)" → the AI is
   invoked from `session::play_pending_ai_turns` (session.rs); `Command`
   lives in `terminal/command.rs`. `ai.rs:139-141`: `AiTurnReport` is not
   "printed by `lib.rs`" — it is returned through `session.rs`, printed
   by the terminal and logged by the GUI. Fix both.
4. **F13** — `game/leaders.rs:67`: "(see `main.rs`)" → the
   `reassign_leader` prompt lives in `terminal/mod.rs`
   (`run_reassign_leader`).
5. **F14** — `game/mod.rs:49-51` and `game/events.rs:29`: event messages
   are drained by `session::report_turn_transition`/`activate_game` (both
   frontends), not by "`run`". Name session.rs.
6. **F15** — `gui/mod.rs:17`: "`SharedGame` … see lib.rs" → `SharedGame`
   is defined in `session.rs`; lib.rs only re-exports it.
7. **F16** — `game/detection.rs:6-7`: collapse the garbled doubled manual
   reference into one: `see "Fog of war and detection" in docs/manual.md`.

## Acceptance criteria

- All seven comments corrected; grep confirms no remaining mention of
  `MapSnapshot` or `ElementClass::can_target_air` anywhere in `src/`.
- Zero non-comment diff (`git diff` touches comment lines only).
- `cargo test` and `cargo clippy --all-targets` clean.

## Implementation notes

Line numbers reference commit d67cd83; re-locate by content if shifted.

## Open questions

None.
