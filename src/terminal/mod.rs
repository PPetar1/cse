//! The terminal frontend: one-shot command dispatch (`run`/`run_shared`) and
//! the rustyline read-eval-print loop (`run_loop`, this module's equivalent
//! of `gui::run`), including the two-step `reassign_leader` prompt. Mirrors
//! `gui/` as a directory.

mod command;

use std::cell::RefCell;

use rustyline::completion::{Completer, FilenameCompleter, Pair};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use rustyline::history::DefaultHistory;
use rustyline::validate::Validator;
use rustyline::{CompletionType, Config, Context, Editor, Helper};

use crate::Error;
use crate::game::Game;
use crate::session::{self, SharedGame};

use command::{Command, COMMAND_KEYWORDS, HELP_TEXT};

/// Open the terminal read-eval-print loop for the rest of the process's
/// life: rustyline tab completion (`COMMAND_KEYWORDS` + a filename
/// completer, arrow-key history) over `run_shared`, plus the two-step
/// `reassign_leader` prompt (which needs a follow-up line `run`/`run_shared`
/// can't accommodate). Ctrl-C/Ctrl-D/`exit` end the whole process — this
/// thread can't hand control back to `main`'s GUI thread, so it calls
/// `std::process::exit` directly.
pub fn run_loop(shared: SharedGame) {
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

        if let Err(error) = run_shared(&input, &shared) {
            println!("{}", error.error_message);
        }
    }
}

