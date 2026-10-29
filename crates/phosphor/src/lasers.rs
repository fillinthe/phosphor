//! Laser output: finds the controller boxes lasers plug into (Ether Dream, IDN, LaserCube over the network, Helios
//! and LaserCube over USB, all through the `laser-dac` crate) and sends each projector its look while the lasers are
//! armed. Nothing is opened or sent until then: a project always opens dark (`Lasers::armed` isn't saved), and
//! disarming, Blackout and closing Phosphor stop every laser (a dropped session stops its laser).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::{Arc, Mutex};

use laser_dac::{Frame, FrameSession, FrameSessionConfig};
use phosphor_engine::laser::{LaserInputs, LaserPoint, Picture, look_paths, project, test_pattern, trace, trace_source};
use phosphor_engine::{Lasers, ModInputs};

pub use phosphor_engine::laser::FPS;
/// Seconds between looks for controller boxes while the Lasers panel is open.
const SCAN_EVERY: f64 = 4.0;
/// Monitor keys for Video trace pictures: `FIRST_KEY + i` for the i-th source (up to `MAX_PICTURES`).
pub const FIRST_KEY: u8 = 240;
pub const MAX_PICTURES: usize = 8;
/// Video trace pictures: this wide (the height follows the canvas), read this many times a second.
const PICTURE_WIDTH: u32 = 128;
const PICTURES_FPS: u32 = 25;

/// A controller box found on the network or USB.
#[derive(Clone, Debug, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub kind: String,
}

struct Live {
    device: String,
    pps: u32,
    session: FrameSession,
}

#[derive(Default)]
pub struct LaserOut {
    found: Arc<Mutex<Vec<Device>>>,
    scanning: Arc<AtomicBool>,
    last_scan: f64,
    live: HashMap<u64, Live>,
    opening: HashMap<u64, Receiver<Result<Live, String>>>,
    /// When a projector last failed to open (tried again a little later).
    failed_at: HashMap<u64, f64>,
    /// What each projector is doing, for the panel ("sending", "can't open: …").
    pub status: HashMap<u64, String>,
    /// Laser files' frames (read off the UI thread when a look first uses them) and what went wrong reading any.
    files: Arc<Mutex<HashMap<String, Arc<Vec<Vec<phosphor_engine::ilda::Point>>>>>>,
    reading: Arc<Mutex<HashMap<String, Option<String>>>>,
    /// Video trace pictures, by source, and the sources last asked for (in tag order).
    pictures: HashMap<u64, Arc<Picture>>,
    asked: Vec<u64>,
    /// When the Lasers panel last showed (its Designer traces pictures too).
    shown: Option<web_time::Instant>,
    pacer: crate::pacing::FramePacer,
    picture_pacer: crate::pacing::FramePacer,
}

impl LaserOut {
    /// Looks for controller boxes now and then (call while the Lasers panel shows). Off the UI thread.
    pub fn scan(&mut self, now: f64) {
        self.shown = Some(web_time::Instant::now());
        if now - self.last_scan < SCAN_EVERY || self.scanning.swap(true, Ordering::AcqRel) {
            return;
        }
        self.last_scan = now;
        let (found, scanning) = (self.found.clone(), self.scanning.clone());
        let _ = std::thread::Builder::new().name("phosphor-laser-scan".into()).spawn(move || {
            if let Ok(list) = laser_dac::list_devices() {
                *found.lock().unwrap() = list.into_iter().map(|d| Device { id: d.id, name: d.name, kind: d.kind.to_string() }).collect();
            }
            scanning.store(false, Ordering::Release);
        });
    }

    pub fn devices(&self) -> Vec<Device> {
        self.found.lock().unwrap().clone()
    }

