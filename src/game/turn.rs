//! The turn system: how control passes between players and what happens to
//! a faction when its turn begins (MP refill, morale drift, scheduled
//! arrivals and events) and ends (doctrine — see `game::doctrine`). A future
//! WEGO mode (simultaneous orders, resolved together at turn end) lands as a
//! second `TurnSystem` variant plus an order queue — the matches on that
//! enum are the places it plugs in.

use super::{Game, Player, VictoryReport};
use super::scenario::PlayerController;

/// At its faction's turn start every element bucket drifts toward the faction
/// default morale by `ceil(|gap| / MORALE_RECOVERY_STEP)`: battered units
/// recover with rest, battle-euphoric ones settle back down. Gentler than the
/// battle shifts, so combat outcomes dominate the drift.
const MORALE_RECOVERY_STEP: u32 = 10;

impl Game {
    /// End the current player's turn and hand off to the next one. `players`
    /// is grouped by faction (`scenario::build_players`), so a single flat
    /// index sequences both levels: most handoffs just move to the next
    /// player of the same faction ("nothing else," per the design — no
    /// housekeeping runs). Only when the *last* player of a faction ends
    /// their turn do that faction's turn-end effects fire (doctrine, before
    /// the index advances); only when the *first* player of the next
    /// faction's turn begins do that faction's turn-start effects fire
    /// (`begin_turn`). Once every faction's players have all moved, the turn
    /// counter and game date advance. Returns the final score once the
    /// scenario's `last_turn` has just been completed.
    pub fn end_turn(&mut self) -> Option<VictoryReport> {
        let mut victory = None;
        match self.turn_system {
            TurnSystem::IgoUgo => {
                let index = self.phase.player_on_turn as usize;
                if self.is_last_player_of_its_faction(index) {
                    let ending_faction = self.players[index].faction.clone();
                    self.apply_doctrine_turn_end(&ending_faction);
                }

                self.phase.player_on_turn += 1;
                if self.phase.player_on_turn as usize >= self.players.len() {
                    self.phase.player_on_turn = 0;
                    self.turn += 1;
                    self.date += time::Duration::days(self.turn_length.into());
                    if self.victory_conditions.last_turn.is_some_and(|last| self.turn > last) {
                        victory = Some(self.score_victory());
                    }
                }
                if self.is_first_player_of_its_faction(self.phase.player_on_turn as usize) {
                    self.begin_turn();
                }
            }
        }
        victory
    }

    /// Whether `self.players[index]` is the last consecutive player of its
    /// faction — either it's the last entry in `players`, or the next entry
    /// belongs to a different faction.
    fn is_last_player_of_its_faction(&self, index: usize) -> bool {
        index + 1 >= self.players.len() || self.players[index + 1].faction != self.players[index].faction
    }

    /// Whether `self.players[index]` is the first consecutive player of its
    /// faction — either it's the first entry in `players`, or the previous
    /// entry belongs to a different faction. `index == 0` is always true
    /// here even for a single-faction game, since wrapping back to it is a
    /// fresh turn for that faction.
    fn is_first_player_of_its_faction(&self, index: usize) -> bool {
        index == 0 || self.players[index - 1].faction != self.players[index].faction
    }

