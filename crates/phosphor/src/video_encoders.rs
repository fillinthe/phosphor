//! Which FFmpeg video encoder streams, recordings, renders and the Session's WebRTC picture use on this computer.
//! The Mac always has VideoToolbox. Windows and Linux have whatever their graphics card and FFmpeg build offer: each
//! hardware encoder FFmpeg lists is tried once with a tiny test picture (listed is not the same as usable: an NVIDIA
//! encoder is in every FFmpeg build, the card isn't in every PC), the first that works is kept for the run, and FFmpeg's
//! own software encoders are the fallback. The look happens on a thread of its own, started by `warm`; until it's in,
//! the software encoder is used (it always works), so nothing ever waits for it.

use std::process::Stdio;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Codec {
    H264,
    Hevc,
    ProRes,
}

impl Codec {
    const ALL: [Codec; 3] = [Codec::H264, Codec::Hevc, Codec::ProRes];

    fn index(self) -> usize {
        match self {
            Codec::H264 => 0,
            Codec::Hevc => 1,
            Codec::ProRes => 2,
        }
    }

    fn words(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::ProRes => "ProRes",
        }
    }
}

/// One FFmpeg encoder and how Phosphor drives it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoder {
    /// FFmpeg's name (`-c:v`).
    pub name: &'static str,
    /// Who does the work, in words ("the graphics card (NVIDIA)", "the processor").
    pub by: &'static str,
    /// The picture layout it takes (some hardware encoders refuse yuv420p).
    pub pix_fmt: &'static str,
    /// Speed settings for a live encode (always given, also to the test encode).
    pub live: &'static [&'static str],
    /// What makes it H.264 Baseline with no B-frames (WebRTC players need that); None = it can't be told reliably.
    pub baseline: Option<&'static [&'static str]>,
}

impl Encoder {
    /// "H.264 on the graphics card (NVIDIA)".
    pub fn describe(&self, codec: Codec) -> String {
        format!("{} on {}", codec.words(), self.by)
    }
}

const SOFTWARE: &str = "the processor";
const YUV: &str = "yuv420p";
const NV12: &str = "nv12";
const PRORES_PIX: &str = "yuv422p10le";

const fn enc(name: &'static str, by: &'static str, pix_fmt: &'static str, live: &'static [&'static str], baseline: Option<&'static [&'static str]>) -> Encoder {
    Encoder { name, by, pix_fmt, live, baseline }
}

const BASELINE: Option<&[&str]> = Some(&["-profile:v", "baseline", "-bf", "0"]);

const VT_H264: Encoder = enc("h264_videotoolbox", "the graphics chip (VideoToolbox)", YUV, &[], BASELINE);
const VT_HEVC: Encoder = enc("hevc_videotoolbox", "the graphics chip (VideoToolbox)", YUV, &[], None);
const VT_PRORES: Encoder = enc("prores_videotoolbox", "the graphics chip (VideoToolbox)", PRORES_PIX, &[], None);
const NVENC_H264: Encoder = enc("h264_nvenc", "the graphics card (NVIDIA)", YUV, &["-preset", "p4"], BASELINE);
const NVENC_HEVC: Encoder = enc("hevc_nvenc", "the graphics card (NVIDIA)", YUV, &["-preset", "p4"], None);
const QSV_H264: Encoder = enc("h264_qsv", "the graphics chip (Intel Quick Sync)", NV12, &["-preset", "veryfast"], BASELINE);
const QSV_HEVC: Encoder = enc("hevc_qsv", "the graphics chip (Intel Quick Sync)", NV12, &["-preset", "veryfast"], None);
const AMF_H264: Encoder = enc("h264_amf", "the graphics card (AMD)", NV12, &["-quality", "balanced"], Some(&["-profile:v", "constrained_baseline", "-bf", "0"]));
const AMF_HEVC: Encoder = enc("hevc_amf", "the graphics card (AMD)", NV12, &["-quality", "balanced"], None);
// Media Foundation: Windows' own encoder layer, asked for the hardware one (the test fails where there's none).
const MF_H264: Encoder = enc("h264_mf", "the graphics card (Windows Media Foundation)", NV12, &["-hw_encoding", "1"], None);
const MF_HEVC: Encoder = enc("hevc_mf", "the graphics card (Windows Media Foundation)", NV12, &["-hw_encoding", "1"], None);
// Raspberry Pi 4 and other boards with a Video4Linux encoder (a Pi 5 has none: its test fails).
const V4L2_H264: Encoder = enc("h264_v4l2m2m", "the video chip (V4L2)", YUV, &[], None);
const V4L2_HEVC: Encoder = enc("hevc_v4l2m2m", "the video chip (V4L2)", YUV, &[], None);
const X264: Encoder = enc("libx264", SOFTWARE, YUV, &["-preset", "veryfast"], Some(&["-profile:v", "baseline", "-bf", "0", "-tune", "zerolatency"]));
const OPENH264: Encoder = enc("libopenh264", SOFTWARE, YUV, &[], Some(&["-profile:v", "constrained_baseline", "-bf", "0"]));
const X265: Encoder = enc("libx265", SOFTWARE, YUV, &["-preset", "veryfast"], None);
const PRORES_KS: Encoder = enc("prores_ks", SOFTWARE, PRORES_PIX, &[], None);

/// The hardware encoders worth trying on this system, best first.
fn hardware_candidates(codec: Codec) -> &'static [Encoder] {
    if cfg!(target_os = "macos") {
        return &[];
    }
    match codec {
        Codec::H264 if cfg!(windows) => &[NVENC_H264, QSV_H264, AMF_H264, MF_H264],
        Codec::Hevc if cfg!(windows) => &[NVENC_HEVC, QSV_HEVC, AMF_HEVC, MF_HEVC],
        Codec::H264 => &[NVENC_H264, V4L2_H264],
        Codec::Hevc => &[NVENC_HEVC, V4L2_HEVC],
        Codec::ProRes => &[],
    }
}

/// FFmpeg's software encoders, used when no hardware one works (first one this FFmpeg has wins).
fn software_candidates(codec: Codec) -> &'static [Encoder] {
    match codec {
        Codec::H264 => &[X264, OPENH264],
        Codec::Hevc => &[X265],
        Codec::ProRes => &[PRORES_KS],
    }
}

