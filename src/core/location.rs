use std::fmt::Display;

use hexx::*;
use either::Either;

use crate::core::unit::{LocationCoords, OffmapLocationName};

#[derive(Debug, serde::Deserialize, PartialEq, serde::Serialize)]
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

    pub fn get_coords(&self) -> Either<LocationCoords, OffmapLocationName> {
        if let Some(hex) = self.hex {
            let coords = hex.to_offset_coordinates(OffsetHexMode::Even, HexOrientation::Pointy);
            Either::Left(LocationCoords { x: coords[0] as u32, y: coords[1] as u32 })
        }
        else {
            Either::Right(OffmapLocationName { name: self.name.clone().unwrap() })
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

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize, serde::Serialize)]
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

#[derive(Debug, serde::Deserialize, serde::Serialize)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_coords_round_trips_offset_coordinates() {
        let location = Location::new(Some((3, 4)), Terrain::Plains, None);

        let coords = location.get_coords();
        assert_eq!(coords, Either::Left(LocationCoords { x: 3, y: 4 }));
    }

    #[test]
    fn get_coords_returns_name_for_offmap_location() {
        let location = Location::new(None, Terrain::Urban, Some("Reserve".to_string()));

        let coords = location.get_coords();
        assert_eq!(coords, Either::Right(OffmapLocationName { name: "Reserve".to_string() }));
    }

    #[test]
    fn insert_ignores_duplicate_names() {
        let mut off_map = OffmapLocations::new();
        off_map.insert(Location::new(None, Terrain::Urban, Some("Reserve".to_string())));
        off_map.insert(Location::new(None, Terrain::Plains, Some("Reserve".to_string())));

        assert_eq!(off_map.locations.len(), 1);
        assert_eq!(off_map.get("Reserve").unwrap().terrain, Terrain::Urban);
    }

    #[test]
    fn insert_ignores_onmap_locations() {
        let mut off_map = OffmapLocations::new();
        off_map.insert(Location::new(Some((0, 0)), Terrain::Plains, Some("Named Hex".to_string())));

        assert_eq!(off_map.locations.len(), 0);
    }
}
