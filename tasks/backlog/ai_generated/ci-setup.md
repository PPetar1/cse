# Set up CI

Status: pending

## Goal

There is no CI; `cargo test` and `cargo clippy` run only on the author's
machine. A basic workflow (test + clippy on push) would catch drift early;
docs/roadmap.md places cross-platform builds and packaging in Stage 3, but
plain test CI could land any time.

## Mechanics

To be specified by the author.

## Acceptance criteria

- To be specified with the Mechanics.

## Open questions

- GitHub Actions, or something else?
- Linux-only test runs for now, or cross-platform from the start?
- Now, or bundled with Stage 3's packaging work?
