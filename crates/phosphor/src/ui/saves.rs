//! Save the current thing under a name, and pick a saved one to load or delete (looks, venues…).

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use super::widgets::{hint, label};
use crate::presets::PresetStore;

/// What the user asked for this frame.
pub(crate) enum SaveAction {
    /// Load this saved JSON.
    Load(String),
    /// Something went wrong (shown by the caller).
    Error(String),
}

/// Name field + Save, then the built-in and your saved entries with Load and (yours) Delete.
/// `current` makes the JSON to save; `what` names the kind in messages ("look", "venue").
pub(crate) fn named_saves(ui: &mut egui::Ui, store: &mut PresetStore, name: &mut String, what: &str, current: impl FnOnce() -> String) -> Option<SaveAction> {
    store.ensure_loaded();
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(name).hint_text(format!("Name this {what}")).desired_width(180.0));
        if ui.button(label(icon::FLOPPY_DISK, &format!("Save {what}"))).clicked() {
            match store.save(name, &current()) {
                Ok(()) => name.clear(),
                Err(e) => action = Some(SaveAction::Error(e)),
            }
        }
    });
    let builtins: Vec<_> = store.builtins().collect();
    if !builtins.is_empty() {
        ui.label(RichText::new("Built in").strong());
        for p in builtins {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    action = Some(SaveAction::Load(p.json.clone()));
                }
                ui.label(&p.name);
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Yours").strong());
    }
    if store.user.is_empty() {
        hint(ui, format!("Nothing saved yet. Name the current {what} above and save it."));
    }
    let mut delete = None;
    for p in &store.user {
        ui.horizontal(|ui| {
            if ui.button("Load").clicked() {
                action = Some(SaveAction::Load(p.json.clone()));
            }
            ui.label(&p.name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(icon::TRASH).on_hover_text(format!("Delete this {what}")).clicked() {
                    delete = Some(p.name.clone());
                }
            });
        });
    }
    if let Some(n) = delete {
        store.delete(&n);
    }
    action
}
