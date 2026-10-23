//! Timeline panel (MadMapper's Conductor): play / stop, a ruler with markers and the playhead, and a row per track
//! with its keys. Add a track from any control's right-click menu (Key on the timeline); drag keys in time (left /
//! right) and value (up / down); double-click a row to add a key; right-click a key or marker for its settings.

use std::collections::HashMap;

use phosphor_engine::{Event, Marker, MarkerKind, SendTo};

use super::*;

const ROW_H: f32 = 34.0;

/// "1:05.2" for seconds.
fn clock(t: f64) -> String {
    format!("{}:{:04.1}", (t / 60.0) as u32, t % 60.0)
}

pub(crate) fn timeline_panel(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.input(|i| i.time);
    let playing = st.timeline_play.is_some();
    ui.horizontal_wrapped(|ui| {
        let (icon_, text) = if playing { (icon::PAUSE, "Pause") } else { (icon::PLAY, "Play") };
        if action_button(ui, Act::Go, label(icon_, text)).clicked() {
            st.timeline_toggle(now);
        }
        if ui.button(label(icon::SKIP_BACK, "To the start")).clicked() {
            st.timeline_seek(0.0, now);
        }
        crate::ui::toggle(ui, &mut st.timeline_record, label(icon::RECORD, "Record")).on_hover_text("While it plays, moving a control that has a track keys it as you go (key a control once from its menu to give it a track)");
        ui.label(RichText::new(format!("{} / {}", clock(st.timeline_pos), clock(st.comp.timeline.length))).monospace());
        let tl = &mut st.comp.timeline;
        crate::ui::toggle(ui, &mut tl.looping, "Loop");
        guarded!(ui, tl.length, egui::DragValue::new(&mut tl.length).range(1.0..=36_000.0).speed(0.5).prefix("length ").suffix(" s").max_decimals(1));
        if ui.button(label(icon::FLAG, "Marker")).on_hover_text("A marker at the playhead (right-click it: pause or loop there)").clicked() {
            let n = tl.markers.len() + 1;
            tl.markers.push(Marker { t: st.timeline_pos, name: format!("M{n}"), kind: MarkerKind::Label });
        }
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::MUSIC_NOTES, "Sound…")).on_hover_text("A sound that plays along with the timeline (music, a voice-over), with its waveform").clicked() {
            st.dialogs.pick_file(crate::dialogs::Purpose::TimelineSound, Some(("Sound", &["wav", "aif", "aiff", "mp3", "m4a", "aac", "flac", "ogg", "mov", "mp4"])));
        }
        if ui.button(label(icon::LIGHTNING, "Event")).on_hover_text("An event at the playhead: plays a clip, fires a column or cue, sets anything, or sends OSC / MIDI as the playhead passes (right-click it)").clicked() {
            let n = tl.events.len() + 1;
            tl.events.push(Event { t: st.timeline_pos, name: format!("E{n}"), ..Default::default() });
        }
        render_menu(ui, st, now);
    });
    if let Some((p, done, frames)) = st.offline.as_ref().map(|o| (o.progress(), o.done, o.frames)) {
        let stop = ui.horizontal(|ui| {
            ui.add(meter(p).desired_width(200.0));
            ui.label(format!("Rendering {done} / {frames} frames"));
            ui.button("Stop").clicked()
        });
        if stop.inner {
            st.offline_stop();
        }
    }
    if st.comp.timeline.tracks.is_empty() {
        hint(ui, "Right-click any slider or button › Key on the timeline: it gets a track here with a key at the playhead. Move the playhead, change the value, key it again.");
    }
    let names: HashMap<ControlTarget, String> = crate::control::all_targets(&st.comp).into_iter().collect();
    let length = st.comp.timeline.length.max(0.1);
    let full = ui.available_width();
    // Track names take a column (narrower in a narrow panel).
    let label_w = (full * 0.25).clamp(70.0, 150.0);
    let (lane_x, lane_w) = (ui.cursor().left() + label_w, (full - label_w).max(80.0));
    let x_of = |t: f64| lane_x + (t / length) as f32 * lane_w;
    let t_of = |x: f32| (((x - lane_x) / lane_w) as f64 * length).clamp(0.0, length);

    // The ruler: seconds, markers, the playhead; click or drag to move the playhead.
    let (ruler, resp) = ui.allocate_exact_size(egui::vec2(full, 26.0), Sense::click_and_drag());
    let p = ui.painter_at(ruler);
    p.rect_filled(ruler, 0.0, ui.visuals().extreme_bg_color);
    let step = [1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 300.0].into_iter().find(|s| lane_w as f64 / (length / s) > 50.0).unwrap_or(600.0);
    let mut t = 0.0;
    while t <= length {
        let x = x_of(t);
        p.line_segment([egui::pos2(x, ruler.bottom() - 6.0), egui::pos2(x, ruler.bottom())], Stroke::new(1.0, Color32::GRAY));
        p.text(egui::pos2(x + 2.0, ruler.top() + 2.0), egui::Align2::LEFT_TOP, clock(t), egui::FontId::proportional(9.0), Color32::GRAY);
        t += step;
    }
    if (resp.dragged() || resp.clicked())
        && let Some(pos) = resp.interact_pointer_pos()
        && pos.x >= lane_x
    {
        st.timeline_seek(t_of(pos.x), now);
    }
    let mut remove_marker = None;
    for (k, m) in st.comp.timeline.markers.iter_mut().enumerate() {
        let x = x_of(m.t);
        let color = match m.kind {
            MarkerKind::Label => Color32::from_gray(170),
            MarkerKind::Pause => crate::theme::WARN,
            MarkerKind::LoopBack => accent(),
        };
        let flag = egui::Rect::from_min_size(egui::pos2(x, ruler.top() + 12.0), egui::vec2(46.0, 13.0));
        p.line_segment([egui::pos2(x, ruler.top()), egui::pos2(x, ruler.bottom())], Stroke::new(1.5, color));
        p.text(flag.left_top() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_TOP, &m.name, egui::FontId::proportional(9.5), color);
        let r = ui.interact(flag, ui.id().with(("marker", k)), Sense::click_and_drag());
        if r.dragged() {
            m.t = (m.t + (r.drag_delta().x / lane_w) as f64 * length).clamp(0.0, length);
        }
        r.context_menu(|ui| {
            crate::ui::text_field(ui, &mut m.name, "name", Some(120.0));
            choice_buttons(ui, &mut m.kind, |_, _| {});
            if danger_button(ui, label(icon::TRASH, "Remove")).clicked() {
                remove_marker = Some(k);
                ui.close();
            }
        });
    }
    if let Some(k) = remove_marker {
        st.comp.timeline.markers.remove(k);
    }

    #[cfg(not(target_arch = "wasm32"))]
    if st.comp.timeline.sound.is_some() {
        sound_lane(ui, st, lane_x, lane_w, full);
    }
    if !st.comp.timeline.events.is_empty() {
        events_lane(ui, st, &names, lane_x, lane_w, full);
    }

    // One row per track.
    let mut remove_track = None;
    let mut seek = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        for (ti, tr) in st.comp.timeline.tracks.iter_mut().enumerate() {
            let (row, resp) = ui.allocate_exact_size(egui::vec2(full, ROW_H), Sense::click());
            let p = ui.painter_at(row);
            let lane = egui::Rect::from_min_max(egui::pos2(lane_x, row.top() + 2.0), egui::pos2(row.right(), row.bottom() - 2.0));
            p.rect_filled(lane, 2.0, ui.visuals().faint_bg_color);
            let name = names.get(&tr.target).cloned().unwrap_or_else(|| "(gone)".into());
            let color = if tr.enabled { ui.visuals().text_color() } else { Color32::GRAY };
            p.text(egui::pos2(row.left() + 4.0, row.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(11.0), color);
            let y_of = |v: f32| lane.bottom() - v.clamp(0.0, 1.0) * lane.height();
            // The curve, sampled across the lane.
            let curve: Vec<egui::Pos2> = (0..=lane.width() as usize / 3).map(|i| {
                let x = lane.left() + i as f32 * 3.0;
                egui::pos2(x, y_of(tr.value_at(t_of(x)).unwrap_or(0.0)))
            }).collect();
            p.add(egui::Shape::line(curve, Stroke::new(1.5, accent().gamma_multiply(if tr.enabled { 0.9 } else { 0.3 }))));
            // Keys: drag to move (time and value), right-click for the ease and Remove.
            let mut moved = None;
            let mut remove_key = None;
            for (ki, key) in tr.keys.iter_mut().enumerate() {
                let at = egui::pos2(x_of(key.t), y_of(key.v));
                let r = ui.interact(egui::Rect::from_center_size(at, egui::vec2(11.0, 11.0)), ui.id().with(("key", ti, ki)), Sense::click_and_drag());
                let c = if r.hovered() || r.dragged() { Color32::WHITE } else { accent() };
                p.add(egui::Shape::convex_polygon(vec![at + egui::vec2(0.0, -5.0), at + egui::vec2(5.0, 0.0), at + egui::vec2(0.0, 5.0), at + egui::vec2(-5.0, 0.0)], c, Stroke::new(1.0, Color32::BLACK)));
                if r.dragged() {
                    let d = r.drag_delta();
                    key.v = (key.v - d.y / lane.height()).clamp(0.0, 1.0);
                    moved = Some((ki, (key.t + (d.x / lane_w) as f64 * length).clamp(0.0, length)));
                }
                r.on_hover_text(format!("{} · {:.0}%", clock(key.t), key.v * 100.0)).context_menu(|ui| {
                    ui.label("To the next key");
                    choice_buttons(ui, &mut key.ease, |_, _| {});
                    if danger_button(ui, label(icon::TRASH, "Remove key")).clicked() {
                        remove_key = Some(ki);
                        ui.close();
                    }
                });
            }
            if let Some((ki, t)) = moved {
                tr.move_key(ki, t);
            }
            if let Some(ki) = remove_key {
                tr.keys.remove(ki);
            }
            // Double-click the lane: a key there (on the curve).
            if resp.double_clicked()
                && let Some(pos) = resp.interact_pointer_pos()
                && pos.x >= lane_x
            {
                let t = t_of(pos.x);
                let v = 1.0 - ((pos.y - lane.top()) / lane.height()).clamp(0.0, 1.0);
                tr.set_key(t, v);
            } else if resp.clicked()
                && let Some(pos) = resp.interact_pointer_pos()
                && pos.x >= lane_x
            {
                seek = Some(t_of(pos.x));
            }
            resp.context_menu(|ui| {
                crate::ui::toggle(ui, &mut tr.enabled, "Plays");
                if danger_button(ui, label(icon::TRASH, "Remove track")).clicked() {
                    remove_track = Some(ti);
                    ui.close();
                }
            });
        }
    });
    if let Some(ti) = remove_track {
        st.comp.timeline.tracks.remove(ti);
    }
    if let Some(t) = seek {
        st.timeline_seek(t, now);
    }
    // The playhead, over the ruler and the rows.
    let x = x_of(st.timeline_pos);
    let area = ui.min_rect();
    ui.painter().line_segment([egui::pos2(x, ruler.top()), egui::pos2(x, area.bottom())], Stroke::new(1.5, crate::theme::WARN));
}

