mod game;
mod core;
mod procedures;
mod utils;

use std::{fs::File, io::Read};

use game::Game;

pub fn run<'a>(command: &str, current_game: Option<Game<'a>>) -> Result<Game<'a>, Error<'a>> {
    let mut slices = command.split_whitespace();
    
    let command = slices.next();
    let arguments: Vec<&str> = slices.collect();

    match command {
        Some("new") => {
            if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "No scenario file provided. Unable to start a new game.".to_string(),
                    game: current_game,
                });
            }

            // Droping current_game early to prevent potential memory issues 
            std::mem::drop(current_game);
            
            new_game(arguments)
        }
        Some("load") => {
            if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "No file provided. Unable to load a new game.".to_string(),
                    game: current_game,
                });
            }

            // Droping current_game early to prevent potential memory issues
            std::mem::drop(current_game);
            
            load_game(arguments)
        }
        Some("save") => {
             if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "Path to use for the save not specified.".to_string(),
                    game: current_game,
                });
            }

            if let Some(game) = current_game {
                match save_game(arguments) {
                    Ok(_) => Ok(game),
                    Err(mut error) => {
                        error.game = Some(game);
                        Err(error)
                    }
                }
            }
            else {
                Err(Error {
                    error_message: "No game active to save.".to_string(),
                    game: None,
                })
            }
        }
        Some("inspect") => {
            if let Some(game) = current_game {
                if arguments.len() == 1 {
                    if let Some(location) = game.state.map.inspect_offmap_location(arguments[0]) {
                        println!("{}", location);
                        Ok(game)
                    }
                    else {
                        Err (Error { 
                            error_message: "Offmap location not found.".to_string(),
                            game: Some(game),
                        })
                    } 
                }
                else if arguments.len() < 2 {
                    Err(Error { 
                        error_message: "Missing hex coordinate or offmap hex name arguments for inspect.".to_string(),
                        game: Some(game),    
                    })
                }
                else {
                    if let (Ok(x), Ok(y)) = (arguments[0].parse(), arguments[1].parse()) {
                            if let Some(location) = game.state.map.inspect_location(x, y) {
                                println!("{}", location);
                                Ok(game)
                            }
                            else {
                                Err (Error { 
                                    error_message: "Hex not in range.".to_string(),
                                    game: Some(game),
                                })
                            } 
                        }
                    else {
                        Err(Error { 
                            error_message: "Invalid hex coordinates.".to_string(),
                            game: Some(game),
                        })
                    }
                }
            }
            else {
                Err (Error { 
                    error_message: "No game loaded.".to_string(),
                    game: current_game,
                })
            }
        }
        _ => Err(Error{ 
            error_message: "Unknown command.".to_string(),
            game: current_game,
        }),
    }
}

fn new_game<'a>(arguments: Vec<&str>) -> Result<Game<'a>, Error<'a>> {
    let scen_file_path = arguments[0];
    match File::open(scen_file_path) {
        Ok(mut scen_file) => { 
            let mut contents = String::new();
            scen_file.read_to_string(&mut contents)?;
            Game::build(contents)
        }
        Err(error) => Err(Error {
            error_message: error.to_string(),
            game: None,
        }),
    }
    
}

fn load_game<'a>(arguments: Vec<&str>) -> Result<Game<'a>, Error<'a>> { 
    Err(Error { error_message: "Not implemented yet.".to_string(), game: None })
}

fn save_game<'a>(arguments: Vec<&str>) -> Result<(), Error<'a>> {
    Err(Error { error_message: "Not implemented yet.".to_string(), game: None })
}

pub struct Error<'a> {
    pub error_message: String,
    pub game: Option<Game<'a>>,
}

impl<'a> Error<'a> {
    pub fn from_str(error_message: &str) -> Error<'a> {
        Error {
            error_message: error_message.to_string(),
            game: None,
        }
    }
}

impl<'a> From<toml::de::Error> for Error<'a> {
    fn from(error: toml::de::Error) -> Error<'a> {
        Error {
            error_message: error.to_string(),
            game: None,
        }
    } 
}

impl<'a> From<std::io::Error> for Error<'a> {
    fn from(error: std::io::Error) -> Error<'a> {
        Error {
            error_message: error.to_string(),
            game: None,
        }
    } 
}
