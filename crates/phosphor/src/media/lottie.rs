//! Lottie (After Effects via Bodymovin) animations as clips: velato reads and animates the file, and each frame is
//! drawn here with tiny-skia (the same CPU renderer as SVG pictures) into a cached movie (`baked`), which plays like
//! any other clip. Transparent where the animation draws nothing.

use std::collections::HashMap;

use kurbo::{Affine, PathEl, Shape};
use resvg::tiny_skia as sk;
use velato::model::{ImageAsset, fixed};

/// The movie is this many pixels on its longer side (sharp on most outputs, a modest cache).
const LONGEST: f64 = 1280.0;

/// A Lottie file: a .json whose top level has the animation's frame rate, frames and layers.
pub fn is_lottie(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("json")) && looks_like_lottie(path)
}

fn looks_like_lottie(path: &str) -> bool {
    use std::io::Read;
    let mut head = [0u8; 4096];
    let n = std::fs::File::open(path).and_then(|mut f| f.read(&mut head)).unwrap_or(0);
    let head = String::from_utf8_lossy(&head[..n]);
    ["\"fr\"", "\"ip\"", "\"op\"", "\"layers\""].iter().filter(|k| head.contains(*k)).count() >= 2
}

fn transform(t: Affine) -> sk::Transform {
    let [a, b, c, d, e, f] = t.as_coeffs().map(|v| v as f32);
    sk::Transform::from_row(a, b, c, d, e, f)
}

fn path(shape: &impl Shape) -> Option<sk::Path> {
    let mut pb = sk::PathBuilder::new();
    let p = |p: kurbo::Point| (p.x as f32, p.y as f32);
    for el in shape.path_elements(0.1) {
        match el {
            PathEl::MoveTo(a) => pb.move_to(p(a).0, p(a).1),
            PathEl::LineTo(a) => pb.line_to(p(a).0, p(a).1),
            PathEl::QuadTo(a, b) => pb.quad_to(p(a).0, p(a).1, p(b).0, p(b).1),
            PathEl::CurveTo(a, b, c) => pb.cubic_to(p(a).0, p(a).1, p(b).0, p(b).1, p(c).0, p(c).1),
            PathEl::ClosePath => pb.close(),
        }
    }
    pb.finish()
}

fn colour(c: peniko::color::AlphaColor<peniko::color::Srgb>) -> sk::Color {
    let c = c.to_rgba8();
    sk::Color::from_rgba8(c.r, c.g, c.b, c.a)
}

fn paint(brush: &fixed::Brush) -> Option<sk::Paint<'static>> {
    let mut paint = sk::Paint { anti_alias: true, ..Default::default() };
    match brush {
        peniko::Brush::Solid(c) => paint.set_color(colour(*c)),
        peniko::Brush::Gradient(g) => {
            let stops: Vec<sk::GradientStop> = g.stops.iter().map(|s| sk::GradientStop::new(s.offset, colour(s.color.to_alpha_color()))).collect();
            let mode = match g.extend {
                peniko::Extend::Pad => sk::SpreadMode::Pad,
                peniko::Extend::Repeat => sk::SpreadMode::Repeat,
                peniko::Extend::Reflect => sk::SpreadMode::Reflect,
            };
            let pt = |p: kurbo::Point| sk::Point::from_xy(p.x as f32, p.y as f32);
            let first = stops.first().map(|_| colour(g.stops[0].color.to_alpha_color()));
            let shader = match g.kind {
                peniko::GradientKind::Linear(l) => sk::LinearGradient::new(pt(l.start), pt(l.end), stops, mode, sk::Transform::identity()),
                peniko::GradientKind::Radial(r) => sk::RadialGradient::new(pt(r.start_center), pt(r.end_center), r.end_radius, stops, mode, sk::Transform::identity()),
                // tiny-skia has no sweep gradient: its first colour.
                peniko::GradientKind::Sweep(_) => None,
            };
            match (shader, first) {
                (Some(s), _) => paint.shader = s,
                (None, Some(c)) => paint.set_color(c),
                (None, None) => return None,
            }
        }
        peniko::Brush::Image(_) => return None,
    }
    Some(paint)
}

