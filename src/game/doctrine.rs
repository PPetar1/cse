//! Faction doctrine: a faction-wide combat-effectiveness rating (see
//! `Player::doctrine`) that scales unit CV in battle just like experience
//! does (`procedures::combat::stat_modifier`/`fire_round`), and that leaders
//! both draw from and feed back into over time.
//!
//! Two independent effects, both driven by `Leader::doctrine` (1-100,
//! resolved from the scenario or the faction default — see
//! `game::scenario::LeaderConfig`):
//!
//! - **Battle result** (`apply_doctrine_battle_result`, called from
//!   `game::orders::attack`): each side's battle is credited to a single
//!   leader — the one with the highest `average_leader_value` among the
//!   side's participating units' leaders (see "Battle leadership" in
//!   docs/ideas.md for why only one, for now). That leader's personal
//!   doctrine shifts by `(LAV - DOC/10) * FBO * LOS`, never crossing the
//!   `LAV * 10` ceiling/floor its own rating caps it at.
//! - **Turn end** (`apply_doctrine_turn_end`, called from `game::turn::end_turn`
//!   for the faction whose turn just finished, before control passes):
//!   every leader of that faction first nudges the faction doctrine value up
//!   or down (leaders above pull it up, leaders below pull it down —
//!   computed from a single pre-update snapshot so no leader's contribution
//!   is skewed by another's), then drifts personally toward the (now
//!   current) faction value.

use crate::core::leader::LeaderStats;
use crate::procedures::combat::BattleReport;

use super::Game;

/// Final battle odds are capped to this range before scaling a leader's
/// doctrine gain/loss — a rout or an overrun contributes no more than a
/// solid, ordinary win/loss would.
const FBO_MIN: f32 = 0.5;
const FBO_MAX: f32 = 2.0;

/// Divides total element-instance losses (destroyed + damaged, both sides)
/// into the 0-1 `LOS` term — see `apply_doctrine_battle_result`.
const LOSSES_FOR_MAX_LOS: f32 = 500.0;

impl Game {
    /// A faction's current doctrine rating, 1-100. Passing an undefined
    /// faction is a caller bug.
    pub(super) fn doctrine_of(&self, faction: &str) -> u32 {
        self.faction_by_tag(faction).expect("no such faction").doctrine
    }

    /// Credit (or debit) each side's battle leader — see the module doc for
    /// selection and the formula. Must run before any unit in `attacker_names`
    /// /`defender_names` can be removed (a surrendered or shattered defender
    /// — see `execute_retreat`), since leader lookup goes through the unit.
    pub(super) fn apply_doctrine_battle_result(
        &mut self,
        attacker_names: &[String],
        defender_names: &[String],
        battle: &BattleReport,
    ) {
        let los = battle_los(battle);
        self.apply_doctrine_to_side(attacker_names, side_fbo(battle.attacker_cv, battle.defender_cv), los);
        self.apply_doctrine_to_side(defender_names, side_fbo(battle.defender_cv, battle.attacker_cv), los);
    }

    fn apply_doctrine_to_side(&mut self, unit_names: &[String], fbo: f32, los: f32) {
        let Some(leader_name) = self.battle_leader(unit_names) else { return };
        let leader = self.state.leaders.get_mut(&leader_name).expect("battle leader vanished");
        let lav = average_leader_value(&leader.stats);
        let gain = (lav - leader.doctrine as f32 / 10.0) * fbo * los;
        let asymptote = lav * 10.0;
        let mut doctrine = leader.doctrine as f32 + gain;
        if gain > 0.0 {
            doctrine = doctrine.min(asymptote);
        } else if gain < 0.0 {
            doctrine = doctrine.max(asymptote);
        }
        leader.doctrine = doctrine.round().clamp(0.0, 100.0) as u32;
    }

