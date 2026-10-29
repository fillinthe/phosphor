//! Inspector for the selected layer.

use super::*;

pub(crate) fn inspector(ui: &mut egui::Ui, st: &mut State) {
    // A slice or a Control Surface widget selected last: its settings here (the Studio's / the surface's editors).
    match super::focus(ui, st) {
        super::Focus::Slice => {
            super::properties(ui, st);
            return;
        }
        super::Focus::Widget => {
            if let Some(id) = super::selected_widget(ui.ctx()) {
                let targets = crate::control::all_targets(&st.comp);
                crate::ui::column_scroll(ui).id_salt("inspector_widget").auto_shrink(false).show(ui, |ui| {
                    super::widget_editor(ui, st, id, &targets);
                });
            }
            return;
        }
        super::Focus::Layer => {}
    }
    let aspect = st.aspect();
    let Some(sel) = st.selected.filter(|id| *id == MASTER_ID || st.comp.layers.iter().any(|l| l.id == *id) || st.comp.group(*id).is_some() || st.comp.comp(*id).is_some()) else {
        help(ui, "Select a layer (or MASTER) to edit its source and effects.");
        return;
    };
    let clipboard_len = st.fx_clipboard.len();
    let also = st.also_selected.len();
    let also_clips = st.also_clips.len();
    let mut fx_action: Option<FxAction> = None;
    let mut fx_chain = sel;
    // "Shows on": the outputs, and whether the selection already shows on each.
    let outs: Vec<(u64, String, bool)> = {
        let showing = st.comp.outputs_showing(sel);
        st.comp.outputs.iter().map(|o| (o.id, o.name.clone(), showing.contains(&o.id))).collect()
    };
    let mut send: Option<(u64, bool)> = None;
    if let Some(g) = st.comp.groups.iter_mut().find(|g| g.id == sel) {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        (fx_action, send) = group_inspector(ui, g, thumb, aspect, clipboard_len, &outs);
    } else if st.comp.comp(sel).is_some() {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        let master = (st.comp.width, st.comp.height);
        let c = st.comp.comp_mut(sel).unwrap();
        (fx_action, send) = comp_inspector(ui, c, thumb, master, clipboard_len, &outs);
    } else if sel == MASTER_ID {
        let tex = st.texture;
        crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            thumbnail(ui, tex, egui::vec2(w, w / aspect), true);
            ui.label(RichText::new("MASTER").strong().color(accent()));
            help(ui, "These effects run over the whole mix, after every layer has been blended.");
            ui.separator();
            fx_action = effects_editor(ui, &mut st.comp.master_effects, clipboard_len);
        });
    } else {
        let thumb = st.layer_textures.get(&sel).map(|(t, _)| *t);
        let info = st.media.video_info(sel);
        let position = st.media.position(sel);
        let has_sound = st.media.has_sound(sel);
        let bpm = st.comp.bpm;
        let st_chapters = st.media.chapters(sel);
        let inherited = st.comp.inherited_effects(sel);
        let mut seek_to: Option<f64> = None;
        let mut quick = None;
        let mut match_beat: Option<u64> = None;
        let mut reload: Option<String> = None;
        let mut text_file = false;
        #[cfg(not(target_arch = "wasm32"))]
        let (beat_pending, beat) = {
            let path = st.comp.layers.iter().find(|l| l.id == sel).and_then(|l| crate::app::beat_match::movie_path(&l.source));
            let pending = path.as_deref().is_some_and(|p| st.rhythm.is_pending(p));
            let found = path.and_then(|p| st.rhythm.results.get(&p).cloned()).and_then(|r| r.ok()).map(|r| (r.bpm, r.confidence));
            (pending, found)
        };
        #[cfg(target_arch = "wasm32")]
        let (beat_pending, beat): (bool, Option<(f32, f32)>) = (false, None);
        // The clip picked in the grid, when it's on this layer.
        let clip_slot = st.selected_clip.filter(|(l, _)| *l == sel).map(|(_, c)| c);
        let mut clip_act: Option<(usize, super::ClipAct)> = None;
        let live = st.comp.layer_live(sel);
        let mut go_live = false;
        let layer = st.comp.layer_mut(sel).unwrap();
        let mut top_seek = None;
        crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
            ui.spacing_mut().slider_width = (ui.available_width() - 210.0).max(80.0);
            let w = ui.available_width().min(360.0);
            let pic = thumbnail(ui, thumb, egui::vec2(w, w / aspect), layer.enabled);
            if !live {
                // Watching it before it's in the show.
                let at = pic.rect.left_top() + egui::vec2(6.0, 6.0);
                let g = ui.painter().layout_no_wrap("PREVIEW · not live".into(), egui::FontId::proportional(11.0), Color32::BLACK);
                ui.painter().rect_filled(egui::Rect::from_min_size(at, g.size() + egui::vec2(10.0, 4.0)), 3.0, crate::theme::WARN);
                ui.painter().galley(at + egui::vec2(5.0, 2.0), g, Color32::BLACK);
            }
            // The playhead, speed and opacity right under the picture: watch what they do while changing them.
            if matches!(layer.source, SourceSpec::Movie { .. })
                && let Some((pos, dur)) = position
            {
                top_seek = super::clip_bar::loop_bar(ui, layer, pos, dur, 18.0);
            }
            if matches!(layer.source, SourceSpec::Movie { .. }) {
                super::clip_bar::speed_level(ui, layer, w);
            }
            Level::percent(&mut layer.opacity, "Opacity").modulation(&mut layer.opacity_mod).width(w).target(ControlTarget::LayerOpacity(layer.id)).show(ui);
            crate::ui::toolbar(ui, |ui| {
                let (go, out) = (format!("{} Go live", icon::PLAY), format!("{} Take out", icon::ARROW_DOWN));
                let (text, tip) = if live { (out.clone(), "Fade it out of the show (auto-fade time)") } else { (go.clone(), "Fade it into the show (auto-fade time); until then you only see it here") };
                if crate::ui::steady_action_button(ui, crate::ui::Act::Go, text, &[&go, &out]).on_hover_text(tip).clicked() {
                    go_live = true;
                }
            });
            ui.horizontal(|ui| {
                super::layers::show_button(ui, layer);
                send = outputs_menu(ui, &outs).or(send);
                ui.add(egui::TextEdit::singleline(&mut layer.name).desired_width(f32::INFINITY));
            });
            quick = crate::app::quick_row(ui);
            if let Some(c) = clip_slot.filter(|c| layer.clips.get(*c).is_some_and(|k| !k.is_empty())) {
                let k = &layer.clips[c];
                let is_movie = matches!(k.source, SourceSpec::Movie { path: Some(_), .. });
                let title = format!("{} Clip {} · column {}", icon::SQUARES_FOUR, k.name, c + 1);
                crate::ui::section(ui, "inspector_clip", title, true, |ui| {
                    if also_clips > 0 {
                        ui.horizontal(|ui| hint(ui, format!("Editing {} clips", also_clips + 1)).on_hover_text("How a press plays, sound and own settings go to all of them. Click a clip without Shift / Cmd to edit just one."));
                    }
                    if let Some(a) = super::clip_editor(ui, layer, c, true, is_movie) {
                        clip_act = Some((c, a));
                    }
                });
            }
            if also > 0 {
                ui.horizontal(|ui| hint(ui, format!("Editing {} layers", also + 1)).on_hover_text("Changes here go to all of them (names, sources, clips and effects stay each layer's own). Click a layer without Shift / Cmd to edit just one."));
            }
            ui.separator();
            let media = LayerMedia { info: info.as_deref(), position, has_sound, beat_pending, beat, bpm, chapters: st_chapters, reload: Default::default(), text_file: Default::default(), match_beat: Default::default(), inherited, speed_above: true };
            let (fx, seek) = layer_editor(ui, layer, &media, clipboard_len);
            #[cfg(not(target_arch = "wasm32"))]
            if media.text_file.get() {
                text_file = true;
            }
            if media.reload.get() {
                if let SourceSpec::Movie { path: Some(p), .. } | SourceSpec::Image { path: Some(p), .. } = &layer.source {
                    reload = Some(p.clone());
                }
            }
            if let Some((chain, a)) = fx {
                (fx_chain, fx_action) = (chain, Some(a));
            }
            seek_to = seek.or(top_seek);
            if media.match_beat.get() {
                match_beat = Some(layer.id);
            }
        });
        if go_live {
            st.go_live(sel, ui.input(|i| i.time));
        }
        if let Some((c, a)) = clip_act {
            let now = ui.ctx().input(|i| i.time);
            super::apply_clip_act(st, sel, c, a, now);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(id) = match_beat {
            st.match_beat(crate::media::rhythm::MatchTarget::Layer(id));
        }
        #[cfg(target_arch = "wasm32")]
        let _ = match_beat;
        if let Some(t) = seek_to {
            st.media.seek(sel, t);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if text_file {
            st.dialogs.pick_file(crate::dialogs::Purpose::TextFile(sel), Some(("Text", &["txt", "md", "srt", "lrc", "csv"])));
        }
        if let Some(path) = reload {
            st.media.reload(&st.comp, &path);
            st.thumbs.forget(&path);
        }
        if let Some(q) = quick {
            st.quick_on(q, Some(sel), ui.ctx().input(|i| i.time));
        }
    }
    if let Some((o, on)) = send {
        if on {
            st.comp.send_to_output(sel, o);
        } else {
            st.comp.remove_from_output(sel, o);
        }
        let oname = st.comp.outputs.iter().find(|x| x.id == o).map_or(String::new(), |x| x.name.clone());
        st.notice = Some(if on { format!("Now on \"{oname}\" as a slice (Outputs area to shape it)") } else { format!("No longer on \"{oname}\" on its own") });
    }
    if let Some(action) = fx_action {
        apply_fx_action(st, fx_chain, action);
    }
}

