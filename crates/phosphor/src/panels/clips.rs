//! Clip grid and launch settings.

use super::*;
use phosphor_engine::TriggerMode;

/// Columns per bank page.
pub const CLIP_PAGE_SIZE: usize = 9;

/// Cut / Fade and Now / Beat / Bar: how clips and media land on layers.
pub fn launch_controls(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition) {
    ui.label(RichText::new("Launch").small().color(Color32::GRAY));
    let cut = ui.selectable_value(&mut comp.launch.fade, false, "Cut").on_hover_text("Switch instantly");
    map_menu(ui, &cut, ControlTarget::App(phosphor_engine::AppControl::LaunchFade));
    let fade = ui.selectable_value(&mut comp.launch.fade, true, "Fade").on_hover_text("Fade between sources in the chosen style");
    map_menu(ui, &fade, ControlTarget::App(phosphor_engine::AppControl::LaunchFade));
    if comp.launch.fade {
        let secs = guarded!(ui, comp.launch.fade_secs, egui::DragValue::new(&mut comp.launch.fade_secs).range(0.05..=10.0).speed(0.02).fixed_decimals(2).suffix(" s"));
        map_menu(ui, &secs, ControlTarget::App(phosphor_engine::AppControl::FadeTime));
        let mut style = comp.launch.style;
        let r = choice_combo(ui, "launch_style", &mut style, 96.0, false).on_hover_text("Fade style for every layer (Crossfade, Wipe, Add, Screen, Subtract…). Each layer can still pick its own in the Inspector.");
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::LaunchStyle));
        if style != comp.launch.style {
            comp.set_launch_style(style);
        }
        if comp.launch.style == phosphor_engine::TransitionKind::Effect {
            let mut effect = comp.launch.transition_effect.clone();
            if super::inspector::transition_effect_combo(ui, "launch_fx", &mut effect) {
                comp.set_transition_effect(&effect);
            }
        }
        if comp.layers.iter().any(|l| l.transition_kind != comp.launch.style) {
            hint(ui, "mixed").on_hover_text("Some layers use their own fade style (Inspector)");
        }
    }
    ui.separator();
    choice_buttons(ui, &mut comp.launch.timing, |ui, r| map_menu(ui, r, ControlTarget::App(phosphor_engine::AppControl::LaunchTiming)));
    #[cfg(not(target_arch = "wasm32"))]
    ui.toggle_value(&mut comp.launch.auto_beat_match, label(icon::MAGIC_WAND, "Auto-match"))
        .on_hover_text("Every movie that lands in the grid is scanned for its visual beat and locked to the tempo (Match the beat)");
    let auto = &mut comp.auto_show;
    ui.toggle_value(&mut auto.enabled, label(icon::PLAY_CIRCLE, "Auto show"))
        .on_hover_text("The show runs itself: the next column with clips every few bars (on the launch quantise), and Surprise when the music drops");
    if auto.enabled {
        ui.label("every");
        guarded!(ui, auto.bars, egui::DragValue::new(&mut auto.bars).range(1..=64).suffix(" bars"));
        ui.toggle_value(&mut auto.surprise_on_drops, "Surprise on drops").on_hover_text("A jump in the music's level fires Surprise (at most every 8 s)");
        ui.toggle_value(&mut auto.react, "React").on_hover_text("Every layer's generator and effects follow the music (re-applied after each Surprise)");
    }
}

