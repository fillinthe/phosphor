//! Small shared pieces every panel uses the same way, so a field looks like a field and a rename works the same
//! everywhere. (Sliders: `Level`; buttons: `label` + `ui.button`; help text: `hint`; pick-one: `choice_*`.)

use super::*;

/// A one-line text field: `hint` shows while it's empty; `width` None = the room there is.
pub(crate) fn text_field(ui: &mut egui::Ui, text: &mut String, hint: &str, width: Option<f32>) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width.unwrap_or_else(|| ui.available_width())))
}

/// "Shows on": which outputs show this layer / group on their own (a slice each). `outs` = (output id, name,
/// showing now). Returns (output, on) when one is switched.
pub(crate) fn outputs_menu(ui: &mut egui::Ui, outs: &[(u64, String, bool)]) -> Option<(u64, bool)> {
    let n = outs.iter().filter(|o| o.2).count();
    let text = if n == 0 { icon::MONITOR.to_string() } else { format!("{} {n}", icon::MONITOR) };
    let mut out = None;
    ui.menu_button(text, |ui| {
        ui.label(RichText::new("Shows on").strong());
        hint(ui, "An output of its own, besides the mix (a slice there: shape it in the Outputs area)");
        for (id, name, on) in outs {
            let mut v = *on;
            if ui.checkbox(&mut v, name).changed() {
                out = Some((*id, v));
            }
        }
        if outs.len() <= 1 {
            hint(ui, "More outputs: Outputs area › Add output");
        }
    })
    .response
    .on_hover_text(if n == 0 { "Send this to an output of its own".to_string() } else { format!("On {n} output(s) of its own") });
    out
}

/// The body of a "Rename" menu: a field with the current name, OK (or Enter). Returns the new name when confirmed
/// (trimmed, never empty). `id` keeps what's typed while the menu is open.
pub(crate) fn rename_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &str) -> Option<String> {
    name_menu(ui, id, current, "OK", "")
}

/// The body of a "New …" menu: an empty field with `hint`, a `confirm` button (or Enter). Returns the name typed.
pub(crate) fn new_name_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, hint: &str, confirm: &str) -> Option<String> {
    name_menu(ui, id, "", confirm, hint)
}

fn name_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, initial: &str, confirm: &str, hint: &str) -> Option<String> {
    let key = egui::Id::new(("name_menu", id));
    let mut text: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_else(|| initial.to_string());
    let mut out = None;
    ui.horizontal(|ui| {
        let field = text_field(ui, &mut text, hint, Some(170.0));
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = ui.add_enabled(!text.trim().is_empty(), egui::Button::new(confirm)).clicked();
        if (enter || ok) && !text.trim().is_empty() {
            out = Some(text.trim().to_string());
            text = initial.to_string();
            ui.close();
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, text));
    out
}
