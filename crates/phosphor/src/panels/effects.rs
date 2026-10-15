//! Effect stacks: editor, reorder, add/remove.

use super::*;

pub enum FxAction {
    /// Copy the effect at this index right after itself (needs a fresh id from the composition).
    Duplicate(usize),
    Browse(Option<egui::Pos2>),
    CopyChain,
    PasteChain,
}

/// Drag payload for reordering effects (distinct from the layer payload).
#[derive(Clone, Copy)]
pub(crate) struct FxDrag(u64);

/// An effect chain editor: collapsible cards, drag to reorder, copy/paste whole chains.
pub(crate) fn effects_editor(ui: &mut egui::Ui, effects: &mut Vec<EffectState>, clipboard_len: usize) -> Option<FxAction> {
    let mut action: Option<FxAction> = None;
    ui.horizontal(|ui| {
        ui.label(RichText::new("Effects").strong());
        let add = ui.button(label(icon::PLUS, "Add effect")).on_hover_text("Browse effects with previews");
        if add.clicked() {
            action = Some(FxAction::Browse(Some(add.rect.left_bottom())));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.add_enabled(clipboard_len > 0, egui::Button::new(label(icon::CLIPBOARD_TEXT, "Paste"))).on_hover_text("Append the copied chain").clicked() {
                action = Some(FxAction::PasteChain);
            }
            if ui.add_enabled(!effects.is_empty(), egui::Button::new(label(icon::COPY, "Copy"))).on_hover_text("Copy this whole chain").clicked() {
                action = Some(FxAction::CopyChain);
            }
        });
    });
    if effects.is_empty() {
        hint(ui, "No effects yet. Effects run top to bottom; drag the handle to reorder.");
    }
    let mut remove = None;
    let mut swap = None;
    let mut duplicate: Option<usize> = None;
    let mut drop: Option<(u64, u64)> = None;
    let count = effects.len();
    for (i, fx) in effects.iter_mut().enumerate() {
        let Some(desc) = shaders::effect(&fx.effect) else { continue };
        let fill = if fx.enabled { ui.visuals().faint_bg_color } else { ui.visuals().panel_fill };
        let frame = egui::Frame::group(ui.style()).fill(fill).corner_radius(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let id = ui.make_persistent_id(("fx", fx.id));
            let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, true);
            let header = state.show_header(ui, |ui| {
                let handle = ui.add(egui::Label::new(RichText::new(icon::DOTS_SIX_VERTICAL).color(Color32::GRAY)).sense(Sense::drag()));
                handle.dnd_set_drag_payload(FxDrag(fx.id));
                handle.on_hover_text("Drag to reorder");
                let on = ui.checkbox(&mut fx.enabled, RichText::new(desc.name).strong());
                mapped_badge(ui, &on, &ControlTarget::EffectEnabled(fx.id));
                on.right_menu(|ui| {
                    learn_items(ui, &ControlTarget::EffectEnabled(fx.id));
                    ui.separator();
                    if ui.button(label(icon::ARROW_COUNTER_CLOCKWISE, "Reset all controls")).clicked() {
                        fx.values = desc.defaults();
                        fx.mods.clear();
                        fx.mix = 1.0;
                        ui.close();
                    }
                    if ui.button(label(icon::COPY, "Duplicate")).clicked() {
                        duplicate = Some(i);
                        ui.close();
                    }
                    if ui.button(label(icon::ARROW_UP, "Earlier in the chain")).clicked() && i > 0 {
                        swap = Some((i, i - 1));
                        ui.close();
                    }
                    if ui.button(label(icon::ARROW_DOWN, "Later in the chain")).clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                        ui.close();
                    }
                    if ui.button(label(icon::TRASH, "Remove")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
                if fx.mix < 0.999 {
                    hint(ui, format!("{:.0}%", fx.mix * 100.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button(icon::TRASH).on_hover_text("Remove effect").clicked() {
                        remove = Some(i);
                    }
                    if ui.small_button(icon::ARROW_DOWN).on_hover_text("Later in the chain").clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                    }
                    if ui.small_button(icon::ARROW_UP).on_hover_text("Earlier in the chain").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                });
            });
            header.body(|ui| {
                for (pi, p) in desc.params.iter().enumerate() {
                    param_slider(ui, &mut fx.values, &mut fx.mods, pi, p, Some(ControlTarget::EffectParam { effect: fx.id, index: pi }));
                }
                let mix = guarded!(ui, fx.mix, Slider::new(&mut fx.mix, 0.0..=1.0).text("Mix"));
                mapped_badge(ui, &mix, &ControlTarget::EffectMix(fx.id));
                if control_menu(&mix, Some(&ControlTarget::EffectMix(fx.id)), None) {
                    fx.mix = 1.0;
                }
            });
        });
        if let Some(p) = frame.response.dnd_release_payload::<FxDrag>() {
            drop = Some((p.0, fx.id));
        }
        if frame.response.dnd_hover_payload::<FxDrag>().is_some_and(|p| p.0 != fx.id) {
            ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
        }
    }
    if let Some((from, onto)) = drop
        && from != onto
        && let Some(a) = effects.iter().position(|e| e.id == from)
        && let Some(b) = effects.iter().position(|e| e.id == onto)
    {
        let moved = effects.remove(a);
        effects.insert(b, moved);
    }
    if let Some((a, b)) = swap {
        effects.swap(a, b);
    }
    if let Some(i) = remove {
        effects.remove(i);
    }
    if let Some(i) = duplicate {
        action = Some(FxAction::Duplicate(i));
    }
    action
}