/// What a layer's movie is doing, for `layer_editor` (read before the layer is borrowed).
pub(crate) struct LayerMedia<'a> {
    pub info: Option<&'a str>,
    pub position: Option<(f64, f64)>,
    pub has_sound: bool,
    /// Match the beat: a scan of this movie is running.
    pub beat_pending: bool,
    /// Match the beat: what the last scan of this movie found (BPM, how steady 0..1).
    pub beat: Option<(f32, f32)>,
    /// The tempo (jump buttons step in beats).
    pub bpm: f32,
    /// Chapters in the movie (start in seconds, title).
    pub chapters: Vec<(f64, String)>,
    /// Set by the editor when Reload is pressed (the caller opens the file again).
    pub reload: std::cell::Cell<bool>,
    /// Set by the text editor's "From a file…" (the caller opens the dialog).
    pub text_file: std::cell::Cell<bool>,
    /// Set by the editor when the Match the beat button is pressed (the caller starts the scan).
    pub match_beat: std::cell::Cell<bool>,
    /// Effects reaching this layer from its group, comp and MASTER: (where, which, running).
    pub inherited: Vec<(String, String, bool)>,
    /// The playhead and speed are already shown under the picture (Inspector): Playback doesn't repeat them.
    pub speed_above: bool,
}

