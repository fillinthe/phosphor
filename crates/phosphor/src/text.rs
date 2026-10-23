//! Text layers: draws a `TextStyle` into an RGBA picture on the CPU (ab_glyph). Runs off the UI thread
//! (see `MediaManager::sync`) and only when the text or its style changes; the result goes up like an image.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use phosphor_engine::{TextAlign, TextStyle};

/// A font you can pick: shown name, and the `TextStyle::font` value that selects it.
#[derive(Clone, Debug)]
pub struct FontChoice {
    pub name: String,
    pub id: String,
}

pub const BUILTIN: [(&str, &str); 2] = [("Phosphor Sans (light)", "builtin:sans"), ("Phosphor Mono", "builtin:mono")];

type FontList = std::sync::Arc<Vec<FontChoice>>;

fn font_list() -> &'static std::sync::RwLock<Option<FontList>> {
    static FONTS: std::sync::RwLock<Option<FontList>> = std::sync::RwLock::new(None);
    &FONTS
}

/// Built-in fonts plus the ones installed on this computer and the ones downloaded into Phosphor (`fonts_dir`),
/// sorted by name. The first call scans the font folders (a fraction of a second); later calls are free.
pub fn fonts() -> FontList {
    if let Some(list) = font_list().read().ok().and_then(|l| l.clone()) {
        return list;
    }
    let mut out: Vec<FontChoice> = BUILTIN.iter().map(|(n, id)| FontChoice { name: n.to_string(), id: id.to_string() }).collect();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut found = system_fonts();
        sort_fonts(&mut found);
        out.extend(found);
    }
    let list = std::sync::Arc::new(out);
    if let Ok(mut l) = font_list().write() {
        // Another thread may have finished first: keep its list.
        return l.get_or_insert(list).clone();
    }
    list
}

#[cfg(not(target_arch = "wasm32"))]
fn sort_fonts(list: &mut Vec<FontChoice>) {
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    list.dedup_by(|a, b| a.name == b.name);
}

/// Where fonts brought in from online libraries go (scanned with the installed ones).
#[cfg(not(target_arch = "wasm32"))]
pub fn fonts_dir() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("fonts"))
}

/// Adds a newly downloaded font file's faces to the list (no rescan). Returns their names.
#[cfg(not(target_arch = "wasm32"))]
pub fn add_font_file(path: &std::path::Path) -> Vec<String> {
    let new = faces_in(path);
    let names = new.iter().map(|f| f.name.clone()).collect();
    let mut all: Vec<FontChoice> = fonts().iter().filter(|f| !new.iter().any(|n| n.name == f.name)).cloned().collect();
    let builtin = all.len().min(BUILTIN.len());
    let mut rest = all.split_off(builtin);
    rest.extend(new);
    sort_fonts(&mut rest);
    all.extend(rest);
    if let Ok(mut l) = font_list().write() {
        *l = Some(std::sync::Arc::new(all));
    }
    names
}

/// A new text layer's look: a bold installed font when there is one (titles read better bold).
pub fn default_style() -> TextStyle {
    let bold = ["Avenir Next Bold", "Helvetica Neue Bold", "Arial Bold", "Futura Bold"];
    let all = fonts();
    let font = bold.iter().find_map(|want| all.iter().find(|f| f.name == *want)).map_or_else(|| "builtin:sans".to_string(), |f| f.id.clone());
    TextStyle { font, outline: 0.03, ..Default::default() }
}

/// The shown name of a `TextStyle::font` value.
pub fn font_name(id: &str) -> String {
    fonts().iter().find(|f| f.id == id).map_or_else(|| id.rsplit('/').next().unwrap_or(id).to_string(), |f| f.name.clone())
}

#[cfg(not(target_arch = "wasm32"))]
fn system_fonts() -> Vec<FontChoice> {
    let mut dirs = vec![std::path::PathBuf::from("/System/Library/Fonts"), "/System/Library/Fonts/Supplemental".into(), "/Library/Fonts".into()];
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(std::path::PathBuf::from(home).join("Library/Fonts"));
    }
    dirs.extend(fonts_dir());
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            // Skip huge CJK collections: slow to read and rarely what a title wants.
            if e.metadata().is_ok_and(|m| m.len() > 30 << 20) {
                continue;
            }
            out.extend(faces_in(&e.path()));
        }
    }
    out
}

