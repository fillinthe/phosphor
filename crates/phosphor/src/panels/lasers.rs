//! Lasers panel: arm / stop, projectors (which controller box, which look, size, colour balance, safety floor)
//! and the Laser Designer (looks made of shapes, drawn on a canvas the way the laser draws them: the traced path,
//! glowing). Output: `crate::lasers`; geometry: `phosphor_engine::laser`.

use phosphor_engine::laser::{look_paths, trace};
use phosphor_engine::{LaserLook, LaserProjector, LaserShape, LaserShapeKind, MASTER_ID};

use super::*;

/// A controller box as the panel lists it (none in the browser: lasers need the Mac / PC app).
struct Dev {
    id: String,
    name: String,
    kind: String,
}

/// Which look and shape the Designer shows (kept for the session).
#[derive(Clone, Copy, Default)]
struct Picked {
    look: u64,
    shape: Option<u64>,
}

pub(crate) fn lasers(ui: &mut egui::Ui, st: &mut State) {
    let id = egui::Id::new("laser_picked");
    let mut picked: Picked = ui.ctx().data(|d| d.get_temp(id)).unwrap_or_default();
    #[cfg(not(target_arch = "wasm32"))]
    st.lasers.scan(st.mod_inputs.time as f64);
    safety_bar(ui, st);
    ui.separator();
    section(ui, "laser_projectors", label(icon::PROJECTOR_SCREEN, "Projectors"), true, |ui| projectors(ui, st));
    ui.separator();
    designer(ui, st, &mut picked);
    ui.ctx().data_mut(|d| d.insert_temp(id, picked));
}

/// Arm / stop and the master brightness: always at the top.
fn safety_bar(ui: &mut egui::Ui, st: &mut State) {
    let l = &mut st.comp.lasers;
    crate::ui::toolbar(ui, |ui| {
        let ready = l.projectors.iter().any(|p| p.enabled && !p.device.is_empty());
        if l.armed {
            if crate::ui::steady_action_button(ui, Act::Record, label(icon::STOP, "Stop lasers"), &["Stop lasers", "Arm lasers"]).on_hover_text("Every laser goes dark now").clicked() {
                l.armed = false;
            }
            ui.colored_label(egui::Color32::from_rgb(255, 80, 60), RichText::new(format!("{} LASERS ON", icon::WARNING)).strong());
        } else if ui
            .add_enabled_ui(ready, |ui| crate::ui::steady_action_button(ui, Act::Record, label(icon::LIGHTNING, "Arm lasers"), &["Stop lasers", "Arm lasers"]))
            .inner
            .on_hover_text("Lasers light up only while armed. Projects always open with them off; Blackout and closing Phosphor turn them off too")
            .on_disabled_hover_text("Add a projector and choose its controller box first")
            .clicked()
        {
            l.armed = true;
        }
        Level::percent(&mut l.master, "Lasers").default(1.0).width(150.0).target(ControlTarget::LaserBrightness(MASTER_ID)).tip("Every laser's brightness").show(ui);
    });
    help(ui, "Lasers can hurt eyes. Keep beams above the audience (each projector's Floor), start dim, and know where your emergency stop is.");
}

