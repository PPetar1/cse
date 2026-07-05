//! Interdiction: a fighter-capable unit declares coverage of up to
//! `INTERDICTION_HEX_LIMIT` hexes per turn; any battle that happens at a
//! covered hex automatically pulls that unit's elements into the defender
//! snapshot (see `game::orders::attack::prepare_battle`), whether or not
//! the attacker flew an `air_support` mission. Declaring costs nothing
//! beyond the per-unit hex cap and clears at the covering faction's own
//! next turn start, so it must be redeclared every time they act.

use crate::Error;
use crate::core::unit::{ElementClass, Unit};

use super::Game;

const INTERDICTION_HEX_LIMIT: usize = 3;

impl Game {
    /// Declare that `unit` covers `target` this turn. A no-op if it's
    /// already covering that hex; rejects a hex beyond the per-unit cap.
    pub fn interdict(&mut self, unit: &str, target: (u32, u32)) -> Result<(), Error> {
        let faction = self.player_on_turn().faction_tag.clone();
        let fighter = self.state.units.get(unit)
            .ok_or_else(|| Error::new(format!("No such unit '{unit}'.")))?;
        if fighter.faction != faction {
            return Err(Error::new(format!("It is not {}'s turn.", fighter.faction)));
        }
        let is_fighter = fighter.elements.iter().any(|element| {
            self.state.elements.get(&element.name)
                .is_some_and(|element_type| element_type.class == ElementClass::Fighter)
        });
        if !is_fighter {
            return Err(Error::new(format!("'{unit}' has no fighter elements to interdict with.")));
        }
        if self.state.map.get_location(target.0, target.1).is_none() {
            return Err(Error::new("Invalid target location."));
        }

        let coverage = self.interdiction_coverage.entry(unit.to_string()).or_default();
        if coverage.contains(&target) {
            return Ok(());
        }
        if coverage.len() >= INTERDICTION_HEX_LIMIT {
            return Err(Error::new(format!(
                "'{unit}' is already covering {INTERDICTION_HEX_LIMIT} hexes this turn.",
            )));
        }
        coverage.push(target);
        Ok(())
    }

    /// Units of `faction` currently covering `hex` — what a battle there
    /// automatically pulls in as extra defenders.
    pub(super) fn covering_fighter_units(&self, faction: &str, hex: (u32, u32)) -> Vec<&Unit> {
        self.interdiction_coverage.iter()
            .filter(|(_, hexes)| hexes.contains(&hex))
            .filter_map(|(name, _)| self.state.units.get(name))
            .filter(|unit| unit.faction == faction)
            .collect()
    }

    /// Clear every covering unit belonging to `faction` — called at that
    /// faction's own turn start, so coverage survives exactly through the
    /// opponent's intervening turn and must be redeclared after that.
    pub(super) fn reset_interdiction_coverage(&mut self, faction: &str) {
        let own_units: std::collections::HashSet<String> = self.state.units.values()
            .filter(|unit| unit.faction == faction)
            .map(|unit| unit.name.clone())
            .collect();
        self.interdiction_coverage.retain(|unit, _| !own_units.contains(unit));
    }

    /// Human-readable rundown of every unit currently covering hexes.
    /// Backs the `interdiction` command.
    pub fn interdiction_summary(&self) -> String {
        let mut units: Vec<(&String, &Vec<(u32, u32)>)> = self.interdiction_coverage.iter()
            .filter(|(_, hexes)| !hexes.is_empty())
            .collect();
        if units.is_empty() {
            return "No units are currently covering any hexes.".to_string();
        }
        units.sort_by(|a, b| a.0.cmp(b.0));

        let mut out = String::from("Interdiction coverage:\n");
        for (unit, hexes) in units {
            let hex_list: Vec<String> = hexes.iter().map(|(x, y)| format!("({x}, {y})")).collect();
            out.push_str(&format!("  {unit}: {}\n", hex_list.join(", ")));
        }
        out.pop();
        out
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    const FIGHTER_UNITS: &str = r#"
[[toe]]
name = "fighter_toe"
size = "Regiment"
mp = 0
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "fighter_element"
amount = 10

[[elements]]
name = "fighter_element"
class = "Fighter"
cv = 5
vulnerability = 100
[[elements.devices]]
name = "cannon"
accuracy = 30
range = 3000
rate_of_fire = 1
soft_attack = 0
hard_attack = 0
air_attack = 60

[[units]]
name = "Axis Fighter Wing"
toe = "fighter_toe"
faction = "AX"
location = "GE Reserve"
"#;

    #[test]
    fn interdict_declares_coverage_up_to_the_limit_and_rejects_a_fourth() {
        let units = format!("{ONMAP_UNIT}\n{FIGHTER_UNITS}");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        game.interdict("Axis Fighter Wing", (0, 0)).unwrap();
        game.interdict("Axis Fighter Wing", (1, 1)).unwrap();
        game.interdict("Axis Fighter Wing", (2, 2)).unwrap();
        assert!(game.interdiction_summary().contains("(0, 0), (1, 1), (2, 2)"));

        let error = game.interdict("Axis Fighter Wing", (3, 3)).unwrap_err();
        assert!(error.error_message.contains("already covering 3 hexes"));
    }

    #[test]
    fn redeclaring_an_already_covered_hex_is_a_no_op() {
        let units = format!("{ONMAP_UNIT}\n{FIGHTER_UNITS}");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        game.interdict("Axis Fighter Wing", (0, 0)).unwrap();
        game.interdict("Axis Fighter Wing", (1, 1)).unwrap();
        game.interdict("Axis Fighter Wing", (2, 2)).unwrap();
        // Already at the cap, but re-declaring an existing hex still succeeds.
        game.interdict("Axis Fighter Wing", (1, 1)).unwrap();
    }

    #[test]
    fn interdict_rejects_a_unit_with_no_fighter_elements() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        let error = game.interdict("1st Test Division", (0, 0)).unwrap_err();

        assert!(error.error_message.contains("no fighter elements"));
    }

    #[test]
    fn interdict_rejects_an_unknown_unit() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        let error = game.interdict("Ghost Wing", (0, 0)).unwrap_err();

        assert!(error.error_message.contains("No such unit"));
    }

    #[test]
    fn interdict_rejects_a_unit_of_the_off_turn_faction() {
        let units = format!("{OPPOSING_UNITS}\n{FIGHTER_UNITS}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        game.end_turn(); // Soviet Union now on turn.

        let error = game.interdict("Axis Fighter Wing", (0, 0)).unwrap_err();

        assert!(error.error_message.contains("not AX's turn"));
    }

    #[test]
    fn coverage_survives_the_opponents_turn_but_clears_at_the_covering_factions_next_turn() {
        let units = format!("{OPPOSING_UNITS}\n{FIGHTER_UNITS}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.interdict("Axis Fighter Wing", (0, 0)).unwrap();
        assert!(game.interdiction_summary().contains("Axis Fighter Wing"));

        // Soviet Union's turn: Axis's coverage isn't theirs to clear.
        game.end_turn();
        assert!(game.interdiction_summary().contains("Axis Fighter Wing"));

        // Axis's own turn starts again: their coverage resets.
        game.end_turn();
        assert_eq!(game.interdiction_summary(), "No units are currently covering any hexes.");
    }
}
