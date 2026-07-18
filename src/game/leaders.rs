//! Leaders: named commanders a faction can assign to a unit (see
//! `core::leader::Leader`). Assignment lives on `Unit::leader` — a leader
//! object carries no back-reference, so "who leads whom" is always looked
//! up from the unit side (`unit_led_by`).

use crate::Error;
use crate::core::leader::Leader;
use crate::core::unit::Unit;

use super::Game;

impl Game {
    /// A faction's leaders, sorted by name.
    pub fn leaders_of_faction(&self, faction: &str) -> Vec<&Leader> {
        let mut leaders: Vec<&Leader> = self.state.leaders.values()
            .filter(|leader| leader.faction == faction)
            .collect();
        leaders.sort_by(|a, b| a.name.cmp(&b.name));
        leaders
    }

    /// The unit `leader` currently commands, if any.
    fn unit_led_by(&self, leader: &str) -> Option<&Unit> {
        self.state.units.values().find(|unit| unit.leader.as_deref() == Some(leader))
    }

    /// Human-readable rundown of a faction's leaders and which unit (if
    /// any) each commands. Backs the `leaders` command.
    pub fn leaders_summary(&self, faction: &str) -> Result<String, Error> {
        if !self.players.iter().any(|player| player.faction_tag == faction) {
            return Err(Error::new(format!("Unknown faction '{faction}'.")));
        }
        let leaders = self.leaders_of_faction(faction);
        if leaders.is_empty() {
            return Ok(format!("No leaders for faction '{faction}'."));
        }

        let mut out = format!("Leaders for {faction}:\n");
        for leader in leaders {
            let assignment = self.unit_led_by(&leader.name)
                .map_or("unassigned".to_string(), |unit| unit.name.clone());
            out.push_str(&format!("  {} -> {}\n", leader.name, assignment));
        }
        out.pop();
        Ok(out)
    }

    /// Full stat breakdown for one leader plus their current assignment.
    /// Backs the `leader` command.
    pub fn leader_detail(&self, name: &str) -> Result<String, Error> {
        let leader = self.state.leaders.get(name)
            .ok_or_else(|| Error::new(format!("No such leader '{name}'.")))?;
        let assignment = self.unit_led_by(&leader.name)
            .map_or("unassigned".to_string(), |unit| unit.name.clone());
        let stats = &leader.stats;
        Ok(format!(
            "{} ({})\n  Political: {}  Morale: {}  Initiative: {}  Administration: {}  Mechanized: {}  Infantry: {}  Air: {}\n  Assigned to: {}",
            leader.name, leader.faction,
            stats.political, stats.morale, stats.initiative, stats.administration,
            stats.mechanized, stats.infantry, stats.air,
            assignment,
        ))
    }

    /// Assign `leader` to command `unit`, clearing any unit it previously
    /// led — a leader commands at most one unit at a time. Backs the
    /// terminal's `reassign_leader` prompt (see `run_reassign_leader` in
    /// `terminal/mod.rs`).
    pub fn reassign_leader(&mut self, leader: &str, unit: &str) -> Result<(), Error> {
        let leader_faction = self.state.leaders.get(leader)
            .ok_or_else(|| Error::new(format!("No such leader '{leader}'.")))?
            .faction.clone();
        let unit_faction = self.state.units.get(unit)
            .ok_or_else(|| Error::new(format!("No such unit '{unit}'.")))?
            .faction.clone();
        if leader_faction != unit_faction {
            return Err(Error::new(format!(
                "'{leader}' belongs to faction '{leader_faction}', not '{unit_faction}' like '{unit}'.",
            )));
        }

        for other in self.state.units.values_mut() {
            if other.leader.as_deref() == Some(leader) {
                other.leader = None;
            }
        }
        self.state.units.get_mut(unit).unwrap().leader = Some(leader.to_string());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    const AXIS_LEADER: &str = r#"
[[leaders]]
name = "Erwin Rommel"
faction = "AX"
[leaders.stats]
political = 5
morale = 7
initiative = 8
administration = 4
mechanized = 7
infantry = 5
air = 1
"#;

    #[test]
    fn a_leader_assigned_in_the_scenario_shows_up_on_its_unit() {
        let units = format!("{ONMAP_UNIT}\n{AXIS_LEADER}")
            .replace("faction = \"AX\"\nlocation", "faction = \"AX\"\nleader = \"Erwin Rommel\"\nlocation");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(game.state.units["1st Test Division"].leader.as_deref(), Some("Erwin Rommel"));
    }

    #[test]
    fn rejects_a_unit_assigning_an_unknown_leader() {
        let units = ONMAP_UNIT
            .replace("faction = \"AX\"\nlocation", "faction = \"AX\"\nleader = \"Ghost\"\nlocation");

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("Ghost"));
        assert!(error.error_message.contains("not defined"));
    }

