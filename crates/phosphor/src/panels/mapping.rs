//! Projection mapping editor.

use super::*;

/// One draggable point on the mapping canvas. Returns its response; `delta` gets the drag in
/// output units (0..1).
pub(crate) fn point_handle(ui: &mut egui::Ui, id: egui::Id, at: egui::Pos2, frame: egui::Rect, color: Color32, square: bool, delta: &mut [f32; 2]) -> egui::Response {
    let r = ui.interact(egui::Rect::from_center_size(at, egui::vec2(18.0, 18.0)), id, Sense::click_and_drag());
    if r.dragged() {
        let d = r.drag_delta();
        *delta = [d.x / frame.width(), d.y / frame.height()];
    }
    let active = r.hovered() || r.dragged();
    let fill = if active { Color32::WHITE } else { color };
    if square {
        ui.painter().rect_filled(egui::Rect::from_center_size(at, egui::vec2(10.0, 10.0)), 2.0, fill);
    } else {
        ui.painter().circle(at, if active { 7.0 } else { 5.0 }, fill, Stroke::new(1.5, Color32::BLACK));
    }
    r
}

/// A bezier handle's end: a small hollow dot (points are filled), dragged like a point.
fn bezier_handle(ui: &mut egui::Ui, id: egui::Id, at: egui::Pos2, frame: egui::Rect, color: Color32, delta: &mut [f32; 2]) -> egui::Response {
    let r = ui.interact(egui::Rect::from_center_size(at, egui::vec2(14.0, 14.0)), id, Sense::drag());
    if r.dragged() {
        let d = r.drag_delta();
        *delta = [d.x / frame.width(), d.y / frame.height()];
    }
    let c = if r.hovered() || r.dragged() { Color32::WHITE } else { color };
    ui.painter().circle(at, 4.0, Color32::BLACK, Stroke::new(1.5, c));
    r
}

pub(crate) const MASK_COLOR: Color32 = Color32::from_rgb(255, 130, 80);

