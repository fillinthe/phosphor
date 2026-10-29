//! Laser shows in ILDA files (.ild, the laser world's interchange format) as clips: each frame's points become
//! glowing lines, drawn once into a cached Hap Alpha movie, which then plays like any other clip (loop, speed,
//! scrub, cues). Formats 0, 1 (indexed colour, 3D / 2D), 2 (a palette), 4 and 5 (true colour, 3D / 2D).

use resvg::tiny_skia;

/// Frames a second (ILDA files don't say; 30 is what laser software plays them at by default).
pub const FPS: u32 = 30;
/// The movie's size: square, as a laser's scan area is.
pub const SIZE: u32 = 1024;

pub use phosphor_engine::ilda::{Point, default_palette, parse};

pub fn is_ilda(path: &str) -> bool {
    path.rsplit_once('.').is_some_and(|(_, e)| e.eq_ignore_ascii_case("ild"))
}

/// One frame as a straight-alpha RGBA picture `size` square: lines between lit points with a soft glow, a dot for
/// a point lit on its own, transparent elsewhere.
pub fn draw(points: &[Point], size: u32) -> Vec<u8> {
    let Some(mut pixmap) = tiny_skia::Pixmap::new(size, size) else { return Vec::new() };
    let s = size as f32;
    let at = |p: &Point| (((p.x + 1.0) * 0.5) * s, ((1.0 - p.y) * 0.5) * s);
    let width = (s / 400.0).max(1.5);
    for (glow, alpha) in [(4.0, 0.22), (1.0, 1.0)] {
        for (i, p) in points.iter().enumerate() {
            if p.blank || p.rgb == [0, 0, 0] {
                continue;
            }
            let (x1, y1) = at(p);
            let (x0, y0) = if i > 0 { at(&points[i - 1]) } else { (x1, y1) };
            let mut paint = tiny_skia::Paint::default();
            paint.set_color_rgba8(p.rgb[0], p.rgb[1], p.rgb[2], (alpha * 255.0) as u8);
            paint.anti_alias = true;
            let mut pb = tiny_skia::PathBuilder::new();
            pb.move_to(x0, y0);
            // A lit point where the beam was already: draw a dot (a stroke needs some length to show).
            pb.line_to(if (x0, y0) == (x1, y1) { x1 + 0.01 } else { x1 }, y1);
            let Some(path) = pb.finish() else { continue };
            let stroke = tiny_skia::Stroke { width: width * glow, line_cap: tiny_skia::LineCap::Round, ..Default::default() };
            pixmap.stroke_path(&path, &paint, &stroke, tiny_skia::Transform::identity(), None);
        }
    }
    pixmap.pixels().iter().flat_map(|p| {
        let c = p.demultiply();
        [c.red(), c.green(), c.blue(), c.alpha()]
    }).collect()
}