/// The Mac's encoders: always there, never tested.
fn mac_encoder(codec: Codec) -> Encoder {
    match codec {
        Codec::H264 => VT_H264,
        Codec::Hevc => VT_HEVC,
        Codec::ProRes => VT_PRORES,
    }
}

/// What the look found, per codec: (hardware that worked, software this FFmpeg has).
#[derive(Clone, Debug, Default)]
struct Found {
    hardware: [Option<Encoder>; 3],
    software: [Option<Encoder>; 3],
}

static FOUND: OnceLock<Found> = OnceLock::new();
static LOOKING: AtomicBool = AtomicBool::new(false);

/// Starts the look for this computer's encoders (once; nothing on the Mac). Cheap to call every frame.
pub fn warm() {
    if cfg!(target_os = "macos") || FOUND.get().is_some() || LOOKING.swap(true, Ordering::SeqCst) {
        return;
    }
    let started = std::thread::Builder::new().name("video-encoders".into()).spawn(|| {
        let found = crate::stream::ffmpeg_path().map(|ffmpeg| look(&ffmpeg)).unwrap_or_default();
        for codec in Codec::ALL {
            let i = codec.index();
            log::info!(
                "{} encoder: {}",
                codec.words(),
                found.hardware[i].or(found.software[i]).map_or_else(|| "none in this FFmpeg".to_string(), |e| format!("{} ({})", e.name, e.by))
            );
        }
        let _ = FOUND.set(found);
    });
    if started.is_err() {
        LOOKING.store(false, Ordering::SeqCst);
    }
}

/// Lists what this FFmpeg can encode, then test-encodes the hardware candidates it lists.
fn look(ffmpeg: &str) -> Found {
    let listed = crate::os::command(ffmpeg).args(["-hide_banner", "-encoders"]).stdin(Stdio::null()).stderr(Stdio::null()).output();
    let names = listed.map(|o| encoder_names(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    let mut found = Found::default();
    for codec in Codec::ALL {
        let i = codec.index();
        found.software[i] = software_candidates(codec).iter().find(|e| names.iter().any(|n| n == e.name)).copied();
        // (With the rate settings a stream uses: an encoder that refuses them must not be picked.)
        let rate = ["-b:v", "2000k", "-maxrate", "2000k", "-bufsize", "4000k", "-g", "60"];
        found.hardware[i] = hardware_candidates(codec).iter().filter(|e| names.iter().any(|n| n == e.name)).find(|e| works(ffmpeg, e, &rate)).copied();
    }
    found
}

/// The encoder names in `ffmpeg -encoders`' answer (" V....D h264_nvenc   NVIDIA NVENC H.264 encoder").
fn encoder_names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let mut words = l.split_whitespace();
            let flags = words.next()?;
            let name = words.next()?;
            (flags.len() == 6 && flags.starts_with('V') && name != "=").then(|| name.to_string())
        })
        .collect()
}

/// How long a test encode may take (a driver that hangs must not keep the look waiting).
const TEST_SECS: u64 = 10;

