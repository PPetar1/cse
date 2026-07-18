//! The side panel for an inspected hex: terrain, the units there with their
//! rosters, Move/Attack order buttons, and the air-operations block.

use eframe::egui;

use crate::game::Game;

use super::{GuiApp, OrderKind, PendingOrder};

impl GuiApp {
    /// The side panel for an inspected hex: its terrain and units, Move/
    /// Attack buttons if it holds a unit of the current faction, and an
    /// air-operations block (a unit picker plus Air Support/Interdict)
    /// available regardless of who holds the hex, since interdiction covers
    /// hexes you don't occupy. Each hex/unit lookup is scoped tightly (and
    /// repeated where needed) so no shared borrow of `game` is still alive
    /// when `Interdict` needs a `&mut Game` call partway through.
    pub(super) fn render_inspector(&mut self, ui: &mut egui::Ui, game: &mut Game, x: u32, y: u32) {
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

        if !game.is_visible_to(game.current_faction(), x, y) {
            ui.label("Unknown — outside detection range.");
            return;
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
            ui.label(format!("Entrenchment: level {}", unit.fort_level));
            ui.label(format!("Leader: {}", unit.leader.as_deref().unwrap_or("none")));
            for element in &unit.elements {
                ui.label(format!(
                    "{}: {} ready, {} damaged — morale {}, experience {}",
                    element.name, element.ready, element.damaged, element.morale, element.experience,
                ));
            }
        }
    }
}
