//! Morpholio Trace drawings (.morpholiotrace, the iPad sketching app): a zip of `data-books.plist` (the drawing's
//! size and its layers, top first: title, hidden, opacity, a 4×4 placement, the token of the layer's picture), one
//! picture per layer (`<token>_original.png`, a big square, stored bottom row first) and a SQLite file of pen strokes
//! (not read yet). Opening one makes a folder with each layer drawn at the drawing's size (placed as Trace placed
//! it) and a project with a layer per drawing layer: name, on / off and opacity kept, top layer on top.

use std::io::Read;
use std::path::{Path, PathBuf};

use crate::job::{self, Shared};
use phosphor_engine::Composition;

pub const EXT: &str = "morpholiotrace";

pub fn is_trace(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(EXT))
}

/// One drawing layer.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceLayer {
    pub title: String,
    pub hidden: bool,
    pub opacity: f32,
    /// Where the picture goes on the drawing (points; y up): x' = a x + c y + tx, y' = b x + d y + ty.
    pub place: [f32; 6],
    /// The layer's own size in points (its picture is this many points across, at a few pixels each).
    pub size: Option<(f32, f32)>,
    /// The drawn picture's token, and a photo under it (a backing), if any.
    pub drawing: Option<String>,
    pub backing: Option<String>,
}

/// A drawing: its size (points) and layers, top first.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceBook {
    pub title: String,
    pub width: f32,
    pub height: f32,
    pub layers: Vec<TraceLayer>,
}

/// "{960, 540}" → (960, 540).
fn size_of(text: &str) -> Option<(f32, f32)> {
    let mut parts = text.trim().trim_start_matches('{').trim_end_matches('}').split(',').map(|s| s.trim().parse::<f32>().ok());
    let (w, h) = (parts.next()??, parts.next()??);
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then_some((w, h))
}

/// Reads `data-books.plist` (the first drawing in it).
pub fn parse_books(bytes: &[u8]) -> Result<TraceBook, String> {
    let top = plist::Value::from_reader(std::io::Cursor::new(bytes)).map_err(|e| format!("not a Trace drawing ({e})"))?;
    let book = top.as_dictionary().and_then(|d| d.values().find_map(plist::Value::as_dictionary)).ok_or("not a Trace drawing (no drawing in it)")?;
    let text = |d: &plist::Dictionary, k: &str| d.get(k).and_then(plist::Value::as_string).map(str::to_string);
    let real = |d: &plist::Dictionary, k: &str| d.get(k).and_then(|v| v.as_real().or_else(|| v.as_signed_integer().map(|i| i as f64))).map(|v| v as f32);
    let (width, height) = text(book, "size").as_deref().and_then(size_of).unwrap_or((1024.0, 768.0));
    let pages = book.get("page_data").and_then(plist::Value::as_dictionary).ok_or("not a Trace drawing (no layers)")?;
    let mut keyed: Vec<(usize, &plist::Dictionary)> = pages.iter().filter_map(|(k, v)| Some((k.parse().ok()?, v.as_dictionary()?))).collect();
    keyed.sort_by_key(|(k, _)| *k);
    let layers = keyed
        .into_iter()
        .take(super::MAX_LAYERS)
        .map(|(i, p)| {
            let m: Vec<f32> = p.get("transform").and_then(plist::Value::as_array).map(|a| a.iter().map(|v| v.as_real().unwrap_or(0.0) as f32).collect()).unwrap_or_default();
            let place = if m.len() == 16 && m.iter().all(|v| v.is_finite()) { [m[0], m[1], m[4], m[5], m[12], m[13]] } else { [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] };
            TraceLayer {
                title: text(p, "title").filter(|t| !t.trim().is_empty()).unwrap_or_else(|| format!("Layer {}", i + 1)),
                hidden: p.get("hidden").and_then(plist::Value::as_boolean).unwrap_or(false),
                opacity: real(p, "drawingOpacity").unwrap_or(1.0).clamp(0.0, 1.0),
                place,
                size: text(p, "size").as_deref().and_then(size_of),
                drawing: text(p, "drawingToken").filter(|t| token_ok(t)),
                backing: text(p, "backingToken").filter(|t| token_ok(t)),
            }
        })
        .collect();
    Ok(TraceBook { title: text(book, "title").unwrap_or_default(), width, height, layers })
}