/// Everything about one layer below its name: generator / text settings, fade style, movie playback, transform
/// and effect chain (Inspector, and each column of the Layer Windows panel). Returns an effect-chain action and a
/// seek the caller carries out.
/// The effects action comes with the chain it's for (the layer's, or its clip's: `clip_chain`).
pub(crate) fn layer_editor(ui: &mut egui::Ui, layer: &mut LayerState, media: &LayerMedia, clipboard_len: usize) -> (Option<(u64, FxAction)>, Option<f64>) {
    let (info, position, has_sound) = (media.info, media.position, media.has_sound);
    let mut seek_to: Option<f64> = None;
    source_line(ui, layer);

        if let SourceSpec::Generator(id) = &layer.source
            && let Some(desc) = shaders::generator(id)
        {
            crate::ui::heading(ui, format!("Generator · {}", desc.name));
            for (i, p) in desc.params.iter().enumerate() {
                param_slider(ui, &mut layer.generator_values, &mut layer.generator_mods, i, p, Some(ControlTarget::GeneratorParam { layer: layer.id, index: i }));
            }
            crate::ui::field(ui, "Render size", |ui| {
                let sizes: Vec<(f32, String)> = [1.0, 0.75, 0.5, 0.25].iter().map(|s| (*s, format!("{:.0}%", s * 100.0))).collect();
                let mut scale = sizes.iter().map(|(s, _)| *s).min_by(|a, b| (a - layer.render_scale).abs().total_cmp(&(b - layer.render_scale).abs())).unwrap_or(1.0);
                let width = (ui.available_width() - 24.0).clamp(80.0, 200.0);
                let (changed, r) = crate::ui::segmented_list(ui, ("render_scale", layer.id), &mut scale, &sizes, width);
                r.on_hover_text("Draw this generator smaller and scale it up: much lighter on the GPU for heavy shaders (softer picture)");
                if changed {
                    layer.render_scale = scale;
                }
            });
            ui.separator();
        }
        if let SourceSpec::Text(style) = &mut layer.source {
            crate::ui::heading(ui, "Text");
            if text_editor(ui, style, layer.id) {
                media.text_file.set(true);
            }
            // Ready-made movement (it sets the layer's move / size / opacity motion, on the tempo).
            let mut motion = layer.text_motion();
            crate::ui::field(ui, "Moves", |ui| {
                let was = motion;
                let width = (ui.available_width() - 24.0).clamp(80.0, 200.0);
                crate::ui::choice_combo(ui, ("text_motion", layer.id), &mut motion, width, false);
                if motion != was {
                    layer.set_text_motion(motion);
                }
            });
            ui.separator();
        }
        if let Some(name) = layer.collection.clone() {
            ui.horizontal_wrapped(|ui| {
                crate::ui::heading(ui, format!("{} Playing collection · {name}", icon::BOOKMARKS_SIMPLE));
                if crate::ui::small_command(ui, label(icon::STOP, "Stop")).on_hover_text("Keep what's showing; stop stepping through the collection").clicked() {
                    layer.collection = None;
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Order");
                use phosphor_engine::Autopilot;
                let orders: Vec<(Autopilot, String)> = [Autopilot::Next, Autopilot::Previous, Autopilot::Random].into_iter().map(|a| (a, a.name().to_string())).collect();
                segmented(ui, &mut layer.autopilot, &orders, |_, _, _| {});
                let mut at_end = layer.autopilot_beats <= 0.0;
                if segmented(ui, &mut at_end, &[(true, "when the clip ends".to_string()), (false, "after".to_string())], |_, _, _| {}) {
                    layer.autopilot_beats = if at_end { 0.0 } else { 16.0 };
                }
                if layer.autopilot_beats > 0.0 {
                    guarded!(ui, layer.autopilot_beats, egui::DragValue::new(&mut layer.autopilot_beats).range(1.0..=256.0).speed(0.2).suffix(" beats"));
                }
            });
            help(ui, "New files in the collection's folders join as they arrive. Put any clip or file on this layer yourself and it stops following the collection.");
            ui.separator();
        }
        crate::ui::toolbar(ui, |ui| {
            crate::ui::heading(ui, "Fade style");
            choice_combo(ui, ("trans", layer.id), &mut layer.transition_kind, 120.0, false);
            if transition_effect_combo(ui, ("trans_fx", layer.id), layer.transition_kind, &mut layer.transition_effect) {
                layer.transition_values.clear();
            }
            help(ui, "The fade time and Cut / Fade are in the Clips panel's Launch settings.");
        });
        transition_settings(ui, layer);
        crate::ui::field(ui, "Frame rate", |ui| {
            let rates: Vec<(f32, String)> = [(0.0, "Every frame".to_string()), (30.0, "30 a second".into()), (15.0, "15 a second".into()), (10.0, "10 a second".into()), (5.0, "5 a second".into())].into_iter().collect();
            let width = (ui.available_width() - 24.0).clamp(80.0, 200.0);
            crate::ui::segmented_list(ui, ("render_fps", layer.id), &mut layer.render_fps, &rates, width)
                .1
                .on_hover_text("Frames a second this layer is drawn. Fewer: its source and effects are drawn only that often and the picture is held between: saves the GPU, or a choppy look on purpose");
        });
        ui.separator();
        if matches!(layer.source, SourceSpec::Movie { .. }) {
            crate::ui::heading(ui, "Playback");
            // The clip on the layer can keep these (speed, direction, loop, BPM sync) as its own: they come back
            // whenever it's triggered, on any layer (the same switch as the clip's right-click menu).
            if let Some(slot) = layer.active_slot() {
                let current = layer.clip_settings();
                let mut own = layer.clips[slot].own.is_some();
                if crate::ui::toggle(ui, &mut own, "Keep these with the clip")
                    .on_hover_text("Speed, direction (ping-pong, reverse), loop section and BPM sync stay with the clip that's playing and come back each time it's triggered. Off = they belong to the layer")
                    .changed()
                {
                    layer.clips[slot].own = own.then_some(current);
                }
            }
            // What the movie is, on one line (all of it on hover; File info in the Media panel can copy it).
            if let Some(info) = &info {
                ui.horizontal(|ui| hint(ui, *info).on_hover_text(*info));
            }
            if let Some((pos, dur)) = position {
                // The Inspector shows it under the picture already.
                if !media.speed_above {
                    seek_to = super::clip_bar::loop_bar(ui, layer, pos, dur, 18.0);
                }
                let at = (pos / dur.max(1e-6)) as f32;
                crate::ui::toolbar(ui, |ui| {
                    ui.label(RichText::new("Section").small().color(Color32::GRAY));
                    let r = crate::ui::command(ui, "In").on_hover_text("Loop starts here (drag the yellow marks on the bar, or right-click to map)");
                    map_menu(ui, &r, ControlTarget::LayerLoopIn(layer.id));
                    if r.clicked() {
                        layer.loop_in = at.min(layer.loop_out - 0.01).max(0.0);
                    }
                    let r = crate::ui::command(ui, "Out").on_hover_text("Loop ends here");
                    map_menu(ui, &r, ControlTarget::LayerLoopOut(layer.id));
                    if r.clicked() {
                        layer.loop_out = at.max(layer.loop_in + 0.01).min(1.0);
                        layer.loop_beats = 0.0;
                    }
                    let beats = if layer.loop_beats > 0.0 { format!("{} beats", layer.loop_beats) } else { "Beats: off".into() };
                    crate::ui::dropdown(ui, ("loop_beats", layer.id), 84.0).selected_text(beats).show_ui(ui, |ui| {
                        crate::ui::pick_value(ui, &mut layer.loop_beats, 0.0, "Off (use Out)");
                        for b in [0.5, 1.0, 2.0, 4.0, 8.0, 16.0, 32.0] {
                            crate::ui::pick_value(ui, &mut layer.loop_beats, b, format!("{b} beats"));
                        }
                    }).response.on_hover_text("Loop a number of beats from In, at the current tempo");
                    if (layer.loop_in, layer.loop_out, layer.loop_beats) != (0.0, 1.0, 0.0) && crate::ui::small_command(ui, "Clear").on_hover_text("Play the whole clip").clicked() {
                        (layer.loop_in, layer.loop_out, layer.loop_beats) = (0.0, 1.0, 0.0);
                    }
                });
            }
            crate::ui::toolbar(ui, |ui| {
                // Play / pause makes something happen: an action button (not a toggle pill).
                let (pause, play) = (label(icon::PAUSE, "Pause"), label(icon::PLAY, "Play"));
                let text = if layer.playing { pause.clone() } else { play.clone() };
                if steady_action_button(ui, Act::Go, text, &[&pause, &play]).clicked() {
                    layer.playing = !layer.playing;
                }
                if crate::ui::command(ui, icon::ARROW_CLOCKWISE).on_hover_text("Reload: open the file again from disk (after you've changed it in another app)").clicked() {
                    media.reload.set(true);
                }
                if crate::ui::command(ui, icon::SKIP_BACK).on_hover_text("Back to the start").clicked() {
                    seek_to = Some(0.0);
                }
                toggle(ui, &mut layer.looping, "Loop");
                if !layer.looping {
                    toggle(ui, &mut layer.black_at_end, "Black at end").on_hover_text("When the movie ends, show nothing instead of holding its last frame");
                }
            });
            ui.horizontal_wrapped(|ui| {
                choice_mapped(ui, &mut layer.direction, ControlTarget::LayerDirection(layer.id));
            });
            if let Some((pos, dur)) = position {
                crate::ui::toolbar(ui, |ui| {
                    ui.label("Jump");
                    let step = layer.jump_secs(Some(dur), media.bpm);
                    for (forward, glyph, tip) in [(false, icon::REWIND, "Jump back one step"), (true, icon::FAST_FORWARD, "Jump forward one step")] {
                        let r = crate::ui::command(ui, glyph).on_hover_text(tip);
                        if r.clicked() {
                            seek_to = Some(if forward { pos + step } else { pos - step }.clamp(0.0, dur));
                        }
                        map_menu(ui, &r, ControlTarget::LayerJump { layer: layer.id, forward });
                    }
                    if !media.chapters.is_empty() {
                        ui.menu_button(label(icon::BOOKMARKS_SIMPLE, "Chapters"), |ui| {
                            for (at, title) in &media.chapters {
                                if crate::ui::command(ui, format!("{}  {title}", crate::app::format_tc(*at))).clicked() {
                                    seek_to = Some(*at);
                                    ui.close();
                                }
                            }
                        });
                    }
                    guarded!(ui, layer.jump, egui::DragValue::new(&mut layer.jump).range(0.05..=64.0).speed(0.05).max_decimals(2)).on_hover_text("How far each jump goes");
                    let unit = if layer.jump_in_seconds { "seconds" } else if layer.jump == 1.0 { "beat" } else { "beats" };
                    if crate::ui::small_command(ui, unit).on_hover_text("Click: step in seconds or in beats").clicked() {
                        layer.jump_in_seconds = !layer.jump_in_seconds;
                    }
                });
            }
            if let Some((pos, dur)) = position {
                // Scratch: drag along the strip to move through the movie (a screen-width drag = 4 seconds), endlessly.
                // The movie holds still while you scratch (measured from where the drag began, so seeks that
                // haven't landed yet don't hold it back) and plays on when you let go.
                ui.horizontal(|ui| {
                    ui.label("Scratch");
                    let (rect, r) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(260.0), 18.0), Sense::drag());
                    let grooves_id = egui::Id::new(("scratch_grooves", layer.id));
                    let drag_id = egui::Id::new(("scratch_drag", layer.id));
                    let mut grooves: f32 = ui.data(|d| d.get_temp(grooves_id)).unwrap_or(0.0);
                    if r.drag_started() {
                        // (where it began, whether it was playing, how far the hand has gone)
                        ui.data_mut(|d| d.insert_temp(drag_id, (pos, layer.playing, 0.0f64)));
                        layer.playing = false;
                    }
                    if r.dragged()
                        && let Some((start, was_playing, gone)) = ui.data(|d| d.get_temp::<(f64, bool, f64)>(drag_id))
                    {
                        let dx = r.drag_delta().x;
                        if dx != 0.0 {
                            let gone = gone + dx as f64;
                            grooves += dx;
                            ui.data_mut(|d| {
                                d.insert_temp(drag_id, (start, was_playing, gone));
                                d.insert_temp(grooves_id, grooves);
                            });
                            seek_to = Some((start + gone / 1600.0 * 4.0).rem_euclid(dur.max(1e-3)));
                        }
                    }
                    if r.drag_stopped()
                        && let Some((_, was_playing, _)) = ui.data_mut(|d| d.remove_temp::<(f64, bool, f64)>(drag_id))
                    {
                        layer.playing = was_playing;
                    }
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, ui.visuals().extreme_bg_color);
                    // Ridges like a record's grooves: they move only with your hand.
                    let mut x = rect.left() + grooves.rem_euclid(12.0) - 12.0;
                    while x < rect.right() {
                        if x > rect.left() {
                            p.line_segment([egui::pos2(x, rect.top() + 3.0), egui::pos2(x, rect.bottom() - 3.0)], egui::Stroke::new(1.0, ui.visuals().weak_text_color()));
                        }
                        x += 12.0;
                    }
                    if r.hovered() || r.dragged() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
                    }
                    r.on_hover_text("Drag to scratch through the movie (wraps round at the ends)");
                });
            }
            if !(media.speed_above && layer.sync_beats <= 0.0) {
                ui.horizontal(|ui| {
                    ui.add_enabled_ui(layer.sync_beats <= 0.0, |ui| {
                        Level::range(&mut layer.rate, 0.1, 4.0, "Speed").log().default(1.0).target(ControlTarget::LayerSpeed(layer.id)).show(ui);
                    });
                });
            }
            crate::ui::toolbar(ui, |ui| {
                let mut sync = layer.sync_beats > 0.0;
                if toggle(ui, &mut sync, label(icon::METRONOME, "BPM sync")).on_hover_text("Stretch the clip (or its loop) to a number of beats; it speeds up and slows down with the tempo").changed() {
                    layer.sync_beats = if sync { 16.0 } else { 0.0 };
                }
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(path) = crate::app::beat_match::movie_path(&layer.source) {
                    let _ = path;
                    let pending = media.beat_pending;
                    let text = if pending { format!("{} Listening to the picture…", icon::HOURGLASS) } else { format!("{} Match the beat", icon::MAGIC_WAND) };
                    if crate::ui::command_if(ui, !pending, text).on_hover_text("Scans the movie's motion for its own rhythm, then starts the loop on its first hit and stretches it so every hit lands on a beat").clicked() {
                        media.match_beat.set(true);
                    }
                    if let Some((bpm, steady)) = media.beat {
                        if steady >= crate::media::rhythm::STEADY {
                            hint(ui, format!("visual beat {bpm:.0} BPM · {:.0}% steady", steady * 100.0));
                        } else {
                            hint(ui, format!("no steady visual beat ({:.0}%): not locked", steady * 100.0));
                        }
                        if sync {
                            if crate::ui::small_command(ui, "÷2").on_hover_text("Hits every second beat (half as many beats)").clicked() {
                                layer.sync_beats = (layer.sync_beats / 2.0).round().max(1.0);
                            }
                            if crate::ui::small_command(ui, "×2").on_hover_text("Two hits per beat (twice the beats)").clicked() {
                                layer.sync_beats = (layer.sync_beats * 2.0).round().min(256.0);
                            }
                        }
                    }
                }
                if sync {
                    guarded!(ui, layer.sync_beats, egui::DragValue::new(&mut layer.sync_beats).range(1.0..=256.0).speed(0.1).suffix(" beats"));
                    let counts: Vec<(f32, String)> = [4.0f32, 8.0, 16.0, 32.0].into_iter().map(|b| (b, format!("{b:.0}"))).collect();
                    segmented(ui, &mut layer.sync_beats, &counts, |_, _, _| {});
                    if layer.loop_beats > 0.0 {
                        hint(ui, "off while a beat loop is set");
                    }
                }
            });
            if has_sound {
                sound_row(ui, layer);
            }
            ui.separator();
        }
    transform_editor(ui, layer);
    mixer_editor(ui, layer);
    ui.separator();
    // Effects that also act on this layer from above its own chain (they don't show in the list below).
    if !media.inherited.is_empty() {
        crate::ui::toolbar(ui, |ui| {
            hint(ui, "Also:").on_hover_text("Effects also acting on this layer, from its group, comp or MASTER");
            for (scope, name, running) in &media.inherited {
                let text = format!("{name} ({scope})");
                if *running {
                    ui.label(RichText::new(text).small().color(accent())).on_hover_text("Runs over this layer from there: select that group, comp or MASTER to change or remove it");
                } else {
                    hint(ui, text).on_hover_text("Bypassed or off");
                }
            }
        });
    }
    // Clips that put their own effects back when they play (an effect taken off here returns with them).
    let bringing = layer.clips_with_effects();
    if bringing > 0 {
        crate::ui::toolbar(ui, |ui| {
            let what = if bringing == 1 { "1 clip brings its effects".to_string() } else { format!("{bringing} clips bring their effects") };
            hint(ui, what).on_hover_text("These clips put their own effects back when they play. Their speed and loop still come back; turn it off for one clip in its right-click menu (Brings its effects)");
            if crate::ui::small_command(ui, "Stop that").on_hover_text("These clips leave the layer's effects as they are from now on").clicked() {
                layer.clips_forget_effects();
            }
        });
    }
    // The playing clip's (or file's) own effects: they come and go with it, before the layer's.
    let mut action = None;
    let clip_chain = match &layer.clip_effects_from {
        phosphor_engine::EffectsOwner::Clip(_) => Some(("Clip effects", "Effects added here belong to the clip: they come back whenever it plays, on any layer. The layer's effects below stay on the layer.")),
        phosphor_engine::EffectsOwner::File(_) => Some(("File effects", "Effects added here belong to this file: they come back whenever it plays, on any layer or clip made from it. The layer's effects below stay on the layer.")),
        phosphor_engine::EffectsOwner::None => None,
    };
    if let Some((heading, empty)) = clip_chain {
        if let Some(a) = super::effects::effects_list(ui, heading, empty, &mut layer.clip_effects, clipboard_len) {
            action = Some((phosphor_engine::clip_chain(layer.id), a));
        }
        ui.separator();
    }
    if let Some(a) = effects_editor(ui, &mut layer.effects, clipboard_len) {
        action = Some((layer.id, a));
    }
    (action, seek_to)
}

