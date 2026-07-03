//! Fires-based battle resolution. The model is documented in
//! docs/combat_design.md — update that file when changing this one.
//!
//! The engine is deliberately pure: it works on battle-local snapshots
//! (`CombatElement`s) plus an RNG, and never touches `Game`/`State`. The game
//! layer builds the snapshots, calls `resolve_battle`, and persists each
//! element's final state back to its unit. Future per-element stats
//! (experience, morale, ammo, …) extend the snapshot and the roll math here,
//! not the engine's control flow.

use std::collections::HashMap;
use std::fmt::Display;

use rand::Rng;

use crate::Error;
use crate::core::location::Terrain;
use crate::core::unit::{Element, ElementClass, Unit};

/// Engagement range bands in meters. One combat round per band, closing in;
/// an element fires in a round iff its `range` stat covers the band.
const RANGE_BANDS: [u32; 5] = [3000, 1500, 800, 400, 100];

/// Severity split for an effective hit (percent): the rest is destroyed.
const DISRUPT_CHANCE: f32 = 50.0;
const DAMAGE_CHANCE: f32 = 35.0;

/// Final-CV odds ratio at which the defender is forced to retreat.
const RETREAT_ODDS: f32 = 2.0;

/// One individual squad/gun/vehicle in a battle: a snapshot of everything
/// resolution needs, decoupled from game state.
#[derive(Debug, Clone)]
pub struct CombatElement {
    pub unit_name: String,
    pub element_name: String,
    pub state: CombatElementState,
    cv: f32,
    morale: u32,
    experience: u32,
    accuracy: u32,
    range: u32,
    v_inf: u32,
    v_arm: u32,
    fires: FireType,
}

/// Variant order matters: later = worse, and a doubly-hit element keeps the
/// worse effect (see `apply_hits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CombatElementState {
    Ready,
    /// Stops fighting and counts for nothing, but recovers after the battle.
    Disrupted,
    /// Persisted on the unit as ready → damaged.
    Damaged,
    /// Persisted on the unit as a permanent loss.
    Destroyed,
}

#[derive(Debug, Clone, Copy)]
enum FireType {
    Soft,
    ArmorPiercing,
}

/// Which vulnerability a target rolls against when this class fires at it.
fn fire_type(class: &ElementClass) -> FireType {
    match class {
        ElementClass::AtGun | ElementClass::LightTank | ElementClass::MedTank => {
            FireType::ArmorPiercing
        }
        ElementClass::Inf | ElementClass::MotInf | ElementClass::LightArt => FireType::Soft,
    }
}

/// Expand units into per-instance combat elements — one entry per ready
/// squad/gun/vehicle. Damaged elements sit the battle out. Instances keep
/// their units' order, so a name-sorted unit list gives a deterministic
/// snapshot (and with it, seed-reproducible battles).
pub fn combat_elements(
    units: &[&Unit],
    element_types: &HashMap<String, Element>,
) -> Result<Vec<CombatElement>, Error> {
    let mut elements = Vec::new();
    for unit in units {
        for element_in_unit in &unit.elements {
            let element_type = element_types.get(&element_in_unit.name).ok_or_else(|| Error {
                error_message: format!(
                    "Unit '{}' contains element '{}' with no element definition.",
                    unit.name, element_in_unit.name
                ),
            })?;
            for _ in 0..element_in_unit.ready {
                elements.push(CombatElement {
                    unit_name: unit.name.clone(),
                    element_name: element_type.name.clone(),
                    state: CombatElementState::Ready,
                    cv: element_type.cv,
                    morale: element_in_unit.morale,
                    experience: element_in_unit.experience,
                    accuracy: element_type.accuracy,
                    range: element_type.range,
                    v_inf: element_type.v_inf,
                    v_arm: element_type.v_arm,
                    fires: fire_type(&element_type.class),
                });
            }
        }
    }
    Ok(elements)
}

