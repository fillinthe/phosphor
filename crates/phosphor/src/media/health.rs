//! Will this movie play smoothly, and if not, what to do about it. `advise` turns what's known about a movie (what
//! it's made of, where it lives, how its playback is going right now) into plain warnings, each with a next step:
//! convert it (and to what), copy it onto this computer, make it smaller. The layer card shows a warning sign from
//! it, and File info lists it in full.

/// Where a file lives.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Place {
    /// This computer's own disk.
    Here,
    /// A drive plugged into this computer.
    External,
    /// Another computer or a NAS, over the network.
    Network,
    #[default]
    Unknown,
}

/// What's known about a movie. Leave out what isn't (0 / None): the checks that need it are skipped.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub codec: String,
    /// The graphics card decodes it (None: not played yet, so not known).
    pub hardware: Option<bool>,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    /// Megabits a second on disk.
    pub mbps: Option<f64>,
    pub decoded: u64,
    /// Frames thrown away because they came too late to show.
    pub skipped: u64,
    /// Backwards play has had to wait for the decoder.
    pub reverse_slow: bool,
    /// Its decoding's share of one CPU core (0..1+).
    pub cpu: Option<f32>,
    pub place: Place,
    pub has_audio: bool,
    /// The project's picture size.
    pub canvas: (u32, u32),
}

/// One thing worth knowing about a movie.
#[derive(Clone, Debug, PartialEq)]
pub struct Advice {
    /// It stutters now, or is likely to in a show (the card shows its warning sign). Otherwise a tip.
    pub serious: bool,
    pub what: String,
    pub try_this: String,
}

/// Codecs the graphics card plays as they are: nothing to decode on the CPU.
fn is_hap(codec: &str) -> bool {
    codec.to_lowercase().contains("hap")
}

/// Codecs where every frame stands alone: jumping, scrubbing and playing backwards are instant.
fn every_frame_alone(codec: &str) -> bool {
    let c = codec.to_lowercase();
    ["hap", "prores", "dnxh", "mjpeg", "png", "qtrle", "utvideo", "huffyuv", "ffv1", "cfhd", "dirac", "jpeg2000", "gif"].iter().any(|k| c.contains(k))
}

/// Codecs that can carry transparency (their conversion should keep it).
fn may_have_alpha(codec: &str) -> bool {
    let c = codec.to_lowercase();
    c.contains("4444") || c.contains("alpha") || ["png", "qtrle", "vp8", "vp9", "gif", "webp"].iter().any(|k| c.contains(k))
}

/// The conversion that suits this movie best, as (the name in Convert…, why).
pub fn best_conversion(f: &Facts) -> (&'static str, &'static str) {
    if may_have_alpha(&f.codec) {
        ("Hap Alpha", "it keeps the transparency and the graphics card plays it")
    } else if f.has_audio {
        ("H.264, every frame a keyframe", "it keeps the sound, and jumps and plays backwards cleanly (Hap is lighter still but has no sound)")
    } else {
        ("Hap", "the graphics card plays it, so it costs almost no CPU, and it jumps and plays backwards instantly (Hap Q if you see banding in gradients)")
    }
}

const CONVERT: &str = "Media › right-click the file › Convert…";
/// Getting a file onto this computer: Convert… does it in one step (its Saves to), or a plain copy in the Finder.
const LOCAL: &str = "copy it onto this computer's own disk and use that copy (Convert… does both at once: set its Saves to to a folder on this computer)";

