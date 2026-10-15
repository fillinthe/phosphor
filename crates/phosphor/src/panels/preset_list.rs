//! Presets panel: whole looks you save and recall (shared list widget: `ui::named_saves`).

use super::*;

pub(crate) fn presets(ui: &mut egui::Ui, st: &mut State) {
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg);
    }
    let action = {
        let State { presets, preset_name, comp, .. } = st;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| named_saves(ui, presets, preset_name, "look", || comp.to_json())).inner
    };
    match action {
        Some(SaveAction::Load(json)) => match phosphor_engine::Composition::from_json(&json) {
            Ok(comp) => st.load_composition(comp),
            Err(e) => st.notice = Some(format!("Couldn't load preset: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
}