/// What the layer is showing and where it comes from: generated, a file on disk (its path, Show in Finder, and a
/// warning when it's missing), a live input, another layer, text; and the clip slot it was triggered from.
pub(crate) fn source_line(ui: &mut egui::Ui, layer: &LayerState) {
    let (glyph, kind, detail): (&str, &str, Option<String>) = match &layer.source {
        SourceSpec::None => (icon::PROHIBIT, "No source", None),
        // (Its name is the heading of its settings just below.)
        SourceSpec::Generator(_) => (icon::SPARKLE, "Generated", None),
        SourceSpec::Movie { name, path } if path.as_deref().is_some_and(|p| p.starts_with("http")) => (icon::LINK, "Movie from a link (not saved)", Some(name.clone())),
        SourceSpec::Movie { name, path } => (icon::FILM_STRIP, "Movie file", Some(path.clone().unwrap_or_else(|| name.clone()))),
        SourceSpec::Image { name, path } => (icon::IMAGE, "Picture file", Some(path.clone().unwrap_or_else(|| name.clone()))),
        SourceSpec::Live { name, format, url } => {
            let what = match format.as_str() {
                "avfoundation" => "Camera / capture",
                "screen" => "Screen capture",
                "syphon" => "Syphon",
                "ndi" => "NDI",
                "lavfi" => "Test pattern (made by FFmpeg)",
                _ => "Live stream",
            };
            (icon::VIDEO_CAMERA, what, Some(if url.is_empty() || url == name { name.clone() } else { format!("{name} · {url}") }))
        }
        SourceSpec::Feed { name, .. } => (icon::ARROW_BEND_DOWN_RIGHT, "Another layer's picture", Some(name.clone())),
        SourceSpec::Text(_) => (icon::TEXT_T, "Text", None),
    };
    // Triggered from the clip grid: which slot.
    let clip = layer.active_clip.and_then(|id| layer.clips.iter().position(|c| c.id == id));
    ui.horizontal_wrapped(|ui| {
        crate::ui::heading(ui, format!("{glyph} {kind}"));
        if let Some(i) = clip {
            hint(ui, format!("· clip {} (column {})", layer.clips[i].name.trim().is_empty().then(|| (i + 1).to_string()).unwrap_or_else(|| layer.clips[i].name.clone()), i + 1));
        }
    });
    if let Some(d) = detail {
        let is_file = matches!(&layer.source, SourceSpec::Movie { path: Some(p), .. } | SourceSpec::Image { path: Some(p), .. } if !p.starts_with("http"));
        crate::ui::toolbar(ui, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            if is_file {
                // (Cached and checked off the show's thread: a slow or unplugged drive mustn't stall drawing.)
                if crate::media::present::file_present(&d) == Some(false) {
                    ui.colored_label(WARN, format!("{} missing", icon::WARNING)).on_hover_text("The file isn't at this path any more: File › Find Missing Media");
                }
                if crate::ui::small_command(ui, icon::FOLDER_OPEN).on_hover_text("Show in Finder").clicked() {
                    crate::os::reveal(std::path::Path::new(&d));
                }
            }
            #[cfg(target_arch = "wasm32")]
            let _ = is_file;
            let path = ui.add(egui::Label::new(RichText::new(&d).small().color(Color32::GRAY)).truncate().sense(Sense::click())).on_hover_text(format!("{d}\nRight-click: copy the name or the path"));
            // For reports: what this layer is playing and where it is.
            path.right_menu(|ui| {
                let file_name = std::path::Path::new(&d).file_name().map_or_else(|| d.clone(), |n| n.to_string_lossy().into_owned());
                if crate::ui::command(ui, label(icon::COPY, "Copy file name")).clicked() {
                    ui.ctx().copy_text(file_name);
                    ui.close();
                }
                if crate::ui::command(ui, label(icon::COPY, "Copy full path")).clicked() {
                    ui.ctx().copy_text(d.clone());
                    ui.close();
                }
            });
        });
    }
    ui.separator();
}

