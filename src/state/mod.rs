mod location;

use std::collections::HashMap;

use hexx::*;

use location::{Location, Terrain};

pub struct State {
    map: HashMap<Hex, Location>,
}

impl State {
    pub fn build() -> State {
        let map = State::build_map();
        State { 
            map 
        }    
    }

    fn build_map() -> HashMap<Hex, Location> {
        let mut iterator = shapes::flat_rectangle([0, 10, 0, 10]);
        
        let mut map = HashMap::new();

        while let Some(item) = iterator.next() {
            map.insert(item, Location { terrain: Terrain::Plains});
        }

        map
    }

    pub fn inspect_location(&self, x: i32, y: i32) -> Option<&Location> {
       self.map.get(&Hex::from_offset_coordinates([y, x], OffsetHexMode::Even, HexOrientation::Pointy))
    }
}
