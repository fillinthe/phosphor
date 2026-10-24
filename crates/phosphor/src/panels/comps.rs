//! Comps in the UI (`phosphor_engine::Comp`): the tabs over Clips / Layers / Mixer that pick which comp they show
//! (add, rename, reorder, delete from there), the Inspector for a selected comp, and "Move to comp" for a layer.

use phosphor_engine::{Comp, Composition};

use super::*;

enum CompAct {
    Rename(u64, String),
    Select(u64),
    Move(u64, bool),
    Delete(u64),
}

/// The comp tabs: click one to show its layers here; right-click for its settings; + Comp makes another canvas.
pub(crate) fn comp_tabs(ui: &mut egui::Ui, st: &mut State) {
    let add_tip = "Another canvas with its own layers, size and effects, mixed into the master (or sent to outputs of its own)";
    crate::ui::toolbar(ui, |ui| {
        let mut act = None;
        if let Some(mut open) = st.comp.open_comp() {
            let options: Vec<(u64, String)> = st.comp.comps.iter().map(|c| (c.id, format!("{} {}", icon::FRAME_CORNERS, c.name))).collect();
            let many = options.len() > 1;
            segmented(ui, &mut open, &options, |_, r, id| {
                let name = st.comp.comp(*id).map(|c| c.name.clone()).unwrap_or_default();
                r.right_menu(|ui| {
                    ui.menu_button(label(icon::PENCIL_SIMPLE, "Rename"), |ui| {
                        if let Some(n) = rename_menu(ui, ("comp_rename", *id), &name) {
                            act = Some(CompAct::Rename(*id, n));
                            ui.close();
                        }
                    });
                    if crate::ui::command(ui, label(icon::SLIDERS_HORIZONTAL, "Settings (size, opacity, effects)")).clicked() {
                        act = Some(CompAct::Select(*id));
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::MONITOR, "Preview in a window")).clicked() {
                        request_preview(ui, MonitorSource::Group(*id));
                        ui.close();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui::command(ui, label(icon::CAMERA, "Save a picture…")).on_hover_text("This comp's picture now, as a PNG").clicked() {
                        super::request_snapshot(ui, *id);
                        ui.close();
                    }
                    if many {
                        ui.separator();
                        if crate::ui::command(ui, label(icon::ARROW_LEFT, "Earlier (further down in the master)")).clicked() {
                            act = Some(CompAct::Move(*id, false));
                            ui.close();
                        }
                        if crate::ui::command(ui, label(icon::ARROW_RIGHT, "Later (on top in the master)")).clicked() {
                            act = Some(CompAct::Move(*id, true));
                            ui.close();
                        }
                        ui.separator();
                        if danger_button(ui, label(icon::TRASH, "Delete comp (its layers move to the first)")).clicked() {
                            act = Some(CompAct::Delete(*id));
                            ui.close();
                        }
                    }
                });
                r.clone().on_hover_text("Its layers show here · right-click: rename, settings, order");
            });
            st.comp.open_comp = Some(open);
        }
        if crate::ui::small_command(ui, label(icon::PLUS, "Comp")).on_hover_text(add_tip).clicked() {
            let n = st.comp.comps.len().max(1) + 1;
            let id = st.comp.add_comp(&format!("Comp {n}"));
            st.selected = Some(id);
        }
        match act {
            Some(CompAct::Rename(id, n)) => {
                if let Some(c) = st.comp.comp_mut(id) {
                    c.name = n;
                }
            }
            Some(CompAct::Select(id)) => st.selected = Some(id),
            Some(CompAct::Move(id, later)) => st.comp.move_comp(id, later),
            Some(CompAct::Delete(id)) => {
                st.comp.remove_comp(id);
                if st.selected == Some(id) {
                    st.selected = None;
                }
            }
            None => {}
        }
    });
}

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
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
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
        hint(ui, "A comp is a canvas of its own: its layers mix at its size, then its effects run, then it mixes into the master like a layer. The screen button sends it to outputs of its own.");
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
