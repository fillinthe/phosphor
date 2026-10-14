//! Clip grid and launch settings.

use super::*;

/// Columns per bank page.
pub const CLIP_PAGE_SIZE: usize = 9;

/// Cut / Fade and Now / Beat / Bar: how clips and media land on layers.
pub fn launch_controls(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition) {
    ui.label(RichText::new("Launch").small().color(Color32::GRAY));
    let cut = ui.selectable_value(&mut comp.launch.fade, false, "Cut").on_hover_text("Switch instantly");
    map_menu(ui, &cut, ControlTarget::App(phosphor_engine::AppControl::LaunchFade));
    let fade = ui.selectable_value(&mut comp.launch.fade, true, "Fade").on_hover_text("Crossfade (or the layer's transition style)");
    map_menu(ui, &fade, ControlTarget::App(phosphor_engine::AppControl::LaunchFade));
    if comp.launch.fade {
        let secs = guarded!(ui, comp.launch.fade_secs, egui::DragValue::new(&mut comp.launch.fade_secs).range(0.05..=10.0).speed(0.02).fixed_decimals(2).suffix(" s"));
        map_menu(ui, &secs, ControlTarget::App(phosphor_engine::AppControl::FadeTime));
    }
    ui.separator();
    for t in phosphor_engine::LaunchTiming::ALL {
        let r = ui.selectable_value(&mut comp.launch.timing, t, t.name());
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::LaunchTiming));
    }
}

