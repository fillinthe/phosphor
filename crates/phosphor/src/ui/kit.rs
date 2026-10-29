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

/// Show / hide: an eye, crossed out in orange when hidden. Marks the response changed.
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
pub(crate) fn pick_item(ui: &mut egui::Ui, picked: bool, text: impl Into<egui::WidgetText>) -> egui::Response {
    // egui frames an entry only while it's hovered or picked, which made it 2 points wider then (and moved what's
    // next to it): it's always as wide as when framed.
    let text: egui::WidgetText = text.into();
    let w = text.clone().into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button).size().x;
    ui.add(egui::Button::selectable(picked, text).min_size(egui::vec2(w + ui.spacing().button_padding.x * 2.0 + 2.0, 0.0)))
}

/// A section of the app (Show, Outputs, Control, Designer) in its colour: the one you're in is filled with the
/// colour and its words are black; hovering another tints it. One size whatever its state.
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

/// A command that can't be used right now (nothing to save, nothing selected): greyed out until it can.
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

/// `small_command`, greyed out until it can be used.
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
    let text: String = text.into();
    let mut r = ui.add(egui::Button::new(RichText::new(&text).strong().size(16.0).color(ink)).fill(fill).stroke(Stroke::NONE).corner_radius(6.0).min_size(egui::vec2(80.0, 30.0)));
    crate::ui::press::mappable(ui, &mut r, &text);
    r
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

/// A panel's scrolling column: scrolls up and down and is never wider than the panel. (egui's own vertical scroll
/// area widens itself to fit anything too wide, and stays wide, so one long row made every row under it lay out wider
/// than the panel and get cut off.)
///
/// It also scrolls while something is being dragged over it (egui stops the wheel during a drag): see `carry_scroll`.
pub(crate) fn column_scroll(ui: &egui::Ui) -> ColumnScroll {
    ColumnScroll(egui::ScrollArea::vertical().max_width(ui.available_width()))
}

/// `column_scroll`'s scroll area.
pub(crate) struct ColumnScroll(egui::ScrollArea);

impl ColumnScroll {
    pub(crate) fn id_salt(self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        Self(self.0.id_salt(id))
    }

    pub(crate) fn auto_shrink(self, shrink: impl Into<egui::Vec2b>) -> Self {
        Self(self.0.auto_shrink(shrink))
    }

    pub(crate) fn max_height(self, h: f32) -> Self {
        Self(self.0.max_height(h))
    }

    pub(crate) fn max_width(self, w: f32) -> Self {
        Self(self.0.max_width(w))
    }

    pub(crate) fn stick_to_bottom(self, stick: bool) -> Self {
        Self(self.0.stick_to_bottom(stick))
    }

    pub(crate) fn show<R>(self, ui: &mut egui::Ui, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::scroll_area::ScrollAreaOutput<R> {
        self.0.show(ui, |ui| {
            let r = add(ui);
            carry_scroll(ui);
            r
        })
    }
}

/// While something is dragged (an effect, a clip, a file), the scroll area this is called in (at the end of its
/// contents) still scrolls: with the wheel / trackpad, and by itself when the pointer is near its top or bottom edge.
/// The innermost one under the pointer wins.
pub(crate) fn carry_scroll(ui: &egui::Ui) {
    let ctx = ui.ctx();
    if !egui::DragAndDrop::has_any_payload(ctx) {
        return;
    }
    let area = ui.clip_rect();
    let Some(pos) = ctx.pointer_latest_pos().filter(|p| area.contains(*p)) else { return };
    let pass = ctx.cumulative_pass_nr();
    let done = egui::Id::new("carry_scroll_pass");
    if ctx.data(|d| d.get_temp::<u64>(done)) == Some(pass) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(done, pass));
    let mut dy = ctx.input(|i| i.smooth_scroll_delta.y);
    // Near an edge: faster the closer it gets.
    let edge = (area.height() * 0.15).clamp(12.0, 40.0);
    let speed = 900.0 * ctx.input(|i| i.stable_dt).min(0.05);
    if pos.y < area.top() + edge {
        dy += speed * (1.0 - (pos.y - area.top()) / edge);
    } else if pos.y > area.bottom() - edge {
        dy -= speed * (1.0 - (area.bottom() - pos.y) / edge);
    }
    if dy != 0.0 {
        ui.scroll_with_delta_animation(egui::vec2(0.0, dy), egui::style::ScrollAnimation::none());
        ctx.request_repaint();
    }
}

/// In a wrapping row, starts a new line when what comes next (`need` wide) won't fit in what's left of this one.
/// Containers (pick-one groups, drop-downs) only know their size once drawn, so a wrapping row can't move them along
/// by itself. Does nothing in other rows, or at the start of a line.
pub(crate) fn make_room(ui: &mut egui::Ui, need: f32) {
    if !ui.layout().main_wrap {
        return;
    }
    // (In a wrapping row `available_width` is the whole row; what's left of this line is up to the cursor.)
    let room = ui.max_rect().right() - ui.cursor().left();
    if need > room && ui.cursor().left() > ui.max_rect().left() + 1.0 {
        ui.end_row();
    }
}