    #[test]
    fn rejects_a_unit_assigning_a_leader_of_another_faction() {
        let units = format!("{OPPOSING_UNITS}\n{AXIS_LEADER}").replace(
            "name = \"Soviet Division\"\ntoe = \"test_toe\"\nfaction = \"SU\"",
            "name = \"Soviet Division\"\ntoe = \"test_toe\"\nfaction = \"SU\"\nleader = \"Erwin Rommel\"",
        );

        let error = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap_err();

        assert!(error.error_message.contains("Erwin Rommel"));
        assert!(error.error_message.contains("belongs to faction 'AX', not 'SU'"));
    }

    #[test]
    fn rejects_the_same_leader_assigned_to_two_units() {
        let units = format!(
            "{ONMAP_UNIT}\n{AXIS_LEADER}\n[[units]]\nname = \"2nd Test Division\"\ntoe = \"test_toe\"\nfaction = \"AX\"\nlocation = {{ x = 2, y = 2 }}\nleader = \"Erwin Rommel\"\n",
        )
        .replace(
            "name = \"1st Test Division\"\ntoe = \"test_toe\"\nfaction = \"AX\"",
            "name = \"1st Test Division\"\ntoe = \"test_toe\"\nfaction = \"AX\"\nleader = \"Erwin Rommel\"",
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("assigned to multiple units"));
    }

    #[test]
    fn leaders_summary_lists_assignment_status() {
        let units = format!("{ONMAP_UNIT}\n{AXIS_LEADER}");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let summary = game.leaders_summary("AX").unwrap();
        assert!(summary.contains("Erwin Rommel -> unassigned"));
    }

    #[test]
    fn leaders_summary_rejects_an_unknown_faction() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        let error = game.leaders_summary("ZZ").unwrap_err();

        assert!(error.error_message.contains("Unknown faction"));
    }

    #[test]
    fn leader_detail_reports_stats_and_assignment() {
        let units = format!("{ONMAP_UNIT}\n{AXIS_LEADER}");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let detail = game.leader_detail("Erwin Rommel").unwrap();
        assert!(detail.contains("Political: 5"));
        assert!(detail.contains("Air: 1"));
        assert!(detail.contains("Assigned to: unassigned"));
    }

    #[test]
    fn leader_detail_rejects_an_unknown_leader() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        let error = game.leader_detail("Ghost").unwrap_err();

        assert!(error.error_message.contains("No such leader"));
    }

    #[test]
    fn reassign_leader_assigns_and_reports_the_new_unit() {
        let units = format!("{ONMAP_UNIT}\n{AXIS_LEADER}");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        game.reassign_leader("Erwin Rommel", "1st Test Division").unwrap();

        assert_eq!(game.state.units["1st Test Division"].leader.as_deref(), Some("Erwin Rommel"));
        assert!(game.leaders_summary("AX").unwrap().contains("Erwin Rommel -> 1st Test Division"));
    }

    #[test]
    fn reassign_leader_clears_the_previous_unit() {
        let units = format!("{OPPOSING_UNITS}\n{AXIS_LEADER}").replace(
            "name = \"Axis Division\"\ntoe = \"test_toe\"\nfaction = \"AX\"\nlocation = { x = 1, y = 1 }",
            "name = \"Axis Division\"\ntoe = \"test_toe\"\nfaction = \"AX\"\nlocation = { x = 1, y = 1 }\n\n[[units]]\nname = \"Axis Second\"\ntoe = \"test_toe\"\nfaction = \"AX\"\nlocation = { x = 1, y = 1 }",
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.reassign_leader("Erwin Rommel", "Axis Division").unwrap();
        game.reassign_leader("Erwin Rommel", "Axis Second").unwrap();

        assert_eq!(game.state.units["Axis Division"].leader, None);
        assert_eq!(game.state.units["Axis Second"].leader.as_deref(), Some("Erwin Rommel"));
    }

    #[test]
    fn reassign_leader_rejects_an_unknown_leader() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        let error = game.reassign_leader("Ghost", "1st Test Division").unwrap_err();

        assert!(error.error_message.contains("No such leader"));
    }

    #[test]
    fn reassign_leader_rejects_an_unknown_unit() {
        let units = format!("{ONMAP_UNIT}\n{AXIS_LEADER}");
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let error = game.reassign_leader("Erwin Rommel", "Ghost Division").unwrap_err();

        assert!(error.error_message.contains("No such unit"));
    }

    #[test]
    fn reassign_leader_rejects_a_faction_mismatch() {
        let units = format!("{OPPOSING_UNITS}\n{AXIS_LEADER}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        let error = game.reassign_leader("Erwin Rommel", "Soviet Division").unwrap_err();

        assert!(error.error_message.contains("belongs to faction 'AX', not 'SU'"));
    }
}
