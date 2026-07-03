pub mod location;
mod map;
pub mod unit;

use std::{collections::HashMap, fs::File, io::Read};

use crate::{Error, game::{Player, Scenario}};

use map::Map;
use unit::*;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct State {
    pub map: Map,
    pub units: HashMap<String, Unit>,
    pub toe: HashMap<String, Toe>,
    pub elements: HashMap<String, Element>,
}

impl State {
    pub fn build(scenario: Scenario) -> Result<State, Error> {
        let mut map_file = File::open(&scenario.map)?;
        let mut contents = String::new();
        map_file.read_to_string(&mut contents)?;

        let map = Map::map_from_string(&contents)?;
        
        let mut units = HashMap::new();
        let mut toe = HashMap::new();
        let mut elements = HashMap::new();
       
        for element in scenario.elements {
            if element.devices.is_empty() {
                return Err(Error {
                    error_message: format!("Element '{}' has no devices.", element.name),
                });
            }
            elements.insert(element.name.clone(), element);
        }

        for toe_ in scenario.toe {
            for element_in_toe in &toe_.elements {
                if !elements.contains_key(&element_in_toe.name) {
                    return Err(Error {
                        error_message: format!(
                            "Toe '{}' references element '{}' which is not defined in the scenario.",
                            toe_.name, element_in_toe.name
                        ),
                    });
                }
            }
            toe.insert(toe_.name.clone(), toe_);
        }

        let players_by_tag: HashMap<&str, &Player> = scenario.players.iter()
            .map(|player| (player.faction_tag.as_str(), player))
            .collect();

        for unit in scenario.units {
            let player = players_by_tag.get(unit.faction.as_str()).ok_or_else(|| Error {
                error_message: format!(
                    "Unit '{}' belongs to faction '{}' which has no player.",
                    unit.name, unit.faction
                ),
            })?;
            let unit_toe = toe.get(&unit.toe).ok_or_else(|| Error {
                error_message: format!("Unit '{}' has a toe '{}' that cannot be found.", unit.name, unit.toe),
            })?;
            for stats in &unit.elements {
                if !unit_toe.elements.iter().any(|e| e.name == stats.name) {
                    return Err(Error {
                        error_message: format!(
                            "Unit '{}' overrides stats of element '{}' which is not in its toe '{}'.",
                            unit.name, stats.name, unit.toe
                        ),
                    });
                }
            }

            let mut elements = Vec::new();
            for element_in_toe in &unit_toe.elements {
                // Morale/experience: the most specific scenario setting wins —
                // element override, then the unit, then the faction default.
                let stats = unit.elements.iter().find(|s| s.name == element_in_toe.name);
                elements.push(ElementInUnit {
                    name: element_in_toe.name.clone(),
                    ready: element_in_toe.amount,
                    damaged: 0,
                    morale: stats.and_then(|s| s.morale).or(unit.morale).unwrap_or(player.morale),
                    experience: stats.and_then(|s| s.experience).or(unit.experience).unwrap_or(player.experience),
                });
            }

            units.insert(unit.name.clone(), Unit {
                name: unit.name,
                toe: unit.toe,
                faction: unit.faction,
                location: unit.location.into(),
                elements,
            });
        }

        Ok(State {
            map,
            units,
            toe,
            elements,
        })
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    // Uses the real map file so fixtures only have to vary scenario data.
    fn scenario_toml(players: &str, toe_elements: &str, units: &str) -> String {
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
{toe_elements}

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

[[elements]]
name = "second_element"
class = "AtGun"
cv = 0.5
vulnerability = 60
[[elements.devices]]
name = "test_at_gun"
accuracy = 60
range = 300
rate_of_fire = 2
soft_attack = 15
hard_attack = 90

{units}
"#)
    }

    const DEFAULT_PLAYERS: &str = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
"#;

    fn build_state_with_players(players: &str, toe_elements: &str, units: &str) -> Result<State, Error> {
        let scenario: Scenario = toml::from_str(&scenario_toml(players, toe_elements, units)).unwrap();
        State::build(scenario)
    }

    fn build_state(toe_elements: &str, units: &str) -> Result<State, Error> {
        build_state_with_players(DEFAULT_PLAYERS, toe_elements, units)
    }

    const VALID_TOE_ELEMENTS: &str = r#"
[[toe.elements]]
name = "test_element"
amount = 10
"#;

    #[test]
    fn unit_elements_are_instantiated_from_its_toe() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let state = build_state(VALID_TOE_ELEMENTS, units).unwrap();

        let unit = &state.units["1st Test Division"];
        assert_eq!(unit.elements.len(), 1);
        assert_eq!(unit.elements[0].name, "test_element");
        assert_eq!(unit.elements[0].ready, 10);
        assert_eq!(unit.elements[0].damaged, 0);
    }