/// Draws every surface (corners or mesh grid) and mask over `frame` (the rect the output occupies
/// on screen) with draggable handles, and applies drags. Shared by the Mapping panel and the
/// output window's edit mode. Returns true if anything was touched.
pub fn mapping_handles(ui: &mut egui::Ui, frame: egui::Rect, mapping: &mut Mapping, selected: &mut Option<u64>, labels: bool) -> bool {
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let mut touched = false;
    let many = super::mapping_points::selected_slices(ui.ctx(), *selected);
    let shift = ui.input(|i| i.modifiers.shift);
    let mut group_move: Option<[f32; 2]> = None;
    let mut clicked: Option<u64> = None;
    for s in &mut mapping.surfaces {
        let is_selected = *selected == Some(s.id) || many.contains(&s.id);
        let base = if s.enabled { accent() } else { Color32::GRAY };
        let line = if is_selected { Stroke::new(2.0, base) } else { Stroke::new(1.0, base.gamma_multiply(0.55)) };
        let id = ui.id().with(("surface", s.id));
        // Outline: the corner quad, the curved edges, or the mesh's curved grid lines.
        match &s.mesh {
            Some(m) => {
                let steps = 24;
                for j in 0..=m.rows {
                    let t = j as f32 / m.rows as f32;
                    let pts: Vec<egui::Pos2> = (0..=steps).map(|k| to_screen(s.warp_at(k as f32 / steps as f32, t))).collect();
                    ui.painter().add(egui::Shape::line(pts, if j == 0 || j == m.rows { line } else { Stroke::new(1.0, base.gamma_multiply(0.35)) }));
                }
                for i in 0..=m.cols {
                    let u = i as f32 / m.cols as f32;
                    let pts: Vec<egui::Pos2> = (0..=steps).map(|k| to_screen(s.warp_at(u, k as f32 / steps as f32))).collect();
                    ui.painter().add(egui::Shape::line(pts, if i == 0 || i == m.cols { line } else { Stroke::new(1.0, base.gamma_multiply(0.35)) }));
                }
            }
            // A line slice draws its own polyline below; its corners aren't used.
            None if s.line.is_some() => {}
            // A 3D model: a light wireframe of what it covers on the output.
            None if s.model.is_some() => {
                if let Some(m) = &s.model {
                    let aspect = frame.width() / frame.height().max(1.0);
                    let tris = m.wireframe(aspect);
                    let step = (tris.len() / 3 / 600).max(1);
                    for t in tris.chunks_exact(3).step_by(step) {
                        let pts: Vec<egui::Pos2> = t.iter().map(|v| to_screen([v[0], v[1]])).collect();
                        ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.0, base.gamma_multiply(0.35))));
                    }
                }
            }
            None => {
                let pts: Vec<egui::Pos2> = if s.bend.is_some() { s.outline(24).into_iter().map(to_screen).collect() } else { s.corners.iter().map(|c| to_screen(*c)).collect() };
                ui.painter().add(egui::Shape::closed_line(pts, line));
            }
        }
        let mut d = [0.0, 0.0];
        let base = if s.locked { Color32::from_gray(120) } else { base };
        let r = point_handle(ui, id.with("move"), to_screen(s.center()), frame, base, true, &mut d);
        if r.clicked() || (r.drag_started() && !many.contains(&s.id)) {
            clicked = Some(s.id);
            super::mapping_points::select_point(ui.ctx(), None);
            touched = true;
        }
        if s.locked {
            d = [0.0, 0.0];
        }
        if d != [0.0, 0.0] {
            // Dragging one of several selected slices moves them all (after this loop).
            if many.len() > 1 && many.contains(&s.id) {
                group_move = Some(d);
            } else {
                s.translate(d[0], d[1]);
            }
            touched = true;
        }
        r.on_hover_text("Drag to move the whole slice · Shift-click to select several");
        // Line points, mesh points, or the four corners.
        if let Some(l) = &s.line {
            let pts: Vec<egui::Pos2> = l.points.iter().map(|p| to_screen(*p)).collect();
            ui.painter().add(egui::Shape::line(pts, line));
        }
        let locked = s.locked;
        let sid = s.id;
        let picked = super::mapping_points::selected_point(ui.ctx()).filter(|(p, _)| *p == sid).map(|(_, i)| i);
        // Mesh points bent by hand get a Straighten item.
        let bent: Vec<bool> = match (&s.line, &s.mesh) {
            (None, Some(m)) => (0..m.points.len()).map(|k| m.has_handles(k)).collect(),
            _ => Vec::new(),
        };
        let mut straighten = None;
        let points = if locked { Vec::new() } else { super::mapping_points::points_mut(s) };
        for (i, p) in points.into_iter().enumerate() {
            let mut d = [0.0, 0.0];
            let r = point_handle(ui, id.with(i), to_screen(*p), frame, base, false, &mut d);
            if bent.get(i).copied().unwrap_or(false) {
                r.clone().right_menu(|ui| {
                    if ui.button("Straighten").on_hover_text("This point's handles back to the automatic smooth curve").clicked() {
                        straighten = Some(i);
                        ui.close();
                    }
                });
            }
            if r.clicked() || r.drag_started() {
                *selected = Some(sid);
                super::mapping_points::select_point(ui.ctx(), Some((sid, i)));
                touched = true;
            }
            // The selected point (Tab moves it on; arrows nudge it).
            if picked == Some(i) && is_selected {
                ui.painter().circle_stroke(to_screen(*p), 10.0, Stroke::new(2.0, Color32::WHITE));
            }
            p[0] += d[0];
            p[1] += d[1];
            touched |= d != [0.0, 0.0];
        }
        if let (Some(k), Some(m)) = (straighten, s.mesh.as_mut()) {
            m.reset_handles(k);
            touched = true;
        }
        // Mesh handles (the selected mesh point): across and down, a tip each side; dragging one bends the mesh's
        // curve through the point (the other side mirrors it).
        if is_selected
            && !locked
            && s.line.is_none()
            && let (Some(k), Some(m)) = (picked, s.mesh.as_mut())
            && k < m.points.len()
        {
            let p = m.points[k];
            let dirs = m.direction(k);
            for (dir, m_dir) in dirs.iter().enumerate() {
                for sign in [1.0f32, -1.0] {
                    let tip = [p[0] + m_dir[0] * sign / 3.0, p[1] + m_dir[1] * sign / 3.0];
                    ui.painter().line_segment([to_screen(p), to_screen(tip)], Stroke::new(1.0, base.gamma_multiply(0.6)));
                    let mut d = [0.0, 0.0];
                    let hr = bezier_handle(ui, id.with(("mesh_handle", k, dir, sign as i8)), to_screen(tip), frame, base, &mut d);
                    if d != [0.0, 0.0] {
                        m.set_direction(k, dir, [(tip[0] + d[0] - p[0]) * 3.0 * sign, (tip[1] + d[1] - p[1]) * 3.0 * sign]);
                        touched = true;
                    }
                    hr.on_hover_text("Drag to bend the mesh through this point (the other side mirrors it). Right-click the point › Straighten to undo.");
                }
            }
        }
        // Curved edges (selected slice): two handles per edge, each joined to its corner.
        if is_selected
            && !locked
            && s.line.is_none()
            && s.model.is_none()
            && let Some(mut b) = s.bend
        {
            let corners = s.corners;
            for k in 0..8 {
                let at = b.handle_at(&corners, k);
                let anchor = phosphor_engine::mapping::Bend::anchor(&corners, k);
                ui.painter().line_segment([to_screen(anchor), to_screen(at)], Stroke::new(1.0, base.gamma_multiply(0.6)));
                let mut d = [0.0, 0.0];
                let r = bezier_handle(ui, id.with(("bend", k)), to_screen(at), frame, base, &mut d);
                if d != [0.0, 0.0] {
                    b.set_handle(&corners, k, [at[0] + d[0], at[1] + d[1]]);
                    touched = true;
                }
                r.on_hover_text("Drag to bend this edge (the picture follows)");
            }
            s.bend = Some(b);
        }
        // 3D model calibration: click a model point to pin it, drag pins to where those points really are.
        if is_selected
            && let Some(m) = &mut s.model
            && m.calibrating
        {
            let aspect = frame.width() / frame.height().max(1.0);
            let mesh = phosphor_engine::mapping::obj_mesh(&m.path);
            let mut changed = false;
            if let Ok(mesh) = mesh.as_ref() {
                for (k, v) in mesh.verts.iter().enumerate().take(2000) {
                    if m.pins.iter().any(|p| p.model == *v) {
                        continue;
                    }
                    let Some((xy, _)) = m.project_point(*v, aspect) else { continue };
                    let at = to_screen(xy);
                    let r = ui.interact(egui::Rect::from_center_size(at, egui::vec2(12.0, 12.0)), id.with(("vert", k)), Sense::click());
                    ui.painter().circle_filled(at, if r.hovered() { 5.0 } else { 3.0 }, if r.hovered() { Color32::WHITE } else { Color32::from_rgb(120, 200, 255) });
                    if r.on_hover_text("Pin this point, then drag the pin to where it really is").clicked() {
                        m.pins.push(phosphor_engine::mapping::Pin { model: *v, at: xy });
                        touched = true;
                    }
                }
            }
            let mut remove = None;
            let now: Vec<Option<[f32; 2]>> = m.pins.iter().map(|p| m.project_point(p.model, aspect).map(|x| x.0)).collect();
            for (k, pin) in m.pins.iter_mut().enumerate() {
                let mut d = [0.0, 0.0];
                let r = point_handle(ui, id.with(("pin", k)), to_screen(pin.at), frame, MASK_COLOR, true, &mut d);
                if d != [0.0, 0.0] {
                    pin.at = [pin.at[0] + d[0], pin.at[1] + d[1]];
                    changed = true;
                }
                // Where the model point is now, joined to its pin.
                if let Some(Some(now)) = now.get(k) {
                    ui.painter().line_segment([to_screen(*now), to_screen(pin.at)], Stroke::new(1.0, MASK_COLOR.gamma_multiply(0.6)));
                }
                if r.secondary_clicked() {
                    remove = Some(k);
                }
                r.on_hover_text("Drag to where this point is on the real object · right-click to remove");
            }
            if let Some(k) = remove {
                m.pins.remove(k);
                changed = true;
            }
            if changed {
                m.solve(aspect);
                touched = true;
            }
        }
        s.sync_corners();
        if labels {
            let at = if s.model.is_some() || s.line.is_some() { to_screen(s.center()) + egui::vec2(10.0, -18.0) } else { to_screen(s.corners[0]) + egui::vec2(10.0, 8.0) };
            ui.painter().text(at, egui::Align2::LEFT_TOP, &s.name, egui::FontId::proportional(12.0), Color32::WHITE);
        }
    }
    if let Some(id) = clicked {
        super::mapping_points::click_slice(ui.ctx(), selected, id, shift);
    }
    slice_drop(ui, frame, mapping);
    if let Some(d) = group_move {
        super::mapping_points::move_slices(mapping, &many, d);
    }
    // Masks: drag points, + between points adds one, right-click a point to delete it, make it a sharp corner or give
    // it bezier handles.
    use phosphor_engine::mapping::{mask_to_output, output_to_mask};
    let surfaces = &mapping.surfaces;
    for m in mapping.masks.iter_mut() {
        let id = ui.id().with(("mask", m.id));
        let color = if m.enabled { MASK_COLOR } else { Color32::GRAY };
        let is_selected = *selected == Some(m.id);
        // Attached masks live in their slice's square: draw and drag them where they land on the output.
        let out: Vec<[f32; 2]> = m.points.iter().map(|p| mask_to_output(surfaces, m, *p)).collect();
        let pts: Vec<egui::Pos2> = out.iter().map(|p| to_screen(*p)).collect();
        let placed = m.placed(surfaces);
        let outline: Vec<egui::Pos2> = placed.outline().iter().map(|p| to_screen(*p)).collect();
        if outline.len() >= 2 {
            ui.painter().add(egui::Shape::closed_line(outline, Stroke::new(if is_selected { 2.0 } else { 1.0 }, color)));
        }
        let n = out.len().max(1) as f32;
        let centre = out.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
        let mut d = [0.0, 0.0];
        let r = point_handle(ui, id.with("move"), to_screen(centre), frame, color, true, &mut d);
        if r.clicked() || r.drag_started() {
            *selected = Some(m.id);
            touched = true;
        }
        if d != [0.0, 0.0] {
            let moved: Vec<[f32; 2]> = out.iter().map(|p| output_to_mask(surfaces, m, [p[0] + d[0], p[1] + d[1]])).collect();
            m.points = moved;
            touched = true;
        }
        r.on_hover_text(format!("{}: drag to move the mask", m.name));
        let mut delete = None;
        let mut insert = None;
        let count = m.points.len();
        for i in 0..count {
            let mut d = [0.0, 0.0];
            let r = point_handle(ui, id.with(i), pts[i], frame, color, false, &mut d);
            if r.clicked() || r.drag_started() {
                *selected = Some(m.id);
            }
            if d != [0.0, 0.0] {
                m.points[i] = output_to_mask(surfaces, m, [out[i][0] + d[0], out[i][1] + d[1]]);
                touched = true;
            }
            if m.smooth && m.is_corner(i) {
                ui.painter().rect_stroke(egui::Rect::from_center_size(pts[i], egui::vec2(11.0, 11.0)), 0.0, Stroke::new(1.0, color), egui::StrokeKind::Middle);
            }
            // Bezier handles (selected mask only): drag either end; the other mirrors it.
            if is_selected && let Some(h) = placed.handles.get(i).copied().flatten() {
                for sign in [1.0f32, -1.0] {
                    let tip = [out[i][0] + h[0] * sign, out[i][1] + h[1] * sign];
                    ui.painter().line_segment([pts[i], to_screen(tip)], Stroke::new(1.0, color.gamma_multiply(0.7)));
                    let mut d = [0.0, 0.0];
                    let hr = bezier_handle(ui, id.with(("handle", i, sign as i8)), to_screen(tip), frame, color, &mut d);
                    if d != [0.0, 0.0] {
                        let local = output_to_mask(surfaces, m, [tip[0] + d[0], tip[1] + d[1]]);
                        let p = m.points[i];
                        m.set_handle(i, Some([(local[0] - p[0]) * sign, (local[1] - p[1]) * sign]));
                        touched = true;
                    }
                    hr.on_hover_text("Drag to bend the outline here (the other side mirrors it)");
                }
            }
            let smooth = m.smooth;
            let mut corner = m.is_corner(i);
            let mut bezier = m.handle(i).is_some();
            r.right_menu(|ui| {
                if crate::ui::toggle(ui, &mut bezier, "Bezier handles").on_hover_text("Two handles on this point to bend the outline by hand").changed() {
                    ui.close();
                }
                if smooth && crate::ui::toggle(ui, &mut corner, "Sharp corner").on_hover_text("The smooth outline makes a point here instead of curving through it").changed() {
                    ui.close();
                }
                if !smooth {
                    hint(ui, "Turn on Smooth to curve the whole outline; points can then be sharp corners.");
                }
                if count > 3 && ui.button(label(icon::TRASH, "Delete point")).clicked() {
                    delete = Some(i);
                    ui.close();
                }
            });
            if corner != m.is_corner(i) {
                m.set_corner(i, corner);
                touched = true;
            }
            if bezier != m.handle(i).is_some() {
                let h = bezier.then(|| m.auto_handle(i));
                m.set_handle(i, h);
                *selected = Some(m.id);
                touched = true;
            }
            // A small + halfway to the next point adds a point there.
            let next = pts[(i + 1) % count];
            let mid = pts[i] + (next - pts[i]) * 0.5;
            let a = ui.interact(egui::Rect::from_center_size(mid, egui::vec2(14.0, 14.0)), id.with(("add", i)), Sense::click());
            let c = if a.hovered() { Color32::WHITE } else { color.gamma_multiply(0.6) };
            ui.painter().circle_stroke(mid, 4.0, Stroke::new(1.0, c));
            ui.painter().text(mid, egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(9.0), c);
            if a.on_hover_text("Add a point here").clicked() {
                insert = Some(i + 1);
            }
        }
        if let Some(i) = delete {
            m.remove_point(i);
            touched = true;
        }
        if let Some(i) = insert {
            let (a, b) = (m.points[i - 1], m.points[i % m.points.len()]);
            m.insert_point(i, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
            *selected = Some(m.id);
            touched = true;
        }
        if labels && let Some(p) = pts.first() {
            ui.painter().text(*p + egui::vec2(8.0, -14.0), egui::Align2::LEFT_TOP, &m.name, egui::FontId::proportional(11.0), color);
        }
    }
    touched
}

