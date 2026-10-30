//! Phosphor's look: near-black panels (or as light as you like: `set_brightness`), one green accent, soft corners.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const WARN: Color32 = Color32::from_rgb(255, 140, 90);

/// True while the interface is on the light side (`set_brightness` above about half).
static LIGHT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Which accent colour (index into `ACCENTS`).
static ACCENT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// The accent colours to choose from: (name, on dark backgrounds, on light ones). Gold means "mapped" and orange
/// "warning", so neither is offered.
pub const ACCENTS: [(&str, [u8; 3], [u8; 3]); 5] = [
    ("Green", [120, 255, 170], [16, 138, 74]),
    ("Cyan", [90, 225, 255], [0, 118, 150]),
    ("Violet", [190, 160, 255], [104, 70, 190]),
    ("Pink", [255, 140, 200], [176, 40, 110]),
    ("Ice", [200, 230, 255], [60, 100, 150]),
];

/// The accent colour: bright on dark backgrounds, deeper on light ones (readable as text).
pub fn accent() -> Color32 {
    let (_, dark, light) = ACCENTS[ACCENT.load(std::sync::atomic::Ordering::Relaxed).min(ACCENTS.len() - 1)];
    let [r, g, b] = if LIGHT.load(std::sync::atomic::Ordering::Relaxed) { light } else { dark };
    Color32::from_rgb(r, g, b)
}

/// Picks the accent colour (call `set_brightness` after to repaint the widgets with it).
pub fn set_accent(i: usize) {
    ACCENT.store(i.min(ACCENTS.len() - 1), std::sync::atomic::Ordering::Relaxed);
}

pub fn accent_index() -> usize {
    ACCENT.load(std::sync::atomic::Ordering::Relaxed)
}

/// `base` nudged toward the accent (selected cards, the playing clip): works on any brightness.
pub fn tint(base: Color32, amount: f32) -> Color32 {
    base.lerp_to_gamma(accent(), amount)
}

/// What kind of thing something is, shown by one colour everywhere (a panel's tab, a stripe on a clip or layer):
/// so a glance tells a movie from a generator from a camera, and an output panel from a controller panel. The
/// accent stays for "on / playing / selected"; gold means mapped, orange a warning, red record / delete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hue {
    /// Movies, pictures, the media bin, online media.
    Media,
    /// Generators, effects, shaders, visualisers.
    Effect,
    /// Cameras, screens, streams, NDI / Syphon in, sound in.
    Live,
    /// Text layers.
    Text,
    /// Outputs, mapping, screens, LEDs, streaming.
    Output,
    /// MIDI, OSC, controllers, modulators, the control surface.
    Control,
}

impl Hue {
    /// Its colour: bright on dark backgrounds, deeper on light ones.
    pub fn color(self) -> Color32 {
        let (dark, light): ([u8; 3], [u8; 3]) = match self {
            Hue::Media => ([105, 150, 255], [40, 80, 190]),
            Hue::Effect => ([180, 140, 255], [105, 65, 190]),
            Hue::Live => ([70, 205, 190], [10, 120, 110]),
            Hue::Text => ([225, 210, 170], [120, 100, 50]),
            Hue::Output => ([245, 125, 170], [170, 40, 95]),
            Hue::Control => ([95, 205, 255], [0, 110, 160]),
        };
        let [r, g, b] = if LIGHT.load(std::sync::atomic::Ordering::Relaxed) { light } else { dark };
        Color32::from_rgb(r, g, b)
    }

    /// The kind of a layer or clip source (None: empty).
    pub fn of_source(s: &phosphor_engine::SourceSpec) -> Option<Hue> {
        use phosphor_engine::SourceSpec as S;
        Some(match s {
            S::None => return None,
            S::Image { .. } | S::Movie { .. } => Hue::Media,
            S::Generator(_) | S::Feed { .. } => Hue::Effect,
            // MilkDrop presets are drawn, like generators.
            S::Live { format, .. } if format == "milkdrop" => Hue::Effect,
            S::Live { .. } => Hue::Live,
            S::Text(_) => Hue::Text,
        })
    }
}

/// The interface brightness a fresh install starts at (Phosphor's usual near-black).
pub const DEFAULT_BRIGHTNESS: f32 = 0.35;

/// Named steps for the brightness menu.
pub const BRIGHTNESS_PRESETS: &[(&str, f32)] = &[("Darkest", 0.0), ("Dark", DEFAULT_BRIGHTNESS), ("Dim", 0.5), ("Soft light", 0.75), ("Light", 1.0)];

/// The background and widget tones at one brightness.
#[derive(Clone, Copy)]
struct Palette {
    panel: u8,
    window: u8,
    extreme: u8,
    faint: u8,
    noninteractive: u8,
    stroke: u8,
    inactive: u8,
    inactive_weak: u8,
    hovered: u8,
    hovered_weak: u8,
}

