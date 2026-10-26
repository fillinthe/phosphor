//! The clip grid's parts, shared by the Clips panel and the Stack: the transport bar (tempo, launch, bank), the
//! column headers, one layer's row of clip cells, and what the cells asked for (`RowOut`), applied in one place.

use super::*;
use phosphor_engine::TriggerMode;

/// Everything a row of cells needs to know this frame (built once per panel).
pub(crate) struct Grid {
    pub cols: usize,
    pub bank: usize,
    pub cell: egui::Vec2,
    pub aspect: f32,
    pub thumb_h: u32,
    /// Files being dragged in from the Finder (over the window).
    pub drag_at: Option<egui::Pos2>,
    /// Shift / Cmd held: clicks add to the clip selection instead of playing.
    pub adding: bool,
    pub selected_clip: Option<(u64, usize)>,
    pub held_id: egui::Id,
    pub pressed: bool,
}

impl Grid {
    /// Also hands back a `RowOut` with any flash / piano release that happened this frame.
    pub(crate) fn new(ui: &egui::Ui, st: &State) -> (Grid, RowOut) {
        let aspect = st.aspect();
        let cols = st.comp.clip_columns.max(1);
        let bank = st.clip_page;
        let cell = egui::vec2(96.0, 96.0 / aspect + 16.0);
        let mut out = RowOut::default();
        // The flash/piano cell held down with the mouse (released when the button comes up anywhere).
        let held_id = egui::Id::new("clip_cell_held");
        let held: Option<(u64, usize)> = ui.ctx().data(|d| d.get_temp(held_id));
        let (pressed, released) = ui.input(|i| (i.pointer.primary_pressed(), i.pointer.primary_released()));
        if released && let Some(h) = held {
            out.releases.push(h);
            ui.ctx().data_mut(|d| d.remove::<(u64, usize)>(held_id));
        }
        let grid = Grid {
            cols,
            bank,
            cell,
            aspect,
            thumb_h: (crate::thumbs::THUMB_W as f32 / aspect).max(1.0) as u32,
            drag_at: crate::app::file_drag_point(ui.ctx()),
            adding: ui.input(|i| i.modifiers.shift || i.modifiers.command),
            selected_clip: st.selected_clip,
            held_id,
            pressed,
        };
        (grid, out)
    }
}

/// The parts of `State` the cells draw with (borrowed apart from `comp`, so a layer can be `&mut` meanwhile).
pub(crate) struct CellCtx<'a> {
    pub thumbs: &'a mut crate::thumbs::Thumbs,
    pub clip_cells: &'a mut Vec<(egui::Rect, u64, usize)>,
    pub preview_wants: &'a mut Vec<(String, phosphor_engine::PreviewItem, bool)>,
    pub preview_keys: &'a mut Vec<String>,
    pub preview_textures: &'a mut Vec<Option<(egui::TextureId, (u32, u32), bool)>>,
    pub also_clips: &'a mut Vec<(u64, usize)>,
    pub ctx: egui::Context,
}

/// What a row's menu asked for (beyond fill / match the beat).
pub(crate) enum RowAct {
    /// Another of the row's clips, at random (`ControlTarget::LayerRandomClip`).
    Random,
    Map(u8, u8),
    Shift(i32),
    ExportBank,
    ImportBank,
}

/// What the cells and row menus asked for this frame; `apply_row_out` does it all once the borrows are over.
#[derive(Default)]
pub(crate) struct RowOut {
    pub triggers: Vec<Trigger>,
    pub releases: Vec<(u64, usize)>,
    /// (layer idx, slot, media idx)
    pub set_cell: Option<(usize, usize, usize)>,
    /// (layer idx, slot, a live input…)
    pub set_source: Option<(usize, usize, super::SourceDrag)>,
    pub use_current: Option<(usize, usize)>,
    /// A clip clicked: it plays and is selected (its layer too), so the Inspector shows its settings.
    pub picked: Option<(u64, usize)>,
    pub clear: Option<(usize, usize)>,
    pub thumb_req: Option<(u64, usize)>,
    pub beat: Option<(u64, usize)>,
    pub beat_row: Option<u64>,
    pub fill_row: Option<u64>,
    pub row_midi: Option<(u64, RowAct)>,
    /// A clip's file starred / unstarred in Media.
    pub favorite: Option<(u64, usize)>,
    /// Stop this layer's clip (`MASTER_ID`: every layer's).
    pub stop: Option<u64>,
}

