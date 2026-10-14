//! Projection mapping editor.

use super::*;

/// Draws every surface's outline, corner handles and a centre handle over `frame` (the rect the
/// output occupies on screen) and applies drags. Returns true if anything was touched.
pub fn mapping_handles(ui: &mut egui::Ui, frame: egui::Rect, mapping: &mut Mapping, selected: &mut Option<u64>, labels: bool) -> bool {
    let to_screen = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let mut touched = false;
    for s in &mut mapping.surfaces {
        let is_selected = *selected == Some(s.id);
        let base = if s.enabled { ACCENT } else { Color32::GRAY };
        let line = if is_selected { Stroke::new(2.0, base) } else { Stroke::new(1.0, base.gamma_multiply(0.55)) };
        let pts: Vec<egui::Pos2> = s.corners.iter().map(|c| to_screen(*c)).collect();
        ui.painter().add(egui::Shape::closed_line(pts.clone(), line));
        let id = ui.id().with(("surface", s.id));

        let c = to_screen(s.center());
        let r = ui.interact(egui::Rect::from_center_size(c, egui::vec2(16.0, 16.0)), id.with("move"), Sense::click_and_drag());
        if r.clicked() || r.drag_started() {
            *selected = Some(s.id);
            touched = true;
        }
        if r.dragged() {
            let d = r.drag_delta();
            s.translate(d.x / frame.width(), d.y / frame.height());
            touched = true;
        }
        let fill = if r.hovered() || r.dragged() { Color32::WHITE } else { base };
        ui.painter().rect_filled(egui::Rect::from_center_size(c, egui::vec2(10.0, 10.0)), 2.0, fill);
        r.on_hover_text("Drag to move the whole surface");

        for (i, corner) in s.corners.iter_mut().enumerate() {
            let p = to_screen(*corner);
            let r = ui.interact(egui::Rect::from_center_size(p, egui::vec2(20.0, 20.0)), id.with(i), Sense::click_and_drag());
            if r.clicked() || r.drag_started() {
                *selected = Some(s.id);
                touched = true;
            }
            if r.dragged() {
                let d = r.drag_delta();
                corner[0] += d.x / frame.width();
                corner[1] += d.y / frame.height();
                touched = true;
            }
            let active = r.hovered() || r.dragged();
            ui.painter().circle(p, if active { 7.0 } else { 5.0 }, if active { Color32::WHITE } else { base }, Stroke::new(1.5, Color32::BLACK));
        }
        if labels {
            ui.painter().text(pts[0] + egui::vec2(10.0, 8.0), egui::Align2::LEFT_TOP, &s.name, egui::FontId::proportional(12.0), Color32::WHITE);
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
    });
    let oid = st.mapping_output;
    let tex = st.output_tex(oid);
    let mut edit_on_output = st.output.lock().unwrap().get(&oid).is_some_and(|o| o.edit_mapping);
    let canvas = (st.comp.width, st.comp.height);
    let mut add_surface = false;
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
    ui.weak("Drag corners to pin a surface onto the real object. Drag the centre to move it. Overlap soft edges to blend projectors.");

    let State { comp, selected_surface, .. } = st;
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
            ui.weak("No surfaces yet. Add one, then drag its corners.");
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
                guarded!(ui, s.opacity, Slider::new(&mut s.opacity, 0.0..=1.0).text("Opacity"));
                guarded!(ui, s.feather, Slider::new(&mut s.feather, 0.0..=0.5).text("Soft edge"));
                ui.collapsing("Input region (part of the mix shown)", |ui| {
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
