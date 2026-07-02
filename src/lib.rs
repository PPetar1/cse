mod game;
mod core;
mod procedures;
mod utils;
mod visualiser;

use postcard::{from_bytes, to_allocvec};
extern crate alloc;

use std::{fs::File, io::{Read, Write}};

use crate::core::unit::UnitLocation;
use game::Game;

pub fn run(input: &str, current_game: Option<&mut Game>) -> Result<Option<Game>, Error> {
    match Command::parse(input)? {
        Command::New { scenario_path } => Ok(Some(new_game(scenario_path)?)),
        Command::Load { save_path } => Ok(Some(load_game(save_path)?)),
        Command::Save { save_path } => {
            save_game(save_path, require_game(current_game)?)?;
            Ok(None)
        }
        Command::Inspect(target) => {
            inspect(require_game(current_game)?, &target)?;
            Ok(None)
        }
        Command::Units { detail } => {
            let game = require_game(current_game)?;
            if detail {
                game.list_units_detail();
            } else {
                game.list_units();
            }
            Ok(None)
        }
        Command::Move { from, to, unit_index } => {
            require_game(current_game)?.move_unit(from.0, from.1, to.0, to.1, unit_index)?;
            Ok(None)
        }
        Command::Attack { from, to } => {
            let report = require_game(current_game)?.attack(from, to, &mut rand::rng())?;
            println!("{report}");
            Ok(None)
        }
        Command::Simulate { from, to, runs } => {
            let report = require_game(current_game)?.simulate(from, to, runs, &mut rand::rng())?;
            println!("{report}");
            Ok(None)
        }
        Command::View => {
            view(require_game(current_game)?)?;
            Ok(None)
        }
        Command::Help => {
            println!("{HELP_TEXT}");
            Ok(None)
        }
    }
}

/// Command keywords, used for terminal tab completion. Keep in sync with
/// `Command::parse` and HELP_TEXT.
pub const COMMAND_KEYWORDS: &[&str] = &[
    "new", "load", "save", "inspect", "units", "move", "attack", "simulate", "view", "help", "exit",
];

const HELP_TEXT: &str = "\
Commands:
  new <path.scen>                     start a new game from a scenario file
  load <path.sav>                     load a saved game
  save <path.sav>                     save the current game
  inspect <x> <y> | inspect <name>    show a hex or offmap location and its units
  units [detail]                      list all units
  move <x1> <y1> <x2> <y2> <index>    move the unit with that index (per inspect) between hexes
  attack <x1> <y1> <x2> <y2>          units at hex 1 attack units at hex 2
  simulate <x1> <y1> <x2> <y2> <n>    fight that attack n times without applying it, show statistics
  view                                open the map window
  help                                show this help
  exit                                quit";

