//! Throwaway probe for media/video: plays each file against a simulated 60 Hz clock.
//! Usage: video_probe FILE...            (probe)
//!        video_probe --gen-hap DIR      (write synthetic Hap/Hap Alpha/Hap Q test movies)

#[allow(dead_code)]
#[path = "../src/media/video/mod.rs"]
mod video;

/// What the video code needs from `crate::media` (image sequences).
#[allow(dead_code)]
#[path = "../src/media"]
mod media {
    pub mod sequence;

    /// Lottie files aren't probed here.
    pub mod lottie {
        pub fn is_lottie(_: &str) -> bool {
            false
        }
        pub fn movie_for(path: &str) -> Result<String, String> {
            Ok(path.to_string())
        }
    }

    /// The probe reads files where they are (no copy to this Mac).
    pub mod clip_cache {
        pub fn resolve(path: &str) -> String {
            path.to_string()
        }
    }

    /// Laser files aren't probed here.
    pub mod ilda {
        pub fn is_ilda(_: &str) -> bool {
            false
        }
        pub fn movie_for(path: &str) -> Result<String, String> {
            Ok(path.to_string())
        }
    }

    pub fn is_picture(name: &str) -> bool {
        name.rsplit_once('.').is_some_and(|(_, e)| matches!(e.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "tif" | "tiff" | "bmp" | "webp"))
    }
}

use std::time::{Duration, Instant};
use video::{VideoFrame, VideoPlayer};

fn cpu_secs() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let t = |tv: libc::timeval| tv.tv_sec as f64 + tv.tv_usec as f64 * 1e-6;
    t(ru.ru_utime) + t(ru.ru_stime)
}

struct Run {
    frames: u32,
    after_end: u32,
    max_gap_ms: f64,
    kind: &'static str,
    size: (u32, u32),
}

/// Ticks a 60 Hz clock for `secs` of wall time at `rate`, starting from media time `media`.
fn play(p: &mut VideoPlayer, t0: Instant, secs: f64, rate: f32, media: &mut f64, run: &mut Run) {
    p.set_rate(rate);
    let start = t0.elapsed().as_secs_f64();
    let dur = p.duration().unwrap_or(f64::MAX);
    let mut last = Instant::now();
    loop {
        let now = t0.elapsed().as_secs_f64();
        if now - start >= secs {
            break;
        }
        if let Some(f) = p.poll_frame(now) {
            run.frames += 1;
            if *media > dur + 0.05 {
                run.after_end += 1;
            }
            run.max_gap_ms = run.max_gap_ms.max(last.elapsed().as_secs_f64() * 1e3);
            last = Instant::now();
            run.size = f.size();
            run.kind = match f {
                VideoFrame::Nv12 { .. } => "NV12",
                VideoFrame::Rgba { .. } => "RGBA",
                VideoFrame::Compressed { .. } => "GPU blocks",
                VideoFrame::Surface(_) => "zero-copy",
            };
            p.recycle(f);
        }
        std::thread::sleep(Duration::from_micros(16_667));
        *media += (t0.elapsed().as_secs_f64() - now) * rate as f64;
    }
}

fn probe(path: &str) {
    let name = path.rsplit('/').next().unwrap_or(path);
    let t_open = Instant::now();
    let mut p = match VideoPlayer::open(path) {
        Ok(p) => p,
        Err(e) => return println!("{name:<24} ERROR {e}"),
    };
    let open_ms = t_open.elapsed().as_secs_f64() * 1e3;
    let cpu0 = cpu_secs();
    let t0 = Instant::now();
    let mut run = Run {
        frames: 0,
        after_end: 0,
        max_gap_ms: 0.0,
        kind: "-",
        size: (0, 0),
    };
    let mut media = 0.0;
    // 2 s at 1x, then 1.5 s at 4x (6 s of media) to cross the loop point on short clips.
    play(&mut p, t0, 2.0, 1.0, &mut media, &mut run);
    let first = run.frames;
    run.max_gap_ms = 0.0;
    play(&mut p, t0, 1.5, 4.0, &mut media, &mut run);
    let wall = t0.elapsed().as_secs_f64();
    let cpu = cpu_secs() - cpu0;

    // Pause: decoder must go idle once the queue is full.
    p.set_playing(false);
    let _ = p.poll_frame(t0.elapsed().as_secs_f64());
    std::thread::sleep(Duration::from_millis(150));
    let d0 = p.stats().frames_decoded;
    let c0 = cpu_secs();
    for _ in 0..30 {
        let _ = p.poll_frame(t0.elapsed().as_secs_f64());
        std::thread::sleep(Duration::from_millis(16));
    }
    let paused_decodes = p.stats().frames_decoded - d0;
    let paused_cpu = (cpu_secs() - c0) * 1e3;

    let st = p.stats();
    let t_drop = Instant::now();
    let err = p.error().map(str::to_string);
    drop(p);
    println!(
        "{name:<24} {:<10} {:>4} {:>9} {:>4}  {:>3}@1x {:>3}@4x  loop+{:<3} gap {:>5.1}ms  cpu {:>5.1}%  dec {:>4} skip {:>3}  open {:>4.0}ms drop {:>3.0}ms  paused: +{} dec {:.1}ms cpu{}",
        st.codec,
        format!("{}x{}", run.size.0, run.size.1),
        run.kind,
        if st.hardware { "HW" } else { "SW" },
        first,
        run.frames - first,
        run.after_end,
        run.max_gap_ms,
        cpu / wall * 100.0,
        st.frames_decoded,
        st.frames_skipped,
        open_ms,
        t_drop.elapsed().as_secs_f64() * 1e3,
        paused_decodes,
        paused_cpu,
        err.map(|e| format!("  ERR {e}")).unwrap_or_default(),
    );
}

