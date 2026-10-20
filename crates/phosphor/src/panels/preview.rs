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

pub(crate) fn preview(ui: &mut egui::Ui, st: &mut State) {
    egui::Panel::bottom("preview_bar").show_separator_line(false).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new("MASTER").small().strong());
            let w = (ui.available_width() - 110.0).max(80.0);
            Level::percent(&mut st.comp.master_fade, "Fade").modulation(&mut st.comp.master_fade_mod).target(ControlTarget::MasterFade).width(w - 26.0).tip("Master fade: everything that goes out").show(ui);
            let r = super::inspector::mute_button(ui, &mut st.comp.sound_muted, "all sound");
            map_menu(ui, &r, ControlTarget::App(phosphor_engine::AppControl::MuteSound));
            let text = if st.output_open() { label(icon::STOP, "Output") } else { label(icon::PLAY, "Output") };
            if ui.button(text).on_hover_text("Open the output window (F = fullscreen, E = edit mapping)").clicked() {
                st.toggle_output();
            }
        });
    });
    let rect = ui.available_rect_before_wrap();
    ui.painter().rect_filled(rect, 4.0, Color32::from_gray(6));
    match st.texture {
        Some(tex) => paint_texture(ui.painter(), tex, fit_rect(rect.shrink(6.0), st.aspect())),
        None => {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "GPU unavailable", egui::FontId::proportional(16.0), Color32::GRAY);
        }
    }
    let resp = ui.interact(rect, egui::Id::new("preview_area"), Sense::click());
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
        if ui.button(label(icon::MONITOR, "New monitor")).clicked() {
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
        let text = RichText::new(format!("{} {who} is controlling from the web · Take over", icon::DEVICE_MOBILE)).strong().color(Color32::BLACK);
        let at = egui::Rect::from_min_size(rect.left_top() + egui::vec2(6.0, 6.0), egui::vec2(rect.width().min(380.0), 22.0));
        let take = ui.put(at, egui::Button::new(text).fill(crate::support::DONATE_ORANGE)).on_hover_text("Ignore phones and browsers until you hand control back (status line, or Output › Web Control)");
        if take.clicked() {
            st.set_web_control(false);
        }
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    }
}
