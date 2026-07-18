//! The map: hex-to-screen mapping (`MapView`), terrain/unit/flag drawing,
//! and `render_map` — zoom (mouse wheel), pan (drag), and click resolution
//! to hex selection or an armed order's destination.

use eframe::egui;
use hexx::{Hex, HexLayout, HexOrientation, OffsetHexMode, Vec2 as HexVec2};

use crate::core::location::Terrain;
use crate::core::unit::UnitLocation;
use crate::game::Game;

use super::GuiApp;

const HEX_SIZE: f32 = 40.0;

impl GuiApp {
    pub(super) fn render_map(&mut self, ui: &mut egui::Ui, game: &mut Game) {
        let hexes = game.map_locations();
        let hex_set: std::collections::HashSet<(u32, u32)> =
            hexes.iter().map(|(coords, _)| *coords).collect();

        let rect = ui.available_rect_before_wrap();
        let response = ui.allocate_rect(rect, egui::Sense::click_and_drag());

        // Zoom (mouse wheel, only while hovering the map) and pan (drag)
        // adjust persistent state on `self`; a plain click still resolves to
        // whatever hex is under the cursor afterwards, drags don't count as
        // clicks so panning never re-selects a hex mid-drag.
        if response.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                self.map_zoom = (self.map_zoom * (1.0 + scroll * 0.001)).clamp(0.3, 3.0);
            }
        }
        if response.dragged() {
            self.map_pan += response.drag_delta();
        }

        let painter = ui.painter_at(rect);
        let view = MapView::new(&hexes, rect.center(), self.map_zoom, self.map_pan);

        for ((x, y), terrain) in &hexes {
            view.draw_hex(&painter, *x, *y, *terrain);
        }
        for hex in game.victory_hexes() {
            view.draw_victory_flag(&painter, hex.x, hex.y, hex.points);
        }

        let onmap_units: Vec<((u32, u32), String, String)> = game.units_by_name().into_iter()
            .filter_map(|unit| match &unit.location {
                UnitLocation::OnMap(coords) => Some(((coords.x, coords.y), unit.name.clone(), unit.faction.clone())),
                UnitLocation::Offmap(_) => None,
            })
            .collect();
        for (coords, name, faction, slot) in assign_stack_slots(onmap_units) {
            // Re-looked-up rather than threaded through assign_stack_slots:
            // fort_level doesn't affect sort order, only the drawing.
            let fort_level = game.unit(&name).map_or(0, |unit| unit.fort_level);
            view.draw_unit(&painter, UnitMarker { x: coords.0, y: coords.1, slot, name: &name, faction: &faction, fort_level });
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
}

fn hex_layout(zoom: f32) -> HexLayout {
    HexLayout {
        orientation: HexOrientation::Pointy,
        scale: HexVec2::splat(HEX_SIZE * zoom),
        origin: HexVec2::ZERO,
    }
}

fn to_hex(x: u32, y: u32) -> Hex {
    Hex::from_offset_coordinates([x as i32, y as i32], OffsetHexMode::Even, HexOrientation::Pointy)
}

/// Everything `MapView::draw_unit` needs for one on-map unit marker, bundled
/// to keep the method's argument count sane.
struct UnitMarker<'a> {
    x: u32,
    y: u32,
    slot: u32,
    name: &'a str,
    faction: &'a str,
    fort_level: u32,
}

/// The hex-to-screen mapping for one frame's map render: the map's world-space
/// center (so it sits in the middle of the panel) plus the panel's own
/// on-screen center, bundled so drawing/hit-testing don't each need every
/// piece passed separately.
struct MapView {
    layout: HexLayout,
    panel_center: egui::Pos2,
    map_center: HexVec2,
    /// Screen offset dragged in by the player — added after the map-centering
    /// math below, so panning never disturbs where zoom centers.
    pan: egui::Vec2,
    /// `HEX_SIZE * zoom`: every on-screen size in this view scales from this,
    /// so hexes, unit markers and flags all grow/shrink together.
    size: f32,
}

impl MapView {
    fn new(hexes: &[((u32, u32), Terrain)], panel_center: egui::Pos2, zoom: f32, pan: egui::Vec2) -> Self {
        let layout = hex_layout(zoom);
        let map_center = map_center(&layout, hexes);
        MapView { layout, panel_center, map_center, pan, size: HEX_SIZE * zoom }
    }

    /// Where a hex is drawn: centered in the panel (plus any pan offset),
    /// offset by its hexx world position relative to the map's center.
    /// Screen Y grows downward, hexx's world Y grows upward, hence the flip.
    fn screen_pos(&self, x: u32, y: u32) -> egui::Pos2 {
        let world = self.layout.hex_to_world_pos(to_hex(x, y));
        egui::pos2(
            self.panel_center.x + self.pan.x + (world.x - self.map_center.x),
            self.panel_center.y + self.pan.y - (world.y - self.map_center.y),
        )
    }

