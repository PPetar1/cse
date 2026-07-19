//! Refit: repair and replacements as one combined turn-changeover mechanic,
//! per element bucket, for every on-map unit connected to supply (see
//! `game::supply`). Damaged elements slowly return to ready; destroyed ones
//! slowly regrow, up to the unit's TOE-prescribed strength. Cut-off units
//! get neither — the non-lethal consequence of encirclement for this
//! prototype (full degradation/surrender was deliberately dropped; see
//! "Stage 1" in docs/roadmap.md).

use crate::core::unit::UnitLocation;

use super::Game;

/// Repaired elements taper as the damaged pool shrinks:
/// `ceil(damaged / REPAIR_STEP)` return to ready each turn.
const REPAIR_STEP: u32 = 4;

/// Replacement elements taper as the gap to the unit's prescribed strength
/// shrinks: `ceil(missing / REPLACEMENT_STEP)` join as ready each turn.
const REPLACEMENT_STEP: u32 = 8;

impl Game {
    /// Turn-start refit for the faction coming on turn: every on-map unit of
    /// theirs still connected to supply repairs some damaged elements and
    /// receives some replacements for missing ones, both tapering and
    /// bounded by the unit's TOE-prescribed strength per element type.
    pub(super) fn apply_refit(&mut self) {
        let faction = self.player_on_turn().faction.clone();
        let reachable = self.faction_supplied_hexes(&faction);

        for unit in self.state.units.values_mut() {
            if unit.faction != faction {
                continue;
            }
            let UnitLocation::OnMap(coords) = &unit.location else {
                continue; // Offmap units aren't tracing supply yet.
            };
            if !reachable.contains(&(coords.x, coords.y)) {
                continue; // Cut off: no repair, no replacements.
            }

            let Some(toe) = self.state.toe.get(&unit.toe) else { continue };
            for element in &mut unit.elements {
                let Some(prescribed) = toe.elements.iter()
                    .find(|e| e.name == element.name)
                    .map(|e| e.amount)
                else {
                    continue;
                };

                let repaired = element.damaged.div_ceil(REPAIR_STEP);
                element.damaged -= repaired;
                element.ready += repaired;

                let current = element.ready + element.damaged;
                let missing = prescribed.saturating_sub(current);
                element.ready += missing.div_ceil(REPLACEMENT_STEP);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn a_supplied_units_damaged_elements_repair_over_turns() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 0\ny = 0\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();
        {
            let element = &mut game.state.units.get_mut("1st Test Division").unwrap().elements[0];
            element.ready = 6;
            element.damaged = 4;
        }

        game.end_turn(); // ONE_PLAYER: every end_turn completes a full round.

        // ceil(4 / 4) = 1 repaired.
        let element = &game.state.units["1st Test Division"].elements[0];
        assert_eq!((element.ready, element.damaged), (7, 3));
    }

    #[test]
    fn a_supplied_unit_receives_replacements_for_missing_elements() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 0\ny = 0\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();
        // The TOE prescribes 10; 4 were destroyed outright (gone, not
        // damaged), so ready + damaged is already below prescribed.
        game.state.units.get_mut("1st Test Division").unwrap().elements[0].ready = 6;

        game.end_turn();

        // missing = 10 - 6 = 4; ceil(4 / 8) = 1 replacement.
        assert_eq!(game.state.units["1st Test Division"].elements[0].ready, 7);
    }

    #[test]
    fn replacements_never_exceed_the_toes_prescribed_strength() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 0\ny = 0\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        for _ in 0..20 {
            game.end_turn();
        }

        // Already at full prescribed strength (10): nothing to repair or
        // replace, ever.
        let element = &game.state.units["1st Test Division"].elements[0];
        assert_eq!((element.ready, element.damaged), (10, 0));
    }

    #[test]
    fn a_cut_off_unit_gets_no_repair_or_replacements() {
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
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        {
            let element = &mut game.state.units.get_mut("1st Test Division").unwrap().elements[0];
            element.ready = 4;
            element.damaged = 2;
        }

        game.end_turn(); // Axis -> Soviet, still turn 1.
        game.end_turn(); // Soviet -> Axis, turn 2: Axis's refit would fire here.

        // Cut off: nothing repaired, no replacements — still 4 ready, 2
        // damaged (4 missing out of the prescribed 10 stay missing).
        let element = &game.state.units["1st Test Division"].elements[0];
        assert_eq!((element.ready, element.damaged), (4, 2));
    }
}
