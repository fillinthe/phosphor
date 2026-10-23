//! Small shared pieces every panel uses the same way, so a field looks like a field and a rename works the same
//! everywhere. (Sliders: `Level`; help text: `hint`; pick-one enums: `choice_buttons` / `choice_combo`.)
//!
//! Buttons by what they do, each with its own look so you can tell them apart at a glance:
//! - `action_button`: makes something happen now (play, trigger, record, Surprise): solid fill, bold.
//! - `toggle`: an on/off state (ON, Loop, GRID, Syphon, mute): a pill with a light, lit when on.
//! - `segmented`: pick one of a few (Forward / Reverse / Ping-pong, Now / Beat / Bar, filters): joined segments,
//!   the picked one raised.
//! - plain `ui.button` = a command (Import…, Rename, Add folder…); `danger_button` = Delete and the like.
//! - `icon_toggle`: an on/off state in a tight row (solo S, "3 fx" bypass, invert arrow): a small square, filled when
//!   on. `eye_toggle`: show / hide (an eye, crossed out in orange when hidden).

use super::*;

/// A compact on/off button for tight rows (an icon or a letter): filled when on. Marks the response changed.
pub(crate) fn icon_toggle(ui: &mut egui::Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    let mut r = ui.add(egui::Button::new(text).selected(*on).min_size(egui::vec2(20.0, 18.0)));
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    r
}

/// Show / hide: an eye, crossed out in orange when hidden. Marks the response changed.
pub(crate) fn eye_toggle(ui: &mut egui::Ui, shown: &mut bool) -> egui::Response {
    let (glyph, color, tip) = if *shown { (icon::EYE, ui.visuals().text_color(), "Showing: click to hide") } else { (icon::EYE_SLASH, crate::theme::WARN, "Hidden: click to show") };
    let mut r = ui.add(egui::Button::new(RichText::new(glyph).color(color)).frame(false).min_size(egui::vec2(22.0, 18.0))).on_hover_text(tip);
    if r.clicked() {
        *shown = !*shown;
        r.mark_changed();
    }
    r
}

/// A bar's row of buttons (the top bar): as tall as its tallest button from the start, every item centred on the
/// same line. (`ui.horizontal` starts one button high and grows when a taller one comes, leaving the earlier items
/// high.)
pub(crate) fn bar_row<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // The action buttons' height plus a little air above and below.
    const HEIGHT: f32 = 26.0;
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(HEIGHT);
        add(ui)
    })
    .inner
}

/// A one-line text field: `hint` shows while it's empty; `width` None = the room there is.
pub(crate) fn text_field(ui: &mut egui::Ui, text: &mut String, hint: &str, width: Option<f32>) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width.unwrap_or_else(|| ui.available_width())))
}

/// "Shows on": which outputs show this layer / group on their own (a slice each). `outs` = (output id, name,
/// showing now). Returns (output, on) when one is switched.
pub(crate) fn outputs_menu(ui: &mut egui::Ui, outs: &[(u64, String, bool)]) -> Option<(u64, bool)> {
    let n = outs.iter().filter(|o| o.2).count();
    let text = if n == 0 { icon::MONITOR.to_string() } else { format!("{} {n}", icon::MONITOR) };
    let mut out = None;
    ui.menu_button(text, |ui| {
        ui.label(RichText::new("Shows on").strong());
        hint(ui, "An output of its own, besides the mix (a slice there: shape it in the Outputs area)");
        for (id, name, on) in outs {
            let mut v = *on;
            if ui.checkbox(&mut v, name).changed() {
                out = Some((*id, v));
            }
        }
        if outs.len() <= 1 {
            hint(ui, "More outputs: Outputs area › Add output");
        }
    })
    .response
    .on_hover_text(if n == 0 { "Send this to an output of its own".to_string() } else { format!("On {n} output(s) of its own") });
    out
}

/// The body of a "Rename" menu: a field with the current name, OK (or Enter). Returns the new name when confirmed
/// (trimmed, never empty). `id` keeps what's typed while the menu is open.
pub(crate) fn rename_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &str) -> Option<String> {
    name_menu(ui, id, current, "OK", "")
}

/// The body of a "New …" menu: an empty field with `hint`, a `confirm` button (or Enter). Returns the name typed.
pub(crate) fn new_name_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, hint: &str, confirm: &str) -> Option<String> {
    name_menu(ui, id, "", confirm, hint)
}

fn name_menu(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, initial: &str, confirm: &str, hint: &str) -> Option<String> {
    let key = egui::Id::new(("name_menu", id));
    let mut text: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_else(|| initial.to_string());
    let mut out = None;
    ui.horizontal(|ui| {
        let field = text_field(ui, &mut text, hint, Some(170.0));
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let ok = ui.add_enabled(!text.trim().is_empty(), egui::Button::new(confirm)).clicked();
        if (enter || ok) && !text.trim().is_empty() {
            out = Some(text.trim().to_string());
            text = initial.to_string();
            ui.close();
        }
    });
    ui.ctx().data_mut(|d| d.insert_temp(key, text));
    out
}


