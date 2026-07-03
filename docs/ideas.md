# Ideas backlog

A parking lot for game design ideas we want to keep but aren't building yet.
Add freely; nothing here is a commitment. When an idea graduates into real work,
fold it into the relevant phase in docs/roadmap.md and the "Current focus"
section of CLAUDE.md.

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

- **Air and naval warfare** — the end goal includes air warfare and possibly
  naval. Nothing built yet; the constraint it imposes today is only that
  ground-combat design choices stay generic (fires-based resolution, devices,
  data-driven element types) so new domains slot in as more element classes,
  devices and mission procedures rather than parallel engines.

- **WEGO turn system** — a simultaneous mode next to IGO-UGO: every player
  queues orders, `end_turn` resolves them together. The seam already exists
  (Phase 1): scenarios select a `TurnSystem`, and the matches on that enum in
  the game layer are where a `Wego` variant plus an order queue plug in.
  Eventual customer is roadmap pillar 5 (modern multiplayer) — simultaneous
  turns are what makes server-mediated play with multiple players per faction
  feel modern.

## Command & doctrine

- **Split doctrine out of morale** — WitE "morale" conflates two things: a
  unit's fighting spirit and the faction's overall combat-doctrine
  effectiveness. Split them: morale stays per-element, and a separate
  faction-wide **doctrine** modifier captures how well the faction fights as an
  institution. Doctrine improves when high-initiative leaders take part in
  battles (scaled by battle size or similar) and drifts toward the faction's
  average leader capability over time. The pull goes both ways: leader stats
  also drift toward the doctrine level, so leaders and doctrine converge.
  Depends on leaders existing first. Slots naturally onto the runtime `Player`
  (where faction-wide morale/experience defaults already live).

- **Unit staffs** — beyond a single commanding leader, units carry a staff with
  its own stats, influencing recovery, logistics throughput, leader checks and
  similar administrative rolls. Lets HQ quality matter separately from the
  general's personal ratings.

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
