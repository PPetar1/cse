# Ideas backlog

A parking lot for game design ideas we want to keep but aren't building yet.
Add freely; nothing here is a commitment. When an idea graduates into real work,
move it to the roadmap in CLAUDE.md.

## Combat

- **Low-randomness battle setting** — since battle resolution is computationally
  cheap, offer a setting that resolves each battle by simulating it N times
  (e.g. 10) and taking the average as the result. Reduces dice-luck swings for
  players who want operational decisions, not gambling. Builds on the planned
  `simulate` tooling.

- **Battle-time logistics** — elements have a chance to receive resupply *during*
  a battle, so good organization/supply state improves combat organically rather
  than through hardcoded modifiers (as WitE does). Requires ammo/supply to be
  live per-element state inside the battle snapshot, which the snapshot design
  already anticipates.

- **2D tactical battlefield** — longer-term alternative to the round/range-band
  resolution model: fight battles on a small hex grid with randomly generated
  terrain and line-of-sight limits, elements following simple local rules
  (close, hold at effective range, stand off). Deferred in favor of a WitE-style
  baseline first; the battle-engine snapshot/report interface is designed so this
  can be swapped in behind it later and compared via the simulator.