/// "Blend shader": blend the layer onto the layers below with your own blend shader (`// @kind blend`) or an FFGL
/// mixer instead of its blend mode, and its settings. Only shown when there are some.
fn mixer_editor(ui: &mut egui::Ui, layer: &mut LayerState) {
    let mixers = shaders::all_mixers();
    if mixers.is_empty() {
        return;
    }
    let current = layer.mixer.as_deref().and_then(shaders::mixer);
    let title = match current {
        Some(d) => label(icon::INTERSECT, &format!("Blend shader: {}", d.name)),
        None => label(icon::INTERSECT, "Blend shader"),
    };
    crate::ui::section(ui, ("mixer", layer.id), title, false, |ui| {
        help(ui, "Your own blend shader (Shader Editor, kind blend) or an FFGL mixer puts this layer on the layers below instead of its blend mode; the layer's opacity still applies.");
        let mut pick = layer.mixer.clone();
        crate::ui::dropdown(ui, ("mixer_pick", layer.id), 200.0).selected_text(current.map_or("None (blend mode)", |d| d.name)).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut pick, None, "None (blend mode)");
            for d in &mixers {
                crate::ui::pick_value(ui, &mut pick, Some(d.id.to_string()), d.name);
            }
        });
        if pick != layer.mixer {
            layer.mixer_values = pick.as_deref().and_then(shaders::mixer).map(|d| d.defaults()).unwrap_or_default();
            layer.mixer = pick;
        }
        if let Some(d) = layer.mixer.as_deref().and_then(shaders::mixer) {
            layer.mixer_values.resize(d.params.len(), 0.0);
            let id = layer.id;
            for (i, (p, v)) in d.params.iter().zip(layer.mixer_values.iter_mut()).enumerate() {
                if p.name.eq_ignore_ascii_case("opacity") {
                    continue;
                }
                ui.horizontal(|ui| Level::range(v, p.min, p.max, p.name).default(p.default).target(ControlTarget::MixerParam { layer: id, index: i }).show(ui));
            }
        }
    });
}

