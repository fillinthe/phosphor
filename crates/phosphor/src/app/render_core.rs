//! What the engine draws with, behind one lock: the engine, the media players that feed it, Syphon out and the
//! modulators' running phases. The interface reaches them through [`Core::lock`] (briefly: never across slow work);
//! with the render thread on (`crate::render_thread`) that thread draws and presents the outputs with them, so the
//! interface taking its time never holds the audience's picture up.

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use phosphor_engine::{Engine, ModInputs, ModPhases};

use crate::media::MediaManager;

/// One frame the render thread drew: when (the interface's clock, seconds), what it cost, for the stats.
#[derive(Clone, Copy, Debug)]
pub struct FrameTiming {
    pub at: web_time::Instant,
    pub now: f64,
    pub render_secs: f64,
    pub passes: usize,
    pub gpu_ms: f64,
}

/// Frame timings kept for the stats at most (the interface takes them each frame; a stalled one loses the oldest).
const MAX_TIMINGS: usize = 600;

pub struct RenderCore {
    pub engine: Option<Engine>,
    pub media: MediaManager,
    #[cfg(target_os = "macos")]
    pub syphon: crate::syphon::Syphon,
    /// Shared modulators' running phases (speed changes never jump).
    pub mod_phases: ModPhases,
    /// Render thread only: frames it has drawn, the signals it drew the last one with, and their timings.
    pub frames: u64,
    pub mods: ModInputs,
    timings: Vec<FrameTiming>,
}

impl RenderCore {
    /// The render thread drew a frame.
    pub fn drew(&mut self, mods: ModInputs, timing: FrameTiming) {
        self.frames += 1;
        self.mods = mods;
        if self.timings.len() >= MAX_TIMINGS {
            self.timings.remove(0);
        }
        self.timings.push(timing);
    }

    /// The render thread's frame timings since the last call (oldest first).
    pub fn take_timings(&mut self) -> Vec<FrameTiming> {
        std::mem::take(&mut self.timings)
    }
}

/// The shared [`RenderCore`].
#[derive(Clone)]
pub struct Core(Arc<Mutex<RenderCore>>);

impl Core {
    pub fn new(engine: Option<Engine>) -> Self {
        Self(Arc::new(Mutex::new(RenderCore {
            engine,
            media: MediaManager::default(),
            #[cfg(target_os = "macos")]
            syphon: Default::default(),
            mod_phases: Default::default(),
            frames: 0,
            mods: ModInputs::default(),
            timings: Vec::new(),
        })))
    }

    /// The core, for as short a time as possible. A panic while it was held (one bad frame) doesn't lock everyone
    /// out: the next frame carries on, as the interface does after a bad frame.
    pub fn lock(&self) -> MutexGuard<'_, RenderCore> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timings_are_taken_once_and_capped() {
        let core = Core::new(None);
        let t = |now| FrameTiming { at: web_time::Instant::now(), now, render_secs: 0.001, passes: 3, gpu_ms: 0.5 };
        {
            let mut c = core.lock();
            for i in 0..(MAX_TIMINGS + 5) {
                c.drew(ModInputs::default(), t(i as f64));
            }
            assert_eq!(c.frames, (MAX_TIMINGS + 5) as u64);
        }
        let taken = core.lock().take_timings();
        assert_eq!(taken.len(), MAX_TIMINGS, "a stalled interface keeps the newest timings only");
        assert_eq!(taken.first().map(|t| t.now), Some(5.0));
        assert!(core.lock().take_timings().is_empty(), "taken once");
    }

    #[test]
    fn a_panic_while_held_does_not_lock_the_core_up() {
        let core = Core::new(None);
        let c2 = core.clone();
        let _ = std::thread::spawn(move || {
            let _held = c2.lock();
            panic!("one bad frame");
        })
        .join();
        assert_eq!(core.lock().frames, 0);
    }

    /// Two `core.lock()`s in one statement hold the first guard while the second waits for it: the main thread stops
    /// for good (the Output panel's Syphon row did, in the 0.5.27 release build). One statement locks the core once.
    #[test]
    fn no_statement_locks_the_core_twice() {
        fn scan(dir: &std::path::Path, bad: &mut Vec<String>) {
            for e in std::fs::read_dir(dir).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    scan(&p, bad);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let text = std::fs::read_to_string(&p).unwrap();
                    for part in text.split([';', '{', '}']) {
                        if part.matches("core.lock()").count() > 1 {
                            bad.push(format!("{}: {}", p.display(), part.trim()));
                        }
                    }
                }
            }
        }
        let mut bad = Vec::new();
        scan(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut bad);
        assert!(bad.is_empty(), "the core locked twice in one statement:\n{}", bad.join("\n"));
    }
}
