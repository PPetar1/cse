pub mod leader;
pub mod location;
pub mod map;
pub mod unit;

use std::collections::HashMap;

use location::TerrainCosts;
use map::Map;
use unit::*;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct State {
    pub map: Map,
    pub terrain_costs: TerrainCosts,
    pub units: HashMap<String, Unit>,
    pub toe: HashMap<String, Toe>,
    pub elements: HashMap<String, Element>,
    pub leaders: HashMap<String, leader::Leader>,
    /// Total element instances (ready + damaged, onmap and offmap) each
    /// faction fielded at scenario start — the baseline victory scoring
    /// measures losses against.
    pub starting_strength: HashMap<String, u32>,
}