/// Render to a movie: the file, format, frame rate and length, then Render (frame by frame, never a dropped frame).
fn render_menu(ui: &mut egui::Ui, st: &mut State, now: f64) {
    if st.offline.is_some() {
        return;
    }
    ui.menu_button(label(icon::FILM_STRIP, "Render…"), |ui| {
        let length = st.comp.timeline.length;
        let o = &mut st.offline_setup;
        ui.horizontal(|ui| {
            crate::ui::text_field(ui, &mut o.path, "…/show.mp4 or .mov (ProRes)", Some(220.0));
            #[cfg(not(target_arch = "wasm32"))]
            if ui.button("Choose…").clicked() {
                st.dialogs.save_file(crate::dialogs::Purpose::RenderFile, ("Video", &["mp4", "mov"]), "Phosphor render.mp4");
            }
        });
        let o = &mut st.offline_setup;
        ui.horizontal(|ui| {
            ui.label("Format");
            choice_combo(ui, "render_format", &mut o.format, 200.0, false);
        });
        ui.horizontal(|ui| {
            ui.label("fps");
            egui::ComboBox::from_id_salt("render_fps").width(60.0).selected_text(o.fps.to_string()).show_ui(ui, |ui| {
                for f in [24, 25, 30, 50, 60] {
                    ui.selectable_value(&mut o.fps, f, f.to_string());
                }
            });
            ui.label("Length");
            let mut secs = if o.secs > 0.0 { o.secs } else { length };
            if guarded!(ui, secs, egui::DragValue::new(&mut secs).range(0.1..=36_000.0).speed(0.5).suffix(" s").max_decimals(1)).changed() {
                o.secs = secs;
            }
        });
        crate::ui::toggle(ui, &mut o.from_start, "Timeline from the start");
        hint(ui, "Frame by frame: every movie waits for its picture, so a heavy show comes out smooth. No faster than real time. Picture only, no sound yet.");
        if action_button(ui, Act::Record, label(icon::RECORD, "Render")).clicked() {
            let setup = st.offline_setup.clone();
            st.offline_start(&setup, now);
            ui.close();
        }
    });
}

