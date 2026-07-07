//! The real interface (Phase 6): an eframe/egui window that shows a main
//! menu (New/Load/Quit) when no game is active, and once one is, renders the
//! map, lets the player click a hex to inspect its units, issue `move`/
//! `attack`/`air_support`/`interdict` orders, end the turn, and (from the
//! same header) save/load/start a new game or quit mid-session. Only an
//! in-app scenario/save-file picker is still terminal-only — paths are
//! typed, not browsed (see docs/roadmap.md Phase 6).
//!
//! This window owns the main thread for the whole process — winit event
//! loops must run there — while the terminal command loop (`main.rs`) runs
//! on a background thread. Both act on the same `SharedGame`
//! (`Arc<Mutex<Option<Game>>>`, see lib.rs): a command from either side is
//! immediately visible to the other, since there's only ever one `Game`.
//! `ui()` polls it a few times a second (`request_repaint_after`) so
//! terminal-driven changes show up without needing a window event to
//! trigger a redraw.

use eframe::egui;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode, Vec2 as HexVec2};

use crate::Error;
use crate::SharedGame;
use crate::core::location::Terrain;
use crate::core::unit::UnitLocation;
use crate::game::Game;
use crate::{play_pending_ai_turns, report_turn_transition};

const HEX_SIZE: f32 = 40.0;