    /// Turn-start effects for the faction coming on turn: scheduled
    /// reinforcements/withdrawals and scenario events land first (an event's
    /// morale/experience delta feeds straight into the same turn's drift
    /// target below), then refit (repair/replacements for units connected to
    /// supply), then entrenchment (a fort level for every unit still standing
    /// where it was), then a fresh movement budget from the TOE, interdiction
    /// coverage resetting (it must be redeclared every time this faction
    /// acts), and morale drifting back toward the faction default (rest
    /// heals battered units, euphoria fades). Runs once per faction turn
    /// (see `end_turn`), not once per player.
    fn begin_turn(&mut self) {
        let just_arrived = self.apply_scheduled_arrivals();
        self.apply_scheduled_events();
        self.apply_refit();
        self.apply_entrenchment(&just_arrived);

        let faction = self.player_on_turn().faction.clone();
        let default_morale = self.faction_by_tag(&faction)
            .expect("on-turn player's faction vanished").morale;
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

    /// One-line summary of where the game clock stands. The player's name is
    /// only shown alongside the faction's when that faction has more than one
    /// player — naming the sole player is redundant.
    pub fn status(&self) -> String {
        let index = self.phase.player_on_turn as usize;
        let player = self.player_on_turn();
        let faction = self.faction_by_tag(&player.faction)
            .expect("on-turn player's faction vanished");
        let is_sole_player = self.is_first_player_of_its_faction(index) && self.is_last_player_of_its_faction(index);
        let who = if is_sole_player {
            faction.faction_name.clone()
        } else {
            format!("{} ({})", faction.faction_name, player.name)
        };
        format!(
            "{} — turn {}, {}. {} to move.",
            self.scenario_name, self.turn, self.date, who,
        )
    }

    pub(super) fn player_on_turn(&self) -> &Player {
        &self.players[self.phase.player_on_turn as usize]
    }

    /// The faction tag of the player currently on turn — the seam any
    /// controller-aware logic outside `game` (the AI today) reads to know
    /// whose move it is.
    pub fn current_faction(&self) -> &str {
        &self.player_on_turn().faction
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
        // No [[players]] listed: each faction has a single default player,
        // so status shows just the faction name — naming the sole player
        // would be redundant.
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

    /// Two named Axis players (Alice, Bob) sharing the Axis faction, plus
    /// Soviet Union with no players listed (a default "Soviet Union" player)
    /// — `OPPOSING_UNITS`' two divisions on top.
    fn two_players_one_faction() -> String {
        format!(
            "{OPPOSING_UNITS}\n[[players]]\nname = \"Alice\"\nfaction = \"AX\"\n\
             [[players]]\nname = \"Bob\"\nfaction = \"AX\"\n",
        )
    }

    #[test]
    fn handoff_between_two_players_of_one_faction_does_nothing_else() {
        let factions = r#"
[[factions]]
faction_name = "Axis"
faction_tag = "AX"
[[factions]]
faction_name = "Soviet Union"
faction_tag = "SU"
"#;
        let mut game = Game::build(minimal_scenario(factions, &two_players_one_faction())).unwrap();
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Axis (Alice) to move.");

        game.move_unit(1, 1, 1, 2, 0).unwrap();
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Alice hands off to Bob — still Axis's own turn: no MP refill, no
        // morale drift, nothing but the player index moving.
        game.end_turn();
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Axis (Bob) to move.");
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Bob (the last Axis player) hands off to Soviet Union: Axis's turn
        // has ended and Soviet's has begun, but Axis's own MP only refills
        // when Axis's turn starts again, not now.
        game.end_turn();
        assert_eq!(game.status(), "test scenario — turn 1, 1941-06-22. Soviet Union to move.");
        assert_eq!(game.state.units["Axis Division"].mp_left, 14);

        // Soviet Union (only one player) ends its turn: every faction has
        // now moved, so the full round completes and Axis's turn begins
        // again — Alice is back up, with fresh Axis MP.
        game.end_turn();
        assert_eq!(game.status(), "test scenario — turn 2, 1941-06-29. Axis (Alice) to move.");
        assert_eq!(game.state.units["Axis Division"].mp_left, 16);
    }

    #[test]
    fn doctrine_turn_end_fires_once_after_a_factions_last_player_not_every_handoff() {
        // Same setup as doctrine_updates_when_a_factions_own_turn_ends...,
        // but with two Axis players: Guderian's doctrine must stay put while
        // Alice and Bob hand off between themselves, and only drift once Bob
        // (the last Axis player) ends Axis's turn.
        let leader = r#"
[[leaders]]
name = "Guderian"
faction = "AX"
doctrine = 70
[leaders.stats]
political = 5
morale = 5
initiative = 5
administration = 5
mechanized = 5
infantry = 5
air = 5
"#;
        let factions = r#"
[[factions]]
faction_name = "Axis"
faction_tag = "AX"
[[factions]]
faction_name = "Soviet Union"
faction_tag = "SU"
"#;
        let units = format!("{}\n{leader}", two_players_one_faction());
        let mut game = Game::build(minimal_scenario(factions, &units)).unwrap();
        assert_eq!(game.state.leaders["Guderian"].doctrine, 70);

        game.end_turn(); // Alice -> Bob: still Axis's own turn.
        assert_eq!(game.state.leaders["Guderian"].doctrine, 70);

        game.end_turn(); // Bob -> Soviet Union: Axis's turn ends.
        assert_eq!(game.state.leaders["Guderian"].doctrine, 69);
    }

    #[test]
    fn doctrine_updates_when_a_factions_own_turn_ends_not_when_the_next_ones_starts() {
        // Guderian's doctrine (70) sits above AX's unspecified default (50);
        // ending AX's own turn — not SU's turn starting next — must drift
        // him down right away. ((50-70)/10) * ((15-5)/15) = -1.3333; 70 -
        // 1.3333 = 68.6667, rounds to 69. (The leader-to-faction contribution
        // this same step barely moves FDO: it stays 50, rounded.)
        let leader = r#"
[[leaders]]
name = "Guderian"
faction = "AX"
doctrine = 70
[leaders.stats]
political = 5
morale = 5
initiative = 5
administration = 5
mechanized = 5
infantry = 5
air = 5
"#;
        let units = format!("{OPPOSING_UNITS}\n{leader}");
        let mut game = Game::build(minimal_scenario(TWO_PLAYERS, &units)).unwrap();
        assert_eq!(game.state.leaders["Guderian"].doctrine, 70);

        game.end_turn(); // Axis ends its own turn; Soviet Union's turn starts.

        assert_eq!(game.state.leaders["Guderian"].doctrine, 69);
    }
}