/// Fight a battle to completion, mutating the snapshots' states in place.
/// The caller persists the final states back to the units.
pub fn resolve_battle(
    attackers: &mut [CombatElement],
    defenders: &mut [CombatElement],
    defender_terrain: Terrain,
    rng: &mut impl Rng,
) -> BattleReport {
    let mut rounds = Vec::new();

    for &band in &RANGE_BANDS[opening_band(defender_terrain)..] {
        if !has_ready(attackers) || !has_ready(defenders) {
            break;
        }

        // Both sides fire simultaneously: every shot resolves against the
        // states elements had at the start of the round, and the effects are
        // applied afterwards — no first-strike advantage from code ordering.
        let (attacker_shots, hits_on_defenders) =
            fire_round(attackers, defenders, band, cover_modifier(defender_terrain), rng);
        let (defender_shots, hits_on_attackers) = fire_round(defenders, attackers, band, 1.0, rng);

        apply_hits(defenders, &hits_on_defenders);
        apply_hits(attackers, &hits_on_attackers);

        rounds.push(RoundReport {
            range: band,
            attacker_shots,
            attacker_hits: hits_on_defenders.len() as u32,
            defender_shots,
            defender_hits: hits_on_attackers.len() as u32,
        });
    }

    let attacker_cv = ready_cv(attackers);
    let defender_cv = ready_cv(defenders) * terrain_defense(defender_terrain);
    let outcome = if attacker_cv > 0.0
        && (defender_cv <= 0.0 || attacker_cv / defender_cv >= RETREAT_ODDS)
    {
        BattleOutcome::DefenderRetreats
    } else {
        BattleOutcome::DefenderHolds
    };

    BattleReport {
        rounds,
        attacker_losses: losses(attackers),
        defender_losses: losses(defenders),
        attacker_cv,
        defender_cv,
        outcome,
    }
}

/// Fight the same battle `runs` times against copies of the snapshots and
/// aggregate the results — the tuning tool: change a knob, compare
/// distributions. The passed snapshots are never mutated.
pub fn simulate_battles(
    attackers: &[CombatElement],
    defenders: &[CombatElement],
    defender_terrain: Terrain,
    runs: u32,
    rng: &mut impl Rng,
) -> SimulationReport {
    let mut retreats = 0;
    let mut attacker_losses = LossTotals::default();
    let mut defender_losses = LossTotals::default();
    let mut attacker_cv = 0.0;
    let mut defender_cv = 0.0;

    for _ in 0..runs {
        let mut attackers_run = attackers.to_vec();
        let mut defenders_run = defenders.to_vec();
        let report = resolve_battle(&mut attackers_run, &mut defenders_run, defender_terrain, rng);

        if report.outcome == BattleOutcome::DefenderRetreats {
            retreats += 1;
        }
        attacker_losses.add(&report.attacker_losses);
        defender_losses.add(&report.defender_losses);
        attacker_cv += report.attacker_cv as f64;
        defender_cv += report.defender_cv as f64;
    }

    SimulationReport {
        runs,
        retreats,
        attacker_losses: attacker_losses.average(runs),
        defender_losses: defender_losses.average(runs),
        attacker_cv: (attacker_cv / runs as f64) as f32,
        defender_cv: (defender_cv / runs as f64) as f32,
    }
}

#[derive(Default)]
struct LossTotals {
    disrupted: u64,
    damaged: u64,
    destroyed: u64,
}

impl LossTotals {
    fn add(&mut self, losses: &Losses) {
        self.disrupted += losses.disrupted as u64;
        self.damaged += losses.damaged as u64;
        self.destroyed += losses.destroyed as u64;
    }

    fn average(&self, runs: u32) -> AverageLosses {
        AverageLosses {
            disrupted: self.disrupted as f32 / runs as f32,
            damaged: self.damaged as f32 / runs as f32,
            destroyed: self.destroyed as f32 / runs as f32,
        }
    }
}

