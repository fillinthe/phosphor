//! Words on screen: numbers, plurals, names cut to fit, and the "file is away" mark.

use crate::ui::*;

/// Show / hide: an eye, crossed out in orange when hidden. Marks the response changed.
/// A number as written ("-0.00", "-0", "-0%") without the minus a value just below zero picks up when rounded:
/// zero is never negative on screen (owner, 2026-11-14).
pub(crate) fn no_minus_zero(s: String) -> String {
    match s.strip_prefix('-') {
        Some(rest) if !rest.chars().any(|c| c.is_ascii_digit() && c != '0') => rest.to_string(),
        _ => s,
    }
}

/// A destructive command (Delete, Remove, Stop watching…): red words, so it's plain before the click that
/// something goes away (owner, 2026-11-16: these can hurt a running show).
/// Over a picture whose file isn't there right now (its drive unplugged): the picture dimmed, with a plug and
/// "Not connected" along its top. `plays` = a copy on this computer still plays it, so it's only marked, not dimmed.
/// One look for every place footage is shown (clip cells, media tiles).
pub(crate) fn away_mark(p: &egui::Painter, rect: egui::Rect, plays: bool) {
    if !plays {
        p.rect_filled(rect, 4.0, Color32::from_black_alpha(140));
    }
    let band = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 14.0f32.min(rect.height())));
    p.rect_filled(band, 4.0, Color32::from_black_alpha(190));
    let words = if band.width() > 96.0 { format!("{} {}", egui_phosphor::regular::PLUG, crate::i18n::tr("Not connected")) } else { egui_phosphor::regular::PLUG.to_string() };
    p.with_clip_rect(band).text(band.center(), egui::Align2::CENTER_CENTER, words, egui::FontId::proportional(10.0), crate::theme::WARN);
}

/// Is the file at `path` away (not there the last time the disk was looked at)? Never touches the disk itself
/// (`media::present`); false in the browser and for web addresses.
pub(crate) fn file_away(path: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        !path.starts_with("http") && crate::media::present::file_present(path) == Some(false)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        false
    }
}

/// A copy of `path` on this computer still plays it (`media::clip_cache`).
pub(crate) fn plays_away(path: &str) -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        crate::media::clip_cache::has(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        let _ = path;
        false
    }
}

/// `one` or `many` by `n`, with `{0}` replaced by the number ("{0} file was" / "{0} files were"): plain plurals
/// instead of "file(s)". Write both as whole sentences so each translates.
pub(crate) fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 { one } else { many }.replace("{0}", &n.to_string())
}

/// `text` cut to at most `max_chars` characters with an ellipsis (a layer named after a movie file on a button, a
/// media bin label). For a width in points use `pick_item_cut`.
pub(crate) fn short_name(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max_chars.saturating_sub(1)).collect::<String>().trim_end())
}

/// `text` cut with an ellipsis so it is at most `room` points wide in the button font (whole if it fits). For
/// anything that must keep its width whatever it's called: a long file name never pushes a row past its panel.
pub(crate) fn cut_to_width(ui: &egui::Ui, text: &str, room: f32) -> String {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let width = |t: &str| ui.painter().layout_no_wrap(t.to_string(), font.clone(), Color32::WHITE).size().x;
    if width(text) <= room {
        return text.to_string();
    }
    let mut chars: Vec<char> = text.chars().collect();
    while !chars.is_empty() && width(&format!("{}…", chars.iter().collect::<String>())) > room {
        chars.pop();
    }
    format!("{}…", chars.iter().collect::<String>().trim_end())
}
