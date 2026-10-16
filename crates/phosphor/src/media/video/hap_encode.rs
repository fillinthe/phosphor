//! Hap encoding for recordings (FFmpeg as shipped here has no Hap encoder): RGBA frames → DXT1 (Hap) or DXT5
//! (Hap Alpha) blocks on every core → Snappy → Hap frame header → a QuickTime .mov muxed in-process by the
//! FFmpeg libraries Phosphor already links. Plays in Phosphor, VDMX, Resolume, MadMapper and any Hap player.

use ff::ffi;
use ffmpeg_next as ff;

/// One 4×4 block of RGBA pixels.
type Block = [[u8; 4]; 16];

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

/// DXT5 alpha block (eight-value mode), then the colour block.
fn bc3(px: &Block) -> [u8; 16] {
    let (amin, amax) = px.iter().fold((255u8, 0u8), |(lo, hi), p| (lo.min(p[3]), hi.max(p[3])));
    let mut out = [0u8; 16];
    out[0] = amax;
    out[1] = amin;
    if amax > amin {
        let (a0, a1) = (amax as i32, amin as i32);
        let pal: Vec<i32> = (0..8).map(|k| match k {
            0 => a0,
            1 => a1,
            k => ((8 - k) * a0 + (k - 1) * a1) / 7,
        }).collect();
        let mut bits = 0u64;
        for (i, p) in px.iter().enumerate() {
            let best = (0..8).min_by_key(|&k| (pal[k] - p[3] as i32).abs()).unwrap_or(0) as u64;
            bits |= best << (3 * i);
        }
        out[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    }
    out[8..].copy_from_slice(&bc1(px));
    out
}

/// The frame's DXT blocks, row by row (edges padded by repeating the last pixel), encoded on every core.
pub fn dxt(rgba: &[u8], w: u32, h: u32, alpha: bool) -> Vec<u8> {
    let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
    let size = if alpha { 16 } else { 8 };
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
                        let dst = &mut row[bx * size..bx * size + size];
                        if alpha {
                            dst.copy_from_slice(&bc3(&block));
                        } else {
                            dst.copy_from_slice(&bc1(&block));
                        }
                    }
                }
            });
        }
    });
    out
}

/// One Hap frame: section header (Snappy-compressed DXT1 = 0xBB, DXT5 = 0xBE) + data.
pub fn hap_frame(rgba: &[u8], w: u32, h: u32, alpha: bool) -> Vec<u8> {
    let blocks = dxt(rgba, w, h, alpha);
    let packed = snap::raw::Encoder::new().compress_vec(&blocks).unwrap_or_default();
    let kind: u8 = if alpha { 0xBE } else { 0xBB };
    let mut out = Vec::with_capacity(packed.len() + 8);
    if packed.len() <= 0xFF_FFFF {
        let n = packed.len() as u32;
        out.extend_from_slice(&[n as u8, (n >> 8) as u8, (n >> 16) as u8, kind]);
    } else {
        out.extend_from_slice(&[0, 0, 0, kind]);
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    }
    out.extend_from_slice(&packed);
    out
}

/// Writes Hap frames into a .mov file.
pub struct HapWriter {
    octx: ff::format::context::Output,
    w: u32,
    h: u32,
    alpha: bool,
    frame: i64,
    /// Frame numbers count in 1/fps; the muxer may store a finer time base.
    fps_tb: ff::Rational,
    out_tb: ff::Rational,
}

impl HapWriter {
    pub fn create(path: &str, w: u32, h: u32, fps: u32, alpha: bool) -> Result<Self, String> {
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
                let tag = if alpha { *b"Hap5" } else { *b"Hap1" };
                (*par).codec_tag = u32::from_le_bytes(tag);
                (*par).width = w as i32;
                (*par).height = h as i32;
            }
        }
        octx.write_header().map_err(|e| format!("can't start the movie ({e})"))?;
        let fps_tb = ff::Rational::new(1, fps);
        let out_tb = octx.stream(0).map_or(fps_tb, |s| s.time_base());
        Ok(Self { octx, w, h, alpha, frame: 0, fps_tb, out_tb })
    }

    /// Encodes and writes one RGBA frame.
    pub fn write(&mut self, rgba: &[u8]) -> Result<(), String> {
        if rgba.len() < (self.w * self.h * 4) as usize {
            return Err("frame size changed".into());
        }
        let data = hap_frame(rgba, self.w, self.h, self.alpha);
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
        let mut wr = HapWriter::create(&path, w, h, 30, false).unwrap();
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

/// Writes a sample Hap movie to $PHOSPHOR_HAP_OUT (for checking with other players).
#[cfg(test)]
#[test]
fn write_sample_hap() {
    let Some(out) = std::env::var_os("PHOSPHOR_HAP_OUT") else { return };
    let (w, h) = (320u32, 180u32);
    let mut frame = vec![0u8; (w * h * 4) as usize];
    for (i, p) in frame.chunks_exact_mut(4).enumerate() {
        let x = i as u32 % w;
        p.copy_from_slice(if x < w / 3 { &[255, 0, 0, 255] } else if x < 2 * w / 3 { &[0, 255, 0, 255] } else { &[0, 0, 255, 255] });
    }
    let mut wr = HapWriter::create(&out.to_string_lossy(), w, h, 30, false).unwrap();
    for _ in 0..30 {
        wr.write(&frame).unwrap();
    }
    wr.finish().unwrap();
}
