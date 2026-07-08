mod ai;
mod command;
mod core;
mod error;
mod game;
mod gui;
mod procedures;
mod utils;

pub use command::COMMAND_KEYWORDS;
pub use error::Error;
pub use gui::run as run_gui;

use postcard::{from_bytes, to_allocvec};

use std::{fs::File, io::{Read, Write}};
use std::sync::{Arc, Mutex};

use command::{Command, HELP_TEXT, InspectTarget};
use game::{Game, VictoryReport};

/// A game shared between the terminal thread and the GUI's main thread: both
/// read and mutate the same session, so a command from either side is
/// immediately visible to the other. `None` until a game is started (`new`)
/// or resumed (`load`), from either side.
pub type SharedGame = Arc<Mutex<Option<Game>>>;

pub fn new_shared_game() -> SharedGame {
    Arc::new(Mutex::new(None))
}

/// Run one terminal command against a shared game: locks, runs it exactly
/// like a single-threaded caller would via `run`, and stores the result back
/// (a new/loaded game replaces whatever was there). Output still goes to
/// stdout, matching `run`'s existing per-command printing.
pub fn run_shared(input: &str, shared: &SharedGame) -> Result<(), Error> {
    let mut guard = shared.lock().unwrap();
    if let Some(game) = run(input, guard.as_mut())? {
        *guard = Some(game);
    }
    Ok(())
}

