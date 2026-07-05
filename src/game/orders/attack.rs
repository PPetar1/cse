//! The attack order: validating it, handing the battle to the pure combat
//! engine (`procedures::combat`), and persisting the aftermath — losses,
//! experience and morale shifts, retreats/routs/shatters/surrenders, and the
//! attackers' advance into a vacated hex. `simulate` shares the validation
//! so the tuning tool can only ever simulate a legal attack.

use std::fmt::Display;

use rand::Rng;

use crate::Error;
use crate::core::location::Terrain;
use crate::core::unit::{LocationCoords, Toe, Unit, UnitLocation};
use crate::game::Game;
use crate::procedures::combat::{
    self, BattleOutcome, BattleReport, CombatElement, CombatElementState, SimulationReport,
};

/// Retreat attrition: chance (percent) for each ready element of a retreating
/// unit to end up damaged, and for each damaged element to be lost (captured).
const RETREAT_DAMAGE_CHANCE: f32 = 10.0;
const RETREAT_LOSS_CHANCE: f32 = 25.0;

/// A routing unit whose ready strength has fallen below this fraction of its
/// TOE shatters (disintegrates) when a second roll beats its morale.
const SHATTER_STRENGTH_FRACTION: f32 = 0.5;

/// After a battle every participating element bucket gains
/// `ceil((100 - experience) / EXPERIENCE_GAIN_STEP)` experience: green troops
/// learn fast, veterans have little left to learn, 100 caps itself.
const EXPERIENCE_GAIN_STEP: u32 = 10;

/// Morale settles after a battle: the winning side's buckets gain
/// `ceil((100 - morale) / MORALE_SHIFT_STEP)`, the losing side's lose
/// `ceil(morale / MORALE_SHIFT_STEP)` (routed units lose that twice) —
/// tapering toward the 0/100 bounds just like experience gain.
const MORALE_SHIFT_STEP: u32 = 20;

impl Game {
    /// Resolve an attack by all units in the `from` hex against all units in
    /// the `to` hex. Losses are applied to the units, and a lost defender
    /// retreats to an adjacent hex (or surrenders when there is none).
    pub fn attack(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        rng: &mut impl Rng,
    ) -> Result<AttackReport, Error> {
        self.attack_with_air_support(from, to, None, rng)
    }

    /// Fly one owned unit's elements into an attack as extra firers — ground
    /// support, folded into the same battle and resolved together with it
    /// (see docs/combat_design.md). The unit never moves: it isn't part of
    /// the ground stack, doesn't advance into a vacated hex, and returns to
    /// base regardless of outcome.
    pub fn air_support(
        &mut self,
        air_unit: &str,
        from: (u32, u32),
        to: (u32, u32),
        rng: &mut impl Rng,
    ) -> Result<AttackReport, Error> {
        self.attack_with_air_support(from, to, Some(air_unit), rng)
    }

    fn attack_with_air_support(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        air_support: Option<&str>,
        rng: &mut impl Rng,
    ) -> Result<AttackReport, Error> {
        let BattlePlan {
            mut attackers,
            mut defenders,
            defender_terrain,
            attacker_names,
            defender_names,
            defender_faction,
        } = self.prepare_battle(from, to, air_support)?;

        let battle = combat::resolve_battle(&mut attackers, &mut defenders, defender_terrain, rng);

        // Winners and losers alike learn from standing in a battle — granted
        // before losses and retreats reshape (or remove) the rosters.
        self.apply_experience_gain(&attackers);
        self.apply_experience_gain(&defenders);

        self.apply_battle_losses(&attackers);
        self.apply_battle_losses(&defenders);

        let retreat = if battle.outcome == BattleOutcome::DefenderRetreats {
            self.execute_retreat(from, to, &defender_names, &defender_faction, rng)
        } else {
            Vec::new()
        };

        // A beaten defender always clears its hex (retreated, shattered or
        // surrendered), so the winners advance into it — at no MP cost, the
        // battle already paid for the ground (WitE-style advance after combat).
        let advance = if battle.outcome == BattleOutcome::DefenderRetreats {
            for name in &attacker_names {
                let unit = self.state.units.get_mut(name)
                    .expect("attacking unit vanished mid-attack");
                unit.location = UnitLocation::OnMap(LocationCoords { x: to.0, y: to.1 });
            }
            Some(to)
        } else {
            None
        };

        // Morale settles last, once routs are known: winners rally, losers
        // sag, routed units sag a second time. Morale is collective — every
        // bucket of a participating unit shifts, fought or not — so it works
        // by unit name. (Shattered/surrendered units are gone and skipped.)
        let (winners, losers) = match battle.outcome {
            BattleOutcome::DefenderRetreats => (&attacker_names, &defender_names),
            BattleOutcome::DefenderHolds => (&defender_names, &attacker_names),
        };
        self.apply_morale_shift(winners, true);
        self.apply_morale_shift(losers, false);
        for result in &retreat {
            if let UnitRetreat::Retreated { unit, routed: true, .. } = result
                && let Some(unit) = self.state.units.get_mut(unit) {
                    for entry in &mut unit.elements {
                        entry.morale -= morale_loss(entry.morale);
                    }
                }
        }

        Ok(AttackReport { battle, retreat, advance })
    }

