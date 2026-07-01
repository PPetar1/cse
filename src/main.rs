use std::{env, io};

fn main() {
    // Visualiser subprocess entry: winit event loops can only be created once
    // per process, so each `view` command spawns a fresh process with this flag.
    let args: Vec<String> = env::args().collect();
    if args.len() >= 3 && args[1] == "--view" {
        if let Err(error) = cse::run_view_subprocess(&args[2]) {
            eprintln!("{}", error.error_message);
        }
        return;
    }

    let mut current_game = None;

    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read from stdin");

        if input.trim() == "exit" { break; }

        match cse::run(&input, current_game.as_mut()) {
            Ok(Some(game)) => current_game = Some(game),
            Ok(None) => (),
            Err(error) => println!("{}", error.error_message),
        }
    }
}
