mod events;
mod interdiction;
mod orders;
mod refit;
mod reinforcements;
mod scenario;
mod supply;
mod turn;
mod victory;
#[cfg(test)]
mod test_support;

pub use scenario::{Player, Scenario};
pub use victory::VictoryReport;
use reinforcements::ScheduledArrival;
use scenario::{ScenarioEvent, SupplySource, VictoryConditions};
use turn::{TurnPhase, TurnSystem};

use std::collections::HashMap;
use time::Date;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::Location;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Game {
    pub state: State,
    scenario_name: String,
    players: Vec<Player>,
    turn_system: TurnSystem,
    turn: u32,
    phase: TurnPhase,
    /// The in-game date of the current turn; advances by `turn_length` days
    /// whenever a full turn (every player moved) completes.
    date: Date,
    turn_length: u32,
    victory_conditions: VictoryConditions,
    /// Reinforcements and withdrawals due at a specific turn, applied to the
    /// owning faction's units the moment that turn starts for them.
    scheduled_arrivals: Vec<ScheduledArrival>,
    /// Scenario events: a message plus an optional morale/experience nudge
    /// to a faction's default, due at a specific turn.
    events: Vec<ScenarioEvent>,
    /// Messages from events fired since `run` last drained them via
    /// `take_event_messages` — transient, so it starts empty on load too.
    #[serde(skip)]
    pending_event_messages: Vec<String>,
    /// Each faction's supply-source hexes, for tracing which of its units
    /// are connected back to them (see `game::supply`).
    supply_sources: Vec<SupplySource>,
    /// Hexes each fighter-capable unit is currently covering (up to
    /// `interdiction::INTERDICTION_HEX_LIMIT` each), keyed by unit name.
    /// Declared via `interdict`; cleared for a faction's own units the
    /// moment their turn starts again (see `game::interdiction`).
    interdiction_coverage: HashMap<String, Vec<(u32, u32)>>,
}

impl Game {
    pub fn build(scenario_toml: String) -> Result<Game, Error> {
       Game::parse_scen_from_toml(scenario_toml) 
    }

    fn parse_scen_from_toml(scenario_toml: String) -> Result<Game, Error>  {
       let mut scenario = scenario::parse(&scenario_toml)?;

       let players = scenario.players.clone();
       let scenario_name = scenario.name.clone();
       let turn_system = scenario.turn_system;
       let date = scenario.start_date;
       let turn_length = scenario.turn_length;
       let victory_conditions = scenario.victory_conditions.clone();
       // UnitLocationConfig isn't Clone (untagged enums stay minimal), so take
       // these out of the scenario rather than cloning them.
       let reinforcements = std::mem::take(&mut scenario.reinforcements);
       let withdrawals = std::mem::take(&mut scenario.withdrawals);
       let scheduled_arrivals: Vec<ScheduledArrival> = reinforcements.into_iter()
           .chain(withdrawals)
           .map(ScheduledArrival::from)
           .collect();
       let events = scenario.events.clone();
       let supply_sources = scenario.supply_sources.clone();

       let state = State::build(scenario)?;

       scenario::validate_victory_hexes(&victory_conditions, &state)?;
       scenario::validate_events(&events, &players)?;
       scenario::validate_arrivals(&scheduled_arrivals, &state)?;
       scenario::validate_supply_sources(&supply_sources, &state, &players)?;

       let mut game = Game {
           state,
           scenario_name,
           players,
           turn_system,
           turn: 1,
           phase: TurnPhase { player_on_turn: 0 },
           date,
           turn_length,
           victory_conditions,
           scheduled_arrivals,
           events,
           pending_event_messages: Vec::new(),
           supply_sources,
           interdiction_coverage: HashMap::new(),
       };
       // begin_turn() only fires from end_turn, so the very first player's
       // turn-1 arrivals/events need an explicit first pass here.
       game.apply_scheduled_arrivals();
       game.apply_scheduled_events();

       Ok(game)
    }

    pub fn list_units(&self) {
        for unit in self.units_by_name() {
            println!("{}", unit);
        }
    }

    pub fn list_units_detail(&self) {
        for unit in self.units_by_name() {
            println!("{:?}", unit);
        }
    }

    /// All units sorted by name — HashMap iteration order would make the
    /// listing shuffle between runs.
    fn units_by_name(&self) -> Vec<&Unit> {
        let mut units: Vec<&Unit> = self.state.units.values().collect();
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }
    
