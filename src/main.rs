use std::{io, thread::current};

fn main() {
    let mut current_game = None;
    
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read from stdin");
        
        if input.trim() == "exit" { break; }

        match cse::run(&input, current_game.take()) {
            Ok(game) => current_game = Some(game),
            Err(error) => {
                current_game = error.game;
                println!("{}", error.error_message)
            },
        }
    }
}

