//! Projection mapping editor.

use super::*;

/// One draggable point on the mapping canvas. Returns its response; `delta` gets the drag in
/// output units (0..1).
fn point_handle(ui: &mut egui::Ui, id: egui::Id, at: egui::Pos2, frame: egui::Rect, color: Color32, square: bool, delta: &mut [f32; 2]) -> egui::Response {
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

const MASK_COLOR: Color32 = Color32::from_rgb(255, 130, 80);

/// Draws every surface (corners or mesh grid) and mask over `frame` (the rect the output occupies
/// on screen) with draggable handles, and applies drags. Shared by the Mapping panel and the
/// output window's edit mode. Returns true if anything was touched.
pub fn mapping_handles(ui: &mut egui::Ui, frame: egui::Rect, mapping: &mut Mapping, selected: &mut Option<u64>, labels: bool) -> bool {
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let mut touched = false;
    for s in &mut mapping.surfaces {
        let is_selected = *selected == Some(s.id);
        let base = if s.enabled { ACCENT } else { Color32::GRAY };
        let line = if is_selected { Stroke::new(2.0, base) } else { Stroke::new(1.0, base.gamma_multiply(0.55)) };
        let id = ui.id().with(("surface", s.id));
        // Outline: the corner quad, or the mesh's curved grid lines.
        match &s.mesh {
            Some(m) => {
                let steps = 24;
                for j in 0..=m.rows {
                    let t = j as f32 / m.rows as f32;
                    let pts: Vec<egui::Pos2> = (0..=steps).map(|k| to_screen(m.eval(k as f32 / steps as f32, t))).collect();
                    ui.painter().add(egui::Shape::line(pts, if j == 0 || j == m.rows { line } else { Stroke::new(1.0, base.gamma_multiply(0.35)) }));
                }
                for i in 0..=m.cols {
                    let u = i as f32 / m.cols as f32;
                    let pts: Vec<egui::Pos2> = (0..=steps).map(|k| to_screen(m.eval(u, k as f32 / steps as f32))).collect();
                    ui.painter().add(egui::Shape::line(pts, if i == 0 || i == m.cols { line } else { Stroke::new(1.0, base.gamma_multiply(0.35)) }));
                }
            }
            None => {
                let pts: Vec<egui::Pos2> = s.corners.iter().map(|c| to_screen(*c)).collect();
                ui.painter().add(egui::Shape::closed_line(pts, line));
            }
        }
        let mut d = [0.0, 0.0];
        let r = point_handle(ui, id.with("move"), to_screen(s.center()), frame, base, true, &mut d);
        if r.clicked() || r.drag_started() {
            *selected = Some(s.id);
            touched = true;
        }
        if d != [0.0, 0.0] {
            s.translate(d[0], d[1]);
            touched = true;
        }
        r.on_hover_text("Drag to move the whole surface");
        // Mesh points, or the four corners.
        let points: Vec<&mut [f32; 2]> = match &mut s.mesh {
            Some(m) => m.points.iter_mut().collect(),
            None => s.corners.iter_mut().collect(),
        };
        for (i, p) in points.into_iter().enumerate() {
            let mut d = [0.0, 0.0];
            let r = point_handle(ui, id.with(i), to_screen(*p), frame, base, false, &mut d);
            if r.clicked() || r.drag_started() {
                *selected = Some(s.id);
                touched = true;
            }
            p[0] += d[0];
            p[1] += d[1];
            touched |= d != [0.0, 0.0];
        }
        s.sync_corners();
        if labels {
            ui.painter().text(to_screen(s.corners[0]) + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, &s.name, egui::FontId::proportional(12.0), Color32::WHITE);
        }
    }
    // Masks: drag points, + between points adds one, right-click a point to delete it.
    for m in &mut mapping.masks {
        let id = ui.id().with(("mask", m.id));
        let color = if m.enabled { MASK_COLOR } else { Color32::GRAY };
        let is_selected = *selected == Some(m.id);
        let pts: Vec<egui::Pos2> = m.points.iter().map(|p| to_screen(*p)).collect();
        if pts.len() >= 2 {
            ui.painter().add(egui::Shape::closed_line(pts.clone(), Stroke::new(if is_selected { 2.0 } else { 1.0 }, color)));
        }
        let n = m.points.len().max(1) as f32;
        let centre = m.points.iter().fold([0.0, 0.0], |a, p| [a[0] + p[0] / n, a[1] + p[1] / n]);
        let mut d = [0.0, 0.0];
        let r = point_handle(ui, id.with("move"), to_screen(centre), frame, color, true, &mut d);
        if r.clicked() || r.drag_started() {
            *selected = Some(m.id);
            touched = true;
        }
        if d != [0.0, 0.0] {
            m.points.iter_mut().for_each(|p| {
                p[0] += d[0];
                p[1] += d[1];
            });
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
            m.points[i][0] += d[0];
            m.points[i][1] += d[1];
            touched |= d != [0.0, 0.0];
            if count > 3 {
                r.right_menu(|ui| {
                    if ui.button(label(icon::TRASH, "Delete point")).clicked() {
                        delete = Some(i);
                        ui.close();
                    }
                });
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
            m.points.remove(i);
            touched = true;
        }
        if let Some(i) = insert {
            let (a, b) = (m.points[i - 1], m.points[i % m.points.len()]);
            m.points.insert(i, [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]);
            *selected = Some(m.id);
            touched = true;
        }
        if labels && let Some(p) = pts.first() {
            ui.painter().text(*p + egui::vec2(8.0, -14.0), egui::Align2::LEFT_TOP, &m.name, egui::FontId::proportional(11.0), color);
        }
    }
    touched
}

pub(crate) fn mapping(ui: &mut egui::Ui, st: &mut State) {
    st.comp.ensure_outputs();
    if !st.comp.outputs.iter().any(|o| o.id == st.mapping_output) {
        st.mapping_output = st.primary_output();
    }
    // Which output's surfaces are being edited.
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Output").strong());
        let outs: Vec<(u64, String)> = st.comp.outputs.iter().map(|o| (o.id, o.name.clone())).collect();
        for (id, name) in outs {
            ui.selectable_value(&mut st.mapping_output, id, name);
        }
        if ui.small_button(label(icon::PLUS, "Output")).on_hover_text("Add another output (projector / screen)").clicked() {
            st.mapping_output = st.comp.add_output();
        }
        venue_menu(ui, st);
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
            ui.toggle_value(&mut o.mapping.enabled, label(icon::PROJECTOR_SCREEN, "Mapping on"))
                .on_hover_text("When on, this output shows only its surfaces");
            ui.toggle_value(&mut o.mapping.test_pattern, label(icon::GRID_FOUR, "Alignment grid"));
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
            if ui.checkbox(&mut edit_on_output, "Edit on output (E)").on_hover_text("Show draggable handles in this output's window").changed() {
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
    let layers = &mut comp.layers;
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
                    ui.toggle_value(&mut s.enabled, RichText::new("ON").small().strong());
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
                surface_source(ui, s, layers);
                guarded!(ui, s.opacity, Slider::new(&mut s.opacity, 0.0..=1.0).text("Opacity"));
                guarded!(ui, s.feather, Slider::new(&mut s.feather, 0.0..=0.5).text("Soft edge"));
                if s.feather > 0.0 {
                    guarded!(ui, s.blend_gamma, Slider::new(&mut s.blend_gamma, 0.5..=4.0).text("Blend curve").fixed_decimals(2))
                        .on_hover_text("Shape of the soft edge. Where two projectors overlap, about 2.2 usually removes the bright band.");
                }
                ui.horizontal(|ui| {
                    let mut on = s.mesh.is_some();
                    let (mut cols, mut rows) = s.mesh.as_ref().map_or((3, 3), |m| (m.cols, m.rows));
                    let mut changed = ui.toggle_value(&mut on, label(icon::GRID_NINE, "Mesh warp")).on_hover_text("Bend the surface onto curved or uneven objects with a grid of points").changed();
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
                });
                ui.collapsing("Input region (part of the picture shown)", |ui| {
                    guarded!(ui, s.input[0], Slider::new(&mut s.input[0], 0.0..=1.0).text("X"));
                    guarded!(ui, s.input[1], Slider::new(&mut s.input[1], 0.0..=1.0).text("Y"));
                    guarded!(ui, s.input[2], Slider::new(&mut s.input[2], 0.05..=1.0).text("Width"));
                    guarded!(ui, s.input[3], Slider::new(&mut s.input[3], 0.05..=1.0).text("Height"));
                });
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
                    ui.toggle_value(&mut m.enabled, RichText::new("ON").small().strong());
                    ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(120.0));
                    ui.toggle_value(&mut m.invert, "Invert").on_hover_text("Black out everything outside the shape instead");
                    hint(ui, format!("{} points", m.points.len()));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(icon::TRASH).on_hover_text("Delete mask").clicked() {
                            remove_mask = Some(i);
                        }
                    });
                });
                ui.horizontal(|ui| {
                    ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(60.0);
                    let r = guarded!(ui, m.feather, Slider::new(&mut m.feather, 0.0..=0.2).text("Soft edge").show_value(false))
                        .on_hover_text("Fade the mask's edge (0 = hard); the fade is centred on the outline");
                    value_bubble(ui, &r, if m.feather > 0.0 { format!("{:.1}% of the output", m.feather * 100.0) } else { "hard edge".into() });
                });
            });
            ui.add_space(4.0);
        }
        if let Some(i) = remove_mask {
            o.mapping.masks.remove(i);
        }
    });
    if let Some(i) = remove {
        let removed = o.mapping.surfaces.remove(i);
        if *selected_surface == Some(removed.id) {
            *selected_surface = None;
        }
    }
    if let Some(i) = duplicate {
        let mut copy = o.mapping.surfaces[i].clone();
        copy.id = comp.next_id;
        comp.next_id += 1;
        copy.name = format!("{} copy", copy.name);
        for c in &mut copy.corners {
            c[0] += 0.03;
            c[1] += 0.03;
        }
        if let Some(o) = comp.outputs.iter_mut().find(|o| o.id == oid) {
            o.mapping.surfaces.insert(i + 1, copy);
        }
    }
}

