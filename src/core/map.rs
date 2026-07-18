use std::collections::HashMap;

use crate::{Error, core::location::*};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Map {
    name: String,
    map: HashMap<(u32, u32), Location>,
    off_map: OffmapLocations,
}

impl Map {
    pub fn map_from_string(contents: &str) -> Result<Map, Error> {
        let map_file: MapFile = toml::from_str(contents)?;
        
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

    pub fn all_locations(&self) -> Vec<((u32, u32), Terrain)> {
        self.map
            .iter()
            .map(|(&coords, loc)| (coords, loc.terrain))
            .collect()
    }
}

#[derive(serde::Deserialize)]
struct MapFile {
    name: String,
    #[allow(dead_code)] // will be read once map dimension validation lands
    width: u32,
    #[allow(dead_code)]
    height: u32,
    locations: Vec<LocationConfig>,
    offmap_locations: Vec<OffmapLocationConfig>,
}

#[derive(serde::Deserialize)]
struct LocationConfig {
    x: u32,
    y: u32,
    terrain: Terrain,
    name: Option<String>,
}

#[derive(serde::Deserialize)]
struct OffmapLocationConfig {
    name: String,
    terrain: Terrain,
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAP_FIXTURE: &str = r#"
name = "mini"
width = 1
height = 1

[[locations]]
x = 0
y = 0
terrain = "Plains"

[[locations]]
x = 0
y = 1
terrain = "Water"

[[offmap_locations]]
name = "Reserve"
terrain = "Urban"
"#;

    #[test]
    fn parses_fixture_map() {
        let map = Map::map_from_string(MAP_FIXTURE).unwrap();

        assert_eq!(map.get_location(0, 0).unwrap().terrain, Terrain::Plains);
        assert_eq!(map.get_location(0, 1).unwrap().terrain, Terrain::Water);
        assert!(map.get_location(5, 5).is_none());
    }

    #[test]
    fn looks_up_offmap_locations_by_name() {
        let map = Map::map_from_string(MAP_FIXTURE).unwrap();

        assert_eq!(map.get_offmap_location("Reserve").unwrap().terrain, Terrain::Urban);
        assert!(map.get_offmap_location("No Such Place").is_none());
    }

    #[test]
    fn parses_a_map_with_multiple_offmap_locations() {
        let map_toml = r#"
name = "front"
width = 2
height = 4

[[locations]]
x = 1
y = 3
terrain = "Water"

[[offmap_locations]]
name = "GE Reserve"
terrain = "Urban"

[[offmap_locations]]
name = "SU Reserve"
terrain = "Urban"
"#;
        let map = Map::map_from_string(map_toml).unwrap();

        assert_eq!(map.get_location(1, 3).unwrap().terrain, Terrain::Water);
        assert!(map.get_offmap_location("GE Reserve").is_some());
        assert!(map.get_offmap_location("SU Reserve").is_some());
    }
}
