//! Victory scoring and reporting: who holds the objective hexes, how much
//! of each army is gone, and the final tally when a scenario's `last_turn`
//! completes. The conditions themselves are scenario data
//! (`scenario::VictoryConditions`); this module is the runtime side.

use std::fmt::Display;

use super::Game;

impl Game {
    /// Tally each faction's score: points for victory hexes it holds, points
    /// for the enemy strength it destroyed, minus a penalty for its own
    /// losses — all measured against `State::starting_strength`.
    pub(super) fn score_victory(&self) -> VictoryReport {
        let scores = self.players.iter().map(|player| {
            let faction = &player.faction_tag;

            let hex_points: f32 = self.victory_conditions.hexes.iter()
                .filter(|hex| self.controlling_faction(hex.x, hex.y).as_deref() == Some(faction.as_str()))
                .map(|hex| hex.points)
                .sum();

            let destruction_points: f32 = self.players.iter()
                .filter(|other| other.faction_tag != *faction)
                .map(|other| {
                    self.percent_destroyed(&other.faction_tag)
                        * self.victory_conditions.points_per_percent_enemy_destroyed
                })
                .sum();

            let loss_penalty = self.percent_destroyed(faction)
                * self.victory_conditions.points_per_percent_own_lost;

            FactionScore {
                faction_name: player.faction_name.clone(),
                hex_points,
                destruction_points,
                loss_penalty,
                total: hex_points + destruction_points - loss_penalty,
            }
        }).collect();

        VictoryReport { scores }
    }

    /// The faction currently holding a hex — the units there, if any (mixed
    /// stacks don't occur: enemy-occupied hexes can't be moved or attacked
    /// into without clearing the defenders first).
    fn controlling_faction(&self, x: u32, y: u32) -> Option<String> {
        let location = self.state.map.get_location(x, y)?;
        Some(self.units_at_location(location).first()?.faction.clone())
    }

    /// Whether `faction` currently controls hex (x, y) — see
    /// `controlling_faction`. The GUI's inspector uses this to decide
    /// whether to show Move/Attack buttons for the hex it's showing.
    pub fn hex_controlled_by(&self, faction: &str, x: u32, y: u32) -> bool {
        self.controlling_faction(x, y).as_deref() == Some(faction)
    }

    /// Percent of a faction's starting element strength (ready + damaged)
    /// that is now gone — destroyed, shattered or surrendered.
    fn percent_destroyed(&self, faction: &str) -> f32 {
        let starting = *self.state.starting_strength.get(faction).unwrap_or(&0) as f32;
        if starting == 0.0 {
            return 0.0;
        }
        let current: u32 = self.state.units.values()
            .filter(|unit| unit.faction == faction)
            .map(|unit| unit.elements.iter().map(|e| e.ready + e.damaged).sum::<u32>())
            .sum();
        ((starting - current as f32) / starting * 100.0).max(0.0)
    }

    /// Human-readable rundown of the scenario's victory conditions: the last
    /// turn, each objective hex with its current controller, and the
    /// destruction/loss point multipliers. Backs the `victory` command,
    /// since a scenario's win conditions are otherwise invisible in play.
    pub fn victory_conditions_summary(&self) -> String {
        let mut out = String::new();
        match self.victory_conditions.last_turn {
            Some(last) => out.push_str(&format!("Scenario ends after turn {last}.\n")),
            None => out.push_str("This scenario has no automatic end turn.\n"),
        }
        if self.victory_conditions.hexes.is_empty() {
            out.push_str("No objective hexes.\n");
        } else {
            out.push_str("Objective hexes:\n");
            for hex in &self.victory_conditions.hexes {
                let label = hex.name.as_deref().unwrap_or("(unnamed)");
                let holder = self.controlling_faction(hex.x, hex.y)
                    .unwrap_or_else(|| "nobody".to_string());
                out.push_str(&format!(
                    "  ({}, {}) {}: {:.0} points — held by {}\n",
                    hex.x, hex.y, label, hex.points, holder,
                ));
            }
        }
        out.push_str(&format!(
            "Enemy destruction: {:.1} pts per % of enemy strength destroyed.\n",
            self.victory_conditions.points_per_percent_enemy_destroyed,
        ));
        out.push_str(&format!(
            "Own losses: -{:.1} pts per % of own strength lost.",
            self.victory_conditions.points_per_percent_own_lost,
        ));
        out
    }

    /// Objective hexes for map-view display: coordinates, points and name.
    pub fn victory_hexes(&self) -> Vec<VictoryHexInfo> {
        self.victory_conditions.hexes.iter()
            .map(|hex| VictoryHexInfo { x: hex.x, y: hex.y, points: hex.points, name: hex.name.clone() })
            .collect()
    }
}

/// One objective hex, for display outside the game module (the `victory`
/// command's text and the map view's flag markers) — decoupled from the
/// scenario-parsed `VictoryHex` the same way `MapSnapshot` is decoupled from
/// `State`.
#[derive(Debug, Clone)]
pub struct VictoryHexInfo {
    pub x: u32,
    pub y: u32,
    pub points: f32,
    pub name: Option<String>,
}