/// Anchors, blended in between. Two families: dark (light text) up to 0.5 = a lifted charcoal, light (dark text)
/// from just above 0.5 = soft grey, up to white at 1. No mid-grey in between, where neither text would read.
const ANCHORS: [(f32, Palette); 5] = [
    (0.0, Palette { panel: 6, window: 10, extreme: 0, faint: 14, noninteractive: 10, stroke: 28, inactive: 24, inactive_weak: 18, hovered: 38, hovered_weak: 32 }),
    (0.35, Palette { panel: 21, window: 27, extreme: 13, faint: 31, noninteractive: 27, stroke: 47, inactive: 45, inactive_weak: 39, hovered: 60, hovered_weak: 53 }),
    (0.5, Palette { panel: 44, window: 50, extreme: 32, faint: 54, noninteractive: 50, stroke: 72, inactive: 68, inactive_weak: 62, hovered: 84, hovered_weak: 77 }),
    (0.5001, Palette { panel: 206, window: 214, extreme: 226, faint: 198, noninteractive: 210, stroke: 170, inactive: 190, inactive_weak: 198, hovered: 176, hovered_weak: 184 }),
    (1.0, Palette { panel: 238, window: 246, extreme: 255, faint: 228, noninteractive: 242, stroke: 204, inactive: 222, inactive_weak: 230, hovered: 206, hovered_weak: 214 }),
];

fn palette(b: f32) -> Palette {
    let b = b.clamp(0.0, 1.0);
    let i = ANCHORS.iter().rposition(|(at, _)| *at <= b).unwrap_or(0).min(ANCHORS.len() - 2);
    let ((a0, p0), (a1, p1)) = (ANCHORS[i], ANCHORS[i + 1]);
    let t = ((b - a0) / (a1 - a0)).clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Palette {
        panel: m(p0.panel, p1.panel),
        window: m(p0.window, p1.window),
        extreme: m(p0.extreme, p1.extreme),
        faint: m(p0.faint, p1.faint),
        noninteractive: m(p0.noninteractive, p1.noninteractive),
        stroke: m(p0.stroke, p1.stroke),
        inactive: m(p0.inactive, p1.inactive),
        inactive_weak: m(p0.inactive_weak, p1.inactive_weak),
        hovered: m(p0.hovered, p1.hovered),
        hovered_weak: m(p0.hovered_weak, p1.hovered_weak),
    }
}

/// A grey with a hint of blue (Phosphor's panels aren't pure grey).
fn tone(g: u8) -> Color32 {
    Color32::from_rgb(g.saturating_sub(3), g.saturating_sub(1), g)
}

const LOGO_FAMILY: &str = "phosphor-logo";

/// The PHOSPHOR VJ wordmark at `size`, in the logo's font and the accent colour (top bar, locked top bar).
pub fn logo(size: f32) -> egui::RichText {
    egui::RichText::new("PHOSPHOR VJ").font(egui::FontId::new(size, egui::FontFamily::Name(LOGO_FAMILY.into()))).color(accent())
}

/// The solid icons (a filled star for a favourite): draw an `icon::` glyph with `filled_icons(size)`.
const FILLED_ICONS: &str = "phosphor-fill";

pub fn filled_icons(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name(FILLED_ICONS.into()))
}