/// Does `e` encode a few frames of a small picture here, with the settings it's used with (and `extra`)?
fn works(ffmpeg: &str, e: &Encoder, extra: &[&str]) -> bool {
    let mut args: Vec<&str> = vec!["-hide_banner", "-loglevel", "error", "-f", "lavfi", "-i", "color=black:s=320x240:r=30", "-frames:v", "5", "-c:v", e.name];
    args.extend(e.live);
    args.extend(extra);
    args.extend(["-pix_fmt", e.pix_fmt, "-f", "null", "-"]);
    let Ok(mut child) = crate::os::command(ffmpeg).args(&args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    let until = Instant::now() + Duration::from_secs(TEST_SECS);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("{} didn't finish a test encode in {TEST_SECS} s: not used", e.name);
                return false;
            }
        }
    }
}

/// The encoder to use now for a live encode (stream, recording, render). Never waits: until the look is in, the
/// software encoder (Mac: VideoToolbox). `baseline` = only one that can make H.264 Baseline (WebRTC).
pub fn pick(codec: Codec, baseline: bool) -> Encoder {
    if cfg!(target_os = "macos") {
        return mac_encoder(codec);
    }
    warm();
    let i = codec.index();
    let fits = |e: &Encoder| !baseline || e.baseline.is_some();
    let found = FOUND.get();
    found
        .and_then(|f| f.hardware[i].filter(fits))
        .or_else(|| found.and_then(|f| f.software[i].filter(fits)))
        .unwrap_or_else(|| software_candidates(codec).iter().copied().find(fits).unwrap_or(software_candidates(codec)[0]))
}

/// The encoder for an offline job (file conversion): VideoToolbox on the Mac, else FFmpeg's software encoder
/// (it has every setting, and nothing waits on its speed).
pub fn offline(codec: Codec) -> Encoder {
    if cfg!(target_os = "macos") { mac_encoder(codec) } else { software_candidates(codec)[0] }
}

/// What a live `codec` encode runs on, for the panels (the Mac's says it in words of its own) ("H.264 on the graphics card (NVIDIA)"; "…" while looking).
#[cfg_attr(target_os = "macos", allow(dead_code))]
pub fn describe(codec: Codec) -> String {
    if !cfg!(target_os = "macos") && FOUND.get().is_none() {
        warm();
        return format!("{} encoder: looking…", codec.words());
    }
    pick(codec, false).describe(codec)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_encoder_list_is_read_from_ffmpeg() {
        let text = "Encoders:\n V..... = Video\n A..... = Audio\n ------\n V....D libx264              libx264 H.264\n V....D h264_nvenc           NVIDIA NVENC H.264 encoder (codec h264)\n VFS... prores_ks            Apple ProRes\n A....D aac                  AAC\n";
        assert_eq!(encoder_names(text), ["libx264", "h264_nvenc", "prores_ks"]);
    }

    #[test]
    fn every_codec_has_a_software_fallback_and_baseline_h264() {
        for codec in Codec::ALL {
            assert!(!software_candidates(codec).is_empty(), "{codec:?}");
            assert_eq!(software_candidates(codec)[0].by, SOFTWARE, "{codec:?}");
        }
        assert!(software_candidates(Codec::H264).iter().any(|e| e.baseline.is_some()), "WebRTC needs a Baseline H.264");
        // Never the Mac's encoders elsewhere, never another system's on the Mac.
        for codec in Codec::ALL {
            for e in hardware_candidates(codec) {
                assert!(!e.name.contains("videotoolbox"));
                assert!(cfg!(windows) || !e.name.ends_with("_mf"), "{}", e.name);
            }
        }
        assert_eq!(pick(Codec::ProRes, false).pix_fmt, "yuv422p10le");
        assert_eq!(VT_H264.describe(Codec::H264), "H.264 on the graphics chip (VideoToolbox)");
    }

    /// The test encode, run with a real FFmpeg when one is here: an encoder that doesn't exist fails, FFmpeg's own works.
    #[test]
    fn a_test_encode_tells_working_encoders_from_missing_ones() {
        let Some(ffmpeg) = crate::stream::ffmpeg_path() else { return };
        let ghost = enc("h264_not_a_real_encoder", "nothing", YUV, &[], None);
        assert!(!works(&ffmpeg, &ghost, &[]));
        let listed = crate::os::command(&ffmpeg).args(["-hide_banner", "-encoders"]).output().unwrap();
        let names = encoder_names(&String::from_utf8_lossy(&listed.stdout));
        for e in [X264, X265, PRORES_KS, OPENH264].into_iter().filter(|e| names.iter().any(|n| n == e.name)) {
            assert!(works(&ffmpeg, &e, &[]), "{} is listed and should encode with its live settings", e.name);
            if let Some(baseline) = e.baseline {
                assert!(works(&ffmpeg, &e, baseline), "{} should make Baseline H.264", e.name);
            }
        }
    }
}
