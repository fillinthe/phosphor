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
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let State { presets, preset_name, layer_presets, layer_preset_name, chain_presets, chain_preset_name, comp, .. } = &mut *st;
        egui::CollapsingHeader::new(RichText::new("Looks").strong()).default_open(true).show(ui, |ui| {
            hint(ui, "The whole project: layers, effects, mapping.");
            look = named_saves(ui, presets, preset_name, "look", || comp.to_json());
        });
        egui::CollapsingHeader::new(RichText::new("Layers").strong()).default_open(true).show(ui, |ui| {
            match &layer {
                Some(l) => hint(ui, format!("Saves the selected layer ({}). Load adds it above the selected one.", l.name)),
                None => hint(ui, "Select a layer to save it. Load adds a saved one to this project."),
            };
            let json = layer.as_ref().and_then(|l| serde_json::to_string(l).ok()).unwrap_or_default();
            layer_pick = named_saves(ui, layer_presets, layer_preset_name, "layer", || json);
        });
        egui::CollapsingHeader::new(RichText::new("Effect chains").strong()).default_open(true).show(ui, |ui| {
            hint(ui, "Saves the selected layer's (or MASTER's) effects. Load adds them to the selected one.");
            let json = serde_json::to_string(&chain).unwrap_or_default();
            chain_pick = named_saves(ui, chain_presets, chain_preset_name, "chain", || json);
        });
    });
    match look {
        Some(SaveAction::Load(json)) => match phosphor_engine::Composition::from_json(&json) {
            Ok(comp) => st.load_composition(comp),
            Err(e) => st.notice = Some(format!("Couldn't load preset: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
    match layer_pick {
        Some(SaveAction::Load(json)) => match serde_json::from_str::<LayerState>(&json) {
            Ok(l) => {
                let l = st.comp.adopt_layer(l);
                let at = st.comp.layers.iter().position(|x| x.id == target).map_or(st.comp.layers.len(), |i| i + 1);
                st.selected = Some(l.id);
                st.comp.layers.insert(at, l);
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