/// Actions for all outputs at once: Identify, and All to their screens (when outputs remember screens).
pub(crate) fn outputs_actions(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    if ui.button(label(icon::HASH, "Identify")).on_hover_text("Each output shows its number").clicked() {
        st.identify_outputs(now);
    }
    if st.comp.outputs.iter().any(|o| o.enabled && !o.screen.is_empty())
        && ui.button(label(icon::PROJECTOR_SCREEN, "All to their screens")).on_hover_text("Open every output fullscreen on the screen it was last sent to").clicked()
    {
        let missing = st.send_outputs_to_screens();
        st.notice = (!missing.is_empty()).then(|| format!("Not connected right now: {}", missing.join(", ")));
    }
}

/// Every output side by side, live (what each projector shows), with where it's going: the way to see your outputs
/// and pick the one to edit. Click = edit it; right-click = window, screen, Identify, mapping, rename, remove;
/// the last card adds an output. Used by the Mapping Studio and the Mapping panel.
pub(crate) fn output_strip(ui: &mut egui::Ui, st: &mut State, height: f32) {
    let now = ui.ctx().input(|i| i.time);
    let canvas = (st.comp.width, st.comp.height);
    let displays = st.displays.clone();
    enum Do {
        Select(u64),
        Window(u64),
        Send(u64, crate::screens::Display),
        Identify,
        Remove(u64),
        Add,
    }
    let mut todo = Vec::new();
    let count = st.comp.outputs.len();
    egui::ScrollArea::horizontal().id_salt(("output_strip", height as i32)).auto_shrink([false, true]).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            for i in 0..count {
                let (id, (ow, oh)) = (st.comp.outputs[i].id, st.comp.outputs[i].size(canvas));
                let tex = st.output_tex(id);
                let open = st.output_window_open(id);
                let selected = st.mapping_output == id;
                let o = &mut st.comp.outputs[i];
                // Wide enough for its name and status even when the output is portrait.
                let pic_w = (height * ow as f32 / oh.max(1) as f32).clamp(height * 0.6, height * 2.4).max(118.0);
                let (rect, r) = ui.allocate_exact_size(egui::vec2(pic_w, height + 34.0), Sense::click());
                let pic = egui::Rect::from_min_size(rect.min, egui::vec2(pic_w, height));
                let p = ui.painter();
                p.rect_filled(pic, 4.0, Color32::BLACK);
                match tex.filter(|_| o.enabled) {
                    Some(t) => paint_texture(p, t, fit_rect(pic.shrink(2.0), ow as f32 / oh.max(1) as f32)),
                    None => {
                        p.text(pic.center(), egui::Align2::CENTER_CENTER, "off", egui::FontId::proportional(12.0), Color32::GRAY);
                    }
                }
                let border = if selected { accent() } else if r.hovered() { Color32::from_gray(140) } else { Color32::from_gray(50) };
                p.rect_stroke(pic, 4.0, Stroke::new(if selected { 2.0 } else { 1.0 }, border), egui::StrokeKind::Inside);
                // Number badge, like Identify shows on the projector.
                let badge = egui::Rect::from_min_size(pic.min + egui::vec2(4.0, 4.0), egui::vec2(18.0, 16.0));
                p.rect_filled(badge, 3.0, Color32::from_black_alpha(170));
                p.text(badge.center(), egui::Align2::CENTER_CENTER, format!("{}", i + 1), egui::FontId::proportional(11.0), accent());
                if o.mapping.active() {
                    p.text(pic.right_top() + egui::vec2(-5.0, 4.0), egui::Align2::RIGHT_TOP, icon::POLYGON, egui::FontId::proportional(12.0), accent());
                }
                // Name, then where it goes: on a screen, in a window, or nowhere yet.
                // Fit the card (about 6.5 px a character at these sizes).
                let fit = |text: &str, w: f32| {
                    let n = (w / 6.5).max(4.0) as usize;
                    if text.chars().count() > n { format!("{}…", text.chars().take(n - 1).collect::<String>()) } else { text.to_string() }
                };
                let name_at = egui::pos2(pic.left() + 2.0, pic.bottom() + 3.0);
                p.text(name_at, egui::Align2::LEFT_TOP, fit(&o.name, pic_w - 4.0), egui::FontId::proportional(12.0), if selected { accent() } else { Color32::WHITE });
                let on_screen = !o.screen.is_empty() && crate::screens::find_screen(&displays, &o.screen).is_some();
                let (dot, place) = match (open, on_screen) {
                    (true, true) => (accent(), format!("{} {}", icon::PROJECTOR_SCREEN, o.screen)),
                    (true, false) => (accent(), format!("{} window", icon::APP_WINDOW)),
                    (false, _) if !o.screen.is_empty() => (Color32::from_gray(110), format!("closed · {}", o.screen)),
                    _ => (Color32::from_gray(110), "not shown".to_string()),
                };
                let line = egui::pos2(pic.left() + 2.0, pic.bottom() + 19.0);
                p.circle_filled(line + egui::vec2(4.0, 6.0), 3.5, dot);
                let place = fit(&place, pic_w - 14.0);
                p.text(line + egui::vec2(12.0, 0.0), egui::Align2::LEFT_TOP, place, egui::FontId::proportional(11.0), Color32::from_gray(150));
                if r.clicked() {
                    todo.push(Do::Select(id));
                }
                if r.double_clicked() {
                    todo.push(Do::Window(id));
                }
                r.on_hover_text(format!("{} · {ow}×{oh}\nClick to edit its mapping · double-click to open / close its window · right-click for more", o.name)).right_menu(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut o.name).desired_width(160.0));
                    if ui.button(label(if open { icon::STOP } else { icon::PLAY }, if open { "Close window" } else { "Open window" })).clicked() {
                        todo.push(Do::Window(id));
                        ui.close();
                    }
                    ui.menu_button(label(icon::PROJECTOR_SCREEN, "Send to screen"), |ui| {
                        for d in &displays {
                            if ui.button(d.label()).clicked() {
                                todo.push(Do::Send(id, d.clone()));
                                ui.close();
                            }
                        }
                        if displays.is_empty() {
                            hint(ui, "No displays found");
                        }
                    });
                    if ui.button(label(icon::HASH, "Identify all")).on_hover_text("Each output shows its number").clicked() {
                        todo.push(Do::Identify);
                        ui.close();
                    }
                    crate::ui::toggle(ui, &mut o.enabled, "Output on");
                    crate::ui::toggle(ui, &mut o.mapping.enabled, "Mapping on");
                    if count > 1 && ui.button(label(icon::TRASH, "Remove output")).clicked() {
                        todo.push(Do::Remove(id));
                        ui.close();
                    }
                });
                ui.add_space(6.0);
            }
            // Add an output (top-aligned with the pictures).
            let (rect, r) = ui.allocate_exact_size(egui::vec2(height.max(80.0), height + 34.0), Sense::click());
            let rect = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), height));
            let c = if r.hovered() { Color32::WHITE } else { Color32::from_gray(110) };
            ui.painter().rect_stroke(rect.shrink(2.0), 4.0, Stroke::new(1.0, c), egui::StrokeKind::Inside);
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, label(icon::PLUS, "Output"), egui::FontId::proportional(12.0), c);
            if r.on_hover_text("Another projector, screen or feed").clicked() {
                todo.push(Do::Add);
            }
        });
    });
    for d in todo {
        match d {
            Do::Select(id) => {
                if st.mapping_output != id {
                    st.mapping_output = id;
                    st.selected_surface = None;
                }
            }
            Do::Window(id) => st.toggle_output_window(id),
            Do::Send(id, display) => st.send_output_id_to(id, display),
            Do::Identify => st.identify_outputs(now),
            Do::Remove(id) => {
                st.comp.outputs.retain(|o| o.id != id);
                st.close_stale_outputs();
                if st.mapping_output == id {
                    st.mapping_output = st.primary_output();
                }
            }
            Do::Add => {
                st.mapping_output = st.comp.add_output();
                st.selected_surface = None;
            }
        }
    }
}