/// Position, scale, rotation and flips of a layer (all mappable; scale and rotation can be modulated).
/// One of `LayerTransform::PARTS` as a slider with its own control (mappable, keyable on the timeline).
fn part_row(ui: &mut egui::Ui, t: &mut phosphor_engine::LayerTransform, layer: u64, part: usize, tip: &str) {
    let p = &phosphor_engine::LayerTransform::PARTS[part];
    ui.horizontal(|ui| {
        let target = ControlTarget::LayerTransform { layer, part: part as u8 };
        let value = t.part_mut(part);
        let level = if part >= 5 { Level::percent(value, p.name).max(p.max) } else { Level::range(value, p.min, p.max, p.name) };
        let level = level.default(p.default).target(target);
        let level = if p.log { level.log() } else { level };
        let level = if tip.is_empty() { level } else { level.tip(tip) };
        level.show(ui);
    });
}

fn transform_editor(ui: &mut egui::Ui, layer: &mut LayerState) {
    let id = layer.id;
    let t = &mut layer.transform;
    let title = if t.is_identity() { label(icon::ARROWS_OUT_CARDINAL, "Transform") } else { label(icon::ARROWS_OUT_CARDINAL, "Transform (moved)") };
    crate::ui::section(ui, ("transform", id), title, false, |ui| {
        let row = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, text: &str, target: ControlTarget, m: Option<&mut Option<Modulation>>, log: bool, reset: f32| {
            ui.horizontal(|ui| {
                let mut level = Level::range(value, *range.start(), *range.end(), text).default(reset).target(target);
                if log {
                    level = level.log();
                }
                if let Some(m) = m {
                    level = level.modulation(m);
                }
                level.show(ui);
            });
        };
        row(ui, &mut t.x, -1.0..=1.0, "X", ControlTarget::LayerX(id), Some(&mut t.x_mod), false, 0.0);
        row(ui, &mut t.y, -1.0..=1.0, "Y", ControlTarget::LayerY(id), Some(&mut t.y_mod), false, 0.0);
        row(ui, &mut t.scale, 0.05..=4.0, "Scale", ControlTarget::LayerScale(id), Some(&mut t.scale_mod), true, 1.0);
        row(ui, &mut t.rotation, -180.0..=180.0, "Rotation °", ControlTarget::LayerRotation(id), Some(&mut t.rotation_mod), false, 0.0);
        // Scale X / Y apart (with Scale above, a multiplier), depth / tilts in 3D (perspective); crop comes later.
        let tips = ["", "", "Further away (smaller, in perspective) or closer", "Tip it forward / back in 3D", "Turn it left / right in 3D"];
        for (i, tip) in tips.iter().enumerate() {
            part_row(ui, t, id, i, tip);
        }
        // Corner pin: the picture into any four points (drag them on the preview, or type them here).
        let mut pinned = t.corners.is_some();
        if toggle(ui, &mut pinned, label(icon::PERSPECTIVE, "Corner pin")).on_hover_text("Stretch the picture into four corners you drag on the preview (perspective); replaces move / scale / rotate").changed() {
            t.corners = pinned.then_some([[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]]);
        }
        if let Some(c) = &mut t.corners {
            for (k, name) in ["Top left", "Top right", "Bottom right", "Bottom left"].into_iter().enumerate() {
                crate::ui::toolbar(ui, |ui| {
                    ui.label(RichText::new(name).small());
                    for axis in 0..2 {
                        let r = guarded!(ui, c[k][axis], egui::DragValue::new(&mut c[k][axis]).speed(0.002).range(-1.0..=2.0).fixed_decimals(3).prefix(if axis == 0 { "x " } else { "y " }));
                        map_menu(ui, &r, ControlTarget::LayerCorner { layer: id, point: (k * 2 + axis) as u8 });
                    }
                });
            }
        }
        // Crop: one bar per edge.
        for i in 5..9 {
            part_row(ui, t, id, i, "");
        }
        crate::ui::toolbar(ui, |ui| {
            toggle(ui, &mut t.flip_x, format!("Flip {}", icon::ARROWS_LEFT_RIGHT));
            toggle(ui, &mut t.flip_y, format!("Flip {}", icon::ARROWS_DOWN_UP));
            if crate::ui::command_if(ui, !t.is_identity(), label(icon::ARROW_COUNTER_CLOCKWISE, "Reset")).clicked() {
                *t = Default::default();
            }
        });
        help(ui, "Tip: put an LFO on Rotation (~ button) to spin a layer, or the audio level on Scale to pulse it.");
    });
}

/// Speaker toggle: sound on / off for a layer or a clip (keeps the volume). Shared by the Inspector and clip rows.
pub(crate) fn mute_button(ui: &mut egui::Ui, muted: &mut bool, what: &str) -> egui::Response {
    let (glyph, color) = if *muted { (icon::SPEAKER_SIMPLE_X, crate::theme::WARN) } else { (icon::SPEAKER_SIMPLE_HIGH, ui.visuals().text_color()) };
    let r = crate::ui::small_command(ui, RichText::new(glyph).color(color)).on_hover_text(if *muted { format!("Sound off for {what}: click to turn it on") } else { format!("Mute {what}") });
    if r.clicked() {
        *muted = !*muted;
    }
    r
}