/// A token is a short run of letters and digits (it names a file in the zip).
fn token_ok(t: &str) -> bool {
    !t.is_empty() && t.len() <= 64 && t.chars().all(|c| c.is_ascii_alphanumeric())
}

/// Pixels per point of the pictures made: the long side ~1920 (a Trace drawing is a screen's worth of points).
fn scale_for(book: &TraceBook) -> f32 {
    (1920.0 / book.width.max(book.height)).clamp(0.25, 8.0)
}

/// A picture as premultiplied RGBA bytes, with its size.
struct Picture {
    w: u32,
    h: u32,
    px: Vec<u8>,
}

impl Picture {
    fn from_png(bytes: &[u8]) -> Result<Self, String> {
        let img = image::load_from_memory(bytes).map_err(|e| e.to_string())?.into_rgba8();
        let (w, h) = img.dimensions();
        let mut px = img.into_raw();
        for p in px.chunks_exact_mut(4) {
            let a = p[3] as u32;
            for c in &mut p[..3] {
                *c = ((*c as u32 * a + 127) / 255) as u8;
            }
        }
        Ok(Self { w, h, px })
    }

    /// Made smaller by `k` (< 1) first, so drawing it smaller later doesn't skip pixels.
    fn shrunk(self, k: f32) -> Self {
        let (w, h) = (((self.w as f32 * k).round() as u32).max(1), ((self.h as f32 * k).round() as u32).max(1));
        let Some(img) = image::RgbaImage::from_raw(self.w, self.h, self.px) else { return Self { w: 1, h: 1, px: vec![0; 4] } };
        let small = image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle);
        Self { w, h, px: small.into_raw() }
    }

    /// The (premultiplied) color at a point in pixels, blended between the four nearest; clear outside.
    fn sample(&self, x: f32, y: f32) -> [f32; 4] {
        let (x, y) = (x - 0.5, y - 0.5);
        let (x0, y0) = (x.floor(), y.floor());
        let (fx, fy) = (x - x0, y - y0);
        let mut out = [0.0f32; 4];
        for (dx, dy, wgt) in [(0.0, 0.0, (1.0 - fx) * (1.0 - fy)), (1.0, 0.0, fx * (1.0 - fy)), (0.0, 1.0, (1.0 - fx) * fy), (1.0, 1.0, fx * fy)] {
            let (px, py) = (x0 + dx, y0 + dy);
            if wgt <= 0.0 || px < 0.0 || py < 0.0 || px >= self.w as f32 || py >= self.h as f32 {
                continue;
            }
            let i = ((py as usize) * self.w as usize + px as usize) * 4;
            for c in 0..4 {
                out[c] += self.px[i + c] as f32 * wgt;
            }
        }
        out
    }
}

/// Draws `pic` over `out` (premultiplied floats, `w`×`h`, `scale` pixels per point, rows top-down) where `place`
/// puts it: the picture is `per_point` pixels per point. `bottom_up` = the picture's first row is its bottom (Trace's
/// drawings), else its top (photos).
fn draw(out: &mut [[f32; 4]], w: u32, h: u32, scale: f32, book_h: f32, pic: Picture, per_point: f32, place: [f32; 6], bottom_up: bool) {
    let [a, b, c, d, tx, ty] = place;
    let det = a * d - b * c;
    if det.abs() < 1e-6 || per_point <= 0.0 {
        return;
    }
    // Output pixels per picture pixel; well under 1 = shrink the picture first.
    let k = scale * det.abs().sqrt() / per_point;
    let (pic, per_point) = if k < 0.75 { (pic.shrunk(k), per_point * k) } else { (pic, per_point) };
    for oy in 0..h {
        // The drawing's point, y measured up from the bottom for drawings (as Trace keeps them), down for photos.
        let yp = (oy as f32 + 0.5) / scale;
        let y = if bottom_up { book_h - yp } else { yp };
        for ox in 0..w {
            let x = (ox as f32 + 0.5) / scale;
            let (dx, dy) = (x - tx, y - ty);
            let (px, py) = ((d * dx - c * dy) / det, (-b * dx + a * dy) / det);
            let s = pic.sample(px * per_point, py * per_point);
            if s[3] <= 0.0 {
                continue;
            }
            let o = &mut out[(oy * w + ox) as usize];
            let keep = 1.0 - s[3] / 255.0;
            for i in 0..4 {
                o[i] = s[i] + o[i] * keep;
            }
        }
    }
}

