//! Timeline panel (MadMapper's Conductor): play / stop, a ruler with markers and the playhead, and a row per track
//! with its keys. Add a track from any control's right-click menu (Key on the timeline); drag keys in time (left /
//! right) and value (up / down); double-click a row to add a key; right-click a key or marker for its settings.

use std::collections::HashMap;

use phosphor_engine::{Marker, MarkerKind};

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
