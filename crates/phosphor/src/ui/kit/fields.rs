//! Fields and the menus built around them: text, password, search, lists, names, renames, outputs.

use crate::ui::*;

/// A drop-down `width` wide (plus its arrow) that moves to the next line of a wrapping row when it won't fit.
pub(crate) fn dropdown(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, width: f32) -> egui::ComboBox {
    make_room(ui, width + ui.spacing().icon_width + ui.spacing().button_padding.x * 2.0);
    // Cut short at its width: a long choice (a layer named after a long file) made the drop-down, then the whole
    // panel, wider, so a grid beside it jumped between 3 and 4 columns (owner, 2026-11-14: interfaces must not shift).
    egui::ComboBox::from_id_salt(id).width(width).truncate()
}

/// A one-line text field: `hint` shows while it's empty; `width` None = the room there is.
pub(crate) fn text_field(ui: &mut egui::Ui, text: &mut String, hint: &str, width: Option<f32>) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(width.unwrap_or_else(|| ui.available_width())))
}

/// A one-line secret (a password, a stream key): dots instead of the letters; `hint` shows while it's empty.
pub(crate) fn password_field(ui: &mut egui::Ui, text: &mut String, hint: &str, width: f32) -> egui::Response {
    ui.add(egui::TextEdit::singleline(text).password(true).hint_text(hint).desired_width(width))
}

/// A few lines of your own words (a note on a canvas): as wide as there's room, `rows` tall to start with.
pub(crate) fn note_field(ui: &mut egui::Ui, text: &mut String, hint: &str, rows: usize) -> egui::Response {
    ui.add(egui::TextEdit::multiline(text).hint_text(hint).desired_rows(rows).desired_width(ui.available_width()).font(egui::FontId::proportional(12.0)))
}

/// The search box at the top of a drop-down or menu: returns what's typed (trimmed, lowercase) and the field. Starts
/// empty each time the list opens (it used to keep the last search), and takes the keyboard when it opens or when
/// nothing else has it (never every frame: that kept the field from letting go). Use it for any searchable list.
pub(crate) fn search_field(ui: &mut egui::Ui, key: impl std::hash::Hash + std::fmt::Debug, hint: &str, width: f32) -> (String, egui::Response) {
    let key = egui::Id::new(("search_field", key));
    let pass = ui.ctx().cumulative_pass_nr();
    let (mut query, seen): (String, u64) = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
    // Not drawn last pass: the list was closed and has just opened again.
    let fresh = seen + 1 < pass;
    if fresh {
        query.clear();
    }
    let field = text_field(ui, &mut query, hint, Some(width));
    if fresh || ui.memory(|m| m.focused().is_none()) {
        field.request_focus();
    }
    let q = query.trim().to_lowercase();
    ui.ctx().data_mut(|d| d.insert_temp(key, (query, pass)));
    (q, field)
}

/// A menu's search box: the field (it takes the keyboard when nothing else has it), then, while something is typed,
/// the entries `find` gives for it as (what, label, tooltip); Enter picks the first. Returns the one picked and
/// whether a search is on (the menu's own entries hide then). For any add menu with more entries than fit a glance.
pub(crate) fn menu_search<T>(ui: &mut egui::Ui, key: impl std::hash::Hash + std::fmt::Debug, hint_text: &str, nothing: &str, find: impl FnOnce(&str) -> Vec<(T, String, String)>) -> (Option<T>, bool) {
    let (q, field) = search_field(ui, key, hint_text, 200.0);
    let searching = !q.is_empty();
    let mut picked = None;
    if searching {
        let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let found = find(&q);
        if found.is_empty() {
            crate::ui::widgets::hint(ui, nothing);
        }
        for (i, (what, label, about)) in found.into_iter().enumerate() {
            let r = command(ui, label);
            let r = if about.is_empty() { r } else { r.on_hover_text(about) };
            if (r.clicked() || (enter && i == 0)) && picked.is_none() {
                picked = Some(what);
            }
        }
        if picked.is_some() {
            ui.close();
        }
    }
    (picked, searching)
}

/// A list edited as one line of text ("8000, 9001"; "Normal, Add, Screen"). What you type stays as typed while you
/// type it (so "8000," doesn't snap back), but when the list changes elsewhere (a project opens, undo, another
/// control) the field follows it instead of showing stale text. `show` = the list as text; `read` = text to list.
pub(crate) fn list_field<T: PartialEq>(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, list: &mut Vec<T>, hint: &str, width: Option<f32>, show: impl Fn(&[T]) -> String, read: impl Fn(&str) -> Vec<T>) -> egui::Response {
    let key = ui.id().with(("list_field", id));
    let now = show(list);
    let cached: Option<(String, String)> = ui.data_mut(|d| d.get_temp(key));
    let (mut text, _) = list_text(cached, &now);
    let r = text_field(ui, &mut text, hint, width);
    let mut synced = now;
    if r.changed() {
        *list = read(&text);
        synced = show(list);
    }
    ui.data_mut(|d| d.insert_temp(key, (text, synced)));
    r
}

/// The text to show: the typed text while the list is still what it was when typed (`synced`), else the list as it is now.
fn list_text(cached: Option<(String, String)>, now: &str) -> (String, bool) {
    match cached {
        Some((text, synced)) if synced == now => (text, true),
        _ => (now.to_string(), false),
    }
}

