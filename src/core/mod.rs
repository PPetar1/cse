mod location;
mod map;
pub mod unit;

use std::{fs::File, io::Read};

use crate::{Error, game::Scenario};

use map::Map;
use unit::*;

pub struct State<'a> {
    pub map: Map,
    units: Vec<Unit<'a>>,
    toe: Vec<Toe<'a>>,
    elements: Vec<Element>,
}

impl<'a> State<'a> {
    pub fn build(scenario: &Scenario) -> Result<State<'a>, Error<'a>> {
        let mut map_file = File::open(&scenario.map)?;
        let mut contents = String::new();
        map_file.read_to_string(&mut contents)?;

        let map = Map::map_from_string(&contents);
        
        let units = Vec::new();
        let toe = Vec::new();
        let elements = Vec::new();
        
        Ok(State { 
            map,
            units,
            toe,
            elements,
        })    
    }

}