fn projectors(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    let devices: Vec<Dev> = st.lasers.devices().into_iter().map(|d| Dev { id: d.id, name: d.name, kind: d.kind }).collect();
    #[cfg(target_arch = "wasm32")]
    let devices: Vec<Dev> = Vec::new();
    let looks: Vec<(u64, String)> = st.comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
    let mut remove = None;
    for p in &mut st.comp.lasers.projectors {
        ui.push_id(p.id, |ui| {
            crate::ui::toolbar(ui, |ui| {
                crate::ui::toggle(ui, &mut p.enabled, "On").on_hover_text("Off: this laser stays dark even while armed");
                crate::ui::text_field(ui, &mut p.name, "Name", Some(110.0));
                let shown = devices.iter().find(|d| d.id == p.device).map_or_else(|| if p.device.is_empty() { "(choose its box)".to_string() } else { format!("{} (not found)", p.device) }, |d| format!("{} · {}", d.name, d.kind));
                crate::ui::dropdown(ui, ("laser_device", p.id), 220.0).selected_text(shown).show_ui(ui, |ui| {
                    if devices.is_empty() {
                        hint(ui, "Looking… (Ether Dream, IDN, LaserCube, Helios)");
                    }
                    for d in &devices {
                        crate::ui::pick_value(ui, &mut p.device, d.id.clone(), format!("{} · {}", d.name, d.kind));
                    }
                    crate::ui::pick_value(ui, &mut p.device, String::new(), "(none)");
                });
                let look_name = looks.iter().find(|(id, _)| *id == p.look).or(looks.first()).map_or("(no look yet)".to_string(), |l| l.1.clone());
                crate::ui::dropdown(ui, ("laser_look", p.id), 140.0).selected_text(look_name).show_ui(ui, |ui| {
                    for (id, name) in &looks {
                        crate::ui::pick_value(ui, &mut p.look, *id, name);
                    }
                });
                guarded!(ui, p.pps, egui::DragValue::new(&mut p.pps).range(1_000..=100_000).speed(100.0).suffix(" pts/s"))
                    .on_hover_text("Points a second the laser's scanners take: see its manual (30 000 suits most; too fast strains the scanners)");
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(s) = st.lasers.status.get(&p.id) {
                    hint(ui, s);
                }
                if crate::ui::danger_button(ui, label(icon::TRASH, "")).on_hover_text("Remove this projector").clicked() {
                    remove = Some(p.id);
                }
            });
            crate::ui::toolbar(ui, |ui| {
                Level::percent(&mut p.brightness, "Brightness").default(0.5).width(150.0).target(ControlTarget::LaserBrightness(p.id)).show(ui);
                Level::percent(&mut p.size, "Size").default(1.0).width(120.0).show(ui);
                Level::range(&mut p.x, -1.0, 1.0, "X").default(0.0).width(100.0).show(ui);
                Level::range(&mut p.y, -1.0, 1.0, "Y").default(0.0).width(100.0).show(ui);
                Level::range(&mut p.rotate, -180.0, 180.0, "Turn").default(0.0).width(110.0).show(ui);
                crate::ui::toggle(ui, &mut p.flip_x, "Flip across");
                crate::ui::toggle(ui, &mut p.flip_y, "Flip up/down");
            });
            crate::ui::toolbar(ui, |ui| {
                Level::range(&mut p.floor, -1.0, 1.0, "Floor").default(-1.0).width(150.0).tip("Audience safety: nothing is drawn below this height (-1 = no limit)").show(ui);
                for (i, name) in ["Red", "Green", "Blue"].iter().enumerate() {
                    Level::percent(&mut p.color[i], *name).default(1.0).width(100.0).show(ui);
                }
            });
            ui.separator();
        });
    }
    if let Some(id) = remove {
        st.comp.lasers.projectors.retain(|p| p.id != id);
    }
    if crate::ui::command(ui, label(icon::PLUS, "Projector")).on_hover_text("A laser projector: then choose its controller box").clicked() {
        let id = st.comp.new_id();
        let n = st.comp.lasers.projectors.len() + 1;
        let look = st.comp.lasers.looks.first().map_or(0, |l| l.id);
        st.comp.lasers.projectors.push(LaserProjector { id, name: format!("Laser {n}"), look, ..Default::default() });
    }
    if devices.is_empty() {
        help(ui, "Phosphor looks for laser controllers on the network (Ether Dream, IDN, LaserCube) and USB (Helios, LaserCube) while this panel is open.");
    }
}