    #[test]
    fn offmap_unit_location_is_preserved() {
        let units = r#"
[[units]]
name = "Reserve Division"
toe = "test_toe"
faction = "AX"
location = "GE Reserve"
"#;
        let state = build_state(VALID_TOE_ELEMENTS, units).unwrap();

        let unit = &state.units["Reserve Division"];
        assert_eq!(unit.location, UnitLocation::Offmap("GE Reserve".to_string()));
    }

    #[test]
    fn unit_with_unknown_toe_is_rejected() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "no_such_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let error = build_state(VALID_TOE_ELEMENTS, units).unwrap_err();

        assert!(error.error_message.contains("no_such_toe"));
        assert!(error.error_message.contains("1st Test Division"));
    }

    #[test]
    fn toe_referencing_unknown_element_is_rejected() {
        let toe_elements = r#"
[[toe.elements]]
name = "missing_element"
amount = 10
"#;
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let error = build_state(toe_elements, units).unwrap_err();

        assert!(error.error_message.contains("missing_element"));
        assert!(error.error_message.contains("test_toe"));
    }

    #[test]
    fn element_without_devices_is_rejected() {
        // Piggybacks on the units slot to append an extra element block.
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[elements]]
name = "unarmed_element"
class = "Inf"
cv = 1.0
vulnerability = 100
devices = []
"#;
        let error = build_state(VALID_TOE_ELEMENTS, units).unwrap_err();

        assert!(error.error_message.contains("unarmed_element"));
        assert!(error.error_message.contains("no devices"));
    }

    #[test]
    fn element_stats_inherit_the_most_specific_setting() {
        let players = r#"
[[players]]
faction_name = "Axis"
faction_tag = "AX"
morale = 60
experience = 40
"#;
        let toe_elements = r#"
[[toe.elements]]
name = "test_element"
amount = 10
[[toe.elements]]
name = "second_element"
amount = 5
"#;
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 70

[[units.elements]]
name = "second_element"
experience = 90
"#;
        let state = build_state_with_players(players, toe_elements, units).unwrap();

        let elements = &state.units["1st Test Division"].elements;
        // Unit morale beats the faction's; experience falls through to the faction.
        assert_eq!((elements[0].name.as_str(), elements[0].morale, elements[0].experience),
            ("test_element", 70, 40));
        // The element override beats both for experience.
        assert_eq!((elements[1].name.as_str(), elements[1].morale, elements[1].experience),
            ("second_element", 70, 90));
    }

    #[test]
    fn unit_with_unknown_faction_is_rejected() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "ZZ"
location = { x = 1, y = 1 }
"#;
        let error = build_state(VALID_TOE_ELEMENTS, units).unwrap_err();

        assert!(error.error_message.contains("ZZ"));
        assert!(error.error_message.contains("1st Test Division"));
    }

    #[test]
    fn stat_override_for_an_element_not_in_the_toe_is_rejected() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units.elements]]
name = "ghost_element"
morale = 90
"#;
        let error = build_state(VALID_TOE_ELEMENTS, units).unwrap_err();

        assert!(error.error_message.contains("ghost_element"));
        assert!(error.error_message.contains("test_toe"));
    }
}
