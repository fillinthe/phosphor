//! Presets panel: whole looks, single layers and effect chains you save and recall (one shared list
//! widget, `ui::named_saves`, per kind).

use super::*;

pub(crate) fn presets(ui: &mut egui::Ui, st: &mut State) {
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg);
    }
    let target = st.selected.unwrap_or(MASTER_ID);
    let layer = st.comp.layers.iter().find(|l| l.id == target).cloned();
    let chain = st.comp.effects(target).cloned().unwrap_or_default();
    let (mut look, mut layer_pick, mut chain_pick) = (None, None, None);
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        let State { presets, preset_name, layer_presets, layer_preset_name, chain_presets, chain_preset_name, comp, .. } = &mut *st;
        crate::ui::section(ui, "presets_Looks", "Looks", true, |ui| {
            hint(ui, "The whole project: layers, effects, mapping.");
            crate::ui::toolbar(ui, |ui| {
                ui.label("Fade in");
                guarded!(ui, comp.launch.look_fade, egui::DragValue::new(&mut comp.launch.look_fade).range(0.0..=30.0).speed(0.05).suffix(" s").max_decimals(1))
                    .on_hover_text("A look fades in from the picture showing now (0 = cut)");
            });
            look = named_saves(ui, presets, preset_name, "look", || comp.to_json());
        });
        crate::ui::section(ui, "presets_Layers", "Layers", true, |ui| {
            match &layer {
                Some(l) => help(ui, &format!("Saves the selected layer ({}). Load adds it above the selected one.", l.name)),
                None => help(ui, "Select a layer to save it. Load adds a saved one to this project."),
            };
            let json = layer.as_ref().and_then(|l| serde_json::to_string(l).ok()).unwrap_or_default();
            layer_pick = named_saves(ui, layer_presets, layer_preset_name, "layer", || json);
        });
        crate::ui::section(ui, "presets_Effect chains", "Effect chains", true, |ui| {
            help(ui, "Saves the selected layer's (or MASTER's) effects. Load adds them to the selected one.");
            let json = serde_json::to_string(&chain).unwrap_or_default();
            chain_pick = named_saves(ui, chain_presets, chain_preset_name, "chain", || json);
        });
    });
    match look {
        Some(SaveAction::Load(json)) => match phosphor_engine::Composition::from_json(&json) {
            Ok(mut comp) => {
                // The fade time stays this project's (not the saved look's).
                let secs = st.comp.launch.look_fade;
                comp.launch.look_fade = secs;
                if let Some(e) = st.engine.as_mut() {
                    e.fade_in_next(secs);
                }
                st.load_composition(comp);
            }
            Err(e) => st.notice = Some(format!("Couldn't load preset: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
    match layer_pick {
        Some(SaveAction::Load(json)) => match serde_json::from_str::<LayerState>(&json) {
            Ok(l) => {
                let mut l = st.comp.adopt_layer(l);
                // Into the comp on screen, out of any group it was saved in.
                l.comp = st.comp.open_comp();
                l.parent = None;
                let at = st.comp.layers.iter().position(|x| x.id == target).map_or(st.comp.layers.len(), |i| i + 1);
                st.selected = Some(l.id);
                st.comp.layers.insert(at, l);
                st.comp.normalize_groups();
            }
            Err(e) => st.notice = Some(format!("Couldn't load the layer: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
    match chain_pick {
        Some(SaveAction::Load(json)) => match serde_json::from_str::<Vec<EffectState>>(&json) {
            Ok(fx) => append_effects(st, target, fx),
            Err(e) => st.notice = Some(format!("Couldn't load the chain: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
}