fn designer(ui: &mut egui::Ui, st: &mut State, picked: &mut Picked) {
    crate::ui::heading(ui, format!("{} Laser Designer", icon::PEN_NIB));
    let comp = &mut st.comp;
    if comp.lasers.looks.is_empty() {
        help(ui, "A look is shapes the laser draws together: circles, stars, waves, Lissajous figures, beams. Make one, then pick it for a projector.");
        if crate::ui::command(ui, label(icon::SPARKLE, "Starter look")).on_hover_text("A spinning rainbow circle, a star and a Lissajous figure to start from").clicked() {
            let look = LaserLook::starter(|| comp.new_id());
            for p in comp.lasers.projectors.iter_mut().filter(|p| p.look == 0) {
                p.look = look.id;
            }
            picked.look = look.id;
            comp.lasers.looks.push(look);
        }
    }
    crate::ui::toolbar(ui, |ui| {
        let current = comp.lasers.look(picked.look).map(|l| (l.id, l.name.clone()));
        let names: Vec<(u64, String)> = comp.lasers.looks.iter().map(|l| (l.id, l.name.clone())).collect();
        if let Some((cur, name)) = &current {
            picked.look = *cur;
            crate::ui::dropdown(ui, "laser_design_look", 150.0).selected_text(name).show_ui(ui, |ui| {
                for (id, n) in &names {
                    if crate::ui::pick_item(ui, *id == *cur, n).clicked() {
                        picked.look = *id;
                        picked.shape = None;
                    }
                }
            });
        }
        if crate::ui::command(ui, label(icon::PLUS, "Look")).clicked() {
            let (look, shape) = (comp.new_id(), comp.new_id());
            let n = comp.lasers.looks.len() + 1;
            comp.lasers.looks.push(LaserLook { id: look, name: format!("Look {n}"), shapes: vec![LaserShape { id: shape, ..Default::default() }] });
            for p in comp.lasers.projectors.iter_mut().filter(|p| p.look == 0) {
                p.look = look;
            }
            *picked = Picked { look, shape: Some(shape) };
        }
        if let Some(look) = comp.lasers.look_mut(picked.look) {
            crate::ui::text_field(ui, &mut look.name, "Name", Some(120.0));
            ui.menu_button(label(icon::PLUS, "Shape"), |ui| {
                for kind in LaserShapeKind::ALL {
                    if crate::ui::pick_item(ui, false, kind.name()).clicked() {
                        let id = comp.next_id;
                        comp.next_id += 1;
                        let look = comp.lasers.look_mut(picked.look).expect("the look is there");
                        let color = phosphor_engine::laser::hue(look.shapes.len() as f32 * 0.17);
                        look.shapes.push(LaserShape { id, kind, color, ..Default::default() });
                        picked.shape = Some(id);
                        ui.close();
                    }
                }
            });
        }
        if current.is_some() && crate::ui::danger_button(ui, label(icon::TRASH, "Look")).on_hover_text("Delete this look").clicked() {
            comp.lasers.looks.retain(|l| l.id != picked.look);
            *picked = Picked::default();
        }
    });
    let t = st.mod_inputs.time as f64;
    let Some(look) = comp.lasers.look(picked.look).cloned() else { return };
    if !picked.shape.is_some_and(|id| look.shapes.iter().any(|s| s.id == id)) {
        picked.shape = look.shapes.first().map(|s| s.id);
    }
    ui.columns(2, |cols| {
        canvas(&mut cols[0], &look, t, &st.mod_inputs, picked, &mut comp.lasers);
        shape_list(&mut cols[1], comp, picked);
    });
}

/// The look drawn as the laser draws it (the traced path, lit parts glowing); click a shape to pick it, drag it to move it.
fn canvas(ui: &mut egui::Ui, look: &LaserLook, t: f64, mods: &phosphor_engine::ModInputs, picked: &mut Picked, lasers: &mut phosphor_engine::Lasers) {
    let side = ui.available_width().min(560.0).max(160.0);
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::click_and_drag());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, egui::Color32::from_rgb(4, 4, 8));
    let to_screen = |x: f32, y: f32| rect.center() + egui::vec2(x, -y) * (side * 0.5);
    let from_screen = |q: egui::Pos2| ((q.x - rect.center().x) / (side * 0.5), -(q.y - rect.center().y) / (side * 0.5));
    // Grid: centre lines and the scan area's edge.
    let grid = egui::Stroke::new(1.0, egui::Color32::from_gray(28));
    p.line_segment([to_screen(-1.0, 0.0), to_screen(1.0, 0.0)], grid);
    p.line_segment([to_screen(0.0, -1.0), to_screen(0.0, 1.0)], grid);
    // As the first projector showing this look draws it: its points a frame, and its safety floor (red).
    let projector = lasers.projectors.iter().find(|p| lasers.look(p.look).is_some_and(|l| l.id == look.id));
    let budget = projector.map_or(30_000, |p| p.pps) / phosphor_engine::laser::FPS;
    let floor = projector.map_or(-1.0, |p| p.floor);
    if floor > -1.0 {
        let r = egui::Rect::from_two_pos(to_screen(-1.0, floor), to_screen(1.0, -1.0));
        p.rect_filled(r, 0.0, egui::Color32::from_rgba_unmultiplied(255, 40, 30, 24));
        p.line_segment([to_screen(-1.0, floor), to_screen(1.0, floor)], egui::Stroke::new(1.0, egui::Color32::from_rgb(255, 60, 40)));
    }
    let frame = trace(&look_paths(look, t, mods), budget as usize);
    for w in frame.windows(2) {
        if !(w[0].lit() && w[1].lit()) {
            continue;
        }
        let c = w[1].rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0) as u8);
        let (a, b) = (to_screen(w[0].x, w[0].y), to_screen(w[1].x, w[1].y));
        // Below the projector's floor the laser stays dark: shown grey.
        let cut = w[0].y.min(w[1].y) < floor;
        let colour = if cut { egui::Color32::from_gray(70) } else { egui::Color32::from_rgb(c[0], c[1], c[2]) };
        if (a - b).length() < 0.5 {
            // A beam (the laser stays on one spot): a bright dot.
            p.circle_filled(a, 3.0, colour.gamma_multiply(0.35));
            p.circle_filled(a, 1.5, colour);
        } else {
            p.line_segment([a, b], egui::Stroke::new(4.0, colour.gamma_multiply(0.25)));
            p.line_segment([a, b], egui::Stroke::new(1.5, colour));
        }
    }
    // The picked shape: a ring where it is.
    if let Some(s) = picked.shape.and_then(|id| look.shapes.iter().find(|s| s.id == id)) {
        p.circle_stroke(to_screen(s.x, s.y), 6.0, egui::Stroke::new(1.0, egui::Color32::WHITE));
    }
    if let Some(pos) = resp.interact_pointer_pos() {
        let (x, y) = from_screen(pos);
        if resp.drag_started() || resp.clicked() {
            // The nearest shape centre within reach.
            picked.shape = look.shapes.iter().map(|s| (s.id, (s.x - x).powi(2) + (s.y - y).powi(2))).filter(|(_, d)| *d < 0.04).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(id, _)| id).or(picked.shape);
        }
        if resp.dragged()
            && let Some(s) = picked.shape.and_then(|id| lasers.shape_mut(id))
        {
            s.x = x.clamp(-1.0, 1.0);
            s.y = y.clamp(-1.0, 1.0);
        }
    }
    hint(ui, &format!("{} of {budget} points a frame", frame.len()));
    ui.ctx().request_repaint();
}