/// The movie a laser file plays as: made the first time (a moment per hundred frames), then reused while the file
/// is unchanged. Call off the UI thread.
#[cfg(not(target_arch = "wasm32"))]
pub fn movie_for(path: &str) -> Result<String, String> {
    super::baked::cached_movie(path, "ilda", |data| {
        let frames = parse(data)?;
        Ok(super::baked::Drawing { size: (SIZE, SIZE), fps: FPS, frames: Box::new(frames.into_iter().map(|f| draw(&f, SIZE))) })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An ILDA section: header + records.
    fn section(format: u8, records: &[Vec<u8>], frame: u16, total: u16) -> Vec<u8> {
        let mut v = b"ILDA\0\0\0".to_vec();
        v.push(format);
        v.extend_from_slice(b"test\0\0\0\0phosphor");
        v.extend_from_slice(&(records.len() as u16).to_be_bytes());
        v.extend_from_slice(&frame.to_be_bytes());
        v.extend_from_slice(&total.to_be_bytes());
        v.extend_from_slice(&[0, 0]);
        records.iter().for_each(|r| v.extend_from_slice(r));
        v
    }

    fn pt5(x: i16, y: i16, blank: bool, last: bool, rgb: [u8; 3]) -> Vec<u8> {
        let mut r = x.to_be_bytes().to_vec();
        r.extend_from_slice(&y.to_be_bytes());
        r.push(if blank { 0x40 } else { 0 } | if last { 0x80 } else { 0 });
        r.extend_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        r
    }

    /// A square in true colour (format 5), then the end section.
    pub fn square_file() -> Vec<u8> {
        let c = 20000;
        let pts = [
            pt5(-c, -c, true, false, [0, 0, 0]),
            pt5(c, -c, false, false, [255, 0, 0]),
            pt5(c, c, false, false, [0, 255, 0]),
            pt5(-c, c, false, false, [0, 0, 255]),
            pt5(-c, -c, false, true, [255, 255, 255]),
        ];
        let mut f = section(5, &pts, 0, 2);
        f.extend(section(5, &pts[..3], 1, 2));
        f.extend(section(5, &[], 0, 0));
        f
    }

    #[test]
    fn reads_true_colour_frames() {
        let frames = parse(&square_file()).unwrap();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].len(), 5);
        assert!(frames[0][0].blank && !frames[0][1].blank);
        assert_eq!(frames[0][1].rgb, [255, 0, 0], "colours are stored blue, green, red");
        assert!((frames[0][2].x - 20000.0 / 32767.0).abs() < 1e-6 && frames[0][2].y > 0.0);
    }

    #[test]
    fn indexed_colours_use_the_palette() {
        // Format 1: x, y, status, index. Index 24 is green in the standard palette; a format 2 section replaces it.
        let rec = |i: u8| {
            let mut r = 100i16.to_be_bytes().to_vec();
            r.extend_from_slice(&(-100i16).to_be_bytes());
            r.extend_from_slice(&[0x80, i]);
            r
        };
        let mut f = section(1, &[rec(24)], 0, 1);
        f.extend(section(2, &[vec![1, 2, 3]], 0, 0));
        f.extend(section(1, &[rec(0)], 0, 1));
        let frames = parse(&f).unwrap();
        assert_eq!(frames[0][0].rgb, [0, 255, 0]);
        assert_eq!(frames[1][0].rgb, [1, 2, 3]);
        let p = default_palette();
        assert_eq!((p.len(), p[0], p[16], p[40], p[48], p[56]), (64, [255, 0, 0], [255, 255, 0], [0, 0, 255], [255, 0, 255], [255, 255, 255]));
    }

    #[test]
    fn bad_files_say_why() {
        assert!(parse(b"hello there, this is not a laser file at all").unwrap_err().contains("not an ILDA"));
        let mut f = square_file();
        f.truncate(50);
        assert!(parse(&f).unwrap_err().contains("cut short"));
    }

    /// Lines are drawn in the points' colours; the beam's blanked move and the empty area stay transparent.
    #[test]
    fn draws_lit_lines_only() {
        let frames = parse(&square_file()).unwrap();
        let px = draw(&frames[0], 200);
        let at = |x: usize, y: usize| &px[(y * 200 + x) * 4..(y * 200 + x) * 4 + 4];
        // The square's corners are at ±20000/32767 → pixels ~39 and ~161. Bottom edge (y up) = row ~161, red.
        let bottom = at(100, 161);
        assert!(bottom[0] > 200 && bottom[1] < 80 && bottom[3] > 200, "{bottom:?}");
        let right = at(161, 100);
        assert!(right[1] > 200 && right[0] < 80, "{right:?}");
        assert_eq!(at(100, 100)[3], 0, "the middle is empty");
    }

    /// The whole route: a laser file becomes a Hap movie (reused the second time), which opens and plays.
    #[test]
    fn a_laser_file_plays_as_a_movie() {
        let dir = std::env::temp_dir().join(format!("phosphor-ilda-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("square.ild");
        std::fs::write(&file, square_file()).unwrap();
        let path = file.to_string_lossy().to_string();
        let movie = movie_for(&path).unwrap();
        let made = std::fs::metadata(&movie).unwrap().modified().unwrap();
        assert_eq!(movie_for(&path).unwrap(), movie);
        assert_eq!(std::fs::metadata(&movie).unwrap().modified().unwrap(), made, "reused, not made again");
        let mut player = super::super::video::VideoPlayer::open(&path).unwrap();
        assert!(player.duration().is_some_and(|d| (d - 2.0 / FPS as f64).abs() < 0.02), "{:?}", player.duration());
        assert_eq!(player.size(), (SIZE, SIZE));
        player.set_playing(true);
        let mut got = false;
        for i in 0..200 {
            if player.poll_frame(i as f64 * 0.01).is_some() {
                got = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = std::fs::remove_file(&movie);
        let _ = std::fs::remove_dir_all(&dir);
        assert!(got, "a frame came out");
    }
}
