use std::fmt::Display;

use rand::Rng;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::{Location, Terrain};
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Game {
    pub state: State,
    players: Vec<Player>,
    turn: u32,
    phase: TurnPhase,
}

impl Game {
    pub fn build(scenario_toml: String) -> Result<Game, Error> {
       Game::parse_scen_from_toml(scenario_toml) 
    }

    fn parse_scen_from_toml(scenario_toml: String) -> Result<Game, Error>  {
       let scenario: Scenario = toml::from_str(&scenario_toml)?;

       if scenario.players.is_empty() {
           return Err(Error::new("The game must have at least 1 player."))
       }
       
       let players = scenario.players.clone();

       let state = State::build(scenario)?;
       
       let game = Game {
           state,
           players,
           turn: 1,
           phase: TurnPhase { player_on_turn: 0 },
       };

       Ok(game)
    }

    pub fn load() -> Result<Game, Error> {
        Err(Error { error_message: "Not implemented yet.".to_string() })
    }

    pub fn list_units(&self) {
        for unit in self.state.units.values() {
            println!("{}", unit);
        }
    }

    pub fn list_units_detail(&self) {
        for unit in self.state.units.values() {
            println!("{:?}", unit);
        }
    }
    
    /// Units at a location, sorted by name. Sorting matters: the unit index
    /// used by `move_unit` must be stable across calls (HashMap iteration
    /// order is not), and must match what `inspect` shows the player.
    pub fn units_at_location(&self, location: &Location) -> Vec<&Unit> {
        let mut units = Vec::new();
        for unit in self.state.units.values() {
            let units_location = match &unit.location {
                UnitLocation::OnMap(coords) => self.state.map.get_location(coords.x, coords.y),
                UnitLocation::Offmap(name) => self.state.map.get_offmap_location(name),
            };
            if Some(location) == units_location {
                units.push(unit)
            }
        }
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }

    pub fn move_unit(&mut self, x_start: u32, y_start: u32, x_end: u32, y_end: u32, unit_i: usize) -> Result<(), Error> {
        self.state.map.get_location(x_start, y_start).ok_or(Error {
            error_message: "Invalid starting location.".to_string(),
        })?;
        self.state.map.get_location(x_end, y_end).ok_or(Error {
                error_message: "Invalid destination.".to_string(),
        })?;
       
        let location_start = UnitLocation::OnMap(LocationCoords { x: x_start, y: y_start });
        let mut units = Vec::new();
        for unit in self.state.units.values_mut() {
            if unit.location == location_start {
                units.push(unit)
            }
        }
        // Same ordering as units_at_location, so the index the player saw
        // via inspect addresses the same unit here.
        units.sort_by(|a, b| a.name.cmp(&b.name));

        if units.len() < unit_i + 1 {
            return Err(Error {
                error_message: format!("No unit with index {} at ({}, {}).", unit_i, x_start, y_start),
            })
        }

        units[unit_i].location = UnitLocation::OnMap(LocationCoords { x: x_end, y: y_end });

        Ok(())
    }