/// Tempo (tap, bpm, resync, beat dots, MIDI clock, Link, AUTO), the launch settings, the bank and columns.
pub(crate) fn transport_bar(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    ui.horizontal_wrapped(|ui| {
        let tap = crate::ui::command(ui, label(icon::METRONOME, "Tap")).on_hover_text("Tap 3+ times to set the tempo");
        map_menu(ui, &tap, ControlTarget::App(phosphor_engine::AppControl::TapTempo));
        if tap.clicked() {
            st.clock.tap(now);
        }
        let bpm = guarded!(ui, st.clock.bpm, egui::DragValue::new(&mut st.clock.bpm).range(40.0..=300.0).speed(0.5).suffix(" bpm"));
        map_menu(ui, &bpm, ControlTarget::App(phosphor_engine::AppControl::Tempo));
        let resync = crate::ui::command(ui, "Resync").on_hover_text("Beat 1 = now, and it stays there (AUTO keeps the tempo; turn AUTO off and on to let the music move beat 1 again)");
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
        if auto.changed() && st.clock.follow_audio {
            // AUTO on again: the music decides where beat one is again (after a Resync).
            st.clock.bar_by_hand = false;
        }
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
            crate::ui::dropdown(ui, "clip_bank", 60.0).selected_text(bank_name(st.clip_page)).height(400.0).show_ui(ui, |ui| {
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
        help(ui, "Keys 1–9 trigger the lit bank's columns · drop files or a folder onto a cell to fill the row · right-click a cell");
    });
}

/// The column headers: each triggers its whole column (`head_w` = the width of the layer strips before them).
pub(crate) fn column_headers(ui: &mut egui::Ui, grid: &Grid, head_w: f32, out: &mut RowOut) {
    // Column headers trigger whole columns.
    ui.horizontal(|ui| {
        // Stop every layer (Resolume's clear-all): the space over the layer strips.
        let all = ControlTarget::StopLayer(phosphor_engine::MASTER_ID);
        let stop = command_sized(ui, label(icon::STOP, "Stop all"), egui::vec2((head_w - ui.spacing().item_spacing.x).max(40.0), CONTROL_H))
            .on_hover_text("Stop every layer's clip (launch fade) · right-click to map it");
        if stop.clicked() {
            out.stop = Some(phosphor_engine::MASTER_ID);
        }
        mapped_badge(ui, &stop, &all);
        stop.right_menu(|ui| learn_items(ui, &all));
        for c in 0..grid.cols {
            let keyed = c / CLIP_PAGE_SIZE == grid.bank;
            let tip = if keyed { format!("Trigger this column on every layer (key {}) · right-click to Learn", c % CLIP_PAGE_SIZE + 1) } else { "Trigger this column on every layer · right-click to Learn".to_string() };
            // Triggers are actions: solid buttons (the columns on the number keys in full colour).
            let head = if keyed {
                action_button_sized(ui, Act::Go, format!("{} {}", icon::PLAY, c + 1), egui::vec2(grid.cell.x, CONTROL_H))
            } else {
                command_sized(ui, format!("{} {}", icon::PLAY, c + 1), egui::vec2(grid.cell.x, CONTROL_H))
            }
            .on_hover_text(tip);
            if head.clicked() {
                out.triggers.push(Trigger::Column(c));
            }
            mapped_badge(ui, &head, &ControlTarget::TriggerColumn(c));
            head.right_menu(|ui| learn_items(ui, &ControlTarget::TriggerColumn(c)));
        }
    });
}

/// One layer's row of clip cells (`idx` = its index in `comp.layers`).
pub(crate) fn clip_cells(ui: &mut egui::Ui, grid: &Grid, cx: &mut CellCtx, layer: &mut LayerState, idx: usize, out: &mut RowOut) {
    for c in 0..grid.cols {
        let clip = layer.clips.get(c);
        let (rect, resp) = ui.allocate_exact_size(grid.cell, Sense::click());
        if ui.is_rect_visible(rect) {
            cx.clip_cells.push((rect, layer.id, c));
        }
        let active = clip.is_some_and(|k| !k.is_empty() && layer.active_clip == Some(k.id));
        let chosen = cx.also_clips.contains(&(layer.id, c)) || (!cx.also_clips.is_empty() && grid.selected_clip == Some((layer.id, c)));
        let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(grid.cell.x, grid.cell.x / grid.aspect));
        let p = ui.painter();
        let empty = ui.visuals().extreme_bg_color;
        p.rect_filled(rect, 4.0, if active { crate::theme::tint(empty, 0.14) } else { empty });
        if chosen {
            p.rect_stroke(rect, 4.0, Stroke::new(2.0, accent()), StrokeKind::Inside);
        }
        match clip {
            Some(k) if !k.is_empty() => {
                let chosen = (!k.thumb.is_empty()).then(|| cx.thumbs.get(&k.thumb)).flatten();
                let tex = if chosen.is_some() { chosen } else { match &k.source {
                    SourceSpec::Image { path: Some(pth), .. } | SourceSpec::Movie { path: Some(pth), .. } => cx.thumbs.get(pth),
                    // Generators render through the engine's preview slots: a still each, animated while hovered.
                    SourceSpec::Generator(id) => {
                        let key = format!("clip:{}:{:x}", k.id, hash_values(&k.generator_values));
                        cx.preview_wants.push((key.clone(), phosphor_engine::PreviewItem::Clip(id.clone(), k.generator_values.clone()), resp.hovered()));
                        crate::app::preview_lookup(cx.preview_keys, cx.preview_textures, &key)
                    }
                    SourceSpec::Text(style) => {
                        let key = format!("text:{:x}", hash_text(style));
                        cx.thumbs.get_or_build(&cx.ctx, &key, || {
                            let img = crate::text::render(style, crate::thumbs::THUMB_W, grid.thumb_h).ok()?;
                            Some(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], &img.into_raw()))
                        })
                    }
                    _ => None,
                } };
                match tex {
                    Some(t) => paint_texture(p, t, thumb_rect),
                    None => {
                        p.rect_filled(thumb_rect, 4.0, Color32::from_gray(30));
                        // Long names (imported clips) stay inside their own grid.cell; sound files get a note.
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
                // S = keeps its own settings; wand = has its own effects.
                let own = if k.own.is_some() { "S" } else { "" };
                let fx = if !k.effects.is_empty() || k.own.as_ref().is_some_and(|o| o.with_effects) { format!(" {}", icon::MAGIC_WAND) } else { String::new() };
                let badge = format!("{}{own}{fx}", badge.unwrap_or("")).trim_start().to_string();
                if !badge.is_empty() {
                    p.text(egui::pos2(rect.right() - 4.0, rect.top() + 3.0), egui::Align2::RIGHT_TOP, badge, egui::FontId::monospace(11.0), accent());
                }
            }
            _ => {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), ui.visuals().weak_text_color());
            }
        }
        let stroke = if grid.drag_at.is_some_and(|p| rect.contains(p)) {
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
        // What kind of clip it is: a stripe down its left edge in that kind's colour.
        if let Some(h) = clip.and_then(|k| crate::theme::Hue::of_source(&k.source)) {
            let stripe = egui::Rect::from_min_max(rect.left_top() + egui::vec2(1.0, 4.0), egui::pos2(rect.left() + 4.0, rect.bottom() - 4.0));
            p.rect_filled(stripe, 1.5, h.color());
        }
        // A muted clip says so on its grid.cell.
        if clip.is_some_and(|k| k.muted) {
            let at = rect.right_top() + egui::vec2(-4.0, 4.0);
            p.text(at, egui::Align2::RIGHT_TOP, icon::SPEAKER_SIMPLE_X, egui::FontId::proportional(12.0), crate::theme::WARN);
        }
        if let Some(m) = crate::ui::drop_payload::<MediaDrag>(&resp) {
            out.set_cell = Some((idx, c, m.0));
        }
        if let Some(d) = crate::ui::drop_payload::<super::SourceDrag>(&resp) {
            out.set_source = Some((idx, c, (*d).clone()));
        }
        // Flash / piano clips play from the moment the button goes down until it comes up.
        let momentary = clip.is_some_and(|k| !k.is_empty() && matches!(k.trigger, TriggerMode::Flash | TriggerMode::Piano));
        if momentary {
            if grid.pressed && resp.hovered() {
                out.triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                out.picked = Some((layer.id, c));
                ui.ctx().data_mut(|d| d.insert_temp(grid.held_id, (layer.id, c)));
            }
        } else if resp.clicked() && grid.adding && clip.is_some_and(|k| !k.is_empty()) && grid.selected_clip.is_some_and(|s| s != (layer.id, c)) {
            // Shift / Cmd-click: one more clip to edit together (it doesn't play).
            match cx.also_clips.iter().position(|x| *x == (layer.id, c)) {
                Some(k) => {
                    cx.also_clips.remove(k);
                }
                None => cx.also_clips.push((layer.id, c)),
            }
        } else if resp.clicked() {
            cx.also_clips.clear();
            match clip {
                Some(k) if !k.is_empty() => {
                    out.triggers.push(Trigger::Clip { layer: layer.id, slot: c });
                    out.picked = Some((layer.id, c));
                }
                _ => out.use_current = Some((idx, c)),
            }
        }
        mapped_badge(ui, &resp, &ControlTarget::TriggerClip { layer: layer.id, slot: c });
        let filled = clip.is_some_and(|k| !k.is_empty());
        let is_movie = clip.is_some_and(|k| matches!(k.source, SourceSpec::Movie { path: Some(_), .. }));
        resp.right_menu(|ui| {
            if let Some(act) = clip_editor(ui, layer, c, filled, is_movie) {
                match act {
                    ClipAct::Trigger => out.triggers.push(Trigger::Clip { layer: layer.id, slot: c }),
                    ClipAct::MatchBeat => out.beat = Some((layer.id, c)),
                    ClipAct::StoreCurrent => out.use_current = Some((idx, c)),
                    ClipAct::Clear => out.clear = Some((idx, c)),
                    ClipAct::Thumbnail => out.thumb_req = Some((layer.id, c)),
                    ClipAct::Favorite => out.favorite = Some((layer.id, c)),
                }
                ui.close();
            }
        });
    }
}