    /// The leader who commands the battle for one side: the highest
    /// `average_leader_value` among the leaders of `unit_names`' units, ties
    /// broken by name for determinism. `None` if no participating unit has a
    /// leader assigned.
    fn battle_leader(&self, unit_names: &[String]) -> Option<String> {
        unit_names.iter()
            .filter_map(|name| self.state.units.get(name)?.leader.as_deref())
            .filter_map(|leader_name| self.state.leaders.get(leader_name).map(|leader| (leader_name, leader)))
            .max_by(|(name_a, a), (name_b, b)| {
                average_leader_value(&a.stats).partial_cmp(&average_leader_value(&b.stats))
                    .expect("leader ratings are never NaN")
                    .then_with(|| name_a.cmp(name_b))
            })
            .map(|(name, _)| name.to_string())
    }

    /// Turn-end doctrine processing for the faction whose turn just
    /// finished: leaders update the faction value first
    /// (`leader_contributions_to_faction_doctrine`), then drift toward it
    /// (`drift_leaders_toward_faction_doctrine`) — see the module doc for
    /// why the order matters.
    pub(super) fn apply_doctrine_turn_end(&mut self, faction: &str) {
        self.apply_leader_contributions_to_faction_doctrine(faction);
        self.drift_leaders_toward_faction_doctrine(faction);
    }

    fn apply_leader_contributions_to_faction_doctrine(&mut self, faction: &str) {
        let Some(fdo) = self.faction_by_tag(faction).map(|faction| faction.doctrine as f32) else {
            return;
        };

        // Every leader's contribution is computed against the same
        // pre-update `fdo` snapshot, then summed and applied once — so the
        // first leader processed doesn't skew what the rest contribute.
        let mut total_delta = 0.0;
        for leader in self.state.leaders.values().filter(|leader| leader.faction == faction) {
            let doc = leader.doctrine as f32;
            if doc > fdo {
                total_delta += ((doc - fdo) / 100.0) * (1.0 / (11.0 - leader.stats.initiative as f32));
            } else if doc < fdo {
                total_delta -= ((fdo - doc) / 100.0) * (1.0 / (11.0 - leader.stats.political as f32));
            }
        }

        if let Some(scored_faction) = self.factions.iter_mut().find(|f| f.faction_tag == faction) {
            scored_faction.doctrine =
                (scored_faction.doctrine as f32 + total_delta).round().clamp(0.0, 100.0) as u32;
        }
    }

    fn drift_leaders_toward_faction_doctrine(&mut self, faction: &str) {
        let Some(fdo) = self.faction_by_tag(faction).map(|faction| faction.doctrine as f32) else {
            return;
        };

        for leader in self.state.leaders.values_mut().filter(|leader| leader.faction == faction) {
            let doc = leader.doctrine as f32;
            let delta = if doc > fdo {
                // Losing doctrine: resistance scales with initiative.
                ((fdo - doc) / 10.0) * ((15.0 - leader.stats.initiative as f32) / 15.0)
            } else {
                // Gaining doctrine: resistance scales with political rating.
                ((fdo - doc) / 10.0) * ((15.0 - leader.stats.political as f32) / 15.0)
            };
            leader.doctrine = (doc + delta).round().clamp(0.0, 100.0) as u32;
        }
    }
}

/// The average of a leader's ratings other than political and air — both
/// their doctrine ceiling/floor for battle results (`LAV * 10`) and, tied to
/// a unit, their claim to lead a battle (`Game::battle_leader`).
pub(super) fn average_leader_value(stats: &LeaderStats) -> f32 {
    (stats.morale + stats.initiative + stats.administration + stats.mechanized + stats.infantry) as f32 / 5.0
}

/// Final battle odds for one side: its final CV over the enemy's, capped to
/// [`FBO_MIN`, `FBO_MAX`]. An overrun (enemy CV 0) naturally divides to
/// infinity and clamps to `FBO_MAX`; a mutual wipeout (both 0, no battle
/// fought at all) is treated as even.
fn side_fbo(own_cv: f32, enemy_cv: f32) -> f32 {
    let ratio = if own_cv == 0.0 && enemy_cv == 0.0 { 1.0 } else { own_cv / enemy_cv };
    ratio.clamp(FBO_MIN, FBO_MAX)
}