pub(crate) fn clips(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    let aspect = st.aspect();
    ui.horizontal_wrapped(|ui| {
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
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(l) = &mut st.link {
                l.resync();
            }
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
        if st.control.midi_clock.active(now) {
            let bpm = st.control.midi_clock.bpm().map_or("…".to_string(), |b| format!("{b:.1}"));
            ui.toggle_value(&mut st.clock.follow_midi, RichText::new(format!("MIDI {bpm}")).small().strong())
                .on_hover_text("MIDI clock is coming in: tempo follows it, and after a Start the beat does too. Click to ignore it.");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut on = st.link.is_some();
            let text = match &st.link {
                Some(l) => format!("LINK {}", l.peers()),
                None => "LINK".into(),
            };
            let r = ui.toggle_value(&mut on, RichText::new(text).small().strong()).on_hover_text("Ableton Link: share tempo and beat with Live, Traktor, Resolume and other Link apps on the network (the number = apps connected)");
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::Link));
            if r.changed() {
                st.link = on.then(|| crate::link::LinkSync::new(st.clock.bpm));
            }
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
        // At least A–D; more when the columns need them (imported projects can have hundreds).
        let banks = st.comp.clip_columns.div_ceil(CLIP_PAGE_SIZE).max(4);
        st.clip_page = st.clip_page.min(banks - 1);
        if banks <= 8 {
            for i in 0..banks {
                if ui.selectable_label(st.clip_page == i, bank_name(i)).clicked() {
                    st.clip_page = i;
                }
            }
        } else {
            if ui.small_button(icon::CARET_LEFT).clicked() {
                st.clip_page = st.clip_page.saturating_sub(1);
            }
            ComboBox::from_id_salt("clip_bank").width(60.0).selected_text(bank_name(st.clip_page)).height(400.0).show_ui(ui, |ui| {
                for i in 0..banks {
                    let first = i * CLIP_PAGE_SIZE + 1;
                    ui.selectable_value(&mut st.clip_page, i, format!("{}  ({first}–{})", bank_name(i), first + CLIP_PAGE_SIZE - 1));
                }
            });
            if ui.small_button(icon::CARET_RIGHT).clicked() {
                st.clip_page = (st.clip_page + 1).min(banks - 1);
            }
            hint(ui, format!("of {banks}"));
        }
        ui.separator();
        if ui.button(label(icon::PLUS, "Column")).clicked() {
            st.comp.clip_columns += 1;
        }
        hint(ui, "Keys 1–9 trigger the lit bank's columns · drop files or a folder onto a cell to fill the row · right-click a cell");
    });
    ui.add_space(4.0);
    // Every column is in the grid (scroll sideways); the bank says which nine the keys 1–9 hit.
    let cols = st.comp.clip_columns.max(1);
    let bank = st.clip_page;
    let cell = egui::vec2(96.0, 96.0 / aspect + 16.0);
    let mut triggers: Vec<Trigger> = Vec::new();
    let mut releases: Vec<(u64, usize)> = Vec::new();
    // The flash/piano cell held down with the mouse (released when the button comes up anywhere).
    let held_id = egui::Id::new("clip_cell_held");
    let held: Option<(u64, usize)> = ui.ctx().data(|d| d.get_temp(held_id));
    let (pressed, released) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released()));
    if released && let Some(h) = held {
        releases.push(h);
        ui.ctx().data_mut(|d| d.remove::<(u64, usize)>(held_id));
    }
    let mut set_cell: Option<(usize, usize, usize)> = None; // (layer idx, slot, media idx)
    let mut use_current: Option<(usize, usize)> = None;
    let mut clear: Option<(usize, usize)> = None;
    let mut beat: Option<(u64, usize)> = None;
    let mut beat_row: Option<u64> = None;
    let mut fill_row: Option<u64> = None;
    {
        let drag_at = crate::app::file_drag_point(ui.ctx());
        let State { comp, thumbs, clip_cells, preview_wants, preview_keys, preview_textures, .. } = st;
        let ctx = ui.ctx().clone();
        let thumb_h = (crate::thumbs::THUMB_W as f32 / aspect).max(1.0) as u32;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            // Column headers trigger whole columns.
            ui.horizontal(|ui| {
                ui.allocate_exact_size(egui::vec2(110.0, 22.0), Sense::hover());
                for c in 0..cols {
                    let keyed = c / CLIP_PAGE_SIZE == bank;
                    let text = RichText::new(format!("{} {}", icon::PLAY, c + 1));
                    let text = if keyed { text.color(ACCENT) } else { text };
                    let tip = if keyed { format!("Trigger this column on every layer (key {}) · right-click to Learn", c % CLIP_PAGE_SIZE + 1) } else { "Trigger this column on every layer · right-click to Learn".to_string() };
                    let head = ui.add_sized([cell.x, 22.0], egui::Button::new(text)).on_hover_text(tip);
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
                    let (fill, beat_all) = row_header(ui, layer, egui::vec2(110.0, cell.y));
                    if fill {
                        fill_row = Some(layer.id);
                    }
                    if beat_all {
                        beat_row = Some(layer.id);
                    }
                    for c in 0..cols {
                        let clip = layer.clips.get(c);
                        let (rect, resp) = ui.allocate_exact_size(cell, Sense::click());
                        if ui.is_rect_visible(rect) {
                            clip_cells.push((rect, layer.id, c));
                        }
                        let active = clip.is_some_and(|k| !k.is_empty() && layer.active_clip == Some(k.id));
                        let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(cell.x, cell.x / aspect));
                        let p = ui.painter();
                        p.rect_filled(rect, 4.0, if active { Color32::from_rgb(24, 40, 32) } else { Color32::from_gray(18) });
                        match clip {
                            Some(k) if !k.is_empty() => {
                                let tex = match &k.source {
                                    SourceSpec::Image { path: Some(pth), .. } | SourceSpec::Movie { path: Some(pth), .. } => thumbs.get(pth),
                                    // Generators render through the engine's preview slots: a still each, animated while hovered.
                                    SourceSpec::Generator(id) => {
                                        let key = format!("clip:{}:{:x}", k.id, hash_values(&k.generator_values));
                                        preview_wants.push((key.clone(), phosphor_engine::PreviewItem::Clip(id.clone(), k.generator_values.clone()), resp.hovered()));
                                        crate::app::preview_lookup(preview_keys, preview_textures, &key)
                                    }
                                    SourceSpec::Text(style) => {
                                        let key = format!("text:{:x}", hash_text(style));
                                        thumbs.get_or_build(&ctx, &key, || {
                                            let img = crate::text::render(style, crate::thumbs::THUMB_W, thumb_h).ok()?;
                                            Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img.into_raw()))
                                        })
                                    }
                                    _ => None,
                                };
                                match tex {
                                    Some(t) => paint_texture(p, t, thumb_rect),
                                    None => {
                                        p.rect_filled(thumb_rect, 4.0, Color32::from_gray(30));
                                        // Long names (imported clips) stay inside their own cell; sound files get a note.
                                        let label: String = if crate::media::is_sound_file(&k.source.label()) {
                                            format!("{} {}", icon::MUSIC_NOTES, k.source.label().chars().take(13).collect::<String>())
                                        } else {
                                            k.source.label().chars().take(16).collect()
                                        };
                                        p.with_clip_rect(thumb_rect).text(thumb_rect.center(), egui::Align2::CENTER_CENTER, label, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
                                    }
                                }
                                let mut name = if k.name.is_empty() { k.source.label() } else { k.name.clone() };
                                if name.chars().count() > 14 {
                                    name = format!("{}…", name.chars().take(13).collect::<String>());
                                }
                                p.text(egui::pos2(rect.left() + 3.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(10.5), Color32::LIGHT_GRAY);
                                let badge = match k.trigger {
                                    TriggerMode::Normal => None,
                                    TriggerMode::Toggle => Some("T"),
                                    TriggerMode::Flash => Some("F"),
                                    TriggerMode::Piano => Some("P"),
                                };
                                let own = if k.own.is_some() { "S" } else { "" };
                                let badge = format!("{}{own}", badge.unwrap_or(""));
                                if !badge.is_empty() {
                                    p.text(egui::pos2(rect.right() - 4.0, rect.top() + 3.0), egui::Align2::RIGHT_TOP, badge, egui::FontId::monospace(11.0), ACCENT);
                                }
                            }
                            _ => {
                                p.text(rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), Color32::from_gray(70));
                            }
                        }
                        let stroke = if drag_at.is_some_and(|p| rect.contains(p)) {
                            // Files from the Finder land here (and fill the row to the right).
                            Stroke::new(2.5, Color32::from_rgb(255, 196, 70))
                        } else if active {
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
                        // Flash / piano clips play from the moment the button goes down until it comes up.
                        let momentary = clip.is_some_and(|k| !k.is_empty() && matches!(k.trigger, TriggerMode::Flash | TriggerMode::Piano));
                        if momentary {
                            if pressed && resp.hovered() {
                                triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                ui.ctx().data_mut(|d| d.insert_temp(held_id, (layer.id, c)));
                            }
                        } else if resp.clicked() {
                            match clip {
                                Some(k) if !k.is_empty() => triggers.push(Trigger::Clip { layer: layer.id, slot: c }),
                                _ => use_current = Some((idx, c)),
                            }
                        }
                        mapped_badge(ui, &resp, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                        let filled = clip.is_some_and(|k| !k.is_empty());
                        let is_movie = clip.is_some_and(|k| matches!(k.source, SourceSpec::Movie { path: Some(_), .. }));
                        resp.right_menu(|ui| {
                            if filled {
                                if ui.button(label(icon::PLAY, "Trigger")).clicked() {
                                    triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                    ui.close();
                                }
                                learn_items(ui, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                                if is_movie
                                    && ui.button(label(icon::MAGIC_WAND, "Match the beat")).on_hover_text("Scan this movie's motion for its rhythm; the clip then starts on its first hit and is stretched so every hit lands on a beat").clicked()
                                {
                                    beat = Some((layer.id, c));
                                    ui.close();
                                }
                                ui.separator();
                                ui.label(RichText::new("When pressed").small().strong());
                                let current = layer.clip_settings();
                                let k = layer.clip_slot_mut(c);
                                for m in TriggerMode::ALL {
                                    ui.radio_value(&mut k.trigger, m, m.name());
                                }
                                ui.separator();
                                let mut own = k.own.is_some();
                                if ui
                                    .checkbox(&mut own, "Keeps its own settings")
                                    .on_hover_text("Speed, direction, loop, BPM sync and effects are saved with this clip and come back when it's triggered (starts from the layer's current settings)")
                                    .changed()
                                {
                                    k.own = own.then_some(current);
                                }
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
                hint(ui, "Add layers first; each layer gets a row of clip slots.");
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
        slot.mods.clear();
        if slot.id == 0 {
            slot.id = id * 1000 + c as u64 + 1;
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some((layer, c)) = beat {
            st.match_beat(crate::media::rhythm::MatchTarget::Clip(layer, c));
        }
        if let Some(layer) = beat_row {
            st.match_beat_all(layer);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (beat, beat_row);
    if let Some((idx, c)) = use_current
        && let Some(layer) = st.comp.layers.get_mut(idx)
    {
        let (src, vals, mods, name, id) = (layer.source.clone(), layer.generator_values.clone(), layer.generator_mods.clone(), layer.name.clone(), layer.id);
        if src != SourceSpec::None {
            let slot = layer.clip_slot_mut(c);
            slot.source = src;
            slot.generator_values = vals;
            slot.mods = mods;
            slot.name = name;
            slot.id = id * 1000 + c as u64 + 1;
            layer.active_clip = Some(slot.id);
        }
    }
    if let Some(id) = fill_row {
        st.quick_on(crate::app::Quick::Fill, Some(id), now);
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
    for (layer, slot) in releases {
        st.release_clip(layer, slot, now);
    }
}

/// The layer's name at the start of its clip row; click for autopilot (plays the next / a random clip by itself).
/// The layer name at the start of a clip row: click for autopilot and "fill with variations" (returns true for that).
/// The layer's name at the start of its clip row, with its menu. Returns (fill the row, match every clip's beat).
fn row_header(ui: &mut egui::Ui, layer: &mut phosphor_engine::LayerState, size: egui::Vec2) -> (bool, bool) {
    use phosphor_engine::Autopilot;
    let mut fill = false;
    let mut beat_all = false;
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_size(size);
        ui.add_space((size.y - 40.0).max(0.0) / 2.0);
        let name: String = layer.name.chars().take(14).collect();
        ui.menu_button(RichText::new(name).color(Color32::LIGHT_GRAY), |ui| {
            ui.set_min_width(220.0);
            let q = crate::app::Quick::Fill;
            if ui.button(format!("{} {}", q.icon(), q.label())).on_hover_text(q.about()).clicked() {
                fill = true;
                ui.close();
            }
            if layer.clips.iter().any(|c| matches!(c.source, SourceSpec::Movie { path: Some(_), .. }))
                && ui.button(label(icon::MAGIC_WAND, "Match the beat of every movie here")).on_hover_text("Scans each movie clip in this row for its rhythm and locks it to the tempo").clicked()
            {
                beat_all = true;
                ui.close();
            }
            ui.separator();
            ui.label(RichText::new(format!("{} Autopilot", icon::AIRPLANE_TILT)).strong());
            hint(ui, "Plays another clip by itself when this one is done.");
            for a in Autopilot::ALL {
                ui.radio_value(&mut layer.autopilot, a, a.name());
            }
            if layer.autopilot != Autopilot::Off {
                ui.separator();
                let mut at_end = layer.autopilot_beats <= 0.0;
                ui.horizontal(|ui| {
                    if ui.radio_value(&mut at_end, true, "When the clip ends").changed() {
                        layer.autopilot_beats = 0.0;
                    }
                });
                ui.horizontal(|ui| {
                    if ui.radio_value(&mut at_end, false, "After").changed() {
                        layer.autopilot_beats = 16.0;
                    }
                    if !at_end {
                        guarded!(ui, layer.autopilot_beats, egui::DragValue::new(&mut layer.autopilot_beats).range(1.0..=256.0).speed(0.2).suffix(" beats"));
                    }
                });
                if at_end {
                    hint(ui, "Stills, generators and live inputs have no end: they move on after 16 beats.");
                }
            }
        })
        .response
        .on_hover_text("Click for autopilot and fill with variations");
        if layer.autopilot != Autopilot::Off {
            let when = if layer.autopilot_beats > 0.0 { format!("{:.0} beats", layer.autopilot_beats) } else { "at end".into() };
            ui.label(RichText::new(format!("{} {} · {when}", icon::AIRPLANE_TILT, layer.autopilot.name())).small().color(ACCENT));
        }
    });
    (fill, beat_all)
}

/// Bank letters: A–Z, then AA, AB…
fn bank_name(i: usize) -> String {
    let letter = |n: usize| char::from(b'A' + (n % 26) as u8);
    if i < 26 { letter(i).to_string() } else { format!("{}{}", letter(i / 26 - 1), letter(i % 26)) }
}

fn hash_values(values: &[f32]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in values {
        v.to_bits().hash(&mut h);
    }
    h.finish()
}

fn hash_text(style: &phosphor_engine::TextStyle) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    format!("{style:?}").hash(&mut h);
    h.finish()
}
