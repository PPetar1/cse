//! The terminal command language: the `Command` enum, its parser, and the
//! keyword/help listings that must stay in sync with it. Parsing is
//! separated from execution (`run` in lib.rs) so argument handling can be
//! tested without a running game.

use crate::Error;

/// Command keywords, used for terminal tab completion. Keep in sync with
/// `Command::parse` and HELP_TEXT.
pub const COMMAND_KEYWORDS: &[&str] = &[
    "new", "load", "save", "inspect", "units", "move", "attack", "simulate", "end_turn", "status",
    "victory", "reinforcements", "events", "supply", "view", "help", "exit",
];

pub(crate) const HELP_TEXT: &str = "\
Commands:
  new <path.scen>                     start a new game from a scenario file
  load <path.sav>                     load a saved game
  save <path.sav>                     save the current game
  inspect <x> <y> | inspect <name>    show a hex or offmap location and its units
  units [detail]                      list all units
  move <x1> <y1> <x2> <y2> <index>    move the unit with that index (per inspect) between hexes
  attack <x1> <y1> <x2> <y2>          units at hex 1 attack units at hex 2
  simulate <x1> <y1> <x2> <y2> <n>    fight that attack n times without applying it, show statistics
  end_turn                            pass control to the next player, advancing turn and date
  status                              show scenario, turn, date and who is to move
  victory                             show this scenario's victory conditions and who currently holds each objective hex
  reinforcements                      show scheduled reinforcements/withdrawals and whether each has arrived
  events                              show scheduled scenario events and whether each has fired
  supply                              show whether each on-map unit is supplied or cut off
  view                                open the map window
  help                                show this help
  exit                                quit";

