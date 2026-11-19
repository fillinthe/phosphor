//! Rows, sections, headings, labelled fields, scrolling columns, dialogs and counts.

use crate::ui::*;

/// The one height of a control in a row (buttons, toggles, pick-one groups, fields, sliders: `theme` sets egui's
/// `interact_size.y` to it). Everything in a row being this tall means a row never grows part-way through, so nothing
/// sits high or jumps. Taller things (the big GO) get a row of their own.
pub(crate) const CONTROL_H: f32 = 22.0;

/// A bar's row of buttons (the top bar): as tall as its tallest button from the start, every item centered on the
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

/// How wide each tile of a wrapping grid is so the tiles fill the row: as many columns as fit at `min_w`, then the
/// leftover shared out (no ragged empty strip down the right of a panel).
pub(crate) fn tile_width(available: f32, min_w: f32, gap: f32) -> f32 {
    let columns = ((available + gap) / (min_w + gap)).floor().max(1.0);
    ((available - gap * (columns - 1.0)) / columns).floor().max(min_w.min(available))
}

/// Width of the label column in `field` rows (labels line up down a panel).
pub(crate) const LABEL_W: f32 = 96.0;

/// A group of settings that folds away: a bold title, its contents below. `id` keeps it open or shut on its own.
pub(crate) fn section<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, open: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::CollapsingResponse<R> {
    egui::CollapsingHeader::new(RichText::new(title.into()).strong()).id_salt(id).default_open(open).show(ui, add)
}

/// A section whose help rides in its header: a small (i) after the title (hover for the sentence) when the user
/// picked "Help icons", else the sentence as the body's first line. (A `help` call as a section's first line made a
/// row with nothing but an (i) on it.) `help` None = a plain `section`.
pub(crate) fn section_help<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, help: Option<&str>, open: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> Option<R> {
    let title = title.into();
    let Some(help) = help.filter(|_| crate::ui::widgets::help_icons()) else {
        return section(ui, id, title, open, |ui| {
            if let Some(h) = help {
                crate::ui::widgets::help(ui, h);
            }
            add(ui)
        })
        .body_returned;
    };
    let state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), ui.id().with(id), open);
    state
        .show_header(ui, |ui| {
            ui.label(RichText::new(title).strong());
            ui.label(RichText::new(icon::INFO).weak()).on_hover_text(help);
        })
        .body(add)
        .2
        .map(|r| r.inner)
}

/// A smaller fold-away group inside a section (a transition's settings, corners by number).
pub(crate) fn subsection<R>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>, open: bool, add: impl FnOnce(&mut egui::Ui) -> R) -> egui::CollapsingResponse<R> {
    egui::CollapsingHeader::new(RichText::new(title.into()).small()).id_salt(id).default_open(open).show(ui, add)
}

/// A heading with its help: the (i) rides at the end of the title row when help icons are on (the sentence goes
/// under the title when they're off), never a row with nothing but the icon.
pub(crate) fn heading_help(ui: &mut egui::Ui, text: impl ToString, help_text: &str) -> egui::Response {
    if !help_icons() {
        let r = heading(ui, text);
        help(ui, help_text);
        return r;
    }
    ui.horizontal(|ui| {
        let r = heading(ui, text);
        help(ui, help_text);
        r
    })
    .inner
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

/// A dialog window: it opens in the middle of the app and can't fold up (egui puts a new window at the top left,
/// over the menu bar). Chain `.open()`, `.default_width()`… and `.show()` as with any window.
pub(crate) fn dialog<'a>(ctx: &egui::Context, title: impl Into<egui::WidgetText>) -> egui::Window<'a> {
    egui::Window::new(title).collapsible(false).pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.content_rect().center())
}

/// Counts as small gray words side by side ("1 output", "75 slices"): `(n, one, many)` with `{0}` where the number
/// goes; counts of 0 are left out. Each is its own label, so each translates.
pub(crate) fn counts(ui: &mut egui::Ui, items: &[(usize, &str, &str)]) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for (n, one, many) in items.iter().filter(|(n, _, _)| *n > 0) {
            crate::ui::hint(ui, if *n == 1 { one } else { many }.replace("{0}", &n.to_string()));
        }
    });
}

#[cfg(test)]
mod tile_tests {
    use crate::ui::tile_width;

    #[test]
    fn tiles_fill_the_row_and_never_wrap_early() {
        for avail in [150.0_f32, 152.0, 300.0, 312.0, 385.0, 600.0, 1234.5] {
            let (min_w, gap) = (152.0, 8.0);
            let w = tile_width(avail, min_w, gap);
            let columns = ((avail + gap) / (min_w + gap)).floor().max(1.0);
            // All the columns that fit at the smallest size still fit at the wider one.
            assert!(w * columns + gap * (columns - 1.0) <= avail.max(w) + 0.01, "{avail}: {w} x {columns}");
            // Less than a column's worth is left over (one pixel a column for rounding).
            assert!(avail - (w * columns + gap * (columns - 1.0)) <= columns.max(1.0) || avail < min_w, "{avail}");
            assert!(w >= min_w.min(avail), "{avail}: {w}");
        }
    }
}