fn stroke(s: &fixed::Stroke) -> sk::Stroke {
    let cap = |c: kurbo::Cap| match c {
        kurbo::Cap::Butt => sk::LineCap::Butt,
        kurbo::Cap::Square => sk::LineCap::Square,
        kurbo::Cap::Round => sk::LineCap::Round,
    };
    let join = match s.join {
        kurbo::Join::Bevel => sk::LineJoin::Bevel,
        kurbo::Join::Miter => sk::LineJoin::Miter,
        kurbo::Join::Round => sk::LineJoin::Round,
    };
    let dash = (!s.dash_pattern.is_empty()).then(|| sk::StrokeDash::new(s.dash_pattern.iter().map(|d| *d as f32).collect(), s.dash_offset as f32)).flatten();
    sk::Stroke { width: s.width as f32, miter_limit: s.miter_limit as f32, line_cap: cap(s.start_cap), line_join: join, dash }
}

fn blend(mode: peniko::BlendMode) -> sk::BlendMode {
    use peniko::{Compose, Mix};
    match mode.compose {
        Compose::SrcOver => match mode.mix {
            Mix::Multiply => sk::BlendMode::Multiply,
            Mix::Screen => sk::BlendMode::Screen,
            Mix::Overlay => sk::BlendMode::Overlay,
            Mix::Darken => sk::BlendMode::Darken,
            Mix::Lighten => sk::BlendMode::Lighten,
            Mix::ColorDodge => sk::BlendMode::ColorDodge,
            Mix::ColorBurn => sk::BlendMode::ColorBurn,
            Mix::HardLight => sk::BlendMode::HardLight,
            Mix::SoftLight => sk::BlendMode::SoftLight,
            Mix::Difference => sk::BlendMode::Difference,
            Mix::Exclusion => sk::BlendMode::Exclusion,
            Mix::Hue => sk::BlendMode::Hue,
            Mix::Saturation => sk::BlendMode::Saturation,
            Mix::Color => sk::BlendMode::Color,
            Mix::Luminosity => sk::BlendMode::Luminosity,
            _ => sk::BlendMode::SourceOver,
        },
        Compose::Clear => sk::BlendMode::Clear,
        Compose::Copy => sk::BlendMode::Source,
        Compose::Dest => sk::BlendMode::Destination,
        Compose::DestOver => sk::BlendMode::DestinationOver,
        Compose::SrcIn => sk::BlendMode::SourceIn,
        Compose::DestIn => sk::BlendMode::DestinationIn,
        Compose::SrcOut => sk::BlendMode::SourceOut,
        Compose::DestOut => sk::BlendMode::DestinationOut,
        Compose::SrcAtop => sk::BlendMode::SourceAtop,
        Compose::DestAtop => sk::BlendMode::DestinationAtop,
        Compose::Xor => sk::BlendMode::Xor,
        _ => sk::BlendMode::Plus,
    }
}

/// One group being drawn: composited onto the one below when it's popped (opacity, blend, clipped to its shape).
struct Layer {
    pixmap: sk::Pixmap,
    alpha: f32,
    blend: sk::BlendMode,
    clip: Option<sk::Mask>,
}

/// velato's drawing commands onto tiny-skia pixmaps.
struct Sink {
    size: (u32, u32),
    stack: Vec<Layer>,
    /// The file's folder (images it names sit next to it), and the images it uses, decoded once.
    folder: std::path::PathBuf,
    images: HashMap<String, Option<sk::Pixmap>>,
}

impl Sink {
    fn new(size: (u32, u32), folder: std::path::PathBuf) -> Option<Self> {
        let base = Layer { pixmap: sk::Pixmap::new(size.0, size.1)?, alpha: 1.0, blend: sk::BlendMode::SourceOver, clip: None };
        Some(Self { size, stack: vec![base], folder, images: HashMap::new() })
    }

    fn top(&mut self) -> &mut sk::Pixmap {
        &mut self.stack.last_mut().expect("the base layer stays").pixmap
    }

    fn mask(&self, t: Affine, shape: &impl Shape) -> Option<sk::Mask> {
        let mut m = sk::Mask::new(self.size.0, self.size.1)?;
        m.fill_path(&path(shape)?, sk::FillRule::Winding, true, transform(t));
        Some(m)
    }

    fn push(&mut self, blend: sk::BlendMode, alpha: f32, t: Affine, shape: &impl Shape) {
        // A shape that can't be made into a mask (empty) hides everything drawn in the group.
        let clip = self.mask(t, shape).or_else(|| sk::Mask::new(self.size.0, self.size.1));
        if let Some(pixmap) = sk::Pixmap::new(self.size.0, self.size.1) {
            self.stack.push(Layer { pixmap, alpha, blend, clip });
        }
    }