pub(crate) fn apply_fx_action(st: &mut State, target: u64, action: FxAction) {
    match action {
        FxAction::Browse(pos) => st.fx_browser = Some(crate::explorer::FxBrowser::new(target, false, pos)),
        FxAction::Duplicate(i) => {
            let id = st.comp.new_id();
            let chain = if target == MASTER_ID { Some(&mut st.comp.master_effects) } else { st.comp.layer_mut(target).map(|l| &mut l.effects) };
            if let Some(chain) = chain
                && let Some(fx) = chain.get(i).cloned()
            {
                chain.insert(i + 1, phosphor_engine::EffectState { id, ..fx });
            }
        }
        FxAction::CopyChain => {
            let chain = if target == MASTER_ID {
                st.comp.master_effects.clone()
            } else {
                st.comp.layers.iter().find(|l| l.id == target).map(|l| l.effects.clone()).unwrap_or_default()
            };
            st.fx_clipboard = chain;
        }
        FxAction::PasteChain => append_effects(st, target, st.fx_clipboard.clone()),
    }
}

/// Appends effects (copied or saved) to a layer's chain or MASTER, with fresh ids.
pub fn append_effects(st: &mut State, target: u64, effects: Vec<phosphor_engine::EffectState>) {
    let effects = st.comp.adopt_effects(effects);
    if target == MASTER_ID {
        st.comp.master_effects.extend(effects);
    } else if let Some(l) = st.comp.layer_mut(target) {
        l.effects.extend(effects);
    }
}

pub fn add_effect_to(st: &mut State, target: u64, effect: &str) {
    let Some(mut fx) = st.comp.new_effect(effect) else { return };
    // With Launch → Fade, a new effect blends in instead of popping on.
    if st.comp.launch.fade {
        st.fx_fades.push((fx.id, st.mod_inputs.time, st.comp.launch.fade_secs.max(0.05), fx.mix));
        fx.mix = 0.0;
    }
    st.explorer.recent.retain(|r| r != effect);
    st.explorer.recent.insert(0, effect.to_string());
    st.explorer.recent.truncate(12);
    if target == MASTER_ID {
        st.comp.master_effects.push(fx);
    } else if let Some(l) = st.comp.layer_mut(target) {
        l.effects.push(fx);
    }
}
