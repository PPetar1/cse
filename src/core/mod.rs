pub mod location;
mod map;
pub mod unit;

use std::{collections::HashMap, fs::File, io::Read};

use crate::{Error, game::Scenario};

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

        for unit in scenario.units {
            let mut elements = Vec::new();
            for element_in_toe in &toe.get(&unit.toe).ok_or_else(|| Error {
                error_message: format!("Unit '{}' has a toe '{}' that cannot be found.", unit.name, unit.toe),
            })?.elements {
                elements.push(ElementInUnit { name: element_in_toe.name.clone(), ready: element_in_toe.amount, damaged: 0 });
            }

            units.insert(unit.name.clone(), Unit {
                name: unit.name,
                toe: unit.toe,
                faction: unit.faction,
                location: unit.location,
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
    use either::Either;

    // Uses the real map file so fixtures only have to vary scenario data.
    fn scenario_toml(toe_elements: &str, units: &str) -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[players]]
faction_name = "Axis"
faction_tag = "AX"

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
accuracy = 20
range = 100
v_inf = 100
v_arm = 3

{units}
"#)
    }

    fn build_state(toe_elements: &str, units: &str) -> Result<State, Error> {
        let scenario: Scenario = toml::from_str(&scenario_toml(toe_elements, units)).unwrap();
        State::build(scenario)
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
location.Left.x = 1
location.Left.y = 1
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
location.Right.name = "GE Reserve"
"#;
        let state = build_state(VALID_TOE_ELEMENTS, units).unwrap();

        let unit = &state.units["Reserve Division"];
        assert_eq!(
            unit.location,
            Either::Right(OffmapLocationName { name: "GE Reserve".to_string() })
        );
    }

    #[test]
    fn unit_with_unknown_toe_is_rejected() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "no_such_toe"
faction = "AX"
location.Left.x = 1
location.Left.y = 1
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
location.Left.x = 1
location.Left.y = 1
"#;
        let error = build_state(toe_elements, units).unwrap_err();

        assert!(error.error_message.contains("missing_element"));
        assert!(error.error_message.contains("test_toe"));
    }
}
