//! Fog of war: whether a faction can currently see a given hex or unit.
//! Display-only — move/attack validation, the AI, and `units_at_location`
//! all keep seeing full state regardless; this is a presentation-layer
//! query, not a change to game mechanics. Opt-in per scenario via
//! `[fog_of_war]` (absent = full visibility, every scenario's behavior
//! before this module existed). See "Fog of war and detection" in
//! docs/manual.md for the model and its deliberate simplifications.

use crate::core::unit::{Unit, UnitLocation};

use super::Game;

impl Game {
    /// Whether faction `viewer` can currently see (x, y): always true
    /// without `[fog_of_war]`, or if the coordinate isn't on the map;
    /// otherwise true iff some on-map unit of `viewer`'s is within
    /// `detection_range` hexes (`Location::distance_to`, the same distance
    /// query `check_mission_range` uses).
    pub fn is_visible_to(&self, viewer: &str, x: u32, y: u32) -> bool {
        let Some(range) = self.fog_of_war else { return true };
        let Some(target) = self.state.map.get_location(x, y) else { return true };

        self.state.units.values()
            .filter(|unit| unit.faction == viewer)
            .filter_map(|unit| match &unit.location {
                UnitLocation::OnMap(coords) => self.state.map.get_location(coords.x, coords.y),
                UnitLocation::Offmap(_) => None,
            })
            .filter_map(|location| location.distance_to(target))
            .any(|distance| distance <= range)
    }

    /// Whether `unit` is currently visible to `viewer`: always true for the
    /// viewer's own units, or without `[fog_of_war]`. An enemy unit needs
    /// `is_visible_to` on its on-map hex; an enemy unit still off-map (e.g.
    /// a reserve box) is never visible — fog of war can't reveal a reserve
    /// box's contents at all in this first slice.
    pub fn is_unit_visible_to(&self, unit: &Unit, viewer: &str) -> bool {
        if unit.faction == viewer || self.fog_of_war.is_none() {
            return true;
        }
        match &unit.location {
            UnitLocation::OnMap(coords) => self.is_visible_to(viewer, coords.x, coords.y),
            UnitLocation::Offmap(_) => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::unit::{LocationCoords, UnitLocation};
    use crate::game::Game;
    use crate::game::test_support::*;

    const FOG_OF_WAR: &str = "\n[fog_of_war]\ndetection_range = 2\n";

    fn two_faction_units() -> String {
        format!(
            "{OPPOSING_UNITS}{FOG_OF_WAR}",
        )
    }

    #[test]
    fn without_fog_of_war_everything_is_visible() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // Axis Division is at (1, 1), Soviet Division at (2, 1) — far outside
        // any reasonable detection range, but there's no [fog_of_war] table.
        assert!(game.is_visible_to("AX", 2, 1));
        assert!(game.is_visible_to("SU", 1, 1));
    }

    #[test]
    fn a_hex_within_detection_range_of_an_own_unit_is_visible() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, &two_faction_units())).unwrap();

        // Axis Division sits at (1, 1); Soviet Division at (2, 1) is
        // distance 1 away, within the configured detection_range of 2.
        assert!(game.is_visible_to("AX", 2, 1));
    }

    #[test]
    fn a_hex_beyond_detection_range_is_not_visible() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, &two_faction_units())).unwrap();

        // Far corner of the 10x8 basic_map, well beyond range 2 from (1, 1).
        assert!(!game.is_visible_to("AX", 9, 7));
    }

    #[test]
    fn a_units_own_hex_is_always_visible_even_at_zero_range() {
        let units = format!("{OPPOSING_UNITS}\n[fog_of_war]\ndetection_range = 0\n");
        let game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        assert!(game.is_visible_to("AX", 1, 1));
        // The Soviet Division one hex away is now out of range.
        assert!(!game.is_visible_to("AX", 2, 1));
    }

    #[test]
    fn an_enemy_unit_still_offmap_is_never_visible_under_fog_of_war() {
        let units = format!("{ONMAP_UNIT}\n{OFFMAP_UNIT}{FOG_OF_WAR}");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();
        // Reassign the offmap unit to the opposing faction for this check.
        game.state.units.get_mut("Reserve Division").unwrap().faction = "SU".to_string();

        let reserve = &game.state.units["Reserve Division"];
        assert!(matches!(reserve.location, UnitLocation::Offmap(_)));
        assert!(!game.is_unit_visible_to(reserve, "AX"));
    }

    #[test]
    fn a_units_visibility_matches_its_hexs_visibility() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &two_faction_units())).unwrap();
        game.state.units.get_mut("Soviet Division").unwrap().location =
            UnitLocation::OnMap(LocationCoords { x: 9, y: 7 });

        let soviet = &game.state.units["Soviet Division"];
        assert!(!game.is_unit_visible_to(soviet, "AX"));
    }
}
