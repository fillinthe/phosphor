//! Clip grid and launch settings.

use super::*;
use super::clip_grid::*;

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
        crate::ui::toolbar(ui, |ui| {
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
    transport_bar(ui, st);
    ui.add_space(4.0);
    let (grid, mut out) = Grid::new(ui, st);
    {
        let State { comp, thumbs, clip_cells, preview_wants, preview_keys, preview_textures, layer_textures, selected, also_clips, .. } = st;
        let ctx = ui.ctx().clone();
        let mut cx = CellCtx { thumbs, clip_cells, preview_wants, preview_keys, preview_textures, also_clips, ctx };
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            column_headers(ui, &grid, ROW_HEADER_W, &mut out);
            let n = comp.layers.len();
            // Every comp's rows, under its name (as in Layers), the later comp first; folded comps hide theirs.
            let comp_ids: Vec<Option<u64>> = if comp.comps.is_empty() { vec![None] } else { comp.comps.iter().rev().map(|c| Some(c.id)).collect() };
            let many = comp.comps.len() > 1;
            for cid in comp_ids {
            if let Some(c) = cid.and_then(|id| comp.comp(id)).filter(|c| many || !c.is_plain()) {
                let caret = if c.collapsed { icon::CARET_RIGHT } else { icon::CARET_DOWN };
                ui.label(RichText::new(format!("{caret} {} {}", icon::FRAME_CORNERS, c.name)).strong().color(crate::theme::Hue::Output.color()))
                    .on_hover_text("A comp (fold it, rename it and its settings: the Layers panel)");
                if c.collapsed {
                    continue;
                }
            }
            for idx in (0..n).rev() {
                if comp.comp_of(&comp.layers[idx]) != cid {
                    continue;
                }
                let layer = &mut comp.layers[idx];
                ui.horizontal(|ui| {
                    let thumb = layer_textures.get(&layer.id).map(|(t, _)| *t);
                    let (fill, beat_all, midi) = row_header(ui, layer, thumb, selected, egui::vec2(ROW_HEADER_W, grid.cell.y));
                    if let Some(m) = midi {
                        out.row_midi = Some((layer.id, m));
                    }
                    if fill {
                        out.fill_row = Some(layer.id);
                    }
                    if beat_all {
                        out.beat_row = Some(layer.id);
                    }
                    super::clip_grid::clip_cells(ui, &grid, &mut cx, layer, idx, &mut out);
                });
            }
            }
            if n == 0 {
                hint(ui, "Add layers first; each layer gets a row of clip slots.");
            }
        });
    }
    apply_row_out(st, out, now);
}

/// Width of the layer strip at the start of each clip row.
const ROW_HEADER_W: f32 = 176.0;

/// The start of a clip row is the layer itself: its live picture (click = select it, the Inspector opens), its name
/// (menu: autopilot, fill, match the beat), ON and opacity. Returns (fill the row, match every clip's beat).
/// A row's MIDI notes, asked for in its menu: map the row from a note (channel, first note), or shift it.