/// The events: a flag each on their own lane; drag to move, right-click for what it does.
fn events_lane(ui: &mut egui::Ui, st: &mut State, names: &HashMap<ControlTarget, String>, lane_x: f32, lane_w: f32, full: f32) {
    let length = st.comp.timeline.length.max(0.1);
    let (row, _) = ui.allocate_exact_size(egui::vec2(full, 24.0), Sense::hover());
    let p = ui.painter_at(row);
    let lane = egui::Rect::from_min_max(egui::pos2(lane_x, row.top() + 2.0), egui::pos2(row.right(), row.bottom() - 2.0));
    p.rect_filled(lane, 2.0, ui.visuals().faint_bg_color);
    p.text(egui::pos2(row.left() + 4.0, row.center().y), egui::Align2::LEFT_CENTER, "Events", egui::FontId::proportional(11.0), ui.visuals().text_color());
    let targets: Vec<(ControlTarget, String)> = names.iter().map(|(t, n)| (t.clone(), n.clone())).collect();
    let mut remove = None;
    for (k, e) in st.comp.timeline.events.iter_mut().enumerate() {
        let x = lane_x + (e.t / length) as f32 * lane_w;
        let at = egui::pos2(x, lane.center().y);
        let r = ui.interact(egui::Rect::from_center_size(at, egui::vec2(12.0, 16.0)), ui.id().with(("event", k)), Sense::click_and_drag());
        let wired = e.target.is_some() || e.send.is_some();
        let c = if r.hovered() || r.dragged() { Color32::WHITE } else if wired { accent() } else { Color32::GRAY };
        p.add(egui::Shape::convex_polygon(vec![at + egui::vec2(0.0, -7.0), at + egui::vec2(5.0, 0.0), at + egui::vec2(0.0, 7.0), at + egui::vec2(-5.0, 0.0)], c, Stroke::NONE));
        p.text(at + egui::vec2(7.0, 0.0), egui::Align2::LEFT_CENTER, &e.name, egui::FontId::proportional(9.5), c);
        if r.dragged() {
            e.t = (e.t + (r.drag_delta().x / lane_w) as f64 * length).clamp(0.0, length);
        }
        let what = [e.target.as_ref().map(|t| format!("sets {}", names.get(t).map_or("(gone)", |n| n.as_str()))), e.send.as_ref().map(|s| format!("sends {}", s.label()))].into_iter().flatten().collect::<Vec<_>>().join(", ");
        r.on_hover_text(format!("{} · {}", clock(e.t), if what.is_empty() { "does nothing yet (right-click)".to_string() } else { what })).context_menu(|ui| {
            ui.set_min_width(300.0);
            crate::ui::text_field(ui, &mut e.name, "name", Some(140.0));
            ui.label(RichText::new("Sets").strong());
            ui.horizontal(|ui| {
                if let Some(t) = target_picker(ui, ("event_target", k), &targets, e.target.as_ref(), 200.0) {
                    e.target = Some(t);
                }
                if e.target.is_some() && ui.small_button(icon::X).on_hover_text("Set nothing").clicked() {
                    e.target = None;
                }
            });
            Level::percent(&mut e.value, "to").default(1.0).width(200.0).show(ui);
            hint(ui, "A clip or column: on plays it. A pick-one setting: the value picks.");
            let mut sends = e.send.is_some();
            if crate::ui::toggle(ui, &mut sends, "Sends").on_hover_text("Also sends the value out (OSC to the Control Surface's OSC out, or MIDI)").changed() {
                e.send = sends.then(|| SendTo::Osc { address: "/phosphor/event".into() });
            }
            if let Some(to) = &mut e.send {
                ui.horizontal(|ui| crate::ui::send_to_editor(ui, to));
            }
            if danger_button(ui, label(icon::TRASH, "Remove")).clicked() {
                remove = Some(k);
                ui.close();
            }
        });
    }
    if let Some(k) = remove {
        st.comp.timeline.events.remove(k);
    }
}

