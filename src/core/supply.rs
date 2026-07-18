//! A faction's supply-source hexes, for tracing which of its units are
//! connected back to them (see `game::supply`). Deserialized directly from
//! the scenario TOML like `Leader`/`ScenarioEvent` — nothing here is
//! untagged, so no config/runtime split is needed.

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SupplySource {
    pub faction: String,
    pub x: u32,
    pub y: u32,
}
