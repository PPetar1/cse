mod location;

use std::collections::HashMap;

use hexx::*;

use location::{Location, Terrain};

pub struct State {
    map: HashMap<(u32, u32), Location>,
}

impl State {
    pub fn build() -> State {
        let map = State::build_map(5, 5);
        State { 
            map 
        }    
    }

    fn build_map(width: u32, height: u32) -> HashMap<(u32, u32), Location> {
        let mut map = HashMap::new();

        for i in 0..=width {
            for j in 0..=height {
                map.insert(
                    (i, j),
                    Location { 
                        hex: Hex::from_offset_coordinates([i as i32, j as i32], OffsetHexMode::Even, HexOrientation::Pointy), 
                        terrain: Terrain::Plains,
                   }
                );
            }
        }

        map
    }

    pub fn inspect_location(&self, x: u32, y: u32) -> Option<&Location> {
       self.map.get(&(x, y))
    }
}
