//! A laser projector's zones and dark areas, on a picture of its whole scan area: what it draws now (lit lines),
//! each zone's outline with a handle at every corner (drag them onto a surface seen at an angle), and the dark
//! areas in red with a handle at two corners. Geometry: `phosphor_engine::laser::{projector_paths, in_zone}`.

use phosphor_engine::laser::LaserPoint;
use phosphor_engine::{FULL_AREA, LaserArea, LaserProjector, LaserZone};

use super::*;

/// Zones and dark areas of projector `p` (`frame`: what it draws now, placed; `looks`: (id, name) to pick from;
/// `next_id`: the project's id counter).
pub(crate) fn zones_editor(ui: &mut egui::Ui, p: &mut LaserProjector, frame: &[LaserPoint], looks: &[(u64, String)], next_id: &mut u64) {
    let picked_id = egui::Id::new(("laser_zone_picked", p.id));
    let mut picked: Option<u64> = ui.ctx().data(|d| d.get_temp(picked_id)).flatten();
    area_canvas(ui, p, frame, &mut picked);
    let mut new_id = || {
        *next_id += 1;
        *next_id - 1
    };
    let mut remove = None;
    for (n, z) in p.zones.iter_mut().enumerate() {
        ui.push_id(z.id, |ui| {
            crate::ui::toolbar(ui, |ui| {
                crate::ui::toggle(ui, &mut z.enabled, "").on_hover_text("Off: this zone stays dark");
                if crate::ui::pick_item(ui, picked == Some(z.id), format!("{}", n + 1)).on_hover_text("Show its corners on the picture").clicked() {
                    picked = Some(z.id);
                }
                crate::ui::text_field(ui, &mut z.name, "Name", Some(90.0));
                let shown = if z.look == 0 { "(the projector's look)".to_string() } else { looks.iter().find(|(id, _)| *id == z.look).map_or("(removed look)".into(), |l| l.1.clone()) };
                crate::ui::dropdown(ui, ("laser_zone_look", z.id), 150.0).selected_text(shown).show_ui(ui, |ui| {
                    crate::ui::pick_value(ui, &mut z.look, 0, "(the projector's look)");
                    for (id, name) in looks {
                        crate::ui::pick_value(ui, &mut z.look, *id, name);
                    }
                });
                Level::percent(&mut z.brightness, "Brightness").default(1.0).width(130.0).target(ControlTarget::LaserBrightness(z.id)).show(ui);
                if crate::ui::danger_button(ui, label(icon::TRASH, "")).on_hover_text("Remove this zone").clicked() {
                    remove = Some(z.id);
                }
            });
            if picked == Some(z.id) {
                crate::ui::toolbar(ui, |ui| {
                    Level::range(&mut z.x, -1.0, 1.0, "X").default(0.0).width(100.0).show(ui);
                    Level::range(&mut z.y, -1.0, 1.0, "Y").default(0.0).width(100.0).show(ui);
                    Level::percent(&mut z.size, "Size").default(1.0).width(110.0).show(ui);
                    Level::range(&mut z.rotate, -180.0, 180.0, "Turn").default(0.0).width(110.0).show(ui);
                    if crate::ui::command_if(ui, z.corners != FULL_AREA, "Reset corners").on_hover_text("The zone fills the whole area again").clicked() {
                        z.corners = FULL_AREA;
                    }
                });
            }
        });
    }
    if let Some(id) = remove {
        p.zones.retain(|z| z.id != id);
    }
    let mut remove_dark = None;
    for (n, _) in p.dark.iter().enumerate() {
        crate::ui::toolbar(ui, |ui| {
            ui.push_id(("dark", n), |ui| {
                ui.colored_label(egui::Color32::from_rgb(255, 90, 70), format!("Dark area {}", n + 1));
                if crate::ui::danger_button(ui, label(icon::TRASH, "")).on_hover_text("Remove this dark area").clicked() {
                    remove_dark = Some(n);
                }
            });
        });
    }
    if let Some(n) = remove_dark {
        p.dark.remove(n);
    }
    crate::ui::toolbar(ui, |ui| {
        if crate::ui::command(ui, label(icon::PLUS, "Zone")).on_hover_text("A part of this laser's area with its own look: drag its corners on the picture").clicked() {
            let id = new_id();
            let n = p.zones.len() + 1;
            p.zones.push(LaserZone { id, name: format!("Zone {n}"), ..Default::default() });
            picked = Some(id);
        }
        if crate::ui::command(ui, label(icon::PLUS, "Dark area")).on_hover_text("An area the beam never lights (a balcony, a doorway): drag its corners on the picture").clicked() {
            p.dark.push(LaserArea::default());
        }
    });
    help(ui, "Zones split a laser's area: each shows a look, placed by dragging its corners (straighten a wall seen at an angle). With no zones the look fills the area. Dark areas are never lit, whatever is drawn.");
    ui.ctx().data_mut(|d| d.insert_temp(picked_id, picked));
}

