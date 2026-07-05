//! The real interface (Phase 6): an eframe/egui window that renders the
//! map, lets the player click a hex to inspect its units, and (slice 2)
//! issue `move`/`attack` orders and end the turn. `air_support`/`interdict`/
//! save/load still need the terminal (see docs/roadmap.md Phase 6).
//!
//! Unlike the debug `view` window (`view.rs`/`visualiser.rs`: a Bevy
//! subprocess built around a serialized snapshot, because winit can only
//! ever create one event loop per process and `view` might be invoked
//! repeatedly from a long-running terminal session), this window is
//! launched once for the whole session as the primary way to play. So it
//! owns the `Game` directly, in-process, and reads its public API fresh
//! every frame — the same relationship `command.rs`/`ai.rs` already have
//! with `Game` — with none of `view.rs`'s snapshot/subprocess machinery.

use eframe::egui;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode, Vec2 as HexVec2};

use crate::Error;
use crate::core::location::Terrain;
use crate::core::unit::UnitLocation;
use crate::game::Game;
use crate::{play_pending_ai_turns, report_turn_transition};

const HEX_SIZE: f32 = 40.0;

/// Load `scenario_path` and open the window for the rest of the process's
/// life. Blocks until the window is closed.
pub fn run(scenario_path: &str) -> Result<(), Error> {
    let contents = std::fs::read_to_string(scenario_path)?;
    let mut game = Game::build(contents)?;
    // A scenario whose first player is AI (e.g. frontline_sector.scen) needs
    // that turn auto-played before the window ever shows control sitting
    // with an AI faction — the same treatment new/load give it in lib.rs.
    let log = play_pending_ai_turns(&mut game, None);

    eframe::run_native(
        "CSE",
        eframe::NativeOptions::default(),
        Box::new(|_cc| Ok(Box::new(GuiApp { game, selected_hex: None, pending_order: None, log }))),
    )
    .map_err(|error| Error::new(format!("Failed to start the GUI: {error:?}")))
}

/// A `Move` or `Attack` armed from `from`, awaiting a destination click.
struct PendingOrder {
    kind: OrderKind,
    from: (u32, u32),
}

enum OrderKind {
    Move,
    Attack,
}

struct GuiApp {
    game: Game,
    selected_hex: Option<(u32, u32)>,
    pending_order: Option<PendingOrder>,
    /// Status/event/battle-report/error lines, oldest first — the window's
    /// equivalent of the terminal's scrollback.
    log: Vec<String>,
}

impl eframe::App for GuiApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("status").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(self.game.status());
                if ui.button("End Turn").clicked() {
                    self.end_turn();
                }
            });
        });

        egui::Panel::bottom("log").min_size(140.0).show(ui, |ui| {
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for line in &self.log {
                    ui.label(line);
                }
            });
        });

        if let Some((x, y)) = self.selected_hex {
            egui::Panel::right("inspector").min_size(240.0).show(ui, |ui| {
                render_inspector(ui, &self.game, x, y, &mut self.pending_order);
            });
        }

        egui::CentralPanel::default().show(ui, |ui| {
            self.render_map(ui);
        });
    }
}

impl GuiApp {
    fn end_turn(&mut self) {
        let victory = self.game.end_turn();
        self.log.extend(report_turn_transition(&mut self.game, &victory));
        self.log.extend(play_pending_ai_turns(&mut self.game, victory));
    }

    /// Resolve an armed order against the just-clicked destination hex:
    /// `Move` moves the destination hex's first unit (index 0 — picking
    /// which unit in a multi-unit stack moves is a deferred nicety, not
    /// needed for a first cut), `Attack` fights it. Either way the result
    /// (or the error) goes into the log.
    fn resolve_order(&mut self, order: PendingOrder, to: (u32, u32)) {
        match order.kind {
            OrderKind::Move => match self.game.move_unit(order.from.0, order.from.1, to.0, to.1, 0) {
                Ok(()) => self.selected_hex = Some(to),
                Err(error) => self.log.push(error.error_message),
            },
            OrderKind::Attack => match self.game.attack(order.from, to, &mut rand::rng()) {
                Ok(report) => {
                    self.log.push(report.to_string());
                    self.selected_hex = Some(to);
                }
                Err(error) => self.log.push(error.error_message),
            },
        }
    }

