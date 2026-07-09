//! The main menu (no game loaded yet) and the mid-game Save/Load/New popups,
//! plus the deferred application of whatever they confirm — see
//! `MenuAction`'s doc comment for why confirmation and execution are split.

use eframe::egui;

use crate::Error;
use crate::game::Game;
use crate::play_pending_ai_turns;

use super::GuiApp;
use super::file_picker::{FilePicker, PickerField};

/// Text fields and any error from the last New/Load attempt — shared between
/// the main menu and the mid-game Save/Load/New dialogs (`DialogKind`),
/// since both need the same three path fields.
pub(super) struct MainMenuState {
    pub(super) scenario_path: String,
    pub(super) load_path: String,
    pub(super) save_path: String,
    pub(super) error: Option<String>,
}

impl Default for MainMenuState {
    fn default() -> Self {
        MainMenuState {
            scenario_path: "scenarios/basic_scenario.scen".to_string(),
            load_path: String::new(),
            save_path: String::new(),
            error: None,
        }
    }
}

/// A mid-game Save/Load/New popup, awaiting a path and a Confirm/Cancel.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum DialogKind {
    Save,
    Load,
    New,
}

/// A Save/Load/New/Quit action confirmed from a `DialogKind` popup, deferred
/// until after `ui()` releases its lock on `shared` — `MenuAction::Load`/
/// `New` end up calling `GuiApp::adopt_game`, which locks `shared` itself,
/// and `std::sync::Mutex` isn't reentrant, so running these while
/// `render_playing` still holds the guard would deadlock.
pub(super) enum MenuAction {
    Save(String),
    Load(String),
    New(String),
    Quit,
}

impl GuiApp {
    pub(super) fn render_main_menu(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("CSE — Combat Simulation Engine");
                ui.add_space(30.0);

                ui.label("Scenario file:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.menu.scenario_path).desired_width(280.0));
                    if ui.button("Browse…").clicked() {
                        self.file_picker = Some(FilePicker::open(PickerField::Scenario));
                    }
                });
                if ui.button("New Game").clicked() {
                    self.pending_menu_action = Some(MenuAction::New(self.menu.scenario_path.clone()));
                }

                ui.add_space(20.0);