    /// Fight the same attack `runs` times without touching the game state and
    /// report the aggregated outcome/loss distributions — the tuning tool.
    pub fn simulate(
        &self,
        from: (u32, u32),
        to: (u32, u32),
        runs: u32,
        rng: &mut impl Rng,
    ) -> Result<SimulationReport, Error> {
        if runs == 0 {
            return Err(Error::new("Number of battles to simulate must be at least 1."));
        }
        let plan = self.prepare_battle(from, to, None)?;
        Ok(combat::simulate_battles(&plan.attackers, &plan.defenders, plan.defender_terrain, runs, rng))
    }

    /// Validate an attack order and build the battle snapshots for it.
    /// Shared by `attack`/`air_support` (which then persist results) and
    /// `simulate` (which never does) — both obey the same rules, adjacency
    /// and turn order included, so a simulation is always of a legal
    /// attack. Future order logic that cares about the source hex or whose
    /// turn it is (reserve activation etc.) belongs here too.
    ///
    /// `air_support`, if given, names one owned unit whose elements join the
    /// attacker snapshot as extra firers without joining `attacker_names` —
    /// see `BattlePlan.air_support_name` and `air_support` above.
    fn prepare_battle(
        &self,
        from: (u32, u32),
        to: (u32, u32),
        air_support: Option<&str>,
    ) -> Result<BattlePlan, Error> {
        let from_location = self.state.map.get_location(from.0, from.1)
            .ok_or_else(|| Error::new("Invalid attacking location."))?;
        let to_location = self.state.map.get_location(to.0, to.1)
            .ok_or_else(|| Error::new("Invalid target location."))?;
        if from_location.distance_to(to_location) != Some(1) {
            return Err(Error::new("Attacks can only target an adjacent hex."));
        }

        // Sorted by name (units_at_location), so the snapshot order — and with
        // it a seeded battle — is deterministic despite HashMap storage.
        let attacker_units = self.units_at_location(from_location);
        let defender_units = self.units_at_location(to_location);

        let attacker_faction = single_faction(&attacker_units, "attacking")?;
        let defender_faction = single_faction(&defender_units, "defending")?;
        if attacker_faction == defender_faction {
            return Err(Error::new("Cannot attack units of the same faction."));
        }
        if attacker_faction != self.player_on_turn().faction_tag {
            return Err(Error::new(format!("It is not {attacker_faction}'s turn.")));
        }

        let mut attackers = combat::combat_elements(&attacker_units, &self.state.elements)?;
        if let Some(name) = air_support {
            if attacker_units.iter().any(|unit| unit.name == name) {
                return Err(Error::new(format!(
                    "'{name}' is already part of the ground attack at ({}, {}).",
                    from.0, from.1,
                )));
            }
            let air_unit = self.state.units.get(name)
                .ok_or_else(|| Error::new(format!("No such unit '{name}' for air support.")))?;
            if air_unit.faction != attacker_faction {
                return Err(Error::new(format!("'{name}' does not belong to {attacker_faction}.")));
            }
            attackers.extend(combat::combat_elements(&[air_unit], &self.state.elements)?);
        }

        Ok(BattlePlan {
            attackers,
            defenders: combat::combat_elements(&defender_units, &self.state.elements)?,
            defender_terrain: to_location.terrain,
            attacker_names: attacker_units.iter().map(|unit| unit.name.clone()).collect(),
            defender_names: defender_units.iter().map(|unit| unit.name.clone()).collect(),
            defender_faction,
        })
    }

