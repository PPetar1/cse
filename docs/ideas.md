# Ideas backlog

A parking lot for game design ideas we want to keep but aren't building yet.
Add freely; nothing here is a commitment. When an idea graduates into real work,
move it to the roadmap in CLAUDE.md.

## UI / platform direction (discussed 2026-07, decision deferred)

- The engine↔visualiser snapshot seam keeps the UI choice swappable; decide
  only when the game is playable end-to-end in the terminal.
- Leading candidate for the real UI: **egui/eframe** — a WitE-like is a
  data-dense panel/table UI around a 2D hex map, which is egui's sweet spot;
  ~10 MB binaries, fast compiles. Step up to a Bevy + bevy_egui hybrid if the
  map view needs game-engine feel (animated pan/zoom, movement tweening).
- Cross-platform is mostly free (all deps are pure Rust, tier-1 targets);
  set up GitHub Actions matrix builds when nearing distribution. Habits now:
  `Path::join` over string paths, case-sensitive asset names.

## Structure

- **Separate factions from players** — factions are currently tied 1:1 to
  players (`[[players]]` carries the faction). Later, multiple players should
  be able to control parts of one faction (multiplayer), so faction-owned
  state (units, morale/experience defaults, supplies) and player-owned state
  (control, turn order) need to become distinct concepts. Fine as-is until
  the turn system firms up.

## Combat

- **Armor piercing model** — replace the flat `hard_attack × vulnerability`
  effect roll with penetration vs armor: a device's AP value degrades with
  range and must beat the target's armor rating for full effect (glances/
  partial damage otherwise). Gives heavy tanks their proper terror and makes
  gun/armor upgrades over the war matter. Slots onto devices.

- **Area-of-effect fire** — grenades, artillery stonks and rockets shouldn't
  resolve as single-target shots: one hit should roll effects against several
  co-located targets (blast value per device deciding how many). Needed for
  artillery to feel right; also the hook for entrenchment reducing blast.

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