/// The `reassign_leader` two-step prompt: validate `unit_name`, arm the
/// helper's completer with its faction's leader names, read the leader name
/// on a second line, then apply it.
fn run_reassign_leader(unit_name: &str, editor: &mut Editor<CommandHelper, DefaultHistory>, shared: &SharedGame) {
    if unit_name.is_empty() {
        println!("Missing unit name. Usage: reassign_leader <unit name>");
        return;
    }
    let (faction, leader_names) = match unit_leader_context(unit_name, shared) {
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

    match reassign_leader_shared(unit_name, leader_name, shared) {
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
            let candidates = COMMAND_KEYWORDS
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

/// Run one terminal command against a shared game: locks, runs it exactly
/// like a single-threaded caller would via `run`, and stores the result back
/// (a new/loaded game replaces whatever was there). Output still goes to
/// stdout, matching `run`'s existing per-command printing.
fn run_shared(input: &str, shared: &SharedGame) -> Result<(), Error> {
    let mut guard = shared.lock().unwrap();
    if let Some(game) = run(input, guard.as_mut())? {
        *guard = Some(game);
    }
    Ok(())
}

fn run(input: &str, mut current_game: Option<&mut Game>) -> Result<Option<Game>, Error> {
    let mut new_game = match Command::parse(input)? {
        Command::New { scenario_path } => Some(session::new_game(scenario_path)?),
        Command::Load { save_path } => Some(session::load_game(save_path)?),
        Command::Save { save_path } => {
            session::save_game(save_path, require_game(current_game.as_deref_mut())?)?;
            None
        }
        Command::Inspect(target) => {
            println!("{}", require_game(current_game.as_deref_mut())?.inspect_summary(&target)?);
            None
        }
        Command::Units { detail } => {
            println!("{}", require_game(current_game.as_deref_mut())?.units_summary(detail));
            None
        }
        Command::Move { from, to, unit_index } => {
            require_game(current_game.as_deref_mut())?.move_unit(from.0, from.1, to.0, to.1, unit_index)?;
            None
        }
        Command::Attack { from, to } => {
            let report = require_game(current_game.as_deref_mut())?.attack(from, to, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::AirSupport { from, to, air_unit } => {
            let report = require_game(current_game.as_deref_mut())?
                .air_support(&air_unit, from, to, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::Interdict { target, unit } => {
            require_game(current_game.as_deref_mut())?.interdict(&unit, target)?;
            None
        }
        Command::Interdiction => {
            println!("{}", require_game(current_game.as_deref_mut())?.interdiction_summary());
            None
        }
        Command::Simulate { from, to, runs } => {
            let report = require_game(current_game.as_deref_mut())?.simulate(from, to, runs, &mut rand::rng())?;
            println!("{report}");
            None
        }
        Command::EndTurn => {
            let game = require_game(current_game.as_deref_mut())?;
            let victory = game.end_turn();
            for line in session::report_turn_transition(game, &victory) {
                println!("{line}");
            }
            for line in session::play_pending_ai_turns(game, victory) {
                println!("{line}");
            }
            None
        }
        Command::Status => {
            println!("{}", require_game(current_game.as_deref_mut())?.status());
            None
        }
        Command::Victory => {
            println!("{}", require_game(current_game.as_deref_mut())?.victory_conditions_summary());
            None
        }
        Command::Reinforcements => {
            println!("{}", require_game(current_game.as_deref_mut())?.reinforcement_schedule_summary());
            None
        }
        Command::Events => {
            println!("{}", require_game(current_game.as_deref_mut())?.event_schedule_summary());
            None
        }
        Command::Supply => {
            println!("{}", require_game(current_game.as_deref_mut())?.supply_status_summary());
            None
        }
        Command::Leaders { faction } => {
            println!("{}", require_game(current_game.as_deref_mut())?.leaders_summary(&faction)?);
            None
        }
        Command::Leader { name } => {
            println!("{}", require_game(current_game)?.leader_detail(&name)?);
            None
        }
        Command::Help => {
            println!("{HELP_TEXT}");
            None
        }
    };

    if let Some(game) = new_game.as_mut() {
        // A freshly built (or loaded) game gets the same auto-play-then-drain
        // treatment as `gui::menu::adopt_game`, before anything else about
        // the new game prints — see `session::activate_game`.
        for line in session::activate_game(game) {
            println!("{line}");
        }
    }

    Ok(new_game)
}

fn require_game(game: Option<&mut Game>) -> Result<&mut Game, Error> {
    game.ok_or_else(|| Error::new("No game loaded."))
}

/// `unit`'s faction and that faction's leader names, sorted — what
/// `run_reassign_leader`'s prompt needs before it can validate the unit and
/// build the leader-name completer, ahead of asking for a name.
fn unit_leader_context(unit: &str, shared: &SharedGame) -> Result<(String, Vec<String>), Error> {
    let guard = shared.lock().unwrap();
    let game = guard.as_ref().ok_or_else(|| Error::new("No game loaded."))?;
    let faction = game.unit(unit)
        .ok_or_else(|| Error::new(format!("No such unit '{unit}'.")))?
        .faction.clone();
    let names = game.leaders_of_faction(&faction).iter().map(|leader| leader.name.clone()).collect();
    Ok((faction, names))
}

/// Assign `leader` to `unit` against the shared game — the second half of
/// `run_reassign_leader`'s prompt, once the leader's name has been read.
fn reassign_leader_shared(unit: &str, leader: &str, shared: &SharedGame) -> Result<(), Error> {
    let mut guard = shared.lock().unwrap();
    require_game(guard.as_mut())?.reassign_leader(leader, unit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn human_vs_ai_scenario() -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "terminal test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[factions]]
faction_name = "Soviet Union"
faction_tag = "SU"

[[factions]]
faction_name = "Axis"
faction_tag = "AX"

[[players]]
name = "Axis"
faction = "AX"
controller = "Ai"

[[toe]]
name = "test_toe"
size = "Division"
mp = 16
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = {{ x = 1, y = 1 }}

[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = {{ x = 5, y = 5 }}
"#)
    }

    #[test]
    fn end_turn_auto_plays_ai_factions_until_control_reaches_a_human() {
        let mut game = Game::build(human_vs_ai_scenario()).unwrap();
        assert_eq!(
            game.status(),
            "terminal test scenario — turn 1, 1941-06-22. Soviet Union (Soviet Union) to move.",
        );

        // The human ends turn 1; the Axis AI then plays its own turn 1
        // automatically, and control lands back on the human for turn 2 —
        // all from a single "end_turn" command.
        run("end_turn", Some(&mut game)).unwrap();

        assert_eq!(
            game.status(),
            "terminal test scenario — turn 2, 1941-06-29. Soviet Union (Soviet Union) to move.",
        );
    }

    #[test]
    fn new_auto_plays_an_ai_faction_that_is_first_on_turn() {
        // Axis (AI) listed before Soviet Union (human): the fresh game opens
        // on the AI's own turn, which "new" must play out immediately rather
        // than leaving the human staring at an AI-controlled turn 1.
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        let scenario = format!(r#"
name = "terminal test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[factions]]
faction_name = "Axis"
faction_tag = "AX"

[[factions]]
faction_name = "Soviet Union"
faction_tag = "SU"

[[players]]
name = "Axis"
faction = "AX"
controller = "Ai"

[[toe]]
name = "test_toe"
size = "Division"
mp = 16
start_date = "1941-01-01"
end_date = "1941-08-01"
[[toe.elements]]
name = "test_element"
amount = 10

[[elements]]
name = "test_element"
class = "Inf"
cv = 4.0
vulnerability = 100
[[elements.devices]]
name = "test_rifles"
accuracy = 20
range = 100
rate_of_fire = 1
soft_attack = 100
hard_attack = 3

[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = {{ x = 5, y = 5 }}

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = {{ x = 1, y = 1 }}
"#);
        let path = std::env::temp_dir()
            .join(format!("cse_test_scenario_{}.scen", std::process::id()));
        std::fs::write(&path, scenario).unwrap();

        let result = run(&format!("new {}", path.display()), None);
        let _ = std::fs::remove_file(&path);
        let game = result.unwrap().unwrap();

        assert_eq!(
            game.status(),
            "terminal test scenario — turn 1, 1941-06-22. Soviet Union (Soviet Union) to move.",
        );
    }
}