/// What an action button does: its colour.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Act {
    /// Play / trigger / go (the accent colour).
    Go,
    /// Record (red).
    Record,
}

/// A button that makes something happen now: solid, bold. (Commands that open or edit things stay plain buttons.)
pub(crate) fn action_button(ui: &mut egui::Ui, act: Act, text: impl Into<String>) -> egui::Response {
    action_button_sized(ui, act, text, egui::vec2(0.0, 22.0))
}

/// `action_button` at a set size (clip column triggers).
pub(crate) fn action_button_sized(ui: &mut egui::Ui, act: Act, text: impl Into<String>, size: egui::Vec2) -> egui::Response {
    let fill = match act {
        Act::Go => accent(),
        Act::Record => Color32::from_rgb(225, 70, 60),
    };
    let ink = if act == Act::Go { Color32::from_gray(12) } else { Color32::WHITE };
    let b = egui::Button::new(RichText::new(text.into()).strong().color(ink)).fill(fill).stroke(Stroke::NONE).corner_radius(6.0).min_size(size);
    let r = if size.x > 0.0 { ui.add_sized(size, b) } else { ui.add(b) };
    if r.hovered() {
        ui.painter().rect_stroke(r.rect, 6.0, Stroke::new(1.5, Color32::WHITE.gamma_multiply(0.7)), StrokeKind::Inside);
    }
    r
}

/// An on/off state: a pill with a light. Lit (accent) when on, hollow when off. Returns the response (`changed`).
pub(crate) fn toggle(ui: &mut egui::Ui, on: &mut bool, text: impl Into<String>) -> egui::Response {
    toggle_locked(ui, on, text, false)
}

/// `toggle` that ignores clicks while `locked`.
pub(crate) fn toggle_locked(ui: &mut egui::Ui, on: &mut bool, text: impl Into<String>, locked: bool) -> egui::Response {
    let text: String = text.into();
    let font = egui::FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text, font, Color32::WHITE);
    let size = egui::vec2(galley.size().x + 30.0, 22.0);
    let (rect, mut r) = ui.allocate_exact_size(size, Sense::click());
    if r.clicked() && !locked {
        *on = !*on;
        r.mark_changed();
    }
    let v = ui.visuals();
    let hovered = r.hovered();
    let (fill, stroke, ink) = if *on {
        (accent().gamma_multiply(0.22), Stroke::new(1.0, accent()), accent())
    } else {
        (Color32::TRANSPARENT, Stroke::new(1.0, if hovered { v.widgets.hovered.fg_stroke.color } else { v.widgets.inactive.bg_stroke.color.gamma_multiply(1.6) }), v.widgets.inactive.fg_stroke.color)
    };
    let p = ui.painter();
    p.rect(rect, size.y * 0.5, fill, stroke, StrokeKind::Inside);
    let dot = rect.left_center() + egui::vec2(11.0, 0.0);
    if *on {
        p.circle_filled(dot, 4.0, accent());
        p.circle_filled(dot, 7.0, accent().gamma_multiply(0.18));
    } else {
        p.circle_stroke(dot, 3.5, Stroke::new(1.0, ink.gamma_multiply(0.7)));
    }
    p.galley(egui::pos2(rect.left() + 21.0, rect.center().y - galley.size().y * 0.5), galley, ink);
    r.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, ui.is_enabled(), *on, ""));
    r
}

/// Pick one of a few: joined segments in one rounded frame, the picked one raised. `each` gets each segment's
/// response (to add a map menu, a tooltip…). Returns true when the choice changed.
pub(crate) fn segmented<T: PartialEq + Clone>(ui: &mut egui::Ui, value: &mut T, options: &[(T, String)], mut each: impl FnMut(&mut egui::Ui, &egui::Response, &T)) -> bool {
    let before = value.clone();
    let v = ui.visuals().clone();
    egui::Frame::new().fill(v.extreme_bg_color).corner_radius(6.0).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (opt, text) in options {
                let picked = *value == *opt;
                let ink = if picked { v.strong_text_color() } else { v.weak_text_color() };
                let fill = if picked { v.widgets.active.weak_bg_fill } else { Color32::TRANSPARENT };
                let b = egui::Button::new(RichText::new(text).color(ink)).fill(fill).stroke(if picked { Stroke::new(1.0, v.widgets.hovered.bg_stroke.color) } else { Stroke::NONE }).corner_radius(4.0).min_size(egui::vec2(0.0, 18.0));
                let r = ui.add(b);
                if r.clicked() {
                    *value = opt.clone();
                }
                each(ui, &r, opt);
            }
        });
    });
    *value != before
}

/// A destructive command (Delete, Remove…): red text.
pub(crate) fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    ui.button(RichText::new(text.into()).color(crate::theme::WARN))
}