fn no_loop(path: &str) {
    let Ok(mut p) = VideoPlayer::open(path) else {
        return;
    };
    p.set_looping(false);
    let t0 = Instant::now();
    let (mut frames, mut last_frame_at) = (0, 0.0);
    while t0.elapsed().as_secs_f64() < 4.0 {
        let now = t0.elapsed().as_secs_f64();
        if p.poll_frame(now).is_some() {
            frames += 1;
            last_frame_at = now;
        }
        std::thread::sleep(Duration::from_micros(16_667));
    }
    let after = p.stats().frames_decoded;
    p.set_looping(true);
    let t1 = Instant::now();
    let mut resumed = 0;
    while t1.elapsed().as_secs_f64() < 0.5 {
        if p.poll_frame(t0.elapsed().as_secs_f64()).is_some() {
            resumed += 1;
        }
        std::thread::sleep(Duration::from_micros(16_667));
    }
    println!(
        "no-loop {}: {frames} frames, last at {last_frame_at:.2}s (duration {:.2}s), decoded {after}; re-enabled looping -> {resumed} frames in 0.5s",
        path.rsplit('/').next().unwrap_or(path),
        p.duration().unwrap_or(0.0)
    );
}

fn gen_hap(dir: &str) -> Result<(), ffmpeg_next::Error> {
    use ffmpeg_next as ff;
    ff::init()?;
    // (file, fourcc, section type, bytes per 4x4 block, w, h)
    for (file, tag, kind, bpb, w, h) in [
        ("hap.mov", *b"Hap1", 0xABu8, 8usize, 1280u32, 720u32),
        ("hap_alpha.mov", *b"Hap5", 0xAE, 16, 640, 360),
        ("hapq.mov", *b"HapY", 0xAF, 16, 1280, 720),
    ] {
        let mut oc = ff::format::output(&format!("{dir}/{file}"))?;
        unsafe {
            let st = ff::ffi::avformat_new_stream(oc.as_mut_ptr(), std::ptr::null());
            let par = (*st).codecpar;
            (*par).codec_type = ff::ffi::AVMediaType::AVMEDIA_TYPE_VIDEO;
            (*par).codec_id = ff::ffi::AVCodecID::AV_CODEC_ID_HAP;
            (*par).codec_tag = u32::from_le_bytes(tag);
            (*par).width = w as i32;
            (*par).height = h as i32;
            (*st).time_base = ff::ffi::AVRational { num: 1, den: 30 };
        }
        oc.write_header()?;
        let tb = oc
            .stream(0)
            .map(|s| s.time_base())
            .unwrap_or(ff::Rational(1, 30));
        let (bw, bh) = (w / 4, h / 4);
        for i in 0..75u32 {
            let size = (bw * bh) as usize * bpb;
            let mut data = vec![kind; 4];
            data[0] = size as u8;
            data[1] = (size >> 8) as u8;
            data[2] = (size >> 16) as u8;
            for by in 0..bh {
                for bx in 0..bw {
                    let r = ((bx * 31 / bw + i) % 32) as u16;
                    let g = ((by * 63 / bh) % 64) as u16;
                    let b = (((bx + by + i * 2) / 8) % 32) as u16;
                    let c = (r << 11 | g << 5 | b).to_le_bytes();
                    if bpb == 16 {
                        let a = ((bx * 255) / bw) as u8;
                        data.extend_from_slice(&[a, a, 0, 0, 0, 0, 0, 0]);
                    }
                    data.extend_from_slice(&[c[0], c[1], c[0], c[1], 0, 0, 0, 0]);
                }
            }
            let mut pkt = ff::Packet::copy(&data);
            pkt.set_stream(0);
            pkt.set_pts(Some(i as i64));
            pkt.set_dts(Some(i as i64));
            pkt.set_duration(1);
            pkt.set_flags(ff::packet::Flags::KEY);
            pkt.rescale_ts(ff::Rational(1, 30), tb);
            pkt.write_interleaved(&mut oc)?;
        }
        oc.write_trailer()?;
        println!("wrote {dir}/{file}");
    }
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // --gpu-hap: Hap frames come out as GPU blocks (what the app does when the GPU supports BC textures).
    let args: Vec<String> = if args.first().map(String::as_str) == Some("--gpu-hap") {
        video::set_gpu_hap(true);
        args[1..].to_vec()
    } else {
        args
    };
    if args.first().map(String::as_str) == Some("--gen-hap") {
        if let Err(e) = gen_hap(args.get(1).map_or(".", |s| s.as_str())) {
            eprintln!("gen-hap failed: {e}");
        }
        return;
    }
    println!(
        "cpu% = whole-process CPU over the 3.5 s run as % of one core (VideoToolbox work runs out of process)"
    );
    for path in &args {
        probe(path);
    }
    if let Some(first) = args.first() {
        no_loop(first);
    }
}