    /// The inverse of `screen_pos`: which on-map hex (if any) a screen
    /// position falls in. `hexes` restricts the result to real map hexes —
    /// `world_pos_to_hex` always returns the mathematically nearest hex, on
    /// or off the map.
    fn hex_at(&self, screen: egui::Pos2, hexes: &std::collections::HashSet<(u32, u32)>) -> Option<(u32, u32)> {
        let world = HexVec2::new(
            screen.x - self.panel_center.x - self.pan.x + self.map_center.x,
            -(screen.y - self.panel_center.y - self.pan.y) + self.map_center.y,
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
                    center.x + self.size * 0.93 * angle.cos(),
                    center.y + self.size * 0.93 * angle.sin(),
                )
            })
            .collect();
        painter.add(egui::Shape::convex_polygon(
            corners,
            terrain_color(terrain),
            egui::Stroke::new(1.0, egui::Color32::BLACK),
        ));
        painter.text(
            egui::pos2(center.x, center.y + self.size * 0.55),
            egui::Align2::CENTER_CENTER,
            format!("{x},{y}"),
            egui::FontId::proportional(10.0 * (self.size / HEX_SIZE)),
            egui::Color32::from_black_alpha(180),
        );
    }

    /// `slot` is this unit's 0-based position among others stacked on the
    /// same hex (see `assign_stack_slots`) — stacked units offset sideways
    /// instead of drawing on top of each other.
    fn draw_unit(&self, painter: &egui::Painter, marker: UnitMarker) {
        let center = self.screen_pos(marker.x, marker.y);
        let offset_x = marker.slot as f32 * self.size * 0.28;
        let pos = egui::pos2(center.x + offset_x, center.y - self.size * 0.15);
        painter.circle_filled(pos, self.size * 0.22, faction_color(marker.faction));
        painter.text(
            pos,
            egui::Align2::CENTER_CENTER,
            short_name(marker.name),
            egui::FontId::proportional(9.0 * (self.size / HEX_SIZE)),
            egui::Color32::WHITE,
        );

        // Entrenchment: one small pip per fort level, in a row under the
        // marker — a glance at how dug in this unit is without needing to
        // inspect it.
        let pip_radius = self.size * 0.035;
        let pip_gap = self.size * 0.09;
        let pip_y = pos.y + self.size * 0.22 + pip_radius * 1.5;
        let first_pip_x = pos.x - pip_gap * (marker.fort_level.saturating_sub(1)) as f32 / 2.0;
        for level in 0..marker.fort_level {
            let pip_x = first_pip_x + pip_gap * level as f32;
            painter.circle_filled(
                egui::pos2(pip_x, pip_y),
                pip_radius,
                egui::Color32::from_rgb(230, 200, 60),
            );
        }
    }

    /// A small pennant flag in the hex's top-right quadrant, with its point
    /// value beneath — mirrors the old Bevy visualiser's victory-hex marker,
    /// just never ported to egui until now.
    fn draw_victory_flag(&self, painter: &egui::Painter, x: u32, y: u32, points: f32) {
        let center = self.screen_pos(x, y);
        let pole_x = center.x + self.size * 0.35;
        let pole_bottom = egui::pos2(pole_x, center.y + self.size * 0.05);
        let pole_top = egui::pos2(pole_x, center.y - self.size * 0.45);
        let pole_color = egui::Color32::from_rgb(38, 38, 38);

        painter.line_segment([pole_bottom, pole_top], egui::Stroke::new(self.size * 0.05, pole_color));
        painter.add(egui::Shape::convex_polygon(
            vec![
                egui::pos2(pole_top.x, pole_top.y - self.size * 0.16),
                egui::pos2(pole_top.x, pole_top.y + self.size * 0.05),
                egui::pos2(pole_top.x + self.size * 0.22, pole_top.y - self.size * 0.055),
            ],
            egui::Color32::from_rgb(242, 204, 26),
            egui::Stroke::NONE,
        ));
        painter.text(
            egui::pos2(pole_x, center.y + self.size * 0.13),
            egui::Align2::CENTER_CENTER,
            format!("{points:.0}"),
            egui::FontId::proportional(11.0 * (self.size / HEX_SIZE)),
            egui::Color32::from_rgb(242, 204, 26),
        );
    }
}

/// Each on-map unit's 0-based position among others sharing its hex, ordered
/// by name for determinism — lets `draw_unit` offset stacked units sideways
/// instead of drawing them on top of each other. `units` is (hex, name,
/// faction); returns the same with a slot index appended.
fn assign_stack_slots(mut units: Vec<((u32, u32), String, String)>) -> Vec<((u32, u32), String, String, u32)> {
    units.sort_by(|a, b| a.1.cmp(&b.1));
    let mut counts: std::collections::HashMap<(u32, u32), u32> = std::collections::HashMap::new();
    units.into_iter()
        .map(|(coords, name, faction)| {
            let slot = counts.entry(coords).or_insert(0);
            let assigned = *slot;
            *slot += 1;
            (coords, name, faction, assigned)
        })
        .collect()
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
        let view = MapView::new(&all, egui::pos2(400.0, 300.0), 1.0, egui::Vec2::ZERO);

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
        let view = MapView::new(&all, egui::pos2(400.0, 300.0), 1.0, egui::Vec2::ZERO);

        let far_away = egui::pos2(view.panel_center.x + 5000.0, view.panel_center.y + 5000.0);
        assert_eq!(view.hex_at(far_away, &hex_set), None);
    }

    #[test]
    fn map_center_of_no_hexes_is_the_origin() {
        assert_eq!(map_center(&hex_layout(1.0), &[]), HexVec2::ZERO);
    }

    #[test]
    fn stack_slots_are_zero_based_and_ordered_by_name_within_a_hex() {
        let units = vec![
            ((1, 1), "Bravo".to_string(), "AX".to_string()),
            ((1, 1), "Alpha".to_string(), "AX".to_string()),
            ((2, 2), "Charlie".to_string(), "SU".to_string()),
        ];

        let slots = assign_stack_slots(units);

        let bravo = slots.iter().find(|(_, name, ..)| name == "Bravo").unwrap();
        let alpha = slots.iter().find(|(_, name, ..)| name == "Alpha").unwrap();
        let charlie = slots.iter().find(|(_, name, ..)| name == "Charlie").unwrap();
        // Alpha sorts before Bravo, so it claims slot 0 at their shared hex.
        assert_eq!(alpha.3, 0);
        assert_eq!(bravo.3, 1);
        // A different hex starts its own count from 0.
        assert_eq!(charlie.3, 0);
    }
}
