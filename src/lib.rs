mod ai;
mod command;
mod core;
mod error;
mod game;
mod gui;
mod procedures;
mod session;

pub use command::COMMAND_KEYWORDS;
pub use error::Error;
pub use gui::run as run_gui;
pub use session::{SharedGame, new_shared_game};

use command::{Command, HELP_TEXT};
use game::Game;

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
        Command::New { scenario_path } => Some(session::new_game(scenario_path)?),
        Command::Load { save_path } => Some(session::load_game(save_path)?),
        Command::Save { save_path } => {
            session::save_game(save_path, require_game(current_game.as_deref_mut())?)?;
            None
        }
        Command::Inspect(target) => {
            println!("{}", require_game(current_game.as_deref_mut())?.inspect_summary(&target)?);
            None
        }
        Command::Units { detail } => {
            println!("{}", require_game(current_game.as_deref_mut())?.units_summary(detail));
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
            for line in session::report_turn_transition(game, &victory) {
                println!("{line}");
            }
            for line in session::play_pending_ai_turns(game, victory) {
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
            println!("{}", require_game(current_game.as_deref_mut())?.supply_status_summary());
            None
        }
        Command::Leaders { faction } => {
            println!("{}", require_game(current_game.as_deref_mut())?.leaders_summary(&faction)?);
            None
        }
        Command::Leader { name } => {
            println!("{}", require_game(current_game)?.leader_detail(&name)?);
            None
        }
        Command::Help => {
            println!("{HELP_TEXT}");
            None
        }
    };

    if let Some(game) = new_game.as_mut() {
        // A freshly built (or loaded) game gets the same auto-play-then-drain
        // treatment as `gui::menu::adopt_game`, before anything else about
        // the new game prints — see `session::activate_game`.
        for line in session::activate_game(game) {
            println!("{line}");
        }
    }

    Ok(new_game)
}

fn require_game(game: Option<&mut Game>) -> Result<&mut Game, Error> {
    game.ok_or_else(|| Error::new("No game loaded."))
}

/// `unit`'s faction and that faction's leader names, sorted — what
/// `main.rs`'s `reassign_leader` prompt needs before it can validate the
/// unit and build the leader-name completer, ahead of asking for a name.
pub fn unit_leader_context(unit: &str, shared: &SharedGame) -> Result<(String, Vec<String>), Error> {
    let guard = shared.lock().unwrap();
    let game = guard.as_ref().ok_or_else(|| Error::new("No game loaded."))?;
    let faction = game.state.units.get(unit)
        .ok_or_else(|| Error::new(format!("No such unit '{unit}'.")))?
        .faction.clone();
    let names = game.leaders_of_faction(&faction).iter().map(|leader| leader.name.clone()).collect();
    Ok((faction, names))
}

/// Assign `leader` to `unit` against the shared game — the second half of
/// `main.rs`'s `reassign_leader` prompt, once the leader's name has been
/// read.
pub fn reassign_leader_shared(unit: &str, leader: &str, shared: &SharedGame) -> Result<(), Error> {
    let mut guard = shared.lock().unwrap();
    require_game(guard.as_mut())?.reassign_leader(leader, unit)
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
