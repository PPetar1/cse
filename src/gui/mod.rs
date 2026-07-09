//! The real interface (Phase 6): an eframe/egui window that shows a main
//! menu (New/Load/Quit) when no game is active, and once one is, renders the
//! map, lets the player click a hex to inspect its units, issue `move`/
//! `attack`/`air_support`/`interdict` orders, end the turn, and (from the
//! same header) save/load/start a new game or quit mid-session. Every
//! scenario/save-file path field has a "Browse…" button (`FilePicker`) that
//! lists a directory instead of requiring the path to be typed.
//!
//! Module layout mirrors `game/`: `GuiApp`'s fields live here, submodules
//! add `impl GuiApp` blocks per concern — `menu` (main menu, Save/Load/New
//! dialogs), `map_view` (hex-to-screen mapping and map rendering),
//! `inspector` (the hex side panel), `file_picker` (the Browse popup).
//!
//! This window owns the main thread for the whole process — winit event
//! loops must run there — while the terminal command loop (`main.rs`) runs
//! on a background thread. Both act on the same `SharedGame`
//! (`Arc<Mutex<Option<Game>>>`, see lib.rs): a command from either side is
//! immediately visible to the other, since there's only ever one `Game`.
//! `ui()` polls it a few times a second (`request_repaint_after`) so
//! terminal-driven changes show up without needing a window event to
//! trigger a redraw.

mod file_picker;
mod inspector;
mod map_view;
mod menu;
#[cfg(test)]
mod test_support;

use eframe::egui;

use crate::Error;
use crate::SharedGame;
use crate::game::Game;
use crate::{play_pending_ai_turns, report_turn_transition};

use file_picker::FilePicker;
use menu::{DialogKind, MainMenuState, MenuAction};

/// Open the window for the rest of the process's life. Blocks until closed.
pub fn run(shared: SharedGame) -> Result<(), Error> {
    eframe::run_native(
        "CSE",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(GuiApp::new(shared)))),
    )
    .map_err(|error| Error::new(format!("Failed to start the GUI: {error:?}")))
}

/// A `Move`/`Attack`/`AirSupport` armed from `from`, awaiting a destination
/// click. `Interdict` isn't here: it only needs the already-inspected hex
/// plus the picked air unit, so it applies immediately from a single button
/// rather than arming a second click.
struct PendingOrder {
    kind: OrderKind,
    from: (u32, u32),
}

enum OrderKind {
    Move,
    Attack,
    AirSupport { air_unit: String },
}

struct GuiApp {
    shared: SharedGame,
    selected_hex: Option<(u32, u32)>,
    pending_order: Option<PendingOrder>,
    /// Status/event/battle-report/error lines, oldest first — the window's
    /// equivalent of the terminal's scrollback. Only covers actions taken
    /// through this window; the terminal's own output stays in the terminal.
    log: Vec<String>,
    menu: MainMenuState,
    /// The unit picked in the inspector's air-operations combo box, by name —
    /// carried into an armed `AirSupport` order or an immediate `Interdict`
    /// call. Empty means nothing picked yet.
    selected_air_unit: String,
    /// An open mid-game Save/Load/New popup, if any.
    dialog: Option<DialogKind>,
    /// A confirmed dialog action, applied once `ui()` has released its lock
    /// on `shared` — see `MenuAction`'s doc comment.
    pending_menu_action: Option<MenuAction>,
    /// Map zoom, 1.0 = `HEX_SIZE`'s nominal pixel size; mouse wheel over the
    /// map adjusts it, clamped in `render_map`.
    map_zoom: f32,
    /// Map pan offset in screen pixels, dragged with the primary button.
    map_pan: egui::Vec2,
    /// An open "Browse…" directory listing, if any.
    file_picker: Option<FilePicker>,
}

impl GuiApp {
    fn new(shared: SharedGame) -> Self {
        GuiApp {
            shared,
            selected_hex: None,
            pending_order: None,
            log: Vec::new(),
            menu: MainMenuState::default(),
            selected_air_unit: String::new(),
            dialog: None,
            pending_menu_action: None,
            map_zoom: 1.0,
            map_pan: egui::Vec2::ZERO,
            file_picker: None,
        }
    }
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The shared game can change from the terminal thread at any time,
        // with no window event to prompt a redraw here — poll for it.
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));

        // Lock a clone of the Arc, not `self.shared` directly, so the guard
        // doesn't borrow `self` and block the `&mut self` calls below.
        let shared = self.shared.clone();
        let mut guard = shared.lock().unwrap();
        match guard.as_mut() {
            Some(game) => self.render_playing(ui, game),
            None => self.render_main_menu(ui),
        }
        // Release the lock before acting on a confirmed Save/Load/New/Quit —
        // see `MenuAction`'s doc comment.
        drop(guard);
        self.render_file_picker(ui.ctx());
        self.apply_pending_menu_action();
    }
}