/// Movie sound: volume (mappable) and whether it fades with the layer's opacity.
fn sound_row(ui: &mut egui::Ui, layer: &mut LayerState) {
    ui.horizontal(|ui| {
        mute_button(ui, &mut layer.muted, "this layer");
        let w = ui.available_width();
        volume_bar(ui, layer, w);
    });
    crate::ui::toggle(ui, &mut layer.volume_follows_opacity, "Sound follows opacity").on_hover_text("Fading the layer (or its group, or the crossfader) fades its sound too");
    // Another sound device (headphones for cueing, a second interface…): the device list is asked for only when the
    // menu opens.
    #[cfg(not(target_arch = "wasm32"))]
    crate::ui::toolbar(ui, |ui| {
        ui.label("Sound device");
        let current = layer.sound_device.clone().unwrap_or_else(|| "Main output".into());
        crate::ui::dropdown(ui, ("sound_device", layer.id), 180.0).selected_text(current).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut layer.sound_device, None, "Main output");
            for d in crate::media::video::sound::output_devices() {
                crate::ui::pick_value(ui, &mut layer.sound_device, Some(d.clone()), d);
            }
        })
        .response
        .on_hover_text("Play this layer's sound on another device (cue headphones, a second interface). Streams and recordings only hear the main output.");
    });
    // A multichannel sound output: which pair of its outputs this layer plays on.
    #[cfg(not(target_arch = "wasm32"))]
    {
        let pairs = (crate::media::video::sound::output_channels() / 2).max(1) as u8;
        if pairs > 1 || layer.sound_pair > 0 {
            let name = |p: u8| format!("Outputs {}-{}", p as u32 * 2 + 1, p as u32 * 2 + 2);
            crate::ui::toolbar(ui, |ui| {
                ui.label("Plays on");
                crate::ui::dropdown(ui, ("sound_pair", layer.id), 120.0).selected_text(name(layer.sound_pair)).show_ui(ui, |ui| {
                    for p in 0..pairs.max(layer.sound_pair + 1) {
                        crate::ui::pick_value(ui, &mut layer.sound_pair, p, name(p));
                    }
                });
            });
        }
    }
}

/// A layer's volume (to 200%; the mark is 100%).
pub(crate) fn volume_bar(ui: &mut egui::Ui, layer: &mut LayerState, width: f32) {
    let tip = if layer.muted { "Volume (muted: click the speaker to hear it)" } else { "Volume" };
    Level::percent(&mut layer.volume, "Volume").max(2.0).target(ControlTarget::LayerVolume(layer.id)).width(width).tip(tip).show(ui);
}

/// Temp-data key: a text layer's city clock asks for a place, `(layer, name)` (the app looks it up: `zone_requests`).
pub(crate) const ZONE_REQUEST: &str = "phosphor_zone_request";

/// Starts any city-clock lookup a text editor asked for. Once a frame.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn zone_requests(ctx: &egui::Context, st: &mut State) {
    if let Some((layer, name)) = ctx.data_mut(|d| d.remove_temp::<(u64, String)>(egui::Id::new(ZONE_REQUEST))) {
        st.feeds.find_zone(layer, &name);
    }
    if let Some(p) = st.feeds.take_problems().pop() {
        st.notice = Some(p);
    }
}

