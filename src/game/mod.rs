use either::Either;

use crate::core::State;
use crate::Error;
use crate::core::unit::*;
use crate::core::location::Location;

#[derive(serde::Deserialize)]
pub struct Game {
    pub state: State,
    players: Vec<Player>,
    turn: u32,
    phase: TurnPhase,
}

impl Game {
    pub fn build(scenario_toml: String) -> Result<Game, Error> {
       Game::parse_scen_from_toml(scenario_toml) 
    }

    fn parse_scen_from_toml(scenario_toml: String) -> Result<Game, Error>  {
       let scenario: Scenario = toml::from_str(&scenario_toml)?;

       if scenario.players.len() == 0 {
           return Err(Error::from_str("The game must have at least 1 player."))
       }
       
       let mut players = Vec::new();
       for player in &scenario.players {
            players.push(player.clone());
       }

       let state = State::build(scenario)?;
       
       let game = Game {
           state,
           players,
           turn: 1,
           phase: TurnPhase { player_on_turn: 0 },
       };

       Ok(game)
    }

    pub fn load() -> Result<Game, Error> {
        Err(Error { error_message: "Not implemented yet.".to_string() })
    }

    pub fn list_units(&self) {
        for (_, unit) in &self.state.units {
            println!("{}", unit);
        }
    }

    pub fn list_units_detail(&self) {
        for (_, unit) in &self.state.units {
            println!("{:?}", unit);
        }
    }
    
    pub fn units_at_location(&self, location: &Location) -> Vec<&Unit> {
        let mut units = Vec::new();
        for (_, unit) in &self.state.units {
            if Some(location) == unit.location.as_ref().either(
                |location_coords| self.state.map.get_location(location_coords.x, location_coords.y), 
                |offmap_location| self.state.map.get_offmap_location(&offmap_location.name)
            ) {
                units.push(unit)
            }
        }
        units
    }

    pub fn move_unit(&mut self, x_start: u32, y_start: u32, x_end: u32, y_end: u32, unit_i: usize) -> Result<(), Error> {
        self.state.map.get_location(x_start, y_start).ok_or(Error {
            error_message: "Invalid starting location.".to_string(),
        })?;
        self.state.map.get_location(x_end, y_end).ok_or(Error {
                error_message: "Invalid destination.".to_string(),
        })?;
       
        let location_start = LocationCoords { x: x_start, y: y_start };
        let location_end = LocationCoords { x: x_end, y: y_end };
        let mut units = Vec::new();
        for (_, unit) in &mut self.state.units {
            if let Some(location) = unit.location.as_ref().left() {
                if *location == location_start {
                   units.push(unit)
                }
            }
        }
    
        if units.len() < unit_i + 1 {
            return Err(Error::from_str("No unit with index {} at {}"))
        }
        
        units[unit_i].location = Either::Left(location_end);
      
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct Player {
    faction_name: String,
    faction_tag: String,
}

impl Player {
    fn clone(&self) -> Player {
        Player {
            faction_name: self.faction_name.clone(),
            faction_tag: self.faction_tag.clone(),
        }
    }
}

#[derive(serde::Deserialize)]
struct TurnPhase {
    player_on_turn: u32,
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    name: String,
    game_version: String,
    pub map: String,

    start_date: String,
    turn_length: u32,

    players: Vec<Player>,

    pub toe: Vec<Toe>,
    
    pub elements: Vec<Element>,

    pub units: Vec<Unit_>,
}

#[derive(serde::Deserialize)]
pub struct Unit_ {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: Either<LocationCoords, OffmapLocationName>,
}
