use toml::Table;
use serde::{Deserialize, Serialize};
use time::Date;

use crate::core::State;
use crate::core::unit::*;
use crate::Error;

pub struct Game<'a> {
    pub state: State<'a>,
    players: Vec<Player>,
    turn: u32,
    phase: TurnPhase<'a>,
}

impl<'a> Game<'a> {
    pub fn build(scenario_toml: String) -> Result<Game<'a>, Error<'a>> {
       Game::parse_scen_from_toml(scenario_toml) 
       // Ok(Game {
       //     state: State::build(),
       //     players: vec![Player, Player],
       //     turn: 1,
       //     phase: TurnPhase,
       //     units: Vec::new(),
       //     toe: Vec::new(),
       //     elements: Vec::new(),
       // })
    }

    fn parse_scen_from_toml(scenario_toml: String) -> Result<Game<'a>, Error<'a>>  {
       let scenario: Scenario = toml::from_str(&scenario_toml)?;

       let state = State::build(&scenario)?;

       let mut players = Vec::new();
       if scenario.players.len() == 0 {
           return Err(Error::from_str("The game must have at least 1 player."))
       }
       for player in scenario.players {
            players.push(player);
       }

      let player = Player { faction_name: "".to_string(), faction_tag: "".to_string() };
       let mut game = Game {
           state,
           players,
           turn: 1,
           phase: TurnPhase { player_on_turn: &player },
       };


       let phase = TurnPhase { player_on_turn: &game.players[0] };
       
       game.phase = phase;

       Ok(game)
    }

    pub fn load() -> Result<Game<'a>, Error<'a>> {
        Err(Error { error_message: "Not implemented yet.".to_string(), game: None })
    }

}

#[derive(serde::Deserialize)]
struct Player {
    faction_name: String,
    faction_tag: String,
}
struct TurnPhase<'a> {
    player_on_turn: &'a Player,
}

#[derive(serde::Deserialize)]
pub struct Scenario {
    name: String,
    game_version: String,
    pub map: String,

    start_date: String,
    turn_length: u32,

    players: Vec<Player>,

    pub toe: Vec<Toe_>,
    
    pub elements: Vec<Element>,

    pub units: Vec<Unit_>,
}

#[derive(serde::Deserialize)]
struct Toe_ {
    pub name: String,
    pub size: String,
    pub start_date: String,
    pub end_date: String,
    pub elements: Vec<ElementAmount_>,
}

#[derive(serde::Deserialize)]
struct ElementAmount_ {
    pub name: String,
    pub amount: u32,
}

#[derive(serde::Deserialize)]
struct Unit_ {
    pub name: String,
    pub toe: String,
    pub faction: String,
    pub location: Location_,
}

#[derive(serde::Deserialize)]
struct Location_ {
    pub x: u32,
    pub y: u32,
    pub name: Option<String>,
}
