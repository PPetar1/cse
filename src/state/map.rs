use std::collections::HashMap;

use hexx::*;

use crate::state::location::{Location, Terrain};

pub struct Map {
    map: HashMap<Hex, Location>,
}

impl Map {
    pub fn new([left, right, top, bottom]: [i32;4]) -> Map {
        let mut iterator = shapes::flat_rectangle([left, right, top, bottom]);
        
        let mut map = HashMap::new();

        while let Some(item) = iterator.next() {
            map.insert(item, Location { terrain: Terrain::Plains});
        }

        Map { map }
    }
}