impl GuiApp {
    fn render_playing(&mut self, ui: &mut egui::Ui, game: &mut Game) {
        egui::Panel::top("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(game.status());
                if ui.button("End Turn").clicked() {
                    self.end_turn(game);
                }
                ui.separator();
                if ui.button("Save").clicked() {
                    self.dialog = Some(DialogKind::Save);
                }
                if ui.button("Load").clicked() {
                    self.dialog = Some(DialogKind::Load);
                }
                if ui.button("New").clicked() {
                    self.dialog = Some(DialogKind::New);
                }
                if ui.button("Quit").clicked() {
                    self.pending_menu_action = Some(MenuAction::Quit);
                }
            });
            ui.horizontal(|ui| {
                ui.label("Reports:");
                if ui.button("Victory").clicked() {
                    self.log.push(game.victory_conditions_summary());
                }
                if ui.button("Reinforcements").clicked() {
                    self.log.push(game.reinforcement_schedule_summary());
                }
                if ui.button("Events").clicked() {
                    self.log.push(game.event_schedule_summary());
                }
                if ui.button("Supply").clicked() {
                    self.log.push(game.supply_status_summary());
                }
                if ui.button("Interdiction").clicked() {
                    self.log.push(game.interdiction_summary());
                }
            });
        });

        self.render_dialog(ui);

        egui::Panel::bottom("log").min_size(140.0).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for line in &self.log {
                    ui.label(line);
                }
            });
        });

        if let Some((x, y)) = self.selected_hex {
            egui::Panel::right("inspector").min_size(240.0).show(ui, |ui| {
                self.render_inspector(ui, game, x, y);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_map(ui, game);
        });
    }

    fn end_turn(&mut self, game: &mut Game) {
        let victory = game.end_turn();
        self.log.extend(report_turn_transition(game, &victory));
        self.log.extend(play_pending_ai_turns(game, victory));
    }

    /// Resolve an armed order against the just-clicked destination hex:
    /// `Move` moves the destination hex's first unit (index 0 — picking
    /// which unit in a multi-unit stack moves is a deferred nicety, not
    /// needed for a first cut), `Attack` fights it, `AirSupport` folds the
    /// picked air unit into the same attack without it ever moving. Either
    /// way the result (or the error) goes into the log.
    fn resolve_order(&mut self, game: &mut Game, order: PendingOrder, to: (u32, u32)) {
        match order.kind {
            OrderKind::Move => match game.move_unit(order.from.0, order.from.1, to.0, to.1, 0) {
                Ok(()) => self.selected_hex = Some(to),
                Err(error) => self.log.push(error.error_message),
            },
            OrderKind::Attack => match game.attack(order.from, to, &mut rand::rng()) {
                Ok(report) => {
                    self.log.push(report.to_string());
                    self.selected_hex = Some(to);
                }
                Err(error) => self.log.push(error.error_message),
            },
            OrderKind::AirSupport { air_unit } => {
                match game.air_support(&air_unit, order.from, to, &mut rand::rng()) {
                    Ok(report) => {
                        self.log.push(report.to_string());
                        self.selected_hex = Some(to);
                    }
                    Err(error) => self.log.push(error.error_message),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::*;
    use super::*;
    use crate::core::unit::{LocationCoords, UnitLocation};

    #[test]
    fn resolve_order_moves_a_unit_on_success() {
        let mut game = Game::build(minimal_scenario(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#)).unwrap();
        let mut app = app();

        app.resolve_order(&mut game, PendingOrder { kind: OrderKind::Move, from: (1, 1) }, (2, 1));

        assert!(app.log.is_empty());
        assert_eq!(app.selected_hex, Some((2, 1)));
        assert_eq!(
            game.state.units["Axis Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }),
        );
    }

    #[test]
    fn resolve_order_logs_an_error_on_a_failed_move() {
        let mut game = Game::build(minimal_scenario(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
"#)).unwrap();
        let mut app = app();

        app.resolve_order(&mut game, PendingOrder { kind: OrderKind::Move, from: (1, 1) }, (2, 1));

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].contains("enemy"));
    }

    #[test]
    fn resolve_order_attacks_and_logs_the_battle_report() {
        let mut game = Game::build(minimal_scenario(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }
"#)).unwrap();
        let mut app = app();

        app.resolve_order(&mut game, PendingOrder { kind: OrderKind::Attack, from: (1, 1) }, (2, 1));

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].contains("Outcome"));
        assert_eq!(app.selected_hex, Some((2, 1)));
    }

    #[test]
    fn resolve_order_air_supports_and_logs_the_battle_report() {
        let mut game = Game::build(minimal_scenario(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }

[[units]]
name = "Soviet Division"
toe = "test_toe"
faction = "SU"
location = { x = 2, y = 1 }

[[units]]
name = "Stuka Wing"
toe = "test_toe"
faction = "AX"
location = { x = 0, y = 0 }
"#)).unwrap();
        let mut app = app();

        app.resolve_order(
            &mut game,
            PendingOrder { kind: OrderKind::AirSupport { air_unit: "Stuka Wing".to_string() }, from: (1, 1) },
            (2, 1),
        );

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].contains("Outcome"));
        assert_eq!(app.selected_hex, Some((2, 1)));
        // The air unit never moves — see the "Air support" note in CLAUDE.md.
        assert_eq!(
            game.state.units["Stuka Wing"].location,
            UnitLocation::OnMap(LocationCoords { x: 0, y: 0 }),
        );
    }

    #[test]
    fn end_turn_appends_status_to_the_log() {
        let mut game = Game::build(minimal_scenario(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#)).unwrap();
        let mut app = app();

        app.end_turn(&mut game);

        assert!(!app.log.is_empty());
        assert!(app.log[0].contains("to move"));
    }
}