/// A fully parsed player command.
#[derive(Debug, PartialEq)]
pub(crate) enum Command<'a> {
    New { scenario_path: &'a str },
    Load { save_path: &'a str },
    Save { save_path: &'a str },
    Inspect(InspectTarget),
    Units { detail: bool },
    Move { from: (u32, u32), to: (u32, u32), unit_index: usize },
    Attack { from: (u32, u32), to: (u32, u32) },
    Simulate { from: (u32, u32), to: (u32, u32), runs: u32 },
    EndTurn,
    Status,
    Victory,
    Reinforcements,
    Events,
    Supply,
    View,
    Help,
}

#[derive(Debug, PartialEq)]
pub(crate) enum InspectTarget {
    Hex { x: u32, y: u32 },
    Offmap(String),
}

impl<'a> Command<'a> {
    pub(crate) fn parse(input: &'a str) -> Result<Command<'a>, Error> {
        let mut words = input.split_whitespace();
        let keyword = words.next().unwrap_or("");
        let args: Vec<&str> = words.collect();

        match keyword {
            "new" => {
                let path = args.first().ok_or_else(|| Error::new("No scenario file provided. Unable to start a new game."))?;
                Ok(Command::New { scenario_path: path })
            }
            "load" => {
                let path = args.first().ok_or_else(|| Error::new("No file provided. Unable to load a game."))?;
                Ok(Command::Load { save_path: path })
            }
            "save" => {
                let path = args.first().ok_or_else(|| Error::new("Path to use for the save not specified."))?;
                Ok(Command::Save { save_path: path })
            }
            "inspect" => {
                if args.is_empty() {
                    return Err(Error::new("Missing hex coordinate or offmap location name arguments for inspect."));
                }
                if let [x, y] = args[..]
                    && let (Ok(x), Ok(y)) = (x.parse(), y.parse()) {
                        return Ok(Command::Inspect(InspectTarget::Hex { x, y }));
                    }
                Ok(Command::Inspect(InspectTarget::Offmap(args.join(" "))))
            }
            "units" => Ok(Command::Units { detail: args.first() == Some(&"detail") }),
            "move" => {
                if args.len() < 5 {
                    return Err(Error::new("Need source, destination and index of the unit to move it."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end), Ok(unit_index))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse(), args[4].parse())
                {
                    Ok(Command::Move { from: (x_start, y_start), to: (x_end, y_end), unit_index })
                } else {
                    Err(Error::new("Unable to parse arguments for move order."))
                }
            }
            "attack" => {
                if args.len() < 4 {
                    return Err(Error::new("Need attacking hex and target hex coordinates for attack."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse())
                {
                    Ok(Command::Attack { from: (x_start, y_start), to: (x_end, y_end) })
                } else {
                    Err(Error::new("Unable to parse arguments for attack order."))
                }
            }
            "simulate" => {
                if args.len() < 5 {
                    return Err(Error::new("Need attacking hex, target hex and number of battles for simulate."));
                }
                if let (Ok(x_start), Ok(y_start), Ok(x_end), Ok(y_end), Ok(runs))
                    = (args[0].parse(), args[1].parse(), args[2].parse(), args[3].parse(), args[4].parse())
                {
                    Ok(Command::Simulate { from: (x_start, y_start), to: (x_end, y_end), runs })
                } else {
                    Err(Error::new("Unable to parse arguments for simulate."))
                }
            }
            "end_turn" => Ok(Command::EndTurn),
            "status" => Ok(Command::Status),
            "victory" => Ok(Command::Victory),
            "reinforcements" => Ok(Command::Reinforcements),
            "events" => Ok(Command::Events),
            "supply" => Ok(Command::Supply),
            "view" => Ok(Command::View),
            "help" => Ok(Command::Help),
            _ => Err(Error::new("Unknown command. Type 'help' for a list of commands.")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_move_command() {
        let command = Command::parse("move 3 4 2 2 0").unwrap();

        assert_eq!(command, Command::Move { from: (3, 4), to: (2, 2), unit_index: 0 });
    }

    #[test]
    fn rejects_move_with_missing_arguments() {
        let error = Command::parse("move 3 4").unwrap_err();

        assert!(error.error_message.contains("source, destination and index"));
    }

    #[test]
    fn rejects_move_with_unparseable_arguments() {
        let error = Command::parse("move 3 4 a b 0").unwrap_err();

        assert!(error.error_message.contains("Unable to parse"));
    }

    #[test]
    fn parses_an_attack_command() {
        let command = Command::parse("attack 3 4 3 3").unwrap();

        assert_eq!(command, Command::Attack { from: (3, 4), to: (3, 3) });
    }

    #[test]
    fn rejects_attack_with_missing_arguments() {
        let error = Command::parse("attack 3 4").unwrap_err();

        assert!(error.error_message.contains("attacking hex and target hex"));
    }

    #[test]
    fn parses_a_simulate_command() {
        let command = Command::parse("simulate 3 4 3 3 100").unwrap();

        assert_eq!(command, Command::Simulate { from: (3, 4), to: (3, 3), runs: 100 });
    }

    #[test]
    fn parses_inspect_with_hex_coordinates() {
        let command = Command::parse("inspect 2 3").unwrap();

        assert_eq!(command, Command::Inspect(InspectTarget::Hex { x: 2, y: 3 }));
    }

    #[test]
    fn parses_inspect_with_multi_word_offmap_name() {
        let command = Command::parse("inspect GE Reserve").unwrap();

        assert_eq!(command, Command::Inspect(InspectTarget::Offmap("GE Reserve".to_string())));
    }

    #[test]
    fn parses_units_with_and_without_detail() {
        assert_eq!(Command::parse("units").unwrap(), Command::Units { detail: false });
        assert_eq!(Command::parse("units detail").unwrap(), Command::Units { detail: true });
    }

    #[test]
    fn rejects_new_without_a_scenario_path() {
        let error = Command::parse("new").unwrap_err();

        assert!(error.error_message.contains("No scenario file provided"));
    }

    #[test]
    fn rejects_unknown_commands() {
        let error = Command::parse("teleport 1 2").unwrap_err();

        assert!(error.error_message.contains("Unknown command"));
    }

    #[test]
    fn parses_a_help_command() {
        assert_eq!(Command::parse("help").unwrap(), Command::Help);
    }

    #[test]
    fn parses_a_victory_command() {
        assert_eq!(Command::parse("victory").unwrap(), Command::Victory);
    }

    #[test]
    fn parses_a_reinforcements_command() {
        assert_eq!(Command::parse("reinforcements").unwrap(), Command::Reinforcements);
    }

    #[test]
    fn parses_an_events_command() {
        assert_eq!(Command::parse("events").unwrap(), Command::Events);
    }

    #[test]
    fn parses_a_supply_command() {
        assert_eq!(Command::parse("supply").unwrap(), Command::Supply);
    }

    #[test]
    fn every_command_keyword_parses_or_is_exit() {
        // Guards COMMAND_KEYWORDS (used by tab completion) against drifting
        // from the parser. "exit" is handled by the main loop, not the parser.
        for keyword in COMMAND_KEYWORDS {
            if *keyword == "exit" {
                continue;
            }
            // Parsing with dummy arguments must never hit "Unknown command".
            let input = format!("{keyword} 1 1 2 2 0");
            if let Err(error) = Command::parse(&input) {
                assert!(
                    !error.error_message.contains("Unknown command"),
                    "keyword '{keyword}' is not known to the parser",
                );
            }
        }
    }
}
