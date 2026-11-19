//! Buttons by job: commands, actions, toggles, pick-one groups, danger, and buttons that keep one width.

use crate::ui::*;

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
    let mut r = ui.add(egui::Button::new(text.into()).min_size(egui::vec2(w, CONTROL_H)));
    // (Named by its first label, so it's the same button whichever it shows.)
    crate::ui::press::mappable(ui, &mut r, labels.first().copied().unwrap_or_default());
    r
}

/// `steady_button`, small (tight rows).
pub(crate) fn steady_small_button(ui: &mut egui::Ui, text: impl Into<String>, labels: &[&str]) -> egui::Response {
    let w = steady_width(ui, labels, true);
    let old = ui.spacing().button_padding;
    ui.spacing_mut().button_padding.x = 4.0;
    let mut r = ui.add(egui::Button::new(text.into()).wrap_mode(egui::TextWrapMode::Extend).min_size(egui::vec2(w, CONTROL_H)));
    ui.spacing_mut().button_padding = old;
    crate::ui::press::mappable(ui, &mut r, labels.first().copied().unwrap_or_default());
    r
}

/// An `action_button` whose label changes (Play / Pause): as wide as the widest of `labels`.
pub(crate) fn steady_action_button(ui: &mut egui::Ui, act: Act, text: impl Into<String>, labels: &[&str]) -> egui::Response {
    let w = steady_width(ui, labels, false);
    // (Mapped by its first label, so a pad keeps working whichever label it shows.)
    action_button_named(ui, act, text, egui::vec2(w, CONTROL_H), labels.first().copied())
}

/// A compact on/off button for tight rows (an icon or a letter): filled when on. Marks the response changed.
pub(crate) fn icon_toggle(ui: &mut egui::Ui, on: &mut bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    let text: egui::WidgetText = text.into();
    let label = text.text().to_string();
    let mut r = ui.add(egui::Button::new(text).selected(*on).min_size(egui::vec2(20.0, CONTROL_H)));
    crate::ui::press::mappable(ui, &mut r, &label);
    if r.clicked() {
        *on = !*on;
        r.mark_changed();
    }
    r
}

/// A menu entry's words: lit in the accent color with what it's set to when that thing is on ("Modulate: LFO",
/// "Mapped: MIDI CC 12"), plain otherwise, so what's active on a control shows in its right-click menu without
/// opening anything (owner, 2026-11-14).
pub(crate) fn menu_label(glyph: &str, text: &str, active: Option<String>) -> egui::RichText {
    match active {
        Some(what) => RichText::new(format!("{glyph} {text}: {what}")).color(accent()).strong(),
        None => RichText::new(format!("{glyph} {text}")),
    }
}

/// A bare icon button the size of `eye_toggle` (the column in front of a card's bars: play / pause by the playhead).
pub(crate) fn row_icon(ui: &mut egui::Ui, glyph: &str, height: f32) -> egui::Response {
    ui.add(egui::Button::new(glyph).frame(false).min_size(egui::vec2(22.0, height)))
}

pub(crate) fn eye_toggle(ui: &mut egui::Ui, shown: &mut bool) -> egui::Response {
    let (glyph, color, tip) = if *shown { (icon::EYE, ui.visuals().text_color(), "Showing: click to hide") } else { (icon::EYE_SLASH, crate::theme::WARN, "Hidden: click to show") };
    let mut r = ui.add(egui::Button::new(RichText::new(glyph).color(color)).frame(false).min_size(egui::vec2(22.0, CONTROL_H))).on_hover_text(tip);
    crate::ui::press::mappable(ui, &mut r, "Show / hide");
    if r.clicked() {
        *shown = !*shown;
        r.mark_changed();
    }
    r
}

