use std::fmt::Display;

use hexx::Hex;

pub struct Location {
    pub hex: Hex, 
    pub terrain: Terrain,
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "(x: {}, y: {})\nTerrain: {:?}", self.hex.x(), self.hex.y(), self.terrain)
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
}