/// The UI fonts, plus a font for Japanese / Chinese characters while that language is on (`i18n`).
pub fn set_fonts(ctx: &egui::Context, cjk: Option<std::sync::Arc<Vec<u8>>>) {
    let mut fonts = egui::FontDefinitions::default();
    // The logo's font (Orbitron Bold, OFL, assets/fonts): only for the PHOSPHOR wordmark (`logo`).
    fonts.font_data.insert(LOGO_FAMILY.into(), std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../../../assets/fonts/logo.ttf"))));
    fonts.families.insert(egui::FontFamily::Name(LOGO_FAMILY.into()), vec![LOGO_FAMILY.into()]);
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    egui_phosphor::add_font_bytes_as_family(&mut fonts, FILLED_ICONS, egui_phosphor::Variant::Fill.font_bytes());
    if let Some(bytes) = cjk {
        // Hiragino sits higher than the UI font on a line: nudged down to share its baseline.
        let tweak = egui::FontTweak { y_offset_factor: 0.07, ..Default::default() };
        fonts.font_data.insert("cjk".into(), std::sync::Arc::new(egui::FontData::from_owned((*bytes).clone()).tweak(tweak)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            fonts.families.entry(family).or_default().push("cjk".into());
        }
    }
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context) {
    set_fonts(ctx, None);
    ctx.all_styles_mut(|s| {
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
        // One control height for every row (`ui::CONTROL_H`).
        s.spacing.interact_size.y = crate::ui::CONTROL_H;
        s.spacing.menu_margin = egui::Margin::same(6);
        // A scroll bar keeps room of its own: nothing sits under it (a slider's or loop mark's right end couldn't be
        // grabbed while a panel's bar was showing).
        let sc = &mut s.spacing.scroll;
        sc.floating_allocated_width = sc.bar_inner_margin + sc.bar_width + sc.bar_outer_margin;
        // Tooltips only when the pointer rests (a second), not as it passes over a list: they sit right under the
        // pointer and covered the next items ("Add a file" and the rows under it). Moving to a neighbour shows its
        // tooltip at once only inside the grace time, so a list doesn't flicker either.
        s.interaction.tooltip_delay = 1.0;
        s.interaction.tooltip_grace_time = 0.1;
    });
    set_brightness(ctx, DEFAULT_BRIGHTNESS);
}

/// Tooltips on or off (off: for people who know the app; (i) help still opens on a click).
pub fn set_tooltips(ctx: &egui::Context, on: bool) {
    TOOLTIPS.store(on, std::sync::atomic::Ordering::Relaxed);
    ctx.all_styles_mut(|s| s.interaction.tooltip_delay = if on { 1.0 } else { f32::INFINITY });
}

static TOOLTIPS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn tooltips_on() -> bool {
    TOOLTIPS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Interface brightness, 0 = darkest, 1 = light (0.35 is Phosphor's usual look). Above the middle the text turns
/// dark and the accent deepens so everything stays readable.
pub fn set_brightness(ctx: &egui::Context, b: f32) {
    let light = b > 0.5;
    LIGHT.store(light, std::sync::atomic::Ordering::Relaxed);
    let p = palette(b);
    let accent = accent();
    let mut v = if light { egui::Visuals::light() } else { egui::Visuals::dark() };
    v.panel_fill = tone(p.panel);
    v.window_fill = tone(p.window);
    v.extreme_bg_color = tone(p.extreme);
    v.faint_bg_color = tone(p.faint);
    v.widgets.noninteractive.bg_fill = tone(p.noninteractive);
    v.widgets.noninteractive.weak_bg_fill = tone(p.noninteractive);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, tone(p.stroke));
    v.widgets.inactive.bg_fill = tone(p.inactive);
    v.widgets.inactive.weak_bg_fill = tone(p.inactive_weak);
    v.widgets.hovered.bg_fill = tone(p.hovered);
    v.widgets.hovered.weak_bg_fill = tone(p.hovered_weak);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, accent.gamma_multiply(0.6));
    v.widgets.active.bg_fill = accent.gamma_multiply(if light { 0.55 } else { 0.35 });
    v.widgets.open.bg_fill = tone(p.hovered_weak);
    v.selection.bg_fill = accent.gamma_multiply(if light { 0.4 } else { 0.32 });
    v.selection.stroke = Stroke::new(1.0, accent);
    v.hyperlink_color = accent;
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
        // Nothing grows when hovered or pressed (egui's default is a pixel each side): buttons stay put; hover and
        // press show in their colour and outline.
        w.expansion = 0.0;
        // Every state has the same outline width (a resting button's is just invisible): egui sizes a button by its
        // outline, so a 0-wide resting outline and a 1-wide hovered one made every button 2 points smaller on hover
        // and moved its row.
        if w.bg_stroke.width < 1.0 {
            w.bg_stroke = Stroke::new(1.0, Color32::TRANSPARENT);
        }
    }
    let theme = if light { egui::Theme::Light } else { egui::Theme::Dark };
    ctx.set_visuals_of(theme, v);
    ctx.set_theme(theme);
}

#[cfg(test)]
mod brightness_tests {
    #[test]
    fn brightness_blends_between_anchors() {
        assert_eq!(super::palette(0.35).panel, 21, "the default is Phosphor's usual panel");
        assert!(super::palette(0.0).panel < super::palette(0.2).panel);
        assert!(super::palette(0.9).panel > super::palette(0.6).panel);
        assert!(super::palette(0.5).panel < 60 && super::palette(0.51).panel > 190, "no mid-grey");
        assert_eq!(super::palette(1.0).extreme, 255);
        assert_eq!(super::palette(-3.0).panel, super::palette(0.0).panel);
    }
}

/// Every non-ASCII character in the app's string literals must be in the UI fonts (missing ones draw as boxes).
#[cfg(test)]
#[test]
fn ui_text_has_glyphs() {
    let ctx = egui::Context::default();
    apply(&ctx);
    // Texture deltas must be taken or cleared (epaint checks on drop): nothing draws here.
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
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
    // The language packs that use the UI font (Japanese and Chinese bring their own).
    for code in ["es", "de", "fr", "it", "pt-BR"] {
        let path = format!("{}/lang/{code}.json", env!("CARGO_MANIFEST_DIR"));
        let pack: std::collections::HashMap<String, String> = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for c in pack.values().flat_map(|t| t.chars()).filter(|c| !c.is_ascii() && !('\u{E000}'..='\u{F8FF}').contains(c)).collect::<std::collections::BTreeSet<char>>() {
            if !ctx.fonts_mut(|f| f.has_glyph(&egui::FontId::proportional(14.0), c)) {
                missing.push(format!("{c} (U+{:04X}) {path}", c as u32));
            }
        }
    }
    // Glyphs rasterised by `has_glyph` above leave a pending delta: take it so dropping the context is clean.
    ctx.run_ui(Default::default(), |_| {}).textures_delta.clear();
    assert!(missing.is_empty(), "glyphs the UI font can't draw:\n{}", missing.join("\n"));
}
