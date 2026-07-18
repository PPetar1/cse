# Move inspect into Game as a summary method

Status: pending

## Goal

`inspect` (lib.rs:210) is the one read command that violates the
"summaries return Strings" convention: it is a free function in lib.rs
that `println!`s directly, reaches through `game.state.map`, and
hard-codes a unit/element display format that partly duplicates `Unit`'s
own `Display`. Every other report is a `Game` method returning a `String`
that any interface can show. Make inspect follow the same convention.

## Mechanics

- Add `Game::inspect_summary(&self, target: &InspectTarget) ->
  Result<String, Error>` in the game layer, producing exactly the text the
  terminal prints today (location line, then per unit: display line, TOE,
  leader, unit-average morale/experience, one line per element).
- Move the `InspectTarget` enum (`Hex { x, y }` / `Offmap(name)`) from
  `command.rs` into the game layer, since the game method now consumes it;
  `command.rs` imports it from there. The interface layer may depend on
  game, never the reverse.
- The fog-of-war gate stays: for a hex outside the current faction's
  detection range, return `"Unknown — outside detection range."` as the
  summary text (not an error), matching current behavior.
- `lib.rs`'s `run` arm becomes a one-liner:
  `println!("{}", game.inspect_summary(&target)?)`. Delete the free
  function.
- Where the current format duplicates `Unit`'s `Display`, call the
  `Display` impl instead of re-formatting.

## Acceptance criteria

- No `inspect` function in lib.rs; the game layer owns the text.
- Terminal `inspect` output is unchanged for: a visible hex with units, an
  empty hex, a fogged hex, an offmap location, and an invalid target
  (error).
- Unit tests on `inspect_summary` cover the fogged and offmap cases.
- `cargo test` / `cargo clippy --all-targets` clean; `docs/architecture.md`
  updated (lib.rs and game file-map entries).

## Implementation notes

- Natural home: `game/mod.rs` next to `units_summary`, which already
  documents the summary convention. The GUI inspector is *not* switched to
  this method here — that is arch-6's concern.
- `inspect_summary` uses `units_at_location` (sorted), so ordering stays
  stable and consistent with `move`'s unit indices.
