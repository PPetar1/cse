use crate::core::State;
use crate::core::unit::*;
use crate::Error;

pub struct Game<'a> {
    pub state: State,
    players: Vec<Player>,
    turn: u32,
    phase: TurnPhase,
    units: Vec<Unit<'a>>,
    toe: Vec<Toe<'a>>,
    elements: Vec<Element>,
}

impl<'a> Game<'a> {
    pub fn build() -> Result<Game<'a>, Error<'a>> {
        Ok(Game {
            state: State::build(),
            players: vec![Player, Player],
            turn: 1,
            phase: TurnPhase,
            units: Vec::new(),
            toe: Vec::new(),
            elements: Vec::new(),
        })
    }

    pub fn load() -> Result<Game<'a>, Error<'a>> {
        Ok(Game {
            state: State::build(),
            players: vec![Player, Player],
            turn: 1,
            phase: TurnPhase,
            units: Vec::new(),
            toe: Vec::new(),
            elements: Vec::new(),
        })
    }

}

struct Player;
struct TurnPhase;
