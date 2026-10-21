//! Scopes (VDMX's Scopes plugin): waveform, RGB parade, vectorscope and histogram of the mix or any layer, for
//! matching colours between cameras, clips and projectors. A small copy of the picture is read back a few times a
//! second, only while the Scopes panel is on screen; the scope picture is drawn on the CPU (a few thousand pixels).

use eframe::egui;

/// The monitor key scopes use (preview windows use small numbers, tracking 250).
pub const MONITOR_KEY: u8 = 251;
/// Width of the picture read back (height keeps the canvas's shape).
const WIDTH: u32 = 320;
/// Scope updates per second.
const RATE: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScopeKind {
    #[default]
    Waveform,
    Parade,
    Vectorscope,
    Histogram,
}

impl ScopeKind {
    pub const ALL: [ScopeKind; 4] = [ScopeKind::Waveform, ScopeKind::Parade, ScopeKind::Vectorscope, ScopeKind::Histogram];

    pub fn name(self) -> &'static str {
        match self {
            ScopeKind::Waveform => "Waveform",
            ScopeKind::Parade => "RGB parade",
            ScopeKind::Vectorscope => "Vectorscope",
            ScopeKind::Histogram => "Histogram",
        }
    }
}

impl phosphor_engine::Choice for ScopeKind {
    const OPTIONS: &'static [Self] = &ScopeKind::ALL;
    fn label(self) -> &'static str {
        self.name()
    }
}

pub struct Scopes {
    pub kind: ScopeKind,
    /// What to measure: a layer / group id or `MASTER_ID`.
    pub source: u64,
    /// The panel was drawn at this time (no panel, no work).
    pub shown_at: f64,
    next_at: f64,
    busy_since: Option<f64>,
    /// The newest scope picture, waiting for the panel to show it.
    pending: Option<egui::ColorImage>,
    texture: Option<egui::TextureHandle>,
}

impl Default for Scopes {
    fn default() -> Self {
        Self { kind: ScopeKind::Waveform, source: phosphor_engine::MASTER_ID, shown_at: f64::MIN, next_at: 0.0, busy_since: None, pending: None, texture: None }
    }
}

impl Scopes {
    /// The monitor picture to ask for this frame, while the panel is open.
    pub fn picture(&self, now: f64, canvas: (u32, u32)) -> Option<(u64, u32, u32)> {
        (now - self.shown_at < 0.5).then(|| (self.source, WIDTH, ((WIDTH * canvas.1) / canvas.0.max(1)).clamp(16, 1080)))
    }

    /// Read this frame's picture back? (Paced, one at a time.)
    pub fn due(&mut self, now: f64) -> bool {
        if self.busy_since.is_some_and(|t| now - t > 1.0) {
            self.busy_since = None;
        }
        if self.busy_since.is_some() || now < self.next_at {
            return false;
        }
        self.busy_since = Some(now);
        self.next_at = now + 1.0 / RATE;
        true
    }

    /// The picture came back: draw the scope (the panel shows it next time it's drawn).
    pub fn give(&mut self, rgba: &[u8], width: u32, height: u32) {
        self.busy_since = None;
        self.pending = Some(draw(self.kind, rgba, width as usize, height as usize));
    }

    /// The scope picture to show (uploads the newest one first).
    pub fn texture(&mut self, ctx: &egui::Context) -> Option<&egui::TextureHandle> {
        if let Some(image) = self.pending.take() {
            match &mut self.texture {
                Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("scopes", image, egui::TextureOptions::LINEAR)),
            }
        }
        self.texture.as_ref()
    }
}

const SIZE: [usize; 2] = [256, 160];

