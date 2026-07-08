//! The game-level `.scen` TOML schema: every type that exists to mirror the
//! scenario file, plus parsing and load-time validation. Domain types the
//! schema references live with their domain instead (`Toe`/`Element` in
//! `core::unit`, `TurnSystem` in `game::turn`); runtime-only types (reports,
//! the postcard-safe `ScheduledArrival`) live with their behavior modules.

use time::Date;

use crate::Error;
use crate::core::State;
use crate::core::location::Terrain;
use crate::core::unit::{Element, LocationCoords, Toe, UnitLocation};

use super::reinforcements::ScheduledArrival;

/// Parse a scenario file's TOML into the schema, rejecting scenarios no
/// game could start from.
pub(super) fn parse(scenario_toml: &str) -> Result<Scenario, Error> {
    let scenario: Scenario = toml::from_str(scenario_toml)?;

    if scenario.players.is_empty() {
        return Err(Error::new("The game must have at least 1 player."));
    }

    Ok(scenario)
}

/// Every victory hex must exist on the map the scenario plays on.
pub(super) fn validate_victory_hexes(
    conditions: &VictoryConditions,
    state: &State,
) -> Result<(), Error> {
    for hex in &conditions.hexes {
        if state.map.get_location(hex.x, hex.y).is_none() {
            return Err(Error::new(format!(
                "Victory hex ({}, {}) is not on the map.", hex.x, hex.y,
            )));
        }
    }
    Ok(())
}

