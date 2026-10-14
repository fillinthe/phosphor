//! Phosphor's look: near-black panels, a single green accent, soft corners.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(120, 255, 170);
pub const WARN: Color32 = Color32::from_rgb(255, 140, 90);

pub fn apply(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Dark);
    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(20, 22, 24);
    v.window_fill = Color32::from_rgb(26, 28, 31);
    v.extreme_bg_color = Color32::from_rgb(12, 13, 15);
    v.faint_bg_color = Color32::from_rgb(29, 32, 35);
    v.widgets.noninteractive.bg_fill = Color32::from_rgb(26, 28, 31);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(44, 48, 52));
    v.widgets.inactive.bg_fill = Color32::from_rgb(42, 46, 50);
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(36, 40, 44);
    v.widgets.hovered.bg_fill = Color32::from_rgb(56, 62, 68);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(48, 54, 60);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, ACCENT.gamma_multiply(0.6));
    v.widgets.active.bg_fill = ACCENT.gamma_multiply(0.35);
    v.widgets.open.bg_fill = Color32::from_rgb(48, 54, 60);
    v.selection.bg_fill = ACCENT.gamma_multiply(0.32);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = ACCENT;
    // Sliders: a thick filled bar with a thin line where the value sits (no round knob).
    v.slider_trailing_fill = true;
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.18 };
    v.window_corner_radius = CornerRadius::same(8);
    v.menu_corner_radius = CornerRadius::same(6);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(5);
    }
    ctx.set_visuals(v);
    ctx.style_mut_of(egui::Theme::Dark, |s| {
        use egui::{FontId, TextStyle};
        // Compact: smaller labels so more controls fit on screen.
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(9.0)),
            (TextStyle::Body, FontId::proportional(11.5)),
            (TextStyle::Button, FontId::proportional(11.5)),
            (TextStyle::Heading, FontId::proportional(14.0)),
            (TextStyle::Monospace, FontId::monospace(10.5)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(7.0, 3.0);
        s.spacing.slider_width = 150.0;
        s.spacing.slider_rail_height = 10.0;
        s.spacing.interact_size.y = 19.0;
        s.spacing.menu_margin = egui::Margin::same(6);
    });
}
