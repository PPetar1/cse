//! The movement order: relocate one unit along the cheapest path to any
//! reachable hex, charging its movement points. Pathfinding itself lives in
//! `procedures::pathfinding` (`cheapest_path_cost`); this module owns the
//! order's rules — whose turn it is, what blocks the way, what the trip
//! costs.

use crate::Error;
use crate::core::unit::{LocationCoords, UnitLocation};
use crate::game::Game;
use crate::procedures::pathfinding;

impl Game {
    pub fn move_unit(&mut self, x_start: u32, y_start: u32, x_end: u32, y_end: u32, unit_i: usize) -> Result<(), Error> {
        let start = self.state.map.get_location(x_start, y_start).ok_or(Error {
            error_message: "Invalid starting location.".to_string(),
        })?;
        let destination = self.state.map.get_location(x_end, y_end).ok_or(Error {
                error_message: "Invalid destination.".to_string(),
        })?;
        if (x_start, y_start) == (x_end, y_end) {
            return Err(Error::new("The unit is already at the destination."));
        }
        let terrain = destination.terrain;
        if self.state.terrain_costs.cost(terrain).is_none() {
            return Err(Error::new(format!("{terrain:?} is impassable.")));
        }

        // Resolve the order to a unit: units_at_location sorts by name, so
        // the index matches what inspect showed the player.
        let unit = self.units_at_location(start).into_iter().nth(unit_i).ok_or_else(|| Error {
            error_message: format!("No unit with index {} at ({}, {}).", unit_i, x_start, y_start),
        })?;
        let unit_name = unit.name.clone();

        let on_turn = self.player_on_turn().faction.clone();
        if unit.faction != on_turn {
            return Err(Error::new(format!("It is not {}'s turn.", unit.faction)));
        }

        // Taking ground held by the enemy is what `attack` is for — enemy
        // hexes can be neither the destination nor passed through.
        let enemy_hexes: std::collections::HashSet<(u32, u32)> = self.state.units.values()
            .filter(|other| other.faction != on_turn)
            .filter_map(|other| match &other.location {
                UnitLocation::OnMap(coords) => Some((coords.x, coords.y)),
                UnitLocation::Offmap(_) => None,
            })
            .collect();
        if enemy_hexes.contains(&(x_end, y_end)) {
            return Err(Error::new("Cannot move into a hex occupied by the enemy."));
        }

        let cost = pathfinding::cheapest_path_cost(
            &self.state.map, (x_start, y_start), (x_end, y_end),
            |coords, location| {
                if enemy_hexes.contains(&coords) {
                    return None;
                }
                self.state.terrain_costs.cost(location.terrain)
            },
        ).ok_or_else(|| Error::new("No passable route to the destination."))?;

        let unit = self.state.units.get_mut(&unit_name).expect("moving unit vanished");
        if unit.mp_left < cost {
            return Err(Error::new(format!(
                "Not enough movement points: the way there costs {cost}, {unit_name} has {} left.",
                unit.mp_left,
            )));
        }

        unit.mp_left -= cost;
        unit.location = UnitLocation::OnMap(LocationCoords { x: x_end, y: y_end });
        unit.fort_level = 0; // Digging in is lost the moment a unit moves.

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::core::unit::{LocationCoords, UnitLocation};
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn move_unit_updates_location_and_spends_movement_points() {
        let mut game = one_unit_game();

        // (2, 1) is adjacent Plains: cost 1 from the budget of 16.
        game.move_unit(1, 1, 2, 1, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }));
        assert_eq!(unit.mp_left, 15);
    }

    #[test]
    fn rough_terrain_costs_more_movement_points() {
        let mut game = one_unit_game();

        // (1, 2) is adjacent Forest: cost 2.
        game.move_unit(1, 1, 1, 2, 0).unwrap();

        assert_eq!(game.state.units["1st Test Division"].mp_left, 14);
    }

    #[test]
    fn move_unit_crosses_multiple_hexes_charging_the_path_cost() {
        let mut game = one_unit_game();

        // (1, 1) to (2, 2) is two hexes: cheapest route is two Plains steps
        // via (2, 1), total 2 (the direct Forest neighbour would cost 3).
        game.move_unit(1, 1, 2, 2, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 2 }));
        assert_eq!(unit.mp_left, 14);
    }

    #[test]
    fn pathfinding_routes_around_impassable_terrain() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 3 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // The Water hex (1, 3) sits between (0, 3) and (2, 3): the cheapest
        // way around is three Plains steps via (0, 4) and (1, 4).
        game.move_unit(0, 3, 2, 3, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 3 }));
        assert_eq!(unit.mp_left, 13);
    }

    #[test]
    fn pathfinding_detours_around_enemy_held_hexes() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 3 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 1, y = 4 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        // With (1, 4) enemy-held on top of the Water at (1, 3), the 3-point
        // route from the previous test is blocked; the cheapest is now 4.
        game.move_unit(0, 3, 2, 3, 0).unwrap();

        let unit = &game.state.units["1st Test Division"];
        assert_eq!(unit.location, UnitLocation::OnMap(LocationCoords { x: 2, y: 3 }));
        assert_eq!(unit.mp_left, 12);
    }

    #[test]
    fn move_unit_rejects_an_unreachable_destination() {
        // Plains = 0 leaves only scattered Forest passable: (3, 3) exists
        // and is enterable, but no route reaches it from (1, 2).
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 2 }

