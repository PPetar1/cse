# Ideas backlog

A parking lot for game design ideas we want to keep but aren't building yet.
Add freely; nothing here is a commitment. When an idea graduates into real work,
it becomes a task file in tasks/ (and, if it changes direction, a note in
docs/roadmap.md).

## UI / platform direction (decided 2026-07: egui/eframe, since delivered)

- Chosen once the game was playable end-to-end in the terminal (Phase 5
  done): **egui/eframe**, not a Bevy + bevy_egui hybrid — a WitE-like is a
  data-dense panel/table UI around a 2D hex map, egui's sweet spot, and this
  game has no real-time motion to animate. `src/gui.rs` landed in three
  slices: a real window with hex map + click-to-inspect (slice 1), Move/
  Attack/End Turn order issuing (slice 2), then a main menu (New/Load/Quit)
  plus running alongside the terminal at once against one shared game
  (slice 3) — retiring the old Bevy debug view (`visualiser.rs`/`view.rs`,
  and the `bevy` dependency with it) once `gui.rs` covered everything it
  showed.
- Dev-environment gotcha hit while building slice 1: eframe's default `wgpu`
  backend opens a window but never presents a frame in this project's
  sandboxed dev VM (confirmed via debug logging that render calls fire
  correctly — the GPU path just never shows anything). `glow` (OpenGL, via
  Mesa/llvmpipe here) works correctly in the same environment, so
  `Cargo.toml` disables eframe's default features and opts into `glow`
  specifically. Worth re-checking if this ever moves to a real GPU-backed
  environment, but no reason to prefer wgpu here regardless — glow is
  plenty for 2D immediate-mode UI.
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

- **Doctrine turn-timing** — doctrine's turn-start step (leader-to-faction
  contribution, then faction-to-leader drift; see `game::doctrine` and
  "Doctrine" in docs/manual.md) currently runs per-faction, at each
  faction's own turn boundary, purely because that's the existing pattern
  morale drift and refit already follow. Whether it should instead run once
  per full game turn (after every faction has moved) is still open —
  revisit once there's a clearer feel for which cadence plays better,
  especially once a WEGO turn system exists.

- **Battle leadership attribution** — a battle currently credits doctrine
  gain/loss to exactly one leader per side: whichever participating unit's
  leader has the highest average rating (`average_leader_value`). Leaders
  of air-support or interdiction-covering units are never eligible (they
  aren't in `attacker_names`/`defender_names`). Both the "one leader only"
  rule and this eligibility scope are simplifications for now — worth
  reworking once leader battle rolls (see docs/manual.md's "Combat"
  deviations) give a fuller picture of how leaders should participate.

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
