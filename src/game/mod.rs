use rand::Rng;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::Location;
use crate::procedures::combat::{self, BattleReport, CombatElement, CombatElementState};

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
    /// the `to` hex. Losses are applied to the units; the returned report says
    /// what happened. No unit movement yet — a "defender retreats" outcome is
    /// reported but not executed (needs adjacency/stacking rules first).
    pub fn attack(
        &mut self,
        from: (u32, u32),
        to: (u32, u32),
        rng: &mut impl Rng,
    ) -> Result<BattleReport, Error> {
        let from_location = self.state.map.get_location(from.0, from.1)
            .ok_or_else(|| Error::new("Invalid attacking location."))?;
        let to_location = self.state.map.get_location(to.0, to.1)
            .ok_or_else(|| Error::new("Invalid target location."))?;
        let defender_terrain = to_location.terrain;

        // Sorted by name (units_at_location), so the snapshot order — and with
        // it a seeded battle — is deterministic despite HashMap storage.
        let attacker_units = self.units_at_location(from_location);
        let defender_units = self.units_at_location(to_location);

        let attacker_faction = single_faction(&attacker_units, "attacking")?;
        let defender_faction = single_faction(&defender_units, "defending")?;
        if attacker_faction == defender_faction {
            return Err(Error::new("Cannot attack units of the same faction."));
        }

        let mut attackers = combat::combat_elements(&attacker_units, &self.state.elements)?;
        let mut defenders = combat::combat_elements(&defender_units, &self.state.elements)?;

        let report = combat::resolve_battle(&mut attackers, &mut defenders, defender_terrain, rng);

        self.apply_battle_losses(&attackers);
        self.apply_battle_losses(&defenders);

        Ok(report)
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
struct Player {
    faction_name: String,
    faction_tag: String,
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

    players: Vec<Player>,

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
accuracy = 20
range = 100
v_inf = 100
v_arm = 3

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
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        let mut rng = StdRng::seed_from_u64(42);

        let report = game.attack((1, 1), (2, 1), &mut rng).unwrap();

        assert!(!report.rounds.is_empty());
        // Each side started with 10 ready elements; whatever the dice did,
        // the persisted counts must match the report exactly.
        let defender = &game.state.units["Soviet Division"].elements[0];
        assert_eq!(defender.damaged, report.defender_losses.damaged);
        assert_eq!(10 - defender.ready - defender.damaged, report.defender_losses.destroyed);
        let attacker = &game.state.units["Axis Division"].elements[0];
        assert_eq!(attacker.damaged, report.attacker_losses.damaged);
        assert_eq!(10 - attacker.ready - attacker.damaged, report.attacker_losses.destroyed);
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
    }
}