/// One entry in a list or a drop-down (a layer, a device, a size): highlighted when it's the one picked. For a few
/// options side by side use `segmented` / `choice_buttons`.
pub(crate) fn pick_item(ui: &mut egui::Ui, picked: bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    // egui frames an entry only while it's hovered or picked, which made it 2 points wider then (and moved what's
    // next to it): it's always as wide as when framed.
    let text: egui::WidgetText = text.into();
    let w = text.clone().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button).size().x;
    ui.add(egui::Button::selectable(picked, text).min_size(egui::vec2(w + ui.spacing().button_padding.x * 2.0 + 2.0, 0.0)))
}

/// A section of the app (Show, Outputs, Control, Designer) in its color: the one you're in is filled with the
/// color and its words are black; hovering another tints it. One size whatever its state.
pub(crate) fn section_tab(ui: &mut egui::Ui, active: bool, text: &str, colour: Color32) -> egui::Response {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, colour);
    let pad = ui.spacing().button_padding.x;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(galley.size().x + pad * 2.0 + 2.0, CONTROL_H), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let radius = ui.visuals().widgets.inactive.corner_radius;
        let (fill, words) = if active {
            (colour, Color32::BLACK)
        } else if resp.hovered() {
            (colour.gamma_multiply(0.22), colour)
        } else {
            (Color32::TRANSPARENT, colour)
        };
        p.rect_filled(rect, radius, fill);
        let galley = ui.painter().layout_no_wrap(text.to_string(), egui::TextStyle::Button.resolve(ui.style()), words);
        p.galley(egui::pos2(rect.left() + pad + 1.0, rect.center().y - galley.size().y / 2.0), galley, words);
    }
    resp
}

/// `pick_item` that sets `current` to `value` when clicked (marks the response changed).
pub(crate) fn pick_value<V: PartialEq>(ui: &mut egui::Ui, current: &mut V, value: V, text: impl Into<egui::WidgetText>) -> egui::Response {
    let mut r = pick_item(ui, *current == value, text);
    if r.clicked() && *current != value {
        *current = value;
        r.mark_changed();
    }
    r
}

/// A command (Import…, Rename, Add folder…): a plain button that never wraps its words.
/// One row of a source list (the Media panel's sidebar): the whole width, a picked or hovered fill, the name on the
/// left and a count or badge on the right. Never changes size.
pub(crate) fn list_row(ui: &mut egui::Ui, picked: bool, text: &str, right: &str, badge: bool) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), CONTROL_H), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        let v = ui.visuals();
        let fill = if picked { v.selection.bg_fill } else if resp.hovered() { v.widgets.hovered.weak_bg_fill } else { Color32::TRANSPARENT };
        ui.painter().rect_filled(rect, 3.0, fill);
        let color = if picked { v.selection.stroke.color } else { v.text_color() };
        let font = egui::TextStyle::Button.resolve(ui.style());
        let mut right_w = 0.0;
        if !right.is_empty() {
            let g = ui.painter().layout_no_wrap(right.to_string(), egui::FontId::proportional(11.0), if badge { Color32::BLACK } else { v.weak_text_color() });
            right_w = g.size().x + 10.0;
            let at = egui::pos2(rect.right() - 6.0 - g.size().x, rect.center().y - g.size().y / 2.0);
            if badge {
                ui.painter().rect_filled(egui::Rect::from_min_size(at - egui::vec2(4.0, 1.0), g.size() + egui::vec2(8.0, 2.0)), 7.0, crate::theme::accent());
            }
            ui.painter().galley(at, g, Color32::BLACK);
        }
        let room = (rect.width() - 14.0 - right_w).max(10.0);
        let job = egui::text::LayoutJob { wrap: egui::text::TextWrapping::truncate_at_width(room), ..egui::text::LayoutJob::simple_singleline(text.to_string(), font, color) };
        let g = ui.painter().layout_job(job);
        ui.painter().galley(egui::pos2(rect.left() + 6.0, rect.center().y - g.size().y / 2.0), g, color);
    }
    resp
}

pub(crate) fn command<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    command_if(ui, true, text)
}

/// The words on a button (its name for mapping).
fn words(atoms: &egui::Atoms<'_>) -> String {
    atoms.text().map(|t| t.into_owned()).unwrap_or_default()
}