/// A fully parsed player command. Parsing is separated from execution so
/// argument handling can be tested without a running game.
#[derive(Debug, PartialEq)]
enum Command<'a> {
    New { scenario_path: &'a str },
    Load { save_path: &'a str },
    Save { save_path: &'a str },
    Inspect(InspectTarget),
    Units { detail: bool },
    Move { from: (u32, u32), to: (u32, u32), unit_index: usize },
    Attack { from: (u32, u32), to: (u32, u32) },
    Simulate { from: (u32, u32), to: (u32, u32), runs: u32 },
    View,
    Help,
}

#[derive(Debug, PartialEq)]
enum InspectTarget {
    Hex { x: u32, y: u32 },
    Offmap(String),
}

impl<'a> Command<'a> {
    fn parse(input: &'a str) -> Result<Command<'a>, Error> {
        let mut words = input.split_whitespace();
        let keyword = words.next().unwrap_or("");
        let args: Vec<&str> = words.collect();

        match keyword {
            "new" => {
                let path = args.first().ok_or_else(|| Error::new("No scenario file provided. Unable to start a new game."))?;
                Ok(Command::New { scenario_path: path })
            }
            "load" => {
                let path = args.first().ok_or_else(|| Error::new("No file provided. Unable to load a game."))?;
                Ok(Command::Load { save_path: path })
            }
            "save" => {
                let path = args.first().ok_or_else(|| Error::new("Path to use for the save not specified."))?;
                Ok(Command::Save { save_path: path })
            }
            "inspect" => {
                if args.is_empty() {
                    return Err(Error::new("Missing hex coordinate or offmap location name arguments for inspect."));
                }
                if let [x, y] = args[..]
                    && let (Ok(x), Ok(y)) = (x.parse(), y.parse()) {
                        return Ok(Command::Inspect(InspectTarget::Hex { x, y }));
                    }
                Ok(Command::Inspect(InspectTarget::Offmap(args.join(" "))))
            }
            "units" => Ok(Command::Units { detail: args.first() == Some(&"detail") }),
            "move" => {
                if args.len() < 5 {
                    return Err(Error::new("Need source, destination and index of the unit to move it."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end), Ok(unit_index))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse(), args[4].parse())
                {
                    Ok(Command::Move { from: (x_start, y_start), to: (x_end, y_end), unit_index })
                } else {
                    Err(Error::new("Unable to parse arguments for move order."))
                }
            }
            "attack" => {
                if args.len() < 4 {
                    return Err(Error::new("Need attacking hex and target hex coordinates for attack."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse())
                {
                    Ok(Command::Attack { from: (x_start, y_start), to: (x_end, y_end) })
                } else {
                    Err(Error::new("Unable to parse arguments for attack order."))
                }
            }
            "simulate" => {
                if args.len() < 5 {
                    return Err(Error::new("Need attacking hex, target hex and number of battles for simulate."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end), Ok(runs))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse(), args[4].parse())
                {
                    Ok(Command::Simulate { from: (x_start, y_start), to: (x_end, y_end), runs })
                } else {
                    Err(Error::new("Unable to parse arguments for simulate."))
                }
            }
            "view" => Ok(Command::View),
            "help" => Ok(Command::Help),
            _ => Err(Error::new("Unknown command. Type 'help' for a list of commands.")),
        }
    }
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
        for element in &unit.elements {
            println!("  {}: {} ready, {} damaged", element.name, element.ready, element.damaged);
        }
    }
    Ok(())
}

fn view(game: &Game) -> Result<(), Error> {
    let hexes = game.state.map.all_locations()
        .into_iter()
        .map(|((x, y), terrain)| visualiser::HexDisplay { x, y, terrain })
        .collect();

    let units = game.state.units.values()
        .filter_map(|unit| match &unit.location {
            UnitLocation::OnMap(coords) => Some(visualiser::UnitDisplay {
                x: coords.x,
                y: coords.y,
                name: unit.name.clone(),
                faction: unit.faction.clone(),
            }),
            UnitLocation::Offmap(_) => None,
        })
        .collect();

    spawn_view_subprocess(visualiser::MapSnapshot { hexes, units })
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

    let bin: alloc::vec::Vec<u8> = to_allocvec(game)?;

    file.write_all(&bin)?;

    Ok(())
}

// winit event loops can only be created once per process, so each `view` runs
// the visualiser in a fresh subprocess (this binary re-invoked with --view).
// The snapshot crosses over via a temp file the subprocess deletes after reading.
fn spawn_view_subprocess(snapshot: visualiser::MapSnapshot) -> Result<(), Error> {
    let bin: alloc::vec::Vec<u8> = to_allocvec(&snapshot)?;

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let snapshot_path = std::env::temp_dir().join(format!("cse_view_{}_{}.snapshot", std::process::id(), unique));
    let mut file = File::create(&snapshot_path)?;
    file.write_all(&bin)?;

    let current_exe = std::env::current_exe()?;
    let mut child = std::process::Command::new(current_exe)
        .arg("--view")
        .arg(&snapshot_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;

    // Reap the child when the window closes so it doesn't linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });

    Ok(())
}

pub fn run_view_subprocess(snapshot_path: &str) -> Result<(), Error> {
    let mut file = File::open(snapshot_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;
    let _ = std::fs::remove_file(snapshot_path);

    let snapshot: visualiser::MapSnapshot = from_bytes(&contents)?;
    visualiser::launch(snapshot);
    Ok(())
}

#[derive(Debug)]
pub struct Error {
    pub error_message: String,
}

impl Error {
    pub fn new(error_message: &str) -> Error {
        Error {
            error_message: error_message.to_string(),
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.error_message)
    }
}

impl std::error::Error for Error {}

impl From<toml::de::Error> for Error {
    fn from(error: toml::de::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}

impl From<postcard::Error> for Error {
    fn from(error: postcard::Error) -> Error {
        Error {
            error_message: error.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_move_command() {
        let command = Command::parse("move 3 4 2 2 0").unwrap();

        assert_eq!(command, Command::Move { from: (3, 4), to: (2, 2), unit_index: 0 });
    }

    #[test]
    fn rejects_move_with_missing_arguments() {
        let error = Command::parse("move 3 4").unwrap_err();

        assert!(error.error_message.contains("source, destination and index"));
    }

    #[test]
    fn rejects_move_with_unparseable_arguments() {
        let error = Command::parse("move 3 4 a b 0").unwrap_err();

        assert!(error.error_message.contains("Unable to parse"));
    }

    #[test]
    fn parses_an_attack_command() {
        let command = Command::parse("attack 3 4 3 3").unwrap();

        assert_eq!(command, Command::Attack { from: (3, 4), to: (3, 3) });
    }

    #[test]
    fn rejects_attack_with_missing_arguments() {
        let error = Command::parse("attack 3 4").unwrap_err();

        assert!(error.error_message.contains("attacking hex and target hex"));
    }

    #[test]
    fn parses_a_simulate_command() {
        let command = Command::parse("simulate 3 4 3 3 100").unwrap();

        assert_eq!(command, Command::Simulate { from: (3, 4), to: (3, 3), runs: 100 });
    }

    #[test]
    fn parses_inspect_with_hex_coordinates() {
        let command = Command::parse("inspect 2 3").unwrap();

        assert_eq!(command, Command::Inspect(InspectTarget::Hex { x: 2, y: 3 }));
    }

    #[test]
    fn parses_inspect_with_multi_word_offmap_name() {
        let command = Command::parse("inspect GE Reserve").unwrap();

        assert_eq!(command, Command::Inspect(InspectTarget::Offmap("GE Reserve".to_string())));
    }

    #[test]
    fn parses_units_with_and_without_detail() {
        assert_eq!(Command::parse("units").unwrap(), Command::Units { detail: false });
        assert_eq!(Command::parse("units detail").unwrap(), Command::Units { detail: true });
    }

    #[test]
    fn rejects_new_without_a_scenario_path() {
        let error = Command::parse("new").unwrap_err();

        assert!(error.error_message.contains("No scenario file provided"));
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = Command::parse("teleport 1 2").unwrap_err();

        assert!(error.error_message.contains("Unknown command"));
    }

    #[test]
    fn parses_a_help_command() {
        assert_eq!(Command::parse("help").unwrap(), Command::Help);
    }

    #[test]
    fn every_command_keyword_parses_or_is_exit() {
        // Guards COMMAND_KEYWORDS (used by tab completion) against drifting
        // from the parser. "exit" is handled by the main loop, not the parser.
        for keyword in COMMAND_KEYWORDS {
            if *keyword == "exit" {
                continue;
            }
            // Parsing with dummy arguments must never hit "Unknown command".
            let input = format!("{keyword} 1 1 2 2 0");
            if let Err(error) = Command::parse(&input) {
                assert!(
                    !error.error_message.contains("Unknown command"),
                    "keyword '{keyword}' is not known to the parser",
                );
            }
        }
    }
}
