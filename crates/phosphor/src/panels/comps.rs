//! Comps in the UI (`phosphor_engine::Comp`): the Inspector for a selected comp and "Move to comp" for a layer. Comps
//! show as bands in Layers / Clips (and strips in the Mixer), not as tabs; new layers go to the selected one's comp
//! (`follow_selection_comp`).

use phosphor_engine::{Comp, Composition};

use super::*;

/// The comps a layer of the open comp could move to (empty with one comp).
pub(crate) fn other_comps(comp: &Composition) -> Vec<(u64, String)> {
    let open = comp.open_comp();
    comp.comps.iter().filter(|c| Some(c.id) != open).map(|c| (c.id, c.name.clone())).collect()
}

/// "Move to comp" for a layer's menu (`others` from `other_comps`; nothing with one comp). Returns the comp picked.
pub(crate) fn move_to_comp_menu(ui: &mut egui::Ui, others: &[(u64, String)]) -> Option<u64> {
    if others.is_empty() {
        return None;
    }
    let mut picked = None;
    ui.menu_button(label(icon::FRAME_CORNERS, "Move to comp"), |ui| {
        for (id, name) in others {
            if crate::ui::command(ui, name).clicked() {
                picked = Some(*id);
                ui.close();
            }
        }
    });
    picked
}

/// Inspector for a selected comp: on/off, outputs it shows on, name, size and fit, opacity, blend, master mix and
/// its effects. Returns an effect-chain action and an output switched on/off.
pub(crate) fn comp_inspector(ui: &mut egui::Ui, c: &mut Comp, thumb: Option<egui::TextureId>, master: (u32, u32), clipboard_len: usize, outs: &[(u64, String, bool)]) -> (Option<FxAction>, Option<(u64, bool)>) {
    let mut fx_action = None;
    let mut send = None;
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
        let (cw, ch) = c.size(master);
        let w = ui.available_width().min(360.0);
        thumbnail(ui, thumb, egui::vec2(w, w * ch as f32 / cw.max(1) as f32), c.enabled);
        ui.horizontal(|ui| {
            let on = toggle(ui, &mut c.enabled, "ON");
            mapped_badge(ui, &on, &ControlTarget::LayerEnabled(c.id));
            on.right_menu(|ui| learn_items(ui, &ControlTarget::LayerEnabled(c.id)));
            ui.label(icon::FRAME_CORNERS);
            send = outputs_menu(ui, outs).or(send);
            ui.add(egui::TextEdit::singleline(&mut c.name).desired_width(f32::INFINITY));
        });
        help(ui, "A comp is a canvas of its own: its layers mix at its size, then its effects run, then it mixes into the master like a layer. The screen button sends it to outputs of its own.");
        crate::ui::toolbar(ui, |ui| {
            let mut same = c.width == 0;
            if toggle(ui, &mut same, "Master size").on_hover_text(format!("{}×{}", master.0, master.1)).changed() {
                (c.width, c.height) = if same { (0, 0) } else { master };
            }
            if !same {
                ui.add(egui::DragValue::new(&mut c.width).range(16..=8192).suffix(" w"));
                ui.label("×");
                ui.add(egui::DragValue::new(&mut c.height).range(16..=8192).suffix(" h"));
            }
        });
        if c.width != 0 {
            crate::ui::toolbar(ui, |ui| {
                ui.label("In the master");
                choice_buttons(ui, &mut c.fit, |_, _| {});
            });
        }
        ui.horizontal(|ui| {
            Level::percent(&mut c.opacity, "Opacity").modulation(&mut c.opacity_mod).target(ControlTarget::LayerOpacity(c.id)).show(ui);
        });
        crate::ui::toolbar(ui, |ui| {
            ui.label("Blend");
            choice_combo(ui, ("comp_blend", c.id), &mut c.blend, 120.0, false);
            if let Some(r) = fx_badge(ui, &c.effects, &mut c.fx_bypass) {
                map_menu(ui, &r, ControlTarget::LayerFxBypass(c.id));
            }
        });
        toggle(ui, &mut c.in_mix, "In the master mix").on_hover_text("Off: the comp shows only on outputs, slices and Feed sources that pick it");
        ui.separator();
        fx_action = effects_editor(ui, &mut c.effects, clipboard_len);
    });
    (fx_action, send)
}

/// New layers go into the comp of what's selected (a layer, a group, the comp itself): with every comp in one list
/// there are no tabs to say which is "open". Cheap: only when the selection moves to another comp.
pub(crate) fn follow_selection_comp(comp: &mut Composition, selected: Option<u64>) {
    let Some(sel) = selected else { return };
    let target = if comp.comp(sel).is_some() {
        Some(sel)
    } else if let Some(l) = comp.layers.iter().find(|l| l.id == sel) {
        comp.comp_of(l)
    } else if let Some(l) = comp.layers.iter().find(|l| l.parent == Some(sel)) {
        comp.comp_of(l)
    } else {
        None
    };
    if target.is_some() && target != comp.open_comp {
        comp.open_comp = target;
    }
}

#[cfg(test)]
mod tests {
    use phosphor_engine::Composition;

    /// Selecting a layer in Comp 2 makes new layers go to Comp 2 (no tabs to pick it with any more).
    #[test]
    fn new_layers_go_to_the_selected_layers_comp() {
        let mut c = Composition::default();
        let a = c.add_layer().id;
        let side = c.add_comp("Side");
        let b = c.add_layer().id;
        let main = c.comps[0].id;
        assert_eq!(c.layer(b).and_then(|l| l.comp), Some(side), "a new comp opens");
        super::follow_selection_comp(&mut c, Some(a));
        assert_eq!(c.open_comp, Some(main));
        let n = c.add_layer().id;
        assert_eq!(c.comp_of(c.layer(n).unwrap()), Some(main), "the new layer lands with the selected one");
        super::follow_selection_comp(&mut c, Some(side));
        assert_eq!(c.open_comp, Some(side), "selecting the comp itself");
    }
}
