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

/// Every non-ASCII character in the app's string literals must be in the UI fonts (missing ones draw as boxes).
#[cfg(test)]
#[test]
fn ui_text_has_glyphs() {
    let ctx = egui::Context::default();
    apply(&ctx);
    let _ = ctx.run_ui(Default::default(), |_| {});
    let mut missing = Vec::new();
    // The app, and the engine (its names and labels show in the UI too). Shaders and tests aren't UI.
    let mut stack = vec![
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/src")),
        std::path::PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../phosphor-engine/src")),
    ];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|e| e != "rs") || p.ends_with("tests.rs") {
                continue;
            }
            let text = std::fs::read_to_string(&p).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.trim_start();
                if code.starts_with("//") {
                    continue;
                }
                // Characters between quotes, before any trailing comment.
                let code = code.split(" // ").next().unwrap_or(code);
                let mut in_str = false;
                for c in code.chars() {
                    if c == '"' {
                        in_str = !in_str;
                    } else if in_str && !c.is_ascii() && !('\u{E000}'..='\u{F8FF}').contains(&c) {
                        let ok = ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.0), c));
                        if !ok {
                            missing.push(format!("{c} (U+{:04X}) {}:{}", c as u32, p.display(), n + 1));
                        }
                    }
                }
            }
        }
    }
    assert!(missing.is_empty(), "glyphs the UI font can't draw:\n{}", missing.join("\n"));
}
