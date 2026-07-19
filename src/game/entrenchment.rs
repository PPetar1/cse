//! Entrenchment: units gain a defensive fort level the longer they stay put.
//! `Unit.fort_level` is the only state; this module just increments it at
//! turn start and caps it. Resetting to 0 on relocation lives with whatever
//! order actually moves the unit (`move_unit`, retreat, advance, scheduled
//! arrivals) — see "Entrenchment" in docs/manual.md.

use std::collections::HashSet;

use crate::core::unit::UnitLocation;

use super::Game;

/// A unit's fort level never rises above this — WitE-style fortification
/// levels, capped rather than unbounded.
pub const MAX_FORT_LEVEL: u32 = 5;

impl Game {
    /// Turn-start entrenchment for the faction coming on turn: every on-map
    /// unit of theirs gains one fort level, capped at `MAX_FORT_LEVEL`. A
    /// unit that relocated since its last turn already had its level reset
    /// to 0 at the moment it moved (see the callers listed above), so this
    /// just ticks whoever's still standing where they were. Offmap units
    /// (reserve boxes) never entrench, and `just_arrived` — names this
    /// turn's scheduled arrivals already reset to 0 — skips one tick so a
    /// fresh reinforcement doesn't immediately dig in on the turn it lands.
    pub(super) fn apply_entrenchment(&mut self, just_arrived: &HashSet<String>) {
        let faction = self.player_on_turn().faction.clone();
        for (name, unit) in self.state.units.iter_mut() {
            if unit.faction != faction {
                continue;
            }
            if !matches!(unit.location, UnitLocation::OnMap(_)) {
                continue;
            }
            if just_arrived.contains(name) {
                continue;
            }
            unit.fort_level = (unit.fort_level + 1).min(MAX_FORT_LEVEL);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::core::unit::{LocationCoords, UnitLocation};
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn a_stationary_unit_gains_a_fort_level_every_turn_it_stays_put() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        assert_eq!(game.state.units["1st Test Division"].fort_level, 0);
        game.end_turn();
        assert_eq!(game.state.units["1st Test Division"].fort_level, 1);
        game.end_turn();
        assert_eq!(game.state.units["1st Test Division"].fort_level, 2);
    }

    #[test]
    fn fort_level_caps_at_the_maximum() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();
        game.state.units.get_mut("1st Test Division").unwrap().fort_level =
            super::MAX_FORT_LEVEL;

        game.end_turn();

        assert_eq!(game.state.units["1st Test Division"].fort_level, super::MAX_FORT_LEVEL);
    }

    #[test]
    fn an_offmap_unit_never_entrenches() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, OFFMAP_UNIT)).unwrap();

        game.end_turn();
        game.end_turn();

        assert_eq!(game.state.units["Reserve Division"].fort_level, 0);
    }

    #[test]
    fn a_reinforcement_gets_no_entrenchment_tick_on_its_arrival_turn() {
        let units = format!(
            "{OFFMAP_UNIT}\n\n[[units]]\nname = \"Soviet Division\"\ntoe = \"test_toe\"\nfaction = \"SU\"\nlocation = {{ x = 2, y = 1 }}\n\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 4, y = 4 }}\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.end_turn(); // Axis -> Soviet, still turn 1.
        game.end_turn(); // Soviet -> Axis, turn becomes 2: the reinforcement lands.

        assert_eq!(game.state.units["Reserve Division"].fort_level, 0);

        game.end_turn(); // Axis -> Soviet.
        game.end_turn(); // Soviet -> Axis, turn 3: Axis's next turn start.

        assert_eq!(game.state.units["Reserve Division"].fort_level, 1);
    }

    #[test]
    fn moving_resets_fort_level_to_zero() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();
        game.state.units.get_mut("1st Test Division").unwrap().fort_level = 3;

        game.move_unit(1, 1, 2, 1, 0).unwrap();

        assert_eq!(game.state.units["1st Test Division"].fort_level, 0);
    }

    #[test]
    fn a_faction_not_yet_on_turn_does_not_gain_fort_levels() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        game.end_turn(); // Axis (first mover) ends its turn.

        // Soviet Division never moved and it's now Soviet Union's turn, so it
        // gains a level; Axis Division, having already had its turn-start
        // tick this cycle, is untouched until its own next turn.
        assert_eq!(game.state.units["Soviet Division"].fort_level, 1);
        assert_eq!(game.state.units["Axis Division"].fort_level, 0);
    }

    #[test]
    fn retreating_resets_fort_level_to_zero() {
        use rand::SeedableRng;
        use rand::rngs::StdRng;

        // Three attackers against one — the same reliably-forces-a-retreat
        // fixture `game::orders::attack`'s own retreat tests use. Morale 100
        // never routs or shatters, so the defender always plainly retreats.
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &three_vs_one(100))).unwrap();
        game.state.units.get_mut("Soviet Division").unwrap().fort_level = 4;
        let mut rng = StdRng::seed_from_u64(2);

        game.attack((1, 1), (2, 1), &mut rng).unwrap();

        let unit = &game.state.units["Soviet Division"];
        assert_eq!(unit.fort_level, 0);
        assert_ne!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }));
    }
}
