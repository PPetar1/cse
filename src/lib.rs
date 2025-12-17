mod game;
mod core;
mod procedures;
mod utils;

use serde::{Serialize, Deserialize};
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
            if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "No scenario file provided. Unable to start a new game.".to_string(),
                });
            }
            
            Ok(Some(new_game(arguments)?))
        }
        Some("load") => {
            if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "No file provided. Unable to load a new game.".to_string(),
                });
            }

            Ok(Some(load_game(arguments)?))
        }
        Some("save") => {
             if arguments.len() == 0 { 
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
                if arguments.len() > 0 {
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
                Err(Error::from_str("No game loaded."))
            }
        }
        Some("move") => {
            if let Some(mut game) = current_game {
                if arguments.len() < 5 {
                    Err(Error::from_str("Need source, destination and index of the unit to move it."))
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
                Err(Error::from_str("No game loaded."))
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

fn save_game(arguments: Vec<&str>, game: &Game) -> Result<(), Error> {
    let save_file_path = arguments[0];
    let mut file = File::create(save_file_path)?;
    
    let bin: alloc::vec::Vec<u8> = to_allocvec(game)?;
    
    file.write_all(&bin)?;

    Ok(())
}

pub struct Error {
    pub error_message: String,
}

impl Error {
    pub fn from_str(error_message: &str) -> Error {
        Error {
            error_message: error_message.to_string(),
        }
    }
}

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
