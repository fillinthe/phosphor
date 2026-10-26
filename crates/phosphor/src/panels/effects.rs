//! Effect stacks: editor, reorder, add/remove.

use super::*;

pub enum FxAction {
    /// Copy the effect at this index right after itself (needs a fresh id from the composition).
    Duplicate(usize),
    Browse(Option<egui::Pos2>),
    CopyChain,
    PasteChain,
    /// An effect dragged in from the Effects panel.
    Add(&'static str),
}

/// Drag payload for reordering effects (distinct from the layer payload).
#[derive(Clone, Copy)]
pub(crate) struct FxDrag(u64);

/// An effect chain editor: collapsible cards, drag to reorder, copy/paste whole chains.
pub(crate) fn effects_editor(ui: &mut egui::Ui, effects: &mut Vec<EffectState>, clipboard_len: usize) -> Option<FxAction> {
    effects_list(ui, "Effects", "No effects yet. Effects run top to bottom; drag the handle to reorder.", effects, clipboard_len)
}

/// `effects_editor` with its own heading and words for when it's empty (a clip's or file's chain).
pub(crate) fn effects_list(ui: &mut egui::Ui, heading: &str, empty: &str, effects: &mut Vec<EffectState>, clipboard_len: usize) -> Option<FxAction> {
    let top = ui.cursor().min;
    let action = effects_list_body(ui, heading, empty, effects, clipboard_len);
    // An effect carried from the Effects panel drops anywhere on the list (it's added at the end).
    if !egui::DragAndDrop::has_payload_of_type::<crate::explorer::PaletteDrag>(ui.ctx()) {
        return action;
    }
    let area = egui::Rect::from_min_max(top, egui::pos2(ui.max_rect().right(), ui.cursor().min.y.max(top.y + 40.0)));
    let zone = ui.interact(area, ui.id().with(("fx_drop", heading)), Sense::hover());
    let carried = zone.dnd_hover_payload::<crate::explorer::PaletteDrag>().and_then(|p| p.effect);
    let p = ui.painter();
    if carried.is_some() {
        p.rect_filled(area, 6.0, accent().gamma_multiply(0.12));
        p.rect_stroke(area, 6.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
    } else {
        p.rect_stroke(area, 6.0, Stroke::new(1.0, accent().gamma_multiply(0.5)), StrokeKind::Outside);
    }
    match crate::ui::drop_payload::<crate::explorer::PaletteDrag>(&zone).and_then(|p| p.effect) {
        Some(id) => Some(FxAction::Add(id)),
        None => action,
    }
}

fn effects_list_body(ui: &mut egui::Ui, heading: &str, empty: &str, effects: &mut Vec<EffectState>, clipboard_len: usize) -> Option<FxAction> {
    let mut action: Option<FxAction> = None;
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, heading);
        let add = crate::ui::command(ui, label(icon::PLUS, "Add effect")).on_hover_text("Browse effects with previews");
        if add.clicked() {
            action = Some(FxAction::Browse(Some(add.rect.left_bottom())));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Icons only: the row has to fit narrow columns (names on hover).
            if crate::ui::command_if(ui, clipboard_len > 0, icon::CLIPBOARD_TEXT).on_hover_text("Paste: append the copied chain").clicked() {
                action = Some(FxAction::PasteChain);
            }
            if crate::ui::command_if(ui, !effects.is_empty(), icon::COPY).on_hover_text("Copy this whole chain").clicked() {
                action = Some(FxAction::CopyChain);
            }
        });
    });
    if effects.is_empty() {
        help(ui, empty);
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
                let locked = is_locked(ui, &ControlTarget::EffectEnabled(fx.id));
                let on = crate::ui::toggle_locked(ui, &mut fx.enabled, desc.name, locked);
                mapped_badge(ui, &on, &ControlTarget::EffectEnabled(fx.id));
                on.right_menu(|ui| {
                    learn_items(ui, &ControlTarget::EffectEnabled(fx.id));
                    ui.separator();
                    if crate::ui::command(ui, label(icon::ARROW_COUNTER_CLOCKWISE, "Reset all controls")).clicked() {
                        fx.values = desc.defaults();
                        fx.mods.clear();
                        fx.mix = 1.0;
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::COPY, "Duplicate")).clicked() {
                        duplicate = Some(i);
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::ARROW_UP, "Earlier in the chain")).clicked() && i > 0 {
                        swap = Some((i, i - 1));
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::ARROW_DOWN, "Later in the chain")).clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                        ui.close();
                    }
                    if crate::ui::command(ui, label(icon::TRASH, "Remove")).clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
                if fx.mix < 0.999 {
                    hint(ui, format!("{:.0}%", fx.mix * 100.0));
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Remove effect").clicked() {
                        remove = Some(i);
                    }
                    if crate::ui::small_command(ui, icon::ARROW_DOWN).on_hover_text("Later in the chain").clicked() && i + 1 < count {
                        swap = Some((i, i + 1));
                    }
                    if crate::ui::small_command(ui, icon::ARROW_UP).on_hover_text("Earlier in the chain").clicked() && i > 0 {
                        swap = Some((i, i - 1));
                    }
                    // Before / after: the picture without this one effect while the button is held.
                    let key = egui::Id::new(("fx_compare", fx.id));
                    let was: Option<bool> = ui.ctx().data(|d| d.get_temp(key));
                    if fx.enabled || was.is_some() {
                        let held = crate::ui::small_command(ui, icon::EYE_SLASH).on_hover_text("Hold to see the picture without this effect").is_pointer_button_down_on();
                        match (held, was) {
                            (true, None) => {
                                ui.ctx().data_mut(|d| d.insert_temp(key, fx.enabled));
                                fx.enabled = false;
                            }
                            (false, Some(on)) => {
                                fx.enabled = on;
                                ui.ctx().data_mut(|d| d.remove_temp::<bool>(key));
                            }
                            _ => {}
                        }
                    }
                });
            });
            header.body(|ui| {
                for (pi, p) in desc.params.iter().enumerate() {
                    param_slider(ui, &mut fx.values, &mut fx.mods, pi, p, Some(ControlTarget::EffectParam { effect: fx.id, index: pi }));
                }
                ui.horizontal(|ui| {
                    Level::percent(&mut fx.mix, "Mix").target(ControlTarget::EffectMix(fx.id)).width((ui.available_width() - 110.0).max(80.0)).tip("How much of this effect: 0% = bypassed").show(ui);
                    choice_combo(ui, ("fx_blend", fx.id), &mut fx.blend, 96.0, true).on_hover_text("How the effect's result lands on the picture it was given (Normal = a plain mix)");
                    // The masks are for layers (they cut what's below), not effects.
                    if fx.blend.is_mask() || fx.blend.is_wipe() {
                        fx.blend = phosphor_engine::BlendMode::Normal;
                    }
                });
            });
        });
        if let Some(p) = crate::ui::drop_payload::<FxDrag>(&frame.response) {
            drop = Some((p.0, fx.id));
        }
        if frame.response.dnd_hover_payload::<FxDrag>().is_some_and(|p| p.0 != fx.id) {
            ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, accent()), StrokeKind::Outside);
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
        let gone = effects.remove(i);
        log::info!(target: "phosphor::action", "effect removed: {} ({} left in that chain)", gone.effect, effects.len());
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
            if let Some(chain) = st.comp.effects_mut(target)
                && let Some(fx) = chain.get(i).cloned()
            {
                chain.insert(i + 1, phosphor_engine::EffectState { id, ..fx });
            }
        }
        FxAction::CopyChain => {
            st.fx_clipboard = st.comp.effects(target).cloned().unwrap_or_default();
        }
        FxAction::PasteChain => append_effects(st, target, st.fx_clipboard.clone()),
        FxAction::Add(id) => add_effect_to(st, target, id),
    }
}

/// Appends effects (copied or saved) to a layer's chain or MASTER, with fresh ids.
pub fn append_effects(st: &mut State, target: u64, effects: Vec<phosphor_engine::EffectState>) {
    let effects = st.comp.adopt_effects(effects);
    if let Some(chain) = st.comp.effects_mut(target) {
        chain.extend(effects);
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
    let on = if target == phosphor_engine::MASTER_ID { "MASTER".to_string() } else if let Some(l) = phosphor_engine::chain_layer(target).and_then(|l| st.comp.layer(l)) { format!("the clip on layer \"{}\"", l.name) } else { st.comp.layer(target).map(|l| format!("layer \"{}\"", l.name)).or_else(|| st.comp.group(target).map(|g| format!("group \"{}\"", g.name))).or_else(|| st.comp.comp(target).map(|c| format!("comp \"{}\"", c.name))).unwrap_or_else(|| "?".into()) };
    log::info!(target: "phosphor::action", "effect added: {effect} on {on}");
    if let Some(chain) = st.comp.effects_mut(target) {
        chain.push(fx);
    }
}