/// A command that can't be used right now (nothing to save, nothing selected): grayed out until it can.
pub(crate) fn command_if<'a>(ui: &mut egui::Ui, enabled: bool, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    let atoms = text.into_atoms();
    let label = words(&atoms);
    let mut r = ui.add_enabled(enabled, egui::Button::new(atoms).wrap_mode(egui::TextWrapMode::Extend));
    crate::ui::press::mappable(ui, &mut r, &label);
    r
}

/// A command for a tight row (an icon, a short word): narrower than `ui.button` but the same height, so it lines up
/// with everything beside it.
pub(crate) fn small_command<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    small_command_if(ui, true, text)
}

/// `small_command`, grayed out until it can be used.
pub(crate) fn small_command_if<'a>(ui: &mut egui::Ui, enabled: bool, text: impl egui::IntoAtoms<'a>) -> egui::Response {
    // Tighter padding just for this button (set and put back on the row itself: a nested container would hide the
    // button's size from a wrapping row, which then couldn't move it to the next line). Never wraps its words.
    let old = ui.spacing().button_padding;
    ui.spacing_mut().button_padding.x = 4.0;
    let atoms = text.into_atoms();
    let label = words(&atoms);
    let mut r = ui.add_enabled(enabled, egui::Button::new(atoms).wrap_mode(egui::TextWrapMode::Extend).min_size(egui::vec2(0.0, CONTROL_H)));
    ui.spacing_mut().button_padding = old;
    crate::ui::press::mappable(ui, &mut r, &label);
    r
}

/// A command at a set size (a column of same-width buttons, a grid cell's header).
pub(crate) fn command_sized<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>, size: egui::Vec2) -> egui::Response {
    let atoms = text.into_atoms();
    let label = words(&atoms);
    let mut r = ui.add_sized(size, egui::Button::new(atoms));
    crate::ui::press::mappable(ui, &mut r, &label);
    r
}

/// A command at least `width` wide (a list of choices that should line up).
pub(crate) fn wide_button<'a>(ui: &mut egui::Ui, text: impl egui::IntoAtoms<'a>, width: f32) -> egui::Response {
    let atoms = text.into_atoms();
    let label = words(&atoms);
    let mut r = ui.add(egui::Button::new(atoms).min_size(egui::vec2(width, CONTROL_H)));
    crate::ui::press::mappable(ui, &mut r, &label);
    r
}

/// A callout that can be put away: one solid button with a small x at its right end (two halves drawn flush, the x
/// on the right whichever way the row is laid out). Returns the callout's response and whether the x was clicked.
/// `tip` and `close_tip` are their hover words.
pub(crate) fn callout_closable(ui: &mut egui::Ui, text: impl Into<String>, fill: Color32, tip: &str, close_tip: &str) -> (egui::Response, bool) {
    let round = ui.visuals().widgets.inactive.corner_radius.nw;
    let left = egui::CornerRadius { nw: round, sw: round, ne: 0, se: 0 };
    let right = egui::CornerRadius { nw: 0, sw: 0, ne: round, se: round };
    let text = text.into();
    let close = |ui: &mut egui::Ui| {
        let pad = ui.spacing().button_padding;
        ui.spacing_mut().button_padding.x = 5.0;
        let r = ui.add(egui::Button::new(RichText::new(icon::X).small().color(Color32::from_gray(16))).fill(fill).corner_radius(right)).on_hover_text(close_tip);
        ui.spacing_mut().button_padding = pad;
        r.clicked()
    };
    let main = |ui: &mut egui::Ui| ui.add(callout(text.clone(), fill).corner_radius(left)).on_hover_text(tip);
    let gap = ui.spacing().item_spacing.x;
    ui.spacing_mut().item_spacing.x = 0.0;
    let out = if ui.layout().main_dir() == egui::Direction::RightToLeft {
        let closed = close(ui);
        (main(ui), closed)
    } else {
        let r = main(ui);
        (r, close(ui))
    };
    ui.spacing_mut().item_spacing.x = gap;
    ui.add_space(gap);
    out
}