/// Does what the cells asked: drops, plays, selection, row menus, releases.
pub(crate) fn apply_row_out(st: &mut State, out: RowOut, now: f64) {
    if let Some(id) = out.stop {
        st.stop_clips(id, now);
    }
    if let Some((layer, c)) = out.favorite {
        apply_clip_act(st, layer, c, ClipAct::Favorite, now);
    }
    // A media bin item or a live input dropped on a cell: that cell's clip (the way any clips are placed).
    let dropped = out.set_cell
        .and_then(|(idx, c, m)| st.comp.media_bin.get(m).map(|item| (idx, c, item.name.clone(), crate::media::spec_for_path(&item.path))))
        .or(out.set_source.map(|(idx, c, d)| (idx, c, d.name, d.source)));
    if let Some((idx, c, name, source)) = dropped
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        st.comp.place_clips(id, c, vec![(name, source)]);
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some((layer, c)) = out.beat {
            st.match_beat(crate::media::rhythm::MatchTarget::Clip(layer, c));
        }
        if let Some(layer) = out.beat_row {
            st.match_beat_all(layer);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = (out.beat, out.beat_row);
    if let Some((layer, c)) = out.picked {
        st.selected = Some(layer);
        st.selected_clip = Some((layer, c));
    }
    if let Some((idx, c)) = out.use_current
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        apply_clip_act(st, id, c, ClipAct::StoreCurrent, now);
    }
    match out.row_midi {
        Some((id, RowAct::Map(ch, note))) => {
            st.comp.map_clip_row(id, ch, note);
            st.notice = Some(format!("Clip row on MIDI channel {} from note {note}", ch + 1));
        }
        Some((id, RowAct::Random)) => st.set_control(&phosphor_engine::ControlTarget::LayerRandomClip(id), 1.0, now),
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
    if let Some(id) = out.fill_row {
        st.quick_on(crate::app::Quick::Fill, Some(id), now);
    }
    if out.thumb_req.is_some() {
        st.clip_thumb_request = out.thumb_req;
    }
    if let Some((idx, c)) = out.clear
        && let Some(id) = st.comp.layers.get(idx).map(|l| l.id)
    {
        apply_clip_act(st, id, c, ClipAct::Clear, now);
    }
    for t in out.triggers {
        st.trigger(t, now);
    }
    for (layer, slot) in out.releases {
        st.release_clip(layer, slot, now);
    }
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
