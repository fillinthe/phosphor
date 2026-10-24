//! Clip grid and launch settings.

use super::*;
use phosphor_engine::TriggerMode;

/// Columns per bank page.
pub const CLIP_PAGE_SIZE: usize = 9;

/// Cut / Fade and Now / Beat / Bar: how clips and media land on layers.
pub fn launch_controls(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition) {
    ui.label(RichText::new("Launch").small().color(Color32::GRAY));
    let fade_target = ControlTarget::App(phosphor_engine::AppControl::LaunchFade);
    let was = comp.launch.fade;
    segmented(ui, &mut comp.launch.fade, &[(false, "Cut".to_string()), (true, "Fade".to_string())], |ui, r, fade| {
        let r = r.clone().on_hover_text(if *fade { "Fade between sources in the chosen style" } else { "Switch instantly" });
        map_menu(ui, &r, fade_target.clone());
    });
    if is_locked(ui, &fade_target) {
        comp.launch.fade = was;
    }
    if comp.launch.fade {
        let secs = guarded!(ui, comp.launch.fade_secs, egui::DragValue::new(&mut comp.launch.fade_secs).range(0.05..=10.0).speed(0.02).fixed_decimals(2).suffix(" s"));
        map_menu(ui, &secs, ControlTarget::App(phosphor_engine::AppControl::FadeTime));
        let mut style = comp.launch.style;
        let r = choice_combo(ui, "launch_style", &mut style, 96.0, false).on_hover_text("Fade style for every layer (Crossfade, Wipe, Add, Screen, Subtract…). Each layer can still pick its own in the Inspector.");
        map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::LaunchStyle));
        if style != comp.launch.style {
            comp.set_launch_style(style);
        }
        let mut effect = comp.launch.transition_effect.clone();
        if super::inspector::transition_effect_combo(ui, "launch_fx", comp.launch.style, &mut effect) {
            comp.set_transition_effect(&effect);
        }
        if comp.layers.iter().any(|l| l.transition_kind != comp.launch.style) {
            hint(ui, "mixed").on_hover_text("Some layers use their own fade style (Inspector)");
        }
    }
    ui.separator();
    choice_mapped(ui, &mut comp.launch.timing, ControlTarget::App(phosphor_engine::AppControl::LaunchTiming));
    #[cfg(not(target_arch = "wasm32"))]
    toggle(ui, &mut comp.launch.preload, "Preload").on_hover_text("Keep the next movie clips in this bank open, paused on their first frame, so they start the moment you trigger them (a few at a time; uses some memory)");
    #[cfg(not(target_arch = "wasm32"))]
    toggle(ui, &mut comp.launch.auto_beat_match, label(icon::MAGIC_WAND, "Auto-match"))
        .on_hover_text("Every movie that lands in the grid is scanned for its visual beat and locked to the tempo (Match the beat)");
    let auto = &mut comp.auto_show;
    toggle(ui, &mut auto.enabled, label(icon::PLAY_CIRCLE, "Auto show"))
        .on_hover_text("The show runs itself: the next column with clips every few bars (on the launch quantise), and Surprise when the music drops");
    if auto.enabled {
        ui.label("every");
        guarded!(ui, auto.bars, egui::DragValue::new(&mut auto.bars).range(1..=64).suffix(" bars"));
        toggle(ui, &mut auto.surprise_on_drops, "Surprise on drops").on_hover_text("A jump in the music's level fires Surprise (at most every 8 s)");
        toggle(ui, &mut auto.react, "React").on_hover_text("Every layer's generator and effects follow the music (re-applied after each Surprise)");
    }
}

/// What `clip_editor` asks for (the rest it changes on the clip directly).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ClipAct {
    Trigger,
    MatchBeat,
    StoreCurrent,
    /// The layer's picture now becomes the clip's picture in the grid.
    Thumbnail,
    Clear,
}

