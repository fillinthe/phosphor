//! The interface language. Language packs (`crates/phosphor/lang/<code>.json`, English text → translation, built in)
//! are applied where text is laid out (the egui/epaint "PHOSPHOR PATCH": `epaint::text::translation`), so every
//! label, button, menu and tooltip changes without each one asking. Text being typed stays as typed.
//!
//! Lookup (`Catalog::translate`): the whole text; else without leading/trailing icons; else a template ("Used by {0}
//! layers", from `format!` strings) with the numbers / names filled back in; else line by line and part by part
//! (" · "). `scripts/i18n/extract.py` lists the app's text (`lang/strings.json`); `scripts/i18n/merge.py` checks a pack.
//! Japanese and Chinese need a font with their characters: the system's (Hiragino on a Mac) is added while one is on.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use eframe::egui;

/// The languages there are packs for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    /// The computer's language, when there's a pack for it (else English).
    Auto,
    English,
    Spanish,
    German,
    French,
    Italian,
    PortugueseBr,
    Japanese,
    ChineseSimplified,
}

impl Lang {
    pub const ALL: [Lang; 9] = [Lang::Auto, Lang::English, Lang::Spanish, Lang::German, Lang::French, Lang::Italian, Lang::PortugueseBr, Lang::Japanese, Lang::ChineseSimplified];

    /// The code it's saved as (and its pack's file name).
    pub fn code(self) -> &'static str {
        match self {
            Lang::Auto => "auto",
            Lang::English => "en",
            Lang::Spanish => "es",
            Lang::German => "de",
            Lang::French => "fr",
            Lang::Italian => "it",
            Lang::PortugueseBr => "pt-BR",
            Lang::Japanese => "ja",
            Lang::ChineseSimplified => "zh-CN",
        }
    }

    pub fn from_code(code: &str) -> Lang {
        Lang::ALL.into_iter().find(|l| l.code().eq_ignore_ascii_case(code)).unwrap_or(Lang::Auto)
    }

    /// Its own name (CJK names only once a CJK font is in: until then they'd draw as boxes).
    pub fn name(self) -> &'static str {
        match self {
            Lang::Auto => "Automatic",
            Lang::English => "English",
            Lang::Spanish => "Espa\u{f1}ol",
            Lang::German => "Deutsch",
            Lang::French => "Fran\u{e7}ais",
            Lang::Italian => "Italiano",
            Lang::PortugueseBr => "Portugu\u{ea}s (Brasil)",
            Lang::Japanese if cjk_font() => "\u{65e5}\u{672c}\u{8a9e}",
            Lang::Japanese => "Japanese",
            Lang::ChineseSimplified if cjk_font() => "\u{7b80}\u{4f53}\u{4e2d}\u{6587}",
            Lang::ChineseSimplified => "Chinese (Simplified)",
        }
    }

    /// The pack's text (None = English: nothing to translate).
    fn pack(self) -> Option<&'static str> {
        Some(match self {
            Lang::Auto | Lang::English => return None,
            Lang::Spanish => include_str!("../lang/es.json"),
            Lang::German => include_str!("../lang/de.json"),
            Lang::French => include_str!("../lang/fr.json"),
            Lang::Italian => include_str!("../lang/it.json"),
            Lang::PortugueseBr => include_str!("../lang/pt-BR.json"),
            Lang::Japanese => include_str!("../lang/ja.json"),
            Lang::ChineseSimplified => include_str!("../lang/zh-CN.json"),
        })
    }

    /// Auto → the computer's language (English when there's no pack for it).
    pub fn resolved(self) -> Lang {
        if self != Lang::Auto {
            return self;
        }
        system_languages().iter().find_map(|code| pack_for(code)).unwrap_or(Lang::English)
    }

    fn is_cjk(self) -> bool {
        matches!(self, Lang::Japanese | Lang::ChineseSimplified)
    }

    /// This computer has the font to draw it (Japanese and Chinese need one from the system; the browser app has none).
    pub fn drawable(self) -> bool {
        !self.resolved().is_cjk() || cjk_font_file(self.resolved()).is_some()
    }
}