/// The faces in one font file (a collection holds several).
#[cfg(not(target_arch = "wasm32"))]
fn faces_in(path: &std::path::Path) -> Vec<FontChoice> {
    let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_lowercase();
    if !matches!(ext.as_str(), "ttf" | "otf" | "ttc") {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    let count = ttf_parser::fonts_in_collection(&bytes).unwrap_or(1);
    let mut out = Vec::new();
    for index in 0..count.min(24) {
        let Ok(face) = ttf_parser::Face::parse(&bytes, index) else { continue };
        let Some(name) = face_name(&face) else { continue };
        let id = if count > 1 { format!("{}#{index}", path.display()) } else { path.display().to_string() };
        out.push(FontChoice { name, id });
    }
    out
}

/// "Avenir Next Bold": the full name, or family + style.
#[cfg(not(target_arch = "wasm32"))]
fn face_name(face: &ttf_parser::Face) -> Option<String> {
    let get = |id: u16| {
        face.names().into_iter().filter(|n| n.name_id == id).find_map(|n| n.to_string()).filter(|s| !s.trim().is_empty() && !s.starts_with('.'))
    };
    get(ttf_parser::name_id::FULL_NAME).or_else(|| {
        let family = get(ttf_parser::name_id::FAMILY)?;
        Some(match get(ttf_parser::name_id::SUBFAMILY) {
            Some(style) if style != "Regular" => format!("{family} {style}"),
            _ => family,
        })
    })
}

/// Font file bytes and the face index for a `TextStyle::font` value.
fn font_data(id: &str) -> Result<(std::borrow::Cow<'static, [u8]>, u32), String> {
    match id {
        "builtin:mono" => Ok((epaint_default_fonts::HACK_REGULAR.into(), 0)),
        "builtin:sans" | "" => Ok((epaint_default_fonts::UBUNTU_LIGHT.into(), 0)),
        _ => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let (path, index) = match id.rsplit_once('#') {
                    Some((p, i)) if i.parse::<u32>().is_ok() => (p, i.parse().unwrap_or(0)),
                    _ => (id, 0),
                };
                let bytes = std::fs::read(path).map_err(|e| format!("Couldn't open font {path}: {e}"))?;
                Ok((bytes.into(), index))
            }
            #[cfg(target_arch = "wasm32")]
            Ok((epaint_default_fonts::UBUNTU_LIGHT.into(), 0))
        }
    }
}

/// Draws the text centred vertically in a `w`×`h` transparent picture (straight RGBA).
pub fn render(style: &TextStyle, w: u32, h: u32) -> Result<image::RgbaImage, String> {
    let (bytes, index) = font_data(&style.font)?;
    let font = FontRef::try_from_slice_and_index(&bytes, index).map_err(|_| format!("{} isn't a font Phosphor can read", font_name(&style.font)))?;
    let (w, h) = (w.max(1), h.max(1));
    let px = (style.size.clamp(0.01, 2.0) * h as f32).max(4.0);
    let font = font.as_scaled(PxScale::from(px));
    let line_h = (font.ascent() - font.descent() + font.line_gap()) * style.line_spacing.clamp(0.5, 3.0);
    let lines: Vec<&str> = style.text.split('\n').collect();
    let width_of = |line: &str| {
        let mut x = 0.0;
        let mut prev = None;
        for c in line.chars() {
            let g = font.glyph_id(c);
            if let Some(p) = prev {
                x += font.kern(p, g);
            }
            x += font.h_advance(g);
            prev = Some(g);
        }
        x
    };
    let mut cover = vec![0f32; (w * h) as usize];
    let block = line_h * lines.len() as f32 - (line_h - (font.ascent() - font.descent()));
    let mut baseline = (h as f32 - block) / 2.0 + font.ascent();
    let margin = px * 0.25;
    for line in &lines {
        let lw = width_of(line);
        let mut x = match style.align {
            TextAlign::Left => margin,
            TextAlign::Center => (w as f32 - lw) / 2.0,
            TextAlign::Right => w as f32 - margin - lw,
        };
        let mut prev = None;
        for c in line.chars() {
            let id = font.glyph_id(c);
            if let Some(p) = prev {
                x += font.kern(p, id);
            }
            let glyph = id.with_scale_and_position(px, ab_glyph::point(x, baseline));
            if let Some(outlined) = font.outline_glyph(glyph) {
                let b = outlined.px_bounds();
                outlined.draw(|gx, gy, c| {
                    let (px_, py_) = (b.min.x as i32 + gx as i32, b.min.y as i32 + gy as i32);
                    if px_ >= 0 && py_ >= 0 && (px_ as u32) < w && (py_ as u32) < h {
                        let i = (py_ as u32 * w + px_ as u32) as usize;
                        cover[i] = (cover[i] + c).min(1.0);
                    }
                });
            }
            x += font.h_advance(id);
            prev = Some(id);
        }
        baseline += line_h;
    }
    let ring = if style.outline > 0.0 { dilate(&cover, w as usize, h as usize, (style.outline * px).round().max(1.0) as usize) } else { Vec::new() };
    let [r, g, b, a] = style.color;
    let mut img = image::RgbaImage::new(w, h);
    for (i, p) in img.pixels_mut().enumerate() {
        let fill = cover[i] * a as f32 / 255.0;
        let edge = ring.get(i).copied().unwrap_or(0.0) * (1.0 - fill);
        let alpha = fill + edge;
        if alpha <= 0.0 {
            continue;
        }
        // Text colour over a black outline.
        let mix = |c: u8| ((c as f32 * fill) / alpha).round().min(255.0) as u8;
        *p = image::Rgba([mix(r), mix(g), mix(b), (alpha * 255.0).round() as u8]);
    }
    Ok(img)
}

