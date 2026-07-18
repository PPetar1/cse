//! Scenario events at runtime: firing `[[events]]` entries when their
//! faction's turn comes up — a printed message plus an optional nudge to the
//! faction's default morale/experience. The event shape itself is scenario
//! data (`scenario::ScenarioEvent`).

use super::Game;
use super::scenario::ScenarioEvent;

impl Game {
    /// Fire every scenario event whose turn falls on the current turn and
    /// whose faction is coming on turn: nudge that faction's default
    /// morale/experience and queue the event's message for `run` to print.
    pub(super) fn apply_scheduled_events(&mut self) {
        let faction = self.player_on_turn().faction_tag.clone();
        let turn = self.turn;
        for event in &self.events {
            if event.turn != turn || event.faction != faction {
                continue;
            }
            if let Some(player) = self.players.iter_mut().find(|p| p.faction_tag == faction) {
                player.morale = clamp_percent(player.morale as i32 + event.morale_delta);
                player.experience = clamp_percent(player.experience as i32 + event.experience_delta);
            }
            self.pending_event_messages.push(event.message.clone());
        }
    }

    /// Every event message queued since the last time this was called —
    /// `session.rs` (`report_turn_transition`/`activate_game`) drains and
    /// reports them after `end_turn`/`new`, for both frontends.
    pub fn take_event_messages(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending_event_messages)
    }

    /// Human-readable rundown of every scheduled event: the turn, faction,
    /// message and stat deltas, and whether it has already fired. Backs the
    /// `events` command.
    pub fn event_schedule_summary(&self) -> String {
        if self.events.is_empty() {
            return "No scheduled events.".to_string();
        }
        let mut events: Vec<&ScenarioEvent> = self.events.iter().collect();
        events.sort_by_key(|event| (event.turn, event.faction.clone()));

        let mut out = String::from("Scheduled events:\n");
        for event in events {
            let status = if self.turn >= event.turn { "fired" } else { "pending" };
            out.push_str(&format!(
                "  Turn {} ({}): {} (morale {:+}, experience {:+}) [{}]\n",
                event.turn, event.faction, event.message,
                event.morale_delta, event.experience_delta, status,
            ));
        }
        out.pop();
        out
    }
}

/// Applies an event's stat delta and keeps the 0-100 range morale/experience
/// are defined over.
fn clamp_percent(value: i32) -> u32 {
    value.clamp(0, 100) as u32
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn a_turn_one_event_fires_immediately_and_shifts_faction_morale() {
        // begin_turn() only fires from end_turn, so turn-1 events for the
        // first-moving player need the same explicit first pass as
        // reinforcements.
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 1\nfaction = \"AX\"\nmessage = \"Opening barrage\"\nmorale_delta = 10\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(game.players[0].morale, 60); // default 50 + 10
        assert_eq!(game.take_event_messages(), vec!["Opening barrage".to_string()]);
    }

    #[test]
    fn an_event_fires_only_for_its_faction_on_its_scheduled_turn() {
        let units = format!(
            "{OPPOSING_UNITS}\n[[events]]\nturn = 2\nfaction = \"SU\"\nmessage = \"Reinforcement convoy arrives\"\nmorale_delta = 5\n"
        );
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();

        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Axis -> Soviet, still turn 1: not yet scheduled.
        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Soviet -> Axis, turn becomes 2: Axis's start, wrong faction.
        assert!(game.take_event_messages().is_empty());

        game.end_turn(); // Axis -> Soviet, turn stays 2: the Soviet event fires.
        assert_eq!(game.take_event_messages(), vec!["Reinforcement convoy arrives".to_string()]);
    }

    #[test]
    fn event_morale_delta_clamps_to_the_valid_range() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 1\nfaction = \"AX\"\nmessage = \"Catastrophe\"\nmorale_delta = -1000\n"
        );
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert_eq!(game.players[0].morale, 0);
    }

    #[test]
    fn an_events_morale_delta_feeds_the_same_turns_drift_target() {
        // The unit's element sits at 20, far below the unspecified default
        // of 50 — it would drift up toward 50 on an ordinary turn start. An
        // event bumping the default to 90 on the same turn should make the
        // drift aim at 90 instead, since events apply before the drift.
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"AX\"\nmessage = \"Big win\"\nmorale_delta = 40\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();
        game.state.units.get_mut("1st Test Division").unwrap().elements[0].morale = 20;

        game.end_turn(); // turn becomes 2: the event fires (default 50 -> 90), then drift runs.

        // 20 + ceil((90 - 20) / 10) = 27.
        assert_eq!(game.state.units["1st Test Division"].elements[0].morale, 27);
    }

    #[test]
    fn event_schedule_summary_tracks_fired_status() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"AX\"\nmessage = \"Spring thaw\"\n"
        );
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        assert!(game.event_schedule_summary().contains("pending"));

        game.end_turn(); // ONE_PLAYER: every end_turn completes a full round.

        assert!(game.event_schedule_summary().contains("fired"));
    }
}