/// The pack for a system language code ("es-MX", "pt_BR", "zh-Hans-CN", "ja-JP").
fn pack_for(code: &str) -> Option<Lang> {
    let c = code.replace('_', "-").to_ascii_lowercase();
    let main = c.split('-').next().unwrap_or("");
    Some(match main {
        "en" => Lang::English,
        "es" => Lang::Spanish,
        "de" => Lang::German,
        "fr" => Lang::French,
        "it" => Lang::Italian,
        // Portuguese from anywhere gets the Brazilian pack (the only one).
        "pt" => Lang::PortugueseBr,
        "ja" => Lang::Japanese,
        // Traditional Chinese readers get English rather than Simplified characters.
        "zh" if !(c.contains("hant") || c.ends_with("-tw") || c.ends_with("-hk") || c.ends_with("-mo")) => Lang::ChineseSimplified,
        _ => return None,
    })
}

/// The computer's preferred languages, most wanted first.
fn system_languages() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        let langs = objc2_foundation::NSLocale::preferredLanguages();
        langs.iter().map(|s| s.to_string()).collect()
    }
    #[cfg(not(target_os = "macos"))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"].iter().filter_map(|k| std::env::var(k).ok()).filter(|v| !v.is_empty() && v != "C" && v != "POSIX").collect()
    }
}

/// The chosen language (as chosen: may be Auto).
static CHOSEN: Mutex<Lang> = Mutex::new(Lang::Auto);
/// The pack in use (None = English).
static CATALOG: Mutex<Option<Arc<Catalog>>> = Mutex::new(None);
/// A CJK font is in the UI fonts now.
static CJK_FONT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn chosen() -> Lang {
    *CHOSEN.lock().unwrap()
}

pub fn cjk_font() -> bool {
    CJK_FONT.load(std::sync::atomic::Ordering::Relaxed)
}

/// Switches the interface to `lang` (also at start): its pack, and the fonts it needs.
pub fn set_language(ctx: &egui::Context, lang: Lang) {
    *CHOSEN.lock().unwrap() = lang;
    // No font for its characters: English rather than boxes.
    let now = if lang.drawable() { lang.resolved() } else { Lang::English };
    let catalog = now.pack().map(|text| Arc::new(Catalog::parse(text)));
    *CATALOG.lock().unwrap() = catalog.clone();
    eframe::egui::epaint::text::translation::set_translator(catalog.map(|c| Arc::new(move |s: &str| c.translate(s)) as Arc<eframe::egui::epaint::text::translation::Translator>));
    let cjk = now.is_cjk().then(|| cjk_font_file(now)).flatten();
    CJK_FONT.store(cjk.is_some(), std::sync::atomic::Ordering::Relaxed);
    crate::theme::set_fonts(ctx, cjk);
    ctx.request_repaint();
}

/// `text` in the interface language, for text egui doesn't draw (the Mac menu bar).
pub fn tr(text: &str) -> String {
    CATALOG.lock().unwrap().as_ref().and_then(|c| c.translate(text)).unwrap_or_else(|| text.to_string())
}