/// Something asking for attention (Donate, an update is out, take back control): solid color, dark bold text.
/// Add it with `ui.add`, or place it somewhere exact with `ui.put`.
pub(crate) fn callout(text: impl Into<String>, fill: Color32) -> egui::Button<'static> {
    egui::Button::new(RichText::new(text.into()).strong().color(Color32::from_gray(16))).fill(fill)
}

/// A mode that changes what clicks do (MAP): plain when off, filled with the accent while on.
pub(crate) fn mode_button(ui: &mut egui::Ui, on: bool, text: impl Into<String>) -> egui::Response {
    let t = RichText::new(text.into()).strong();
    let t = if on { t.color(Color32::BLACK) } else { t };
    ui.add(egui::Button::new(t).selected(on).fill(if on { accent() } else { Color32::TRANSPARENT }))
}

/// A menu entry with its keyboard shortcut on the right (grayed out when it can't be used).
pub(crate) fn menu_item(ui: &mut egui::Ui, enabled: bool, text: impl Into<String>, shortcut: impl Into<String>) -> egui::Response {
    ui.add_enabled(enabled, egui::Button::new(text.into()).shortcut_text(shortcut.into()))
}

/// The one big button of a panel (the cue list's GO): an action button, bigger.
pub(crate) fn big_action_button(ui: &mut egui::Ui, act: Act, text: impl Into<String>) -> egui::Response {
    let fill = match act {
        Act::Go => accent(),
        Act::Record => crate::theme::RECORD,
    };
    let ink = if act == Act::Go { Color32::from_gray(12) } else { Color32::WHITE };
    let text: String = text.into();
    let mut r = ui.add(egui::Button::new(RichText::new(&text).strong().size(16.0).color(ink)).fill(fill).stroke(Stroke::NONE).corner_radius(6.0).min_size(egui::vec2(80.0, 30.0)));
    crate::ui::press::mappable(ui, &mut r, &text);
    r
}

/// What an action button does: its color.
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Act {
    /// Play / trigger / go (the accent color).
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
    action_button_named(ui, act, text, size, None)
}

/// `action_button_sized`, mapped under `name` (a label that changes) instead of its text.
fn action_button_named(ui: &mut egui::Ui, act: Act, text: impl Into<String>, size: egui::Vec2, name: Option<&str>) -> egui::Response {
    let fill = match act {
        Act::Go => accent(),
        Act::Record => crate::theme::RECORD,
    };
    let ink = if act == Act::Go { Color32::from_gray(12) } else { Color32::WHITE };
    let text: String = text.into();
    let b = egui::Button::new(RichText::new(&text).strong().color(ink)).fill(fill).stroke(Stroke::NONE).corner_radius(6.0).min_size(size);
    let mut r = if size.x > 0.0 { ui.add_sized(size, b) } else { ui.add(b) };
    crate::ui::press::mappable(ui, &mut r, name.unwrap_or(&text));
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
    crate::ui::press::mappable(ui, &mut r, &text);
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
        make_room(ui, need);
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

pub(crate) fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    command(ui, RichText::new(text.into()).color(crate::theme::DANGER))
}

/// `danger_button` for a tight row (a red trash can or X beside the thing it removes).
pub(crate) fn small_danger(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    small_command(ui, RichText::new(text.into()).color(crate::theme::DANGER))
}

#[cfg(test)]
mod steady_tests {
    use eframe::egui;

    /// Draws a row like the Clips toolbar's Launch controls with the mouse at `pointer`; returns where each piece
    /// starts and the row's end.
    fn layout_with_pointer(ctx: &egui::Context, pointer: egui::Pos2) -> Vec<f32> {
        layout(ctx, pointer, true)
    }

