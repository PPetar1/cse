use std::fmt::Display;

use hexx::*;

pub struct Location {
    hex: Option<Hex>, 
    pub terrain: Terrain,
    pub name: Option<String>,
}

impl Location {
    pub fn new(hex: Option<(u32, u32)>, terrain: Terrain, name: Option<String>) -> Location {
        if let Some((x, y)) = hex {
            Location { 
                 hex: Some(Hex::from_offset_coordinates([x as i32, y as i32], OffsetHexMode::Even, HexOrientation::Pointy)), 
                 terrain: terrain,
                 name: name,
            }
        }
        else {
            Location { 
                 hex: None, 
                 terrain: terrain,
                 name: name,
            }
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut location_name = String::new();
        if let Some(name) = &self.name {
            location_name.push_str(name);
        }
        
        if let Some(hex) = self.hex {
            let [x, y] = hex.to_offset_coordinates(OffsetHexMode::Even, HexOrientation::Pointy);
            write!(f, "{}(x: {}, y: {})\nTerrain: {:?}", location_name, x, y, self.terrain)
        }
        else {
            write!(f, "{}(offmap)\nTerrain: {:?}", location_name, self.terrain)
        }
    }
}

#[derive(Debug)]
pub enum Terrain {
    Mountain,
    Plains,
    Forest,
    Swamp,
    Desert,
    Water,
    Hills,
    Urban,
}

pub struct OffmapLocations {
    locations: Vec<Location>,
}

impl OffmapLocations {
    pub fn new() -> OffmapLocations {
        OffmapLocations { locations: Vec::new() }
    }

    pub fn get(&self, name: &str) -> Option<&Location> {
        for location in &self.locations {
            if let Some(location_name) = &location.name {
                if location_name == name {
                    return Some(&location)
                }
            } 
        }
        None
    }

    pub fn insert(&mut self, location: Location) {
        if let None = location.hex {
            if let Some(name) = &location.name {
                if let None = self.get(name) {
                    self.locations.push(location);
                }
            }
        }
    }

}