/// The sound track: its waveform from where it starts (drag to move it), right-click for its volume, another file
/// or Remove.
#[cfg(not(target_arch = "wasm32"))]
fn sound_lane(ui: &mut egui::Ui, st: &mut State, lane_x: f32, lane_w: f32, full: f32) {
    let length = st.comp.timeline.length.max(0.1);
    let (row, resp) = ui.allocate_exact_size(egui::vec2(full, 40.0), Sense::click_and_drag());
    let p = ui.painter_at(row);
    let lane = egui::Rect::from_min_max(egui::pos2(lane_x, row.top() + 2.0), egui::pos2(row.right(), row.bottom() - 2.0));
    p.rect_filled(lane, 2.0, ui.visuals().faint_bg_color);
    let Some(sound) = st.comp.timeline.sound.as_mut() else { return };
    let name = std::path::Path::new(&sound.path).file_name().map_or("Sound".into(), |n| n.to_string_lossy().to_string());
    p.text(egui::pos2(row.left() + 4.0, row.center().y), egui::Align2::LEFT_CENTER, &name, egui::FontId::proportional(11.0), ui.visuals().text_color());
    // The waveform: the loudest moment under each column of pixels.
    let peaks = st.timeline_sound.as_ref().and_then(|s| s.peaks.lock().ok().and_then(|p| p.clone()));
    let per_sec = crate::media::video::sound::PEAKS_PER_SEC;
    let x_of = |t: f64| lane_x + (t / length) as f32 * lane_w;
    match &peaks {
        Some(peaks) if !peaks.is_empty() => {
            let secs_per_px = length / lane_w as f64;
            let first = x_of(sound.start).max(lane.left()).floor() as i32;
            let last = x_of(sound.start + peaks.len() as f64 / per_sec).min(lane.right()) as i32;
            for x in first..last {
                let t = (x as f64 - lane_x as f64) * secs_per_px - sound.start;
                let (a, b) = (((t * per_sec) as usize).min(peaks.len() - 1), (((t + secs_per_px) * per_sec) as usize).min(peaks.len()));
                let v = peaks[a..b.max(a + 1)].iter().copied().fold(0.0, f32::max) * sound.volume.min(1.0);
                let h = (v * lane.height() * 0.5).max(0.5);
                p.line_segment([egui::pos2(x as f32, lane.center().y - h), egui::pos2(x as f32, lane.center().y + h)], Stroke::new(1.0, accent().gamma_multiply(0.8)));
            }
        }
        Some(_) => {
            p.text(lane.center(), egui::Align2::CENTER_CENTER, "no sound in this file", egui::FontId::proportional(10.0), Color32::GRAY);
        }
        None => {
            p.text(lane.center(), egui::Align2::CENTER_CENTER, "reading the sound…", egui::FontId::proportional(10.0), Color32::GRAY);
        }
    }
    if resp.dragged() {
        sound.start = (sound.start + (resp.drag_delta().x / lane_w) as f64 * length).clamp(0.0, length);
    }
    let mut remove = false;
    let mut choose = false;
    resp.on_hover_text(format!("{name} · starts at {} · drag to move it", clock(sound.start))).context_menu(|ui| {
        Level::range(&mut sound.volume, 0.0, 2.0, "Volume").default(1.0).width(200.0).show(ui);
        if ui.button("Another sound…").clicked() {
            choose = true;
            ui.close();
        }
        if danger_button(ui, label(icon::TRASH, "Remove the sound")).clicked() {
            remove = true;
            ui.close();
        }
    });
    if remove {
        st.comp.timeline.sound = None;
    }
    if choose {
        st.dialogs.pick_file(crate::dialogs::Purpose::TimelineSound, Some(("Sound", &["wav", "aif", "aiff", "mp3", "m4a", "aac", "flac", "ogg", "mov", "mp4"])));
    }
}
