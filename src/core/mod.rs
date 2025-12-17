pub mod location;
mod map;
pub mod unit;

use std::{collections::HashMap, fs::File, io::Read};

use crate::{Error, game::Scenario};

use map::Map;
use unit::*;

#[derive(serde::Deserialize, serde::Serialize)]
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
            toe.insert(toe_.name.clone(), toe_);
        }

        for unit in scenario.units {
            let mut elements = Vec::new();
            for element_in_toe in &toe.get(&unit.toe).ok_or(Error::from_str("Unit has a toe that cannot be found"))?.elements {
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