                ui.label("Save file:");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.menu.load_path).desired_width(280.0));
                    if ui.button("Browse…").clicked() {
                        self.file_picker = Some(FilePicker::open(PickerField::Load));
                    }
                });
                if ui.button("Load Game").clicked() {
                    self.pending_menu_action = Some(MenuAction::Load(self.menu.load_path.clone()));
                }

                ui.add_space(20.0);
                if ui.button("Quit").clicked() {
                    self.pending_menu_action = Some(MenuAction::Quit);
                }

                if let Some(error) = &self.menu.error {
                    ui.add_space(10.0);
                    ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
                }
            });
        });
    }

    /// The Save/Load/New popup armed by the top panel's buttons: a path
    /// field (reusing the same `MainMenuState` fields the main menu's own
    /// New/Load use) plus Confirm/Cancel. Confirm only arms
    /// `pending_menu_action` — see its doc comment for why it can't act
    /// immediately here.
    pub(super) fn render_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(kind) = self.dialog else { return };
        let title = match kind {
            DialogKind::Save => "Save game",
            DialogKind::Load => "Load game",
            DialogKind::New => "New game",
        };
        egui::Window::new(title).collapsible(false).resizable(false).show(ui.ctx(), |ui| {
            let path = match kind {
                DialogKind::Save => &mut self.menu.save_path,
                DialogKind::Load => &mut self.menu.load_path,
                DialogKind::New => &mut self.menu.scenario_path,
            };
            ui.horizontal(|ui| {
                ui.text_edit_singleline(path);
                if ui.button("Browse…").clicked() {
                    self.file_picker = Some(FilePicker::open(match kind {
                        DialogKind::Save => PickerField::Save,
                        DialogKind::Load => PickerField::Load,
                        DialogKind::New => PickerField::Scenario,
                    }));
                }
            });
            ui.horizontal(|ui| {
                if ui.button("Confirm").clicked() {
                    self.pending_menu_action = Some(match kind {
                        DialogKind::Save => MenuAction::Save(self.menu.save_path.clone()),
                        DialogKind::Load => MenuAction::Load(self.menu.load_path.clone()),
                        DialogKind::New => MenuAction::New(self.menu.scenario_path.clone()),
                    });
                    self.dialog = None;
                }
                if ui.button("Cancel").clicked() {
                    self.dialog = None;
                }
            });
        });
    }

    /// Adopt the result of a New/Load attempt: on success, auto-play any
    /// AI-controlled faction already on turn and drain turn-1 event
    /// messages — the same treatment `lib.rs`'s `run()` gives a freshly
    /// built or loaded game — then publish it to `shared`. On failure,
    /// leave the shared game untouched and show the error on the menu.
    fn adopt_game(&mut self, result: Result<Game, Error>) {
        match result {
            Ok(mut game) => {
                self.log.clear();
                self.log.extend(play_pending_ai_turns(&mut game, None));
                self.log.extend(game.take_event_messages());
                self.selected_hex = None;
                self.pending_order = None;
                self.menu.error = None;
                *self.shared.lock().unwrap() = Some(game);
            }
            Err(error) => self.menu.error = Some(error.error_message),
        }
    }

    /// Apply a confirmed Save/Load/New/Quit action. Always runs after
    /// `ui()` has dropped its lock on `shared` (see `MenuAction`'s doc
    /// comment), so it's free to lock again here.
    pub(super) fn apply_pending_menu_action(&mut self) {
        let Some(action) = self.pending_menu_action.take() else { return };
        match action {
            MenuAction::Quit => std::process::exit(0),
            MenuAction::Save(path) => {
                let guard = self.shared.lock().unwrap();
                let result = match guard.as_ref() {
                    Some(game) => crate::save_game(&path, game),
                    None => Err(Error::new("No game loaded.")),
                };
                drop(guard);
                match result {
                    Ok(()) => self.log.push(format!("Saved to {path}.")),
                    Err(error) => self.log.push(error.error_message),
                }
            }
            MenuAction::Load(path) => self.adopt_game(crate::load_game(&path)),
            MenuAction::New(path) => self.adopt_game(crate::new_game(&path)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::test_support::*;
    use super::*;

    #[test]
    fn adopt_game_populates_shared_state_on_success() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen");
        let mut app = app();

        let result = crate::new_game(path);
        app.adopt_game(result);

        assert!(app.menu.error.is_none());
        assert!(app.shared.lock().unwrap().is_some());
    }

    #[test]
    fn adopt_game_records_an_error_on_failure_and_leaves_shared_state_untouched() {
        let mut app = app();

        let result = crate::new_game("does/not/exist.scen");
        app.adopt_game(result);

        assert!(app.menu.error.is_some());
        assert!(app.shared.lock().unwrap().is_none());
    }

    #[test]
    fn apply_pending_menu_action_new_populates_shared_state() {
        let mut app = app();
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen");

        app.pending_menu_action = Some(MenuAction::New(path.to_string()));
        app.apply_pending_menu_action();

        assert!(app.menu.error.is_none());
        assert!(app.shared.lock().unwrap().is_some());
    }

    #[test]
    fn apply_pending_menu_action_saves_then_loads_through_a_file() {
        let mut app = app();
        let scenario_path = concat!(env!("CARGO_MANIFEST_DIR"), "/scenarios/basic_scenario.scen");
        app.pending_menu_action = Some(MenuAction::New(scenario_path.to_string()));
        app.apply_pending_menu_action();

        let save_path = std::env::temp_dir().join(format!("cse_gui_test_save_{}.sav", std::process::id()));
        let save_path_str = save_path.display().to_string();

        app.pending_menu_action = Some(MenuAction::Save(save_path_str.clone()));
        app.apply_pending_menu_action();
        assert!(save_path.exists());
        assert!(app.log.last().unwrap().contains("Saved"));

        // Clear the shared game, then confirm Load brings it back from the file.
        *app.shared.lock().unwrap() = None;
        app.pending_menu_action = Some(MenuAction::Load(save_path_str));
        app.apply_pending_menu_action();

        let _ = std::fs::remove_file(&save_path);
        assert!(app.shared.lock().unwrap().is_some());
    }
}