/// `LOS`: how costly the battle was, 0-1, from total element-instance losses
/// (destroyed + damaged) on both sides combined — deliberately excluding
/// retreat/rout/surrender attrition, which `BattleReport` doesn't carry
/// (see `game::orders::attack::execute_retreat`).
fn battle_los(battle: &BattleReport) -> f32 {
    let losses = battle.attacker_losses.damaged + battle.attacker_losses.destroyed
        + battle.defender_losses.damaged + battle.defender_losses.destroyed;
    (losses as f32 / LOSSES_FOR_MAX_LOS).min(1.0)
}

#[cfg(test)]
mod tests {
    use crate::core::leader::LeaderStats;
    use crate::game::Game;
    use crate::game::test_support::*;
    use crate::procedures::combat::{BattleOutcome, BattleReport, Losses};

    use super::{average_leader_value, battle_los, side_fbo};

    fn stats(political: u32, morale: u32, initiative: u32, administration: u32, mechanized: u32, infantry: u32, air: u32) -> LeaderStats {
        LeaderStats { political, morale, initiative, administration, mechanized, infantry, air }
    }

    fn empty_battle() -> BattleReport {
        BattleReport {
            rounds: Vec::new(),
            attacker_losses: Losses::default(),
            defender_losses: Losses::default(),
            attacker_cv: 0.0,
            defender_cv: 0.0,
            outcome: BattleOutcome::DefenderHolds,
        }
    }

    #[test]
    fn average_leader_value_excludes_political_and_air() {
        // (7 + 8 + 4 + 7 + 5) / 5 = 6.2 — political 5 and air 1 play no part.
        let stats = stats(5, 7, 8, 4, 7, 5, 1);
        assert_eq!(average_leader_value(&stats), 6.2);
    }

    #[test]
    fn side_fbo_caps_between_half_and_double() {
        assert_eq!(side_fbo(30.0, 10.0), 2.0); // 3:1 caps down to 2.0.
        assert_eq!(side_fbo(10.0, 30.0), 0.5); // 1:3 caps up to 0.5.
        assert_eq!(side_fbo(20.0, 10.0), 2.0); // Exactly at the cap.
        assert_eq!(side_fbo(0.0, 10.0), 0.5); // Wiped out: floors at 0.5.
        assert_eq!(side_fbo(10.0, 0.0), 2.0); // Overrun: infinity caps at 2.0.
    }

    #[test]
    fn side_fbo_treats_a_mutual_wipeout_as_even() {
        assert_eq!(side_fbo(0.0, 0.0), 1.0);
    }

    #[test]
    fn battle_los_counts_destroyed_and_damaged_both_sides_clamped_to_one() {
        let mut battle = empty_battle();
        battle.attacker_losses = Losses { disrupted: 5, damaged: 3, destroyed: 2 };
        battle.defender_losses = Losses { disrupted: 0, damaged: 1, destroyed: 1 };

        // Disrupted losses don't count: (3 + 2 + 1 + 1) / 500 = 0.014.
        assert_eq!(battle_los(&battle), 7.0 / 500.0);

        battle.attacker_losses.destroyed = 600;
        assert_eq!(battle_los(&battle), 1.0);
    }

    const TWO_LEADERS: &str = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
leader = "High LAV"

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
leader = "Low LAV"

[[units]]
name = "Axis Third"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[leaders]]
name = "High LAV"
faction = "AX"
[leaders.stats]
political = 1
morale = 9
initiative = 9
administration = 9
mechanized = 9
infantry = 9
air = 1

