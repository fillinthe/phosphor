//! Preview panel and layer thumbnails.

use super::*;

/// A layer's live picture; dimmed when the layer is off, a placeholder when it has no source.
pub(crate) fn thumbnail(ui: &mut egui::Ui, tex: Option<egui::TextureId>, size: egui::Vec2, enabled: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let p = ui.painter();
    p.rect_filled(rect, 4.0, Color32::from_gray(10));
    match tex {
        Some(t) => {
            let tint = if enabled { Color32::WHITE } else { Color32::from_white_alpha(80) };
            p.image(t, rect, FULL_UV, tint);
        }
        None => {
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "no source", egui::FontId::proportional(11.0), Color32::GRAY);
        }
    }
    let stroke = if resp.hovered() { Stroke::new(1.0, accent()) } else { Stroke::new(1.0, Color32::from_gray(52)) };
    p.rect_stroke(rect, 4.0, stroke, StrokeKind::Inside);
    resp
}

/// The selected layer's corner pin on the preview: drag its four corners (the same handles as the mapping canvas).
fn corner_pin_handles(ui: &mut egui::Ui, st: &mut State, frame: egui::Rect) {
    let Some(layer) = st.selected.and_then(|id| st.comp.layer_mut(id)) else { return };
    let id = layer.id;
    let Some(corners) = &mut layer.transform.corners else { return };
    let at = |p: [f32; 2]| egui::pos2(frame.left() + p[0] * frame.width(), frame.top() + p[1] * frame.height());
    let pts: Vec<egui::Pos2> = corners.iter().map(|c| at(*c)).collect();
    ui.painter().add(egui::Shape::closed_line(pts.clone(), Stroke::new(1.0, accent())));
    for (k, c) in corners.iter_mut().enumerate() {
        let mut d = [0.0, 0.0];
        let r = super::mapping::point_handle(ui, egui::Id::new(("corner_pin", id, k)), pts[k], frame, accent(), false, &mut d);
        c[0] += d[0];
        c[1] += d[1];
        r.on_hover_text(["Top left", "Top right", "Bottom right", "Bottom left"][k]);
    }
}

