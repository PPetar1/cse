mod game;
mod core;
mod procedures;
mod utils;
mod visualiser;

use postcard::{from_bytes, to_allocvec};
extern crate alloc;

use std::{fs::File, io::{Read, Write}};

use game::Game;

pub fn run(command: &str, current_game: Option<&mut Game>) -> Result<Option<Game>, Error> {
    let mut slices = command.split_whitespace();
    
    let command = slices.next();
    let arguments: Vec<&str> = slices.collect();

    match command {
        Some("new") => {
            if arguments.is_empty() { 
                return Err(Error { 
                    error_message: "No scenario file provided. Unable to start a new game.".to_string(),
                });
            }
            
            Ok(Some(new_game(arguments)?))
        }
        Some("load") => {
            if arguments.is_empty() { 
                return Err(Error { 
                    error_message: "No file provided. Unable to load a new game.".to_string(),
                });
            }

            Ok(Some(load_game(arguments)?))
        }
        Some("save") => {
             if arguments.is_empty() { 
                return Err(Error { 
                    error_message: "Path to use for the save not specified.".to_string(),
                });
            }

            if let Some(game) = current_game {
                save_game(arguments, game)?;
                Ok(None)
            }
            else {
                Err(Error {
                    error_message: "No game active to save.".to_string(),
                })
            }
        }
        Some("inspect") => {
            if let Some(game) = current_game {
                if arguments.len() == 1 {
                    if let Some(location) = game.state.map.get_offmap_location(arguments[0]) {
                        println!("{}", location);
                        for unit in game.units_at_location(location) {
                            println!("{}", unit);
                        }
                        Ok(None)
                    }
                    else {
                        Err (Error { 
                            error_message: "Offmap location not found.".to_string(),
                        })
                    } 
                }
                else if arguments.len() < 2 {
                    Err(Error { 
                        error_message: "Missing hex coordinate or offmap hex name arguments for inspect.".to_string(),
                    })
                }
                else {
                    if let (Ok(x), Ok(y)) = (arguments[0].parse(), arguments[1].parse()) {
                            if let Some(location) = game.state.map.get_location(x, y) {
                                println!("{}", location);
                                for unit in game.units_at_location(location) {
                                    println!("{}", unit);
                                }
                                Ok(None)
                            }
                            else {
                                Err (Error { 
                                    error_message: "Hex not in range.".to_string(),
                                })
                            } 
                        }
                    else {
                        if let Some(location) = game.state.map.get_offmap_location(&arguments.join(" ")) {
                            println!("{}", location);
                            for unit in game.units_at_location(location) {
                                println!("{}", unit);
                            }
                            Ok(None)
                        }
                        else {
                            Err (Error { 
                                error_message: "Location not found.".to_string(),
                            })
                        } 
                    }
                }
            }
            else {
                Err (Error { 
                    error_message: "No game loaded.".to_string(),
                })
            }
        }
        Some("units") => {
            if let Some(game) = current_game {
                if !arguments.is_empty() {
                    if arguments[0] == "detail" {
                        game.list_units_detail();
                    }
                }
                else {
                    game.list_units();
                }
                Ok(None)
            }
            else {
                Err(Error::new("No game loaded."))
            }
        }
        Some("move") => {
            if let Some(game) = current_game {
                if arguments.len() < 5 {
                    Err(Error::new("Need source, destination and index of the unit to move it."))
                }
                else {
                    if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end), Ok(unit_i)) 
                        = (arguments[0].parse(), arguments[1].parse(), arguments[2].parse(), arguments[3].parse(), arguments[4].parse())
                    {
                        game.move_unit(x_start, y_start, x_end, y_end,  unit_i)?;
                        Ok(None)
                    }
                    else {
                        Err(Error {
                            error_message: "Unable to parse arguments for move order.".to_string(),
                        })
                    }
                }
            }
            else {
                Err(Error::new("No game loaded."))
            }
        }
        Some("view") => {
            if let Some(game) = current_game {
                let hexes = game.state.map.all_locations()
                    .into_iter()
                    .map(|((x, y), terrain)| visualiser::HexDisplay { x, y, terrain })
                    .collect();

                let units = game.state.units.values()
                    .filter_map(|unit| match &unit.location {
                        core::unit::UnitLocation::OnMap(coords) => Some(visualiser::UnitDisplay {
                            x: coords.x,
                            y: coords.y,
                            name: unit.name.clone(),
                            faction: unit.faction.clone(),
                        }),
                        core::unit::UnitLocation::Offmap(_) => None,
                    })
                    .collect();

                spawn_view_subprocess(visualiser::MapSnapshot { hexes, units })?;
                Ok(None)
            } else {
                Err(Error::new("No game loaded."))
            }
        }
        _ => Err(Error{
            error_message: "Unknown command.".to_string(),
        }),
    }
}

fn new_game(arguments: Vec<&str>) -> Result<Game, Error> {
    let scen_file_path = arguments[0];
    match File::open(scen_file_path) {
        Ok(mut scen_file) => { 
            let mut contents = String::new();
            scen_file.read_to_string(&mut contents)?;
            Game::build(contents)
        }
        Err(error) => Err(Error {
            error_message: error.to_string(),
        }),
    }
    
}

fn load_game(arguments: Vec<&str>) -> Result<Game, Error> { 
    let load_file_path = arguments[0];
    let mut file = File::open(load_file_path)?;
    let mut contents = Vec::new();
    file.read_to_end(&mut contents)?;

    let game: Game = from_bytes(&contents)?;
    Ok(game)
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

fn save_game(arguments: Vec<&str>, game: &Game) -> Result<(), Error> {
    let save_file_path = arguments[0];
    let mut file = File::create(save_file_path)?;
    
    let bin: alloc::vec::Vec<u8> = to_allocvec(game)?;
    
    file.write_all(&bin)?;

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
