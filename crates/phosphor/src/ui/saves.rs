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

/// A menu button holding `named_saves` (Venues, Control Surfaces, Mappings…): returns what was picked.
pub(crate) fn saves_menu(ui: &mut egui::Ui, title: &str, about: &str, store: &mut PresetStore, name: &mut String, what: &str, current: impl FnOnce() -> String) -> Option<SaveAction> {
    ui.menu_button(title, |ui| {
        ui.set_min_width(300.0);
        hint(ui, about);
        named_saves(ui, store, name, what, current)
    })
    .inner
    .flatten()
}

/// Name field + Save, then the built-in and your saved entries with Load and (yours) Delete.
/// `current` makes the JSON to save; `what` names the kind in messages ("look", "venue").
pub(crate) fn named_saves(ui: &mut egui::Ui, store: &mut PresetStore, name: &mut String, what: &str, current: impl FnOnce() -> String) -> Option<SaveAction> {
    store.ensure_loaded();
    let mut action = None;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(name).hint_text(format!("Name this {what}")).desired_width(180.0));
        if crate::ui::command(ui, label(icon::FLOPPY_DISK, &format!("Save {what}"))).clicked() {
            let json = current();
            if json.is_empty() {
                action = Some(SaveAction::Error(format!("Nothing to save: select a {what} first")));
            } else if let Err(e) = store.save(name, &json) {
                action = Some(SaveAction::Error(e));
            } else {
                name.clear();
            }
        }
    });
    let builtins: Vec<_> = store.builtins().collect();
    if !builtins.is_empty() {
        crate::ui::heading(ui, "Built in");
        for p in builtins {
            ui.horizontal(|ui| {
                if crate::ui::command(ui, "Load").clicked() {
                    action = Some(SaveAction::Load(p.json.clone()));
                }
                ui.label(&p.name);
            });
        }
        ui.add_space(6.0);
        crate::ui::heading(ui, "Yours");
    }
    if store.user.is_empty() {
        crate::ui::empty_note(ui, &format!("Nothing saved yet. Name the current {what} above and save it."));
    }
    let mut delete = None;
    for p in &store.user {
        ui.horizontal(|ui| {
            if crate::ui::command(ui, "Load").clicked() {
                action = Some(SaveAction::Load(p.json.clone()));
            }
            ui.label(&p.name);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if crate::ui::small_command(ui, icon::TRASH).on_hover_text(format!("Delete this {what}")).clicked() {
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