    /// Resolve an attack by all units in the `from` hex against all units in
    /// the `to` hex. Losses are applied to the units, and a lost defender
    /// retreats to an adjacent hex (or surrenders when there is none).
    pub fn attack(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        rng: &mut impl Rng,
    ) -> Result<AttackReport, Error> {
        let BattlePlan { mut attackers, mut defenders, defender_terrain, defender_names, defender_faction } =
            self.prepare_battle(from, to)?;

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

        // Morale settles last, once routs are known: winners rally, losers
        // sag, routed units sag a second time. (Shattered/surrendered units
        // are already gone and are skipped.)
        let (winners, losers) = match battle.outcome {
            BattleOutcome::DefenderRetreats => (&attackers, &defenders),
            BattleOutcome::DefenderHolds => (&defenders, &attackers),
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

        Ok(AttackReport { battle, retreat })
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
        let plan = self.prepare_battle(from, to)?;
        Ok(combat::simulate_battles(&plan.attackers, &plan.defenders, plan.defender_terrain, runs, rng))
    }

    /// Validate an attack order and build the battle snapshots for it.
    /// Shared by `attack` (which then persists results) and `simulate`
    /// (which never does).
    fn prepare_battle(&self, from: (u32, u32), to: (u32, u32)) -> Result<BattlePlan, Error> {
        let from_location = self.state.map.get_location(from.0, from.1)
            .ok_or_else(|| Error::new("Invalid attacking location."))?;
        let to_location = self.state.map.get_location(to.0, to.1)
            .ok_or_else(|| Error::new("Invalid target location."))?;

        // Sorted by name (units_at_location), so the snapshot order — and with
        // it a seeded battle — is deterministic despite HashMap storage.
        let attacker_units = self.units_at_location(from_location);
        let defender_units = self.units_at_location(to_location);

        let attacker_faction = single_faction(&attacker_units, "attacking")?;
        let defender_faction = single_faction(&defender_units, "defending")?;
        if attacker_faction == defender_faction {
            return Err(Error::new("Cannot attack units of the same faction."));
        }

        Ok(BattlePlan {
            attackers: combat::combat_elements(&attacker_units, &self.state.elements)?,
            defenders: combat::combat_elements(&defender_units, &self.state.elements)?,
            defender_terrain: to_location.terrain,
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

    /// Post-battle morale for one side's participating buckets (once per
    /// bucket): winners rally toward 100, losers sag toward 0.
    fn apply_morale_shift(&mut self, elements: &[CombatElement], won: bool) {
        let mut seen = std::collections::HashSet::new();
        for element in elements {
            if !seen.insert((&element.unit_name, &element.element_name)) {
                continue;
            }
            if let Some(unit) = self.state.units.get_mut(&element.unit_name)
                && let Some(entry) = unit.elements.iter_mut().find(|e| e.name == element.element_name) {
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

/// A validated attack order, ready to fight: the two battle snapshots plus
/// what the game layer needs to persist the aftermath.
struct BattlePlan {
    attackers: Vec<CombatElement>,
    defenders: Vec<CombatElement>,
    defender_terrain: Terrain,
    defender_names: Vec<String>,
    defender_faction: String,
}

/// Everything one attack command did: the battle itself, plus what the losing
/// defenders had to do afterwards (empty when the defender held).
#[derive(Debug)]
pub struct AttackReport {
    pub battle: BattleReport,
    pub retreat: Vec<UnitRetreat>,
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

/// The one faction the units on a battle side belong to; errors on an empty
/// side or a mixed stack (multi-faction hexes are unsupported for now).
fn single_faction(units: &[&Unit], side: &str) -> Result<String, Error> {
    let first = units.first()
        .ok_or_else(|| Error::new(&format!("No units at the {side} hex.")))?;
    if units.iter().any(|unit| unit.faction != first.faction) {
        return Err(Error::new(&format!(
            "Units of multiple factions at the {side} hex are not supported.",
        )));
    }
    Ok(first.faction.clone())
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Player {
    faction_name: String,
    pub faction_tag: String,
    /// Faction-wide default morale/experience, inherited by every element of
    /// the faction's units unless the unit or element sets its own. Lives on
    /// the runtime player so future events can shift it over time.
    #[serde(default = "default_stat")]
    pub morale: u32,
    #[serde(default = "default_stat")]
    pub experience: u32,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct TurnPhase {
    player_on_turn: u32,
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    #[allow(dead_code)] // will be read once the turn system lands
    name: String,
    #[allow(dead_code)]
    game_version: String,
    pub map: String,

    #[allow(dead_code)]
    start_date: String,
    #[allow(dead_code)]
    turn_length: u32,

    pub players: Vec<Player>,

    pub toe: Vec<Toe>,
    
    pub elements: Vec<Element>,

    pub units: Vec<UnitConfig>,
}

#[derive(serde::Deserialize)]
pub struct UnitConfig {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: UnitLocationConfig,
    /// Unit-wide morale/experience, inherited by all its elements. Absent =
    /// the faction default from [[players]].
    pub morale: Option<u32>,
    pub experience: Option<u32>,
    /// Per-element stat overrides ([[units.elements]]), the most specific
    /// setting. Names must exist in the unit's TOE.
    #[serde(default)]
    pub elements: Vec<ElementStatsConfig>,
}

#[derive(serde::Deserialize)]
pub struct ElementStatsConfig {
    pub name: String,
    pub morale: Option<u32>,
    pub experience: Option<u32>,
}

/// Factions that don't specify default morale/experience get an average rating.
fn default_stat() -> u32 {
    50
}

/// Scenario-file form of a unit location. Untagged so the TOML reads naturally:
/// `location = { x = 3, y = 3 }` for a hex, `location = "GE Reserve"` for offmap.
#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum UnitLocationConfig {
    OnMap { x: u32, y: u32 },
    Offmap(String),
}

impl From<UnitLocationConfig> for UnitLocation {
    fn from(config: UnitLocationConfig) -> UnitLocation {
        match config {
            UnitLocationConfig::OnMap { x, y } => UnitLocation::OnMap(LocationCoords { x, y }),
            UnitLocationConfig::Offmap(name) => UnitLocation::Offmap(name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    const ONE_PLAYER: &str = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
"#;

    const TWO_PLAYERS: &str = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"
"#;

    const OPPOSING_UNITS: &str = r#"
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
"#;

    const ONMAP_UNIT: &str = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;

    const OFFMAP_UNIT: &str = r#"
[[units]]
name = "Reserve Division"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
"#;

    fn minimal_scenario(players: &str, units: &str) -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7
{players}

[[toe]]
name = "test_toe"
size = "Division"
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

{units}
"#)
    }

    fn one_unit_game() -> Game {
        Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap()
    }

    #[test]
    fn builds_a_game_from_a_minimal_scenario() {
        let game = one_unit_game();

        assert_eq!(game.turn, 1);
        assert_eq!(game.players.len(), 1);
        assert_eq!(game.players[0].faction_tag, "AX");
        assert_eq!(game.state.units.len(), 1);
    }

    #[test]
    fn rejects_a_scenario_with_no_players() {
        let error = Game::build(minimal_scenario("players = []", ONMAP_UNIT)).unwrap_err();

        assert!(error.error_message.contains("at least 1 player"));
    }

    #[test]
    fn move_unit_updates_the_units_location() {
        let mut game = one_unit_game();

        game.move_unit(1, 1, 2, 2, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 2 }));
    }

    #[test]
    fn move_unit_rejects_invalid_start_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(99, 99, 2, 2, 0).unwrap_err();
        assert!(error.error_message.contains("starting location"));
    }

    #[test]
    fn move_unit_rejects_invalid_destination_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 99, 99, 0).unwrap_err();
        assert!(error.error_message.contains("destination"));
    }

    #[test]
    fn move_unit_rejects_index_with_no_unit() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 2, 2, 5).unwrap_err();
        assert!(error.error_message.contains("index 5"));
        assert!(error.error_message.contains("(1, 1)"));
    }

    #[test]
    fn units_at_location_finds_onmap_and_offmap_units() {
        let units = format!("{ONMAP_UNIT}\n{OFFMAP_UNIT}");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "1st Test Division");

        let reserve = game.state.map.get_offmap_location("GE Reserve").unwrap();
        let found = game.units_at_location(reserve);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Reserve Division");
    }

    #[test]
    fn stacked_units_are_indexed_in_name_order() {
        let units = r#"
[[units]]
name = "Bravo Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Alpha Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Charlie Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        let names: Vec<&str> = found.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(names, ["Alpha Division", "Bravo Division", "Charlie Division"]);

        // Index 1 must address the same unit move_unit sees: Bravo.
        game.move_unit(1, 1, 2, 2, 1).unwrap();
        assert_eq!(
            game.state.units["Bravo Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 2 })
        );
        assert_eq!(
            game.state.units["Alpha Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 1, y: 1 })
        );
    }

    #[test]
    fn units_at_location_returns_empty_for_an_empty_hex() {
        let game = one_unit_game();

        let hex = game.state.map.get_location(0, 0).unwrap();
        assert!(game.units_at_location(hex).is_empty());
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
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Third"
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
    fn unit_stats_come_from_the_scenario_with_defaults() {
        let units = r#"
[[units]]
name = "Rated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 80
experience = 65

[[units]]
name = "Unrated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // Stats live on the elements; a unit-level scenario setting is
        // inherited by all of them.
        let rated = &game.state.units["Rated Division"].elements[0];
        assert_eq!((rated.morale, rated.experience), (80, 65));
        // No unit or faction setting: the default rating.
        let unrated = &game.state.units["Unrated Division"].elements[0];
        assert_eq!((unrated.morale, unrated.experience), (50, 50));
    }

    #[test]
    fn a_defender_with_broken_morale_routs() {
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 0
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
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
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 0
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
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
    fn a_routed_unit_loses_morale_twice() {
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 40
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();
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

        let error = game.attack((0, 0), (2, 1), &mut rng).unwrap_err();

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
    fn builds_the_real_basic_scenario() {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen"),
        ).unwrap();
        let game = Game::build(contents).unwrap();

        assert_eq!(game.players.len(), 2);
        assert_eq!(game.state.units.len(), 3);
        // Guards the TOE/element referential integrity of the shipped scenario.
        assert!(game.state.elements.contains_key("SU_45mm_at_gun"));
        // Morale/experience inheritance: the 101st takes the Soviet faction
        // defaults, except its howitzer crews' experience override.
        let infantry = &game.state.units["101st Infantry division"];
        let squads = infantry.elements.iter().find(|e| e.name == "SU_inf_squad").unwrap();
        assert_eq!((squads.morale, squads.experience), (45, 35));
        let howitzers = infantry.elements.iter()
            .find(|e| e.name == "SU_122mm_howitzer_M1938").unwrap();
        assert_eq!((howitzers.morale, howitzers.experience), (45, 55));
    }
}