[[leaders]]
name = "Low LAV"
faction = "AX"
[leaders.stats]
political = 9
morale = 1
initiative = 1
administration = 1
mechanized = 1
infantry = 1
air = 9
"#;

    #[test]
    fn battle_leader_picks_the_highest_average_rating() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, TWO_LEADERS)).unwrap();

        let leader = game.battle_leader(&["Axis First".to_string(), "Axis Second".to_string()]);

        assert_eq!(leader.as_deref(), Some("High LAV"));
    }

    #[test]
    fn battle_leader_ignores_units_without_a_leader() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, TWO_LEADERS)).unwrap();

        // "Axis Third" has no leader assigned; only "Axis Second" (led by
        // "Low LAV") counts.
        let leader = game.battle_leader(&["Axis Third".to_string(), "Axis Second".to_string()]);

        assert_eq!(leader.as_deref(), Some("Low LAV"));
    }

    #[test]
    fn battle_leader_is_none_without_any_participating_leader() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, TWO_LEADERS)).unwrap();

        assert_eq!(game.battle_leader(&["Axis Third".to_string()]), None);
    }

    #[test]
    fn battle_leader_breaks_a_tie_by_name() {
        let units = r#"
[[units]]
name = "Axis First"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
leader = "Alpha"

[[units]]
name = "Axis Second"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
leader = "Beta"

[[leaders]]
name = "Alpha"
faction = "AX"
[leaders.stats]
political = 5
morale = 5
initiative = 5
administration = 5
mechanized = 5
infantry = 5
air = 5

[[leaders]]
name = "Beta"
faction = "AX"
[leaders.stats]
political = 5
morale = 5
initiative = 5
administration = 5
mechanized = 5
infantry = 5
air = 5
"#;
        let game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        let leader = game.battle_leader(&["Axis First".to_string(), "Axis Second".to_string()]);

        assert_eq!(leader.as_deref(), Some("Beta"));
    }

    /// A single leader, `doctrine` and `stats` set by the caller, assigned to
    /// "1st Test Division" — the battle-result and turn-end tests build on
    /// this instead of repeating the scenario TOML.
    fn one_leader_game(doctrine: u32, stats: LeaderStats) -> Game {
        let units = format!(
            "{ONMAP_UNIT}\n[[leaders]]\nname = \"Leader\"\nfaction = \"AX\"\ndoctrine = {doctrine}\n\
             [leaders.stats]\npolitical = {}\nmorale = {}\ninitiative = {}\nadministration = {}\n\
             mechanized = {}\ninfantry = {}\nair = {}\n",
            stats.political, stats.morale, stats.initiative,
            stats.administration, stats.mechanized, stats.infantry, stats.air,
        )
        .replace(
            "faction = \"AX\"\nlocation",
            "faction = \"AX\"\nleader = \"Leader\"\nlocation",
        );
        Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap()
    }

    #[test]
    fn apply_doctrine_to_side_applies_the_gain_formula() {
        // LAV = (6+6+6+6+6)/5 = 6.0. gain = (6.0 - 50/10) * 2.0 * 1.0 = 2.0.
        let mut game = one_leader_game(50, stats(5, 6, 6, 6, 6, 6, 5));

        game.apply_doctrine_to_side(&["1st Test Division".to_string()], 2.0, 1.0);

        assert_eq!(game.state.leaders["Leader"].doctrine, 52);
    }

    #[test]
    fn apply_doctrine_to_side_caps_a_gain_at_the_lav_asymptote() {
        // LAV = 6.0, asymptote 60. An oversized fbo/los would overshoot past
        // it in one step; the cap holds it at exactly the asymptote.
        let mut game = one_leader_game(50, stats(5, 6, 6, 6, 6, 6, 5));

        game.apply_doctrine_to_side(&["1st Test Division".to_string()], 100.0, 1.0);

        assert_eq!(game.state.leaders["Leader"].doctrine, 60);
    }

    #[test]
    fn apply_doctrine_to_side_caps_a_loss_at_the_lav_asymptote() {
        // LAV = 6.0, asymptote 60. Doctrine starts above it; an oversized
        // loss floors at the asymptote instead of crashing through it.
        let mut game = one_leader_game(90, stats(5, 6, 6, 6, 6, 6, 5));

        game.apply_doctrine_to_side(&["1st Test Division".to_string()], 100.0, 1.0);

        assert_eq!(game.state.leaders["Leader"].doctrine, 60);
    }

    #[test]
    fn apply_doctrine_to_side_leaves_a_leaderless_side_untouched() {
        let mut game = Game::build(minimal_scenario(ONE_PLAYER, ONMAP_UNIT)).unwrap();

        // Must not panic without a battle leader to credit.
        game.apply_doctrine_to_side(&["1st Test Division".to_string()], 2.0, 1.0);
    }

    fn faction_doctrine(game: &Game) -> u32 {
        game.factions.iter().find(|faction| faction.faction_tag == "AX").unwrap().doctrine
    }

    #[test]
    fn faction_contribution_below_fdo_leaders_pull_it_down() {
        // Two leaders above FDO=100, political irrelevant, initiative 9:
        // each contributes -((100-0)/100)*(1/(11-9)) = -0.5, total -1.0.
        let units = format!(
            "{ONMAP_UNIT}\n[[leaders]]\nname = \"A\"\nfaction = \"AX\"\ndoctrine = 0\n[leaders.stats]\npolitical = 5\nmorale = 5\ninitiative = 5\nadministration = 5\nmechanized = 5\ninfantry = 5\nair = 5\n\
             [[leaders]]\nname = \"B\"\nfaction = \"AX\"\ndoctrine = 0\n[leaders.stats]\npolitical = 9\nmorale = 5\ninitiative = 5\nadministration = 5\nmechanized = 5\ninfantry = 5\nair = 5\n",
        );
        let mut game = Game::build(minimal_scenario_with_doctrine(100, &units)).unwrap();

        game.apply_leader_contributions_to_faction_doctrine("AX");

        assert_eq!(faction_doctrine(&game), 99);
    }

    #[test]
    fn faction_contribution_above_and_below_fdo_leaders_offset_correctly() {
        // Regression for the sign fix: A (doctrine 100, above FDO 50,
        // initiative 9) contributes +((100-50)/100)*(1/(11-9)) = +0.25; B
        // (doctrine 0, below FDO 50, political 9) must pull FDO *down* by
        // the same magnitude, not push it up too — net zero, FDO unchanged.
        // The old (buggy) sign would instead land this at 51.
        let units = format!(
            "{ONMAP_UNIT}\n[[leaders]]\nname = \"A\"\nfaction = \"AX\"\ndoctrine = 100\n[leaders.stats]\npolitical = 5\nmorale = 5\ninitiative = 9\nadministration = 5\nmechanized = 5\ninfantry = 5\nair = 5\n\
             [[leaders]]\nname = \"B\"\nfaction = \"AX\"\ndoctrine = 0\n[leaders.stats]\npolitical = 9\nmorale = 5\ninitiative = 5\nadministration = 5\nmechanized = 5\ninfantry = 5\nair = 5\n",
        );
        let mut game = Game::build(minimal_scenario_with_doctrine(50, &units)).unwrap();

        game.apply_leader_contributions_to_faction_doctrine("AX");

        assert_eq!(faction_doctrine(&game), 50);
    }

    #[test]
    fn leader_drift_toward_a_higher_faction_doctrine_uses_the_political_rating() {
        // Gaining (doc 30 < fdo 50): ((50-30)/10) * ((15-5)/15) = 2 * 0.6667
        // = 1.3333; 30 + 1.3333 = 31.3333, rounds to 31.
        let mut game = one_leader_game(30, stats(5, 5, 5, 5, 5, 5, 5));
        game.factions[0].doctrine = 50;

        game.drift_leaders_toward_faction_doctrine("AX");

        assert_eq!(game.state.leaders["Leader"].doctrine, 31);
    }

    #[test]
    fn leader_drift_toward_a_lower_faction_doctrine_uses_the_initiative_rating() {
        // Losing (doc 70 > fdo 50): ((50-70)/10) * ((15-5)/15) = -2 * 0.6667
        // = -1.3333; 70 - 1.3333 = 68.6667, rounds to 69.
        let mut game = one_leader_game(70, stats(5, 5, 5, 5, 5, 5, 5));
        game.factions[0].doctrine = 50;

        game.drift_leaders_toward_faction_doctrine("AX");

        assert_eq!(game.state.leaders["Leader"].doctrine, 69);
    }
}