/// One faction's tally at scenario end.
#[derive(Debug)]
pub struct FactionScore {
    pub faction_name: String,
    pub hex_points: f32,
    pub destruction_points: f32,
    pub loss_penalty: f32,
    pub total: f32,
}

/// The final scores for every faction once a scenario's `last_turn` completes.
#[derive(Debug)]
pub struct VictoryReport {
    pub scores: Vec<FactionScore>,
}

impl VictoryReport {
    /// The sole highest-scoring faction, or None on a tie (a draw).
    pub fn winner(&self) -> Option<&str> {
        let best = self.scores.iter()
            .map(|score| score.total)
            .fold(f32::NEG_INFINITY, f32::max);
        let mut leaders = self.scores.iter().filter(|score| score.total == best);
        let winner = leaders.next()?;
        match leaders.next() {
            None => Some(winner.faction_name.as_str()),
            Some(_) => None,
        }
    }
}

impl Display for VictoryReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "=== Scenario over: final scores ===")?;
        for score in &self.scores {
            // Empty-iterator sums can land on -0.0 (IEEE 754, harmless but
            // ugly to print); +0.0 normalizes the sign for display.
            writeln!(
                f,
                "{}: {:.1} pts (hexes {:.1}, enemy destroyed {:.1}, own losses {:.1})",
                score.faction_name, score.total + 0.0, score.hex_points + 0.0,
                score.destruction_points + 0.0, -score.loss_penalty + 0.0,
            )?;
        }
        match self.winner() {
            Some(name) => write!(f, "{name} wins!"),
            None => write!(f, "The scenario ends in a draw."),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn end_turn_never_scores_without_a_last_turn() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
        assert!(game.end_turn().is_none());
    }

    #[test]
    fn hex_controlled_by_reports_the_occupying_faction() {
        let game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        // Axis Division sits at (1, 1), Soviet Division at (2, 1) — see
        // OPPOSING_UNITS.
        assert!(game.hex_controlled_by("AX", 1, 1));
        assert!(!game.hex_controlled_by("SU", 1, 1));
        assert!(game.hex_controlled_by("SU", 2, 1));

        // An empty hex belongs to no one.
        assert!(!game.hex_controlled_by("AX", 0, 0));
        assert!(!game.hex_controlled_by("SU", 0, 0));
    }

    #[test]
    fn victory_score_awards_points_for_controlled_hexes() {
        let victory_conditions = r#"
[victory_conditions]
last_turn = 1

[[victory_conditions.hexes]]
x = 1
y = 1
points = 10

[[victory_conditions.hexes]]
x = 2
y = 1
points = 20
"#;
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        // Turn 1 is not over yet: no score.
        assert!(game.end_turn().is_none());
        // Every player has now moved in turn 1: last_turn is complete.
        let report = game.end_turn().unwrap();

        let axis = report.scores.iter().find(|s| s.faction_name == "Axis").unwrap();
        assert_eq!(axis.hex_points, 10.0);
        let soviet = report.scores.iter().find(|s| s.faction_name == "Soviet Union").unwrap();
        assert_eq!(soviet.hex_points, 20.0);
        assert_eq!(report.winner(), Some("Soviet Union"));
    }

    #[test]
    fn victory_score_rewards_enemy_destruction_and_penalizes_own_losses() {
        let victory_conditions = r#"
[victory_conditions]
last_turn = 1
points_per_percent_enemy_destroyed = 2.0
points_per_percent_own_lost = 1.0
"#;
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        // The Soviet division starts with 10 ready test_element instances;
        // half of them are gone.
        game.state.units.get_mut("Soviet Division").unwrap().elements[0].ready = 5;

        game.end_turn();
        let report = game.end_turn().unwrap();

        let axis = report.scores.iter().find(|s| s.faction_name == "Axis").unwrap();
        // 50% of the Soviets destroyed, times the 2.0 multiplier.
        assert_eq!(axis.destruction_points, 100.0);
        assert_eq!(axis.loss_penalty, 0.0);
        assert_eq!(axis.total, 100.0);

        let soviet = report.scores.iter().find(|s| s.faction_name == "Soviet Union").unwrap();
        assert_eq!(soviet.destruction_points, 0.0);
        // 50% of their own strength lost, times the 1.0 penalty multiplier.
        assert_eq!(soviet.loss_penalty, 50.0);
        assert_eq!(soviet.total, -50.0);

        assert_eq!(report.winner(), Some("Axis"));
    }

    #[test]
    fn a_tied_score_is_a_draw() {
        let victory_conditions = "\n[victory_conditions]\nlast_turn = 1\n";
        let units = format!("{OPPOSING_UNITS}\n{victory_conditions}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        game.end_turn();
        let report = game.end_turn().unwrap();

        assert_eq!(report.winner(), None);
    }
}