pub(crate) fn mapping(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    if !st.comp.outputs.iter().any(|o| o.id == st.mapping_output) {
        st.mapping_output = st.primary_output();
    }
    // Every output, live; the selected one is edited below.
    output_strip(ui, st, 54.0);
    ui.horizontal(|ui| {
        venue_menu(ui, st);
        svg_menu(ui, st);
        outputs_actions(ui, st);
    });
    let oid = st.mapping_output;
    let tex = st.output_tex(oid);
    let mut edit_on_output = st.output.lock().unwrap().get(&oid).is_some_and(|o| o.edit_mapping);
    let canvas = (st.comp.width, st.comp.height);
    let mut add_surface = false;
    let mut add_mask: Option<phosphor_engine::mapping::MaskShape> = None;
    {
        let State { comp, output, .. } = st;
        let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
        ui.horizontal(|ui| {
            toggle(ui, &mut o.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on"))
                .on_hover_text("When on, this output shows only its surfaces");
            super::output::grid_toggle(ui, &mut o.grid);
            toggle(ui, &mut o.mapping.test_pattern, label(icon::CHECKERBOARD, "Slice test pattern"));
            if ui.button(label(icon::PLUS, "Surface")).clicked() {
                add_surface = true;
            }
            ui.menu_button(label(icon::PLUS, "Mask"), |ui| {
                for &shape in <phosphor_engine::mapping::MaskShape as phosphor_engine::Choice>::OPTIONS {
                    if ui.button(shape.name()).clicked() {
                        add_mask = Some(shape);
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("A shape that blacks out part of this output (doors, faces, the booth…); drag its points to fit");
            if crate::ui::toggle(ui, &mut edit_on_output, "Edit on output (E)").on_hover_text("Show draggable handles in this output's window").changed() {
                output.lock().unwrap().entry(oid).or_default().edit_mapping = edit_on_output;
            }
        });
    }
    if add_surface {
        let id = st.comp.new_id();
        if let Some(o) = st.comp.output_mut(oid) {
            o.mapping.add_surface(id);
            o.mapping.enabled = true;
        }
        st.selected_surface = Some(id);
    }
    if let Some(shape) = add_mask {
        let id = st.comp.new_id();
        if let Some(o) = st.comp.output_mut(oid) {
            let (w, h) = if o.width > 0 && o.height > 0 { (o.width, o.height) } else { canvas };
            o.mapping.add_mask_shape(id, shape, w as f32 / h.max(1) as f32);
            o.mapping.enabled = true;
        }
        st.selected_surface = Some(id);
    }
    hint(ui, "Drag corners to pin a surface onto the real object (or turn on Mesh warp for curved ones). Drag the centre to move it. Overlap soft edges to blend projectors. Orange shapes are masks.");

    let State { comp, selected_surface, .. } = st;
    let others = picture_sources(comp, oid);
    let (layers, groups) = (&mut comp.layers, &mut comp.groups);
    let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) else { return };
    let (ow, oh) = o.size(canvas);
    let aspect = ow as f32 / oh.max(1) as f32;
    let avail = ui.available_size();
    let h = (avail.y * 0.5).min(avail.x / aspect).max(120.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(avail.x, h), Sense::hover());
    let frame = fit_rect(rect.shrink(4.0), aspect);
    ui.painter().rect_filled(frame, 0.0, Color32::BLACK);
    if let Some(t) = tex {
        paint_texture(ui.painter(), t, frame);
    }
    ui.painter().rect_stroke(frame, 0.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Outside);
    mapping_handles(ui, frame, &mut o.mapping, selected_surface, true);

    ui.add_space(4.0);
    let mut remove = None;
    let mut duplicate = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().slider_width = (ui.available_width() - 180.0).max(80.0);
        if o.mapping.surfaces.is_empty() {
            hint(ui, "No surfaces yet. Add one, then drag its corners.");
        }
        for (i, s) in o.mapping.surfaces.iter_mut().enumerate() {
            let is_selected = *selected_surface == Some(s.id);
            let card_id = egui::Id::new(("surface_card", s.id));
            let bg = card_background(ui, card_id);
            let frame = card_frame(ui, is_selected).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    toggle_mapped(ui, &mut s.enabled, "ON", ControlTarget::SliceEnabled(s.id));
                    ui.add(egui::TextEdit::singleline(&mut s.name).desired_width(140.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete surface").clicked() {
                            remove = Some(i);
                        }
                        if ui.small_button(icon::CORNERS_OUT).on_hover_text("Reset corners to the full output").clicked() {
                            s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                        }
                    });
                });
                surface_settings(ui, s, layers, groups, &others);
            });
            remember_card(ui, card_id, frame.response.rect);
            let resp = bg.unwrap_or_else(|| frame.response.clone());
            if resp.clicked() {
                *selected_surface = Some(s.id);
            }
            resp.right_menu(|ui| {
                if ui.button(label(icon::CORNERS_OUT, "Reset corners")).clicked() {
                    s.corners = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
                    ui.close();
                }
                if ui.button(label(icon::COPY, "Duplicate")).clicked() {
                    duplicate = Some(i);
                    ui.close();
                }
                if ui.button(label(icon::TRASH, "Delete")).clicked() {
                    remove = Some(i);
                    ui.close();
                }
            });
            ui.add_space(4.0);
        }
        // Masks.
        let mut remove_mask = None;
        for (i, m) in o.mapping.masks.iter_mut().enumerate() {
            card_frame(ui, *selected_surface == Some(m.id)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new(icon::SELECTION).color(MASK_COLOR));
                    toggle(ui, &mut m.enabled, "ON");
                    ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(120.0));
                    hint(ui, format!("{} points", m.points.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete mask").clicked() {
                            remove_mask = Some(i);
                        }
                    });
                });
                mask_settings(ui, m);
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove_mask {
            o.mapping.masks.remove(i);
        }
    });
    if let Some(i) = remove {
        let removed = o.mapping.surfaces[i].id;
        o.mapping.remove_slices(|s| s.id == removed);
        if *selected_surface == Some(removed) {
            *selected_surface = None;
        }
    }
    if let Some(i) = duplicate {
        let sid = o.mapping.surfaces[i].id;
        comp.duplicate_slice(sid);
    }
}

