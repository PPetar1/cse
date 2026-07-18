//! On-demand supply status: whether each on-map unit can trace a path back
//! to one of its faction's supply sources. Nothing here is persisted; it's
//! computed fresh every call. Tracing itself has no effect on play — cut off
//! units neither degrade nor surrender — but `game::refit` gates repair and
//! replacements on it, so encirclement isn't purely informational.

use std::collections::HashSet;

use crate::core::unit::{Unit, UnitLocation};
use crate::procedures::supply;

use super::Game;

impl Game {
    /// Every hex a faction's units can currently reach from that faction's
    /// supply sources, blocked by enemy-occupied hexes the same way movement
    /// is.
    pub(super) fn faction_supplied_hexes(&self, faction: &str) -> HashSet<(u32, u32)> {
        let sources = self.state.supply_sources.iter()
            .filter(|source| source.faction == faction)
            .map(|source| (source.x, source.y));

        let enemy_hexes: HashSet<(u32, u32)> = self.state.units.values()
            .filter(|unit| unit.faction != faction)
            .filter_map(|unit| match &unit.location {
                UnitLocation::OnMap(coords) => Some((coords.x, coords.y)),
                UnitLocation::Offmap(_) => None,
            })
            .collect();

        supply::reachable_hexes(&self.state.map, &self.state.terrain_costs, sources, &enemy_hexes)
    }

    /// Human-readable rundown of every on-map unit's supply status: whether
    /// it can currently trace a path back to one of its faction's supply
    /// sources. Backs the `supply` command.
    pub fn supply_status_summary(&self) -> String {
        if self.state.supply_sources.is_empty() {
            return "This scenario has no supply sources.".to_string();
        }

        let mut units: Vec<&Unit> = self.state.units.values().collect();
        units.sort_by(|a, b| a.name.cmp(&b.name));

        let mut out = String::from("Supply status:\n");
        let mut reachable_by_faction: std::collections::HashMap<&str, HashSet<(u32, u32)>> =
            std::collections::HashMap::new();
        for unit in &units {
            let UnitLocation::OnMap(coords) = &unit.location else {
                continue; // Offmap units aren't tracing anything yet.
            };
            let reachable = reachable_by_faction.entry(unit.faction.as_str())
                .or_insert_with(|| self.faction_supplied_hexes(&unit.faction));
            let status = if reachable.contains(&(coords.x, coords.y)) { "supplied" } else { "cut off" };
            out.push_str(&format!("  {} ({}, {}): {}\n", unit.name, coords.x, coords.y, status));
        }
        out.pop();
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn a_unit_connected_to_its_supply_source_is_supplied() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 0\ny = 0\n"
        );
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert!(game.supply_status_summary().contains("1st Test Division (1, 1): supplied"));
    }

    #[test]
    fn a_unit_walled_off_from_its_source_is_cut_off() {
        // Ring the Axis division's hex (1, 1) with Soviet divisions on every
        // neighbour so nothing traces back to the source at (0, 0).
        let mut units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 0\ny = 0\n"
        );
        let game_for_neighbours = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();
        let hex = game_for_neighbours.state.map.get_location(1, 1).unwrap();
        for (i, (x, y)) in hex.neighbour_coords().into_iter().enumerate() {
            units.push_str(&format!(
                "\n[[units]]\nname = \"Blocker {i}\"\ntoe = \"test_toe\"\nfaction = \"SU\"\nlocation = {{ x = {x}, y = {y} }}\n"
            ));
        }
        let game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        assert!(game.supply_status_summary().contains("1st Test Division (1, 1): cut off"));
    }

    #[test]
    fn a_scenario_with_no_supply_sources_says_so() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        assert_eq!(game.supply_status_summary(), "This scenario has no supply sources.");
    }
}
