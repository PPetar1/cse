use std::collections::HashMap;

use crate::core::location::*;

pub struct Map {
    map: HashMap<(u32, u32), Location>,
    off_map: OffmapLocations,
}

impl Map {
    pub fn new_debug_map(width: u32, height: u32) -> Map {
        let mut map = HashMap::new();

        for i in 0..=width {
            for j in 0..=height {
                map.insert(
                    (i, j),
                    Location::new(Some((i, j)), Terrain::Plains, None),
                );
            }
        }

        let mut off_map = OffmapLocations::new();
        off_map.insert(Location::new(None, Terrain::Urban, Some("SUReserve".to_string())));
        off_map.insert(Location::new(None, Terrain::Urban, Some("GEReserve".to_string())));

        Map { 
            map,
            off_map,
        }
    }

    pub fn map_from_string(string: &str) -> Map {
        Map::new_debug_map(5, 5)
    }
    
    pub fn inspect_location(&self, x: u32, y: u32) -> Option<&Location> {
       self.map.get(&(x, y))
    }

    pub fn inspect_offmap_location(&self, name: &str) -> Option<&Location> {
        self.off_map.get(name)
    }
}

