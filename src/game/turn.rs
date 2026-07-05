//! The turn system: how control passes between players and what happens to
//! a faction when its turn begins (MP refill, morale drift, scheduled
//! arrivals and events). A future WEGO mode (simultaneous orders, resolved
//! together at turn end) lands as a second `TurnSystem` variant plus an
//! order queue — the matches on that enum are the places it plugs in.

use super::{Game, Player, VictoryReport};
use super::scenario::PlayerController;

/// At its faction's turn start every element bucket drifts toward the faction
/// default morale by `ceil(|gap| / MORALE_RECOVERY_STEP)`: battered units
/// recover with rest, battle-euphoric ones settle back down. Gentler than the
/// battle shifts, so combat outcomes dominate the drift.
const MORALE_RECOVERY_STEP: u32 = 10;

impl Game {
    /// End the current player's turn. Under IGO-UGO control passes to the
    /// next player; once every player has moved, the turn counter and the
    /// game date advance. Turn-start effects for the faction coming on turn
    /// (MP reset, morale recovery) hook in here as they land. Returns the
    /// final score once the scenario's `last_turn` has just been completed.
    pub fn end_turn(&mut self) -> Option<VictoryReport> {
        let mut victory = None;
        match self.turn_system {
            TurnSystem::IgoUgo => {
                self.phase.player_on_turn += 1;
                if self.phase.player_on_turn as usize >= self.players.len() {
                    self.phase.player_on_turn = 0;
                    self.turn += 1;
                    self.date += time::Duration::days(self.turn_length.into());
                    if self.victory_conditions.last_turn.is_some_and(|last| self.turn > last) {
                        victory = Some(self.score_victory());
                    }
                }
                self.begin_turn();
            }
        }
        victory
    }

    /// Turn-start effects for the faction coming on turn: scheduled
    /// reinforcements/withdrawals and scenario events land first (an event's
    /// morale/experience delta feeds straight into the same turn's drift
    /// target below), then refit (repair/replacements for units connected to
    /// supply), then a fresh movement budget from the TOE, interdiction
    /// coverage resetting (it must be redeclared every time this faction
    /// acts), and morale drifting back toward the faction default (rest
    /// heals battered units, euphoria fades).
    fn begin_turn(&mut self) {
        self.apply_scheduled_arrivals();
        self.apply_scheduled_events();
        self.apply_refit();

        let player = self.player_on_turn();
        let faction = player.faction_tag.clone();
        let default_morale = player.morale;
        self.reset_interdiction_coverage(&faction);
        for unit in self.state.units.values_mut() {
            if unit.faction == faction {
                unit.mp_left = self.state.toe.get(&unit.toe).expect("unit's toe vanished").mp;
                for entry in &mut unit.elements {
                    entry.morale = morale_drift(entry.morale, default_morale);
                }
            }
        }
    }

    /// One-line summary of where the game clock stands.
    pub fn status(&self) -> String {
        format!(
            "{} — turn {}, {}. {} to move.",
            self.scenario_name, self.turn, self.date, self.player_on_turn().faction_name,
        )
    }

    pub(super) fn player_on_turn(&self) -> &Player {
        &self.players[self.phase.player_on_turn as usize]
    }

    /// The faction tag of the player currently on turn — the seam any
    /// controller-aware logic outside `game` (the AI today) reads to know
    /// whose move it is.
    pub fn current_faction(&self) -> &str {
        &self.player_on_turn().faction_tag
    }

    /// Whether the player currently on turn is AI-controlled.
    pub fn current_player_is_ai(&self) -> bool {
        self.player_on_turn().controller == PlayerController::Ai
    }
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub(super) struct TurnPhase {
    pub(super) player_on_turn: u32,
}

/// How player turns are sequenced. Scenario-selectable; only IGO-UGO exists
/// today. A future WEGO mode (simultaneous orders, resolved together at turn
/// end) lands as a second variant plus an order queue — the matches on this
/// enum are the places it plugs in.
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize, serde::Serialize)]
pub(super) enum TurnSystem {
    #[default]
    IgoUgo,
}

/// One turn-start step of morale recovery: toward the faction default from
/// either side, tapering as the gap closes (zero exactly at the default).
fn morale_drift(morale: u32, default: u32) -> u32 {
    if morale < default {
        morale + (default - morale).div_ceil(MORALE_RECOVERY_STEP)
    } else {
        morale - (morale - default).div_ceil(MORALE_RECOVERY_STEP)
    }
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn end_turn_cycles_players_and_advances_the_clock() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Axis to move.");

        game.end_turn();
        // Control passes within the same turn: the clock stands still.
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Soviet Union to move.");

        game.end_turn();
        // Every player has moved: turn and date (turn_length = 7) advance.
        assert_eq!(game.status(), "test scenario — turn 2, 1941-06-29. Axis to move.");
    }

    #[test]
    fn a_factions_movement_points_refill_when_it_comes_on_turn() {
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, OPPOSING_UNITS)).unwrap();

        game.move_unit(1, 1, 1, 2, 0).unwrap();
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Soviet turn: the spent Axis budget stays spent.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Axis on turn again: fresh budget from the TOE.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].mp_left, 16);
    }

    #[test]
    fn morale_drifts_toward_the_faction_default_at_turn_start() {
        // Faction defaults are the unspecified 50; the units start far off it.
        let units = r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 20

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
morale = 90
"#;
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, units)).unwrap();

        // Soviet turn starts: only Soviet morale drifts — down toward 50,
        // 90 - ceil(40 / 10) = 86.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 20);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 86);

        // Axis turn starts: 20 + ceil(30 / 10) = 23; the Soviets keep 86.
        game.end_turn();
        assert_eq!(game.state.units["Axis Division"].elements[0].morale, 23);
        assert_eq!(game.state.units["Soviet Division"].elements[0].morale, 86);
    }

    #[test]
    fn morale_at_the_faction_default_stays_put() {
        // Single player: every end_turn is an Axis turn start. The unit sits
        // at the faction default (50) already.
        let mut game = one_unit_game();

        game.end_turn();
        assert_eq!(game.state.units["1st Test Division"].elements[0].morale, 50);
    }
}