/// A clip row's menu: fill with variations, match the beat, MIDI notes for the row, export / import the bank,
/// pick a clip by value, "plays on" and autopilot. Shared by the Clips panel's row header and the Stack's strips.
pub(crate) fn row_menu(ui: &mut egui::Ui, layer: &mut phosphor_engine::LayerState, fill: &mut bool, beat_all: &mut bool, row_midi: &mut Option<RowAct>) {
    use phosphor_engine::Autopilot;
    ui.set_min_width(220.0);
        let q = crate::app::Quick::Fill;
        if crate::ui::command(ui, format!("{} {}", q.icon(), q.label())).on_hover_text(q.about()).clicked() {
            *fill = true;
            ui.close();
        }
        if layer.clips.iter().any(|c| matches!(c.source, SourceSpec::Movie { path: Some(_), .. }))
            && crate::ui::command(ui, label(icon::MAGIC_WAND, "Match the beat of every movie here")).on_hover_text("Scans each movie clip in this row for its rhythm and locks it to the tempo").clicked()
        {
            *beat_all = true;
            ui.close();
        }
        ui.separator();
        crate::ui::heading(ui, format!("{} MIDI notes", icon::PIANO_KEYS));
        hint(ui, "The whole row on a run of notes (a pad grid's row): clip 1 = the first note, clip 2 = the next…");
        let key = ui.id().with(("row_midi", layer.id));
        let (mut ch, mut note): (u8, u8) = ui.data(|d| d.get_temp(key)).unwrap_or((0, 36));
        crate::ui::toolbar(ui, |ui| {
            let mut c1 = ch + 1;
            guarded!(ui, c1, egui::DragValue::new(&mut c1).range(1..=16).prefix("ch "));
            ch = c1 - 1;
            guarded!(ui, note, egui::DragValue::new(&mut note).range(0..=127).prefix("from note "));
            if crate::ui::command(ui, "Map the row").clicked() {
                *row_midi = Some(RowAct::Map(ch, note));
                ui.close();
            }
        });
        ui.data_mut(|d| d.insert_temp(key, (ch, note)));
        crate::ui::toolbar(ui, |ui| {
            ui.label("Shift its notes");
            for by in [-8, -1, 1, 8] {
                if crate::ui::small_command(ui, format!("{by:+}")).on_hover_text("Move this row's note mappings (a bank on the controller)").clicked() {
                    *row_midi = Some(RowAct::Shift(by));
                }
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        crate::ui::toolbar(ui, |ui| {
            if crate::ui::command(ui, label(icon::EXPORT, "Export clips…")).on_hover_text("Save this row's clips and their settings as a bank file").clicked() {
                *row_midi = Some(RowAct::ExportBank);
                ui.close();
            }
            if crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Import…")).on_hover_text("Replace this row's clips with a saved bank").clicked() {
                *row_midi = Some(RowAct::ImportBank);
                ui.close();
            }
        });
        let pick = crate::ui::command(ui, label(icon::SLIDERS_HORIZONTAL, "Pick a clip by value…")).on_hover_text("One knob or fader picks among this row's clips (map it: right-click › Listen)");
        map_menu(ui, &pick, ControlTarget::LayerClipPick(layer.id));
        ui.separator();
        crate::ui::heading(ui, format!("{} Plays on", icon::STACK));
        hint(ui, "Share this row's clips over the layers beside it (same group): each clip on the next layer, or on one with nothing playing.");
        choice_buttons(ui, &mut layer.voices, |_, _| {});
        ui.separator();
        crate::ui::heading(ui, format!("{} Autopilot", icon::AIRPLANE_TILT));
        hint(ui, "When this clip is done: play another one by itself, or clear the layer.");
        for a in Autopilot::ALL {
            ui.radio_value(&mut layer.autopilot, a, a.name());
        }
        if layer.autopilot != Autopilot::Off {
            ui.separator();
            let mut at_end = layer.autopilot_beats <= 0.0;
            crate::ui::toolbar(ui, |ui| {
                if ui.radio_value(&mut at_end, true, "When the clip ends").changed() {
                    layer.autopilot_beats = 0.0;
                }
            });
            crate::ui::toolbar(ui, |ui| {
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
            crate::ui::toolbar(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                super::inspector::mute_button(ui, &mut layer.muted, "this layer");
                super::inspector::volume_bar(ui, layer, size.x - 34.0);
            });
        }
        let name: String = layer.name.chars().take(20).collect();
        ui.menu_button(RichText::new(name).color(ui.visuals().text_color()), |ui| {
            row_menu(ui, layer, &mut fill, &mut beat_all, &mut row_midi);
        })
        .response
        .on_hover_text("Click for autopilot and fill with variations");
        let when = if layer.autopilot_beats > 0.0 { format!("{:.0} beats", layer.autopilot_beats) } else { "at end".into() };
        if let Some(c) = layer.collection.clone() {
            // Playing a collection: say which, and a one-click stop right here.
            crate::ui::toolbar(ui, |ui| {
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