/// What to know about this movie, the serious things first. Empty = it should play fine.
pub fn advise(f: &Facts) -> Vec<Advice> {
    let mut out = Vec::new();
    let (to, why) = best_conversion(f);
    let convert = format!("{CONVERT} › {to}: {why}.");
    let pixels = f.width as f64 * f.height as f64;
    // 1. It's dropping frames right now.
    if f.decoded >= 90 && f.skipped as f64 > f.decoded as f64 * 0.04 {
        let share = (f.skipped as f64 / f.decoded as f64 * 100.0).round();
        let what = format!("It's dropping frames: {} of the last {} came too late to show ({share:.0}%), so it stutters.", f.skipped, f.decoded);
        let try_this = match f.place {
            Place::Network => format!("It plays from a network share, which is the likeliest cause: {LOCAL}. If it still stutters, {convert}"),
            _ if f.hardware == Some(false) || f.cpu.is_some_and(|c| c >= 0.5) => convert.clone(),
            Place::External => format!("It plays from an external drive: {LOCAL}. Or {convert}"),
            _ => convert.clone(),
        };
        out.push(Advice { serious: true, what, try_this });
    }
    // 2. The CPU is working hard on it.
    if let Some(c) = f.cpu.filter(|c| *c >= 0.5)
        && !is_hap(&f.codec)
    {
        out.push(Advice { serious: c >= 0.7, what: format!("Decoding it takes {:.0}% of one CPU core. A few like this at once and the show stutters.", c * 100.0), try_this: convert.clone() });
    } else if f.hardware == Some(false) && !is_hap(&f.codec) && pixels * f.fps.max(24.0) > 1920.0 * 1080.0 * 31.0 {
        // 3. Big and decoded by the CPU, though it hasn't been measured as heavy yet.
        out.push(Advice { serious: false, what: format!("The CPU decodes it, not the graphics card ({} at {}×{}).", f.codec, f.width, f.height), try_this: convert.clone() });
    }
    // 4. Backwards play waits.
    if f.reverse_slow {
        out.push(Advice { serious: true, what: "Playing it backwards has to wait for the decoder: it has few keyframes, so going back means decoding a whole group of frames first.".into(), try_this: convert.clone() });
    }
    // 5. Where it lives.
    let heavy_read = f.mbps.is_some_and(|m| m > 200.0);
    match f.place {
        Place::Network if !out.iter().any(|a| a.try_this.contains("network share")) => out.push(Advice {
            serious: true,
            what: "It plays from a network share. One slow moment on the network and the picture freezes mid-show.".into(),
            try_this: format!("Before the show, {LOCAL}."),
        }),
        Place::External if heavy_read => out.push(Advice {
            serious: false,
            what: format!("It reads {:.0} megabits a second from an external drive. A slow, sleeping or busy drive can't keep that up.", f.mbps.unwrap_or(0.0)),
            try_this: format!("Play it from a fast SSD on its own cable, or {LOCAL}."),
        }),
        _ => {}
    }
    // 6. Far bigger than the show.
    let canvas = f.canvas.0 as f64 * f.canvas.1 as f64;
    if canvas > 0.0 && pixels > canvas * 2.5 {
        out.push(Advice {
            serious: false,
            what: format!("It's {}×{}, much bigger than the project's {}×{}: most of that detail is decoded and thrown away.", f.width, f.height, f.canvas.0, f.canvas.1),
            try_this: format!("{CONVERT} with Size set to the project's size: lighter to play, and it looks the same."),
        });
    }
    // 7. Fine forwards; a tip for people who scrub, jump or reverse.
    if out.is_empty() && f.width > 0 && !every_frame_alone(&f.codec) {
        out.push(Advice {
            serious: false,
            what: format!("It should play smoothly forwards. {} keeps only some frames whole, so scrubbing, jumping around and playing backwards are slower than they could be.", f.codec),
            try_this: format!("If you do those live: {convert}"),
        });
    }
    out.sort_by_key(|a| !a.serious);
    out
}

/// Every advice as text: one "what" line and one "do this" line each.
pub fn as_text(advice: &[Advice]) -> String {
    advice.iter().map(|a| format!("{} {}\n    Do this: {}", if a.serious { "(!)" } else { "(i)" }, a.what, a.try_this)).collect::<Vec<_>>().join("\n")
}