/// Edits a text layer: the words, font (searchable list of built-in and installed fonts), size, colour,
/// alignment, outline and line spacing. Changes redraw the layer in the background.
/// Returns true when "From a file…" was pressed (the caller opens the file dialog).
pub(crate) fn text_editor(ui: &mut egui::Ui, style: &mut phosphor_engine::TextStyle, id: u64) -> bool {
    let mut from_file = false;
    // Live: the words follow the clock.
    crate::ui::field(ui, "Live", |ui| {
        use phosphor_engine::LiveText;
        let kinds = [
            (LiveText::Off, "Off"),
            (LiveText::Clock, "Clock"),
            (LiveText::ClockAt(0), "City clock"),
            (LiveText::Countdown(18 * 3600), "Countdown"),
            (LiveText::Stopwatch, "Stopwatch"),
            (LiveText::Headlines(8), "Headlines"),
            (LiveText::Code, "QR code seen"),
            (LiveText::Read, "Text seen"),
        ];
        // (A list: eight buttons wrapped into a tall column in a narrow Inspector.)
        let same = |k: &LiveText| std::mem::discriminant(&style.live) == std::mem::discriminant(k);
        let from_designer = "A number (Designer)";
        let current = kinds.iter().find(|(k, _)| same(k)).map_or(from_designer, |(_, n)| *n);
        let mut picked = None;
        crate::ui::dropdown(ui, ("text_live", id), (ui.available_width() - 24.0).clamp(80.0, 200.0)).selected_text(current).show_ui(ui, |ui| {
            for (k, name) in &kinds {
                if crate::ui::pick_item(ui, same(k), *name).clicked() {
                    picked = Some(*k);
                }
            }
        });
        if let Some(k) = picked {
            style.live = k;
            style.live_start = ui.input(|i| i.time);
        }
        match &mut style.live {
            LiveText::Countdown(at) => {
                let mut minutes = (*at / 60) as f64;
                if guarded!(ui, minutes, egui::DragValue::new(&mut minutes).range(0.0..=1439.0).speed(1.0).prefix("to ").custom_formatter(|v, _| format!("{:02}:{:02}", v as u32 / 60, v as u32 % 60)).custom_parser(|s| {
                    let (h, m) = s.trim().split_once(':')?;
                    Some((h.trim().parse::<u32>().ok()?.min(23) * 60 + m.trim().parse::<u32>().ok()?.min(59)) as f64)
                }))
                .on_hover_text("Counts down to this time of day")
                .changed()
                {
                    *at = minutes as u32 * 60;
                }
            }
            LiveText::Stopwatch => {
                if crate::ui::small_command(ui, label(icon::ARROW_COUNTER_CLOCKWISE, "Restart")).clicked() {
                    style.live_start = ui.input(|i| i.time);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            LiveText::ClockAt(_) => {
                // A town's name: looked up (where it is, then its time zone) by the app (`ZONE_REQUEST`).
                let key = egui::Id::new(("zone_name", id));
                let mut name: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
                let shown = if style.live_source.is_empty() { "A town or city".to_string() } else { style.live_source.clone() };
                let r = crate::ui::text_field(ui, &mut name, &shown, Some(140.0));
                if ((r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) || crate::ui::small_command(ui, icon::MAGNIFYING_GLASS).on_hover_text("Find this place and its time").clicked()) && !name.trim().is_empty() {
                    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(ZONE_REQUEST), (id, name.trim().to_string())));
                    name.clear();
                }
                ui.ctx().data_mut(|d| d.insert_temp(key, name));
                if !style.live_source.is_empty() {
                    hint(ui, format!("Time in {}", style.live_source));
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            LiveText::Headlines(secs) => {
                let current = crate::live_data::NEWS.iter().find(|(_, u)| *u == style.live_source).map_or_else(|| if style.live_source.is_empty() { "Pick a news feed".to_string() } else { "Your feed".to_string() }, |(n, _)| n.to_string());
                crate::ui::dropdown(ui, ("news_feed", id), 150.0).selected_text(current).show_ui(ui, |ui| {
                    for (name, url) in crate::live_data::NEWS {
                        if crate::ui::pick_item(ui, style.live_source == url, name).clicked() {
                            style.live_source = url.to_string();
                        }
                    }
                });
                crate::ui::text_field(ui, &mut style.live_source, "or a feed address (RSS / Atom)", Some(180.0));
                guarded!(ui, *secs, egui::DragValue::new(secs).range(2..=120).suffix(" s each")).on_hover_text("How long each headline shows");
            }
            _ => {}
        }
    });
    // In parts: one line / sentence / word at a time, stepped by the buttons (or a controller).
    crate::ui::field(ui, "Parts", |ui| {
        choice_combo(ui, ("chunk_by", id), &mut style.chunk_by, 100.0, true);
        if crate::ui::small_command(ui, label(icon::FILE_TEXT, "From a file…")).on_hover_text("Lyrics, a script, a list: shown a part at a time").clicked() {
            from_file = true;
        }
        if style.parts.is_empty() {
            if crate::ui::small_command(ui, "Split what's typed").on_hover_text("Cut the text below into parts to show one at a time").clicked() {
                let text = style.text.clone();
                let by = style.chunk_by;
                style.set_parts(&text, by);
            }
        } else {
            for (forward, glyph) in [(false, icon::CARET_LEFT), (true, icon::CARET_RIGHT)] {
                let r = crate::ui::small_command(ui, glyph).on_hover_text(if forward { "Next part" } else { "Previous part" });
                if r.clicked() {
                    style.step_part(forward);
                }
                map_menu(ui, &r, ControlTarget::LayerTextStep { layer: id, forward });
            }
            hint(ui, format!("{} / {}", style.part + 1, style.parts.len()));
            if crate::ui::small_command(ui, icon::X).on_hover_text("Back to plain text (keeps the part showing)").clicked() {
                style.parts.clear();
            }
        }
    });
    use phosphor_engine::TextAlign;
    ui.add(egui::TextEdit::multiline(&mut style.text).desired_rows(3).desired_width(f32::INFINITY).hint_text("Type your text (Enter for a new line)"));
    // (Font, colour and alignment wrap in a narrow Inspector.)
    crate::ui::toolbar(ui, |ui| {
        let search_id = egui::Id::new(("font_search", id));
        let mut search: String = ui.ctx().data(|d| d.get_temp(search_id)).unwrap_or_default();
        crate::ui::dropdown(ui, ("font", id), 170.0).selected_text(crate::text::font_name(&style.font)).height(320.0).show_ui(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut search).hint_text("Search fonts").desired_width(180.0)).request_focus();
            let q = search.to_lowercase();
            for f in crate::text::fonts().iter().filter(|f| q.is_empty() || f.name.to_lowercase().contains(&q)).take(400) {
                crate::ui::pick_value(ui, &mut style.font, f.id.clone(), &f.name);
            }
        });
        ui.ctx().data_mut(|d| d.insert_temp(search_id, search));
        let mut rgb = [style.color[0], style.color[1], style.color[2]].map(|v| v as f32 / 255.0);
        if crate::ui::colour_button(ui, &mut rgb) {
            for k in 0..3 {
                style.color[k] = (rgb[k] * 255.0).round() as u8;
            }
        }
        let aligns = [(TextAlign::Left, icon::TEXT_ALIGN_LEFT.to_string()), (TextAlign::Center, icon::TEXT_ALIGN_CENTER.to_string()), (TextAlign::Right, icon::TEXT_ALIGN_RIGHT.to_string())];
        segmented(ui, &mut style.align, &aligns, |_, r, a| {
            r.clone().on_hover_text(match a {
                TextAlign::Left => "Left",
                TextAlign::Center => "Centre",
                TextAlign::Right => "Right",
            });
        });
    });
    // Size, outline, line spacing: controls of their own (mappable, keyable on the timeline, OSC).
    for (i, part) in phosphor_engine::TextStyle::PARTS.iter().enumerate() {
        ui.horizontal(|ui| {
            let level = Level::range(style.part_mut(i), part.min, part.max, part.name).default(part.default).target(ControlTarget::TextPart { layer: id, part: i as u8 });
            if i == 0 { level.tip("Text height, as a share of the picture's height") } else { level }.show(ui);
        });
    }
    help(ui, "Move, scale or animate it with effects (Zoom/Rotate, Tile, Wave…) and the layer's blend mode.");
    from_file
}

/// A shader transition's own settings (all but its progress), as bars; "Reset" goes back to its defaults.
fn transition_settings(ui: &mut egui::Ui, layer: &mut LayerState) {
    use phosphor_engine::{TransitionKind, shaders};
    if layer.transition_kind != TransitionKind::Shader {
        return;
    }
    let Some(desc) = shaders::transition(&layer.transition_effect) else { return };
    let progress = shaders::progress_index(desc);
    if desc.params.len() <= usize::from(progress.is_some()) {
        return;
    }
    if layer.transition_values.len() != desc.params.len() {
        layer.transition_values = desc.defaults();
    }
    crate::ui::subsection(ui, ("trans_settings", layer.id), format!("{} settings", desc.name), false, |ui| {
        for (i, p) in desc.params.iter().enumerate() {
            if Some(i) == progress {
                continue;
            }
            let target = ControlTarget::TransitionParam { layer: layer.id, index: i };
            ui.horizontal(|ui| {
                Level::range(&mut layer.transition_values[i], p.min, p.max, p.name).default(p.default).target(target).show(ui);
            });
        }
    });
}

/// Picks what a fade uses: the effect a "Through an effect" fade runs both pictures through, or the shader of a
/// "Shader transition" (cube, doorway, morph…). Returns true when it changed. Other styles: nothing to pick.
pub(crate) fn transition_effect_combo(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, kind: phosphor_engine::TransitionKind, effect: &mut String) -> bool {
    use phosphor_engine::{TransitionKind, shaders};
    let (list, current, empty) = match kind {
        TransitionKind::Effect => (shaders::all_effects(), shaders::effect(effect).map(|d| d.name), "Pick an effect"),
        TransitionKind::Shader => (shaders::all_transitions(), shaders::transition(effect).map(|d| d.name), "Pick a transition"),
        _ => return false,
    };
    let mut changed = false;
    egui::ComboBox::from_id_salt(id).selected_text(current.unwrap_or(empty)).width(150.0).height(360.0).show_ui(ui, |ui| {
        if list.is_empty() {
            help(ui, "No shader transitions found (they come with the ISF collection).");
        }
        for d in list {
            if crate::ui::pick_item(ui, d.id == effect.as_str(), d.name).clicked() {
                *effect = d.id.to_string();
                changed = true;
            }
        }
    });
    changed
}