    /// Move the beaten defenders out of their hex, with attrition on the way.
    /// All of them go to the same destination; when no valid hex exists the
    /// stack is cut off and surrenders (units removed from the game).
    fn execute_retreat(
        &mut self,
        attacker_hex: (u32, u32),
        defender_hex: (u32, u32),
        defender_names: &[String],
        defender_faction: &str,
        rng: &mut impl Rng,
    ) -> Vec<UnitRetreat> {
        let destination = self.retreat_destination(attacker_hex, defender_hex, defender_faction);

        let mut results = Vec::new();
        for name in defender_names {
            match destination {
                Some((x, y)) => {
                    let unit = self.state.units.get_mut(name)
                        .expect("retreating unit vanished mid-attack");
                    let morale = unit.average_morale() as f32;
                    // Broken morale turns an orderly retreat into a rout:
                    // the attrition rolls happen twice.
                    let routed = rng.random_range(0.0..100.0) >= morale;
                    // A rout can end the unit outright: badly depleted units
                    // that fail a second morale roll disintegrate.
                    let toe = self.state.toe.get(&unit.toe).expect("unit's toe vanished");
                    let shattered = routed
                        && ready_fraction(unit, toe) < SHATTER_STRENGTH_FRACTION
                        && rng.random_range(0.0..100.0) >= morale;
                    if shattered {
                        self.state.units.remove(name);
                        results.push(UnitRetreat::Shattered { unit: name.clone() });
                        continue;
                    }
                    let (mut damaged, mut lost) = retreat_attrition(unit, rng);
                    if routed {
                        let (extra_damaged, extra_lost) = retreat_attrition(unit, rng);
                        damaged += extra_damaged;
                        lost += extra_lost;
                    }
                    unit.location = UnitLocation::OnMap(LocationCoords { x, y });
                    results.push(UnitRetreat::Retreated { unit: name.clone(), to: (x, y), damaged, lost, routed });
                }
                None => {
                    self.state.units.remove(name);
                    results.push(UnitRetreat::Surrendered { unit: name.clone() });
                }
            }
        }
        results
    }

    /// Where a beaten defender goes: an adjacent on-map, non-Water hex free of
    /// enemy units, preferring the one farthest from the attacker. Ties break
    /// on the lowest (x, y) so retreats are deterministic. None = cut off.
    fn retreat_destination(
        &self,
        attacker_hex: (u32, u32),
        defender_hex: (u32, u32),
        defender_faction: &str,
    ) -> Option<(u32, u32)> {
        let attacker_location = self.state.map.get_location(attacker_hex.0, attacker_hex.1)?;
        let defender_location = self.state.map.get_location(defender_hex.0, defender_hex.1)?;

        defender_location.neighbour_coords()
            .into_iter()
            .filter_map(|(x, y)| self.state.map.get_location(x, y).map(|location| ((x, y), location)))
            .filter(|(_, location)| location.terrain != Terrain::Water)
            .filter(|(_, location)| {
                self.units_at_location(location)
                    .iter()
                    .all(|unit| unit.faction == defender_faction)
            })
            .max_by_key(|(coords, location)| {
                (attacker_location.distance_to(location), std::cmp::Reverse(*coords))
            })
            .map(|(coords, _)| coords)
    }

    /// Every element bucket that fielded instances in the battle learns from
    /// it (once per bucket, however many instances fought).
    fn apply_experience_gain(&mut self, elements: &[CombatElement]) {
        let mut seen = std::collections::HashSet::new();
        for element in elements {
            if !seen.insert((&element.unit_name, &element.element_name)) {
                continue;
            }
            if let Some(unit) = self.state.units.get_mut(&element.unit_name)
                && let Some(entry) = unit.elements.iter_mut().find(|e| e.name == element.element_name) {
                    entry.experience +=
                        100u32.saturating_sub(entry.experience).div_ceil(EXPERIENCE_GAIN_STEP);
                }
        }
    }

