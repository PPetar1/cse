mod command;
mod core;
mod error;
mod game;
mod procedures;
mod utils;
mod view;
mod visualiser;

pub use command::COMMAND_KEYWORDS;
pub use error::Error;
pub use view::{cleanup_view, run_view_subprocess};

use postcard::{from_bytes, to_allocvec};

use std::{fs::File, io::{Read, Write}};

use command::{Command, HELP_TEXT, InspectTarget};
use game::Game;
use view::{refresh_view, view};

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
        Command::Simulate { from, to, runs } => {
            let report = require_game(current_game.as_deref_mut())?.simulate(from, to, runs, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::EndTurn => {
            let game = require_game(current_game.as_deref_mut())?;
            let victory = game.end_turn();
            println!("{}", game.status());
            for message in game.take_event_messages() {
                println!("{message}");
            }
            if let Some(report) = victory {
                println!("{report}");
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
        Command::View => {
            view(require_game(current_game.as_deref_mut())?)?;
            None
        }
        Command::Help => {
            println!("{HELP_TEXT}");
            None
        }
    };

    // A freshly built game may already have fired turn-1 events (see
    // Game::parse_scen_from_toml); print those now, since nothing else does.
    if let Some(game) = new_game.as_mut() {
        for message in game.take_event_messages() {
            println!("{message}");
        }
    }

    // Any open view window watches the snapshot file, so keep it mirroring the
    // game after every successful command (no-op until `view` has created it).
    if let Some(game) = new_game.as_ref().or(current_game.as_deref()) {
        refresh_view(game);
    }

    Ok(new_game)
}

fn require_game(game: Option<&mut Game>) -> Result<&mut Game, Error> {
    game.ok_or_else(|| Error::new("No game loaded."))
}

fn inspect(game: &Game, target: &InspectTarget) -> Result<(), Error> {
    let location = match target {
        InspectTarget::Hex { x, y } => game.state.map.get_location(*x, *y)
            .ok_or_else(|| Error::new("Hex not in range."))?,
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

fn new_game(scenario_path: &str) -> Result<Game, Error> {
    let mut scen_file = File::open(scenario_path)?;
    let mut contents = String::new();
    scen_file.read_to_string(&mut contents)?;
    Game::build(contents)
}

fn load_game(save_path: &str) -> Result<Game, Error> {
    let mut file = File::open(save_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let game: Game = from_bytes(&contents)?;
    Ok(game)
}

fn save_game(save_path: &str, game: &Game) -> Result<(), Error> {
    let mut file = File::create(save_path)?;

    let bin: Vec<u8> = to_allocvec(game)?;

    file.write_all(&bin)?;

    Ok(())
}