pub(crate) fn preview(ui: &mut egui::Ui, st: &mut State) {
    egui::Panel::bottom("preview_bar").show_separator_line(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MASTER").small().strong());
            let w = (ui.available_width() - 140.0).max(80.0);
            Level::percent(&mut st.comp.master_fade, "Fade").modulation(&mut st.comp.master_fade_mod).target(ControlTarget::MasterFade).width(w - 26.0).tip("Master fade: everything that goes out").show(ui);
            crate::ui::icon_toggle(ui, &mut st.mouse_to_shaders, icon::CURSOR_CLICK).on_hover_text("Mouse into shaders: drag in the preview to move the selected layer's points (ISF point settings: centres, positions)");
            let r = super::inspector::mute_button(ui, &mut st.comp.sound_muted, "all sound");
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::MuteSound));
            if ui.button(icon::PLUS).on_hover_text("New preview window: the mix, a layer, a group or one output, as many as you like").clicked() {
                st.new_monitor = Some(MonitorSource::Mix);
            }
            let (stop, play) = (label(icon::STOP, "Output"), label(icon::PLAY, "Output"));
            let text = if st.output_open() { stop.clone() } else { play.clone() };
            if steady_button(ui, text, &[&stop, &play]).on_hover_text("Open the output window (F = fullscreen, E = edit mapping)").clicked() {
                st.toggle_output();
            }
        });
    });
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(6));
    let frame = fit_rect(rect.shrink(6.0), st.aspect());
    match st.texture {
        Some(tex) => paint_texture(ui.painter(), tex, frame),
        None => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GPU unavailable", egui::FontId::proportional(16.0), Color32::GRAY);
        }
    }
    let resp = ui.interact(rect, egui::Id::new("preview_area"), if st.mouse_to_shaders { Sense::click_and_drag() } else { Sense::click() });
    if st.mouse_to_shaders
        && (resp.dragged() || resp.clicked())
        && ui.input(|i| i.pointer.primary_down() || i.pointer.primary_released())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let uv = [(p.x - frame.left()) / frame.width(), (p.y - frame.top()) / frame.height()];
        mouse_into_shaders(st, uv, ui.input(|i| i.pointer.primary_down()));
    }
    // Eyedropper: a colour button is waiting for a click on the picture.
    if crate::ui::eyedropper::armed(ui.ctx()).is_some() {
        ui.painter().text(frame.left_top() + egui::vec2(8.0, 8.0), egui::Align2::LEFT_TOP, "Click the picture to pick a colour · Esc stops", egui::FontId::proportional(12.0), Color32::WHITE);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            crate::ui::eyedropper::arm(ui.ctx(), None);
        } else if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
            && frame.contains(p)
        {
            crate::ui::eyedropper::want(ui.ctx(), [(p.x - frame.left()) / frame.width(), (p.y - frame.top()) / frame.height()]);
        }
    }
    corner_pin_handles(ui, st, frame);
    resp.right_menu(|ui| {
        let open = st.output_open();
        if ui.button(label(if open { icon::STOP } else { icon::PLAY }, if open { "Close output" } else { "Open output" })).clicked() {
            st.toggle_output();
            ui.close();
        }
        ui.menu_button(label(icon::PROJECTOR_SCREEN, "Send output to"), |ui| {
            if st.displays.is_empty() {
                st.displays = crate::screens::displays();
            }
            for d in st.displays.clone() {
                if ui.button(d.label()).clicked() {
                    st.send_output_to(d);
                    ui.close();
                }
            }
        });
        if ui.button(label(icon::MONITOR, "New preview window")).clicked() {
            st.new_monitor = Some(MonitorSource::Mix);
            ui.close();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if ui.button(label(icon::CAMERA, "Save snapshot…")).clicked() {
            st.snapshot_requested = true;
            ui.close();
        }
        ui.separator();
        if ui.button(label(icon::BROADCAST, "Learn master fade")).clicked() {
            request(ui, LEARN_REQUEST, ControlTarget::MasterFade);
            ui.close();
        }
    });
    let s = st.stats.latest;
    let mut info = format!("{}×{}  {:.0} fps", st.comp.width, st.comp.height, s.fps);
    if st.comp.outputs.iter().any(|o| o.mapping.active()) {
        info = format!("mapped · {info}");
    }
    let font = egui::FontId::monospace(11.0);
    let galley = ui.painter().layout_no_wrap(info, font.clone(), Color32::from_white_alpha(170));
    let pill = egui::Rect::from_min_max(
        rect.right_top() + egui::vec2(-galley.size().x - 18.0, 6.0),
        rect.right_top() + egui::vec2(-4.0, galley.size().y + 14.0),
    );
    ui.painter().rect_filled(pill, 4.0, Color32::from_black_alpha(150));
    ui.painter().galley(pill.min + egui::vec2(7.0, 4.0), galley, Color32::WHITE);
    ui.allocate_rect(rect, Sense::hover());
    // Someone on a phone is changing the visuals: say so on the picture, with Take over one click away.
    let now = ui.ctx().input(|i| i.time);
    if let Some(who) = st.web_active(now).map(str::to_string) {
        let text = format!("{} {who} is controlling from the web · Take over", icon::DEVICE_MOBILE);
        let at = egui::Rect::from_min_size(rect.left_top() + egui::vec2(6.0, 6.0), egui::vec2(rect.width().min(380.0), 22.0));
        let take = ui.put(at, crate::ui::callout(text, crate::support::DONATE_ORANGE)).on_hover_text("Ignore phones and browsers until you hand control back (status line, or Output › Web Control)");
        if take.clicked() {
            st.set_web_control(false);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}

/// Puts the selected layer's shader points (its generator's and effects') where the mouse is in the picture, and
/// holds their Mouse down button while it's pressed (Shadertoy's iMouse).
fn mouse_into_shaders(st: &mut State, uv: [f32; 2], down: bool) {
    use phosphor_engine::shaders;
    let Some(layer) = st.selected.and_then(|id| st.comp.layer_mut(id)) else { return };
    if let SourceSpec::Generator(g) = &layer.source
        && let Some(d) = shaders::generator(g)
    {
        shaders::set_points(d.params, &mut layer.generator_values, uv);
        shaders::set_mouse_down(d.params, &mut layer.generator_values, down);
    }
    for fx in &mut layer.effects {
        if let Some(d) = shaders::effect(&fx.effect) {
            shaders::set_points(d.params, &mut fx.values, uv);
            shaders::set_mouse_down(d.params, &mut fx.values, down);
        }
    }
}