    /// Post-battle morale for one side's units: winners rally toward 100,
    /// losers sag toward 0. Every bucket of the unit shifts — morale is
    /// collective, unlike the individual experience gain.
    fn apply_morale_shift(&mut self, unit_names: &[String], won: bool) {
        for name in unit_names {
            let Some(unit) = self.state.units.get_mut(name) else {
                continue;
            };
            for entry in &mut unit.elements {
                if won {
                    entry.morale +=
                        100u32.saturating_sub(entry.morale).div_ceil(MORALE_SHIFT_STEP);
                } else {
                    entry.morale -= morale_loss(entry.morale);
                }
            }
        }
    }

    /// Persist battle results: damaged elements move ready → damaged,
    /// destroyed ones are removed for good. Disrupted elements recover and
    /// leave no trace. Each snapshot instance came from one point of `ready`,
    /// so decrementing once per instance cannot underflow.
    fn apply_battle_losses(&mut self, elements: &[CombatElement]) {
        for element in elements {
            let damaged = match element.state {
                CombatElementState::Damaged => true,
                CombatElementState::Destroyed => false,
                CombatElementState::Ready | CombatElementState::Disrupted => continue,
            };
            if let Some(unit) = self.state.units.get_mut(&element.unit_name)
                && let Some(entry) = unit.elements.iter_mut().find(|e| e.name == element.element_name) {
                    entry.ready -= 1;
                    if damaged {
                        entry.damaged += 1;
                    }
                }
        }
    }
}

/// Morale lost from a defeat (or a rout, applied on top): tapers toward 0,
/// and never exceeds the current value, so no clamping is needed.
fn morale_loss(morale: u32) -> u32 {
    morale.div_ceil(MORALE_SHIFT_STEP)
}

/// The unit's ready elements as a fraction of what its TOE prescribes —
/// the strength measure behind the shatter check.
fn ready_fraction(unit: &Unit, toe: &Toe) -> f32 {
    let prescribed: u32 = toe.elements.iter().map(|element| element.amount).sum();
    if prescribed == 0 {
        return 0.0;
    }
    let ready: u32 = unit.elements.iter().map(|element| element.ready).sum();
    ready as f32 / prescribed as f32
}

/// Retreat attrition rolls for one unit: ready elements may end up damaged
/// (RETREAT_DAMAGE_CHANCE), and damaged elements — hard to drag along — may be
/// lost for good (RETREAT_LOSS_CHANCE). Returns (newly damaged, lost).
fn retreat_attrition(unit: &mut Unit, rng: &mut impl Rng) -> (u32, u32) {
    let mut newly_damaged = 0;
    let mut lost = 0;
    for element in &mut unit.elements {
        let mut captured = 0;
        for _ in 0..element.damaged {
            if rng.random_range(0.0..100.0) < RETREAT_LOSS_CHANCE {
                captured += 1;
            }
        }
        element.damaged -= captured;
        lost += captured;

        let mut hurt = 0;
        for _ in 0..element.ready {
            if rng.random_range(0.0..100.0) < RETREAT_DAMAGE_CHANCE {
                hurt += 1;
            }
        }
        element.ready -= hurt;
        element.damaged += hurt;
        newly_damaged += hurt;
    }
    (newly_damaged, lost)
}

/// The one faction the units on a battle side belong to; errors on an empty
/// side or a mixed stack (multi-faction hexes are unsupported for now).
fn single_faction(units: &[&Unit], side: &str) -> Result<String, Error> {
    let first = units.first()
        .ok_or_else(|| Error::new(format!("No units at the {side} hex.")))?;
    if units.iter().any(|unit| unit.faction != first.faction) {
        return Err(Error::new(format!(
            "Units of multiple factions at the {side} hex are not supported.",
        )));
    }
    Ok(first.faction.clone())
}

/// A validated attack order, ready to fight: the two battle snapshots plus
/// what the game layer needs to persist the aftermath.
struct BattlePlan {
    attackers: Vec<CombatElement>,
    defenders: Vec<CombatElement>,
    defender_terrain: Terrain,
    /// Ground attacker names only — deliberately excludes any air-support
    /// unit (see `prepare_battle`), so it never advances into a vacated hex
    /// or shares in the post-battle morale shift.
    attacker_names: Vec<String>,
    defender_names: Vec<String>,
    defender_faction: String,
}

