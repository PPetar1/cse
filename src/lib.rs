mod game;
mod state;
mod procedures;
mod utils;

use game::Game;

pub fn run(command: &str, current_game: Option<Game>) -> Result<Game, Error> {
    let mut slices = command.split_whitespace();
    
    let command = slices.next();
    let arguments: Vec<&str> = slices.collect();

    match command {
        Some("new") => {
            if arguments.len() == 0 { 
                return Err(Error { 
                    error_message: "Please specify a scenario file to start a new game.".to_string(),
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
                    error_message: "Please specify a savefile to load.".to_string(),
                    game: current_game,
                });
            }

            // Droping current_game early to prevent potential memory issues
            std::mem::drop(current_game);
            
            load_game(arguments)
        }
        Some("inspect") => {
            if arguments.len() < 2 {
                Err(Error { 
                    error_message: "Please specify the coordinates of the hex you want to inspect".to_string(),
                    game: current_game,    
                })
            }
            else {
                if let (Ok(x), Ok(y)) = (arguments[0].parse(), arguments[1].parse()) {
                    if let Some(game) = current_game {
                        if let Some(location) = game.state.inspect_location(x, y) {
                            println!("{}", location);
                            Ok(game)
                        }
                        else {
                            Err (Error { 
                                error_message: "Hex not in range".to_string(),
                                game: Some(game),
                            })
                        } 
                    }
                    else {
                        Err (Error { 
                            error_message: "No game loaded".to_string(),
                            game: current_game,
                        })
                    }
                }
                else {
                    Err(Error { 
                        error_message: "Please provide valid coordinates for the hex".to_string(),
                        game: current_game,
                    })
                }
            }
        }
        _ => Err(Error{ 
            error_message: "Unknown command".to_string(),
            game: current_game,
        }),
    }
}

fn new_game(arguments: Vec<&str>) -> Result<Game, Error> {
   Game::build()
}

fn load_game(arguments: Vec<&str>) -> Result<Game, Error> { 
    Game::load()
}

pub struct Error {
    pub error_message: String,
    pub game: Option<Game>,
}