/// One side shoots at the other. Returns the number of shots taken and the
/// effective hits as (target index, severity); the caller applies them after
/// both sides have fired. `target_cover` scales hit chances (defender terrain
/// protects the defender; attackers are assumed advancing in the open).
fn fire_round(
    firers: &[CombatElement],
    targets: &[CombatElement],
    band: u32,
    target_cover: f32,
    rng: &mut impl Rng,
) -> (u32, Vec<(usize, CombatElementState)>) {
    let target_pool: Vec<usize> = targets
        .iter()
        .enumerate()
        .filter(|(_, target)| target.state == CombatElementState::Ready)
        .map(|(index, _)| index)
        .collect();
    if target_pool.is_empty() {
        return (0, Vec::new());
    }

    let mut shots = 0;
    let mut hits = Vec::new();
    for firer in firers {
        if firer.state != CombatElementState::Ready || firer.range < band {
            continue;
        }
        // Failure to commit: green elements often don't fire at all (WitE's
        // "notional CV lost as combat progresses"). No shot is recorded.
        if rng.random_range(0.0..100.0) >= firer.experience as f32 {
            continue;
        }
        shots += 1;

        // Uniform pick over enemy instances: numerous element types soak
        // proportionally more fire, an emergent screening effect.
        let target_index = target_pool[rng.random_range(0..target_pool.len())];
        let target = &targets[target_index];

        let hit_chance = firer.accuracy as f32 * target_cover;
        if rng.random_range(0.0..100.0) >= hit_chance {
            continue;
        }

        let vulnerability = match firer.fires {
            FireType::ArmorPiercing => target.v_arm,
            FireType::Soft => target.v_inf,
        };
        if rng.random_range(0.0..100.0) >= vulnerability as f32 {
            continue;
        }

        hits.push((target_index, severity(rng.random_range(0.0..100.0))));
    }
    (shots, hits)
}

fn severity(roll: f32) -> CombatElementState {
    if roll < DISRUPT_CHANCE {
        CombatElementState::Disrupted
    } else if roll < DISRUPT_CHANCE + DAMAGE_CHANCE {
        CombatElementState::Damaged
    } else {
        CombatElementState::Destroyed
    }
}

fn apply_hits(side: &mut [CombatElement], hits: &[(usize, CombatElementState)]) {
    for &(target_index, effect) in hits {
        // An element hit twice in one round keeps the worse effect.
        if effect > side[target_index].state {
            side[target_index].state = effect;
        }
    }
}

fn has_ready(side: &[CombatElement]) -> bool {
    side.iter().any(|element| element.state == CombatElementState::Ready)
}

fn ready_cv(side: &[CombatElement]) -> f32 {
    side.iter()
        .filter(|element| element.state == CombatElementState::Ready)
        .map(|element| element.cv * morexp_modifier(element))
        .sum()
}

/// Morale/experience scaling of an element's CV: ×1 at 0/0, ×2 at the 50/50
/// baseline, ×3 at 100/100. Additive (WitE-style) so stats tilt the odds
/// without dwarfing equipment — the multiplicative alternative
/// (mor/100 × exp/100) would hand elite-vs-green a 3.5:1 CV gap on stats
/// alone. Swap this function to try other curves.
fn morexp_modifier(element: &CombatElement) -> f32 {
    1.0 + element.morale as f32 / 100.0 + element.experience as f32 / 100.0
}

fn losses(side: &[CombatElement]) -> Losses {
    let mut result = Losses::default();
    for element in side {
        match element.state {
            CombatElementState::Ready => {}
            CombatElementState::Disrupted => result.disrupted += 1,
            CombatElementState::Damaged => result.damaged += 1,
            CombatElementState::Destroyed => result.destroyed += 1,
        }
    }
    result
}

/// Index into RANGE_BANDS where the battle opens — dense terrain means the
/// sides start closer together.
fn opening_band(terrain: Terrain) -> usize {
    match terrain {
        Terrain::Plains | Terrain::Desert | Terrain::Water => 0,
        Terrain::Hills | Terrain::Forest | Terrain::Swamp | Terrain::Mountain => 2,
        Terrain::Urban => 3,
    }
}

/// Multiplier on hit chances against defenders in this terrain.
fn cover_modifier(terrain: Terrain) -> f32 {
    match terrain {
        Terrain::Plains | Terrain::Desert | Terrain::Water => 1.0,
        Terrain::Hills => 0.8,
        Terrain::Swamp => 0.7,
        Terrain::Forest => 0.6,
        Terrain::Mountain => 0.5,
        Terrain::Urban => 0.4,
    }
}

/// Multiplier on the defender's final CV.
fn terrain_defense(terrain: Terrain) -> f32 {
    match terrain {
        Terrain::Plains | Terrain::Desert | Terrain::Water => 1.0,
        Terrain::Hills => 1.5,
        Terrain::Forest | Terrain::Swamp => 2.0,
        Terrain::Mountain | Terrain::Urban => 3.0,
    }
}

