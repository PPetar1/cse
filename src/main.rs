use std::env;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};

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

    let config = Config::builder()
        .completion_type(CompletionType::List)
        .build();
    let mut editor: Editor<CommandHelper, DefaultHistory> =
        Editor::with_config(config).expect("Failed to initialise the terminal line editor");
    editor.set_helper(Some(CommandHelper { file_completer: FilenameCompleter::new() }));

    let mut current_game = None;

    loop {
        let input = match editor.readline("> ") {
            Ok(line) => line,
            // Ctrl-C / Ctrl-D quit, same as `exit`.
            Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => break,
            Err(error) => {
                eprintln!("{error}");
                break;
            }
        };

        if input.trim().is_empty() {
            continue;
        }
        let _ = editor.add_history_entry(&input);

        if input.trim() == "exit" {
            break;
        }

        match cse::run(&input, current_game.as_mut()) {
            Ok(Some(game)) => current_game = Some(game),
            Ok(None) => (),
            Err(error) => println!("{}", error.error_message),
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
