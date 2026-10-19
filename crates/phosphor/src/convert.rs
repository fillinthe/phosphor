//! Convert a movie into a format that plays well in a show: Hap (smooth on any computer, Mac or Windows, plays
//! backwards perfectly; big, no sound), ProRes (smooth, reversible, keeps sound, very big) or H.264 with every frame
//! a keyframe (reverses smoothly, keeps sound, much smaller). You choose how much to compress and an optional
//! smaller size, and see the size it will take before converting. Runs on a background thread with the bundled
//! ffmpeg (Hap frames go through Phosphor's own encoder, the one recording uses).

use std::io::{BufRead, Read};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use ffmpeg_next as ff;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Hap,
    HapAlpha,
    ProRes,
    H264Keyframes,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Hap, Format::HapAlpha, Format::ProRes, Format::H264Keyframes];

    pub fn name(self) -> &'static str {
        match self {
            Format::Hap => "Hap",
            Format::HapAlpha => "Hap Alpha",
            Format::ProRes => "ProRes",
            Format::H264Keyframes => "H.264, every frame a keyframe",
        }
    }

    pub fn about(self) -> &'static str {
        match self {
            Format::Hap => "Smoothest on any computer (Mac or Windows): the graphics card decodes it. Plays backwards and jumps instantly. Big files; no sound.",
            Format::HapAlpha => "Hap that keeps transparency (for overlays). Bigger than Hap; no sound.",
            Format::ProRes => "Smooth, plays backwards well, keeps the sound. Very big files.",
            Format::H264Keyframes => "Plays backwards smoothly and keeps the sound, at a fraction of the size of the others. Uses a bit more of the computer than Hap.",
        }
    }

    /// The bit added to the file name ("clip (Hap).mov").
    fn suffix(self) -> &'static str {
        match self {
            Format::Hap => "Hap",
            Format::HapAlpha => "Hap Alpha",
            Format::ProRes => "ProRes",
            Format::H264Keyframes => "keyframes",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Format::H264Keyframes => "mp4",
            _ => "mov",
        }
    }

    pub fn has_quality(self) -> bool {
        matches!(self, Format::ProRes | Format::H264Keyframes)
    }
}

/// How hard to compress (ProRes Proxy / LT / 422; H.264 bitrate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Quality {
    Small,
    #[default]
    Balanced,
    Best,
}

impl Quality {
    pub const ALL: [Quality; 3] = [Quality::Small, Quality::Balanced, Quality::Best];

    pub fn name(self) -> &'static str {
        match self {
            Quality::Small => "Smaller file",
            Quality::Balanced => "Balanced",
            Quality::Best => "Best picture",
        }
    }
}

/// Optional smaller size (the picture fits inside this box, shape kept).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Size {
    #[default]
    Original,
    Fit1080,
    Fit720,
}

impl Size {
    pub const ALL: [Size; 3] = [Size::Original, Size::Fit1080, Size::Fit720];

    pub fn name(self) -> &'static str {
        match self {
            Size::Original => "Keep its size",
            Size::Fit1080 => "1080p at most",
            Size::Fit720 => "720p at most",
        }
    }

    /// The output size for a `w`×`h` picture (even numbers; never larger than the original).
    pub fn apply(self, w: u32, h: u32) -> (u32, u32) {
        let (long, short) = match self {
            Size::Original => return (w & !1, h & !1),
            Size::Fit1080 => (1920.0, 1080.0),
            Size::Fit720 => (1280.0, 720.0),
        };
        let (bw, bh) = if w >= h { (long, short) } else { (short, long) };
        let s = (bw / w as f32).min(bh / h as f32).min(1.0);
        (((w as f32 * s).round() as u32) & !1, ((h as f32 * s).round() as u32) & !1)
    }
}

/// What a movie is, for the estimate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub seconds: f64,
    pub bytes: u64,
    pub codec: String,
    pub has_audio: bool,
}