/// The system font with this language's characters (read once per session).
fn cjk_font_file(lang: Lang) -> Option<Arc<Vec<u8>>> {
    static CACHE: Mutex<Vec<(&'static str, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());
    let candidates: &[&str] = match lang {
        Lang::Japanese => &[
            "/System/Library/Fonts/\u{30d2}\u{30e9}\u{30ae}\u{30ce}\u{89d2}\u{30b4}\u{30b7}\u{30c3}\u{30af} W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "C:\\Windows\\Fonts\\YuGothM.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        ],
        _ => &[
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Medium.ttc",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
        ],
    };
    let mut cache = CACHE.lock().unwrap();
    for path in candidates {
        if let Some((_, bytes)) = cache.iter().find(|(p, _)| p == path) {
            return Some(bytes.clone());
        }
        if let Ok(bytes) = std::fs::read(path) {
            let bytes = Arc::new(bytes);
            cache.push((path, bytes.clone()));
            return Some(bytes);
        }
    }
    log::warn!("no font with {} characters found: that text will show as boxes", lang.code());
    None
}

/// A language pack, ready to look things up in.
pub struct Catalog {
    exact: HashMap<String, String>,
    /// Texts with {0}, {1}…: their fixed parts and the translation.
    templates: Vec<Template>,
}

struct Template {
    /// The fixed text around the gaps: `parts[0] {0} parts[1] {1} … parts[n]`.
    parts: Vec<String>,
    /// Which gap each `{i}` in the English is, in order (usually 0, 1, 2…).
    order: Vec<usize>,
    to: String,
}

impl Catalog {
    pub fn parse(json: &str) -> Catalog {
        let map: HashMap<String, String> = serde_json::from_str(json).unwrap_or_default();
        let mut exact = HashMap::new();
        let mut templates = Vec::new();
        for (from, to) in map {
            if from == to {
                continue;
            }
            // "{0} Recently added" is almost always an icon then words (`format!("{} …", icon::X)`): the words on
            // their own are what's looked up once the icon is set aside.
            if let (Some(words), Some(tr)) = (from.strip_prefix("{0} "), to.strip_prefix("{0} "))
                && !words.contains('{')
                && !tr.contains('{')
            {
                exact.entry(words.to_string()).or_insert_with(|| tr.to_string());
            }
            match Template::new(&from, &to) {
                Some(t) => templates.push(t),
                None => {
                    exact.insert(from, to);
                }
            }
        }
        // Most specific first: more fixed text wins over a looser match.
        templates.sort_by_key(|t| std::cmp::Reverse(t.parts.iter().map(String::len).sum::<usize>()));
        Catalog { exact, templates }
    }

    /// `text` in this language (None = no translation: show it as it is).
    pub fn translate(&self, text: &str) -> Option<String> {
        if text.trim().is_empty() {
            return None;
        }
        if let Some(t) = self.whole(text) {
            return Some(t);
        }
        // Line by line, then part by part.
        for sep in ["\n", " \u{b7} "] {
            if text.contains(sep) {
                let parts: Vec<&str> = text.split(sep).collect();
                let done: Vec<Option<String>> = parts.iter().map(|p| self.whole(p).or_else(|| (sep == "\n").then(|| self.translate(p)).flatten())).collect();
                if done.iter().any(Option::is_some) {
                    return Some(parts.iter().zip(done).map(|(p, d)| d.unwrap_or_else(|| p.to_string())).collect::<Vec<_>>().join(sep));
                }
            }
        }
        None
    }

    /// One piece: as it is, or without icons / spaces around it, or as a template.
    fn whole(&self, text: &str) -> Option<String> {
        if let Some(t) = self.exact.get(text) {
            return Some(t.clone());
        }
        let is_edge = |c: char| c.is_whitespace() || ('\u{E000}'..='\u{F8FF}').contains(&c);
        let core = text.trim_matches(is_edge);
        if core.is_empty() {
            return None;
        }
        let start = text.len() - text.trim_start_matches(is_edge).len();
        let (before, after) = (&text[..start], &text[start + core.len()..]);
        let found = if core.len() != text.len() { self.exact.get(core).cloned() } else { None }.or_else(|| self.template(core))?;
        Some(format!("{before}{found}{after}"))
    }

    fn template(&self, text: &str) -> Option<String> {
        self.templates.iter().find_map(|t| {
            let gaps = t.capture(text)?;
            // What filled a gap may be interface text itself ("Cut", "Forward").
            let gaps: Vec<String> = gaps.into_iter().map(|g| self.exact.get(g).cloned().unwrap_or_else(|| g.to_string())).collect();
            Some(t.fill(&gaps))
        })
    }
}

impl Template {
    fn new(from: &str, to: &str) -> Option<Template> {
        let mut parts = vec![String::new()];
        let mut order = Vec::new();
        let mut rest = from;
        while let Some(i) = rest.find('{') {
            let close = rest[i..].find('}')? + i;
            let n: usize = rest[i + 1..close].parse().ok()?;
            parts.last_mut().unwrap().push_str(&rest[..i]);
            order.push(n);
            parts.push(String::new());
            rest = &rest[close + 1..];
        }
        parts.last_mut().unwrap().push_str(rest);
        // Needs some fixed text to know it, and gaps that aren't side by side.
        if order.is_empty() || parts.iter().map(String::len).sum::<usize>() < 3 || parts[1..parts.len() - 1].iter().any(String::is_empty) {
            return None;
        }
        Some(Template { parts, order, to: to.to_string() })
    }

    /// The gap texts if `text` fits this template.
    fn capture<'a>(&self, text: &'a str) -> Option<Vec<&'a str>> {
        let first = &self.parts[0];
        let last = self.parts.last().unwrap();
        if !text.starts_with(first.as_str()) || !text.ends_with(last.as_str()) || text.len() < first.len() + last.len() {
            return None;
        }
        let mut rest = &text[first.len()..text.len() - last.len()];
        let mut gaps = Vec::with_capacity(self.order.len());
        for middle in &self.parts[1..self.parts.len() - 1] {
            let i = rest.find(middle.as_str())?;
            gaps.push(&rest[..i]);
            rest = &rest[i + middle.len()..];
        }
        gaps.push(rest);
        // A gap is a number, a name or a word or two: never empty, never a whole sentence of something else (another
        // line, or the next " · " piece, which translates by itself).
        if gaps.iter().any(|g| g.is_empty() || g.contains('\n') || g.contains(" \u{b7} ")) {
            return None;
        }
        Some(gaps)
    }

    fn fill(&self, gaps: &[String]) -> String {
        let mut out = self.to.clone();
        // {i} in the translation = the gap that was {i} in the English.
        for (pos, n) in self.order.iter().enumerate() {
            out = out.replace(&format!("{{{n}}}"), &gaps[pos]);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cat() -> Catalog {
        Catalog::parse(r#"{
            "Add effect": "Añadir efecto",
            "Opacity": "Opacidad",
            "Cut": "Corte",
            "Used by {0} layers and clips": "Usado por {0} capas y clips",
            "{0} of {1} files": "{1} archivos, {0} listos",
            "Fade style": "Estilo de fundido",
            "Launch: {0}": "Lanzar: {0}",
            "Same": "Same",
            "{0} Recently added": "{0} Añadido hace poco"
        }"#)
    }

    #[test]
    fn whole_texts_icons_and_templates() {
        let c = cat();
        assert_eq!(c.translate("Opacity").as_deref(), Some("Opacidad"));
        // An icon in front (as `ui::label` makes them) stays in front.
        assert_eq!(c.translate("\u{E3D4} Add effect").as_deref(), Some("\u{E3D4} Añadir efecto"));
        assert_eq!(c.translate("Used by 12 layers and clips").as_deref(), Some("Usado por 12 capas y clips"));
        assert_eq!(c.translate("3 of 9 files").as_deref(), Some("9 archivos, 3 listos"), "gaps can change places");
        assert_eq!(c.translate("Launch: Cut").as_deref(), Some("Lanzar: Corte"), "what fills a gap is translated too");
        assert_eq!(c.translate("Opacity\nFade style").as_deref(), Some("Opacidad\nEstilo de fundido"));
        assert_eq!(c.translate("Opacity \u{b7} 50%").as_deref(), Some("Opacidad \u{b7} 50%"));
        assert_eq!(c.translate("Launch: Cut \u{b7} Opacity").as_deref(), Some("Lanzar: Corte \u{b7} Opacidad"), "a gap stops at the next piece");
        assert_eq!(c.translate("\u{E19A} Recently added").as_deref(), Some("\u{E19A} Añadido hace poco"), "icon, then words");
        assert_eq!(c.translate("my_clip.mov"), None);
        assert_eq!(c.translate("Same"), None);
        assert_eq!(c.translate("Used by  layers and clips"), None, "an empty gap is no match");
    }

    #[test]
    fn system_language_codes_find_their_pack() {
        assert_eq!(pack_for("es-MX"), Some(Lang::Spanish));
        assert_eq!(pack_for("pt_PT"), Some(Lang::PortugueseBr));
        assert_eq!(pack_for("zh-Hans-CN"), Some(Lang::ChineseSimplified));
        assert_eq!(pack_for("zh-Hant-TW"), None);
        assert_eq!(pack_for("ja-JP"), Some(Lang::Japanese));
        assert_eq!(pack_for("ko-KR"), None);
        assert_eq!(Lang::from_code("pt-br"), Lang::PortugueseBr);
        assert_eq!(Lang::from_code("xx"), Lang::Auto);
    }

    /// Every pack is valid, keeps each text's {0} {1}…, and covers most of the app's text.
    #[test]
    fn language_packs_are_whole() {
        let strings: Vec<String> = serde_json::from_str(include_str!("../lang/strings.json")).unwrap();
        for lang in Lang::ALL.into_iter().filter(|l| l.pack().is_some()) {
            let map: HashMap<String, String> = serde_json::from_str(lang.pack().unwrap()).unwrap_or_else(|e| panic!("{}: {e}", lang.code()));
            let gaps = |s: &str| {
                let mut g: Vec<String> = s.match_indices('{').filter_map(|(i, _)| s[i..].find('}').map(|j| s[i..i + j + 1].to_string())).collect();
                g.sort();
                g
            };
            for (from, to) in &map {
                assert_eq!(gaps(from), gaps(to), "{}: {from:?} -> {to:?}", lang.code());
            }
            let covered = strings.iter().filter(|s| map.contains_key(*s)).count();
            assert!(covered * 10 >= strings.len() * 9, "{}: only {covered} of {} texts", lang.code(), strings.len());
        }
    }
}