/// One clip's settings: name, trigger (and its MIDI / key mapping), how a press plays it, sound, its own settings,
/// and store / clear. The clip cell's right-click menu and the Inspector both show this.
pub(crate) fn clip_editor(ui: &mut egui::Ui, layer: &mut LayerState, c: usize, filled: bool, is_movie: bool) -> Option<ClipAct> {
    let mut act = None;
    if filled {
        crate::ui::text_field(ui, &mut layer.clip_slot_mut(c).name, "clip name", None);
        ui.horizontal_wrapped(|ui| {
            if crate::ui::command(ui, label(icon::PLAY, "Trigger")).clicked() {
                act = Some(ClipAct::Trigger);
            }
            let target = ControlTarget::TriggerClip { layer: layer.id, slot: c };
            ui.menu_button(label(icon::PLUG, "Map…"), |ui| learn_items(ui, &target)).response.on_hover_text("Trigger it from a MIDI pad, a key or OSC");
            if is_movie
                && crate::ui::command(ui, label(icon::MAGIC_WAND, "Match the beat")).on_hover_text("Scan this movie's motion for its rhythm; the clip then starts on its first hit and is stretched so every hit lands on a beat").clicked()
            {
                act = Some(ClipAct::MatchBeat);
            }
            #[cfg(not(target_arch = "wasm32"))]
            if crate::ui::command(ui, label(icon::IMAGE, "Use this picture")).on_hover_text("The layer's picture right now becomes this clip's picture in the grid (right-click again for a new one)").clicked() {
                act = Some(ClipAct::Thumbnail);
            }
        });
        let current = layer.clip_settings();
        let lid = layer.id;
        let k = layer.clip_slot_mut(c);
        ui.horizontal(|ui| {
            ui.label("When pressed");
            choice_combo(ui, ("clip_trigger", lid, c), &mut k.trigger, 150.0, false);
        });
        if is_movie {
            crate::ui::toggle(ui, &mut k.muted, "Mute sound").on_hover_text("This clip plays without sound, wherever it's triggered");
        }
        let mut own = k.own.is_some();
        if crate::ui::toggle(ui, &mut own, "Keeps its own settings")
            .on_hover_text("Speed, direction, loop, BPM sync and effects are saved with this clip and come back when it's triggered (starts from the layer's current settings)")
            .changed()
        {
            k.own = own.then_some(current);
        }
        ui.separator();
    }
    ui.horizontal_wrapped(|ui| {
        if crate::ui::command(ui, "Store the layer's current source here").clicked() {
            act = Some(ClipAct::StoreCurrent);
        }
        if filled && crate::ui::danger_button(ui, "Clear slot").clicked() {
            act = Some(ClipAct::Clear);
        }
    });
    act
}

/// Carries out a `clip_editor` request for clip `c` of layer `layer` (grid menu and Inspector alike).
pub(crate) fn apply_clip_act(st: &mut State, layer: u64, c: usize, act: ClipAct, now: f64) {
    match act {
        ClipAct::Trigger => st.trigger(Trigger::Clip { layer, slot: c }, now),
        ClipAct::MatchBeat => {
            #[cfg(not(target_arch = "wasm32"))]
            st.match_beat(crate::media::rhythm::MatchTarget::Clip(layer, c));
        }
        // The layer's source, values and modulation as this clip (and it's the playing one now).
        ClipAct::StoreCurrent => {
            let Some(l) = st.comp.layer_mut(layer) else { return };
            let (src, vals, mods, name, id) = (l.source.clone(), l.generator_values.clone(), l.generator_mods.clone(), l.name.clone(), l.id);
            if src != SourceSpec::None {
                let slot = l.clip_slot_mut(c);
                slot.source = src;
                slot.generator_values = vals;
                slot.mods = mods;
                slot.name = name;
                slot.id = id * 1000 + c as u64 + 1;
                l.active_clip = Some(slot.id);
            }
        }
        ClipAct::Thumbnail => st.clip_thumb_request = Some((layer, c)),
        ClipAct::Clear => {
            if let Some(l) = st.comp.layer_mut(layer)
                && c < l.clips.len()
            {
                l.clips[c] = Default::default();
            }
            if st.selected_clip == Some((layer, c)) {
                st.selected_clip = None;
            }
        }
    }
}

