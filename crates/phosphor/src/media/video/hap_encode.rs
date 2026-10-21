//! Hap encoding for recordings and conversions (FFmpeg as shipped here has no Hap encoder): RGBA frames → GPU
//! blocks on every core → Snappy → Hap frame header → a QuickTime .mov muxed in-process by the FFmpeg libraries
//! Phosphor already links. Plays in Phosphor, VDMX, Resolume, MadMapper and any Hap player.
//!
//! Hap = DXT1, Hap Alpha = DXT5, Hap Q = scaled YCoCg in DXT5 (better colour, twice the size), Hap Q Alpha = Hap Q
//! plus an RGTC1 (BC4) alpha image. Format: github.com/Vidvox/hap/blob/master/documentation/HapVideoDRAFT.md.

use ff::ffi;
use ffmpeg_next as ff;

/// One 4×4 block of RGBA pixels.
type Block = [[u8; 4]; 16];

/// Which Hap: the codec tag and the images each frame holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HapKind {
    Hap,
    HapAlpha,
    HapQ,
    HapQAlpha,
}

impl HapKind {
    fn tag(self) -> [u8; 4] {
        match self {
            HapKind::Hap => *b"Hap1",
            HapKind::HapAlpha => *b"Hap5",
            HapKind::HapQ => *b"HapY",
            HapKind::HapQAlpha => *b"HapM",
        }
    }
}

fn rgb565(c: [f32; 3]) -> u16 {
    let r = (c[0].clamp(0.0, 255.0) * 31.0 / 255.0).round() as u16;
    let g = (c[1].clamp(0.0, 255.0) * 63.0 / 255.0).round() as u16;
    let b = (c[2].clamp(0.0, 255.0) * 31.0 / 255.0).round() as u16;
    (r << 11) | (g << 5) | b
}

fn unpack565(c: u16) -> [i32; 3] {
    let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
    [((r << 3) | (r >> 2)) as i32, ((g << 2) | (g >> 4)) as i32, ((b << 3) | (b >> 2)) as i32]
}

/// DXT1 colour block: endpoints from the colour bounding box (diagonal picked by the colours' spread,
/// inset a little), four-colour mode, nearest palette entry per pixel.
fn bc1(px: &Block) -> [u8; 8] {
    let mut lo = [255f32; 3];
    let mut hi = [0f32; 3];
    let mut mean = [0f32; 3];
    for p in px {
        for c in 0..3 {
            let v = p[c] as f32;
            lo[c] = lo[c].min(v);
            hi[c] = hi[c].max(v);
            mean[c] += v / 16.0;
        }
    }
    // Which way the colours run between red/green and blue/green decides the bounding-box diagonal.
    let (mut crg, mut cbg) = (0f32, 0f32);
    for p in px {
        let d = [p[0] as f32 - mean[0], p[1] as f32 - mean[1], p[2] as f32 - mean[2]];
        crg += d[0] * d[1];
        cbg += d[2] * d[1];
    }
    if crg < 0.0 {
        std::mem::swap(&mut lo[0], &mut hi[0]);
    }
    if cbg < 0.0 {
        std::mem::swap(&mut lo[2], &mut hi[2]);
    }
    for c in 0..3 {
        let inset = (hi[c] - lo[c]) / 16.0;
        hi[c] -= inset;
        lo[c] += inset;
    }
    let (mut c0, mut c1) = (rgb565(hi), rgb565(lo));
    if c0 == c1 {
        let mut out = [0u8; 8];
        out[..2].copy_from_slice(&c0.to_le_bytes());
        out[2..4].copy_from_slice(&c1.to_le_bytes());
        return out;
    }
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    let (a, b) = (unpack565(c0), unpack565(c1));
    let palette = [a, b, [(2 * a[0] + b[0]) / 3, (2 * a[1] + b[1]) / 3, (2 * a[2] + b[2]) / 3], [(a[0] + 2 * b[0]) / 3, (a[1] + 2 * b[1]) / 3, (a[2] + 2 * b[2]) / 3]];
    let mut bits = 0u32;
    for (i, p) in px.iter().enumerate() {
        let best = (0..4)
            .min_by_key(|&k| {
                let q = palette[k];
                let d = [p[0] as i32 - q[0], p[1] as i32 - q[1], p[2] as i32 - q[2]];
                d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
            })
            .unwrap_or(0);
        bits |= (best as u32) << (2 * i);
    }
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    out[4..].copy_from_slice(&bits.to_le_bytes());
    out
}

