//! Leaders: named commanders a faction can assign to a unit. The TOML-facing
//! shape (`game::scenario::LeaderConfig`) resolves an absent `doctrine` to
//! the leader's faction default before becoming this runtime type — the one
//! field that keeps this from being a straight `Toe`/`Element`-style
//! deserialize-and-go.

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Leader {
    pub name: String,
    pub faction: String,
    pub stats: LeaderStats,
    /// 1-100. Personal doctrine rating: drifts toward the faction's
    /// doctrine over time and shifts from the outcome of battles this
    /// leader commands — see `game::doctrine`.
    pub doctrine: u32,
}

/// The seven WitE2 leadership ratings, 1-9 each (not enforced at load time,
/// same as `ElementInUnit`'s 0-100 morale/experience). `game::doctrine` reads
/// `initiative`/`political` for the doctrine drift formulas and averages the
/// other five (`average_leader_value`) as a leader's doctrine ceiling/floor;
/// no other gameplay effect yet (individual battle rolls are still
/// unmodeled — see docs/manual.md).
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
