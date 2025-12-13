use std::io;

fn main() {
    loop {
        let mut input = String::new();
        io::stdin().read_line(&mut input).expect("Failed to read from stdin");
        
        if input.trim() == "exit" { break; }
        
        match cse::run(&input) {
            Ok(()) => (),
            Err(error) => println!("{}", error.error_message),
        }
    }
}

