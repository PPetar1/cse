mod detection;
mod entrenchment;
mod events;
mod interdiction;
mod leaders;
mod orders;
mod refit;
mod reinforcements;
mod scenario;
mod supply;
mod turn;
mod victory;
#[cfg(test)]
mod test_support;

pub use scenario::Player;
pub use victory::VictoryReport;
use reinforcements::ScheduledArrival;
use scenario::{ScenarioEvent, VictoryConditions};
use turn::{TurnPhase, TurnSystem};

use std::collections::HashMap;
use time::Date;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::{Location, Terrain};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Game {
    state: State,
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
    /// Messages from events fired since `session.rs` last drained them via
    /// `take_event_messages` — transient, so it starts empty on load too.
    #[serde(skip)]
    pending_event_messages: Vec<String>,
    /// Hexes each fighter-capable unit is currently covering (up to
    /// `interdiction::INTERDICTION_HEX_LIMIT` each), keyed by unit name.
    /// Declared via `interdict`; cleared for a faction's own units the
    /// moment their turn starts again (see `game::interdiction`).
    interdiction_coverage: HashMap<String, Vec<(u32, u32)>>,
    /// This scenario's detection range in hexes, if fog of war is on
    /// (`[fog_of_war]`); `None` means full visibility, the behavior of
    /// every scenario shipped before `game::detection` existed.
    fog_of_war: Option<u32>,
}

/// A target for the `inspect` command: an on-map hex or a named offmap
/// location. Lives here rather than in `command.rs` since `inspect_summary`
/// consumes it; the interface layer imports it from the game layer, never
/// the reverse.
#[derive(Debug, PartialEq)]
pub enum InspectTarget {
    Hex { x: u32, y: u32 },
    Offmap(String),
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
       let fog_of_war = scenario.fog_of_war.as_ref().map(|config| config.detection_range);

       let state = scenario::build_state(scenario)?;