#[derive(Debug)]
pub struct BattleReport {
    pub rounds: Vec<RoundReport>,
    pub attacker_losses: Losses,
    pub defender_losses: Losses,
    pub attacker_cv: f32,
    /// Terrain-modified.
    pub defender_cv: f32,
    pub outcome: BattleOutcome,
}

#[derive(Debug)]
pub struct RoundReport {
    pub range: u32,
    pub attacker_shots: u32,
    /// Effective hits scored by the attacker (shots that changed a state).
    pub attacker_hits: u32,
    pub defender_shots: u32,
    pub defender_hits: u32,
}

#[derive(Debug, Default)]
pub struct Losses {
    pub disrupted: u32,
    pub damaged: u32,
    pub destroyed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BattleOutcome {
    DefenderHolds,
    DefenderRetreats,
}

impl Display for BattleReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for round in &self.rounds {
            writeln!(
                f,
                "Round at {} m — attacker: {} shots, {} hits | defender: {} shots, {} hits",
                round.range,
                round.attacker_shots,
                round.attacker_hits,
                round.defender_shots,
                round.defender_hits,
            )?;
        }
        writeln!(f, "Attacker losses: {}", self.attacker_losses)?;
        writeln!(f, "Defender losses: {}", self.defender_losses)?;
        let odds = if self.defender_cv > 0.0 {
            format!("{:.1}:1", self.attacker_cv / self.defender_cv)
        } else {
            "overrun".to_string()
        };
        writeln!(
            f,
            "Final CV: attacker {:.1} vs defender {:.1} (terrain-modified) — odds {}",
            self.attacker_cv, self.defender_cv, odds,
        )?;
        let outcome = match self.outcome {
            BattleOutcome::DefenderHolds => "defender holds",
            BattleOutcome::DefenderRetreats => "defender retreats",
        };
        write!(f, "Outcome: {}", outcome)
    }
}

impl Display for Losses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} disrupted, {} damaged, {} destroyed",
            self.disrupted, self.damaged, self.destroyed,
        )
    }
}

#[derive(Debug)]
pub struct SimulationReport {
    pub runs: u32,
    /// Battles that ended with the defender forced to retreat.
    pub retreats: u32,
    pub attacker_losses: AverageLosses,
    pub defender_losses: AverageLosses,
    /// Mean final CVs across all runs (defender terrain-modified).
    pub attacker_cv: f32,
    pub defender_cv: f32,
}

#[derive(Debug)]
pub struct AverageLosses {
    pub disrupted: f32,
    pub damaged: f32,
    pub destroyed: f32,
}

impl Display for SimulationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let retreat_percent = 100.0 * self.retreats as f32 / self.runs as f32;
        writeln!(
            f,
            "Simulated {} battles: defender holds {:.0}%, retreats {:.0}%",
            self.runs,
            100.0 - retreat_percent,
            retreat_percent,
        )?;
        writeln!(f, "Average attacker losses: {}", self.attacker_losses)?;
        writeln!(f, "Average defender losses: {}", self.defender_losses)?;
        let odds = if self.defender_cv > 0.0 {
            format!("{:.1}:1", self.attacker_cv / self.defender_cv)
        } else {
            "overrun".to_string()
        };
        write!(
            f,
            "Average final CV: attacker {:.1} vs defender {:.1} (terrain-modified) — odds {}",
            self.attacker_cv, self.defender_cv, odds,
        )
    }
}