/// One channel of a block in eight-value mode: DXT5's alpha half, and the whole of an RGTC1 (BC4) block.
fn bc4(v: [u8; 16]) -> [u8; 8] {
    let (vmin, vmax) = v.iter().fold((255u8, 0u8), |(lo, hi), x| (lo.min(*x), hi.max(*x)));
    let mut out = [0u8; 8];
    out[0] = vmax;
    out[1] = vmin;
    if vmax > vmin {
        let (a0, a1) = (vmax as i32, vmin as i32);
        let pal: Vec<i32> = (0..8).map(|k| match k {
            0 => a0,
            1 => a1,
            k => ((8 - k) * a0 + (k - 1) * a1) / 7,
        }).collect();
        let mut bits = 0u64;
        for (i, x) in v.iter().enumerate() {
            let best = (0..8).min_by_key(|&k| (pal[k] - *x as i32).abs()).unwrap_or(0) as u64;
            bits |= best << (3 * i);
        }
        out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    }
    out
}

/// DXT5: the alpha block, then the colour block.
fn bc3(px: &Block) -> [u8; 16] {
    let mut out = [0u8; 16];
    out[..8].copy_from_slice(&bc4(std::array::from_fn(|i| px[i][3])));
    out[8..].copy_from_slice(&bc1(px));
    out
}

/// Hap Q: scaled YCoCg in DXT5 (alpha = Y; red, green = Co, Cg scaled by 1, 2 or 4 around 128; blue = the scale), the
/// layout the decoding shader reads (`sample_source`, mode 2).
fn ycocg(px: &Block) -> [u8; 16] {
    let conv: [[f32; 3]; 16] = std::array::from_fn(|i| {
        let [r, g, b] = [px[i][0] as f32, px[i][1] as f32, px[i][2] as f32];
        [r / 4.0 + g / 2.0 + b / 4.0, r / 2.0 - b / 2.0, -r / 4.0 + g / 2.0 - b / 4.0]
    });
    // The largest chroma decides how much it can be magnified (more precision for pale blocks).
    let spread = conv.iter().fold(0f32, |m, c| m.max(c[1].abs()).max(c[2].abs()));
    let scale: f32 = if spread < 31.0 { 4.0 } else if spread < 63.0 { 2.0 } else { 1.0 };
    let block: Block = std::array::from_fn(|i| {
        let [y, co, cg] = conv[i];
        let q = |c: f32| (c * scale + 128.0).round().clamp(0.0, 255.0) as u8;
        [q(co), q(cg), ((scale - 1.0) * 8.0) as u8, y.round().clamp(0.0, 255.0) as u8]
    });
    bc3(&block)
}

/// The frame's DXT blocks, row by row (edges padded by repeating the last pixel), encoded on every core.
pub fn dxt(rgba: &[u8], w: u32, h: u32, alpha: bool) -> Vec<u8> {
    if alpha { blocks(rgba, w, h, 16, |b, out| out.copy_from_slice(&bc3(b))) } else { blocks(rgba, w, h, 8, |b, out| out.copy_from_slice(&bc1(b))) }
}

/// Every 4×4 block of the frame through `encode` (`size` bytes each), row by row, edges padded by repeating the last
/// pixel, on every core.
fn blocks(rgba: &[u8], w: u32, h: u32, size: usize, encode: impl Fn(&Block, &mut [u8]) + Sync) -> Vec<u8> {
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    let encode = &encode;
    let mut out = vec![0u8; bw * bh * size];
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get()).min(16);
    let rows_per = bh.div_ceil(threads).max(1);
    std::thread::scope(|s| {
        for (chunk_i, chunk) in out.chunks_mut(rows_per * bw * size).enumerate() {
            s.spawn(move || {
                for (r, row) in chunk.chunks_mut(bw * size).enumerate() {
                    let by = chunk_i * rows_per + r;
                    for bx in 0..bw {
                        let mut block: Block = [[0; 4]; 16];
                        for (i, px) in block.iter_mut().enumerate() {
                            let x = (bx * 4 + i % 4).min(w as usize - 1);
                            let y = (by * 4 + i / 4).min(h as usize - 1);
                            let o = (y * w as usize + x) * 4;
                            px.copy_from_slice(&rgba[o..o + 4]);
                        }
                        encode(&block, &mut row[bx * size..bx * size + size]);
                    }
                }
            });
        }
    });
    out
}