/// Every scheduled reinforcement/withdrawal must name a real unit and a
/// real destination (on-map hex or offmap box).
pub(super) fn validate_arrivals(
    arrivals: &[ScheduledArrival],
    state: &State,
) -> Result<(), Error> {
    for arrival in arrivals {
        if !state.units.contains_key(&arrival.unit) {
            return Err(Error::new(format!(
                "Scheduled arrival references unknown unit '{}'.", arrival.unit,
            )));
        }
        match &arrival.location {
            UnitLocation::OnMap(coords) => {
                if state.map.get_location(coords.x, coords.y).is_none() {
                    return Err(Error::new(format!(
                        "Scheduled arrival for '{}' targets hex ({}, {}) which is not on the map.",
                        arrival.unit, coords.x, coords.y,
                    )));
                }
            }
            UnitLocation::Offmap(name) => {
                if state.map.get_offmap_location(name).is_none() {
                    return Err(Error::new(format!(
                        "Scheduled arrival for '{}' targets offmap location '{}' which does not exist.",
                        arrival.unit, name,
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Every event must belong to a faction that has a player.
pub(super) fn validate_events(
    events: &[ScenarioEvent],
    players: &[Player],
) -> Result<(), Error> {
    for event in events {
        if !players.iter().any(|player| player.faction_tag == event.faction) {
            return Err(Error::new(format!(
                "Event at turn {} references unknown faction '{}'.", event.turn, event.faction,
            )));
        }
    }
    Ok(())
}

/// Every supply source must sit on the map and belong to a faction that has
/// a player.
pub(super) fn validate_supply_sources(
    sources: &[SupplySource],
    state: &State,
    players: &[Player],
) -> Result<(), Error> {
    for source in sources {
        if state.map.get_location(source.x, source.y).is_none() {
            return Err(Error::new(format!(
                "Supply source ({}, {}) is not on the map.", source.x, source.y,
            )));
        }
        if !players.iter().any(|player| player.faction_tag == source.faction) {
            return Err(Error::new(format!(
                "Supply source ({}, {}) references unknown faction '{}'.",
                source.x, source.y, source.faction,
            )));
        }
    }
    Ok(())
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    pub(super) name: String,
    #[allow(dead_code)]
    game_version: String,
    pub map: String,

    pub(super) start_date: Date,
    pub(super) turn_length: u32,
    #[serde(default)]
    pub(super) turn_system: super::turn::TurnSystem,
    /// `[terrain_costs]` — MP to enter a hex per terrain name, 0 = impassable.
    /// Anything unlisted falls back to the code defaults
    /// (`Terrain::default_movement_cost`).
    #[serde(default)]
    pub terrain_costs: std::collections::HashMap<Terrain, u32>,

    pub players: Vec<Player>,

    pub toe: Vec<Toe>,

    pub elements: Vec<Element>,

    pub units: Vec<UnitConfig>,

    /// `[victory_conditions]` — optional; a scenario with none never scores
    /// or ends on its own.
    #[serde(default)]
    pub(super) victory_conditions: VictoryConditions,

    /// `[[reinforcements]]` — units that step onto the map at a scheduled
    /// turn (typically from an offmap box). Mechanically identical to
    /// withdrawals; kept as a separate table only for scenario readability.
    #[serde(default)]
    pub(super) reinforcements: Vec<ScheduledArrivalConfig>,
    /// `[[withdrawals]]` — units that leave the map (typically back to an
    /// offmap box) at a scheduled turn.
    #[serde(default)]
    pub(super) withdrawals: Vec<ScheduledArrivalConfig>,

    /// `[[events]]` — a message plus an optional morale/experience nudge to
    /// a faction's default, due at a scheduled turn.
    #[serde(default)]
    pub(super) events: Vec<ScenarioEvent>,

    /// `[[supply_sources]]` — a faction's supply-source hexes, for tracing
    /// which of its units are connected back to them.
    #[serde(default)]
    pub(super) supply_sources: Vec<SupplySource>,

    /// `[fog_of_war]` — absent means full visibility (every scenario's
    /// behavior before this existed); present, it caps how far a faction
    /// can see past its own units (see `game::detection`).
    #[serde(default)]
    pub(super) fog_of_war: Option<FogOfWarConfig>,
}

/// A scenario's detection range, in hexes, from any of a faction's own
/// on-map units — see `game::detection`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct FogOfWarConfig {
    pub(super) detection_range: u32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct Player {
    pub(super) faction_name: String,
    pub faction_tag: String,
    /// Faction-wide default morale/experience, inherited by every element of
    /// the faction's units unless the unit or element sets its own. Lives on
    /// the runtime player so future events can shift it over time.
    #[serde(default = "default_stat")]
    pub morale: u32,
    #[serde(default = "default_stat")]
    pub experience: u32,
    /// Who plays this faction. Absent = `Human`, so every scenario shipped
    /// before this field existed is unaffected.
    #[serde(default)]
    pub controller: PlayerController,
}

/// Factions that don't specify default morale/experience get an average rating.
fn default_stat() -> u32 {
    50
}

/// Who plays a faction. A future networked mode would add a variant here,
/// not change how this is read (see `Game::current_player_is_ai`).
#[derive(Debug, Clone, Copy, PartialEq, Default, serde::Deserialize, serde::Serialize)]
pub enum PlayerController {
    #[default]
    Human,
    Ai,
}

#[derive(serde::Deserialize)]
pub struct UnitConfig {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: UnitLocationConfig,
    /// Unit-wide morale/experience, inherited by all its elements. Absent =
    /// the faction default from [[players]].
    pub morale: Option<u32>,
    pub experience: Option<u32>,
    /// Per-element stat overrides ([[units.elements]]), the most specific
    /// setting. Names must exist in the unit's TOE.
    #[serde(default)]
    pub elements: Vec<ElementStatsConfig>,
}

#[derive(serde::Deserialize)]
pub struct ElementStatsConfig {
    pub name: String,
    pub morale: Option<u32>,
    pub experience: Option<u32>,
}

/// Scenario-file form of a unit location. Untagged so the TOML reads naturally:
/// `location = { x = 3, y = 3 }` for a hex, `location = "GE Reserve"` for offmap.
#[derive(serde::Deserialize)]
#[serde(untagged)]
pub enum UnitLocationConfig {
    OnMap { x: u32, y: u32 },
    Offmap(String),
}

impl From<UnitLocationConfig> for UnitLocation {
    fn from(config: UnitLocationConfig) -> UnitLocation {
        match config {
            UnitLocationConfig::OnMap { x, y } => UnitLocation::OnMap(LocationCoords { x, y }),
            UnitLocationConfig::Offmap(name) => UnitLocation::Offmap(name),
        }
    }
}

/// TOML shape of one scheduled reinforcement or withdrawal entry: move
/// `unit` to `location` the moment `turn` starts for its faction.
#[derive(serde::Deserialize)]
pub(super) struct ScheduledArrivalConfig {
    pub(super) unit: String,
    pub(super) turn: u32,
    pub(super) location: UnitLocationConfig,
}

/// One scenario event: at `turn`, `faction`'s default morale/experience
/// shifts by the given deltas (0 = no change either way) and `message`
/// prints. No config/runtime split needed here — unlike locations, nothing
/// about this shape is TOML-only.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct ScenarioEvent {
    pub(super) turn: u32,
    pub(super) faction: String,
    pub(super) message: String,
    #[serde(default)]
    pub(super) morale_delta: i32,
    #[serde(default)]
    pub(super) experience_delta: i32,
}

/// One supply source: `faction`'s units trace connectivity back to this hex
/// (see `game::supply`). No config/runtime split needed — plain data, same
/// reasoning as `ScenarioEvent`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct SupplySource {
    pub(super) faction: String,
    pub(super) x: u32,
    pub(super) y: u32,
}

/// How a scenario is won: flat points for holding named hexes at the end,
/// plus points for enemy strength destroyed and a penalty for strength lost
/// (both measured against `State::starting_strength`). `last_turn` is the
/// last turn played; the score is tallied and the scenario ends right after.
#[derive(Debug, Clone, Default, serde::Deserialize, serde::Serialize)]
pub(super) struct VictoryConditions {
    #[serde(default)]
    pub(super) last_turn: Option<u32>,
    #[serde(default)]
    pub(super) hexes: Vec<VictoryHex>,
    #[serde(default)]
    pub(super) points_per_percent_enemy_destroyed: f32,
    #[serde(default)]
    pub(super) points_per_percent_own_lost: f32,
}

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct VictoryHex {
    pub(super) x: u32,
    pub(super) y: u32,
    pub(super) points: f32,
    #[serde(default)]
    #[allow(dead_code)]
    pub(super) name: Option<String>,
}

#[cfg(test)]
mod tests {
    use crate::game::Game;
    use crate::game::test_support::*;

    #[test]
    fn rejects_a_scenario_with_an_invalid_start_date() {
        let scenario = minimal_scenario(ONE_PLAYER, ONMAP_UNIT)
            .replace(r#"start_date = "1941-06-22""#, r#"start_date = "someday""#);

        let error = Game::build(scenario).unwrap_err();
        assert!(error.error_message.contains("start_date"));
    }

    #[test]
    fn rejects_an_unknown_turn_system() {
        let scenario = format!(
            "turn_system = \"Wego\"\n{}",
            minimal_scenario(ONE_PLAYER, ONMAP_UNIT),
        );

        let error = Game::build(scenario).unwrap_err();
        assert!(error.error_message.contains("unknown variant"));
    }

    #[test]
    fn rejects_a_scenario_with_no_players() {
        let error = Game::build(minimal_scenario("players = []", ONMAP_UNIT)).unwrap_err();

        assert!(error.error_message.contains("at least 1 player"));
    }

    #[test]
    fn rejects_an_unknown_terrain_in_terrain_costs() {
        let units = format!("{ONMAP_UNIT}\n[terrain_costs]\nLava = 5\n");

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();
        assert!(error.error_message.contains("unknown variant"));
    }

    #[test]
    fn rejects_a_victory_hex_outside_the_map() {
        let units = r#"
[[units]]
name = "1st Test Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[victory_conditions]
last_turn = 5

[[victory_conditions.hexes]]
x = 999
y = 999
points = 10
"#;
        let error = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap_err();

        assert!(error.error_message.contains("not on the map"));
    }

    #[test]
    fn rejects_a_reinforcement_for_an_unknown_unit() {
        let units = format!(
            "{ONMAP_UNIT}\n[[reinforcements]]\nunit = \"Ghost Division\"\nturn = 2\nlocation = {{ x = 2, y = 2 }}\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("Ghost Division"));
    }

    #[test]
    fn rejects_a_reinforcement_targeting_a_hex_outside_the_map() {
        let units = format!(
            "{OFFMAP_UNIT}\n[[reinforcements]]\nunit = \"Reserve Division\"\nturn = 2\nlocation = {{ x = 999, y = 999 }}\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("not on the map"));
    }

    #[test]
    fn rejects_a_withdrawal_targeting_an_unknown_offmap_location() {
        let units = format!(
            "{ONMAP_UNIT}\n[[withdrawals]]\nunit = \"1st Test Division\"\nturn = 2\nlocation = \"Nowhere\"\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("Nowhere"));
    }

    #[test]
    fn rejects_an_event_for_an_unknown_faction() {
        let units = format!(
            "{ONMAP_UNIT}\n[[events]]\nturn = 2\nfaction = \"ZZ\"\nmessage = \"Ghost event\"\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("ZZ"));
    }

    #[test]
    fn rejects_a_supply_source_outside_the_map() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"AX\"\nx = 999\ny = 999\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("not on the map"));
    }

    #[test]
    fn rejects_a_supply_source_for_an_unknown_faction() {
        let units = format!(
            "{ONMAP_UNIT}\n[[supply_sources]]\nfaction = \"ZZ\"\nx = 0\ny = 0\n"
        );

        let error = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap_err();

        assert!(error.error_message.contains("ZZ"));
    }

    #[test]
    fn unit_stats_come_from_the_scenario_with_defaults() {
        let units = r#"
[[units]]
name = "Rated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
morale = 80
experience = 65

[[units]]
name = "Unrated Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#;
        let game = Game::build(minimal_scenario(ONE_PLAYER, units)).unwrap();

        // Stats live on the elements; a unit-level scenario setting is
        // inherited by all of them.
        let rated = &game.state.units["Rated Division"].elements[0];
        assert_eq!((rated.morale, rated.experience), (80, 65));
        // No unit or faction setting: the default rating.
        let unrated = &game.state.units["Unrated Division"].elements[0];
        assert_eq!((unrated.morale, unrated.experience), (50, 50));
    }
}