/// A slice's settings: what it shows, shape, opacity, soft edge, colour, mesh warp, input region
/// (Mapping panel cards and the Mapping Studio).
/// Temp-data key: a surface asked for its 3D model file (the app opens the dialog).
pub(crate) const PICK_MODEL: &str = "phosphor_pick_model";

/// "Attached to": a mask follows a slice (moves and warps with it) or stays put on the output.
pub(crate) fn mask_attach_row(ui: &mut egui::Ui, mapping: &mut phosphor_engine::Mapping, mask: u64) {
    let Some(current) = mapping.masks.iter().find(|m| m.id == mask).map(|m| m.attached) else { return };
    let name = |id: Option<u64>| id.and_then(|id| mapping.surfaces.iter().find(|s| s.id == id)).map_or("Nothing (stays put)".to_string(), |s| s.name.clone());
    let mut pick = current;
    ui.horizontal(|ui| {
        ui.label("Attached to");
        egui::ComboBox::from_id_salt(("mask_attach", mask)).selected_text(name(current)).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut pick, None, "Nothing (stays put)");
            for s in &mapping.surfaces {
                crate::ui::pick_value(ui, &mut pick, Some(s.id), &s.name);
            }
        });
    })
    .response
    .on_hover_text("Attached, the mask moves and warps with that slice");
    if pick != current {
        mapping.attach_mask(mask, pick);
    }
}

/// Temp-data key: an output asked for a stage photo (the app opens the dialog).
pub(crate) const PICK_STAGE_PHOTO: &str = "phosphor_pick_stage_photo";
/// Temp-data key: a slice asked for a file to show (the app opens the dialog).
pub(crate) const PICK_SLICE_FILE: &str = "phosphor_pick_slice_file";
/// A media item, live input or layer dropped onto a slice on the mapping canvas (`SliceDrop`, for the app).
pub(crate) const SLICE_DROP: &str = "phosphor_slice_drop";

/// What was dropped onto slice `.0`.
#[derive(Clone)]
pub(crate) enum SliceDrop {
    Media(usize),
    Source(super::SourceDrag),
    Layer(u64),
}

/// Carries out a drop on slice `sid`: a file or live input gets a layer of its own shown there (not in the mix), a
/// layer is shown there.
pub(crate) fn apply_slice_drop(comp: &mut phosphor_engine::Composition, sid: u64, what: SliceDrop) {
    match what {
        SliceDrop::Media(i) => {
            if let Some(item) = comp.media_bin.get(i).cloned() {
                comp.show_file_on_slice(sid, &item.name, crate::media::spec_for_path(&item.path));
            }
        }
        SliceDrop::Source(d) => {
            comp.show_file_on_slice(sid, &d.name, d.source);
        }
        SliceDrop::Layer(l) => {
            if comp.layers.iter().any(|x| x.id == l)
                && let Some(s) = comp.slice_mut(sid)
            {
                s.layer = Some(l);
            }
        }
    }
}