/// The look's shapes (on/off, pick, delete) and the picked one's settings.
fn shape_list(ui: &mut egui::Ui, comp: &mut phosphor_engine::Composition, picked: &mut Picked) {
    let Some(look) = comp.lasers.look_mut(picked.look) else { return };
    let mut remove = None;
    for (n, s) in look.shapes.iter_mut().enumerate() {
        crate::ui::toolbar(ui, |ui| {
            ui.push_id(s.id, |ui| {
                crate::ui::toggle(ui, &mut s.enabled, "");
                if crate::ui::pick_item(ui, picked.shape == Some(s.id), format!("{} {}", s.kind.name(), n + 1)).clicked() {
                    picked.shape = Some(s.id);
                }
                if crate::ui::danger_button(ui, label(icon::TRASH, "")).on_hover_text("Remove this shape").clicked() {
                    remove = Some(s.id);
                }
            });
        });
    }
    if let Some(id) = remove {
        look.shapes.retain(|s| s.id != id);
    }
    let Some(s) = picked.shape.and_then(|id| look.shapes.iter_mut().find(|s| s.id == id)) else {
        help(ui, "Pick a shape to change it, or drag it on the picture.");
        return;
    };
    ui.separator();
    crate::ui::toolbar(ui, |ui| {
        choice_combo(ui, ("laser_kind", s.id), &mut s.kind, 120.0, false);
        let mut c = s.color;
        if crate::ui::colour_button(ui, &mut c) {
            s.color = c;
        }
    });
    let id = s.id;
    let field = |i: u8| ControlTarget::LaserShape { shape: id, field: i };
    let w = ui.available_width().min(260.0);
    Level::range(&mut s.x, -1.0, 1.0, "X").default(0.0).width(w).modulation(&mut s.mods.x).target(field(0)).show(ui);
    Level::range(&mut s.y, -1.0, 1.0, "Y").default(0.0).width(w).modulation(&mut s.mods.y).target(field(1)).show(ui);
    Level::range(&mut s.size, 0.0, 1.0, "Size").default(0.5).width(w).modulation(&mut s.mods.size).target(field(2)).show(ui);
    Level::range(&mut s.rotate, -180.0, 180.0, "Turn").default(0.0).width(w).modulation(&mut s.mods.rotate).target(field(3)).show(ui);
    Level::range(&mut s.spin, -2.0, 2.0, "Spin").default(0.0).width(w).target(field(4)).tip("Turns a second").show(ui);
    Level::range(&mut s.draw, 0.0, 1.0, "Drawn").default(1.0).width(w).modulation(&mut s.mods.draw).target(field(5)).tip("How much of it shows: move it to draw the shape on").show(ui);
    Level::range(&mut s.rainbow, 0.0, 1.0, "Rainbow").default(0.0).width(w).target(field(6)).show(ui);
    Level::range(&mut s.motion, -2.0, 2.0, "Motion").default(0.25).width(w).target(field(7)).tip("Movement inside the shape: a wave scrolling, a figure turning").show(ui);
    if let Some((name, min, max)) = s.kind.count_name() {
        Level::range(&mut s.count, min, max, name).default(5.0).width(w).show(ui);
    }
    if let Some(name) = s.kind.shape_name() {
        Level::range(&mut s.shape, 0.0, 1.0, name).default(0.45).width(w).target(field(8)).show(ui);
    }
}