impl Display for AverageLosses {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:.1} disrupted, {:.1} damaged, {:.1} destroyed",
            self.disrupted, self.damaged, self.destroyed,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::unit::{ElementInUnit, UnitLocation};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn element_type(
        name: &str,
        class: ElementClass,
        accuracy: u32,
        range: u32,
        cv: f32,
    ) -> Element {
        Element {
            name: name.to_string(),
            class,
            cv,
            accuracy,
            range,
            v_inf: 100,
            v_arm: 3,
        }
    }

    fn unit_with(name: &str, elements: Vec<ElementInUnit>) -> Unit {
        Unit {
            name: name.to_string(),
            toe: "test_toe".to_string(),
            faction: "AX".to_string(),
            location: UnitLocation::Offmap("irrelevant".to_string()),
            elements,
        }
    }

    /// Veterans (morale/experience 100): every element always commits, keeping
    /// the shot counts asserted below deterministic, and the CV modifier is a
    /// flat ×3.
    fn veterans(name: &str, ready: u32, damaged: u32) -> ElementInUnit {
        ElementInUnit {
            name: name.to_string(),
            ready,
            damaged,
            morale: 100,
            experience: 100,
        }
    }

    fn registry(types: Vec<Element>) -> HashMap<String, Element> {
        types.into_iter().map(|e| (e.name.clone(), e)).collect()
    }

    /// A side of `count` rifle-squad-like elements with the given stats.
    fn side(
        count: u32,
        accuracy: u32,
        range: u32,
        cv: f32,
    ) -> Vec<CombatElement> {
        let unit = unit_with(
            "Test Division",
            vec![veterans("squad", count, 0)],
        );
        let types = registry(vec![element_type("squad", ElementClass::Inf, accuracy, range, cv)]);
        combat_elements(&[&unit], &types).unwrap()
    }

    #[test]
    fn snapshot_expands_ready_counts_and_skips_damaged() {
        let unit = unit_with(
            "Test Division",
            vec![veterans("squad", 3, 2)],
        );
        let types = registry(vec![element_type("squad", ElementClass::Inf, 20, 100, 4.0)]);

        let elements = combat_elements(&[&unit], &types).unwrap();

        assert_eq!(elements.len(), 3);
        assert!(elements.iter().all(|e| e.state == CombatElementState::Ready));
        assert!(elements.iter().all(|e| e.unit_name == "Test Division"));
    }

    #[test]
    fn snapshot_rejects_unknown_element_type() {
        let unit = unit_with(
            "Test Division",
            vec![veterans("ghost", 1, 0)],
        );

        let error = combat_elements(&[&unit], &HashMap::new()).unwrap_err();

        assert!(error.error_message.contains("ghost"));
        assert!(error.error_message.contains("Test Division"));
    }

    #[test]
    fn overwhelming_attacker_forces_retreat_and_battle_ends_early() {
        // Perfect accuracy vs v_inf 100 targets: every shot takes effect.
        let mut attackers = side(20, 100, 3000, 10.0);
        let mut defenders = side(2, 0, 3000, 1.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Plains, &mut rng);

        assert_eq!(report.outcome, BattleOutcome::DefenderRetreats);
        // Both defenders go down in round one, so the battle stops there.
        assert_eq!(report.rounds.len(), 1);
        let total_defender_losses = report.defender_losses.disrupted
            + report.defender_losses.damaged
            + report.defender_losses.destroyed;
        assert_eq!(total_defender_losses, 2);
        assert_eq!(report.defender_cv, 0.0);
    }

    #[test]
    fn bloodless_even_battle_ends_in_a_hold() {
        // Zero accuracy: nobody ever hits, CVs stay even, defender holds.
        let mut attackers = side(10, 0, 3000, 4.0);
        let mut defenders = side(10, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Plains, &mut rng);

        assert_eq!(report.outcome, BattleOutcome::DefenderHolds);
        assert_eq!(report.rounds.len(), RANGE_BANDS.len());
        assert!(report.rounds.iter().all(|r| r.attacker_hits == 0 && r.defender_hits == 0));
        // 10 elements × cv 4 × the veterans' ×3 morale/experience modifier.
        assert_eq!(report.attacker_cv, 120.0);
        assert_eq!(report.defender_cv, 120.0);
    }

    #[test]
    fn short_ranged_elements_hold_fire_until_the_range_closes() {
        let mut attackers = side(5, 0, 100, 4.0);
        let mut defenders = side(5, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Plains, &mut rng);

        for round in &report.rounds {
            let expected_attacker_shots = if round.range <= 100 { 5 } else { 0 };
            assert_eq!(round.attacker_shots, expected_attacker_shots);
            assert_eq!(round.defender_shots, 5);
        }
    }

    #[test]
    fn elements_without_experience_never_commit() {
        let mut attackers = side(5, 100, 3000, 4.0);
        for element in &mut attackers {
            element.experience = 0;
        }
        let mut defenders = side(5, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Plains, &mut rng);

        assert!(report.rounds.iter().all(|round| round.attacker_shots == 0));
        assert!(report.rounds.iter().all(|round| round.defender_shots == 5));
    }

    #[test]
    fn urban_battles_open_at_close_range() {
        let mut attackers = side(5, 0, 3000, 4.0);
        let mut defenders = side(5, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report = resolve_battle(&mut attackers, &mut defenders, Terrain::Urban, &mut rng);

        assert_eq!(report.rounds.len(), 2);
        assert_eq!(report.rounds[0].range, 400);
        assert_eq!(report.rounds[1].range, 100);
    }

    #[test]
    fn terrain_multiplies_the_defenders_final_cv() {
        let mut attackers = side(10, 0, 3000, 4.0);
        let mut defenders = side(10, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Mountain, &mut rng);

        assert_eq!(report.attacker_cv, 120.0);
        assert_eq!(report.defender_cv, 360.0);
    }

    #[test]
    fn morale_and_experience_scale_the_final_cv() {
        // Same equipment on both sides, but the defenders are broken recruits:
        // their CV modifier drops from ×3 to ×1 and the 3:1 modified odds
        // force them back without a shot fired.
        let mut attackers = side(10, 0, 3000, 4.0);
        let mut defenders = side(10, 0, 3000, 4.0);
        for element in &mut defenders {
            element.morale = 0;
            element.experience = 0;
        }
        let mut rng = StdRng::seed_from_u64(42);

        let report =
            resolve_battle(&mut attackers, &mut defenders, Terrain::Plains, &mut rng);

        assert_eq!(report.attacker_cv, 120.0);
        assert_eq!(report.defender_cv, 40.0);
        assert_eq!(report.outcome, BattleOutcome::DefenderRetreats);
    }

    #[test]
    fn simulation_aggregates_without_touching_the_snapshots() {
        // Perfect accuracy vs helpless defenders: every run is a retreat.
        let attackers = side(20, 100, 3000, 10.0);
        let defenders = side(2, 0, 3000, 1.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report = simulate_battles(&attackers, &defenders, Terrain::Plains, 50, &mut rng);

        assert_eq!(report.runs, 50);
        assert_eq!(report.retreats, 50);
        // Both defenders go down every run.
        let average_defender_losses = report.defender_losses.disrupted
            + report.defender_losses.damaged
            + report.defender_losses.destroyed;
        assert_eq!(average_defender_losses, 2.0);
        assert_eq!(report.defender_cv, 0.0);
        // The input snapshots stay pristine.
        assert!(attackers.iter().all(|e| e.state == CombatElementState::Ready));
        assert!(defenders.iter().all(|e| e.state == CombatElementState::Ready));
    }

    #[test]
    fn simulation_of_a_bloodless_standoff_reports_all_holds() {
        let attackers = side(10, 0, 3000, 4.0);
        let defenders = side(10, 0, 3000, 4.0);
        let mut rng = StdRng::seed_from_u64(42);

        let report = simulate_battles(&attackers, &defenders, Terrain::Plains, 20, &mut rng);

        assert_eq!(report.retreats, 0);
        assert_eq!(report.attacker_losses.damaged, 0.0);
        assert_eq!(report.attacker_cv, 120.0);
        assert_eq!(report.defender_cv, 120.0);
    }

    #[test]
    fn severity_maps_the_roll_ranges() {
        assert_eq!(severity(0.0), CombatElementState::Disrupted);
        assert_eq!(severity(49.9), CombatElementState::Disrupted);
        assert_eq!(severity(50.0), CombatElementState::Damaged);
        assert_eq!(severity(84.9), CombatElementState::Damaged);
        assert_eq!(severity(85.0), CombatElementState::Destroyed);
        assert_eq!(severity(99.9), CombatElementState::Destroyed);
    }

    #[test]
    fn an_element_hit_twice_keeps_the_worse_effect() {
        let mut elements = side(1, 0, 100, 4.0);
        let hits = vec![
            (0, CombatElementState::Destroyed),
            (0, CombatElementState::Disrupted),
        ];

        apply_hits(&mut elements, &hits);

        assert_eq!(elements[0].state, CombatElementState::Destroyed);
    }
}