#[cfg(test)]
mod slice_drop_tests {
    use super::*;

    #[test]
    fn drops_put_things_on_the_slice() {
        let mut comp = phosphor_engine::Composition::default();
        let base = comp.add_layer().id;
        comp.add_output();
        let sid = comp.new_id();
        comp.outputs[0].mapping.add_surface(sid);
        comp.add_media("clip.mov", "/x/clip.mov");
        apply_slice_drop(&mut comp, sid, SliceDrop::Media(0));
        let shown = comp.slice(sid).and_then(|s| s.layer).expect("a layer on the slice");
        let l = comp.layers.iter().find(|l| l.id == shown).unwrap();
        assert!(!l.in_mix && l.name == "clip.mov");
        apply_slice_drop(&mut comp, sid, SliceDrop::Source(crate::panels::SourceDrag { name: "Cam".into(), source: phosphor_engine::SourceSpec::None }));
        assert_eq!(comp.layers.len(), 3);
        apply_slice_drop(&mut comp, sid, SliceDrop::Layer(base));
        assert_eq!(comp.slice(sid).unwrap().layer, Some(base));
        apply_slice_drop(&mut comp, sid, SliceDrop::Layer(999));
        assert_eq!(comp.slice(sid).unwrap().layer, Some(base), "no such layer: nothing changes");
    }
}

/// Media, live inputs and layers dragged over the mapping canvas: the slice under the pointer lights up, and a drop
/// asks the app to show it there (`SLICE_DROP`).
fn slice_drop(ui: &mut egui::Ui, frame: egui::Rect, mapping: &Mapping) {
    let ctx = ui.ctx().clone();
    let carrying = egui::DragAndDrop::has_payload_of_type::<super::MediaDrag>(&ctx)
        || egui::DragAndDrop::has_payload_of_type::<super::SourceDrag>(&ctx)
        || egui::DragAndDrop::has_payload_of_type::<u64>(&ctx);
    let Some(pos) = ctx.pointer_hover_pos().filter(|p| carrying && frame.contains(*p)) else { return };
    let at = [(pos.x - frame.left()) / frame.width().max(1.0), (pos.y - frame.top()) / frame.height().max(1.0)];
    // The topmost slice under the pointer (later slices draw on top).
    let Some(s) = mapping.surfaces.iter().rev().find(|s| s.line.is_none() && s.model.is_none() && s.contains(at)) else { return };
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let outline: Vec<egui::Pos2> = s.outline(16).into_iter().map(to_screen).collect();
    ui.painter().add(egui::Shape::convex_polygon(outline, accent().gamma_multiply(0.25), Stroke::new(2.0, accent())));
    ui.painter().text(to_screen(s.center()), egui::Align2::CENTER_CENTER, format!("Show on {}", s.name), egui::FontId::proportional(13.0), Color32::WHITE);
    if !ctx.input(|i| i.pointer.any_released()) {
        return;
    }
    // `take_payload` drops whatever is carried even when it's another type: look before taking.
    use egui::DragAndDrop as Dnd;
    let what = if Dnd::has_payload_of_type::<super::MediaDrag>(&ctx) {
        Dnd::take_payload::<super::MediaDrag>(&ctx).map(|m| SliceDrop::Media(m.0))
    } else if Dnd::has_payload_of_type::<super::SourceDrag>(&ctx) {
        Dnd::take_payload::<super::SourceDrag>(&ctx).map(|d| SliceDrop::Source((*d).clone()))
    } else {
        Dnd::take_payload::<u64>(&ctx).map(|l| SliceDrop::Layer(*l))
    };
    let Some(what) = what else { return };
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(SLICE_DROP), (s.id, what)));
}

