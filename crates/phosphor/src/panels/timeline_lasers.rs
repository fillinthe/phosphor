//! The Timeline's laser lanes: a row per laser projector whose looks change over time, each look a block from where
//! it starts to the next change (drag it to move it, right-click for another look or Remove), and the Laser look
//! menu that adds a change at the playhead. Model: `Timeline::{laser_looks, look_at, set_look}`; played by
//! `State::run_timeline`.

use super::*;

/// "Laser look" in the Timeline's bar: a look for a projector from the playhead on (only with lasers).
pub(crate) fn look_menu(ui: &mut egui::Ui, st: &mut State) {
    if st.comp.lasers.projectors.is_empty() || st.comp.lasers.looks.is_empty() {
        return;
    }
    let looks: Vec<(u64, String)> = st.comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
    let projectors: Vec<(u64, String)> = st.comp.lasers.projectors.iter().map(|p| (p.id, p.name.clone())).collect();
    let mut add = None;
    ui.menu_button(label(icon::PLUS, "Laser look"), |ui| {
        for (pid, pname) in &projectors {
            if projectors.len() > 1 {
                crate::ui::heading(ui, pname);
            }
            for (lid, lname) in &looks {
                if crate::ui::pick_item(ui, false, lname).clicked() {
                    add = Some((*pid, *lid));
                    ui.close();
                }
            }
        }
    })
    .response
    .on_hover_text("A laser look from the playhead on: the projector switches to it as the timeline plays");
    if let Some((projector, look)) = add {
        let t = st.timeline_pos;
        st.comp.timeline.set_look(projector, t, look);
        if let Some(p) = st.comp.lasers.projector_mut(projector) {
            p.look = look;
        }
    }
}

/// A lane per projector with look changes.
pub(crate) fn laser_lanes(ui: &mut egui::Ui, st: &mut State, lane_x: f32, lane_w: f32, full: f32) {
    let length = st.comp.timeline.length.max(0.1);
    let x_of = |t: f64| lane_x + (t / length) as f32 * lane_w;
    let looks: Vec<(u64, String)> = st.comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
    let colour_of = |look: u64| {
        let i = looks.iter().position(|(id, _)| *id == look).unwrap_or(0);
        let [r, g, b] = phosphor_engine::laser::hue(i as f32 * 0.17 + 0.5).map(|c| (c * 255.0) as u8);
        Color32::from_rgb(r, g, b)
    };
    let projectors: Vec<(u64, String)> = st.comp.lasers.projectors.iter().filter(|p| st.comp.timeline.laser_looks.iter().any(|c| c.projector == p.id)).map(|p| (p.id, p.name.clone())).collect();
    let mut moved: Option<(usize, f64)> = None;
    let mut relook: Option<(usize, u64)> = None;
    let mut remove: Option<usize> = None;
    let mut seek = None;
    for (pid, pname) in projectors {
        let (row, resp) = ui.allocate_exact_size(egui::vec2(full, 26.0), Sense::click());
        let p = ui.painter_at(row);
        let lane = egui::Rect::from_min_max(egui::pos2(lane_x, row.top() + 2.0), egui::pos2(row.right(), row.bottom() - 2.0));
        p.rect_filled(lane, 2.0, ui.visuals().faint_bg_color);
        p.text(egui::pos2(row.left() + 4.0, row.center().y), egui::Align2::LEFT_CENTER, format!("{} {pname}", icon::PROJECTOR_SCREEN), egui::FontId::proportional(11.0), ui.visuals().text_color());
        let changes: Vec<(usize, phosphor_engine::LookChange)> = st.comp.timeline.laser_looks.iter().copied().enumerate().filter(|(_, c)| c.projector == pid).collect();
        for (k, (i, c)) in changes.iter().enumerate() {
            let end = changes.get(k + 1).map_or(length, |n| n.1.t);
            let block = egui::Rect::from_min_max(egui::pos2(x_of(c.t), lane.top()), egui::pos2(x_of(end).max(x_of(c.t) + 4.0), lane.bottom()));
            let r = ui.interact(block, ui.id().with(("laser_look", *i)), Sense::click_and_drag());
            let colour = colour_of(c.look);
            let lit = r.hovered() || r.dragged();
            p.rect_filled(block.shrink2(egui::vec2(0.5, 0.0)), 2.0, colour.gamma_multiply(if lit { 0.55 } else { 0.35 }));
            p.line_segment([block.left_top(), block.left_bottom()], Stroke::new(2.0, colour));
            let name = looks.iter().find(|(id, _)| *id == c.look).map_or("(removed look)", |l| l.1.as_str());
            p.with_clip_rect(block.intersect(lane)).text(block.left_center() + egui::vec2(5.0, 0.0), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(10.5), Color32::WHITE);
            if r.dragged() {
                moved = Some((*i, (c.t + (r.drag_delta().x / lane_w) as f64 * length).clamp(0.0, length)));
            } else if r.clicked()
                && let Some(pos) = r.interact_pointer_pos()
            {
                seek = Some((((pos.x - lane_x) / lane_w) as f64 * length).clamp(0.0, length));
            }
            r.on_hover_text(format!("{name} from {} · drag to move it, right-click for another look", super::timeline::clock(c.t))).context_menu(|ui| {
                for (lid, lname) in &looks {
                    if crate::ui::pick_item(ui, *lid == c.look, lname).clicked() {
                        relook = Some((*i, *lid));
                        ui.close();
                    }
                }
                ui.separator();
                if danger_button(ui, label(icon::TRASH, "Remove")).clicked() {
                    remove = Some(*i);
                    ui.close();
                }
            });
        }
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
            && pos.x >= lane_x
        {
            seek = Some((((pos.x - lane_x) / lane_w) as f64 * length).clamp(0.0, length));
        }
    }
    let tl = &mut st.comp.timeline;
    if let Some((i, t)) = moved
        && let Some(c) = tl.laser_looks.get_mut(i)
    {
        c.t = t;
        tl.sort_looks();
    }
    if let Some((i, look)) = relook
        && let Some(c) = tl.laser_looks.get_mut(i)
    {
        c.look = look;
    }
    if let Some(i) = remove.filter(|i| *i < tl.laser_looks.len()) {
        tl.laser_looks.remove(i);
    }
    // (Re-applied at once: the playhead hasn't moved but what it's in may have.)
    if moved.is_some() || relook.is_some() || remove.is_some() {
        st.timeline_applied = None;
    }
    if let Some(t) = seek {
        let now = ui.input(|i| i.time);
        st.timeline_seek(t, now);
    }
}