pub(crate) fn clips(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    let aspect = st.aspect();
    ui.horizontal(|ui| {
        let tap = ui.button(label(icon::METRONOME, "Tap")).on_hover_text("Tap 3+ times to set the tempo");
        map_menu(ui, &tap, ControlTarget::App(phosphor_engine::AppControl::TapTempo));
        if tap.clicked() {
            st.clock.tap(now);
        }
        let bpm = guarded!(ui, st.clock.bpm, egui::DragValue::new(&mut st.clock.bpm).range(40.0..=300.0).speed(0.5).suffix(" bpm"));
        map_menu(ui, &bpm, ControlTarget::App(phosphor_engine::AppControl::Tempo));
        let resync = ui.button("Resync").on_hover_text("Beat 1 = now");
        map_menu(ui, &resync, ControlTarget::App(phosphor_engine::AppControl::Resync));
        if resync.clicked() {
            st.clock.resync(now);
        }
        // Beat indicator: four dots, the current one lit.
        let beat = st.clock.beat_in_bar(now);
        let phase = st.clock.phase(now);
        for i in 0..4 {
            let (r, _) = ui.allocate_exact_size(egui::vec2(12.0, 12.0), Sense::hover());
            let lit = i == beat;
            let color = if lit { ACCENT.gamma_multiply(1.0 - 0.6 * phase) } else { Color32::from_gray(60) };
            ui.painter().circle_filled(r.center(), if lit { 5.0 } else { 4.0 }, color);
        }
        let auto = ui.toggle_value(&mut st.clock.follow_audio, RichText::new("AUTO").small().strong()).on_hover_text("Follow the audio beat detector (tempo and beat)");
        map_menu(ui, &auto, ControlTarget::App(phosphor_engine::AppControl::FollowAudio));
        if st.mod_inputs.audio_beat > 0.05 {
            ui.painter().circle_filled(auto.rect.right_top() + egui::vec2(-2.0, 2.0), 2.0 + 3.0 * st.mod_inputs.audio_beat, Color32::from_rgb(255, 196, 70));
        }
        ui.separator();
        launch_controls(ui, &mut st.comp);
        if !st.pending_triggers.is_empty() {
            ui.colored_label(ACCENT, format!("{} queued", st.pending_triggers.len()));
        }
        ui.separator();
        ui.label(RichText::new("Bank").small());
        for (i, name) in ["A", "B", "C", "D"].iter().enumerate() {
            if ui.selectable_label(st.clip_page == i, *name).clicked() {
                st.clip_page = i;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(label(icon::PLUS, "Column")).clicked() {
                st.comp.clip_columns += 1;
            }
            ui.weak("Keys 1–9 trigger this bank's columns · drop media onto a cell · right-click a cell");
        });
    });
    ui.add_space(4.0);
    let cols = st.comp.clip_columns.max(1);
    let page_offset = st.clip_page * CLIP_PAGE_SIZE;
    let cell = egui::vec2(96.0, 96.0 / aspect + 16.0);
    let mut triggers: Vec<Trigger> = Vec::new();
    let mut set_cell: Option<(usize, usize, usize)> = None; // (layer idx, slot, media idx)
    let mut use_current: Option<(usize, usize)> = None;
    let mut clear: Option<(usize, usize)> = None;
    {
        let State { comp, thumbs, .. } = st;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            // Column headers trigger whole columns.
            ui.horizontal(|ui| {
                ui.allocate_exact_size(egui::vec2(110.0, 22.0), Sense::hover());
                for c in page_offset..page_offset + cols {
                    let head = ui.add_sized([cell.x, 22.0], egui::Button::new(format!("{} {}", icon::PLAY, c + 1))).on_hover_text("Trigger this column on every layer · right-click to Learn");
                    if head.clicked() {
                        triggers.push(Trigger::Column(c));
                    }
                    mapped_badge(ui, &head, &ControlTarget::TriggerColumn(c));
                    head.right_menu(|ui| learn_items(ui, &ControlTarget::TriggerColumn(c)));
                }
            });
            let n = comp.layers.len();
            for idx in (0..n).rev() {
                let layer = &mut comp.layers[idx];
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(110.0, cell.y), Sense::hover());
                    ui.painter().text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, &layer.name, egui::FontId::proportional(13.0), Color32::LIGHT_GRAY);
                    for c in page_offset..page_offset + cols {
                        let clip = layer.clips.get(c);
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                        let active = clip.is_some_and(|k| !k.is_empty() && layer.active_clip == Some(k.id));
                        let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(cell.x, cell.x / aspect));
                        let p = ui.painter();
                        p.rect_filled(rect, 4.0, if active { Color32::from_rgb(24, 40, 32) } else { Color32::from_gray(18) });
                        match clip {
                            Some(k) if !k.is_empty() => {
                                let tex = match &k.source {
                                    SourceSpec::Image { path: Some(pth), .. } | SourceSpec::Movie { path: Some(pth), .. } => thumbs.get(pth),
                                    _ => None,
                                };
                                match tex {
                                    Some(t) => paint_texture(p, t, thumb_rect),
                                    None => {
                                        p.rect_filled(thumb_rect, 4.0, Color32::from_gray(30));
                                        p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, k.source.label(), egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
                                    }
                                }
                                let mut name = if k.name.is_empty() { k.source.label() } else { k.name.clone() };
                                if name.chars().count() > 14 {
                                    name = format!("{}…", name.chars().take(13).collect::<String>());
                                }
                                p.text(egui::pos2(rect.left() + 3.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(10.5), Color32::LIGHT_GRAY);
                            }
                            _ => {
                                p.text(rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), Color32::from_gray(70));
                            }
                        }
                        let stroke = if active {
                            Stroke::new(2.0, ACCENT)
                        } else if resp.hovered() || resp.dnd_hover_payload::<MediaDrag>().is_some() {
                            Stroke::new(1.0, ACCENT.gamma_multiply(0.7))
                        } else {
                            Stroke::new(1.0, Color32::from_gray(45))
                        };
                        p.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
                        if let Some(m) = resp.dnd_release_payload::<MediaDrag>() {
                            set_cell = Some((idx, c, m.0));
                        }
                        if resp.clicked() {
                            match clip {
                                Some(k) if !k.is_empty() => triggers.push(Trigger::Clip { layer: layer.id, slot: c }),
                                _ => use_current = Some((idx, c)),
                            }
                        }
                        mapped_badge(ui, &resp, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                        resp.right_menu(|ui| {
                            if clip.is_some_and(|k| !k.is_empty()) {
                                if ui.button(label(icon::PLAY, "Trigger")).clicked() {
                                    triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                    ui.close();
                                }
                                learn_items(ui, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                                ui.separator();
                            }
                            if ui.button("Store the layer's current source here").clicked() {
                                use_current = Some((idx, c));
                                ui.close();
                            }
                            if ui.button("Clear slot").clicked() {
                                clear = Some((idx, c));
                                ui.close();
                            }
                        });
                    }
                });
            }
            if n == 0 {
                ui.weak("Add layers first; each layer gets a row of clip slots.");
            }
        });
    }
    if let Some((idx, c, m)) = set_cell
        && let Some(item) = st.comp.media_bin.get(m).cloned()
        && let Some(layer) = st.comp.layers.get_mut(idx)
    {
        let id = layer.id;
        let slot = layer.clip_slot_mut(c);
        slot.source = crate::media::spec_for_path(&item.path);
        slot.name = item.name;
        slot.generator_values.clear();
        if slot.id == 0 {
            slot.id = id * 1000 + c as u64 + 1;
        }
    }
    if let Some((idx, c)) = use_current
        && let Some(layer) = st.comp.layers.get_mut(idx)
    {
        let (src, vals, name, id) = (layer.source.clone(), layer.generator_values.clone(), layer.name.clone(), layer.id);
        if src != SourceSpec::None {
            let slot = layer.clip_slot_mut(c);
            slot.source = src;
            slot.generator_values = vals;
            slot.name = name;
            slot.id = id * 1000 + c as u64 + 1;
            layer.active_clip = Some(slot.id);
        }
    }
    if let Some((idx, c)) = clear
        && let Some(layer) = st.comp.layers.get_mut(idx)
        && c < layer.clips.len()
    {
        layer.clips[c] = Default::default();
    }
    for t in triggers {
        st.trigger(t, now);
    }
}
