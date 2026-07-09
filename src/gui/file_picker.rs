//! The "Browse…" popup: a small in-window directory listing that fills in a
//! scenario/save path field instead of requiring the path to be typed. No
//! native-dialog dependency — just `std::fs::read_dir` in an `egui::Window`,
//! so it works the same everywhere `cargo run` does.

use std::path::PathBuf;

use eframe::egui;

use super::GuiApp;

/// Which `MainMenuState` path field a `FilePicker` writes its selection
/// into — the same three fields the main menu and the mid-game `DialogKind`
/// popups already share.
#[derive(Clone, Copy)]
pub(super) enum PickerField {
    Scenario,
    Load,
    Save,
}

/// An open directory listing, browsing toward a New/Load/Save path — the
/// "Browse…" button next to every path field opens one instead of requiring
/// the path to be typed.
pub(super) struct FilePicker {
    dir: PathBuf,
    field: PickerField,
    error: Option<String>,
}

impl FilePicker {
    /// Starts in `scenarios/` (for a scenario path) or `save/` (for a save
    /// path) if that directory exists, else the current directory.
    pub(super) fn open(field: PickerField) -> Self {
        let preferred = match field {
            PickerField::Scenario => "scenarios",
            PickerField::Load | PickerField::Save => "save",
        };
        let dir = if std::path::Path::new(preferred).is_dir() {
            PathBuf::from(preferred)
        } else {
            PathBuf::from(".")
        };
        FilePicker { dir, field, error: None }
    }
}

impl GuiApp {
    /// The "Browse…" popup: lists `self.file_picker`'s current directory —
    /// `..` (if not already at the root), then subdirectories, then files,
    /// each a click away from navigating in or (for a file) filling in the
    /// matching `MainMenuState` path field and closing the popup. A read
    /// error (permissions, a removed directory) shows in the popup instead
    /// of crashing or silently closing it.
    pub(super) fn render_file_picker(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.file_picker else { return };
        let mut selected = None;
        let mut close = false;

        egui::Window::new("Browse").collapsible(false).resizable(true).show(ctx, |ui| {
            ui.label(picker.dir.display().to_string());
            ui.separator();
            egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                if let Some(parent) = picker.dir.parent()
                    && ui.selectable_label(false, "⬆  ..").clicked() {
                        picker.dir = parent.to_path_buf();
                    }
                match std::fs::read_dir(&picker.dir) {
                    Ok(entries) => {
                        let mut entries: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
                        entries.sort_by_key(|entry| (!entry.path().is_dir(), entry.file_name()));
                        for entry in entries {
                            let path = entry.path();
                            let name = entry.file_name().to_string_lossy().to_string();
                            if path.is_dir() {
                                if ui.selectable_label(false, format!("📁 {name}")).clicked() {
                                    picker.dir = path;
                                }
                            } else if ui.selectable_label(false, format!("    {name}")).clicked() {
                                selected = Some(path);
                            }
                        }
                        picker.error = None;
                    }
                    Err(error) => picker.error = Some(format!("Can't read this folder: {error}")),
                }
            });
            if let Some(error) = &picker.error {
                ui.colored_label(egui::Color32::from_rgb(200, 60, 60), error);
            }
            ui.separator();
            if ui.button("Cancel").clicked() {
                close = true;
            }
        });

        if let Some(path) = selected {
            match picker.field {
                PickerField::Scenario => self.menu.scenario_path = path.display().to_string(),
                PickerField::Load => self.menu.load_path = path.display().to_string(),
                PickerField::Save => self.menu.save_path = path.display().to_string(),
            }
            close = true;
        }
        if close {
            self.file_picker = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_picker_starts_in_the_conventional_directory_for_its_field() {
        // Run from the crate root (cargo's test working directory), where
        // both scenarios/ and save/ exist.
        assert_eq!(FilePicker::open(PickerField::Scenario).dir, PathBuf::from("scenarios"));
        assert_eq!(FilePicker::open(PickerField::Load).dir, PathBuf::from("save"));
        assert_eq!(FilePicker::open(PickerField::Save).dir, PathBuf::from("save"));
    }
}