/// Everything one attack command did: the battle itself, what the losing
/// defenders had to do afterwards (empty when the defender held), and the
/// hex the attackers advanced into (None when the defender held).
#[derive(Debug)]
pub struct AttackReport {
    pub battle: BattleReport,
    pub retreat: Vec<UnitRetreat>,
    pub advance: Option<(u32, u32)>,
}

#[derive(Debug, PartialEq)]
pub enum UnitRetreat {
    Retreated { unit: String, to: (u32, u32), damaged: u32, lost: u32, routed: bool },
    Shattered { unit: String },
    Surrendered { unit: String },
}

impl Display for AttackReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.battle)?;
        for retreat in &self.retreat {
            write!(f, "\n{}", retreat)?;
        }
        if let Some((x, y)) = self.advance {
            write!(f, "\nAttackers advance into ({}, {})", x, y)?;
        }
        Ok(())
    }
}

impl Display for UnitRetreat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnitRetreat::Retreated { unit, to, damaged, lost, routed } => write!(
                f,
                "{} {} to ({}, {}) — retreat losses: {} damaged, {} lost",
                unit,
                if *routed { "routs" } else { "retreats" },
                to.0, to.1, damaged, lost,
            ),
            UnitRetreat::Shattered { unit } => {
                write!(f, "{} routs and shatters — the unit disintegrates!", unit)
            }
            UnitRetreat::Surrendered { unit } => {
                write!(f, "{} has nowhere to retreat and surrenders!", unit)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UnitRetreat;
    use crate::core::location::Terrain;
    use crate::core::unit::{LocationCoords, UnitLocation};
    use crate::game::Game;
    use crate::game::test_support::*;
    use crate::procedures::combat::BattleOutcome;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn attack_rejects_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((2, 1), (1, 1), &mut rng).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.attack((2, 1), (1, 1), &mut rng).unwrap();
    }

    #[test]
    fn attack_rejects_a_non_adjacent_target() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 3, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((1, 1), (3, 1), &mut rng).unwrap_err();
        assert!(error.error_message.contains("adjacent"));

        // The tuning tool obeys the same rules — a simulation is always of
        // a legal attack.
        let error = game.simulate((1, 1), (3, 1), 5, &mut rng).unwrap_err();
        assert!(error.error_message.contains("adjacent"));
    }

    #[test]
    fn simulate_rejects_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.simulate((2, 1), (1, 1), 5, &mut rng).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.simulate((2, 1), (1, 1), 5, &mut rng).unwrap();
    }

    #[test]
    fn attack_applies_losses_that_match_the_report() {
        // Two defending units vs one attacker: the defender holds, so no
        // retreat attrition muddies the loss bookkeeping.
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet First"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }

[[units]]
name = "Soviet Second"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert!(!report.battle.rounds.is_empty());
        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        assert!(report.retreat.is_empty());
        // A held hex is not entered.
        assert_eq!(report.advance, None);
        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 1, y: 1 }),
        );
        // The defenders started with 20 ready elements between them; whatever
        // the dice did, the persisted counts must match the report exactly.
        let defenders = [
            &game.state.units["Soviet First"].elements[0],
            &game.state.units["Soviet Second"].elements[0],
        ];
        let damaged: u32 = defenders.iter().map(|e| e.damaged).sum();
        let remaining: u32 = defenders.iter().map(|e| e.ready + e.damaged).sum();
        assert_eq!(damaged, report.battle.defender_losses.damaged);
        assert_eq!(20 - remaining, report.battle.defender_losses.destroyed);
        let attacker = &game.state.units["Axis Division"].elements[0];
        assert_eq!(attacker.damaged, report.battle.attacker_losses.damaged);
        assert_eq!(10 - attacker.ready - attacker.damaged, report.battle.attacker_losses.destroyed);
    }

    #[test]
    fn a_lost_battle_forces_a_retreat_to_an_adjacent_hex() {
        // Three divisions against one: the defender loses and must retreat.
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(100))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        let [UnitRetreat::Retreated { unit, to, routed, .. }] = &report.retreat[..] else {
            panic!("expected exactly one retreated unit, got {:?}", report.retreat);
        };
        assert_eq!(unit, "Soviet Division");
        // Morale 100 never routs.
        assert!(!routed);
        assert_ne!(*to, (1, 1));

        let battle_hex = game.state.map.get_location(2, 1).unwrap();
        let destination = game.state.map.get_location(to.0, to.1)
            .expect("retreat destination must be on the map");
        assert_eq!(battle_hex.distance_to(destination), Some(1));
        assert_ne!(destination.terrain, Terrain::Water);
        assert_eq!(
            game.state.units["Soviet Division"].location,
            UnitLocation::OnMap(LocationCoords { x: to.0, y: to.1 }),
        );
    }

    #[test]
    fn attackers_advance_into_the_vacated_hex_for_free() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(100))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        assert_eq!(report.advance, Some((2, 1)));
        for name in ["Axis First", "Axis Second", "Axis Third"] {
            let unit = &game.state.units[name];
            assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }));
            // Advance after combat costs no movement points.
            assert_eq!(unit.mp_left, 16);
        }
    }

    #[test]
    fn a_defender_with_broken_morale_routs() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(0))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // Morale 0 always routs when forced back — but a unit still near
        // full strength never shatters, it stays in the game.
        assert!(matches!(
            report.retreat[..],
            [UnitRetreat::Retreated { routed: true, .. }],
        ));
        assert!(game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn a_routed_understrength_defender_shatters() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(0))).unwrap();
        // Already mauled: 2 of 10 TOE elements ready — far below the shatter
        // threshold. Morale 0 fails both the rout and the shatter roll.
        game.state.units.get_mut("Soviet Division").unwrap().elements[0].ready = 2;
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(
            report.retreat,
            vec![UnitRetreat::Shattered { unit: "Soviet Division".to_string() }],
        );
        assert!(!game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn battles_grant_experience_to_both_sides() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // Both start at the default 50: gain is ceil(50 / 10) = 5.
        assert_eq!(game.state.units["Axis Division"].elements[0].experience, 55);
        assert_eq!(game.state.units["Soviet Division"].elements[0].experience, 55);
    }

    #[test]
    fn battles_shift_morale_toward_the_victor() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // 1v1 with this seed the defender holds and wins the battle.
        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        // Both start at the default 50; shift is ceil(50 / 20) = 3 each way.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 53);
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 47);
    }

    #[test]
    fn morale_shifts_stop_at_the_bounds() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 0

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderHolds);
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 0);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 100);
    }

    #[test]
    fn morale_shifts_reach_buckets_that_could_not_fight() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        // The defender has nothing ready to fight with, so it fields no
        // combat elements at all — but morale is collective, and losing the
        // hex must still sag it.
        let bucket = &mut game.state.units.get_mut("Soviet Division").unwrap().elements[0];
        bucket.ready = 0;
        bucket.damaged = 10;
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        // Defeat: 100 - ceil(100/20) = 95; morale 100 never routs.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 95);
    }

    #[test]
    fn a_routed_unit_loses_morale_twice() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(40))).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        // With this seed the outnumbered defender is forced back and routs.
        assert!(matches!(
            report.retreat[..],
            [UnitRetreat::Retreated { routed: true, .. }],
        ));
        // Defeat: 40 - ceil(40/20) = 38, then the rout: 38 - ceil(38/20) = 36.
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 36);
    }

    #[test]
    fn experience_gain_tapers_off_and_caps_at_100() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
