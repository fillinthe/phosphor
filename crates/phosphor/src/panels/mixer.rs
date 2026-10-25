//! Mixer: one fader strip per layer, one per group (after its members) and MASTER.

use super::*;

pub(crate) fn fader(ui: &mut egui::Ui, value: &mut f32, m: &mut Option<Modulation>, target: ControlTarget, height: f32) {
    Level::percent(value, "Opacity").modulation(m).target(target).vertical(height).show(ui);
}

/// One Mixer strip: card, thumbnail (click selects) and name; `body` adds the controls. Returns true when clicked.
fn strip(ui: &mut egui::Ui, id: u64, is_sel: bool, thumb: Option<egui::TextureId>, aspect: f32, lit: bool, name: RichText, body: impl FnOnce(&mut egui::Ui)) -> bool {
    let card_id = egui::Id::new(("mixer_strip", id));
    let bg = card_background(ui, card_id);
    let mut clicked = false;
    let frame = card_frame(ui, is_sel).inner_margin(egui::Margin::same(5)).show(ui, |ui| {
        ui.set_width(78.0);
        ui.vertical_centered(|ui| {
            clicked = thumbnail(ui, thumb, egui::vec2(72.0, 72.0 / aspect), lit).clicked();
            ui.label(name);
            body(ui);
        });
    });
    remember_card(ui, card_id, frame.response.rect);
    clicked || bg.is_some_and(|b| b.clicked())
}

fn short(name: &str) -> String {
    if name.chars().count() > 11 { format!("{}…", name.chars().take(10).collect::<String>()) } else { name.to_string() }
}

/// "3 fx" under a strip's fader (lit when running, grey when bypassed; nothing with no effects).
fn fx_count(ui: &mut egui::Ui, effects: &[phosphor_engine::EffectState], bypass: &mut bool, target: ControlTarget) {
    if let Some(r) = super::groups::fx_badge(ui, effects, bypass) {
        map_menu(ui, &r, target);
    }
}

/// ON toggle with Learn on right-click (layers and groups).
fn on_toggle(ui: &mut egui::Ui, enabled: &mut bool, target: ControlTarget) {
    let on = toggle(ui, enabled, "ON");
    mapped_badge(ui, &on, &target);
    on.right_menu(|ui| learn_items(ui, &target));
}

pub(crate) fn mixer(ui: &mut egui::Ui, st: &mut State) {
    // Every layer has its strip (comps have theirs before MASTER), as in Layers and Clips: no comp tabs.
    let aspect = st.aspect();
    let height = (ui.available_height() - 210.0).clamp(80.0, 600.0);
    crossfader(ui, st);
    let State { comp, selected, layer_textures, texture, .. } = st;
    let any_solo = comp.layers.iter().any(|l| l.enabled && l.solo);
    let tex = |id: u64| layer_textures.get(&id).map(|(t, _)| *t);
    // The open comp's layers (and, with several comps, a strip per comp before MASTER).
    let shown: Vec<bool> = vec![true; comp.layers.len()];
    let phosphor_engine::Composition { layers, groups, comps, master_fade, master_fade_mod, master_effects, master_fx_bypass, .. } = comp;
    egui::ScrollArea::horizontal().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            let count = layers.len();
            for i in 0..count {
                if !shown[i] {
                    continue;
                }
                let layer = &mut layers[i];
                let muted = any_solo && !layer.solo;
                let name = RichText::new(short(&layer.name)).small().strong();
                let lit = layer.enabled && !muted;
                if strip(ui, layer.id, *selected == Some(layer.id), tex(layer.id), aspect, lit, name, |ui| {
                    // Opacity and (movies) Speed side by side, both upright: what's played with most live.
                    ui.horizontal_top(|ui| {
                        fader(ui, &mut layer.opacity, &mut layer.opacity_mod, ControlTarget::LayerOpacity(layer.id), height);
                        if matches!(layer.source, SourceSpec::Movie { .. }) {
                            crate::ui::Level::range(&mut layer.rate, 0.1, 4.0, "Speed").log().default(1.0).vertical(height).target(ControlTarget::LayerSpeed(layer.id)).show(ui);
                        }
                    });
                    crate::ui::toolbar(ui, |ui| {
                        super::layers::show_button(ui, layer);
                        toggle(ui, &mut layer.solo, "S");
                    });
                    crate::ui::toolbar(ui, |ui| {
                        for g in [phosphor_engine::CrossGroup::A, phosphor_engine::CrossGroup::B] {
                            let on = layer.group == g;
                            if crate::ui::pick_item(ui, on, RichText::new(g.name()).small().strong()).on_hover_text("Crossfader side (click again for none)").clicked() {
                                layer.group = if on { phosphor_engine::CrossGroup::None } else { g };
                            }
                        }
                    });
                    choice_combo(ui, ("mix_blend", layer.id), &mut layer.blend, 70.0, true);
                    fx_count(ui, &layer.effects, &mut layer.fx_bypass, ControlTarget::LayerFxBypass(layer.id));
                }) {
                    *selected = Some(layer.id);
                }
                // A group's strip follows its last member, like a bus.
                let parent = layers[i].parent;
                if let Some(gid) = parent
                    && layers.get(i + 1).and_then(|l| l.parent) != Some(gid)
                    && let Some(g) = groups.iter_mut().find(|g| g.id == gid)
                {
                    let name = RichText::new(format!("{} {}", icon::STACK, short(&g.name))).small().strong().color(accent());
                    if strip(ui, g.id, *selected == Some(g.id), tex(g.id), aspect, g.enabled, name, |ui| {
                        fader(ui, &mut g.opacity, &mut g.opacity_mod, ControlTarget::LayerOpacity(g.id), height);
                        on_toggle(ui, &mut g.enabled, ControlTarget::LayerEnabled(g.id));
                        choice_combo(ui, ("mix_group_blend", g.id), &mut g.blend, 70.0, true);
                        fx_count(ui, &g.effects, &mut g.fx_bypass, ControlTarget::LayerFxBypass(g.id));
                    }) {
                        *selected = Some(g.id);
                    }
                }
            }
            if comps.len() > 1 {
                for c in comps.iter_mut() {
                    let name = RichText::new(format!("{} {}", icon::FRAME_CORNERS, short(&c.name))).small().strong().color(accent());
                    if strip(ui, c.id, *selected == Some(c.id), tex(c.id), aspect, c.enabled, name, |ui| {
                        fader(ui, &mut c.opacity, &mut c.opacity_mod, ControlTarget::LayerOpacity(c.id), height);
                        on_toggle(ui, &mut c.enabled, ControlTarget::LayerEnabled(c.id));
                        choice_combo(ui, ("mix_comp_blend", c.id), &mut c.blend, 70.0, true);
                        fx_count(ui, &c.effects, &mut c.fx_bypass, ControlTarget::LayerFxBypass(c.id));
                    }) {
                        *selected = Some(c.id);
                    }
                }
            }
            // Master strip.
            let name = RichText::new("MASTER").small().strong().color(accent());
            if strip(ui, MASTER_ID, *selected == Some(MASTER_ID), *texture, aspect, true, name, |ui| {
                fader(ui, master_fade, master_fade_mod, ControlTarget::MasterFade, height);
                fx_count(ui, master_effects, master_fx_bypass, ControlTarget::App(phosphor_engine::AppControl::MasterFxBypass));
            }) {
                *selected = Some(MASTER_ID);
            }
        });
    });
}