/// Rec. 709 brightness, 0..1.
fn luma(p: &[u8]) -> f32 {
    (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
}

/// One scope as a picture (`SIZE`): counts become brightness (square root, so thin traces still show).
pub fn draw(kind: ScopeKind, rgba: &[u8], w: usize, h: usize) -> egui::ColorImage {
    let [sw, sh] = SIZE;
    // Per pixel of the scope, per channel: how many picture pixels landed there.
    let mut counts = vec![[0u32; 3]; sw * sh];
    let mut hit = |x: usize, y: usize, ch: usize| {
        if x < sw && y < sh {
            counts[y * sw + x][ch] += 1;
        }
    };
    let row = |v: f32| ((1.0 - v.clamp(0.0, 1.0)) * (sh - 1) as f32) as usize;
    let pixels = rgba.chunks_exact(4).take(w * h).enumerate();
    match kind {
        ScopeKind::Waveform => {
            for (i, p) in pixels {
                let x = (i % w) * sw / w.max(1);
                let y = row(luma(p));
                for ch in 0..3 {
                    hit(x, y, ch);
                }
            }
        }
        ScopeKind::Parade => {
            let third = sw / 3;
            for (i, p) in pixels {
                let x = (i % w) * third / w.max(1);
                for ch in 0..3 {
                    hit(ch * third + x, row(p[ch] as f32 / 255.0), ch);
                }
            }
        }
        ScopeKind::Vectorscope => {
            let (cx, cy, r) = (sw as f32 / 2.0, sh as f32 / 2.0, sh as f32 / 2.0 - 2.0);
            for (_, p) in pixels {
                let [rr, g, b] = [p[0] as f32 / 255.0, p[1] as f32 / 255.0, p[2] as f32 / 255.0];
                let cb = -0.1146 * rr - 0.3854 * g + 0.5 * b;
                let cr = 0.5 * rr - 0.4542 * g - 0.0458 * b;
                let (x, y) = (cx + cb * 2.0 * r, cy - cr * 2.0 * r);
                for ch in 0..3 {
                    hit(x as usize, y as usize, ch);
                }
            }
        }
        ScopeKind::Histogram => {
            let mut bins = [[0u32; 3]; 256];
            for (_, p) in pixels {
                for ch in 0..3 {
                    bins[p[ch] as usize][ch] += 1;
                }
            }
            let peak = bins.iter().flatten().copied().max().unwrap_or(1).max(1) as f32;
            for (x, b) in bins.iter().enumerate().take(sw) {
                for ch in 0..3 {
                    let top = row((b[ch] as f32 / peak).sqrt());
                    for y in top..sh {
                        hit(x, y, ch);
                    }
                }
            }
        }
    }
    let total = (w * h).max(1) as f32;
    let gain = match kind {
        ScopeKind::Histogram => 0.0,
        _ => (sw * sh) as f32 / total * 4.0,
    };
    let colour = |c: [u32; 3]| -> egui::Color32 {
        let v = |n: u32| if gain == 0.0 { if n > 0 { 150 } else { 0 } } else { ((n as f32 * gain).sqrt() * 120.0).min(255.0) as u8 };
        match kind {
            // One trace in green (brightness only).
            ScopeKind::Waveform | ScopeKind::Vectorscope => {
                let l = v(c[1]);
                egui::Color32::from_rgb(l / 3, l, l / 2)
            }
            _ => egui::Color32::from_rgb(v(c[0]), v(c[1]), v(c[2])),
        }
    };
    let mut image = egui::ColorImage::new(SIZE, counts.iter().map(|c| colour(*c)).collect());
    // Guides: 25 / 50 / 75 % lines (or the vectorscope's circle).
    let guide = egui::Color32::from_gray(55);
    if kind == ScopeKind::Vectorscope {
        let (cx, cy, r) = (sw as f32 / 2.0, sh as f32 / 2.0, sh as f32 / 2.0 - 2.0);
        for k in 0..360 {
            let a = (k as f32).to_radians();
            let (x, y) = ((cx + r * a.cos()) as usize, (cy + r * a.sin()) as usize);
            if x < sw && y < sh {
                image.pixels[y * sw + x] = guide;
            }
        }
    } else {
        for q in [0.25, 0.5, 0.75] {
            let y = row(q);
            for x in 0..sw {
                if image.pixels[y * sw + x] == egui::Color32::BLACK || image.pixels[y * sw + x].r() < 20 {
                    image.pixels[y * sw + x] = guide;
                }
            }
        }
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(rgb: [u8; 3]) -> Vec<u8> {
        (0..64 * 36).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
    }

    fn lit_rows(img: &egui::ColorImage) -> Vec<usize> {
        (0..SIZE[1]).filter(|y| (0..SIZE[0]).any(|x| img.pixels[y * SIZE[0] + x].g() > 100)).collect()
    }

    #[test]
    fn white_sits_at_the_top_black_at_the_bottom() {
        assert_eq!(lit_rows(&draw(ScopeKind::Waveform, &picture([255; 3]), 64, 36)), vec![0]);
        assert_eq!(lit_rows(&draw(ScopeKind::Waveform, &picture([0; 3]), 64, 36)), vec![SIZE[1] - 1]);
    }

    #[test]
    fn grey_is_the_vectorscope_centre_and_red_is_not() {
        let at = |img: &egui::ColorImage| (0..SIZE[0] * SIZE[1]).find(|i| img.pixels[*i].g() > 100).map(|i| (i % SIZE[0], i / SIZE[0]));
        let grey = at(&draw(ScopeKind::Vectorscope, &picture([128; 3]), 64, 36)).unwrap();
        assert!((grey.0 as i32 - 128).abs() <= 1 && (grey.1 as i32 - 80).abs() <= 1, "{grey:?}");
        let red = at(&draw(ScopeKind::Vectorscope, &picture([255, 0, 0]), 64, 36)).unwrap();
        assert!(red.1 < 60, "red is up the Cr axis: {red:?}");
    }
}