    /// What looks draw from: the laser files `lasers` use, as read so far (files not read yet start reading, each
    /// once), and the latest Video trace pictures.
    pub fn inputs(&mut self, lasers: &Lasers) -> LaserInputs {
        for path in lasers.looks.iter().flat_map(|l| &l.shapes).filter(|s| s.kind == phosphor_engine::LaserShapeKind::File && !s.file.is_empty()).map(|s| &s.file) {
            if self.reading.lock().unwrap().contains_key(path) {
                continue;
            }
            self.reading.lock().unwrap().insert(path.clone(), None);
            let (path, files, reading) = (path.clone(), self.files.clone(), self.reading.clone());
            let _ = std::thread::Builder::new().name("phosphor-laser-file".into()).spawn(move || {
                match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| phosphor_engine::ilda::parse(&b)) {
                    Ok(frames) => {
                        files.lock().unwrap().insert(path, Arc::new(frames));
                    }
                    Err(e) => {
                        reading.lock().unwrap().insert(path, Some(e));
                    }
                }
            });
        }
        LaserInputs { files: self.files.lock().unwrap().clone(), pictures: self.pictures.clone() }
    }

    /// The pictures Video trace shapes need now (looks on armed projectors, and every look while the panel shows),
    /// as (source, width, height) monitors (keys `FIRST_KEY + i`), and whether to read them back this frame.
    pub fn pictures_wanted(&mut self, lasers: &Lasers, size: (u32, u32), now: f64) -> (Vec<(u64, u32, u32)>, bool) {
        let previewing = self.shown.is_some_and(|t| t.elapsed().as_secs_f32() < 0.5);
        let in_use = |look: u64| previewing || lasers.armed && lasers.projectors.iter().any(|p| p.enabled && !p.test_pattern && lasers.look(p.look).is_some_and(|l| l.id == look));
        let mut sources: Vec<u64> = Vec::new();
        for s in lasers.looks.iter().filter(|l| in_use(l.id)).flat_map(|l| &l.shapes).filter(|s| s.enabled && s.kind == phosphor_engine::LaserShapeKind::Trace) {
            let src = trace_source(s);
            if !sources.contains(&src) && sources.len() < MAX_PICTURES {
                sources.push(src);
            }
        }
        self.pictures.retain(|src, _| sources.contains(src));
        self.asked = sources.clone();
        let h = (PICTURE_WIDTH * size.1 / size.0.max(1)).max(2);
        let due = !sources.is_empty() && self.picture_pacer.due(now, PICTURES_FPS);
        (sources.into_iter().map(|s| (s, PICTURE_WIDTH, h)).collect(), due)
    }

    /// A picture read back (monitor key `key`).
    pub fn give_picture(&mut self, key: u8, width: u32, height: u32, rgba: Vec<u8>) {
        if let Some(&src) = key.checked_sub(FIRST_KEY).and_then(|i| self.asked.get(i as usize)) {
            self.pictures.insert(src, Arc::new(Picture { width: width as usize, height: height as usize, rgba }));
        }
    }

    /// Why a laser file couldn't be read (None: fine, or still reading).
    pub fn file_error(&self, path: &str) -> Option<String> {
        self.reading.lock().unwrap().get(path).cloned().flatten()
    }

    pub fn sending(&self) -> usize {
        self.live.len()
    }

    /// Every frame: while armed, opens projectors' boxes (off the UI thread) and sends each its look at `t` seconds.
    /// `blackout` (0..1) dims every laser with the video.
    pub fn run(&mut self, lasers: &Lasers, t: f64, mods: &ModInputs, blackout: f32, now: f64) {
        if !lasers.armed {
            if !self.live.is_empty() || !self.opening.is_empty() {
                self.live.clear();
                self.opening.clear();
                self.status.clear();
            }
            return;
        }
        // Projectors that went, were switched off, or changed box or speed: close (reopened below).
        self.live.retain(|id, l| lasers.projectors.iter().any(|p| p.id == *id && p.enabled && p.device == l.device && p.pps == l.pps));
        self.live.retain(|id, l| {
            let alive = !l.session.is_finished();
            if !alive {
                self.status.insert(*id, "Lost the connection".into());
                self.failed_at.insert(*id, now);
            }
            alive
        });
        // Boxes that finished opening.
        let mut done = Vec::new();
        for (id, rx) in &self.opening {
            match rx.try_recv() {
                Ok(Ok(live)) => {
                    self.status.insert(*id, "Sending".into());
                    done.push((*id, Some(live)));
                }
                Ok(Err(e)) => {
                    self.status.insert(*id, format!("Can't open it: {e}"));
                    self.failed_at.insert(*id, now);
                    done.push((*id, None));
                }
                Err(TryRecvError::Disconnected) => done.push((*id, None)),
                Err(TryRecvError::Empty) => {}
            }
        }
        for (id, live) in done {
            self.opening.remove(&id);
            if let Some(live) = live {
                self.live.insert(id, live);
            }
        }
        for p in lasers.projectors.iter().filter(|p| p.enabled && !p.device.is_empty()) {
            if self.live.contains_key(&p.id) || self.opening.contains_key(&p.id) || self.failed_at.get(&p.id).is_some_and(|at| now - at < 3.0) {
                continue;
            }
            let (tx, rx) = std::sync::mpsc::channel();
            let (device, pps) = (p.device.clone(), p.pps);
            self.status.insert(p.id, "Opening…".into());
            let _ = std::thread::Builder::new().name("phosphor-laser-open".into()).spawn(move || {
                let open = || -> Result<Live, String> {
                    let dac = laser_dac::open_device(&device).map_err(|e| e.to_string())?;
                    let (session, _) = dac.start_frame_session(FrameSessionConfig::new(pps)).map_err(|e| e.to_string())?;
                    session.control().arm().map_err(|e| e.to_string())?;
                    Ok(Live { device, pps, session })
                };
                let _ = tx.send(open());
            });
            self.opening.insert(p.id, rx);
        }
        if self.live.is_empty() || !self.pacer.due(now, FPS) {
            return;
        }
        let master = lasers.master * (1.0 - blackout.clamp(0.0, 1.0));
        let inputs = self.inputs(lasers);
        for p in &lasers.projectors {
            let Some(live) = self.live.get(&p.id) else { continue };
            let points = frame_for(lasers, p, t, mods, master, &inputs);
            live.session.send_frame(Frame::new(points.iter().map(to_dac).collect()));
        }
    }
}

/// What projector `p` draws at `t` seconds: its look (or the test pattern) traced to fit its points a second, then
/// placed and dimmed.
pub fn frame_for(lasers: &Lasers, p: &phosphor_engine::LaserProjector, t: f64, mods: &ModInputs, master: f32, inputs: &LaserInputs) -> Vec<LaserPoint> {
    let paths = if p.test_pattern { test_pattern() } else { lasers.look(p.look).map(|l| look_paths(l, t, mods, inputs)).unwrap_or_default() };
    project(&trace(&paths, (p.pps / FPS) as usize), p, master)
}

fn to_dac(p: &LaserPoint) -> laser_dac::LaserPoint {
    let c = |v: f32| (v.clamp(0.0, 1.0) * 65535.0) as u16;
    let [r, g, b] = p.rgb.map(c);
    laser_dac::LaserPoint::new(p.x, p.y, r, g, b, r.max(g).max(b))
}