/// The A ↔ B crossfader across the top of the Mixer (double-click centres it), and Auto-fade: one press glides it to
/// the other side over the time beside it.
fn crossfader(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.input(|i| i.time);
    let auto_target = ControlTarget::App(phosphor_engine::AppControl::AutoFade);
    let gliding = st.control.glide_target(&ControlTarget::App(phosphor_engine::AppControl::Crossfader)).is_some();
    let mut fade = false;
    let comp = &mut st.comp;
    let target = ControlTarget::App(phosphor_engine::AppControl::Crossfader);
    let used = comp.layers.iter().any(|l| l.group != phosphor_engine::CrossGroup::None);
    ui.horizontal(|ui| {
        crate::ui::heading(ui, "A");
        // (Double-click centres it: its default.)
        let width = (ui.available_width() - 200.0).clamp(120.0, 520.0);
        let r = Level::percent(&mut comp.crossfader, "Crossfader").default(0.5).target(target.clone()).width(width).show(ui);
        let r = r.on_hover_text(if used { "Crossfader: left = A layers, right = B layers, centre = both · double-click to centre" } else { "Crossfader: set layers to A or B below" });
        value_bubble(ui, &r, match comp.crossfader {
            x if x < 0.01 => "A".to_string(),
            x if x > 0.99 => "B".to_string(),
            x => format!("A {:.0}% · B {:.0}%", phosphor_engine::CrossGroup::A.gain(x, comp.fader_curve) * 100.0, phosphor_engine::CrossGroup::B.gain(x, comp.fader_curve) * 100.0),
        });
        crate::ui::heading(ui, "B");
        let towards = if comp.crossfader < 0.5 { "B" } else { "A" };
        let auto = action_button(ui, Act::Go, label(if gliding { icon::HOURGLASS } else { icon::ARROWS_LEFT_RIGHT }, "Auto"))
            .on_hover_text(format!("Glide to {towards} over the time beside it (press again on the way to turn back)"));
        fade = auto.clicked();
        map_menu(ui, &auto, auto_target);
        guarded!(ui, comp.auto_fade_secs, egui::DragValue::new(&mut comp.auto_fade_secs).range(0.0..=60.0).speed(0.05).suffix(" s").max_decimals(1)).on_hover_text("Auto-fade time");
        choice_combo(ui, "fader_curve", &mut comp.fader_curve, 150.0, false).on_hover_text("How the crossfader shares out A and B");
        crate::ui::Level::range(&mut comp.master_speed, 0.25, 4.0, "Speed").log().default(1.0).width(130.0).target(ControlTarget::App(phosphor_engine::AppControl::MasterSpeed)).tip("Every movie plays this much faster or slower").show(ui);
    });
    if fade {
        st.auto_fade(now);
    }
}