    fn render_map(&mut self, ui: &mut egui::Ui) {
        let hexes = self.game.state.map.all_locations();
        let hex_set: std::collections::HashSet<(u32, u32)> =
            hexes.iter().map(|(coords, _)| *coords).collect();

        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let painter = ui.painter_at(rect);
        let view = MapView::new(&hexes, rect.center());

        for ((x, y), terrain) in &hexes {
            view.draw_hex(&painter, *x, *y, *terrain);
        }
        for unit in self.game.state.units.values() {
            if let UnitLocation::OnMap(coords) = &unit.location {
                view.draw_unit(&painter, coords.x, coords.y, &unit.name, &unit.faction);
            }
        }

        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos() {
                let hex = view.hex_at(pos, &hex_set);
                match (self.pending_order.take(), hex) {
                    (Some(order), Some(target)) => self.resolve_order(order, target),
                    _ => self.selected_hex = hex,
                }
            }
    }
}

fn render_inspector(ui: &mut egui::Ui, game: &Game, x: u32, y: u32, pending_order: &mut Option<PendingOrder>) {
    let Some(location) = game.state.map.get_location(x, y) else {
        ui.label("Invalid hex.");
        return;
    };
    ui.heading(format!("({x}, {y}) — {:?}", location.terrain));

    let units = game.units_at_location(location);

    if pending_order.is_some() {
        ui.label("Click a destination hex…");
        if ui.button("Cancel").clicked() {
            *pending_order = None;
        }
    } else if units.iter().any(|unit| unit.faction == game.current_faction()) {
        ui.horizontal(|ui| {
            if ui.button("Move").clicked() {
                *pending_order = Some(PendingOrder { kind: OrderKind::Move, from: (x, y) });
            }
            if ui.button("Attack").clicked() {
                *pending_order = Some(PendingOrder { kind: OrderKind::Attack, from: (x, y) });
            }
        });
    }

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
/// as `visualiser.rs`'s `map_center`, so both views agree on where "the
/// middle of the map" is.
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

    // resolve_order/end_turn touch only game/selected_hex/log — no live
    // egui::Context needed, so these are exercised directly rather than
    // through simulated clicks (this sandbox has no input-injection tool
    // to drive the real window with anyway).

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

    fn app(units: &str) -> GuiApp {
        let game = Game::build(minimal_scenario(units)).unwrap();
        GuiApp { game, selected_hex: None, pending_order: None, log: Vec::new() }
    }

    #[test]
    fn resolve_order_moves_a_unit_on_success() {
        let mut app = app(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#);

        app.resolve_order(PendingOrder { kind: OrderKind::Move, from: (1, 1) }, (2, 1));

        assert!(app.log.is_empty());
        assert_eq!(app.selected_hex, Some((2, 1)));
        assert_eq!(
            app.game.state.units["Axis Division"].location,
            UnitLocation::OnMap(LocationCoords { x: 2, y: 1 }),
        );
    }

    #[test]
    fn resolve_order_logs_an_error_on_a_failed_move() {
        let mut app = app(r#"
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
"#);

        app.resolve_order(PendingOrder { kind: OrderKind::Move, from: (1, 1) }, (2, 1));

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].contains("enemy"));
    }

    #[test]
    fn resolve_order_attacks_and_logs_the_battle_report() {
        let mut app = app(r#"
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
"#);

        app.resolve_order(PendingOrder { kind: OrderKind::Attack, from: (1, 1) }, (2, 1));

        assert_eq!(app.log.len(), 1);
        assert!(app.log[0].contains("Outcome"));
        assert_eq!(app.selected_hex, Some((2, 1)));
    }

    #[test]
    fn end_turn_appends_status_to_the_log() {
        let mut app = app(r#"
[[units]]
name = "Axis Division"
toe = "test_toe"
faction = "AX"
location = { x = 1, y = 1 }
"#);

        app.end_turn();

        assert!(!app.log.is_empty());
        assert!(app.log[0].contains("to move"));
    }
}