/// Open the window for the rest of the process's life. Blocks until closed.
pub fn run(shared: SharedGame) -> Result<(), Error> {
    eframe::run_native(
        "CSE",
        eframe::NativeOptions::default(),
        Box::new(|_cc| {
            Ok(Box::new(GuiApp {
                shared,
                selected_hex: None,
                pending_order: None,
                log: Vec::new(),
                menu: MainMenuState::default(),
                selected_air_unit: String::new(),
                dialog: None,
                pending_menu_action: None,
            }))
        }),
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

/// Text fields and any error from the last New/Load attempt — shared between
/// the main menu and the mid-game Save/Load/New dialogs (`DialogKind`),
/// since both need the same three path fields.
struct MainMenuState {
    scenario_path: String,
    load_path: String,
    save_path: String,
    error: Option<String>,
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
enum DialogKind {
    Save,
    Load,
    New,
}

/// A Save/Load/New/Quit action confirmed from a `DialogKind` popup, deferred
/// until after `ui()` releases its lock on `shared` — `MenuAction::Load`/
/// `New` end up calling `GuiApp::adopt_game`, which locks `shared` itself,
/// and `std::sync::Mutex` isn't reentrant, so running these while
/// `render_playing` still holds the guard would deadlock.
enum MenuAction {
    Save(String),
    Load(String),
    New(String),
    Quit,
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
        self.apply_pending_menu_action();
    }
}

impl GuiApp {
    fn render_main_menu(&mut self, ui: &mut egui::Ui) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.heading("CSE — Combat Simulation Engine");
                ui.add_space(30.0);

                ui.label("Scenario file:");
                ui.add(egui::TextEdit::singleline(&mut self.menu.scenario_path).desired_width(320.0));
                if ui.button("New Game").clicked() {
                    self.pending_menu_action = Some(MenuAction::New(self.menu.scenario_path.clone()));
                }

                ui.add_space(20.0);

                ui.label("Save file:");
                ui.add(egui::TextEdit::singleline(&mut self.menu.load_path).desired_width(320.0));
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
    fn apply_pending_menu_action(&mut self) {
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

    /// The Save/Load/New popup armed by the top panel's buttons: a path
    /// field (reusing the same `MainMenuState` fields the main menu's own
    /// New/Load use) plus Confirm/Cancel. Confirm only arms
    /// `pending_menu_action` — see its doc comment for why it can't act
    /// immediately here.
    fn render_dialog(&mut self, ui: &mut egui::Ui) {
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
            ui.text_edit_singleline(path);
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

    fn render_map(&mut self, ui: &mut egui::Ui, game: &mut Game) {
        let hexes = game.state.map.all_locations();
        let hex_set: std::collections::HashSet<(u32, u32)> =
            hexes.iter().map(|(coords, _)| *coords).collect();

        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let painter = ui.painter_at(rect);
        let view = MapView::new(&hexes, rect.center());

        for ((x, y), terrain) in &hexes {
            view.draw_hex(&painter, *x, *y, *terrain);
        }
        for unit in game.state.units.values() {
            if let UnitLocation::OnMap(coords) = &unit.location {
                view.draw_unit(&painter, coords.x, coords.y, &unit.name, &unit.faction);
            }
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos() {
                let hex = view.hex_at(pos, &hex_set);
                match (self.pending_order.take(), hex) {
                    (Some(order), Some(target)) => self.resolve_order(game, order, target),
                    _ => self.selected_hex = hex,
                }
            }
    }

    /// The side panel for an inspected hex: its terrain and units, Move/
    /// Attack buttons if it holds a unit of the current faction, and an
    /// air-operations block (a unit picker plus Air Support/Interdict)
    /// available regardless of who holds the hex, since interdiction covers
    /// hexes you don't occupy. Each hex/unit lookup is scoped tightly (and
    /// repeated where needed) so no shared borrow of `game` is still alive
    /// when `Interdict` needs a `&mut Game` call partway through.
    fn render_inspector(&mut self, ui: &mut egui::Ui, game: &mut Game, x: u32, y: u32) {
        let Some(terrain) = game.state.map.get_location(x, y).map(|location| location.terrain) else {
            ui.label("Invalid hex.");
            return;
        };
        ui.heading(format!("({x}, {y}) — {terrain:?}"));

        let owns_hex = {
            let location = game.state.map.get_location(x, y).unwrap();
            game.units_at_location(location).iter().any(|unit| unit.faction == game.current_faction())
        };

        if let Some(order) = &self.pending_order {
            let prompt = match order.kind {
                OrderKind::AirSupport { .. } => "Click the target hex…",
                OrderKind::Move | OrderKind::Attack => "Click a destination hex…",
            };
            ui.label(prompt);
            if ui.button("Cancel").clicked() {
                self.pending_order = None;
            }
        } else {
            if owns_hex {
                ui.horizontal(|ui| {
                    if ui.button("Move").clicked() {
                        self.pending_order = Some(PendingOrder { kind: OrderKind::Move, from: (x, y) });
                    }
                    if ui.button("Attack").clicked() {
                        self.pending_order = Some(PendingOrder { kind: OrderKind::Attack, from: (x, y) });
                    }
                });
            }

            ui.separator();
            ui.label("Air operations:");
            egui::ComboBox::from_label("Unit")
                .selected_text(if self.selected_air_unit.is_empty() { "(choose a unit)" } else { &self.selected_air_unit })
                .show_ui(ui, |ui| {
                    for unit in game.units_of_faction(game.current_faction()) {
                        ui.selectable_value(&mut self.selected_air_unit, unit.name.clone(), &unit.name);
                    }
                });

            let have_air_unit = !self.selected_air_unit.is_empty();
            ui.horizontal(|ui| {
                if owns_hex
                    && ui.add_enabled(have_air_unit, egui::Button::new("Air Support")).clicked() {
                        self.pending_order = Some(PendingOrder {
                            kind: OrderKind::AirSupport { air_unit: self.selected_air_unit.clone() },
                            from: (x, y),
                        });
                    }
                if ui.add_enabled(have_air_unit, egui::Button::new("Interdict")).clicked() {
                    match game.interdict(&self.selected_air_unit, (x, y)) {
                        Ok(()) => self.log.push(format!("{} now covers ({x}, {y}).", self.selected_air_unit)),
                        Err(error) => self.log.push(error.error_message),
                    }
                }
            });
        }

        let location = game.state.map.get_location(x, y).unwrap();
        let units = game.units_at_location(location);
        if units.is_empty() {
            ui.label("No units here.");
            return;
        }
        for unit in units {
            ui.separator();
            ui.label(egui::RichText::new(&unit.name).strong());
            ui.label(format!("Faction: {}   TOE: {}", unit.faction, unit.toe));
            for element in &unit.elements {
                ui.label(format!(
                    "{}: {} ready, {} damaged — morale {}, experience {}",
                    element.name, element.ready, element.damaged, element.morale, element.experience,
                ));
            }
        }
    }
}

fn hex_layout() -> HexLayout {
    HexLayout {
        orientation: HexOrientation::Pointy,
        scale: HexVec2::splat(HEX_SIZE),
        origin: HexVec2::ZERO,
    }
}

fn to_hex(x: u32, y: u32) -> Hex {
    Hex::from_offset_coordinates([x as i32, y as i32], OffsetHexMode::Even, HexOrientation::Pointy)
}

/// The hex-to-screen mapping for one frame's map render: the map's world-space
/// center (so it sits in the middle of the panel) plus the panel's own
/// on-screen center, bundled so drawing/hit-testing don't each need every
/// piece passed separately.
struct MapView {
    layout: HexLayout,
    panel_center: egui::Pos2,
    map_center: HexVec2,
}

impl MapView {
    fn new(hexes: &[((u32, u32), Terrain)], panel_center: egui::Pos2) -> Self {
        let layout = hex_layout();
        let map_center = map_center(&layout, hexes);
        MapView { layout, panel_center, map_center }
    }

    /// Where a hex is drawn: centered in the panel, offset by its hexx world
    /// position relative to the map's center. Screen Y grows downward,
    /// hexx's world Y grows upward, hence the flip.
    fn screen_pos(&self, x: u32, y: u32) -> egui::Pos2 {
        let world = self.layout.hex_to_world_pos(to_hex(x, y));
        egui::pos2(
            self.panel_center.x + (world.x - self.map_center.x),
            self.panel_center.y - (world.y - self.map_center.y),
        )
    }

    /// The inverse of `screen_pos`: which on-map hex (if any) a screen
    /// position falls in. `hexes` restricts the result to real map hexes —
    /// `world_pos_to_hex` always returns the mathematically nearest hex, on
    /// or off the map.
    fn hex_at(&self, screen: egui::Pos2, hexes: &std::collections::HashSet<(u32, u32)>) -> Option<(u32, u32)> {
        let world = HexVec2::new(
            screen.x - self.panel_center.x + self.map_center.x,
            -(screen.y - self.panel_center.y) + self.map_center.y,
        );
        let hex = self.layout.world_pos_to_hex(world);
        let [x, y] = hex.to_offset_coordinates(OffsetHexMode::Even, HexOrientation::Pointy);
        if x < 0 || y < 0 {
            return None;
        }
        let coords = (x as u32, y as u32);
        hexes.contains(&coords).then_some(coords)
    }

    fn draw_hex(&self, painter: &egui::Painter, x: u32, y: u32, terrain: Terrain) {
        let center = self.screen_pos(x, y);
        let corners: Vec<egui::Pos2> = (0..6)
            .map(|i| {
                let angle = (60.0 * i as f32 - 30.0).to_radians();
                egui::pos2(
                    center.x + HEX_SIZE * 0.93 * angle.cos(),
                    center.y + HEX_SIZE * 0.93 * angle.sin(),
                )
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(
            corners,
            terrain_color(terrain),
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        ));
        painter.text(
            egui::pos2(center.x, center.y + HEX_SIZE * 0.55),
            egui::Align2::CENTER_CENTER,
            format!("{x},{y}"),
            egui::FontId::proportional(10.0),
            egui::Color32::from_black_alpha(180),
        );
    }

    fn draw_unit(&self, painter: &egui::Painter, x: u32, y: u32, name: &str, faction: &str) {
        let center = self.screen_pos(x, y);
        let pos = egui::pos2(center.x, center.y - HEX_SIZE * 0.15);
        painter.circle_filled(pos, HEX_SIZE * 0.22, faction_color(faction));
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            short_name(name),
            egui::FontId::proportional(9.0),
            egui::Color32::WHITE,
        );
    }
}

/// Center of the map's bounding box in hexx world space — same computation
/// as the old Bevy visualiser used, so both agreed on where "the middle of
/// the map" is (retained now as this window's sole map view).
fn map_center(layout: &HexLayout, hexes: &[((u32, u32), Terrain)]) -> HexVec2 {
    let positions = hexes.iter().map(|((x, y), _)| layout.hex_to_world_pos(to_hex(*x, *y)));
    let (mut min, mut max) = (HexVec2::splat(f32::MAX), HexVec2::splat(f32::MIN));
    for pos in positions {
        min = min.min(pos);
        max = max.max(pos);
    }
    if min.x > max.x {
        return HexVec2::ZERO; // No hexes — nothing to center on.
    }
    (min + max) / 2.0
}

fn short_name(name: &str) -> String {
    if name.chars().count() > 18 {
        let truncated: String = name.chars().take(17).collect();
        format!("{truncated}…")
    } else {
        name.to_string()
    }
}

fn terrain_color(terrain: Terrain) -> egui::Color32 {
    let (r, g, b) = match terrain {
        Terrain::Plains => (140, 194, 102),
        Terrain::Forest => (46, 115, 46),
        Terrain::Hills => (184, 153, 97),
        Terrain::Mountain => (148, 148, 153),
        Terrain::Swamp => (97, 128, 77),
        Terrain::Desert => (230, 209, 128),
        Terrain::Water => (51, 107, 199),
        Terrain::Urban => (128, 128, 133),
    };
    egui::Color32::from_rgb(r, g, b)
}

fn faction_color(faction: &str) -> egui::Color32 {
    match faction {
        "AX" => egui::Color32::from_rgb(204, 51, 51),
        "SU" => egui::Color32::from_rgb(51, 51, 204),
        _ => egui::Color32::GRAY,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexes(coords: &[(u32, u32)]) -> Vec<((u32, u32), Terrain)> {
        coords.iter().map(|&c| (c, Terrain::Plains)).collect()
    }

    #[test]
    fn clicking_a_hexs_center_resolves_to_that_hex() {
        let all = hexes(&[(0, 0), (1, 0), (0, 1), (3, 3)]);
        let hex_set: std::collections::HashSet<(u32, u32)> =
            all.iter().map(|(coords, _)| *coords).collect();
        let view = MapView::new(&all, egui::pos2(400.0, 300.0));

        for &(x, y) in &[(0, 0), (1, 0), (0, 1), (3, 3)] {
            let screen = view.screen_pos(x, y);
            assert_eq!(view.hex_at(screen, &hex_set), Some((x, y)));
        }
    }

    #[test]
    fn clicking_well_outside_the_map_resolves_to_nothing() {
        let all = hexes(&[(0, 0), (1, 0), (0, 1)]);
        let hex_set: std::collections::HashSet<(u32, u32)> =
            all.iter().map(|(coords, _)| *coords).collect();
        let view = MapView::new(&all, egui::pos2(400.0, 300.0));

        let far_away = egui::pos2(view.panel_center.x + 5000.0, view.panel_center.y + 5000.0);
        assert_eq!(view.hex_at(far_away, &hex_set), None);
    }

    #[test]
    fn map_center_of_no_hexes_is_the_origin() {
        assert_eq!(map_center(&hex_layout(), &[]), HexVec2::ZERO);
    }

    // resolve_order/end_turn/adopt_game touch only plain data (game, log,
    // selected_hex, shared, menu) — no live egui::Context needed, so these
    // are exercised directly rather than through simulated clicks (this
    // sandbox has no input-injection tool to drive the real window anyway).

    use crate::core::unit::LocationCoords;

    fn minimal_scenario(units: &str) -> String {
        let map_path = concat!(env!("CARGO_MANIFEST_DIR"), "/maps/basic_map.map");
        format!(r#"
name = "gui test scenario"
game_version = "0.1.0"
map = "{map_path}"
start_date = "1941-06-22"
turn_length = 7

[[players]]
faction_name = "Axis"
faction_tag = "AX"
[[players]]
faction_name = "Soviet Union"
faction_tag = "SU"

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

{units}
"#)
    }

    fn app() -> GuiApp {
        GuiApp {
            shared: crate::new_shared_game(),
            selected_hex: None,
            pending_order: None,
            log: Vec::new(),
            menu: MainMenuState::default(),
            selected_air_unit: String::new(),
            dialog: None,
            pending_menu_action: None,
        }
    }

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
