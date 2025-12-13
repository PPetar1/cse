pub fn run(command: &str) -> Result<(), Error> {
    let mut slices = command.split_whitespace();
    
    let command = slices.next();
    let arguments: Vec<&str> = slices.collect();

    match command {
        Some("new") => {
            new_game(arguments)
        }
        _ => Err(Error{ error_message: "Unknown command".to_string() }),
    }
}

fn new_game(arguments: Vec<&str>) -> Result<(), Error> {
    Ok(())
}

pub struct Error {
    pub error_message: String,
}
