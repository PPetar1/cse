//! Scheduled reinforcements and withdrawals: units stepping onto or off the
//! map at a set turn. Both directions are mechanically identical — a
//! relocation of `Unit::location` — so one runtime type covers the two
//! scenario tables (`[[reinforcements]]` / `[[withdrawals]]`).

use crate::core::unit::UnitLocation;

use super::Game;
use super::scenario::ScheduledArrivalConfig;

/// Runtime form of `ScheduledArrivalConfig` — kept separate the same way
/// `UnitLocation` is kept separate from `UnitLocationConfig`, since postcard
/// save files need this to persist across turns.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct ScheduledArrival {
    pub(super) unit: String,
    pub(super) turn: u32,
    pub(super) location: UnitLocation,
}

impl From<ScheduledArrivalConfig> for ScheduledArrival {
    fn from(config: ScheduledArrivalConfig) -> ScheduledArrival {
        ScheduledArrival { unit: config.unit, turn: config.turn, location: config.location.into() }
    }
}

impl Game {
    /// Move every unit whose scheduled arrival falls on the current turn and
    /// belongs to the faction coming on turn — reinforcements step onto the
    /// map, withdrawals step off it, both just a relocation of `location`.
    /// Returns the relocated units' names, so `apply_entrenchment` can skip
    /// ticking a unit that was reset to fort level 0 this same turn start.
    pub(super) fn apply_scheduled_arrivals(&mut self) -> std::collections::HashSet<String> {
        let faction = self.player_on_turn().faction.clone();
        let turn = self.turn;
        let mut relocated = std::collections::HashSet::new();
        for arrival in &self.scheduled_arrivals {
            if arrival.turn != turn {
                continue;
            }
            if let Some(unit) = self.state.units.get_mut(&arrival.unit)
                && unit.faction == faction {
                    unit.location = arrival.location.clone();
                    unit.fort_level = 0; // Stepping on or off the map is a relocation too.
                    relocated.insert(arrival.unit.clone());
                }
        }
        relocated
    }

    /// Human-readable rundown of every scheduled reinforcement/withdrawal:
    /// the turn, unit, destination, and whether it has already happened.
    /// Backs the `reinforcements` command.
    pub fn reinforcement_schedule_summary(&self) -> String {
        if self.scheduled_arrivals.is_empty() {
            return "No scheduled reinforcements or withdrawals.".to_string();
        }
        let mut arrivals: Vec<&ScheduledArrival> = self.scheduled_arrivals.iter().collect();
        arrivals.sort_by_key(|arrival| (arrival.turn, arrival.unit.clone()));

        let mut out = String::from("Scheduled arrivals:\n");
        for arrival in arrivals {
            let destination = match &arrival.location {
                UnitLocation::OnMap(coords) => format!("({}, {})", coords.x, coords.y),
                UnitLocation::Offmap(name) => name.clone(),
            };
            let status = if self.turn >= arrival.turn { "arrived" } else { "pending" };
            out.push_str(&format!(
                "  Turn {}: {} -> {} [{}]\n", arrival.turn, arrival.unit, destination, status,
            ));
        }
        out.pop();
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::core::unit::{LocationCoords, UnitLocation};
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn a_reinforcement_scheduled_for_turn_one_arrives_immediately() {
        // begin_turn() only fires from end_turn, so turn-1 arrivals for the
        // first-moving player need to be applied right at Game::build.
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 1\nlocation = {{ x = 2, y = 2 }}\n"
        );
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 2 }),
        );
    }

    #[test]
    fn a_reinforcement_arrives_only_on_its_scheduled_turn() {
        let units = format!(
            "{OFFMAP_UNIT}\n\n[[units]]\nname = \"Soviet Division\"\ntoe = \"test_toe\"\nfaction = \"SU\"\nlocation = {{ x = 2, y = 1 }}\n\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 4, y = 4 }}\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        // Turn 1, Axis to move: not yet the scheduled turn.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );

        game.end_turn(); // Axis -> Soviet, still turn 1.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );

        game.end_turn(); // Soviet -> Axis, turn becomes 2: the reinforcement lands.
        assert_eq!(
            game.state.units["Reserve Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 4, y: 4 }),
        );
    }

    #[test]
    fn a_withdrawal_moves_a_unit_back_offmap() {
        let units = format!(
            "{OPPOSING_UNITS}\n[[withdrawals]]\nunit = \"Axis Division\"\nturn = 2\nlocation = \"GE Reserve\"\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.end_turn(); // turn 1, Soviet.
        game.end_turn(); // turn 2, Axis: the withdrawal fires.

        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::Offmap("GE Reserve".to_string()),
        );
    }

    #[test]
    fn reinforcement_schedule_summary_tracks_arrival_status() {
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 2, y = 2 }}\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert!(game.reinforcement_schedule_summary().contains("pending"));

        game.end_turn(); // ONE_PLAYER: every end_turn completes a full round.

        assert!(game.reinforcement_schedule_summary().contains("arrived"));
    }
}