    /// The finished frame, straight alpha.
    fn into_rgba(mut self) -> Vec<u8> {
        while self.stack.len() > 1 {
            self.pop_layer_inner();
        }
        self.stack[0].pixmap.pixels().iter().flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        }).collect()
    }

    fn pop_layer_inner(&mut self) {
        if self.stack.len() < 2 {
            return;
        }
        let top = self.stack.pop().expect("checked");
        let paint = sk::PixmapPaint { opacity: top.alpha.clamp(0.0, 1.0), blend_mode: top.blend, quality: sk::FilterQuality::Nearest };
        self.top().draw_pixmap(0, 0, top.pixmap.as_ref(), &paint, sk::Transform::identity(), top.clip.as_ref());
    }

    fn image(&mut self, asset: &ImageAsset) -> Option<&sk::Pixmap> {
        if !self.images.contains_key(&asset.id) {
            let bytes = if asset.is_data_url() {
                use base64::Engine;
                asset.file_name.split_once(',').and_then(|(_, b)| base64::engine::general_purpose::STANDARD.decode(b.trim()).ok())
            } else {
                std::fs::read(self.folder.join(asset.location())).ok()
            };
            let pixmap = bytes.and_then(|b| image::load_from_memory(&b).ok()).and_then(|img| {
                let img = img.to_rgba8();
                let mut pm = sk::Pixmap::new(img.width(), img.height())?;
                for (d, s) in pm.pixels_mut().iter_mut().zip(img.pixels()) {
                    *d = sk::ColorU8::from_rgba(s[0], s[1], s[2], s[3]).premultiply();
                }
                Some(pm)
            });
            self.images.insert(asset.id.clone(), pixmap);
        }
        self.images.get(&asset.id).and_then(|p| p.as_ref())
    }
}

impl velato::RenderSink for Sink {
    fn push_layer(&mut self, mode: impl Into<peniko::BlendMode>, alpha: f32, t: Affine, shape: &impl Shape) {
        self.push(blend(mode.into()), alpha, t, shape);
    }

    fn push_clip_layer(&mut self, t: Affine, shape: &impl Shape) {
        self.push(sk::BlendMode::SourceOver, 1.0, t, shape);
    }

    fn pop_layer(&mut self) {
        self.pop_layer_inner();
    }

    fn draw(&mut self, s: Option<&fixed::Stroke>, t: Affine, brush: &fixed::Brush, shape: &impl Shape) {
        let (Some(p), Some(paint)) = (path(shape), paint(brush)) else { return };
        match s {
            Some(s) => self.top().stroke_path(&p, &paint, &stroke(s), transform(t), None),
            None => self.top().fill_path(&p, &paint, sk::FillRule::Winding, transform(t), None),
        }
    }

    fn draw_image(&mut self, asset: &ImageAsset, mut t: Affine, alpha: f64) {
        let Some(img) = self.image(asset).cloned() else { return };
        // Fitted into the size the animation gave it (as velato's own renderer does).
        if let (Some(w), Some(h)) = (asset.width.filter(|w| *w > 0.0), asset.height.filter(|h| *h > 0.0)) {
            let (iw, ih) = (img.width() as f64, img.height() as f64);
            let scale = (w / iw).min(h / ih);
            t *= Affine::translate(((w - iw * scale) * 0.5, (h - ih * scale) * 0.5)) * Affine::scale(scale);
        }
        let paint = sk::PixmapPaint { opacity: alpha.clamp(0.0, 1.0) as f32, blend_mode: sk::BlendMode::SourceOver, quality: sk::FilterQuality::Bilinear };
        self.top().draw_pixmap(0, 0, img.as_ref(), &paint, transform(t), None);
    }
}

/// The movie's size for an animation `w` × `h`: `LONGEST` on the longer side, rounded to whole 4-pixel blocks
/// (Hap's).
fn movie_size(w: usize, h: usize) -> (u32, u32) {
    let scale = LONGEST / w.max(h).max(1) as f64;
    let side = |v: usize| (((v as f64 * scale) / 4.0).round().max(1.0) * 4.0) as u32;
    (side(w), side(h))
}

/// Every frame of a Lottie animation as pictures, and its frame rate.
pub fn drawing(data: &[u8], folder: std::path::PathBuf) -> Result<super::baked::Drawing, String> {
    let comp = velato::Composition::from_slice(data).map_err(|e| format!("not a Lottie animation ({e})"))?;
    let (w, h) = movie_size(comp.width, comp.height);
    let fps = comp.frame_rate.round().clamp(1.0, 120.0) as u32;
    let (start, end) = (comp.frames.start, comp.frames.end);
    let count = ((end - start).max(1.0).ceil() as usize).min(36_000);
    let sx = w as f64 / comp.width.max(1) as f64;
    let sy = h as f64 / comp.height.max(1) as f64;
    let mut renderer = velato::Renderer::new();
    let frames = (0..count).filter_map(move |i| {
        let mut sink = Sink::new((w, h), folder.clone())?;
        renderer.append(&comp, start + i as f64, Affine::scale_non_uniform(sx, sy), 1.0, &mut sink);
        Some(sink.into_rgba())
    });
    Ok(super::baked::Drawing { size: (w, h), fps, frames: Box::new(frames) })
}

