use std::fmt::Display;

use hexx::*;

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
                 terrain,
                 name,
            }
        }
        else {
            Location { 
                 hex: None, 
                 terrain,
                 name,
            }
        }
    }

    /// Offset coordinates of the six adjacent hexes, negative ones filtered
    /// out. Offmap locations have no neighbours. The caller still has to
    /// check the map actually contains each coordinate.
    pub fn neighbour_coords(&self) -> Vec<(u32, u32)> {
        let Some(hex) = self.hex else {
            return Vec::new();
        };
        hex.all_neighbors()
            .iter()
            .map(|neighbour| neighbour.to_offset_coordinates(OffsetHexMode::Even, HexOrientation::Pointy))
            .filter(|[x, y]| *x >= 0 && *y >= 0)
            .map(|[x, y]| (x as u32, y as u32))
            .collect()
    }

    /// Hex-grid distance to another location; None if either is offmap.
    pub fn distance_to(&self, other: &Location) -> Option<u32> {
        Some(self.hex?.unsigned_distance_to(other.hex?))
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Deserialize, serde::Serialize)]
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

impl Terrain {
    /// Fallback movement cost to enter a hex of this terrain; None =
    /// impassable. Scenarios override these via `[terrain_costs]` — see
    /// `TerrainCosts`.
    fn default_movement_cost(&self) -> Option<u32> {
        match self {
            Terrain::Plains | Terrain::Desert | Terrain::Urban => Some(1),
            Terrain::Forest | Terrain::Hills => Some(2),
            Terrain::Swamp | Terrain::Mountain => Some(3),
            Terrain::Water => None,
        }
    }
}

/// Per-terrain movement entry costs: the scenario's `[terrain_costs]` table
/// layered over the code defaults. An entry of 0 makes the terrain impassable.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
pub struct TerrainCosts {
    overrides: std::collections::HashMap<Terrain, u32>,
}

impl TerrainCosts {
    pub fn new(overrides: std::collections::HashMap<Terrain, u32>) -> TerrainCosts {
        TerrainCosts { overrides }
    }

    /// Movement points to enter a hex of this terrain; None = impassable.
    pub fn cost(&self, terrain: Terrain) -> Option<u32> {
        match self.overrides.get(&terrain) {
            Some(0) => None,
            Some(&cost) => Some(cost),
            None => terrain.default_movement_cost(),
        }
    }
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
            if let Some(location_name) = &location.name
                && location_name == name {
                    return Some(location)
                } 
        }
        None
    }

    pub fn insert(&mut self, location: Location) {
        if location.hex.is_none()
            && let Some(name) = &location.name
                && self.get(name).is_none() {
                    self.locations.push(location);
                }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interior_hex_has_six_neighbours_at_distance_one() {
        let location = Location::new(Some((2, 2)), Terrain::Plains, None);

        let neighbours = location.neighbour_coords();

        assert_eq!(neighbours.len(), 6);
        for (x, y) in neighbours {
            let neighbour = Location::new(Some((x, y)), Terrain::Plains, None);
            assert_eq!(location.distance_to(&neighbour), Some(1));
        }
    }

    #[test]
    fn corner_hex_neighbours_stay_in_positive_coordinates() {
        let location = Location::new(Some((0, 0)), Terrain::Plains, None);

        for (x, y) in location.neighbour_coords() {
            // u32 coords can't go negative; just prove the filter kept only
            // real neighbours and dropped the rest.
            let neighbour = Location::new(Some((x, y)), Terrain::Plains, None);
            assert_eq!(location.distance_to(&neighbour), Some(1));
        }
        assert!(location.neighbour_coords().len() < 6);
    }

    #[test]
    fn offmap_locations_have_no_neighbours_or_distance() {
        let offmap = Location::new(None, Terrain::Urban, Some("Reserve".to_string()));
        let onmap = Location::new(Some((1, 1)), Terrain::Plains, None);

        assert!(offmap.neighbour_coords().is_empty());
        assert_eq!(offmap.distance_to(&onmap), None);
        assert_eq!(onmap.distance_to(&offmap), None);
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