pub(crate) fn clips(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    let aspect = st.aspect();
    ui.horizontal_wrapped(|ui| {
        let tap = crate::ui::command(ui, label(icon::METRONOME, "Tap")).on_hover_text("Tap 3+ times to set the tempo");
        map_menu(ui, &tap, ControlTarget::App(phosphor_engine::AppControl::TapTempo));
        if tap.clicked() {
            st.clock.tap(now);
        }
        let bpm = guarded!(ui, st.clock.bpm, egui::DragValue::new(&mut st.clock.bpm).range(40.0..=300.0).speed(0.5).suffix(" bpm"));
        map_menu(ui, &bpm, ControlTarget::App(phosphor_engine::AppControl::Tempo));
        let resync = crate::ui::command(ui, "Resync").on_hover_text("Beat 1 = now");
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
            let color = if lit { accent().gamma_multiply(1.0 - 0.6 * phase) } else { Color32::from_gray(60) };
            ui.painter().circle_filled(r.center(), if lit { 5.0 } else { 4.0 }, color);
        }
        if st.control.midi_clock.active(now) {
            let bpm = st.control.midi_clock.bpm().map_or("…".to_string(), |b| format!("{b:.1}"));
            toggle(ui, &mut st.clock.follow_midi, format!("MIDI {bpm}"))
                .on_hover_text("MIDI clock is coming in: tempo follows it, and after a Start the beat does too. Click to ignore it.");
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let mut on = st.link.is_some();
            let text = match &st.link {
                Some(l) => format!("LINK {}", l.peers()),
                None => "LINK".into(),
            };
            let r = toggle(ui, &mut on, text).on_hover_text("Ableton Link: share tempo and beat with Live, Traktor, Resolume and other Link apps on the network (the number = apps connected)");
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::Link));
            if r.changed() {
                st.link = on.then(|| crate::link::LinkSync::new(st.clock.bpm));
            }
        }
        let auto = toggle(ui, &mut st.clock.follow_audio, "AUTO").on_hover_text("Follow the audio beat detector (tempo and beat)");
        map_menu(ui, &auto, ControlTarget::App(phosphor_engine::AppControl::FollowAudio));
        if st.mod_inputs.audio_beat > 0.05 {
            ui.painter().circle_filled(auto.rect.right_top() + egui::vec2(-2.0, 2.0), 2.0 + 3.0 * st.mod_inputs.audio_beat, Color32::from_rgb(255, 196, 70));
        }
        ui.separator();
        launch_controls(ui, &mut st.comp);
        if !st.pending_triggers.is_empty() {
            ui.colored_label(accent(), format!("{} queued", st.pending_triggers.len()));
        }
        ui.separator();
        ui.label(RichText::new("Bank").small());
        // At least A–D; more when the columns need them (imported projects can have hundreds).
        let banks = st.comp.clip_columns.div_ceil(CLIP_PAGE_SIZE).max(4);
        st.clip_page = st.clip_page.min(banks - 1);
        if banks <= 8 {
            for i in 0..banks {
                if crate::ui::pick_item(ui, st.clip_page == i, bank_name(i)).clicked() {
                    st.clip_page = i;
                }
            }
        } else {
            if crate::ui::small_command(ui, icon::CARET_LEFT).clicked() {
                st.clip_page = st.clip_page.saturating_sub(1);
            }
            ComboBox::from_id_salt("clip_bank").width(60.0).selected_text(bank_name(st.clip_page)).height(400.0).show_ui(ui, |ui| {
                for i in 0..banks {
                    let first = i * CLIP_PAGE_SIZE + 1;
                    crate::ui::pick_value(ui, &mut st.clip_page, i, format!("{}  ({first}–{})", bank_name(i), first + CLIP_PAGE_SIZE - 1));
                }
            });
            if crate::ui::small_command(ui, icon::CARET_RIGHT).clicked() {
                st.clip_page = (st.clip_page + 1).min(banks - 1);
            }
            hint(ui, format!("of {banks}"));
        }
        ui.separator();
        if crate::ui::command(ui, label(icon::PLUS, "Column")).clicked() {
            st.comp.clip_columns += 1;
        }
        hint(ui, "Keys 1–9 trigger the lit bank's columns · drop files or a folder onto a cell to fill the row · right-click a cell");
    });
    comp_tabs(ui, st);
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
    let mut set_source: Option<(usize, usize, super::SourceDrag)> = None; // (layer idx, slot, a live input…)
    let mut use_current: Option<(usize, usize)> = None;
    // A clip clicked: it plays and is selected (its layer too), so the Inspector shows its settings.
    let mut picked: Option<(u64, usize)> = None;
    let mut clear: Option<(usize, usize)> = None;
    let mut thumb_req: Option<(u64, usize)> = None;
    let mut beat: Option<(u64, usize)> = None;
    let mut beat_row: Option<u64> = None;
    let mut fill_row: Option<u64> = None;
    let mut row_midi: Option<(u64, RowAct)> = None;
    {
        let drag_at = crate::app::file_drag_point(ui.ctx());
        let adding = ui.input(|i| i.modifiers.shift || i.modifiers.command);
        let selected_clip = st.selected_clip;
        let State { comp, thumbs, clip_cells, preview_wants, preview_keys, preview_textures, layer_textures, selected, also_clips, .. } = st;
        let ctx = ui.ctx().clone();
        let thumb_h = (crate::thumbs::THUMB_W as f32 / aspect).max(1.0) as u32;
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            // Column headers trigger whole columns.
            ui.horizontal(|ui| {
                ui.allocate_exact_size(egui::vec2(ROW_HEADER_W, 22.0), Sense::hover());
                for c in 0..cols {
                    let keyed = c / CLIP_PAGE_SIZE == bank;
                    let tip = if keyed { format!("Trigger this column on every layer (key {}) · right-click to Learn", c % CLIP_PAGE_SIZE + 1) } else { "Trigger this column on every layer · right-click to Learn".to_string() };
                    // Triggers are actions: solid buttons (the columns on the number keys in full colour).
                    let head = if keyed {
                        action_button_sized(ui, Act::Go, format!("{} {}", icon::PLAY, c + 1), egui::vec2(cell.x, CONTROL_H))
                    } else {
                        command_sized(ui, format!("{} {}", icon::PLAY, c + 1), egui::vec2(cell.x, CONTROL_H))
                    }
                    .on_hover_text(tip);
                    if head.clicked() {
                        triggers.push(Trigger::Column(c));
                    }
                    mapped_badge(ui, &head, &ControlTarget::TriggerColumn(c));
                    head.right_menu(|ui| learn_items(ui, &ControlTarget::TriggerColumn(c)));
                }
            });
            let n = comp.layers.len();
            for idx in (0..n).rev() {
                if !comp.in_open_comp(&comp.layers[idx]) {
                    continue;
                }
                let layer = &mut comp.layers[idx];
                ui.horizontal(|ui| {
                    let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
                    let (fill, beat_all, midi) = row_header(ui, layer, thumb, selected, egui::vec2(ROW_HEADER_W, cell.y));
                    if let Some(m) = midi {
                        row_midi = Some((layer.id, m));
                    }
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
                        let chosen = also_clips.contains(&(layer.id, c)) || (!also_clips.is_empty() && selected_clip == Some((layer.id, c)));
                        let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(cell.x, cell.x / aspect));
                        let p = ui.painter();
                        let empty = ui.visuals().extreme_bg_color;
                        p.rect_filled(rect, 4.0, if active { crate::theme::tint(empty, 0.14) } else { empty });
                        if chosen {
                            p.rect_stroke(rect, 4.0, Stroke::new(2.0, accent()), StrokeKind::Inside);
                        }
                        match clip {
                            Some(k) if !k.is_empty() => {
                                let chosen = (!k.thumb.is_empty()).then(|| thumbs.get(&k.thumb)).flatten();
                                let tex = if chosen.is_some() { chosen } else { match &k.source {
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
                                } };
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
                                    p.text(egui::pos2(rect.right() - 4.0, rect.top() + 3.0), egui::Align2::RIGHT_TOP, badge, egui::FontId::monospace(11.0), accent());
                                }
                            }
                            _ => {
                                p.text(rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), ui.visuals().weak_text_color());
                            }
                        }
                        let stroke = if drag_at.is_some_and(|p| rect.contains(p)) {
                            // Files from the Finder land here (and fill the row to the right).
                            Stroke::new(2.5, Color32::from_rgb(255, 196, 70))
                        } else if active {
                            Stroke::new(2.0, accent())
                        } else if resp.hovered() || resp.dnd_hover_payload::<MediaDrag>().is_some() || resp.dnd_hover_payload::<super::SourceDrag>().is_some() {
                            Stroke::new(1.0, accent().gamma_multiply(0.7))
                        } else {
                            ui.visuals().widgets.noninteractive.bg_stroke
                        };
                        p.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
                        // A muted clip says so on its cell.
                        if clip.is_some_and(|k| k.muted) {
                            let at = rect.right_top() + egui::vec2(-4.0, 4.0);
                            p.text(at, egui::Align2::RIGHT_TOP, icon::SPEAKER_SIMPLE_X, egui::FontId::proportional(12.0), crate::theme::WARN);
                        }
                        if let Some(m) = crate::ui::drop_payload::<MediaDrag>(&resp) {
                            set_cell = Some((idx, c, m.0));
                        }
                        if let Some(d) = crate::ui::drop_payload::<super::SourceDrag>(&resp) {
                            set_source = Some((idx, c, (*d).clone()));
                        }
                        // Flash / piano clips play from the moment the button goes down until it comes up.
                        let momentary = clip.is_some_and(|k| !k.is_empty() && matches!(k.trigger, TriggerMode::Flash | TriggerMode::Piano));
                        if momentary {
                            if pressed && resp.hovered() {
                                triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                picked = Some((layer.id, c));
                                ui.ctx().data_mut(|d| d.insert_temp(held_id, (layer.id, c)));
                            }
                        } else if resp.clicked() && adding && clip.is_some_and(|k| !k.is_empty()) && selected_clip.is_some_and(|s| s != (layer.id, c)) {
                            // Shift / Cmd-click: one more clip to edit together (it doesn't play).
                            match also_clips.iter().position(|x| *x == (layer.id, c)) {
                                Some(k) => {
                                    also_clips.remove(k);
                                }
                                None => also_clips.push((layer.id, c)),
                            }
                        } else if resp.clicked() {
                            also_clips.clear();
                            match clip {
                                Some(k) if !k.is_empty() => {
                                    triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                                    picked = Some((layer.id, c));
                                }
                                _ => use_current = Some((idx, c)),
                            }
                        }
                        mapped_badge(ui, &resp, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
                        let filled = clip.is_some_and(|k| !k.is_empty());
                        let is_movie = clip.is_some_and(|k| matches!(k.source, SourceSpec::Movie { path: Some(_), .. }));
                        resp.right_menu(|ui| {
                            if let Some(act) = clip_editor(ui, layer, c, filled, is_movie) {
                                match act {
                                    ClipAct::Trigger => triggers.push(Trigger::Clip { layer: layer.id, slot: c }),
                                    ClipAct::MatchBeat => beat = Some((layer.id, c)),
                                    ClipAct::StoreCurrent => use_current = Some((idx, c)),
                                    ClipAct::Clear => clear = Some((idx, c)),
                                    ClipAct::Thumbnail => thumb_req = Some((layer.id, c)),
                                }
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
    // A media bin item or a live input dropped on a cell: that cell's clip (the way any clips are placed).
    let dropped = set_cell
        .and_then(|(idx, c, m)| st.comp.media_bin.get(m).map(|item| (idx, c, item.name.clone(), crate::media::spec_for_path(&item.path))))
        .or(set_source.map(|(idx, c, d)| (idx, c, d.name, d.source)));
    if let Some((idx, c, name, source)) = dropped
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        st.comp.place_clips(id, c, vec![(name, source)]);
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
    if let Some((layer, c)) = picked {
        st.selected = Some(layer);
        st.selected_clip = Some((layer, c));
    }
    if let Some((idx, c)) = use_current
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        apply_clip_act(st, id, c, ClipAct::StoreCurrent, now);
    }
    match row_midi {
        Some((id, RowAct::Map(ch, note))) => {
            st.comp.map_clip_row(id, ch, note);
            st.notice = Some(format!("Clip row on MIDI channel {} from note {note}", ch + 1));
        }
        Some((id, RowAct::Shift(by))) => st.comp.shift_clip_row(id, by),
        #[cfg(not(target_arch = "wasm32"))]
        Some((id, RowAct::ExportBank)) => {
            let name = st.comp.layer(id).map_or_else(|| "Clips".to_string(), |l| format!("{} clips", l.name));
            st.dialogs.save_file(crate::dialogs::Purpose::ExportBank(id), ("Clip bank", &["json"]), &format!("{name}.json"));
        }
        #[cfg(not(target_arch = "wasm32"))]
        Some((id, RowAct::ImportBank)) => st.dialogs.pick_file(crate::dialogs::Purpose::ImportBank(id), Some(("Clip bank", &["json"]))),
        // (The web build has no file dialogs.)
        #[allow(unreachable_patterns)]
        _ => {}
    }
    if let Some(id) = fill_row {
        st.quick_on(crate::app::Quick::Fill, Some(id), now);
    }
    if thumb_req.is_some() {
        st.clip_thumb_request = thumb_req;
    }
    if let Some((idx, c)) = clear
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        apply_clip_act(st, id, c, ClipAct::Clear, now);
    }
    for t in triggers {
        st.trigger(t, now);
    }
    for (layer, slot) in releases {
        st.release_clip(layer, slot, now);
    }
}

/// Width of the layer strip at the start of each clip row.
const ROW_HEADER_W: f32 = 176.0;

/// The start of a clip row is the layer itself: its live picture (click = select it, the Inspector opens), its name
/// (menu: autopilot, fill, match the beat), ON and opacity. Returns (fill the row, match every clip's beat).
/// A row's MIDI notes, asked for in its menu: map the row from a note (channel, first note), or shift it.
/// What a row's menu asked for (beyond fill / match the beat).
enum RowAct {
    Map(u8, u8),
    Shift(i32),
    ExportBank,
    ImportBank,
}

fn row_header(ui: &mut egui::Ui, layer: &mut phosphor_engine::LayerState, thumb: Option<egui::TextureId>, selected: &mut Option<u64>, size: egui::Vec2) -> (bool, bool, Option<RowAct>) {
    use phosphor_engine::Autopilot;
    let mut row_midi = None;
    let mut fill = false;
    let mut beat_all = false;
    let is_selected = *selected == Some(layer.id);
    ui.allocate_ui_with_layout(size, egui::Layout::top_down(egui::Align::Min), |ui| {
        ui.set_min_size(size);
        ui.set_max_width(size.x);
        // Live picture: click to select (and edit in the Inspector).
        let has_sound = matches!(layer.source, SourceSpec::Movie { .. });
        let pic_h = (size.y - if has_sound { 56.0 } else { 34.0 }).clamp(36.0, 60.0);
        let (pic, r) = ui.allocate_exact_size(egui::vec2(size.x - 6.0, pic_h), Sense::click());
        let p = ui.painter();
        p.rect_filled(pic, 4.0, Color32::BLACK);
        if let Some(t) = thumb.filter(|_| layer.enabled) {
            paint_texture(p, t, fit_rect(pic, 16.0 / 9.0));
        }
        let border = if is_selected { accent() } else if r.hovered() { ui.visuals().widgets.hovered.bg_stroke.color } else { ui.visuals().widgets.noninteractive.bg_stroke.color };
        p.rect_stroke(pic, 4.0, Stroke::new(if is_selected { 2.0 } else { 1.0 }, border), StrokeKind::Inside);
        if r.on_hover_text("Click to edit this layer (Inspector)").clicked() {
            *selected = Some(layer.id);
        }
        // Show/hide + opacity under the picture; mute + volume for layers with sound.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            super::layers::show_button(ui, layer);
            Level::percent(&mut layer.opacity, "Opacity").modulation(&mut layer.opacity_mod).target(ControlTarget::LayerOpacity(layer.id)).width(size.x - 34.0).show(ui);
        });
        if has_sound {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                super::inspector::mute_button(ui, &mut layer.muted, "this layer");
                super::inspector::volume_bar(ui, layer, size.x - 34.0);
            });
        }
        let name: String = layer.name.chars().take(20).collect();
        ui.menu_button(RichText::new(name).color(ui.visuals().text_color()), |ui| {
            ui.set_min_width(220.0);
            let q = crate::app::Quick::Fill;
            if crate::ui::command(ui, format!("{} {}", q.icon(), q.label())).on_hover_text(q.about()).clicked() {
                fill = true;
                ui.close();
            }
            if layer.clips.iter().any(|c| matches!(c.source, SourceSpec::Movie { path: Some(_), .. }))
                && crate::ui::command(ui, label(icon::MAGIC_WAND, "Match the beat of every movie here")).on_hover_text("Scans each movie clip in this row for its rhythm and locks it to the tempo").clicked()
            {
                beat_all = true;
                ui.close();
            }
            ui.separator();
            ui.label(RichText::new(format!("{} MIDI notes", icon::PIANO_KEYS)).strong());
            hint(ui, "The whole row on a run of notes (a pad grid's row): clip 1 = the first note, clip 2 = the next…");
            let key = ui.id().with(("row_midi", layer.id));
            let (mut ch, mut note): (u8, u8) = ui.data(|d| d.get_temp(key)).unwrap_or((0, 36));
            ui.horizontal(|ui| {
                let mut c1 = ch + 1;
                guarded!(ui, c1, egui::DragValue::new(&mut c1).range(1..=16).prefix("ch "));
                ch = c1 - 1;
                guarded!(ui, note, egui::DragValue::new(&mut note).range(0..=127).prefix("from note "));
                if crate::ui::command(ui, "Map the row").clicked() {
                    row_midi = Some(RowAct::Map(ch, note));
                    ui.close();
                }
            });
            ui.data_mut(|d| d.insert_temp(key, (ch, note)));
            ui.horizontal(|ui| {
                ui.label("Shift its notes");
                for by in [-8, -1, 1, 8] {
                    if crate::ui::small_command(ui, format!("{by:+}")).on_hover_text("Move this row's note mappings (a bank on the controller)").clicked() {
                        row_midi = Some(RowAct::Shift(by));
                    }
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            ui.horizontal(|ui| {
                if crate::ui::command(ui, label(icon::EXPORT, "Export clips…")).on_hover_text("Save this row's clips and their settings as a bank file").clicked() {
                    row_midi = Some(RowAct::ExportBank);
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Import…")).on_hover_text("Replace this row's clips with a saved bank").clicked() {
                    row_midi = Some(RowAct::ImportBank);
                    ui.close();
                }
            });
            let pick = crate::ui::command(ui, label(icon::SLIDERS_HORIZONTAL, "Pick a clip by value…")).on_hover_text("One knob or fader picks among this row's clips (map it: right-click › Listen)");
            map_menu(ui, &pick, ControlTarget::LayerClipPick(layer.id));
            ui.separator();
            ui.label(RichText::new(format!("{} Plays on", icon::STACK)).strong());
            hint(ui, "Share this row's clips over the layers beside it (same group): each clip on the next layer, or on one with nothing playing.");
            choice_buttons(ui, &mut layer.voices, |_, _| {});
            ui.separator();
            ui.label(RichText::new(format!("{} Autopilot", icon::AIRPLANE_TILT)).strong());
            hint(ui, "When this clip is done: play another one by itself, or clear the layer.");
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
        let when = if layer.autopilot_beats > 0.0 { format!("{:.0} beats", layer.autopilot_beats) } else { "at end".into() };
        if let Some(c) = layer.collection.clone() {
            // Playing a collection: say which, and a one-click stop right here.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let r = crate::ui::small_command(ui, RichText::new(icon::STOP).color(accent())).on_hover_text("Stop stepping through the collection (what's showing stays)");
                if r.clicked() {
                    layer.collection = None;
                }
                ui.label(RichText::new(format!("{} {c} · {when}", icon::BOOKMARKS_SIMPLE)).small().color(accent())).on_hover_text("Playing a collection by itself (order and timing: Inspector)");
            });
        } else if layer.autopilot != Autopilot::Off {
            ui.label(RichText::new(format!("{} {} · {when}", icon::AIRPLANE_TILT, layer.autopilot.name())).small().color(accent()));
        }
    });
    (fill, beat_all, row_midi)
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
