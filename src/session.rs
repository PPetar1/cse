//! The application layer every frontend (terminal, GUI) shares: persistence,
//! the shared game handle, and the turn-flow orchestration neither frontend
//! owns exclusively.

use postcard::{from_bytes, to_allocvec};

use std::{fs::File, io::{Read, Write}};
use std::sync::{Arc, Mutex};

use crate::Error;
use crate::ai;
use crate::game::{Game, VictoryReport};

/// A game shared between the terminal thread and the GUI's main thread: both
/// read and mutate the same session, so a command from either side is
/// immediately visible to the other. `None` until a game is started (`new`)
/// or resumed (`load`), from either side.
pub type SharedGame = Arc<Mutex<Option<Game>>>;

pub fn new_shared_game() -> SharedGame {
    Arc::new(Mutex::new(None))
}

/// `pub(crate)`, not just a `terminal::run()` internal: `gui::menu`'s main
/// menu calls these directly for New/Load, since it builds/loads a game
/// outside the shared mutex (there's nothing to lock yet).
pub(crate) fn new_game(scenario_path: &str) -> Result<Game, Error> {
    let mut scen_file = File::open(scenario_path)?;
    let mut contents = String::new();
    scen_file.read_to_string(&mut contents)?;
    Game::build(contents)
}

pub(crate) fn load_game(save_path: &str) -> Result<Game, Error> {
    let mut file = File::open(save_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let game: Game = from_bytes(&contents)?;
    Ok(game)
}

pub(crate) fn save_game(save_path: &str, game: &Game) -> Result<(), Error> {
    let mut file = File::create(save_path)?;

    let bin: Vec<u8> = to_allocvec(game)?;

    file.write_all(&bin)?;

    Ok(())
}

/// The aftermath of one `end_turn` call, one line per message: status, any
/// event messages, and the final score if the scenario just ended. Shared
/// between the human's `end_turn` and each AI-controlled turn `end_turn`
/// auto-plays afterward, so the two don't drift apart — and between the
/// terminal (which prints each line) and the GUI (which logs them instead).
pub(crate) fn report_turn_transition(game: &mut Game, victory: &Option<VictoryReport>) -> Vec<String> {
    let mut lines = vec![game.status()];
    lines.extend(game.take_event_messages());
    if let Some(report) = victory {
        lines.push(report.to_string());
    }
    lines
}

/// Play out AI-controlled factions one after another — starting from
/// whatever the last-known victory result was — until control reaches a
/// human player or the game ends, returning one line per message (see
/// `report_turn_transition`). Called both after a human's `end_turn` and
/// right after a game is built/loaded, since either can leave an
/// AI-controlled faction on turn. (Known limitation, not guarded against: an
/// all-AI scenario with no `last_turn` would spin here forever — every
/// scenario so far assumes at least one human seat.)
pub(crate) fn play_pending_ai_turns(game: &mut Game, mut victory: Option<VictoryReport>) -> Vec<String> {
    let mut lines = Vec::new();
    while victory.is_none() && game.current_player_is_ai() {
        lines.push(ai::take_turn(game, &mut rand::rng()).to_string());
        victory = game.end_turn();
        lines.extend(report_turn_transition(game, &victory));
    }
    lines
}

/// The post-new/load ritual: auto-play any AI-controlled faction already on
/// turn (e.g. a scenario where the AI plays the first faction listed), then
/// drain whatever turn-1 event messages that didn't already surface. Called
/// from both frontends right after a game is built or loaded, so a fresh
/// game never leaves a human staring at an AI-controlled turn 1 or misses
/// its opening event messages.
pub(crate) fn activate_game(game: &mut Game) -> Vec<String> {
    let mut lines = play_pending_ai_turns(game, None);
    lines.extend(game.take_event_messages());
    lines
}