pub(crate) fn surface_settings(ui: &mut egui::Ui, s: &mut phosphor_engine::mapping::Surface, layers: &mut [phosphor_engine::LayerState], groups: &mut [phosphor_engine::LayerGroup], others: &[(u64, String)]) {
    surface_source(ui, s, layers, groups, others);
    ui.horizontal(|ui| {
        ui.label("Shape");
        choice_buttons(ui, &mut s.shape, |_, _| {});
    });
    ui.horizontal(|ui| {
        ui.label("Blend");
        choice_buttons(ui, &mut s.blend, |_, r| {
            r.clone().on_hover_text("How this slice lands on the slices under it (Add: overlaps get brighter)");
        });
    });
    if s.line.is_none() && s.model.is_none() {
        egui::CollapsingHeader::new(RichText::new("Corners, by number").small()).id_salt(("corner_numbers", s.id)).show(ui, |ui| {
            hint(ui, "Each corner as a share of the output (0–100%); type or drag for a fine move. On the canvas: arrows nudge the selected slice, Shift = 10 px.");
            for (k, name) in ["Top left", "Top right", "Bottom right", "Bottom left"].iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(*name).small());
                    for axis in 0..2 {
                        let mut v = s.corners[k][axis] * 100.0;
                        let r = guarded!(ui, v, egui::DragValue::new(&mut v).range(-100.0..=200.0).speed(0.02).fixed_decimals(2).suffix("%"));
                        if r.changed() {
                            s.corners[k][axis] = v / 100.0;
                        }
                    }
                });
            }
        });
    }
    ui.horizontal(|ui| {
        Level::percent(&mut s.opacity, "Opacity").default(1.0).target(ControlTarget::SliceOpacity(s.id)).show(ui);
    });
    ui.horizontal(|ui| {
        if s.edges.is_none() {
            Level::range(&mut s.feather, 0.0, 0.5, "Soft edge").default(0.0).show(ui);
        }
    });
    ui.horizontal(|ui| {
        let mut per_side = s.edges.is_some();
        if crate::ui::toggle(ui, &mut per_side, "Soft edge per side").on_hover_text("A different soft edge on each side: e.g. only the side that overlaps the next projector").changed() {
            s.edges = per_side.then_some([s.feather; 4]);
        }
        // (The light shows locked / not: one label keeps the row still.)
        toggle(ui, &mut s.locked, label(icon::LOCK, "Lock")).on_hover_text("Locked: its points can't be dragged or nudged");
    });
    if let Some(e) = s.edges.as_mut() {
        for (k, side) in ["Left", "Right", "Top", "Bottom"].iter().enumerate() {
            ui.horizontal(|ui| {
                Level::range(&mut e[k], 0.0, 0.5, *side).default(0.0).show(ui);
            });
        }
    }
    if s.feather > 0.0 {
        ui.horizontal(|ui| {
            Level::range(&mut s.blend_gamma, 0.5, 4.0, "Blend curve").default(1.0).tip("Shape of the soft edge. Where two projectors overlap, about 2.2 usually removes the bright band.").show(ui);
        });
    }
    color_grade_editor(ui, s.id, &mut s.color);
    ui.horizontal(|ui| {
        let mut on3d = s.model.is_some();
        if toggle(ui, &mut on3d, label(icon::CUBE, "3D model")).on_hover_text("Map onto a real object from its 3D model (OBJ with UVs): line the virtual camera up with the projector").changed() {
            if on3d {
                s.model = Some(phosphor_engine::mapping::Model3d::default());
                s.line = None;
                s.mesh = None;
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PICK_MODEL), s.id));
            } else {
                s.model = None;
            }
        }
        if let Some(m) = &mut s.model {
            let name = std::path::Path::new(&m.path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "no model yet".into());
            ui.label(egui::RichText::new(name).small());
            if ui.small_button("Choose…").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PICK_MODEL), s.id));
            }
        }
    });
    if let Some(m) = &mut s.model {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().slider_width = 110.0;
            ui.horizontal(|ui| {
                Level::range(&mut m.yaw, -0.5, 0.5, "Turn").default(0.08).width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.pitch, -0.25, 0.25, "Tilt").default(0.05).width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.roll, -0.25, 0.25, "Roll").default(0.0).width(150.0).show(ui);
            });
        });
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().slider_width = 110.0;
            ui.horizontal(|ui| {
                Level::range(&mut m.distance, 0.8, 12.0, "Distance").default(3.0).log().width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.fov, 10.0, 90.0, "Lens °").default(30.0).width(150.0).tip("The projector's vertical throw angle; a long-throw lens is narrow").show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.scale, 0.1, 4.0, "Size").default(1.0).log().width(150.0).show(ui);
            });
            crate::ui::toggle(ui, &mut m.cull, "Hide back faces");
        });
        // How the picture wraps the model (MadMapper's UV), then moved / sized / turned on it.
        ui.horizontal_wrapped(|ui| {
            ui.label("Picture");
            choice_combo(ui, ("model_uv", s.id), &mut m.uv_mode, 130.0, false)
                .on_hover_text("How the picture wraps the model: the model's own texture coordinates, thrown from one side, or Box (each face from the side it faces)");
            ui.horizontal(|ui| {
                Level::range(&mut m.uv_offset[0], -1.0, 1.0, "Move across").default(0.0).width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.uv_offset[1], -1.0, 1.0, "Move up").default(0.0).width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.uv_scale, 0.1, 10.0, "Picture size").default(1.0).log().width(150.0).show(ui);
            });
            ui.horizontal(|ui| {
                Level::range(&mut m.uv_turn, -0.5, 0.5, "Picture turn").default(0.0).width(150.0).show(ui);
            });
        });
        // Light on the faces and glowing edges (a building's corners and outline drawn in light).
        ui.horizontal_wrapped(|ui| {
            ui.horizontal(|ui| {
                Level::percent(&mut m.shade, "Light").default(0.0).width(150.0).tip("Shades the faces by a light, so the object looks solid (0 = the picture as it is)").show(ui);
            });
            ui.horizontal(|ui| {
                Level::percent(&mut m.shadows, "Shadows").default(0.0).width(150.0).tip("Darkens the parts of the model its other parts hide from the light (0 = none)").show(ui);
            });
            if m.shade > 0.0 || m.shadows > 0.0 {
                ui.horizontal(|ui| {
                    Level::range(&mut m.light[0], -0.5, 0.5, "Light from side").default(0.12).width(150.0).show(ui);
                });
                ui.horizontal(|ui| {
                    Level::range(&mut m.light[1], -0.25, 0.25, "Light from above").default(0.15).width(150.0).show(ui);
                });
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.horizontal(|ui| {
                Level::range(&mut m.edges, 0.0, 20.0, "Edges px").default(0.0).width(150.0).tip("Glowing lines along the model's corners and outline, this many pixels wide (0 = none)").show(ui);
            });
            if m.edges > 0.0 {
                ui.horizontal(|ui| {
                    Level::percent(&mut m.edge_glow, "Edge glow").default(1.0).width(150.0).show(ui);
                });
            }
        });
        ui.horizontal(|ui| {
            toggle(ui, &mut m.calibrating, label(icon::CROSSHAIR, "Calibrate")).on_hover_text("Pin model points and drag them onto the real object: from 4 pins the projector is solved for you (6+ also solves the lens)");
            if m.calibrating {
                let n = m.pins.len();
                hint(ui, if n < 4 { format!("{n} pinned, {} more to solve", 4 - n) } else if n < 6 { format!("{n} pinned: solving position (6 also solve the lens)") } else { format!("{n} pinned: solving position and lens") });
                if n > 0 && ui.small_button("Clear pins").clicked() {
                    m.pins.clear();
                }
            }
        });
        hint(ui, "Drag the dot on the canvas to move it, or Calibrate: click points on the model (blue), drag their pins (orange) to the real corners. The picture follows the model's UVs.");
    }
    ui.horizontal(|ui| {
        let mut on = s.line.is_some();
        if toggle(ui, &mut on, label(icon::LINE_SEGMENTS, "Line")).on_hover_text("The picture runs along a line with a width: outlines, edges, strips. Drag its points on the canvas").changed() {
            s.set_line(on);
        }
        if let Some(l) = &mut s.line {
            ui.horizontal(|ui| {
                Level::range(&mut l.width, 0.005, 0.5, "Width").default(0.06).log().show(ui);
            });
            if ui.small_button(label(icon::PLUS, "point")).on_hover_text("Adds a point after the last one").clicked() {
                let n = l.points.len();
                let last = l.points[n - 1];
                let prev = l.points[n.saturating_sub(2)];
                l.points.push([(last[0] * 2.0 - prev[0]).clamp(0.0, 1.0), (last[1] * 2.0 - prev[1]).clamp(0.0, 1.0)]);
            }
            if l.points.len() > 2 && ui.small_button(label(icon::MINUS, "point")).clicked() {
                l.points.pop();
            }
        }
    });
    ui.add_enabled_ui(s.line.is_none(), |ui| ui.horizontal(|ui| {
        let mut on = s.mesh.is_some();
        let (mut cols, mut rows) = s.mesh.as_ref().map_or((3, 3), |m| (m.cols, m.rows));
        let mut changed = toggle(ui, &mut on, label(icon::GRID_NINE, "Mesh warp")).on_hover_text("Bend the surface onto curved or uneven objects with a grid of points").changed();
        if on {
            changed |= guarded!(ui, cols, egui::DragValue::new(&mut cols).range(1..=8).prefix("cols ")).changed();
            changed |= guarded!(ui, rows, egui::DragValue::new(&mut rows).range(1..=8).prefix("rows ")).changed();
            if ui.small_button("Flatten").on_hover_text("Straighten the grid between the four corners").clicked() {
                s.mesh = Some(phosphor_engine::mapping::Mesh::from_corners(&s.corners, cols, rows));
            }
        }
        if changed {
            s.set_mesh(on, cols, rows);
        }
    }));
    ui.add_enabled_ui(s.line.is_none() && s.model.is_none(), |ui| ui.horizontal(|ui| {
        let mut on = s.bend.is_some();
        if toggle(ui, &mut on, label(icon::BEZIER_CURVE, "Curved edges")).on_hover_text("Two handles on each edge bend it (round tables, arches, curved screens); the picture follows").changed() {
            s.set_bend(on);
        }
        if s.bend.is_some_and(|b| !b.is_straight()) && ui.small_button("Straighten").on_hover_text("Edges back to straight lines").clicked() {
            s.set_bend(false);
            s.set_bend(true);
        }
    }));
    ui.collapsing("Input region (part of the picture shown)", |ui| {
        ui.horizontal(|ui| {
            Level::range(&mut s.input[0], 0.0, 1.0, "X").default(0.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut s.input[1], 0.0, 1.0, "Y").default(0.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut s.input[2], 0.05, 1.0, "Width").default(1.0).show(ui);
        });
        ui.horizontal(|ui| {
            Level::range(&mut s.input[3], 0.05, 1.0, "Height").default(1.0).show(ui);
        });
        ui.horizontal(|ui| {
            toggle(ui, &mut s.flip[0], label(icon::ARROWS_LEFT_RIGHT, "Flip")).on_hover_text("Mirror the picture left to right (rear projection)");
            toggle(ui, &mut s.flip[1], label(icon::ARROWS_DOWN_UP, "Flip")).on_hover_text("Turn the picture upside down (both flips = a ceiling-hung projector)");
        });
    });
}

