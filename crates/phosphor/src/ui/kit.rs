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

/// The one height of a control in a row (buttons, toggles, pick-one groups, fields, sliders: `theme` sets egui's
/// `interact_size.y` to it). Everything in a row being this tall means a row never grows part-way through, so nothing
/// sits high or jumps. Taller things (the big GO) get a row of their own.
pub(crate) const CONTROL_H: f32 = 22.0;

/// How wide a button must be to fit the widest of `labels` (its padding included), so a button whose label changes
/// (Play / Pause, Open / Close) keeps one width and nothing beside it moves.
pub(crate) fn steady_width(ui: &egui::Ui, labels: &[&str], small: bool) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let widest = labels.iter().map(|l| ui.painter().layout_no_wrap(l.to_string(), font.clone(), Color32::WHITE).size().x).fold(0.0, f32::max);
    // (+4: bold text in action buttons is a little wider.)
    widest + if small { 8.0 } else { ui.spacing().button_padding.x * 2.0 } + 4.0
}

/// A command button whose label changes: as wide as the widest of `labels` (`text` is one of them).
pub(crate) fn steady_button(ui: &mut egui::Ui, text: impl Into<String>, labels: &[&str]) -> egui::Response {
    let w = steady_width(ui, labels, false);
    ui.add(egui::Button::new(text.into()).min_size(egui::vec2(w, CONTROL_H)))
}

/// `steady_button`, small (tight rows).
pub(crate) fn steady_small_button(ui: &mut egui::Ui, text: impl Into<String>, labels: &[&str]) -> egui::Response {
    let w = steady_width(ui, labels, true);
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 4.0;
        ui.add(egui::Button::new(text.into()).wrap_mode(egui::TextWrapMode::Extend).min_size(egui::vec2(w, CONTROL_H)))
    })
    .inner
}

/// An `action_button` whose label changes (Play / Pause): as wide as the widest of `labels`.
pub(crate) fn steady_action_button(ui: &mut egui::Ui, act: Act, text: impl Into<String>, labels: &[&str]) -> egui::Response {
    let w = steady_width(ui, labels, false);
    action_button_sized(ui, act, text, egui::vec2(w, CONTROL_H))
}

/// A compact on/off button for tight rows (an icon or a letter): filled when on. Marks the response changed.
pub(crate) fn icon_toggle(ui: &mut egui::Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    let mut r = ui.add(egui::Button::new(text).selected(*on).min_size(egui::vec2(20.0, CONTROL_H)));
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    r
}

/// Show / hide: an eye, crossed out in orange when hidden. Marks the response changed.
pub(crate) fn eye_toggle(ui: &mut egui::Ui, shown: &mut bool) -> egui::Response {
    let (glyph, color, tip) = if *shown { (icon::EYE, ui.visuals().text_color(), "Showing: click to hide") } else { (icon::EYE_SLASH, crate::theme::WARN, "Hidden: click to show") };
    let mut r = ui.add(egui::Button::new(RichText::new(glyph).color(color)).frame(false).min_size(egui::vec2(22.0, CONTROL_H))).on_hover_text(tip);
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
    // A control's height plus a little air above and below.
    const HEIGHT: f32 = CONTROL_H + 4.0;
    ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), HEIGHT), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_height(HEIGHT);
        add(ui)
    })
    .inner
}

/// A row of controls (buttons, toggles, pick-one groups) that wraps onto another line when the panel is narrow,
/// instead of running off its edge (and making the whole panel wider than the column, which cuts off every row).
pub(crate) fn toolbar<R>(ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal_wrapped(|ui| {
        ui.set_min_height(CONTROL_H);
        add(ui)
    })
    .inner
}

/// One entry in a list or a drop-down (a layer, a device, a size): highlighted when it's the one picked. For a few
/// options side by side use `segmented` / `choice_buttons`.
pub(crate) fn pick_item<'a>(ui: &mut egui::Ui, picked: bool, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    ui.add(egui::Button::selectable(picked, text))
}

/// `pick_item` that sets `current` to `value` when clicked (marks the response changed).
pub(crate) fn pick_value<'a, V: PartialEq>(ui: &mut egui::Ui, current: &mut V, value: V, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    let mut r = pick_item(ui, *current == value, text);
    if r.clicked() && *current != value {
        *current = value;
        r.mark_changed();
    }
    r
}

