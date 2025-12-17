use std::collections::HashMap;

use crate::{Error, core::location::*};

#[derive(serde::Deserialize)]
pub struct Map {
    name: String,
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
            name: "debug_map".to_string(),
            map,
            off_map,
        }
    }

    pub fn map_from_string(contents: &str) -> Result<Map, Error> {
        let map_file: MapFile = toml::from_str(&contents)?;
        
        let mut map = HashMap::new();

        for location in map_file.locations {
            map.insert((location.x, location.y), Location::new(Some((location.x, location.y)), location.terrain, location.name));
        }

        let mut off_map = OffmapLocations::new();

        for offmap_location in map_file.offmap_locations {
            off_map.insert(Location::new(None, offmap_location.terrain, Some(offmap_location.name)));
        }

        Ok(Map {
            name: map_file.name,
            map,
            off_map,
        })
    }
    
    pub fn get_location(&self, x: u32, y: u32) -> Option<&Location> {
       self.map.get(&(x, y))
    }

    pub fn get_offmap_location(&self, name: &str) -> Option<&Location> {
        self.off_map.get(name)
    }
}

#[derive(serde::Deserialize)]
struct MapFile {
    name: String,
    width: u32,
    height: u32,
    locations: Vec<Location_>,
    offmap_locations: Vec<OffmapLocation_>,
}

#[derive(serde::Deserialize)]
struct Location_ {
    x: u32,
    y: u32,
    terrain: Terrain,
    name: Option<String>,
}

#[derive(serde::Deserialize)]
struct OffmapLocation_ {
    name: String,
    terrain: Terrain,
}