/// The scan area: the frame as drawn, zone outlines with corner handles, dark areas with two corner handles.
fn area_canvas(ui: &mut egui::Ui, p: &mut LaserProjector, frame: &[LaserPoint], picked: &mut Option<u64>) {
    let side = ui.available_width().min(360.0).max(140.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
    let painter = ui.painter_at(rect.expand(8.0));
    painter.rect_filled(rect, 0.0, egui::Color32::from_rgb(4, 4, 8));
    let to_screen = |q: [f32; 2]| rect.center() + egui::vec2(q[0], -q[1]) * (side * 0.5);
    let grid = egui::Stroke::new(1.0, egui::Color32::from_gray(28));
    painter.line_segment([to_screen([-1.0, 0.0]), to_screen([1.0, 0.0])], grid);
    painter.line_segment([to_screen([0.0, -1.0]), to_screen([0.0, 1.0])], grid);
    let red = egui::Color32::from_rgb(255, 60, 40);
    if p.floor > -1.0 {
        painter.rect_filled(egui::Rect::from_two_pos(to_screen([-1.0, p.floor]), to_screen([1.0, -1.0])), 0.0, egui::Color32::from_rgba_unmultiplied(255, 40, 30, 24));
    }
    for a in &p.dark {
        let r = egui::Rect::from_two_pos(to_screen([a.left, a.bottom]), to_screen([a.right, a.top]));
        painter.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(255, 40, 30, 40));
        painter.rect_stroke(r, 0.0, egui::Stroke::new(1.0, red), egui::StrokeKind::Inside);
    }
    for w in frame.windows(2).filter(|w| w[0].lit() && w[1].lit()) {
        let c = w[1].rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        painter.line_segment([to_screen([w[0].x, w[0].y]), to_screen([w[1].x, w[1].y])], egui::Stroke::new(1.5, egui::Color32::from_rgb(c[0], c[1], c[2])));
    }
    // Handles move points in scan-area units (-1..1, up = +y).
    let moved = |d: [f32; 2]| [d[0] * 2.0, -d[1] * 2.0];
    let clamp = |q: [f32; 2]| q.map(|v| v.clamp(-1.0, 1.0));
    for z in &mut p.zones {
        let on = *picked == Some(z.id);
        let colour = if on { egui::Color32::from_rgb(90, 220, 255) } else { egui::Color32::from_rgb(40, 110, 140) };
        let pts: Vec<egui::Pos2> = z.corners.iter().map(|c| to_screen(*c)).collect();
        painter.add(egui::Shape::closed_line(pts.clone(), egui::Stroke::new(if on { 1.5 } else { 1.0 }, colour)));
        painter.text(pts[3] + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, &z.name, egui::FontId::proportional(11.0), colour);
        for (i, at) in pts.iter().enumerate() {
            let mut d = [0.0; 2];
            let r = crate::panels::mapping::point_handle(ui, egui::Id::new(("laser_zone_corner", z.id, i)), *at, rect, colour, false, &mut d);
            if r.drag_started() || r.clicked() {
                *picked = Some(z.id);
            }
            if d != [0.0; 2] {
                let m = moved(d);
                z.corners[i] = clamp([z.corners[i][0] + m[0], z.corners[i][1] + m[1]]);
            }
        }
    }
    for (n, a) in p.dark.iter_mut().enumerate() {
        for (k, corner) in [(a.left, a.bottom), (a.right, a.top)].into_iter().enumerate() {
            let mut d = [0.0; 2];
            crate::panels::mapping::point_handle(ui, egui::Id::new(("laser_dark", n, k)), to_screen([corner.0, corner.1]), rect, red, true, &mut d);
            if d != [0.0; 2] {
                let m = moved(d);
                let (x, y) = if k == 0 { (&mut a.left, &mut a.bottom) } else { (&mut a.right, &mut a.top) };
                *x = (*x + m[0]).clamp(-1.0, 1.0);
                *y = (*y + m[1]).clamp(-1.0, 1.0);
            }
        }
    }
}