       scenario::validate_victory_hexes(&victory_conditions, &state)?;
       scenario::validate_events(&events, &players)?;
       scenario::validate_arrivals(&scheduled_arrivals, &state)?;

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
           interdiction_coverage: HashMap::new(),
           fog_of_war,
       };
       // begin_turn() only fires from end_turn, so the very first player's
       // turn-1 arrivals/events need an explicit first pass here.
       game.apply_scheduled_arrivals();
       game.apply_scheduled_events();

       Ok(game)
    }

    /// Human-readable listing of every unit visible to the faction on turn
    /// (see `units_by_name`); `detail` swaps the display form for the full
    /// debug dump. Backs the `units` command — returns the text instead of
    /// printing it, like every other summary, so any interface can show it.
    pub fn units_summary(&self, detail: bool) -> String {
        let units = self.units_by_name();
        if units.is_empty() {
            return "No units.".to_string();
        }
        units.iter()
            .map(|unit| if detail { format!("{unit:?}") } else { unit.to_string() })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Units visible to the faction currently on turn, sorted by name (own
    /// units always included; enemy ones only if `is_unit_visible_to` says
    /// so — see `game::detection`) — HashMap iteration order would make the
    /// listing shuffle between runs otherwise.
    pub fn units_by_name(&self) -> Vec<&Unit> {
        let viewer = self.current_faction();
        let mut units: Vec<&Unit> = self.state.units.values()
            .filter(|unit| self.is_unit_visible_to(unit, viewer))
            .collect();
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

    /// All on-map units NOT belonging to `faction`, unsorted — the AI's view
    /// of "enemy units anywhere," used to find the nearest one when there's
    /// no unclaimed objective left.
    pub fn units_not_of_faction(&self, faction: &str) -> Vec<&Unit> {
        self.state.units.values()
            .filter(|unit| unit.faction != faction && matches!(unit.location, UnitLocation::OnMap(_)))
            .collect()
    }

    /// A named unit, if one exists — the reassign-leader prompt's unit
    /// lookup and the GUI's per-marker fort-level lookup both go through
    /// this instead of reaching into `state` directly.
    pub fn unit(&self, name: &str) -> Option<&Unit> {
        self.state.units.get(name)
    }

    /// An on-map hex, if (x, y) is on the map.
    pub fn location(&self, x: u32, y: u32) -> Option<&Location> {
        self.state.map.get_location(x, y)
    }

    /// (x, y)'s neighbouring on-map hexes; empty if the hex doesn't exist or
    /// is offmap. Lets callers outside `game/` (e.g. `ai.rs`) walk hex
    /// adjacency without calling `Location::neighbour_coords()` themselves.
    pub fn adjacent(&self, x: u32, y: u32) -> Vec<(u32, u32)> {
        self.location(x, y).map(Location::neighbour_coords).unwrap_or_default()
    }

    /// Hex distance between `a` and `b`, or `None` if either isn't on the
    /// map. Same reasoning as `adjacent`: keeps `Location::distance_to()`
    /// behind a `Game` query.
    pub fn distance(&self, a: (u32, u32), b: (u32, u32)) -> Option<u32> {
        self.location(a.0, a.1)?.distance_to(self.location(b.0, b.1)?)
    }

    /// A named offmap location (e.g. a reserve box), if one exists.
    pub fn offmap_location(&self, name: &str) -> Option<&Location> {
        self.state.map.get_offmap_location(name)
    }

    /// Every on-map hex's coordinates and terrain, for map rendering
    /// (`gui::map_view::render_map`).
    pub fn map_locations(&self) -> Vec<((u32, u32), Terrain)> {
        self.state.map.all_locations()
    }

    /// Human-readable dump of a hex or offmap location and the units there:
    /// the location, then per unit its `Display` line, TOE, leader,
    /// unit-average morale/experience, and one line per element. Backs the
    /// `inspect` command — returns the text instead of printing it, like
    /// every other summary. A hex outside the current faction's detection
    /// range reports as unknown instead of erroring (see `game::detection`).
    pub fn inspect_summary(&self, target: &InspectTarget) -> Result<String, Error> {
        let location = match target {
            InspectTarget::Hex { x, y } => {
                if !self.is_visible_to(self.current_faction(), *x, *y) {
                    return Ok("Unknown — outside detection range.".to_string());
                }
                self.state.map.get_location(*x, *y).ok_or_else(|| Error::new("Hex not in range."))?
            }
            InspectTarget::Offmap(name) => self.state.map.get_offmap_location(name)
                .ok_or_else(|| Error::new("Location not found."))?,
        };

        let mut lines = vec![location.to_string()];
        for unit in self.units_at_location(location) {
            lines.push(unit.to_string());
            lines.push(format!("TOE: {}", unit.toe));
            lines.push(format!("Leader: {}", unit.leader.as_deref().unwrap_or("none")));
            lines.push(format!(
                "Morale: {}  Experience: {} (unit average)",
                unit.average_morale(), unit.average_experience(),
            ));
            for element in &unit.elements {
                lines.push(format!(
                    "  {}: {} ready, {} damaged — morale {}, experience {}",
                    element.name, element.ready, element.damaged, element.morale, element.experience,
                ));
            }
        }
        Ok(lines.join("\n"))
    }

    /// Whether `unit` can reach `target` for an air mission
    /// (`air_support`/`interdict`) — always fine for a unit still parked
    /// offmap (no coordinate to measure from) or whose TOE sets no `range`;
    /// otherwise the hex distance from its current location must fit.
    pub(super) fn check_mission_range(&self, unit: &Unit, target: (u32, u32)) -> Result<(), Error> {
        let UnitLocation::OnMap(coords) = &unit.location else { return Ok(()) };
        let Some(range) = self.state.toe.get(&unit.toe).and_then(|toe| toe.range) else {
            return Ok(());
        };
        let base = self.state.map.get_location(coords.x, coords.y)
            .expect("unit's own hex vanished");
        let target_location = self.state.map.get_location(target.0, target.1)
            .ok_or_else(|| Error::new("Invalid target location."))?;
        let distance = base.distance_to(target_location).expect("both hexes are on-map");
        if distance > range {
            return Err(Error::new(format!(
                "'{}' is out of range: {distance} hexes away, {range} allowed.", unit.name,
            )));
        }
        Ok(())
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
    fn inspect_summary_reports_unknown_for_a_fogged_hex() {
        let units = format!("{ONMAP_UNIT}\n[fog_of_war]\ndetection_range = 0\n");
        let game = Game::build(minimal_scenario(ONE_PLAYER, &units)).unwrap();

        // Far corner of the 10x8 basic_map, well outside the unit's own hex
        // (1, 1) at detection_range 0.
        let summary = game.inspect_summary(&InspectTarget::Hex { x: 9, y: 7 }).unwrap();

        assert_eq!(summary, "Unknown — outside detection range.");
    }

    #[test]
    fn inspect_summary_lists_units_at_an_offmap_location() {
        let game = Game::build(minimal_scenario(ONE_PLAYER, OFFMAP_UNIT)).unwrap();

        let summary = game.inspect_summary(&InspectTarget::Offmap("GE Reserve".to_string())).unwrap();

        assert!(summary.contains("Reserve Division"));
        assert!(summary.contains("TOE: test_toe"));
    }

    #[test]
    fn inspect_summary_rejects_an_unknown_offmap_location() {
        let game = one_unit_game();

        let error = game.inspect_summary(&InspectTarget::Offmap("Nowhere".to_string())).unwrap_err();

        assert!(error.error_message.contains("not found"));
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
