use crate::core::State;
use crate::Error;

pub struct Game {
    pub state: State,
    players: Vec<Player>,
    turn: u32,
    phase: TurnPhase,
}

impl Game {
    pub fn build() -> Result<Game, Error> {
        Ok(Game {
            state: State::build(),
            players: vec![Player, Player],
            turn: 1,
            phase: TurnPhase,
        })
    }

    pub fn load() -> Result<Game, Error> {
        Ok(Game {
            state: State::build(),
            players: vec![Player, Player],
            turn: 1,
            phase: TurnPhase,
        })
    }

}

struct Player;
struct TurnPhase;