#[cfg(test)]
mod list_field_tests {
    #[test]
    fn typed_text_stays_until_the_list_changes_elsewhere() {
        // Typing "8000," reads as [8000] and shows as "8000": the comma stays on screen.
        assert_eq!(super::list_text(Some(("8000,".into(), "8000".into())), "8000"), ("8000,".to_string(), true));
        // Another project opened: the list is "9000" now, so the old text goes.
        assert_eq!(super::list_text(Some(("8000,".into(), "8000".into())), "9000"), ("9000".to_string(), false));
        assert_eq!(super::list_text(None, "1, 2"), ("1, 2".to_string(), false));
    }
}

/// "Shows on": which outputs show this layer / group on their own (a slice each). `outs` = (output id, name,
/// showing now). Returns (output, on) when one is switched.
/// The name row of a group / comp Inspector: `before` (on/off, kind icon, outputs…), the name field, and the row's
/// help (i) at its end when help icons are on (the sentence goes under the row when they're off), never a row with
/// nothing but the icon.
pub(crate) fn name_row<R>(ui: &mut egui::Ui, name: &mut String, help_text: &str, before: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let icons = help_icons();
    let out = ui
        .horizontal(|ui| {
            let r = before(ui);
            let room = if icons { 28.0 } else { 0.0 };
            ui.add(egui::TextEdit::singleline(name).desired_width(ui.available_width() - room));
            if icons {
                help(ui, help_text);
            }
            r
        })
        .inner;
    if !icons {
        help(ui, help_text);
    }
    out
}

pub(crate) fn outputs_menu(ui: &mut egui::Ui, outs: &[(u64, String, bool)]) -> Option<(u64, bool)> {
    let n = outs.iter().filter(|o| o.2).count();
    let text = if n == 0 { icon::MONITOR.to_string() } else { format!("{} {n}", icon::MONITOR) };
    let mut out = None;
    field_menu(ui, text, |ui| {
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

/// A rename waiting in its pop-up: whose it is, what's typed, and when its owner last drew it.
#[derive(Clone)]
struct RenameAsk {
    key: egui::Id,
    text: String,
    seen: f64,
    fresh: bool,
}

const RENAME_ASK: &str = "phosphor_rename_ask";

/// "Rename…" in a menu: closes the menu and opens a small pop-up in the middle of the window with the name in a
/// field (the owner found a field tucked in a submenu a strange way to rename, 2026-11-14). Pair it with `renamed`
/// (same `id`), called every frame outside the menu: that draws the pop-up and returns the new name.
pub(crate) fn rename_item(ui: &mut egui::Ui, id: impl std::hash::Hash + std::fmt::Debug, current: &str) {
    if crate::ui::command(ui, crate::ui::label(icon::PENCIL_SIMPLE, "Rename…")).clicked() {
        ask_rename(ui.ctx(), id, current);
        ui.close();
    }
}

/// Opens the rename pop-up for `id` (a plain Rename button; `rename_item` is the menu entry).
pub(crate) fn ask_rename(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug, current: &str) {
    let ask = RenameAsk { key: egui::Id::new(("rename", id)), text: current.to_string(), seen: ctx.input(|i| i.time), fresh: true };
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(RENAME_ASK), ask));
}

/// The pop-up for `rename_item(.., id, ..)` while it's open: a field (focused, the name selected), Rename (or Enter)
/// and Cancel (or Escape, or a click outside). Returns the new name once, trimmed and never empty. Call every frame
/// from where the thing is drawn; a pop-up whose owner stopped drawing it goes away.
pub(crate) fn renamed(ctx: &egui::Context, id: impl std::hash::Hash + std::fmt::Debug) -> Option<String> {
    let slot = egui::Id::new(RENAME_ASK);
    let mut ask: RenameAsk = ctx.data(|d| d.get_temp(slot))?;
    let now = ctx.input(|i| i.time);
    if ask.key != egui::Id::new(("rename", id)) {
        return None;
    }
    if now - ask.seen > 0.5 {
        ctx.data_mut(|d| d.remove::<RenameAsk>(slot));
        return None;
    }
    let (mut done, mut cancel) = (false, false);
    let modal = egui::Modal::new(egui::Id::new("phosphor_rename_modal")).show(ctx, |ui| {
        ui.set_width(300.0);
        heading(ui, "Rename");
        let field = text_field(ui, &mut ask.text, "", Some(300.0));
        if ask.fresh {
            field.request_focus();
            // The whole name selected: typing replaces it.
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), field.id) {
                let end = egui::text::CCursor::new(ask.text.chars().count());
                state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), end)));
                state.store(ui.ctx(), field.id);
                ask.fresh = false;
            }
        }
        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            let ok = ui.add_enabled(!ask.text.trim().is_empty(), egui::Button::new("Rename")).clicked();
            done = (ok || enter) && !ask.text.trim().is_empty();
            cancel = ui.button("Cancel").clicked();
        });
    });
    if done || cancel || modal.should_close() {
        ctx.data_mut(|d| d.remove::<RenameAsk>(slot));
        return done.then(|| ask.text.trim().to_string());
    }
    ask.seen = now;
    ctx.data_mut(|d| d.insert_temp(slot, ask));
    None
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
