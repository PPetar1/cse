use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};

fn main() {
    let shared = cse::new_shared_game();

    // The terminal loop runs on its own thread; the GUI (`run_gui`) owns the
    // main thread, since winit event loops need to run there. Both act on
    // the same shared game, so a command from either side is immediately
    // visible to the other — see `SharedGame` in lib.rs.
    let terminal_shared = shared.clone();
    std::thread::spawn(move || run_terminal(terminal_shared));

    if let Err(error) = cse::run_gui(shared) {
        eprintln!("{}", error.error_message);
    }
}

fn run_terminal(shared: cse::SharedGame) {
    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut editor: Editor<CommandHelper, DefaultHistory> =
        Editor::with_config(config).expect("Failed to initialise the terminal line editor");
    editor.set_helper(Some(CommandHelper { file_completer: FilenameCompleter::new() }));

    loop {
        let input = match editor.readline("> ") {
            Ok(line) => line,
            // Ctrl-C / Ctrl-D quit the whole process, same as `exit` — this
            // thread can't gracefully hand off to the GUI on the main thread,
            // so it ends the session outright rather than just itself.
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => std::process::exit(0),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(0);
            }
        };

        if input.trim().is_empty() {
            continue;
        }
        let _ = editor.add_history_entry(&input);

        if input.trim() == "exit" {
            std::process::exit(0);
        }

        if let Err(error) = cse::run_shared(&input, &shared) {
            println!("{}", error.error_message);
        }
    }
}

/// Tab completion: command keywords for the first word, file paths for the
/// commands that take one. History (arrow keys) comes with the editor.
struct CommandHelper {
    file_completer: FilenameCompleter,
}

impl Completer for CommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let before_cursor = &line[..pos];

        // Still typing the first word: complete command keywords.
        if !before_cursor.trim_start().contains(' ') {
            let word = before_cursor.trim_start();
            let candidates = cse::COMMAND_KEYWORDS
                .iter()
                .filter(|keyword| keyword.starts_with(word))
                .map(|keyword| Pair {
                    display: keyword.to_string(),
                    replacement: format!("{keyword} "),
                })
                .collect();
            return Ok((pos - word.len(), candidates));
        }

        // Path argument of a file command: complete filenames.
        let command = before_cursor.split_whitespace().next().unwrap_or("");
        if matches!(command, "new" | "load" | "save") {
            return self.file_completer.complete(line, pos, ctx);
        }

        Ok((pos, Vec::new()))
    }
}

// Only completion is customised; hints, highlighting and validation stay at
// the trait defaults.
impl Hinter for CommandHelper {
    type Hint = String;
}
impl Highlighter for CommandHelper {}
impl Validator for CommandHelper {}
impl Helper for CommandHelper {}