/// The movie a Lottie file plays as (made the first time, then reused). Call off the UI thread.
pub fn movie_for(path: &str) -> Result<String, String> {
    let folder = std::path::Path::new(path).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    super::baked::cached_movie(path, "lottie", |data| drawing(data, folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 100 × 100, 10-frame animation at 10 fps: a red square moving from the left to the right, a blue circle
    /// fading in.
    pub const SAMPLE: &str = r#"{"v":"5.7.0","fr":10,"ip":0,"op":10,"w":100,"h":100,"layers":[
      {"ty":4,"ind":1,"ip":0,"op":10,"st":0,"ks":{"o":{"a":0,"k":100},"p":{"a":1,"k":[{"t":0,"s":[20,30,0],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},{"t":9,"s":[80,30,0]}]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]},"r":{"a":0,"k":0}},
       "shapes":[{"ty":"gr","it":[{"ty":"rc","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[20,20]},"r":{"a":0,"k":0}},{"ty":"fl","c":{"a":0,"k":[1,0,0,1]},"o":{"a":0,"k":100}},{"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}]}]},
      {"ty":4,"ind":2,"ip":0,"op":10,"st":0,"ks":{"o":{"a":1,"k":[{"t":0,"s":[0],"o":{"x":[0],"y":[0]},"i":{"x":[1],"y":[1]}},{"t":9,"s":[100]}]},"p":{"a":0,"k":[50,75,0]},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]},"r":{"a":0,"k":0}},
       "shapes":[{"ty":"gr","it":[{"ty":"el","p":{"a":0,"k":[0,0]},"s":{"a":0,"k":[30,30]}},{"ty":"fl","c":{"a":0,"k":[0,0,1,1]},"o":{"a":0,"k":100}},{"ty":"tr","p":{"a":0,"k":[0,0]},"a":{"a":0,"k":[0,0]},"s":{"a":0,"k":[100,100]},"r":{"a":0,"k":0},"o":{"a":0,"k":100}}]}]}
    ]}"#;

    #[test]
    fn a_lottie_animation_draws_its_frames() {
        let d = drawing(SAMPLE.as_bytes(), std::env::temp_dir()).unwrap();
        assert_eq!((d.size, d.fps), ((1280, 1280), 10));
        let frames: Vec<Vec<u8>> = d.frames.collect();
        assert_eq!(frames.len(), 10);
        let (w, s) = (1280usize, 12.8);
        let at = |f: &Vec<u8>, x: f64, y: f64| {
            let i = ((y * s) as usize * w + (x * s) as usize) * 4;
            [f[i], f[i + 1], f[i + 2], f[i + 3]]
        };
        // The square starts at the left and ends at the right.
        assert_eq!(at(&frames[0], 20.0, 30.0), [255, 0, 0, 255]);
        assert_eq!(at(&frames[0], 80.0, 30.0)[3], 0);
        assert_eq!(at(&frames[9], 80.0, 30.0), [255, 0, 0, 255]);
        assert_eq!(at(&frames[9], 20.0, 30.0)[3], 0);
        // The circle fades in.
        assert!(at(&frames[0], 50.0, 75.0)[3] < 10);
        assert_eq!(at(&frames[9], 50.0, 75.0), [0, 0, 255, 255]);
        assert!((100..160).contains(&at(&frames[5], 50.0, 75.0)[3]), "{:?}", at(&frames[5], 50.0, 75.0));
    }

    #[test]
    fn only_lottie_json_counts() {
        let dir = std::env::temp_dir().join(format!("phosphor-lottie-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (anim, other) = (dir.join("a.json"), dir.join("b.json"));
        std::fs::write(&anim, SAMPLE).unwrap();
        std::fs::write(&other, r#"{"name":"settings","volume":3}"#).unwrap();
        let ok = (is_lottie(&anim.to_string_lossy()), is_lottie(&other.to_string_lossy()));
        assert!(drawing(b"{\"nope\":1}", dir.clone()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(ok, (true, false));
    }
}
