//! Layer groups in the Layers panel (group card above its members) and the Inspector.

use phosphor_engine::LayerGroup;

use super::*;

pub(crate) enum GroupAction {
    Up,
    Down,
    Ungroup,
    /// Ungroup and delete the member layers too.
    Delete,
    BrowseEffects,
}

pub(crate) struct GroupOut {
    pub select: bool,
    pub action: Option<GroupAction>,
    /// A layer dragged onto the group card (joins the group).
    pub dropped: Option<u64>,
}

/// The card heading a group's members: fold arrow, on/off, name, opacity, blend, effects.
pub(crate) fn group_card(ui: &mut egui::Ui, g: &mut LayerGroup, thumb: Option<egui::TextureId>, aspect: f32, is_selected: bool, members: usize) -> GroupOut {
    let mut select = false;
    let mut action = None;
    let card_id = egui::Id::new(("group_card", g.id));
    let bg = card_background(ui, card_id);
    let frame = card_frame(ui, is_selected).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            let caret = if g.collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
            if ui.small_button(caret).on_hover_text(if g.collapsed { "Show the group's layers" } else { "Fold the group's layers away" }).clicked() {
                g.collapsed = !g.collapsed;
            }
            let r = thumbnail(ui, thumb, egui::vec2(80.0, 80.0 / aspect), g.enabled).on_hover_text("Click to select · double-click to toggle");
            if r.clicked() {
                select = true;
            }
            if r.double_clicked() {
                g.enabled = !g.enabled;
            }
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.toggle_value(&mut g.enabled, RichText::new("ON").small().strong()).on_hover_text("Group on/off");
                    let name = RichText::new(format!("{} {}", icon::STACK, g.name)).strong().size(12.5);
                    let width = (ui.available_width() - 3.0 * (ui.spacing().interact_size.y + ui.spacing().item_spacing.x)).max(24.0);
                    let r = ui.scope(|ui| {
                        ui.set_max_width(width);
                        ui.add(egui::Button::selectable(is_selected, name).truncate())
                    });
                    if r.inner.on_hover_text(format!("{members} layers mixed together, then treated like one layer")).clicked() {
                        select = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::ARROW_DOWN).on_hover_text("Move the group down").clicked() {
                            action = Some(GroupAction::Down);
                        }
                        if ui.small_button(icon::ARROW_UP).on_hover_text("Move the group up").clicked() {
                            action = Some(GroupAction::Up);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 100.0).max(60.0);
                    let r = guarded!(ui, g.opacity, Slider::new(&mut g.opacity, 0.0..=1.0).show_value(false)).on_hover_text("Group opacity");
                    value_bubble(ui, &r, format!("{:.0}%", g.opacity * 100.0));
                    percent_field(ui, &mut g.opacity);
                });
                ui.horizontal(|ui| {
                    choice_combo(ui, ("group_blend", g.id), &mut g.blend, 104.0, false);
                    fx_badge(ui, &g.effects, &mut g.fx_bypass);
                    if !g.in_mix {
                        hint(ui, "surfaces only");
                    }
                });
            });
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    let resp = bg.unwrap_or_else(|| frame.response.clone());
    if resp.clicked() {
        select = true;
    }
    resp.right_menu(|ui| {
        if ui.button(label(icon::MAGIC_WAND, "Add effect to the group…")).clicked() {
            action = Some(GroupAction::BrowseEffects);
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::ARROW_UP, "Move up")).clicked() {
            action = Some(GroupAction::Up);
            ui.close();
        }
        if ui.button(label(icon::ARROW_DOWN, "Move down")).clicked() {
            action = Some(GroupAction::Down);
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::SELECTION_SLASH, "Ungroup (keep the layers)")).clicked() {
            action = Some(GroupAction::Ungroup);
            ui.close();
        }
        if ui.button(label(icon::TRASH, "Delete group and its layers")).clicked() {
            action = Some(GroupAction::Delete);
            ui.close();
        }
    });
    let dropped = frame.response.dnd_release_payload::<u64>().map(|p| *p);
    if frame.response.dnd_hover_payload::<u64>().is_some() {
        ui.painter().rect_stroke(frame.response.rect, 6.0, Stroke::new(2.0, ACCENT), StrokeKind::Outside);
    }
    GroupOut { select, action, dropped }
}

/// "N fx" toggle: click to bypass a chain (layers and groups).
pub(crate) fn fx_badge(ui: &mut egui::Ui, effects: &[EffectState], bypass: &mut bool) -> Option<egui::Response> {
    let fx = effects.iter().filter(|e| e.enabled).count();
    if fx == 0 {
        return None;
    }
    let mut active = !*bypass;
    let color = if active { ACCENT } else { Color32::GRAY };
    let r = ui.toggle_value(&mut active, RichText::new(format!("{fx} fx")).small().color(color)).on_hover_text("Click to bypass these effects · right-click to map");
    if r.changed() {
        *bypass = !active;
    }
    Some(r)
}

/// Applies a group card's action.
pub(crate) fn apply_group_action(st: &mut State, gid: u64, action: GroupAction) {
    match action {
        GroupAction::Up => st.comp.move_group(gid, true),
        GroupAction::Down => st.comp.move_group(gid, false),
        GroupAction::Ungroup => st.comp.ungroup(gid),
        GroupAction::Delete => {
            let members: Vec<u64> = st.comp.layers.iter().filter(|l| l.parent == Some(gid)).map(|l| l.id).collect();
            for id in members {
                remove_layer(st, id);
            }
            st.comp.ungroup(gid);
            if st.selected == Some(gid) {
                st.selected = st.comp.layers.last().map(|l| l.id);
            }
        }
        GroupAction::BrowseEffects => st.fx_browser = Some(crate::explorer::FxBrowser::new(gid, false, None)),
    }
}

/// Inspector for a selected group: its settings and effect chain. Returns an effect-chain action.
pub(crate) fn group_inspector(ui: &mut egui::Ui, g: &mut LayerGroup, thumb: Option<egui::TextureId>, aspect: f32, clipboard_len: usize) -> Option<FxAction> {
    let mut fx_action = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
        let w = ui.available_width().min(360.0);
        thumbnail(ui, thumb, egui::vec2(w, w / aspect), g.enabled);
        ui.horizontal(|ui| {
            ui.toggle_value(&mut g.enabled, RichText::new("ON").small().strong());
            ui.label(icon::STACK);
            ui.add(egui::TextEdit::singleline(&mut g.name).desired_width(f32::INFINITY));
        });
        hint(ui, "A group mixes its layers together first; its opacity, blend and effects then apply to that mix.");
        guarded!(ui, g.opacity, Slider::new(&mut g.opacity, 0.0..=1.0).text("Opacity"));
        ui.horizontal(|ui| {
            ui.label("Blend");
            choice_combo(ui, ("group_blend_insp", g.id), &mut g.blend, 120.0, false);
        });
        ui.checkbox(&mut g.in_mix, "In the mix").on_hover_text("Off: the group appears only on mapping surfaces that pick it");
        ui.separator();
        fx_action = effects_editor(ui, &mut g.effects, clipboard_len);
    });
    fx_action
}
