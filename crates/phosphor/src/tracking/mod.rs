//! Video tracking (VDMX's Vision tracking): a small picture of the chosen layer / group / mix is read back a few
//! times a second (a monitor slot, `Engine::read_monitor`), Apple Vision looks for faces, hands and a body on a
//! worker thread, and what it finds lands in `Track …` Hand modulators (`Composition::set_hand`) that any control
//! can follow. Costs nothing while tracking is off; one picture in flight at a time.
//!
//! The same worker makes Remove background mattes: the engine reads back each cut-out effect's input
//! (`request_cutout_frames`), Vision's person segmentation turns it into a matte, `Engine::set_matte` takes it.

use std::sync::mpsc::{Receiver, Sender, channel};

use phosphor_engine::Tracking;

#[cfg(target_os = "macos")]
mod vision;

/// The monitor slot key tracking uses (preview windows use small numbers).
pub const MONITOR_KEY: u8 = 250;
/// Width of the picture Vision looks at (height follows the canvas).
const WIDTH: u32 = 480;
/// Looks per second at most.
const RATE: f64 = 15.0;
/// Remove background mattes per second at most.
const CUTOUT_RATE: f64 = 30.0;

/// What one look found: modulator name → value (0..1). Missing names = not seen this time.
pub type Found = Vec<(&'static str, f32)>;

struct Picture {
    rgba: Vec<u8>,
    width: u32,
    height: u32,
}

enum Job {
    Track(Picture, Tracking),
    /// A matte for the Remove background effect on this layer / group.
    Cutout(Picture, u64),
}

enum Done {
    Track(Result<Found, String>),
    Matte(u64, Result<Matte, String>),
}

/// A person matte: width, height, one byte per pixel (255 = person).
pub type Matte = (u32, u32, Vec<u8>);

#[derive(Default)]
pub struct Tracker {
    jobs: Option<Sender<Job>>,
    results: Option<Receiver<Done>>,
    /// A picture was asked for (or is being looked at) since this time.
    busy_since: Option<f64>,
    next_at: f64,
    cutout_busy_since: Option<f64>,
    cutout_next_at: f64,
    /// Mattes ready for the engine.
    mattes: Vec<(u64, Matte)>,
    /// What the last look found, for the panel ("1 face · 2 hands"), or why it can't look.
    pub status: String,
}

impl Tracker {
    /// The picture tracking watches (source, width, height), asked for every frame while tracking is on (a small
    /// copy; the Tracking panel shows it). None = off.
    pub fn picture(&mut self, cfg: &Tracking, canvas: (u32, u32)) -> Option<(u64, u32, u32)> {
        if !cfg.enabled || cfg.outputs().is_empty() {
            self.busy_since = None;
            return None;
        }
        let h = (WIDTH as f32 * canvas.1 as f32 / canvas.0.max(1) as f32).round().clamp(16.0, 1080.0) as u32;
        Some((cfg.source, WIDTH, h))
    }

    /// Should this frame's picture be read back and looked at? (Paced, one at a time.)
    pub fn due(&mut self, now: f64) -> bool {
        // A picture that never came back (source gone, layer off): ask again.
        if self.busy_since.is_some_and(|t| now - t > 2.0) {
            self.busy_since = None;
        }
        if self.busy_since.is_some() || now < self.next_at {
            return false;
        }
        self.busy_since = Some(now);
        self.next_at = now + 1.0 / RATE;
        true
    }

    /// Should the engine read back the Remove background effects' pictures this frame? (Paced, one batch at a time.)
    pub fn cutout_due(&mut self, now: f64) -> bool {
        if self.cutout_busy_since.is_some_and(|t| now - t > 1.0) {
            self.cutout_busy_since = None;
        }
        if self.cutout_busy_since.is_some() || now < self.cutout_next_at {
            return false;
        }
        self.cutout_busy_since = Some(now);
        self.cutout_next_at = now + 1.0 / CUTOUT_RATE;
        true
    }

    /// The tracking picture came back: hand it to the worker.
    pub fn give(&mut self, rgba: Vec<u8>, width: u32, height: u32, cfg: &Tracking) {
        self.send(Job::Track(Picture { rgba, width, height }, cfg.clone()));
    }

    /// A Remove background effect's picture (on layer / group `res`) came back.
    pub fn give_cutout(&mut self, res: u64, rgba: Vec<u8>, width: u32, height: u32) {
        self.send(Job::Cutout(Picture { rgba, width, height }, res));
    }

    /// Mattes made since last asked, for `Engine::set_matte`.
    pub fn take_mattes(&mut self) -> Vec<(u64, Matte)> {
        std::mem::take(&mut self.mattes)
    }

    fn send(&mut self, job: Job) {
        let jobs = self.jobs.get_or_insert_with(|| {
            let (tx, rx) = channel::<Job>();
            let (rtx, rrx) = channel();
            std::thread::Builder::new()
                .name("tracking".into())
                .spawn(move || {
                    while let Ok(first) = rx.recv() {
                        // Only the newest picture of each kind (and each cut-out effect) matters.
                        let mut pending = vec![first];
                        while let Ok(newer) = rx.try_recv() {
                            pending.retain(|j| !same_kind(j, &newer));
                            pending.push(newer);
                        }
                        for job in pending {
                            if rtx.send(work(job)).is_err() {
                                return;
                            }
                        }
                    }
                })
                .ok();
            self.results = Some(rrx);
            tx
        });
        if jobs.send(job).is_err() {
            self.jobs = None;
            self.busy_since = None;
            self.cutout_busy_since = None;
        }
    }

    /// Writes finished looks into the `Track …` modulators (smoothed); not-found values hold, Found drops to 0.
    pub fn apply(&mut self, comp: &mut phosphor_engine::Composition) {
        let Some(rx) = &self.results else { return };
        while let Ok(done) = rx.try_recv() {
            let r = match done {
                Done::Track(r) => r,
                Done::Matte(res, m) => {
                    self.cutout_busy_since = None;
                    match m {
                        Ok(m) => self.mattes.push((res, m)),
                        Err(e) => self.status = e,
                    }
                    continue;
                }
            };
            self.busy_since = None;
            match r {
                Ok(found) => {
                    let cfg = comp.tracking.clone();
                    for name in cfg.outputs() {
                        let current = comp.hand(name).unwrap_or(0.0);
                        let v = match found.iter().find(|(n, _)| *n == name) {
                            Some((_, v)) if name.ends_with("Found") => *v,
                            Some((_, v)) => phosphor_engine::smooth_toward(current, v.clamp(0.0, 1.0), cfg.smoothing),
                            None if name.ends_with("Found") => 0.0,
                            None => current,
                        };
                        comp.set_hand(name, v);
                    }
                    self.status = summary(&found);
                }
                Err(e) => self.status = e,
            }
        }
    }
}

/// "Face · 2 hands · body" from what was found.
fn summary(found: &Found) -> String {
    let seen = |n: &str| found.iter().any(|(k, v)| *k == n && *v > 0.5);
    let mut parts = Vec::new();
    if seen("Track Face Found") {
        parts.push("face".to_string());
    }
    if seen("Track Hand Found") {
        parts.push("hand".to_string());
    }
    if seen("Track Body Found") {
        parts.push("body".to_string());
    }
    if parts.is_empty() { "Nothing seen right now".into() } else { format!("Seeing: {}", parts.join(", ")) }
}

fn same_kind(a: &Job, b: &Job) -> bool {
    match (a, b) {
        (Job::Track(..), Job::Track(..)) => true,
        (Job::Cutout(_, x), Job::Cutout(_, y)) => x == y,
        _ => false,
    }
}

#[cfg(target_os = "macos")]
fn work(job: Job) -> Done {
    match job {
        Job::Track(p, cfg) => Done::Track(vision::look(&p.rgba, p.width, p.height, &cfg)),
        Job::Cutout(p, res) => Done::Matte(res, vision::person_matte(&p.rgba, p.width, p.height)),
    }
}

#[cfg(not(target_os = "macos"))]
fn work(job: Job) -> Done {
    const NOT_HERE: &str = "Video tracking and Remove background need macOS (Apple Vision) for now.";
    match job {
        Job::Track(..) => Done::Track(Err(NOT_HERE.into())),
        Job::Cutout(_, res) => Done::Matte(res, Err(NOT_HERE.into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker_with(results: Vec<Result<Found, String>>) -> Tracker {
        let (tx, rx) = channel();
        for r in results {
            tx.send(Done::Track(r)).unwrap();
        }
        Tracker { results: Some(rx), busy_since: Some(0.0), ..Default::default() }
    }

    #[test]
    fn finds_become_modulators_and_lost_ones_hold() {
        let mut comp = phosphor_engine::Composition::default();
        comp.tracking = Tracking { enabled: true, smoothing: 0.0, ..Default::default() };
        let mut t = tracker_with(vec![Ok(vec![("Track Face X", 0.25), ("Track Face Y", 0.75), ("Track Face Size", 1.4), ("Track Face Found", 1.0)])]);
        t.apply(&mut comp);
        assert_eq!(comp.hand("Track Face X"), Some(0.25));
        assert_eq!(comp.hand("Track Face Size"), Some(1.0), "kept in 0..1");
        assert_eq!(comp.hand("Track Face Found"), Some(1.0));
        assert!(t.busy_since.is_none(), "ready for the next picture");
        assert_eq!(t.status, "Seeing: face");
        let mut t = tracker_with(vec![Ok(Vec::new())]);
        t.apply(&mut comp);
        assert_eq!(comp.hand("Track Face X"), Some(0.25), "where it was last seen");
        assert_eq!(comp.hand("Track Face Found"), Some(0.0));
        assert!(comp.hand("Track Hand X").is_none(), "hands are off: no hand modulators");
    }

    #[test]
    fn smoothing_glides() {
        let mut comp = phosphor_engine::Composition::default();
        comp.tracking = Tracking { enabled: true, smoothing: 0.5, ..Default::default() };
        comp.set_hand("Track Face X", 0.0);
        tracker_with(vec![Ok(vec![("Track Face X", 1.0)])]).apply(&mut comp);
        assert_eq!(comp.hand("Track Face X"), Some(0.5));
    }

    #[test]
    fn paced_and_off_costs_nothing() {
        let mut t = Tracker::default();
        let off = Tracking::default();
        assert!(t.picture(&off, (1920, 1080)).is_none());
        let on = Tracking { enabled: true, ..Default::default() };
        assert_eq!(t.picture(&on, (1920, 1080)), Some((phosphor_engine::MASTER_ID, 480, 270)));
        assert!(t.due(0.0));
        assert!(!t.due(0.5), "one picture at a time");
        assert!(t.due(2.5), "one that never came back is given up on");
    }

    /// Opt-in (`cargo test -p phosphor finds_a_face -- --ignored`): Vision finds the face in Apple's test portrait
    /// (in the iOS simulator runtime, when Xcode is installed).
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn finds_a_face() {
        let runtimes = "/Library/Developer/CoreSimulator/Volumes";
        let path = std::fs::read_dir(runtimes).unwrap().flatten().find_map(|v| {
            let base = v.path().join("Library/Developer/CoreSimulator/Profiles/Runtimes");
            std::fs::read_dir(base).ok()?.flatten().map(|r| r.path().join("Contents/Resources/RuntimeRoot/System/Library/PrivateFrameworks/CoreIDVShared.framework/TestPortrait.jpg")).find(|p| p.exists())
        });
        let Some(path) = path else { return };
        let img = image::open(path).unwrap().to_rgba8();
        let found = vision::look(img.as_raw(), img.width(), img.height(), &Tracking { enabled: true, ..Default::default() }).unwrap();
        assert!(found.iter().any(|(n, v)| *n == "Track Face Found" && *v == 1.0), "{found:?}");
    }
}
