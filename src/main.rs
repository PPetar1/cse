use std::cell::RefCell;

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
    editor.set_helper(Some(CommandHelper {
        file_completer: FilenameCompleter::new(),
        leader_names: RefCell::new(Vec::new()),
    }));

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

        // A two-step order rather than a single line: the command names the
        // unit, then a second prompt (with leader-name completion) asks
        // which leader to assign — see `command::HELP_TEXT`'s entry for why
        // this bypasses the single-line command parser.
        let trimmed = input.trim();
        if trimmed.split_whitespace().next() == Some("reassign_leader") {
            let unit_name = trimmed["reassign_leader".len()..].trim();
            run_reassign_leader(unit_name, &mut editor, &shared);
            continue;
        }

        if let Err(error) = cse::run_shared(&input, &shared) {
            println!("{}", error.error_message);
        }
    }
}

/// The `reassign_leader` two-step prompt: validate `unit_name`, arm the
/// helper's completer with its faction's leader names, read the leader name
/// on a second line, then apply it.
fn run_reassign_leader(unit_name: &str, editor: &mut Editor<CommandHelper, DefaultHistory>, shared: &cse::SharedGame) {
    if unit_name.is_empty() {
        println!("Missing unit name. Usage: reassign_leader <unit name>");
        return;
    }
    let (faction, leader_names) = match cse::unit_leader_context(unit_name, shared) {
        Ok(context) => context,
        Err(error) => {
            println!("{}", error.error_message);
            return;
        }
    };
    if leader_names.is_empty() {
        println!("Faction '{faction}' has no leaders to assign.");
        return;
    }

    if let Some(helper) = editor.helper_mut() {
        helper.leader_names.replace(leader_names);
    }
    let prompt_result = editor.readline("Leader name> ");
    if let Some(helper) = editor.helper_mut() {
        helper.leader_names.replace(Vec::new());
    }

    let leader_name = match prompt_result {
        Ok(line) => line,
        // Same policy as the main loop's readline: this thread can't hand
        // off gracefully, so an interrupt here ends the whole process too.
        Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => std::process::exit(0),
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(0);
        }
    };
    let leader_name = leader_name.trim();
    if leader_name.is_empty() {
        println!("Cancelled.");
        return;
    }
    let _ = editor.add_history_entry(leader_name);

    match cse::reassign_leader_shared(unit_name, leader_name, shared) {
        Ok(()) => println!("'{leader_name}' now leads '{unit_name}'."),
        Err(error) => println!("{}", error.error_message),
    }
}

/// Tab completion: command keywords for the first word, file paths for the
/// commands that take one, leader names for the `reassign_leader` prompt.
/// History (arrow keys) comes with the editor.
struct CommandHelper {
    file_completer: FilenameCompleter,
    /// Non-empty only while `run_reassign_leader`'s second prompt is live —
    /// completion falls back to keyword/file matching otherwise.
    leader_names: RefCell<Vec<String>>,
}

impl Completer for CommandHelper {
    type Candidate = Pair;

    fn complete(
        &self,
        line: &str,
        pos: usize,
        ctx: &Context<'_>,
    ) -> rustyline::Result<(usize, Vec<Pair>)> {
        let leader_names = self.leader_names.borrow();
        if !leader_names.is_empty() {
            let word = &line[..pos];
            let candidates = leader_names.iter()
                .filter(|name| name.starts_with(word))
                .map(|name| Pair { display: name.clone(), replacement: name.clone() })
                .collect();
            return Ok((0, candidates));
        }
        drop(leader_names);

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
