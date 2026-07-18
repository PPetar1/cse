fn main() {
    let shared = cse::new_shared_game();

    // The terminal loop runs on its own thread; the GUI (`run_gui`) owns the
    // main thread, since winit event loops need to run there. Both act on
    // the same shared game, so a command from either side is immediately
    // visible to the other — see `SharedGame` in session.rs.
    let terminal_shared = shared.clone();
    std::thread::spawn(move || cse::run_terminal(terminal_shared));

    if let Err(error) = cse::run_gui(shared) {
        eprintln!("{}", error.error_message);
    }
}