    fn layout(ctx: &egui::Context, pointer: egui::Pos2, fade_on: bool) -> Vec<f32> {
        let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 200.0))), ..Default::default() };
        input.events.push(egui::Event::PointerMoved(pointer));
        let mut xs = Vec::new();
        let mut out = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                crate::ui::toolbar(ui, |ui| {
                    xs.push(ui.cursor().left());
                    ui.label("Launch");
                    xs.push(ui.cursor().left());
                    let mut fade = fade_on;
                    crate::ui::segmented(ui, &mut fade, &[(false, "Cut".to_string()), (true, "Fade".to_string())], |_, _, _| {});
                    xs.push(ui.cursor().left());
                    let mut secs = 0.5f32;
                    ui.add(egui::DragValue::new(&mut secs).fixed_decimals(2).suffix(" s"));
                    xs.push(ui.cursor().left());
                    let mut on = true;
                    crate::ui::toggle(ui, &mut on, "AUTO");
                    xs.push(ui.cursor().left());
                    crate::ui::command(ui, "Resync");
                    xs.push(ui.cursor().left());
                });
            });
        });
        out.textures_delta.clear();
        xs
    }

    /// Moving the mouse over the buttons never moves anything in the row (the owner saw Cut / Fade shift).
    #[test]
    fn hovering_moves_nothing() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let away = layout_with_pointer(&ctx, egui::pos2(850.0, 190.0));
        let away = layout_with_pointer(&ctx, egui::pos2(850.0, 190.0)).into_iter().zip(away).map(|(a, _)| a).collect::<Vec<_>>();
        for x in (0..500).step_by(4) {
            for frame in 0..2 {
                let at = layout_with_pointer(&ctx, egui::pos2(x as f32, 18.0));
                assert_eq!(at, away, "with the mouse at x={x} (frame {frame}) the row moved");
            }
        }
        // Picking the other option (Cut instead of Fade) doesn't move the row either.
        let _ = layout(&ctx, egui::pos2(850.0, 190.0), false);
        assert_eq!(layout(&ctx, egui::pos2(850.0, 190.0), false), away, "picking Cut moved the row");
    }

    /// Every other kind of control: hovering any of them moves nothing.
    #[test]
    fn hovering_any_control_moves_nothing() {
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let draw = |ctx: &egui::Context, pointer: egui::Pos2| {
            let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1400.0, 200.0))), ..Default::default() };
            input.events.push(egui::Event::PointerMoved(pointer));
            let mut xs = Vec::new();
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    crate::ui::toolbar(ui, |ui| {
                        crate::ui::pick_item(ui, false, "List entry");
                        xs.push(ui.cursor().left());
                        crate::ui::pick_item(ui, true, "Picked entry");
                        xs.push(ui.cursor().left());
                        crate::ui::small_command(ui, "Small");
                        xs.push(ui.cursor().left());
                        crate::ui::command_if(ui, true, "Command");
                        xs.push(ui.cursor().left());
                        crate::ui::steady_button(ui, "Play", &["Play", "Pause"]);
                        xs.push(ui.cursor().left());
                        crate::ui::action_button(ui, crate::ui::Act::Go, "Go");
                        xs.push(ui.cursor().left());
                        let mut v = 1usize;
                        crate::ui::dropdown(ui, "dd", 90.0).selected_text("Crossfade").show_ui(ui, |ui| {
                            crate::ui::pick_value(ui, &mut v, 1, "Crossfade");
                        });
                        xs.push(ui.cursor().left());
                        let mut on = false;
                        crate::ui::toggle(ui, &mut on, "Toggle");
                        xs.push(ui.cursor().left());
                        let mut b = false;
                        crate::ui::icon_toggle(ui, &mut b, "S");
                        xs.push(ui.cursor().left());
                    });
                });
            });
            out.textures_delta.clear();
            xs
        };
        let _ = draw(&ctx, egui::pos2(1390.0, 190.0));
        let away = draw(&ctx, egui::pos2(1390.0, 190.0));
        for x in (0..1000).step_by(3) {
            for _ in 0..2 {
                assert_eq!(draw(&ctx, egui::pos2(x as f32, 18.0)), away, "with the mouse at x={x} the row moved");
            }
        }
    }

    /// Clicking the text field inside a Rename menu keeps the menu open (egui closes a menu on any click inside it,
    /// which closed collection renames the moment the field was clicked); clicking outside closes it.
    #[test]
    fn a_menu_with_a_field_stays_open_when_the_field_is_clicked() {
        use std::cell::Cell;
        let ctx = egui::Context::default();
        crate::theme::apply(&ctx);
        let open = Cell::new(false);
        let field_at = Cell::new(egui::Pos2::ZERO);
        let button_at = Cell::new(egui::Pos2::ZERO);
        let mut time = 0.0;
        let mut frame = |events: Vec<egui::Event>, pointer: egui::Pos2| {
            time += 0.05;
            let mut input = egui::RawInput { screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 400.0))), time: Some(time), ..Default::default() };
            input.events.push(egui::Event::PointerMoved(pointer));
            input.events.extend(events);
            open.set(false);
            let mut out = ctx.run_ui(input, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let r = crate::ui::field_menu(ui, "Rename", |ui| {
                        open.set(true);
                        let mut text = String::from("Show A");
                        let f = crate::ui::text_field(ui, &mut text, "name", Some(120.0));
                        field_at.set(f.rect.center());
                    });
                    button_at.set(r.response.rect.center());
                });
            });
            out.textures_delta.clear();
        };
        let click = |pos: egui::Pos2| {
            let b = egui::PointerButton::Primary;
            vec![
                egui::Event::PointerButton { pos, button: b, pressed: true, modifiers: Default::default() },
                egui::Event::PointerButton { pos, button: b, pressed: false, modifiers: Default::default() },
            ]
        };
        frame(vec![], egui::pos2(500.0, 300.0));
        frame(vec![], egui::pos2(500.0, 300.0));
        let at = button_at.get();
        frame(click(at), at);
        for _ in 0..3 {
            frame(vec![], at);
        }
        assert!(open.get(), "the menu opens when its button is clicked");
        let f = field_at.get();
        frame(click(f), f);
        for _ in 0..3 {
            frame(vec![], f);
        }
        assert!(open.get(), "and stays open when its text field is clicked");
        let away = egui::pos2(550.0, 380.0);
        frame(click(away), away);
        for _ in 0..3 {
            frame(vec![], away);
        }
        assert!(!open.get(), "and closes when the click is outside it");
    }
}