/// Reads a movie's size, rate and length.
pub fn probe(path: &str) -> Result<Probe, String> {
    let input = ff::format::input(&path).map_err(|e| format!("can't open ({e})"))?;
    let video = input.streams().best(ff::media::Type::Video).ok_or("no video in this file")?;
    let params = video.parameters();
    // SAFETY: plain fields of the stream's codec parameters.
    let (width, height) = unsafe { ((*params.as_ptr()).width as u32, (*params.as_ptr()).height as u32) };
    let rate = video.avg_frame_rate();
    let fps = if rate.denominator() > 0 && rate.numerator() > 0 { rate.numerator() as f64 / rate.denominator() as f64 } else { 30.0 };
    let seconds = if input.duration() > 0 { input.duration() as f64 / f64::from(ff::ffi::AV_TIME_BASE) } else { 0.0 };
    let codec = ff::codec::decoder::find(params.id()).map_or_else(|| format!("{:?}", params.id()), |c| c.name().to_string());
    let has_audio = input.streams().best(ff::media::Type::Audio).is_some();
    let bytes = std::fs::metadata(path).map_or(0, |m| m.len());
    Ok(Probe { width, height, fps, seconds, bytes, codec, has_audio })
}

/// About how many bytes the converted file will take.
pub fn estimate(p: &Probe, format: Format, quality: Quality, size: Size) -> u64 {
    let (w, h) = size.apply(p.width, p.height);
    let pixels_per_second = w as f64 * h as f64 * p.fps.max(1.0);
    // Bits per pixel per frame: Hap = DXT1 4 bits (Snappy saves a little), Hap Alpha = DXT5 8 bits;
    // ProRes Proxy / LT / 422 and the H.264 bitrates we ask for.
    let bits = match (format, quality) {
        // Hap: Snappy squeezes DXT anywhere from about a third to most of its size depending on the picture.
        (Format::Hap, _) => 4.0 * 0.6,
        (Format::HapAlpha, _) => 8.0 * 0.6,
        (Format::ProRes, Quality::Small) => 0.72,
        (Format::ProRes, Quality::Balanced) => 1.64,
        (Format::ProRes, Quality::Best) => 2.36,
        (Format::H264Keyframes, q) => h264_bits(q),
    };
    let audio = if p.has_audio && !matches!(format, Format::Hap | Format::HapAlpha) { 192_000.0 / 8.0 * p.seconds } else { 0.0 };
    (pixels_per_second * bits / 8.0 * p.seconds + audio) as u64
}

fn h264_bits(q: Quality) -> f64 {
    match q {
        Quality::Small => 0.2,
        Quality::Balanced => 0.4,
        Quality::Best => 0.8,
    }
}

/// Where the converted file goes: next to the original ("clip (Hap).mov"), never over an existing file.
pub fn output_path(src: &str, format: Format) -> String {
    let p = std::path::Path::new(src);
    let stem = p.file_stem().map_or_else(|| "clip".into(), |s| s.to_string_lossy().into_owned());
    let dir = p.parent().map(|d| d.to_path_buf()).unwrap_or_default();
    let mut n = 1;
    loop {
        let name = if n == 1 { format!("{stem} ({}).{}", format.suffix(), format.extension()) } else { format!("{stem} ({} {n}).{}", format.suffix(), format.extension()) };
        let out = dir.join(name);
        if !out.exists() {
            return out.to_string_lossy().into_owned();
        }
        n += 1;
    }
}