/// A command (Import…, Rename, Add folder…): a plain button that never wraps its words.
pub(crate) fn command<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    ui.add(egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend))
}

/// A command that can't be used right now (nothing to save, nothing selected): greyed out until it can.
pub(crate) fn command_if<'a>(ui: &mut egui::Ui, enabled: bool, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend))
}

/// A command for a tight row (an icon, a short word): narrower than `ui.button` but the same height, so it lines up
/// with everything beside it.
pub(crate) fn small_command<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    small_command_if(ui, true, text)
}

/// `small_command`, greyed out until it can be used.
pub(crate) fn small_command_if<'a>(ui: &mut egui::Ui, enabled: bool, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    ui.scope(|ui| {
        ui.spacing_mut().button_padding.x = 4.0;
        // (Never wraps its words onto a second line: a tight row moves it to the next line instead.)
        ui.add_enabled(enabled, egui::Button::new(text).wrap_mode(egui::TextWrapMode::Extend).min_size(egui::vec2(0.0, CONTROL_H)))
    })
    .inner
}

/// A command at a set size (a column of same-width buttons, a grid cell's header).
pub(crate) fn command_sized<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>, size: egui::Vec2) -> egui::Response {
    ui.add_sized(size, egui::Button::new(text))
}

/// A command at least `width` wide (a list of choices that should line up).
pub(crate) fn wide_button<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>, width: f32) -> egui::Response {
    ui.add(egui::Button::new(text).min_size(egui::vec2(width, CONTROL_H)))
}

/// Something asking for attention (Donate, an update is out, take back control): solid colour, dark bold text.
pub(crate) fn callout_button(ui: &mut egui::Ui, text: impl Into<String>, fill: Color32) -> egui::Response {
    ui.add(callout(text, fill))
}

/// The `callout_button` widget, to place somewhere exact (`ui.put`).
pub(crate) fn callout(text: impl Into<String>, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).strong().color(Color32::from_gray(16))).fill(fill)
}

/// A mode that changes what clicks do (MAP): plain when off, filled with the accent while on.
pub(crate) fn mode_button(ui: &mut egui::Ui, on: bool, text: impl Into<String>) -> egui::Response {
    let t = RichText::new(text.into()).strong();
    let t = if on { t.color(Color32::BLACK) } else { t };
    ui.add(egui::Button::new(t).selected(on).fill(if on { accent() } else { Color32::TRANSPARENT }))
}

/// A menu entry with its keyboard shortcut on the right (greyed out when it can't be used).
pub(crate) fn menu_item(ui: &mut egui::Ui, enabled: bool, text: impl Into<String>, shortcut: impl Into<String>) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(text.into()).shortcut_text(shortcut.into()))
}

/// The one big button of a panel (the cue list's GO): an action button, bigger.
pub(crate) fn big_action_button(ui: &mut egui::Ui, act: Act, text: impl Into<String>) -> egui::Response {
    let fill = match act {
        Act::Go => accent(),
        Act::Record => Color32::from_rgb(225, 70, 60),
    };
    let ink = if act == Act::Go { Color32::from_gray(12) } else { Color32::WHITE };
    ui.add(egui::Button::new(RichText::new(text.into()).strong().size(16.0).color(ink)).fill(fill).stroke(Stroke::NONE).corner_radius(6.0).min_size(egui::vec2(80.0, 30.0)))
}

/// Width of the label column in `field` rows (labels line up down a panel).
pub(crate) const LABEL_W: f32 = 96.0;

/// A group of settings that folds away: a bold title, its contents below. `id` keeps it open or shut on its own.
pub(crate) fn section<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, open: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::CollapsingResponse<R> {
    egui::CollapsingHeader::new(RichText::new(title.into()).strong()).id_salt(id).default_open(open).show(ui, add)
}

/// A smaller fold-away group inside a section (a transition's settings, corners by number).
pub(crate) fn subsection<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, open: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::CollapsingResponse<R> {
    egui::CollapsingHeader::new(RichText::new(title.into()).small()).id_salt(id).default_open(open).show(ui, add)
}