/// Pick one of `options` (value, label) from a list: the same choices as `segmented`, for where the buttons
/// would wrap (a narrow Inspector column). Returns true when the pick changed.
pub(crate) fn segmented_list<T: PartialEq + Clone>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, value: &mut T, options: &[(T, String)], width: f32) -> (bool, egui::Response) {
    let current = options.iter().find(|(v, _)| v == value).map_or("", |(_, l)| l.as_str()).to_string();
    let mut picked = None;
    let r = dropdown(ui, id, width).selected_text(current).show_ui(ui, |ui| {
        for (v, l) in options {
            if pick_item(ui, v == value, l.as_str()).clicked() {
                picked = Some(v.clone());
            }
        }
    });
    let changed = picked.as_ref().is_some_and(|p| p != value);
    if let Some(p) = picked {
        *value = p;
    }
    (changed, r.response)
}

/// `pick_item` cut short to `max_width` (whole on hover): for a list row that must keep one line (a long group or
/// slice name beside its controls).
pub(crate) fn pick_item_cut(ui: &mut egui::Ui, picked: bool, text: &str, max_width: f32) -> egui::Response {
    // (Cut here: a button lays its words out again with its own wrapping, so a truncating layout is lost.)
    let room = (max_width - ui.spacing().button_padding.x * 2.0 - 2.0).max(20.0);
    let shown = cut_to_width(ui, text, room);
    let cut = shown != text;
    let r = pick_item(ui, picked, shown);
    if cut { r.on_hover_text(text) } else { r }
}