    /// All on-map units of a faction, sorted by name — the AI's view of
    /// "my units" (offmap units, e.g. still-pending reinforcements, aren't
    /// anything a controller can act on yet).
    pub fn units_of_faction(&self, faction: &str) -> Vec<&Unit> {
        let mut units: Vec<&Unit> = self.state.units.values()
            .filter(|unit| unit.faction == faction && matches!(unit.location, UnitLocation::OnMap(_)))
            .collect();
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }

    /// Units at a location, sorted by name. Sorting matters: the unit index
    /// used by `move_unit` must be stable across calls (HashMap iteration
    /// order is not), and must match what `inspect` shows the player.
    pub fn units_at_location(&self, location: &Location) -> Vec<&Unit> {
        let mut units = Vec::new();
        for unit in self.state.units.values() {
            let units_location = match &unit.location {
                UnitLocation::OnMap(coords) => self.state.map.get_location(coords.x, coords.y),
                UnitLocation::Offmap(name) => self.state.map.get_offmap_location(name),
            };
            if Some(location) == units_location {
                units.push(unit)
            }
        }
        units.sort_by(|a, b| a.name.cmp(&b.name));
        units
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_support::*;

    #[test]
    fn builds_a_game_from_a_minimal_scenario() {
        let game = one_unit_game();

        assert_eq!(game.turn, 1);
        assert_eq!(game.players.len(), 1);
        assert_eq!(game.players[0].faction_tag, "AX");
        assert_eq!(game.state.units.len(), 1);
    }

    #[test]
    fn units_at_location_finds_onmap_and_offmap_units() {
        let units = format!("{ONMAP_UNIT}\n{OFFMAP_UNIT}");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        let hex = game.state.map.get_location(1, 1).unwrap();
        let found = game.units_at_location(hex);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "1st Test Division");

        let reserve = game.state.map.get_offmap_location("GE Reserve").unwrap();
        let found = game.units_at_location(reserve);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Reserve Division");
    }

    #[test]
    fn units_at_location_returns_empty_for_an_empty_hex() {
        let game = one_unit_game();

        let hex = game.state.map.get_location(0, 0).unwrap();
        assert!(game.units_at_location(hex).is_empty());
    }

    #[test]
    fn builds_the_real_basic_scenario() {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen"),
        ).unwrap();
        let game = Game::build(contents).unwrap();

        assert_eq!(game.players.len(), 2);
        assert_eq!(game.state.units.len(), 10);
        // Guards the TOE/element referential integrity of the shipped scenario.
        assert!(game.state.elements.contains_key("SU_45mm_at_gun"));
        // Morale/experience inheritance: the 101st takes the Soviet faction
        // defaults, except its howitzer crews' experience override.
        let infantry = &game.state.units["101st Infantry division"];
        assert_eq!(infantry.mp_left, 16);
        let squads = infantry.elements.iter().find(|e| e.name == "SU_inf_squad").unwrap();
        assert_eq!((squads.morale, squads.experience), (45, 35));
        let howitzers = infantry.elements.iter()
            .find(|e| e.name == "SU_122mm_howitzer_M1938").unwrap();
        assert_eq!((howitzers.morale, howitzers.experience), (45, 55));
    }

    #[test]
    fn builds_the_real_frontline_sector_scenario() {
        let contents = std::fs::read_to_string(
            concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/frontline_sector.scen"),
        ).unwrap();
        let mut game = Game::build(contents).unwrap();

        assert_eq!(game.players.len(), 2);
        // 10 Soviet frontline + 2 reserve + 1 fighter regiment, 8 German
        // infantry + 2 Panzer on the line + 1 Panzer + 1 Stuka wing in reserve.
        assert_eq!(game.state.units.len(), 25);
        // Guards the TOE/element referential integrity of the shipped scenario.
        assert!(game.state.elements.contains_key("GE_37mm_pak"));
        // The continuous Soviet line: every hex from (0, 4) to (9, 4) is held.
        for x in 0..10 {
            let location = game.state.map.get_location(x, 4).unwrap();
            assert_eq!(game.units_at_location(location).first().unwrap().faction, "SU");
        }
        // Turn-1 event already fired (see Game::build's explicit first pass).
        assert_eq!(
            game.take_event_messages(),
            vec!["The assault opens with total surprise; German morale surges.".to_string()],
        );
        assert_eq!(game.event_schedule_summary().matches("pending").count(), 2);
        assert_eq!(game.reinforcement_schedule_summary().matches("pending").count(), 4);
    }
}