/// A Hap section: its header (3-byte size and type, or 0 + a 4-byte size) and payload.
fn section(kind: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(payload.len() + 8);
    if !payload.is_empty() && payload.len() <= 0xFF_FFFF {
        let n = payload.len() as u32;
        out.extend_from_slice(&[n as u8, (n >> 8) as u8, (n >> 16) as u8, kind]);
    } else {
        out.extend_from_slice(&[0, 0, 0, kind]);
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(payload);
    out
}

fn snappy(data: &[u8]) -> Vec<u8> {
    snap::raw::Encoder::new().compress_vec(data).unwrap_or_default()
}

/// One Hap frame. Section types: Snappy (0xB_) + DXT1 0xB, DXT5 0xE, YCoCg DXT5 0xF, RGTC1 0x1; Hap Q Alpha is a
/// "multiple images" section (0x0D) holding the YCoCg image and the alpha image.
pub fn hap_frame(rgba: &[u8], w: u32, h: u32, kind: HapKind) -> Vec<u8> {
    match kind {
        HapKind::Hap => section(0xBB, &snappy(&dxt(rgba, w, h, false))),
        HapKind::HapAlpha => section(0xBE, &snappy(&dxt(rgba, w, h, true))),
        HapKind::HapQ => section(0xBF, &snappy(&blocks(rgba, w, h, 16, |b, out| out.copy_from_slice(&ycocg(b))))),
        HapKind::HapQAlpha => {
            let colour = section(0xBF, &snappy(&blocks(rgba, w, h, 16, |b, out| out.copy_from_slice(&ycocg(b)))));
            let alpha = section(0xB1, &snappy(&blocks(rgba, w, h, 8, |b, out| out.copy_from_slice(&bc4(std::array::from_fn(|i| b[i][3]))))));
            section(0x0D, &[colour, alpha].concat())
        }
    }
}

/// Writes Hap frames into a .mov file.
pub struct HapWriter {
    octx: ff::format::context::Output,
    w: u32,
    h: u32,
    kind: HapKind,
    frame: i64,
    /// Frame numbers count in 1/fps; the muxer may store a finer time base.
    fps_tb: ff::Rational,
    out_tb: ff::Rational,
}

impl HapWriter {
    pub fn create(path: &str, w: u32, h: u32, fps: u32, kind: HapKind) -> Result<Self, String> {
        super::init();
        let mut octx = ff::format::output_as(&path, "mov").map_err(|e| format!("can't create {path} ({e})"))?;
        let fps = fps.max(1) as i32;
        {
            let mut st = octx.add_stream(None::<ff::Codec>).map_err(|e| e.to_string())?;
            st.set_time_base(ff::Rational::new(1, fps));
            st.set_avg_frame_rate(ff::Rational::new(fps, 1));
            // SAFETY: plain fields of the stream's parameters, before the header is written.
            unsafe {
                let par = (*st.as_mut_ptr()).codecpar;
                (*par).codec_type = ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
                (*par).codec_id = ffi::AVCodecID::AV_CODEC_ID_HAP;
                (*par).codec_tag = u32::from_le_bytes(kind.tag());
                (*par).width = w as i32;
                (*par).height = h as i32;
            }
        }
        octx.write_header().map_err(|e| format!("can't start the movie ({e})"))?;
        let fps_tb = ff::Rational::new(1, fps);
        let out_tb = octx.stream(0).map_or(fps_tb, |s| s.time_base());
        Ok(Self { octx, w, h, kind, frame: 0, fps_tb, out_tb })
    }

    /// Encodes and writes one RGBA frame.
    pub fn write(&mut self, rgba: &[u8]) -> Result<(), String> {
        if rgba.len() < (self.w * self.h * 4) as usize {
            return Err("frame size changed".into());
        }
        let data = hap_frame(rgba, self.w, self.h, self.kind);
        let mut pkt = ff::Packet::copy(&data);
        pkt.set_stream(0);
        pkt.set_pts(Some(self.frame));
        pkt.set_dts(Some(self.frame));
        pkt.set_duration(1);
        pkt.set_flags(ff::packet::Flags::KEY);
        pkt.rescale_ts(self.fps_tb, self.out_tb);
        self.frame += 1;
        pkt.write_interleaved(&mut self.octx).map_err(|e| e.to_string())
    }

    pub fn finish(mut self) -> Result<(), String> {
        self.octx.write_trailer().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_keep_flat_colours_and_alpha() {
        let red: Block = [[255, 0, 0, 255]; 16];
        let b = bc1(&red);
        assert_eq!(u16::from_le_bytes([b[0], b[1]]), 0xF800);
        let mut half: Block = [[0, 0, 255, 0]; 16];
        half[..8].iter_mut().for_each(|p| p[3] = 255);
        let b = bc3(&half);
        assert_eq!((b[0], b[1]), (255, 0));
    }

    /// Records a few Hap frames and plays them back through Phosphor's own player.
    #[test]
    fn records_a_hap_movie_that_plays_back() {
        let path = std::env::temp_dir().join("phosphor_hap_test.mov");
        let path = path.to_string_lossy().to_string();
        let (w, h) = (64u32, 32u32);
        let mut frame = vec![0u8; (w * h * 4) as usize];
        for (i, p) in frame.chunks_exact_mut(4).enumerate() {
            let left = (i as u32 % w) < w / 2;
            p.copy_from_slice(if left { &[255, 0, 0, 255] } else { &[0, 0, 255, 255] });
        }
        let mut wr = HapWriter::create(&path, w, h, 30, HapKind::Hap).unwrap();
        for _ in 0..10 {
            wr.write(&frame).unwrap();
        }
        wr.finish().unwrap();
        let mut p = super::super::VideoPlayer::open(&path).expect("opens");
        assert_eq!(p.size(), (w, h));
        assert!(p.stats().codec.contains("hap") || p.duration().is_some());
        let mut got = None;
        for i in 0..400 {
            if let Some(f) = p.poll_frame(i as f64 / 1000.0) {
                got = Some(f);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(got.is_some(), "a frame came back");
        let _ = std::fs::remove_file(&path);
    }
}

/// Writes sample movies to the folder $PHOSPHOR_HAP_OUT, one per Hap flavour, for checking with other players: a
/// subtle dark ramp across (where DXT1's 5-6-5 colour bands) and alpha rising from top (clear) to bottom (solid).
#[cfg(test)]
#[test]
fn write_sample_hap() {
    let Some(dir) = std::env::var_os("PHOSPHOR_HAP_OUT") else { return };
    let (w, h) = (320u32, 180u32);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    for (i, p) in frame.chunks_exact_mut(4).enumerate() {
        let (x, y) = (i as u32 % w, i as u32 / w);
        let t = x as f32 / (w - 1) as f32;
        p.copy_from_slice(&[(20.0 + 40.0 * t) as u8, (30.0 + 25.0 * t) as u8, (45.0 + 30.0 * t) as u8, (255 * y / (h - 1)) as u8]);
    }
    let dir = std::path::PathBuf::from(dir);
    std::fs::write(dir.join("source.rgba"), &frame).unwrap();
    for (name, kind) in [("hap", HapKind::Hap), ("hap_alpha", HapKind::HapAlpha), ("hapq", HapKind::HapQ), ("hapq_alpha", HapKind::HapQAlpha)] {
        let mut wr = HapWriter::create(&dir.join(format!("{name}.mov")).to_string_lossy(), w, h, 30, kind).unwrap();
        for _ in 0..30 {
            wr.write(&frame).unwrap();
        }
        wr.finish().unwrap();
    }
}
