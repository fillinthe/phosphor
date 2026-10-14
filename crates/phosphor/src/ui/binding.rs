//! Editors for what drives what: pick a target, type an input (MIDI/OSC/key), set a value range,
//! and edit a list of bindings. Used by the Mappings editor, the "Map to…" menu and the Control
//! Surface.

use super::*;
use phosphor_engine::{Binding, ControlSource, ValueRange};

/// A searchable list of everything that can be controlled. `targets` comes from
/// `control::all_targets` (compute it once per frame, not per row). Returns the new pick.
pub(crate) fn target_picker(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, targets: &[(ControlTarget, String)], current: Option<&ControlTarget>, width: f32) -> Option<ControlTarget> {
    let id = egui::Id::new(("target_picker", id_salt));
    let text = current.and_then(|c| targets.iter().find(|(t, _)| t == c)).map(|(_, n)| n.clone()).unwrap_or_else(|| {
        if current.is_some() { "(removed)".into() } else { "Pick what to control…".into() }
    });
    let mut picked = None;
    ComboBox::from_id_salt(id).width(width).selected_text(text).height(320.0).show_ui(ui, |ui| {
        let mut query: String = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
        ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search…").desired_width(width.max(200.0)))
            .request_focus();
        let q = query.to_lowercase();
        for (t, name) in targets.iter().filter(|(_, n)| q.is_empty() || n.to_lowercase().contains(&q)) {
            if ui.selectable_label(current == Some(t), name).clicked() {
                picked = Some(t.clone());
            }
        }
        ui.ctx().data_mut(|d| d.insert_temp(id, query));
    });
    picked
}

const SOURCE_KINDS: [&str; 4] = ["MIDI CC", "MIDI Note", "OSC", "Key"];

fn source_kind(s: &ControlSource) -> usize {
    match s {
        ControlSource::MidiCc { .. } => 0,
        ControlSource::MidiNote { .. } => 1,
        ControlSource::Osc { .. } => 2,
        ControlSource::Key { .. } => 3,
    }
}

/// Type in an input by hand: MIDI CC / note (channel 1–16, number 0–127), an OSC address or a key.
/// Returns true when it changed.
pub(crate) fn source_editor(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug, source: &mut ControlSource) -> bool {
    let before = source.clone();
    let id = egui::Id::new(("source_editor", id_salt));
    let mut kind = source_kind(source);
    ComboBox::from_id_salt(id).width(82.0).selected_text(SOURCE_KINDS[kind]).show_ui(ui, |ui| {
        for (i, k) in SOURCE_KINDS.iter().enumerate() {
            ui.selectable_value(&mut kind, i, *k);
        }
    });
    let (ch, num) = match source {
        ControlSource::MidiCc { channel, cc } => (*channel, *cc),
        ControlSource::MidiNote { channel, note } => (*channel, *note),
        _ => (0, 0),
    };
    if kind != source_kind(source) {
        *source = match kind {
            0 => ControlSource::MidiCc { channel: ch, cc: num },
            1 => ControlSource::MidiNote { channel: ch, note: num },
            2 => ControlSource::Osc { address: "/".into() },
            _ => ControlSource::Key { key: "Q".into() },
        };
    }
    match source {
        ControlSource::MidiCc { channel, cc: n } | ControlSource::MidiNote { channel, note: n } => {
            let mut c = *channel + 1;
            guarded!(ui, c, egui::DragValue::new(&mut c).range(1..=16).prefix("ch "));
            *channel = c - 1;
            guarded!(ui, *n, egui::DragValue::new(n).range(0..=127));
        }
        ControlSource::Osc { address } => {
            ui.add(egui::TextEdit::singleline(address).hint_text("/touchosc/fader1").desired_width(150.0));
        }
        ControlSource::Key { key } => {
            if ui.add(egui::TextEdit::singleline(key).char_limit(12).desired_width(50.0)).changed() {
                *key = key.to_uppercase();
            }
        }
    }
    *source != before
}

/// Min / max / invert for how a 0..1 value lands on its target.
pub(crate) fn range_editor(ui: &mut egui::Ui, range: &mut ValueRange) {
    guarded!(ui, range.min, egui::DragValue::new(&mut range.min).range(0.0..=1.0).speed(0.005).fixed_decimals(2).prefix("from "));
    guarded!(ui, range.max, egui::DragValue::new(&mut range.max).range(0.0..=1.0).speed(0.005).fixed_decimals(2).prefix("to "));
    ui.toggle_value(&mut range.invert, icon::ARROWS_DOWN_UP).on_hover_text("Invert (up = down)");
}

/// The targets one control drives, each with its own range: pick, adjust, remove, add more.
pub(crate) fn binding_list(ui: &mut egui::Ui, id_salt: impl std::hash::Hash + std::fmt::Debug + Copy, targets: &[(ControlTarget, String)], bindings: &mut Vec<Binding>) {
    let mut remove = None;
    for (i, b) in bindings.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            if let Some(t) = target_picker(ui, (id_salt, i), targets, Some(&b.target), 200.0) {
                b.target = t;
            }
            range_editor(ui, &mut b.range);
            if ui.small_button(icon::TRASH).on_hover_text("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        bindings.remove(i);
    }
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::PLUS).color(ACCENT));
        if let Some(t) = target_picker(ui, (id_salt, "add"), targets, None, 200.0) {
            bindings.push(Binding { target: t, range: ValueRange::default() });
        }
    });
}
