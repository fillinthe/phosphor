//! Video tracking section (Modulators panel): watch a layer, group or the mix for faces, hands and a body; what's
//! found moves the `Track …` modulators, which any control can follow (right-click › Modulate).

use phosphor_engine::MASTER_ID;

use super::*;

pub(crate) fn tracking_section(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::section(ui, "tracking", label(icon::SCAN, "Video tracking"), st.comp.tracking.enabled, |ui| {
        let t = &mut st.comp.tracking;
        crate::ui::toolbar(ui, |ui| {
            toggle(ui, &mut t.enabled, "TRACKING").on_hover_text("Look for faces, hands and a body in the picture (Apple Vision, on this Mac)");
            let current = if t.source == MASTER_ID {
                "Mix".to_string()
            } else if let Some(g) = st.comp.groups.iter().find(|g| g.id == t.source) {
                format!("Group: {}", g.name)
            } else {
                format!("Layer: {}", crate::ui::layer_name(&st.comp.layers, t.source))
            };
            crate::ui::dropdown(ui, "tracking_src", 170.0).selected_text(current).show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut t.source, MASTER_ID, "Mix");
                ui.separator();
                crate::ui::layer_items(ui, &st.comp.layers, &mut t.source, |id| id);
                for g in &st.comp.groups {
                    crate::ui::pick_value(ui, &mut t.source, g.id, format!("Group: {}", g.name));
                }
            })
            .response
            .on_hover_text("What to watch: usually a camera layer");
        });
        crate::ui::toolbar(ui, |ui| {
            toggle(ui, &mut t.faces, "Faces").on_hover_text("The biggest face: Track Face X / Y / Size / Found");
            toggle(ui, &mut t.hands, "Hands").on_hover_text("The index fingertip: Track Hand X / Y, Track Pinch (thumb to index), Found");
            toggle(ui, &mut t.body, "Body").on_hover_text("The hips: Track Body X / Y, both wrists' height, Found");
            toggle(ui, &mut t.codes, "QR codes").on_hover_text("Read QR codes and barcodes: a text layer shows the last one (Live › QR code seen); Track Code Found = 1 while one is in view");
            toggle(ui, &mut t.read_text, "Text").on_hover_text("Read the words in the picture: a text layer shows them (Live › Text seen)");
        });
        ui.horizontal(|ui| Level::percent(&mut t.smoothing, "Smoothing").default(0.5).show(ui)).inner.on_hover_text("Higher = less jitter, a little more lag");
        if crate::ui::command(ui, label(icon::EYE_SLASH, "Hide faces")).on_hover_text("Adds a Hide Face effect to MASTER that follows the face it finds (pixelate, blur or black: change it in the effect)").clicked()
            && st.comp.add_face_hider(MASTER_ID).is_some()
        {
            st.notice = Some("Faces are hidden on MASTER (Hide Face effect; tracking on)".into());
        }
        let sel = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
        if crate::ui::command_if(ui, sel.is_some(), label(icon::SMILEY, "Selected layer on the face")).on_hover_text("The selected layer's position and size follow the face (a mask, a logo, a picture over the face). Its X, Y and Scale show the motion; Stop the motion there to undo.").clicked()
            && let Some(id) = sel
            && st.comp.follow_face(id)
        {
            st.notice = Some(format!("{} follows the face now", crate::ui::layer_name(&st.comp.layers, id)));
        }
        if !st.comp.tracking.enabled {
            help(ui, "Turn it on, pick a camera layer, then right-click any slider › Modulate › a Track modulator. To cut people out of the background, add the Remove Background effect to the layer instead (no tracking needed).");
            return;
        }
        let status = if st.tracker.status.is_empty() { "Starting…".to_string() } else { st.tracker.status.clone() };
        hint(ui, status);
        // What it's looking at, with a dot on everything it found.
        let Some(tex) = st.monitor_tex(crate::tracking::MONITOR_KEY) else { return };
        let width = ui.available_width().min(320.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(width, width / st.aspect()), Sense::hover());
        paint_texture(ui.painter(), tex, rect);
        let comp = &st.comp;
        let dot = |x: &str, y: &str, found: &str, color: Color32| {
            if comp.hand(found).unwrap_or(0.0) > 0.5
                && let (Some(x), Some(y)) = (comp.hand(x), comp.hand(y))
            {
                let at = egui::pos2(rect.left() + x * rect.width(), rect.bottom() - y * rect.height());
                ui.painter().circle(at, 6.0, color, Stroke::new(1.5, Color32::BLACK));
            }
        };
        let [fx, fy, _, ff] = phosphor_engine::FACE;
        let [hx, hy, _, hf] = phosphor_engine::HAND;
        let [bx, by, _, _, bf] = phosphor_engine::BODY;
        dot(fx, fy, ff, Color32::from_rgb(255, 210, 80));
        dot(hx, hy, hf, Color32::from_rgb(90, 220, 255));
        dot(bx, by, bf, Color32::from_rgb(170, 255, 120));
    });
}