/// Premultiplied floats → straight RGBA bytes.
fn to_rgba(px: &[[f32; 4]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(px.len() * 4);
    for p in px {
        let a = p[3].clamp(0.0, 255.0);
        let un = |c: f32| if a > 0.0 { (c * 255.0 / a).round().clamp(0.0, 255.0) as u8 } else { 0 };
        out.extend_from_slice(&[un(p[0]), un(p[1]), un(p[2]), a.round() as u8]);
    }
    out
}

/// One layer drawn at the drawing's size (`size` = width, height, pixels per point): its backing photo, then its
/// drawing over it. Straight RGBA.
fn draw_layer(book: &TraceBook, layer: &TraceLayer, pngs: &(Option<Vec<u8>>, Option<Vec<u8>>), (w, h, scale): (u32, u32, f32)) -> Result<Vec<u8>, String> {
    let mut out = vec![[0.0f32; 4]; (w * h) as usize];
    if let Some(png) = &pngs.0 {
        let pic = Picture::from_png(png).map_err(|e| format!("{}: {e}", layer.title))?;
        let per_point = pic.w as f32 / book.width;
        draw(&mut out, w, h, scale, book.height, pic, per_point, layer.place, false);
    }
    if let Some(png) = &pngs.1 {
        let pic = Picture::from_png(png).map_err(|e| format!("{}: {e}", layer.title))?;
        let per_point = pic.w as f32 / layer.size.map_or(book.width, |s| s.0);
        draw(&mut out, w, h, scale, book.height, pic, per_point, layer.place, true);
    }
    Ok(to_rgba(&out))
}

/// A file from the zip, at most `limit` bytes.
fn read_entry(zip: &mut zip::ZipArchive<std::io::BufReader<std::fs::File>>, name: &str, limit: u64) -> Option<Vec<u8>> {
    let entry = zip.by_name(name).ok()?;
    let mut bytes = Vec::new();
    entry.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

/// Turns the Trace drawing `file` into a folder `into` (made; it should be new or empty): a picture per layer and
/// "<name>.phosphor". `output` = the project file to open.
pub fn convert(file: &Path, into: &Path, p: &Shared) -> Result<PathBuf, String> {
    let fresh = !into.exists();
    let result = convert_into(file, into, p);
    // A canceled or failed one takes away the folder it made (never one that was there before).
    if result.is_err() && fresh {
        let _ = std::fs::remove_dir_all(into);
    }
    result
}

fn convert_into(file: &Path, into: &Path, p: &Shared) -> Result<PathBuf, String> {
    const PICTURE_LIMIT: u64 = 256 << 20;
    job::step(p, "Reading the drawing…");
    let f = std::fs::File::open(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(f)).map_err(|e| format!("Not a Trace drawing ({e})"))?;
    let books = read_entry(&mut zip, "data-books.plist", 64 << 20).ok_or("Not a Trace drawing: there's no layer list in it")?;
    let book = parse_books(&books)?;
    std::fs::create_dir_all(into).map_err(|e| format!("{}: {e}", into.display()))?;
    let scale = scale_for(&book);
    let (w, h) = (super::side(book.width * scale), super::side(book.height * scale));
    let name = crate::pack::file_name(&file.file_stem().map_or_else(|| "Drawing".into(), |s| s.to_string_lossy().into_owned()));
    // The pictures' files, read first (small: the zip can't be read from several threads); then drawn a few at a
    // time (each takes a big picture's memory while it works).
    let pngs: Vec<(Option<Vec<u8>>, Option<Vec<u8>>)> = book
        .layers
        .iter()
        .map(|l| {
            let read = |t: &Option<String>, zip: &mut _| t.as_ref().and_then(|t| read_entry(zip, &format!("{t}_original.png"), PICTURE_LIMIT));
            (read(&l.backing, &mut zip), read(&l.drawing, &mut zip))
        })
        .collect();
    // (No byte total: the window shows the step, "Drew 3 of 19 layers", and a spinner.)
    let count = book.layers.len();
    let next = std::sync::atomic::AtomicUsize::new(0);
    let made: Vec<std::sync::Mutex<Option<Result<PathBuf, String>>>> = (0..count).map(|_| Default::default()).collect();
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get().saturating_sub(1)).clamp(1, 3);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| {
                loop {
                    let n = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if n >= count || job::canceled(p) {
                        break;
                    }
                    let layer = &book.layers[n];
                    let result = draw_layer(&book, layer, &pngs[n], (w, h, scale)).and_then(|rgba| {
                        // Numbered bottom first, as the layers stack.
                        let picture = into.join(format!("{:02} {}.png", count - n, crate::pack::file_name(&layer.title)));
                        image::save_buffer(&picture, &rgba, w, h, image::ExtendedColorType::Rgba8).map_err(|e| format!("{}: {e}", picture.display()))?;
                        Ok(picture)
                    });
                    *made[n].lock().unwrap() = Some(result);
                    let done = made.iter().filter(|m| m.lock().unwrap().is_some()).count();
                    job::step(p, format!("Drew {done} of {count} layers"));
                }
            });
        }
    });
    if job::canceled(p) {
        return Err(job::CANCELED.into());
    }
    let mut comp = Composition { width: w, height: h, ..Default::default() };
    // Bottom layer first: Phosphor stacks later layers on top.
    for (layer, made) in book.layers.iter().zip(made).rev() {
        let picture = made.into_inner().unwrap().unwrap_or_else(|| Err(job::CANCELED.into()))?;
        let path = picture.to_string_lossy().into_owned();
        let l = comp.add_layer();
        l.blend = phosphor_engine::BlendMode::Normal;
        l.name = layer.title.clone();
        l.enabled = !layer.hidden;
        l.opacity = layer.opacity;
        l.source = crate::media::spec_for_path(&path);
    }
    job::step(p, "Opening the project…");
    let project = crate::pack::free_path(into, &name, "phosphor");
    crate::project_files::write_whole(&project, comp.to_json()).map_err(|e| e.to_string())?;
    p.lock().unwrap().output = Some(project.clone());
    Ok(project)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn books_plist() -> Vec<u8> {
        let place = |s: f64, tx: f64, ty: f64| plist::Value::Array([s, 0.0, 0.0, 0.0, 0.0, s, 0.0, 0.0, 0.0, 0.0, s, 0.0, tx, ty, 0.0, 1.0].into_iter().map(plist::Value::Real).collect());
        let mut top_layer = plist::Dictionary::new();
        top_layer.insert("title".into(), "Linework".into());
        top_layer.insert("hidden".into(), false.into());
        top_layer.insert("drawingOpacity".into(), plist::Value::Real(0.5));
        top_layer.insert("drawingToken".into(), "iAbc123".into());
        top_layer.insert("size".into(), "{4, 4}".into());
        top_layer.insert("transform".into(), place(1.0, 0.0, 0.0));
        let mut back = plist::Dictionary::new();
        back.insert("title".into(), "Background".into());
        back.insert("hidden".into(), true.into());
        back.insert("drawingToken".into(), "../evil".into());
        back.insert("transform".into(), place(2.0, -1.0, 3.0));
        let mut pages = plist::Dictionary::new();
        pages.insert("1".into(), back.into());
        pages.insert("0".into(), top_layer.into());
        let mut book = plist::Dictionary::new();
        book.insert("size".into(), "{4, 2}".into());
        book.insert("title".into(), "Flyer".into());
        book.insert("page_data".into(), pages.into());
        let mut root = plist::Dictionary::new();
        root.insert("sTok".into(), book.into());
        let mut out = Vec::new();
        plist::Value::Dictionary(root).to_writer_binary(&mut out).unwrap();
        out
    }

    #[test]
    fn a_drawings_layers_are_read_top_first() {
        let book = parse_books(&books_plist()).unwrap();
        assert_eq!((book.title.as_str(), book.width, book.height), ("Flyer", 4.0, 2.0));
        assert_eq!(book.layers.len(), 2);
        let top = &book.layers[0];
        assert_eq!((top.title.as_str(), top.hidden, top.opacity, top.drawing.as_deref(), top.size), ("Linework", false, 0.5, Some("iAbc123"), Some((4.0, 4.0))));
        let bottom = &book.layers[1];
        assert!(bottom.hidden);
        assert_eq!(bottom.opacity, 1.0, "no opacity = fully drawn");
        assert_eq!(bottom.drawing, None, "a token that isn't letters and digits names no file");
        assert_eq!(bottom.place, [2.0, 0.0, 0.0, 2.0, -1.0, 3.0]);
        assert!(parse_books(b"not a plist").is_err());
    }

    #[test]
    fn sizes_read_and_bad_ones_dont() {
        assert_eq!(size_of("{960, 540}"), Some((960.0, 540.0)));
        assert_eq!(size_of("{0, 540}"), None);
        assert_eq!(size_of("nonsense"), None);
    }

    #[test]
    fn a_drawing_stored_bottom_row_first_comes_out_right_way_up() {
        // A 2×2-point drawing, 1 pixel per point: the picture's first row (its bottom) red, second row (top) blue.
        let pic = Picture { w: 2, h: 2, px: vec![255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255] };
        let mut out = vec![[0.0f32; 4]; 4];
        draw(&mut out, 2, 2, 1.0, 2.0, pic, 1.0, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], true);
        let rgba = to_rgba(&out);
        assert_eq!(&rgba[0..4], &[0, 0, 255, 255], "top row is the picture's top (blue)");
        assert_eq!(&rgba[8..12], &[255, 0, 0, 255], "bottom row red");
    }

    #[test]
    fn a_layer_moved_and_scaled_lands_where_trace_put_it() {
        // A white 2×2-point picture (4 pixels a point) moved 2 points right on a 4×2-point drawing drawn 2 pixels a
        // point: it covers the right half.
        let pic = Picture { w: 8, h: 8, px: vec![255; 8 * 8 * 4] };
        let mut out = vec![[0.0f32; 4]; 8 * 4];
        draw(&mut out, 8, 4, 2.0, 2.0, pic, 4.0, [1.0, 0.0, 0.0, 1.0, 2.0, 0.0], true);
        let alpha: Vec<u8> = to_rgba(&out).chunks(4).map(|p| p[3]).collect();
        for row in alpha.chunks(8) {
            assert_eq!(row, [0, 0, 0, 0, 255, 255, 255, 255]);
        }
    }

    #[test]
    fn opening_one_makes_a_project_with_a_layer_per_drawing_layer() {
        let dir = std::env::temp_dir().join(format!("phosphor-trace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Flyer.morpholiotrace");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&file).unwrap());
            let opts = zip::write::SimpleFileOptions::default();
            zip.start_file("data-books.plist", opts).unwrap();
            std::io::Write::write_all(&mut zip, &books_plist()).unwrap();
            let mut png = Vec::new();
            image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 255, 0, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
            zip.start_file("iAbc123_original.png", opts).unwrap();
            std::io::Write::write_all(&mut zip, &png).unwrap();
            zip.finish().unwrap();
        }
        let p: Shared = Default::default();
        let project = convert(&file, &dir.join("Flyer"), &p).unwrap();
        let comp = Composition::from_json(&std::fs::read_to_string(&project).unwrap()).unwrap();
        let names: Vec<&str> = comp.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Background", "Linework"], "Trace's top layer ends up on top");
        assert!(!comp.layers[0].enabled && comp.layers[1].enabled);
        assert_eq!(comp.layers[1].opacity, 0.5);
        assert_eq!((comp.width, comp.height), (32, 16), "the drawing's shape, at most 8 pixels a point");
        assert!(comp.media_paths().iter().all(|m| Path::new(m).exists()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