/// Free space on the disk holding `path` (None where it can't be told).
pub fn free_space(path: &str) -> Option<u64> {
    #[cfg(unix)]
    {
        let dir = std::path::Path::new(path).parent()?;
        let c = std::ffi::CString::new(dir.to_string_lossy().as_bytes()).ok()?;
        let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
        // SAFETY: statvfs fills the struct for an existing path.
        if unsafe { libc::statvfs(c.as_ptr(), &mut s) } == 0 {
            return Some(s.f_bavail as u64 * s.f_frsize as u64);
        }
        None
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// A conversion's progress, read by the window.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    /// 0..1.
    pub done: f32,
    pub finished: bool,
    pub error: Option<String>,
    pub output: String,
}

pub type Shared = Arc<Mutex<Progress>>;

/// One conversion to run.
#[derive(Clone, Debug)]
pub struct Job {
    pub src: String,
    pub format: Format,
    pub quality: Quality,
    pub size: Size,
    pub probe: Probe,
}

/// Starts converting on a background thread.
pub fn start(job: Job) -> Shared {
    let shared: Shared = Arc::new(Mutex::new(Progress { output: output_path(&job.src, job.format), ..Default::default() }));
    let s = shared.clone();
    std::thread::Builder::new()
        .name("phosphor-convert".into())
        .spawn(move || {
            let out = s.lock().unwrap().output.clone();
            let result = run(&job, &out, &s);
            let mut p = s.lock().unwrap();
            p.finished = true;
            match result {
                Ok(()) => p.done = 1.0,
                Err(e) => {
                    let _ = std::fs::remove_file(&out);
                    p.error = Some(e);
                }
            }
        })
        .expect("convert thread");
    shared
}

fn run(job: &Job, out: &str, s: &Shared) -> Result<(), String> {
    let ffmpeg = crate::stream::ffmpeg_path().ok_or("FFmpeg isn't available")?;
    let (w, h) = job.size.apply(job.probe.width, job.probe.height);
    let total = (job.probe.seconds * job.probe.fps).max(1.0);
    let scale = format!("scale={w}:{h}:flags=lanczos");
    match job.format {
        Format::Hap | Format::HapAlpha => {
            // Decode with ffmpeg to RGBA frames, encode them with Phosphor's Hap encoder.
            let alpha = job.format == Format::HapAlpha;
            let fps = job.probe.fps.round().max(1.0) as u32;
            let mut child = Command::new(&ffmpeg)
                .args(["-hide_banner", "-loglevel", "error", "-i", &job.src, "-an", "-vf", &scale, "-f", "rawvideo", "-pix_fmt", "rgba", "-"])
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("couldn't start FFmpeg ({e})"))?;
            let mut stdout = child.stdout.take().ok_or("no output from FFmpeg")?;
            let mut writer = crate::media::video::hap_encode::HapWriter::create(out, w, h, fps, alpha)?;
            let mut frame = vec![0u8; w as usize * h as usize * 4];
            let mut n = 0u64;
            while stdout.read_exact(&mut frame).is_ok() {
                writer.write(&frame)?;
                n += 1;
                s.lock().unwrap().done = (n as f64 / total).min(0.99) as f32;
            }
            let _ = child.wait();
            if n == 0 {
                return Err("no frames could be read from this file".into());
            }
            writer.finish()
        }
        Format::ProRes | Format::H264Keyframes => {
            let mut args: Vec<String> = ["-hide_banner", "-loglevel", "error", "-nostats", "-progress", "pipe:1", "-y", "-i", &job.src, "-vf", &scale]
                .iter()
                .map(|a| a.to_string())
                .collect();
            if job.format == Format::ProRes {
                let profile = match job.quality {
                    Quality::Small => "0",
                    Quality::Balanced => "1",
                    Quality::Best => "2",
                };
                // The Mac's encoder; elsewhere FFmpeg's own.
                let encoder = if cfg!(target_os = "macos") { "prores_videotoolbox" } else { "prores_ks" };
                args.extend(["-c:v", encoder, "-profile:v", profile, "-pix_fmt", "yuv422p10le"].map(String::from));
            } else {
                let kbps = (w as f64 * h as f64 * job.probe.fps * h264_bits(job.quality) / 1000.0) as u64;
                let encoder = if cfg!(target_os = "macos") { "h264_videotoolbox" } else { "libx264" };
                args.extend(
                    ["-c:v", encoder, "-b:v", &format!("{kbps}k"), "-g", "1", "-bf", "0", "-pix_fmt", "yuv420p", "-movflags", "+faststart"]
                        .map(String::from),
                );
            }
            if job.probe.has_audio {
                args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from));
            }
            args.push(out.to_string());
            let mut child = Command::new(&ffmpeg).args(&args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("couldn't start FFmpeg ({e})"))?;
            if let Some(stdout) = child.stdout.take() {
                for line in std::io::BufReader::new(stdout).lines().map_while(Result::ok) {
                    if let Some(us) = line.strip_prefix("out_time_us=").and_then(|v| v.trim().parse::<f64>().ok()) {
                        s.lock().unwrap().done = (us / 1e6 / job.probe.seconds.max(0.1)).clamp(0.0, 0.99) as f32;
                    }
                }
            }
            let status = child.wait().map_err(|e| e.to_string())?;
            if status.success() {
                Ok(())
            } else {
                let mut err = String::new();
                if let Some(mut e) = child.stderr.take() {
                    let _ = e.read_to_string(&mut err);
                }
                Err(format!("FFmpeg stopped: {}", err.lines().last().unwrap_or("unknown error")))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_estimates() {
        assert_eq!(Size::Fit1080.apply(1080, 1920), (1080, 1920), "portrait 1080 stays");
        assert_eq!(Size::Fit720.apply(1080, 1920), (720, 1280));
        assert_eq!(Size::Fit720.apply(3840, 2160), (1280, 720));
        assert_eq!(Size::Fit1080.apply(640, 360), (640, 360), "never enlarged");
        let p = Probe { width: 1920, height: 1080, fps: 30.0, seconds: 10.0, bytes: 0, codec: "h264".into(), has_audio: false };
        let hap = estimate(&p, Format::Hap, Quality::Balanced, Size::Original);
        let h264 = estimate(&p, Format::H264Keyframes, Quality::Balanced, Size::Original);
        assert!(hap > 150_000_000 && hap < 220_000_000, "{hap}");
        assert!(h264 < hap / 5, "{h264}");
        assert!(estimate(&p, Format::Hap, Quality::Balanced, Size::Fit720) < hap / 2);
    }

    #[test]
    fn output_lands_next_to_the_original() {
        assert!(output_path("/tmp/does-not-exist/clip.mp4", Format::Hap).ends_with("clip (Hap).mov"));
        assert!(output_path("/tmp/does-not-exist/clip.mp4", Format::H264Keyframes).ends_with("clip (keyframes).mp4"));
    }
}

#[cfg(test)]
mod real_tests {
    use super::*;

    /// Converts a real movie (when PHOSPHOR_CONVERT_TEST points at one) into every format, in a temp folder, and
    /// prints size vs estimate.
    #[test]
    fn converts_a_real_movie() {
        let Ok(src) = std::env::var("PHOSPHOR_CONVERT_TEST") else { return };
        let dir = std::env::temp_dir().join(format!("phosphor-convert-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let copy = dir.join("clip.mp4");
        std::fs::copy(&src, &copy).unwrap();
        let copy = copy.to_string_lossy().into_owned();
        let probe = probe(&copy).unwrap();
        for format in [Format::Hap, Format::H264Keyframes, Format::ProRes] {
            let t = std::time::Instant::now();
            let job = Job { src: copy.clone(), format, quality: Quality::Balanced, size: Size::Original, probe: probe.clone() };
            let est = estimate(&probe, format, job.quality, job.size);
            let p = start(job);
            loop {
                let g = p.lock().unwrap().clone();
                if g.finished {
                    assert!(g.error.is_none(), "{format:?}: {:?}", g.error);
                    let size = std::fs::metadata(&g.output).unwrap().len();
                    println!("CONVERT {format:?}: {} MB (estimate {} MB) in {:.1}s -> {}", size >> 20, est >> 20, t.elapsed().as_secs_f32(), g.output);
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(200));
            }
        }
    }
}