/// A drop-down `width` wide (plus its arrow) that moves to the next line of a wrapping row when it won't fit.
pub(crate) fn dropdown(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, width: f32) -> egui::ComboBox {
    make_room(ui, width + ui.spacing().icon_width + ui.spacing().button_padding.x * 2.0);
    egui::ComboBox::from_id_salt(id).width(width)
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
        help(ui, "An output of its own, besides the mix (a slice there: shape it in the Outputs area)");
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
    .on_hover_text(if n == 0 { "Send this to an output of its own".to_string() } else { plural(n, "On {0} output of its own", "On {0} outputs of its own") });
    out
}

/// A menu button whose menu has a text field in it: egui closes a menu on ANY click inside it, which closed the
/// pop-up the moment the field was clicked. This one stays open until you click outside it or the field confirms
/// (`rename_menu` / `new_name_menu` close it). Use it for every menu button that holds a field.
pub(crate) fn field_menu<'a, R>(ui: &mut egui::Ui, atoms: impl egui::IntoAtoms<'a>, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> egui::InnerResponse<Option<R>> {
    use egui::containers::menu::{MenuButton, MenuConfig, SubMenuButton, is_in_menu};
    let config = MenuConfig::new().close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside);
    let (response, inner) = if is_in_menu(ui) { SubMenuButton::new(atoms).config(config).ui(ui, add_contents) } else { MenuButton::new(atoms).config(config).ui(ui, add_contents) };
    egui::InnerResponse::new(inner.map(|i| i.inner), response)
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
    action_button_named(ui, act, text, size, None)
}

/// `action_button_sized`, mapped under `name` (a label that changes) instead of its text.
fn action_button_named(ui: &mut egui::Ui, act: Act, text: impl Into<String>, size: egui::Vec2, name: Option<&str>) -> egui::Response {
    let fill = match act {
        Act::Go => accent(),
        Act::Record => Color32::from_rgb(225, 70, 60),
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

/// A destructive command (Delete, Remove…): red text.
pub(crate) fn danger_button(ui: &mut egui::Ui, text: impl Into<String>) -> egui::Response {
    command(ui, RichText::new(text.into()).color(crate::theme::WARN))
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
                super::toolbar(ui, |ui| {
                    xs.push(ui.cursor().left());
                    ui.label("Launch");
                    xs.push(ui.cursor().left());
                    let mut fade = fade_on;
                    super::segmented(ui, &mut fade, &[(false, "Cut".to_string()), (true, "Fade".to_string())], |_, _, _| {});
                    xs.push(ui.cursor().left());
                    let mut secs = 0.5f32;
                    ui.add(egui::DragValue::new(&mut secs).fixed_decimals(2).suffix(" s"));
                    xs.push(ui.cursor().left());
                    let mut on = true;
                    super::toggle(ui, &mut on, "AUTO");
                    xs.push(ui.cursor().left());
                    super::command(ui, "Resync");
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
                    super::toolbar(ui, |ui| {
                        super::pick_item(ui, false, "List entry");
                        xs.push(ui.cursor().left());
                        super::pick_item(ui, true, "Picked entry");
                        xs.push(ui.cursor().left());
                        super::small_command(ui, "Small");
                        xs.push(ui.cursor().left());
                        super::command_if(ui, true, "Command");
                        xs.push(ui.cursor().left());
                        super::steady_button(ui, "Play", &["Play", "Pause"]);
                        xs.push(ui.cursor().left());
                        super::action_button(ui, super::Act::Go, "Go");
                        xs.push(ui.cursor().left());
                        let mut v = 1usize;
                        super::dropdown(ui, "dd", 90.0).selected_text("Crossfade").show_ui(ui, |ui| {
                            super::pick_value(ui, &mut v, 1, "Crossfade");
                        });
                        xs.push(ui.cursor().left());
                        let mut on = false;
                        super::toggle(ui, &mut on, "Toggle");
                        xs.push(ui.cursor().left());
                        let mut b = false;
                        super::icon_toggle(ui, &mut b, "S");
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
                    let r = super::field_menu(ui, "Rename", |ui| {
                        open.set(true);
                        let mut text = String::from("Show A");
                        let f = super::text_field(ui, &mut text, "name", Some(120.0));
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
/// A dialog window: it opens in the middle of the app and can't fold up (egui puts a new window at the top left,
/// over the menu bar). Chain `.open()`, `.default_width()`… and `.show()` as with any window.
pub(crate) fn dialog<'a>(ctx: &egui::Context, title: impl Into<egui::WidgetText>) -> egui::Window<'a> {
    egui::Window::new(title).collapsible(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center())
}

/// Counts as small grey words side by side ("1 output", "75 slices"): `(n, one, many)` with `{0}` where the number
/// goes; counts of 0 are left out. Each is its own label, so each translates.
pub(crate) fn counts(ui: &mut egui::Ui, items: &[(usize, &str, &str)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (n, one, many) in items.iter().filter(|(n, _, _)| *n > 0) {
            super::hint(ui, if *n == 1 { one } else { many }.replace("{0}", &n.to_string()));
        }
    });
}

/// `one` or `many` by `n`, with `{0}` replaced by the number ("{0} file was" / "{0} files were"): plain plurals
/// instead of "file(s)". Write both as whole sentences so each translates.
pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { one } else { many }.replace("{0}", &n.to_string())
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
