//! Leaders: named commanders a faction can assign to a unit. Deserialized
//! directly from the scenario TOML like `Toe`/`Element` (see
//! `core::unit`) — nothing here is untagged, so no config/runtime split is
//! needed.

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Leader {
    pub name: String,
    pub faction: String,
    pub stats: LeaderStats,
}

/// The seven WitE2 leadership ratings, 1-9 each (not enforced at load time,
/// same as `ElementInUnit`'s 0-100 morale/experience). No gameplay effect
/// yet — this just establishes the data a future command/combat system can
/// read.
#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
pub struct LeaderStats {
    pub political: u32,
    pub morale: u32,
    pub initiative: u32,
    pub administration: u32,
    pub mechanized: u32,
    pub infantry: u32,
    pub air: u32,
}