/// The title of a group of controls that doesn't fold (Screens, Outputs, MIDI).
pub(crate) fn heading(ui: &mut egui::Ui, text: impl ToString) -> egui::Response {
    ui.label(RichText::new(text.to_string()).strong())
}

/// A "label: control" row: the label in a column `LABEL_W` wide (cut short if longer, whole on hover), then the
/// control(s) in the rest of the row, so settings line up down a panel.
pub(crate) fn field<R>(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.horizontal(|ui| {
        let (rect, r) = ui.allocate_exact_size(egui::vec2(LABEL_W, CONTROL_H), Sense::hover());
        let font = egui::TextStyle::Body.resolve(ui.style());
        let mut job = egui::text::LayoutJob::single_section(label.to_string(), egui::TextFormat::simple(font, ui.visuals().text_color()));
        job.wrap = egui::text::TextWrapping::truncate_at_width(LABEL_W - 6.0);
        let galley = ui.painter().layout_job(job);
        let elided = galley.elided;
        ui.painter().galley(egui::pos2(rect.left(), rect.center().y - galley.size().y / 2.0), galley, ui.visuals().text_color());
        if elided {
            r.on_hover_text(label);
        }
        // The controls wrap beside the label column, never under it (their own column, exactly the width left).
        ui.vertical(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.set_min_height(CONTROL_H);
                add(ui)
            })
            .inner
        })
        .inner
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
    action_button_sized(ui, act, text, egui::vec2(0.0, CONTROL_H))
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
    // A long label is cut short (whole on hover) rather than making the panel wider than its column.
    let mut job = egui::text::LayoutJob::single_section(text.clone(), egui::TextFormat::simple(font, Color32::PLACEHOLDER));
    job.wrap = egui::text::TextWrapping::truncate_at_width((ui.available_width() - 30.0).max(40.0));
    let galley = ui.painter().layout_job(job);
    let elided = galley.elided;
    let size = egui::vec2(galley.size().x + 30.0, CONTROL_H);
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
    if elided { r.on_hover_text(text) } else { r }
}

/// Pick one of a few: joined segments in one rounded frame, the picked one raised. `each` gets each segment's
/// response (to add a map menu, a tooltip…). Returns true when the choice changed.
pub(crate) fn segmented<T: PartialEq + Clone>(ui: &mut egui::Ui, value: &mut T, options: &[(T, String)], mut each: impl FnMut(&mut egui::Ui, &egui::Response, &T)) -> bool {
    let before = value.clone();
    let v = ui.visuals().clone();
    // A framed group only knows its size once it's drawn, so a wrapping row can't move it along by itself: measure it
    // first and start a new line when it won't fit in what's left of this one.
    if ui.layout().main_wrap {
        let font = egui::TextStyle::Button.resolve(ui.style());
        let pad = ui.spacing().button_padding.x * 2.0 + 2.0;
        let need: f32 = options.iter().map(|(_, t)| ui.painter().layout_no_wrap(t.clone(), font.clone(), Color32::WHITE).size().x + pad).sum::<f32>() + 4.0;
        let row_start = ui.max_rect().left();
        if need > ui.available_width() && ui.cursor().left() > row_start + 1.0 {
            ui.end_row();
        }
    }
    egui::Frame::new().fill(v.extreme_bg_color).corner_radius(6.0).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        // The options wrap onto another line in a narrow panel instead of running off its edge.
        ui.horizontal_wrapped(|ui| {
            for (opt, text) in options {
                let picked = *value == *opt;
                let ink = if picked { v.strong_text_color() } else { v.weak_text_color() };
                let fill = if picked { v.widgets.active.weak_bg_fill } else { Color32::TRANSPARENT };
                let b = egui::Button::new(RichText::new(text).color(ink)).fill(fill).stroke(if picked { Stroke::new(1.0, v.widgets.hovered.bg_stroke.color) } else { Stroke::NONE }).corner_radius(4.0).min_size(egui::vec2(0.0, CONTROL_H - 4.0));
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
    command(ui, RichText::new(text.into()).color(crate::theme::WARN))
}