pub fn run(input: &str, mut current_game: Option<&mut Game>) -> Result<Option<Game>, Error> {
    let mut new_game = match Command::parse(input)? {
        Command::New { scenario_path } => Some(new_game(scenario_path)?),
        Command::Load { save_path } => Some(load_game(save_path)?),
        Command::Save { save_path } => {
            save_game(save_path, require_game(current_game.as_deref_mut())?)?;
            None
        }
        Command::Inspect(target) => {
            inspect(require_game(current_game.as_deref_mut())?, &target)?;
            None
        }
        Command::Units { detail } => {
            let game = require_game(current_game.as_deref_mut())?;
            if detail {
                game.list_units_detail();
            } else {
                game.list_units();
            }
            None
        }
        Command::Move { from, to, unit_index } => {
            require_game(current_game.as_deref_mut())?.move_unit(from.0, from.1, to.0, to.1, unit_index)?;
            None
        }
        Command::Attack { from, to } => {
            let report = require_game(current_game.as_deref_mut())?.attack(from, to, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::AirSupport { from, to, air_unit } => {
            let report = require_game(current_game.as_deref_mut())?
                .air_support(&air_unit, from, to, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::Interdict { target, unit } => {
            require_game(current_game.as_deref_mut())?.interdict(&unit, target)?;
            None
        }
        Command::Interdiction => {
            println!("{}", require_game(current_game.as_deref_mut())?.interdiction_summary());
            None
        }
        Command::Simulate { from, to, runs } => {
            let report = require_game(current_game.as_deref_mut())?.simulate(from, to, runs, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::EndTurn => {
            let game = require_game(current_game.as_deref_mut())?;
            let victory = game.end_turn();
            for line in report_turn_transition(game, &victory) {
                println!("{line}");
            }
            for line in play_pending_ai_turns(game, victory) {
                println!("{line}");
            }
            None
        }
        Command::Status => {
            println!("{}", require_game(current_game.as_deref_mut())?.status());
            None
        }
        Command::Victory => {
            println!("{}", require_game(current_game.as_deref_mut())?.victory_conditions_summary());
            None
        }
        Command::Reinforcements => {
            println!("{}", require_game(current_game.as_deref_mut())?.reinforcement_schedule_summary());
            None
        }
        Command::Events => {
            println!("{}", require_game(current_game.as_deref_mut())?.event_schedule_summary());
            None
        }
        Command::Supply => {
            println!("{}", require_game(current_game)?.supply_status_summary());
            None
        }
        Command::Help => {
            println!("{HELP_TEXT}");
            None
        }
    };

    if let Some(game) = new_game.as_mut() {
        // A freshly built (or loaded) game can already have an AI-controlled
        // faction on turn — e.g. a scenario where the AI plays the first
        // faction listed — so it gets the same auto-play treatment `end_turn`
        // gives it mid-game, before anything else about the new game prints.
        for line in play_pending_ai_turns(game, None) {
            println!("{line}");
        }
        // A freshly built game may already have fired turn-1 events (see
        // Game::parse_scen_from_toml); print those now, since nothing else
        // does. (Usually already drained by play_pending_ai_turns above if
        // it ran; harmless — and necessary — when it didn't.)
        for message in game.take_event_messages() {
            println!("{message}");
        }
    }

    Ok(new_game)
}

fn require_game(game: Option<&mut Game>) -> Result<&mut Game, Error> {
    game.ok_or_else(|| Error::new("No game loaded."))
}

/// The aftermath of one `end_turn` call, one line per message: status, any
/// event messages, and the final score if the scenario just ended. Shared
/// between the human's `end_turn` and each AI-controlled turn `end_turn`
/// auto-plays afterward, so the two don't drift apart — and between the
/// terminal (which prints each line) and the GUI (`gui.rs`, which logs them
/// instead).
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

fn inspect(game: &Game, target: &InspectTarget) -> Result<(), Error> {
    let location = match target {
        InspectTarget::Hex { x, y } => {
            if !game.is_visible_to(game.current_faction(), *x, *y) {
                println!("Unknown — outside detection range.");
                return Ok(());
            }
            game.state.map.get_location(*x, *y).ok_or_else(|| Error::new("Hex not in range."))?
        }
        InspectTarget::Offmap(name) => game.state.map.get_offmap_location(name)
            .ok_or_else(|| Error::new("Location not found."))?,
    };

    println!("{}", location);
    for unit in game.units_at_location(location) {
        println!("{}", unit);
        println!("TOE: {}", unit.toe);
        println!("Morale: {}  Experience: {} (unit average)", unit.average_morale(), unit.average_experience());
        for element in &unit.elements {
            println!(
                "  {}: {} ready, {} damaged — morale {}, experience {}",
                element.name, element.ready, element.damaged, element.morale, element.experience,
            );
        }
    }
    Ok(())
}

/// `pub(crate)`, not just a `run()` internal: `gui.rs`'s main menu calls these
/// directly for New/Load, since it builds/loads a game outside the shared
/// mutex (there's nothing to lock yet) rather than routing through `run`.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn human_vs_ai_scenario() -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "lib test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"

[[players]]
faction_name = "Axis"
faction_tag = "AX"
controller = "Ai"

[[toe]]
name = "test_toe"
size = "Division"
mp = 16
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = {{ x = 1, y = 1 }}

[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = {{ x = 5, y = 5 }}
"#)
    }

    #[test]
    fn end_turn_auto_plays_ai_factions_until_control_reaches_a_human() {
        let mut game = Game::build(human_vs_ai_scenario()).unwrap();
        assert_eq!(game.status(), "lib test scenario — turn 1, 1941-06-22. Soviet Union to move.");

        // The human ends turn 1; the Axis AI then plays its own turn 1
        // automatically, and control lands back on the human for turn 2 —
        // all from a single "end_turn" command.
        run("end_turn", Some(&mut game)).unwrap();

        assert_eq!(game.status(), "lib test scenario — turn 2, 1941-06-29. Soviet Union to move.");
    }

    #[test]
    fn new_auto_plays_an_ai_faction_that_is_first_on_turn() {
        // Axis (AI) listed before Soviet Union (human): the fresh game opens
        // on the AI's own turn, which "new" must play out immediately rather
        // than leaving the human staring at an AI-controlled turn 1.
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        let scenario = format!(r#"
name = "lib test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[players]]
faction_name = "Axis"
faction_tag = "AX"
controller = "Ai"

[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"

[[toe]]
name = "test_toe"
size = "Division"
mp = 16
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = {{ x = 5, y = 5 }}

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = {{ x = 1, y = 1 }}
"#);
        let path = std::env::temp_dir()
            .join(format!("cse_test_scenario_{}.scen", std::process::id()));
        std::fs::write(&path, scenario).unwrap();

        let result = run(&format!("new {}", path.display()), None);
        let _ = std::fs::remove_file(&path);
        let game = result.unwrap().unwrap();

        assert_eq!(game.status(), "lib test scenario — turn 1, 1941-06-22. Soviet Union to move.");
    }
}