/// A mask's settings: invert, smooth curve, soft edge.
pub(crate) fn mask_settings(ui: &mut egui::Ui, m: &mut phosphor_engine::mapping::Mask) {
    ui.horizontal(|ui| {
        toggle(ui, &mut m.invert, "Invert").on_hover_text("Black out everything outside the shape instead");
        toggle(ui, &mut m.smooth, "Smooth").on_hover_text("A smooth curve through the points instead of straight lines (+ between points adds one; right-click a point for a sharp corner or bezier handles)");
    });
    ui.horizontal(|ui| {
        Level::range(&mut m.feather, 0.0, 0.2, "Soft edge").default(0.0).tip("Fade the mask's edge (0 = hard)").show(ui);
    });
    if m.feather > 0.0 {
        ui.horizontal(|ui| {
            ui.label("Fade");
            choice_buttons(ui, &mut m.edge, |_, _| {});
        })
        .response
        .on_hover_text("Where the fade sits: centred on the outline, all inside the shape, or all outside it");
    }
}

/// Venues: save every output's setup (surfaces, mesh, masks, size, colour) under a name and recall it at the next gig.
/// What a surface shows: the mix or one layer; for a layer, whether it also stays in the mix.
/// `outputs`: the other outputs (id, name), whose finished picture a slice can show.
/// Whole pictures a slice can show besides layers and groups: comps (with two or more) and the other outputs,
/// labelled ("Comp: Side wall", "Output: Left").
pub(crate) fn picture_sources(comp: &phosphor_engine::Composition, except_output: u64) -> Vec<(u64, String)> {
    let comps = comp.comps.iter().filter(|_| comp.comps.len() > 1).map(|c| (c.id, format!("Comp: {}", c.name)));
    comps.chain(comp.outputs.iter().filter(|o| o.id != except_output).map(|o| (o.id, format!("Output: {}", o.name)))).collect()
}

pub(crate) fn surface_source(ui: &mut egui::Ui, s: &mut phosphor_engine::mapping::Surface, layers: &mut [phosphor_engine::LayerState], groups: &mut [phosphor_engine::LayerGroup], outputs: &[(u64, String)]) {
    ui.horizontal(|ui| {
        ui.label("Shows");
        let current = s.layer.map_or("Whole composition".into(), |id| match outputs.iter().find(|(o, _)| *o == id) {
            Some((_, name)) => name.clone(),
            None => groups.iter().find(|g| g.id == id).map_or_else(|| crate::ui::layer_name(layers, id), |g| g.name.clone()),
        });
        egui::ComboBox::from_id_salt(("surface_source", s.id)).width(170.0).selected_text(current).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut s.layer, None, "Whole composition");
            if ui.button(label(icon::FILE_VIDEO, "A file…")).on_hover_text("A picture or movie of its own on this slice (a layer that stays out of the mix)").clicked() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(PICK_SLICE_FILE), s.id));
                ui.close();
            }
            ui.separator();
            for g in groups.iter() {
                crate::ui::pick_value(ui, &mut s.layer, Some(g.id), format!("Group: {}", g.name));
            }
            crate::ui::layer_items(ui, layers, &mut s.layer, Some);
            if !outputs.is_empty() {
                ui.separator();
                for (id, name) in outputs {
                    crate::ui::pick_value(ui, &mut s.layer, Some(*id), name).on_hover_text("A comp's picture, or another output's finished picture (its slices, warps and masks)");
                }
            }
        })
        .response
        .on_hover_text("Send one layer to just this surface (its own picture, with its effects), or show the whole composition (everything in the mix, with MASTER effects)");
        let tip = "Off: only surfaces that pick it show it, not the mix or other surfaces";
        if let Some(l) = s.layer.and_then(|id| layers.iter_mut().find(|l| l.id == id)) {
            crate::ui::toggle(ui, &mut l.in_mix, "Also in the mix").on_hover_text(tip);
        } else if let Some(g) = s.layer.and_then(|id| groups.iter_mut().find(|g| g.id == id)) {
            crate::ui::toggle(ui, &mut g.in_mix, "Also in the mix").on_hover_text(tip);
        }
    });
}

/// The edited output's mapping as an SVG drawing, or shapes from one as masks.
pub(crate) fn svg_menu(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    ui.menu_button(label(icon::FILE_SVG, "SVG"), |ui| {
        let oid = st.mapping_output;
        if ui.button("Export the mapping…").on_hover_text("Every slice and mask of this output as a drawing (SVG for the venue, a cutter or another app; PDF to print; PNG for a picture). Pick the kind by the file name's ending.").clicked() {
            st.dialogs.save_file(crate::dialogs::Purpose::ExportMappingSvg(oid), ("Drawing", &["svg", "png", "pdf"]), "Mapping.svg");
            ui.close();
        }
        if ui.button("Masks and lines from a drawing…").on_hover_text("Each closed shape in an SVG becomes a mask on this output, each open line a line slice").clicked() {
            st.dialogs.pick_file(crate::dialogs::Purpose::ImportMasksSvg(oid), Some(("SVG", &["svg"])));
            ui.close();
        }
    });
}

pub(crate) fn venue_menu(ui: &mut egui::Ui, st: &mut State) {
    let State { venues, venue_name, comp, .. } = &mut *st;
    let action = saves_menu(ui, &label(icon::MAP_PIN, "Venues"), "Your whole projection setup: every output, its surfaces, warps, masks and colour.", venues, venue_name, "venue", || comp.venue_json());
    match action {
        Some(SaveAction::Load(json)) => match st.comp.apply_venue(&json) {
            Ok(()) => {
                st.close_stale_outputs();
                st.mapping_output = st.primary_output();
                st.notice = Some("Venue loaded: outputs and mapping replaced (Undo brings the previous ones back)".into());
            }
            Err(e) => st.notice = Some(format!("Couldn't load the venue: {e}")),
        },
        Some(SaveAction::Error(e)) => st.notice = Some(e),
        None => {}
    }
}