/// Grows coverage by `r` pixels (square max filter, separable: O(w·h·r) instead of O(w·h·r²)).
fn dilate(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> {
    let mut tmp = vec![0f32; src.len()];
    for y in 0..h {
        let row = &src[y * w..(y + 1) * w];
        for x in 0..w {
            let (a, b) = (x.saturating_sub(r), (x + r).min(w - 1));
            tmp[y * w + x] = row[a..=b].iter().copied().fold(0.0, f32::max);
        }
    }
    let mut out = vec![0f32; src.len()];
    for x in 0..w {
        for y in 0..h {
            let (a, b) = (y.saturating_sub(r), (y + r).min(h - 1));
            out[y * w + x] = (a..=b).map(|yy| tmp[yy * w + x]).fold(0.0, f32::max);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Text lands in the middle, in its colour, on a transparent picture; an outline adds dark edges.
    #[test]
    fn draws_centred_text_with_an_outline() {
        let style = TextStyle { text: "HELLO\nWORLD".into(), color: [255, 0, 0, 255], outline: 0.06, ..Default::default() };
        let img = render(&style, 640, 360).unwrap();
        assert_eq!(img.get_pixel(5, 5)[3], 0, "corners stay transparent");
        let lit: Vec<(u32, u32)> = img.enumerate_pixels().filter(|(_, _, p)| p[3] > 200 && p[0] > 200 && p[1] < 40).map(|(x, y, _)| (x, y)).collect();
        assert!(lit.len() > 500, "red text pixels: {}", lit.len());
        let cx = lit.iter().map(|p| p.0 as f64).sum::<f64>() / lit.len() as f64;
        let cy = lit.iter().map(|p| p.1 as f64).sum::<f64>() / lit.len() as f64;
        assert!((cx - 320.0).abs() < 40.0 && (cy - 180.0).abs() < 40.0, "centred: {cx},{cy}");
        let dark = img.pixels().filter(|p| p[3] > 200 && p[0] < 40).count();
        assert!(dark > 200, "outline pixels: {dark}");
    }

    #[test]
    fn installed_fonts_are_listed_and_render() {
        let fonts = fonts();
        assert!(fonts.len() >= 2);
        if let Some(f) = fonts.iter().find(|f| f.name.contains("Helvetica") || f.name.contains("Avenir")) {
            let style = TextStyle { font: f.id.clone(), ..Default::default() };
            let img = render(&style, 320, 180).unwrap();
            assert!(img.pixels().any(|p| p[3] > 200), "{} drew nothing", f.name);
        }
    }
}