experience = 98

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
experience = 100
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(game.state.units["Axis Division"].elements[0].experience, 99);
        assert_eq!(game.state.units["Soviet Division"].elements[0].experience, 100);
    }

    #[test]
    fn simulate_reports_statistics_without_changing_the_game() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.simulate((1, 1), (2, 1), 25, &mut rng).unwrap();

        assert_eq!(report.runs, 25);
        assert!(report.retreats <= 25);
        // Nothing happened to the real units.
        for name in ["Axis Division", "Soviet Division"] {
            let element = &game.state.units[name].elements[0];
            assert_eq!((element.ready, element.damaged), (10, 0));
        }

        // And the mutable path still works afterwards.
        game.attack((1, 1), (2, 1), &mut rng).unwrap();
    }

    #[test]
    fn simulate_rejects_zero_runs() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.simulate((1, 1), (2, 1), 0, &mut rng).unwrap_err();

        assert!(error.error_message.contains("at least 1"));
    }

    #[test]
    fn a_surrounded_defender_surrenders() {
        // Every neighbour of the defender's hex is occupied by the enemy
        // (except Water, which is no escape route anyway): nowhere to go.
        let mut units = String::from(r#"
[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 2 }
"#);
        for i in 0..8 {
            units.push_str(&format!(r#"
[[units]]
name = "Axis Division {i}"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
"#));
        }
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        let escape_hexes: Vec<(u32, u32)> = game.state.map.get_location(2, 2).unwrap()
            .neighbour_coords()
            .into_iter()
            .filter(|(x, y)| {
                game.state.map.get_location(*x, *y)
                    .is_some_and(|location| location.terrain != Terrain::Water)
            })
            .collect();
        // Three attackers stacked on the first neighbour (to guarantee the
        // battle is lost), one blocker on each remaining one.
        let mut placements = vec![escape_hexes[0]; 3];
        placements.extend(&escape_hexes[1..]);
        for (i, (x, y)) in placements.iter().enumerate() {
            game.state.units.get_mut(&format!("Axis Division {i}")).unwrap().location =
                UnitLocation::OnMap(LocationCoords { x: *x, y: *y });
        }

        let mut rng = StdRng::seed_from_u64(42);
        let report = game.attack(escape_hexes[0], (2, 2), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        assert_eq!(
            report.retreat,
            vec![UnitRetreat::Surrendered { unit: "Soviet Division".to_string() }],
        );
        assert!(!game.state.units.contains_key("Soviet Division"));
    }

    #[test]
    fn attack_rejects_an_empty_source_hex() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        // (3, 1) is adjacent to the target but empty — past the adjacency
        // gate, the empty stack is the complaint.
        let error = game.attack((3, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("No units at the attacking hex"));
    }

    #[test]
    fn attack_rejects_attacking_the_same_faction() {
        let units = r#"
[[units]]
name = "First Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Second Division"
toe = "test_toe"
faction = "AX"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.attack((1, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("same faction"));
    }

    #[test]
    fn air_support_adds_the_air_units_elements_to_the_attack() {
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 100
experience = 100

[[units]]
name = "3rd Stuka Wing"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
morale = 100
experience = 100

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 100
experience = 100
"#;

        // Ground alone: 2 elements against 10 is hopeless, the attack holds.
        let mut baseline = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        baseline.state.units.get_mut("Axis Division").unwrap().elements[0].ready = 2;
        let holds = baseline.attack((1, 1), (2, 1), &mut StdRng::seed_from_u64(42)).unwrap();
        assert_eq!(holds.battle.outcome, BattleOutcome::DefenderHolds);

        // With the Stuka wing's 30 elements added in, the same odds flip.
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
        game.state.units.get_mut("Axis Division").unwrap().elements[0].ready = 2;
        game.state.units.get_mut("3rd Stuka Wing").unwrap().elements[0].ready = 30;
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.air_support("3rd Stuka Wing", (1, 1), (2, 1), &mut rng).unwrap();

        assert_eq!(report.battle.outcome, BattleOutcome::DefenderRetreats);
        // Far more attacker CV than the 2-element ground stack alone could
        // ever produce (max 2 × 4 × 3 = 24) — proof the wing's 30 elements
        // are really in the snapshot, not just along for the ride.
        assert!(report.battle.attacker_cv > 24.0);
        // It joined the fight but never left base — it isn't part of the
        // ground stack.
        assert_eq!(
            game.state.units["3rd Stuka Wing"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );
        // The ground attackers, in contrast, advanced into the vacated hex.
        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }),
        );
    }

    #[test]
    fn air_support_rejects_an_unknown_unit_name() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.air_support("Ghost Wing", (1, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("No such unit"));
    }

    #[test]
    fn air_support_rejects_a_unit_of_the_wrong_faction() {
        let units = format!("{OPPOSING_UNITS}\n{OFFMAP_UNIT}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        game.end_turn(); // Soviet Union now on turn.
        let mut rng = StdRng::seed_from_u64(42);

        // "Reserve Division" belongs to AX; SU is attacking.
        let error = game.air_support("Reserve Division", (2, 1), (1, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("does not belong to"));
    }

    #[test]
    fn air_support_rejects_a_unit_already_in_the_ground_stack() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let error = game.air_support("Axis Division", (1, 1), (2, 1), &mut rng).unwrap_err();

        assert!(error.error_message.contains("already part of"));
    }
}
