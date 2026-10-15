//! Presets panel.

use super::*;

pub(crate) fn presets(ui: &mut egui::Ui, st: &mut State) {
    st.presets.ensure_loaded();
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(&mut st.preset_name).hint_text("Preset name").desired_width(180.0));
        if ui.button(label(icon::FLOPPY_DISK, "Save current look")).clicked() {
            let json = st.comp.to_json();
            match st.presets.save(&st.preset_name, &json) {
                Ok(()) => {
                    st.preset_name.clear();
                    st.notice = None;
                }
                Err(e) => st.notice = Some(e),
            }
        }
    });
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg);
    }
    ui.separator();
    let mut load: Option<String> = None;
    let mut delete: Option<String> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.label(RichText::new("Built in").strong());
        for p in crate::presets::PresetStore::builtin() {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    load = Some(p.json.clone());
                }
                ui.label(&p.name);
            });
        }
        ui.add_space(8.0);
        ui.label(RichText::new("Yours").strong());
        if st.presets.user.is_empty() {
            hint(ui, "Nothing saved yet. Name the current look above and save it.");
        }
        for p in &st.presets.user {
            ui.horizontal(|ui| {
                if ui.button("Load").clicked() {
                    load = Some(p.json.clone());
                }
                ui.label(&p.name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(icon::TRASH).on_hover_text("Delete preset").clicked() {
                        delete = Some(p.name.clone());
                    }
                });
            });
        }
    });
    if let Some(json) = load {
        match phosphor_engine::Composition::from_json(&json) {
            Ok(comp) => st.load_composition(comp),
            Err(e) => st.notice = Some(format!("Couldn't load preset: {e}")),
        }
    }
    if let Some(name) = delete {
        st.presets.delete(&name);
    }
}