/// Venues: save every output's setup (surfaces, mesh, masks, size, colour) under a name and recall it at the next gig.
/// What a surface shows: the mix or one layer; for a layer, whether it also stays in the mix.
fn surface_source(ui: &mut egui::Ui, s: &mut phosphor_engine::mapping::Surface, layers: &mut [phosphor_engine::LayerState]) {
    ui.horizontal(|ui| {
        ui.label("Shows");
        let current = s.layer.map_or("Mix (all layers)".into(), |id| crate::ui::layer_name(layers, id));
        egui::ComboBox::from_id_salt(("surface_source", s.id)).width(170.0).selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut s.layer, None, "Mix (all layers)");
            ui.separator();
            crate::ui::layer_items(ui, layers, &mut s.layer, Some);
        })
        .response
        .on_hover_text("Send one layer to just this surface (its own picture, with its effects), or show the whole mix");
        if let Some(l) = s.layer.and_then(|id| layers.iter_mut().find(|l| l.id == id)) {
            ui.checkbox(&mut l.in_mix, "Also in the mix")
                .on_hover_text("Off: this layer only appears on surfaces that pick it, not in the mix or on other surfaces");
        }
    });
}

fn venue_menu(ui: &mut egui::Ui, st: &mut State) {
    let action = ui
        .menu_button(label(icon::MAP_PIN, "Venues"), |ui| {
            ui.set_min_width(300.0);
            hint(ui, "Your whole projection setup: every output, its surfaces, warps, masks and colour.");
            let State { venues, venue_name, comp, .. } = &mut *st;
            named_saves(ui, venues, venue_name, "venue", || comp.venue_json())
        })
        .inner
        .flatten();
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