[terrain_costs]
Plains = 0
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let error = game.move_unit(1, 2, 3, 3, 0).unwrap_err();
        assert!(error.error_message.contains("No passable route"));
    }

    #[test]
    fn move_unit_rejects_moving_in_place() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 1, 1, 0).unwrap_err();
        assert!(error.error_message.contains("already at the destination"));
    }

    #[test]
    fn move_unit_rejects_impassable_terrain() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 2 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // (1, 3) is adjacent Water.
        let error = game.move_unit(1, 2, 1, 3, 0).unwrap_err();
        assert!(error.error_message.contains("impassable"));
    }

    #[test]
    fn scenario_terrain_costs_override_the_defaults() {
        // Piggybacks on the units slot to append the scenario-level table.
        let units = format!("{ONMAP_UNIT}\n[terrain_costs]\nForest = 4\nPlains = 0\n");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        // Forest (1, 2) costs the override 4 instead of the default 2.
        game.move_unit(1, 1, 1, 2, 0).unwrap();
        assert_eq!(game.state.units["1st Test Division"].mp_left, 12);

        // 0 makes a terrain impassable: no going back onto the Plains.
        let error = game.move_unit(1, 2, 1, 1, 0).unwrap_err();
        assert!(error.error_message.contains("impassable"));
    }

    #[test]
    fn move_unit_rejects_an_enemy_occupied_destination() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // The Soviet division sits at (2, 1): entering is an attack, not a move.
        let error = game.move_unit(1, 1, 2, 1, 0).unwrap_err();
        assert!(error.error_message.contains("occupied by the enemy"));
    }

    #[test]
    fn move_unit_allows_stacking_with_friends() {
        let units = r#"
[[units]]
name = "First Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Second Division"
toe = "test_toe"
faction = "AX"
location = { x = 2, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        game.move_unit(1, 1, 2, 1, 0).unwrap();

        assert_eq!(
            game.state.units["First Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }),
        );
    }

    #[test]
    fn move_unit_rejects_an_exhausted_unit() {
        let mut game = one_unit_game();
        game.state.units.get_mut("1st Test Division").unwrap().mp_left = 0;

        let error = game.move_unit(1, 1, 2, 1, 0).unwrap_err();
        assert!(error.error_message.contains("Not enough movement points"));
    }

    #[test]
    fn move_unit_rejects_invalid_start_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(99, 99, 2, 2, 0).unwrap_err();
        assert!(error.error_message.contains("starting location"));
    }

    #[test]
    fn move_unit_rejects_invalid_destination_hex() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 99, 99, 0).unwrap_err();
        assert!(error.error_message.contains("destination"));
    }

    #[test]
    fn move_unit_rejects_index_with_no_unit() {
        let mut game = one_unit_game();

        let error = game.move_unit(1, 1, 2, 1, 5).unwrap_err();
        assert!(error.error_message.contains("index 5"));
        assert!(error.error_message.contains("(1, 1)"));
    }

    #[test]
    fn stacked_units_are_indexed_in_name_order() {
        let units = r#"
[[units]]
name = "Bravo Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Alpha Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Charlie Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        let names: Vec<&str> = found.iter().map(|unit| unit.name.as_str()).collect();
        assert_eq!(names, ["Alpha Division", "Bravo Division", "Charlie Division"]);

        // Index 1 must address the same unit move_unit sees: Bravo.
        game.move_unit(1, 1, 2, 1, 1).unwrap();
        assert_eq!(
            game.state.units["Bravo Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 })
        );
        assert_eq!(
            game.state.units["Alpha Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 1, y: 1 })
        );
    }

    #[test]
    fn move_unit_rejects_a_unit_of_the_off_turn_faction() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // Axis moves first: the Soviet division has to wait for its turn.
        let error = game.move_unit(2, 1, 3, 1, 0).unwrap_err();
        assert!(error.error_message.contains("not SU's turn"));

        game.end_turn();
        game.move_unit(2, 1, 3, 1, 0).unwrap();
        assert_eq!(
            game.state.units["Soviet Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 3, y: 1 }),
        );
    }
}