/// Where `path` lives, from the list of mounted drives (asked for without waiting on any of them, so a network
/// share that has gone quiet can't hold this up; the list is looked at again every few seconds).
pub fn place_of(path: &str) -> Place {
    if path.starts_with("\\\\") {
        return Place::Network;
    }
    #[cfg(target_os = "macos")]
    {
        place_among(&mounts(), path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        Place::Unknown
    }
}

/// (mount point, on this computer's own hardware, the startup disk or part of it)
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
type Mount = (String, bool, bool);

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn place_among(mounts: &[Mount], path: &str) -> Place {
    let inside = |m: &str| path == m || m == "/" || path.strip_prefix(m).is_some_and(|rest| rest.starts_with('/'));
    match mounts.iter().filter(|(m, ..)| inside(m)).max_by_key(|(m, ..)| m.len()) {
        Some((_, false, _)) => Place::Network,
        Some((_, true, true)) => Place::Here,
        Some((_, true, false)) => Place::External,
        None => Place::Unknown,
    }
}

#[cfg(target_os = "macos")]
fn mounts() -> Vec<Mount> {
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};
    static CACHE: OnceLock<Mutex<(Instant, Vec<Mount>)>> = OnceLock::new();
    // (None = never looked yet.)
    static LOOKED: OnceLock<Mutex<Option<Instant>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new((Instant::now(), Vec::new())));
    let Ok(mut c) = cache.lock() else { return Vec::new() };
    let fresh = LOOKED.get_or_init(|| Mutex::new(None)).lock().ok().is_some_and(|l| l.is_some_and(|at| at.elapsed() <= Duration::from_secs(8)));
    if !fresh {
        if let Ok(mut l) = LOOKED.get_or_init(|| Mutex::new(None)).lock() {
            *l = Some(Instant::now());
        }
        let mut list = Vec::new();
        // SAFETY: getfsstat fills at most `size` bytes of our buffer; MNT_NOWAIT returns what the kernel already
        // knows without asking any drive, so it can't block on a dead share.
        unsafe {
            let n = libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT);
            if n > 0 {
                let mut buf: Vec<libc::statfs> = Vec::with_capacity(n as usize + 4);
                let got = libc::getfsstat(buf.as_mut_ptr(), (buf.capacity() * std::mem::size_of::<libc::statfs>()) as libc::c_int, libc::MNT_NOWAIT);
                if got > 0 {
                    buf.set_len((got as usize).min(buf.capacity()));
                    for s in &buf {
                        let at = std::ffi::CStr::from_ptr(s.f_mntonname.as_ptr()).to_string_lossy().into_owned();
                        // Another computer's disk: the network file systems by name. (Not "anything that isn't
                        // local": a cloud-drive or encrypted-folder mount isn't a share that drops out mid-show.)
                        let kind = std::ffi::CStr::from_ptr(s.f_fstypename.as_ptr()).to_string_lossy().to_lowercase();
                        let local = !["smbfs", "nfs", "afpfs", "webdav", "cifs", "ftp"].iter().any(|k| kind.starts_with(k));
                        // The startup disk and its own volumes sit at / or under /System; plugged-in drives under /Volumes.
                        let own = at == "/" || at.starts_with("/System/") || at.starts_with("/private/");
                        list.push((at, local, own));
                    }
                }
            }
        }
        *c = (Instant::now(), list);
    }
    c.1.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movie() -> Facts {
        Facts { codec: "h264".into(), hardware: Some(true), width: 1920, height: 1080, fps: 30.0, mbps: Some(12.0), decoded: 600, canvas: (1920, 1080), place: Place::Here, ..Default::default() }
    }

    #[test]
    fn a_healthy_movie_gets_a_tip_and_hap_gets_nothing() {
        let a = advise(&movie());
        assert_eq!(a.len(), 1);
        assert!(!a[0].serious && a[0].what.contains("smoothly forwards") && a[0].try_this.contains("Hap"), "{a:?}");
        assert!(advise(&Facts { codec: "hap".into(), ..movie() }).is_empty());
        assert!(advise(&Facts { codec: "prores".into(), ..movie() }).is_empty());
    }

    #[test]
    fn dropped_frames_say_why_and_what_to_do() {
        // On the CPU and dropping: convert.
        let a = advise(&Facts { hardware: Some(false), skipped: 90, cpu: Some(0.9), ..movie() });
        assert!(a[0].serious && a[0].what.contains("dropping frames") && a[0].try_this.contains("Convert"), "{a:?}");
        assert!(a.iter().any(|x| x.what.contains("90% of one CPU core")));
        // From a network share and dropping: copy it here first (said once, not twice).
        let a = advise(&Facts { skipped: 90, place: Place::Network, ..movie() });
        assert!(a[0].try_this.contains("network share") && a[0].try_this.contains("onto this computer"), "{a:?}");
        assert_eq!(a.iter().filter(|x| x.what.contains("network share") || x.try_this.contains("network share")).count(), 1);
        // A few late frames at the start are not a stutter.
        assert!(advise(&Facts { skipped: 3, ..movie() }).iter().all(|x| !x.serious));
        assert!(advise(&Facts { decoded: 20, skipped: 10, ..movie() }).iter().all(|x| !x.serious), "too soon to say");
    }

    #[test]
    fn where_it_lives_and_how_big_it_is() {
        let a = advise(&Facts { place: Place::Network, ..movie() });
        assert!(a[0].serious && a[0].what.contains("network share"));
        let a = advise(&Facts { place: Place::External, mbps: Some(480.0), codec: "prores".into(), ..movie() });
        assert!(a.iter().any(|x| x.what.contains("480 megabits") && x.try_this.contains("onto this computer")));
        assert!(advise(&Facts { place: Place::External, codec: "prores".into(), ..movie() }).is_empty(), "a light file on a drive is fine");
        let a = advise(&Facts { width: 7680, height: 4320, codec: "hap".into(), ..movie() });
        assert!(a.iter().any(|x| x.what.contains("7680×4320") && x.try_this.contains("Size")));
        let a = advise(&Facts { reverse_slow: true, ..movie() });
        assert!(a[0].serious && a[0].what.contains("backwards"));
    }

    #[test]
    fn the_best_conversion_fits_the_file() {
        assert_eq!(best_conversion(&movie()).0, "Hap");
        assert_eq!(best_conversion(&Facts { has_audio: true, ..movie() }).0, "H.264, every frame a keyframe");
        assert_eq!(best_conversion(&Facts { codec: "prores4444".into(), has_audio: true, ..movie() }).0, "Hap Alpha");
        // (The names are the Convert window's own.)
        for f in [movie(), Facts { has_audio: true, ..movie() }, Facts { codec: "qtrle".into(), ..movie() }] {
            let name = best_conversion(&f).0;
            assert!(crate::convert::Format::ALL.iter().any(|x| x.name() == name), "{name}");
        }
    }

    #[test]
    fn a_path_is_placed_by_its_drive() {
        let mounts = vec![("/".to_string(), true, true), ("/System/Volumes/Data".to_string(), true, true), ("/Volumes/SSD".to_string(), true, false), ("/Volumes/NAS".to_string(), false, false)];
        assert_eq!(place_among(&mounts, "/Users/me/Movies/a.mov"), Place::Here);
        assert_eq!(place_among(&mounts, "/Volumes/SSD/a.mov"), Place::External);
        assert_eq!(place_among(&mounts, "/Volumes/NAS/shows/a.mov"), Place::Network);
        assert_eq!(place_among(&mounts, "/Volumes/SSD2/a.mov"), Place::Here, "not the SSD: falls to the root");
        assert_eq!(place_of("\\\\server\\share\\a.mov"), Place::Network);
    }
}
