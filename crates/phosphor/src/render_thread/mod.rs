//! The render thread (Output panel › Render thread, or `PHOSPHOR_RENDER_THREAD=1`; off by default): draws the show
//! and presents the output windows on its own cadence, so nothing the operator does in the interface (a big panel, a
//! menu, a save, a slow call) makes the audience's picture stutter.
//!
//! The interface publishes a [`FrameJob`] whenever a frame is due on its side (the project as it is now, the beat
//! clock, the sound it heard, which output windows are open). The thread draws from the newest job every frame:
//! movies (their players live in the shared [`Core`]), modulators, the engine, Syphon out, then each output window
//! whose surface the interface has handed over (`egui_wgpu::winit::SharedSurface`). A job that's late (a 200 ms
//! interface hitch) is simply drawn again with the clock moved on: movies, generators, effects and modulators keep
//! going; what only the interface sees (a controller moved, audio levels) waits for the next job.
//!
//! Everything else the engine does for the interface (previews, thumbnails, readbacks, recordings, NDI, DMX) stays on
//! the interface's side of [`Core`], once per frame the thread drew (`PhosphorApp::render`).

mod blit;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use eframe::egui::ViewportId;
use eframe::egui_wgpu::winit::SharedSurface;
use phosphor_engine::{Composition, ModInputs, wgpu};

use crate::app::render_core::{Core, FrameTiming, RenderCore};

/// `PHOSPHOR_RENDER_THREAD=1` turns the render thread on for a run, `=0` off, whatever the setting says.
pub fn forced() -> Option<bool> {
    match std::env::var("PHOSPHOR_RENDER_THREAD").ok()?.trim() {
        "1" | "on" | "yes" => Some(true),
        "0" | "off" | "no" => Some(false),
        _ => None,
    }
}

/// Why the render thread can't run on this graphics setup (None = it can).
pub fn unavailable(backend: wgpu::Backend) -> Option<&'static str> {
    // OpenGL ties a window's drawing to the thread that made its context: presenting from another one isn't safe.
    (backend == wgpu::Backend::Gl).then_some("not with OpenGL graphics")
}

/// The sound the interface heard, for the modulators the render thread works out.
#[derive(Clone, Copy, Debug, Default)]
pub struct Heard {
    pub level: f32,
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    pub beat: f32,
}

/// One output window the render thread presents (when its surface is handed over).
#[derive(Clone, Copy, Debug)]
pub struct OutputWindow {
    pub id: u64,
    pub viewport: ViewportId,
    /// Width / height of the output's picture (letterboxed in the window, as the interface draws it).
    pub aspect: f32,
}

/// What the render thread draws from, published by the interface (`RenderThread::publish`).
pub struct FrameJob {
    pub comp: Arc<Composition>,
    /// The interface's clock (seconds) when the job was made.
    pub now: f64,
    /// Show time minus the clock (following another Phosphor runs on the leader's time).
    pub show_offset: f64,
    /// The beat clock at `now`: beats and tempo (it runs on linearly until the next job).
    pub beats: f64,
    pub bpm: f32,
    pub heard: Heard,
    /// The clip bank on screen (its next clips are kept ready).
    pub clip_page: usize,
    /// Frames a second (None = the display's rate: the first output's refresh paces the thread).
    pub fps: Option<u32>,
    pub outputs: Vec<OutputWindow>,
    /// Output windows don't wait for each other's screens (`crate::present`).
    pub dont_wait: bool,
}

impl FrameJob {
    /// Beats on the BPM clock at `now` (the interface's `Clock::beats`, run on).
    fn beats_at(&self, now: f64) -> f64 {
        self.beats + (now - self.now) * self.bpm.max(1.0) as f64 / 60.0
    }

    /// The signals modulators read at `now`, before the shared modulators are worked out.
    fn inputs_at(&self, now: f64) -> ModInputs {
        let beats = self.beats_at(now);
        ModInputs {
            beats: beats as f32,
            audio_beat: self.heard.beat,
            beat_phase: beats.rem_euclid(1.0) as f32,
            time: (now + self.show_offset) as f32,
            audio_level: self.heard.level,
            audio_low: self.heard.low,
            audio_mid: self.heard.mid,
            audio_high: self.heard.high,
            shared: Default::default(),
        }
    }
}

/// The graphics handles the thread presents with (egui's: its windows' surfaces belong to that device).
#[derive(Clone)]
pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub format: wgpu::TextureFormat,
}

/// The interface's clock on this thread: `time` seconds at `at`, run on by the wall clock. One anchor for the
/// thread's whole run, so its clock never steps back when a job comes in (each job's `now` is a little stale).
#[derive(Clone, Copy)]
pub struct Anchor {
    pub at: web_time::Instant,
    pub time: f64,
}

impl Anchor {
    fn now(&self) -> f64 {
        self.time + self.at.elapsed().as_secs_f64()
    }
}

/// What the interface and the thread share besides the [`Core`].
#[derive(Default)]
struct Inbox {
    job: Mutex<Option<Arc<FrameJob>>>,
    stop: AtomicBool,
    /// The thread is drawing (false once it stopped, or after a panic: the interface then draws itself).
    alive: AtomicBool,
    /// Window surfaces the thread was the last to hold: let go on the main thread (their windows close there).
    graveyard: Mutex<Vec<Arc<SharedSurface>>>,
}

/// The running render thread; stopped and joined when dropped.
pub struct RenderThread {
    inbox: Arc<Inbox>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RenderThread {
    pub fn start(core: Core, gpu: Gpu, clock: Anchor) -> std::io::Result<Self> {
        let inbox = Arc::new(Inbox::default());
        inbox.alive.store(true, Ordering::Release);
        let shared = Arc::clone(&inbox);
        let handle = std::thread::Builder::new().name("render".into()).spawn(move || run(&shared, &core, gpu, clock))?;
        log::info!(target: "phosphor::action", "render thread started: outputs are drawn and shown on their own thread");
        Ok(Self { inbox, handle: Some(handle) })
    }

    /// The newest job replaces any the thread hasn't started on.
    pub fn publish(&self, job: FrameJob) {
        *self.inbox.job.lock().unwrap_or_else(PoisonError::into_inner) = Some(Arc::new(job));
    }

    /// Still drawing (false after a panic on the thread: the interface takes the outputs back).
    pub fn running(&self) -> bool {
        self.inbox.alive.load(Ordering::Acquire)
    }

    /// Lets go of window surfaces the thread held last (call on the main thread, every frame).
    pub fn bury(&self) {
        let dead = std::mem::take(&mut *self.inbox.graveyard.lock().unwrap_or_else(PoisonError::into_inner));
        drop(dead);
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        self.inbox.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take()
            && h.join().is_err()
        {
            log::warn!("render thread ended with a panic");
        }
        self.bury();
        log::info!(target: "phosphor::action", "render thread stopped: the interface draws the outputs again");
    }
}

/// Frames a second when nothing else paces the thread (no job yet, or no output window to wait on).
const IDLE_FPS: f64 = 60.0;

fn run(inbox: &Inbox, core: &Core, gpu: Gpu, clock: Anchor) {
    raise_priority();
    let mut presenter = blit::Presenter::new(gpu);
    let mut next = web_time::Instant::now();
    while !inbox.stop.load(Ordering::Acquire) {
        let job = inbox.job.lock().unwrap_or_else(PoisonError::into_inner).clone();
        let Some(job) = job else {
            std::thread::sleep(std::time::Duration::from_millis(5));
            continue;
        };
        // Paced by the render rate; at the display's rate by the first output's refresh (taking its next picture
        // waits for it), else 60 a second.
        let fps = match job.fps {
            Some(f) => Some(f.max(1) as f64),
            None if presenter.vsync_paced() => None,
            None => Some(IDLE_FPS),
        };
        if let Some(fps) = fps {
            next = wait_until(next, 1.0 / fps);
        }
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            #[cfg(target_os = "macos")]
            objc2::rc::autoreleasepool(|_| frame(&job, clock.now(), core, &mut presenter));
            #[cfg(not(target_os = "macos"))]
            frame(&job, clock.now(), core, &mut presenter);
        }));
        if ok.is_err() {
            log::error!("the render thread hit an internal error: the interface draws the outputs again (details in crash.log)");
            break;
        }
        let dead = presenter.take_dead();
        if !dead.is_empty() {
            inbox.graveyard.lock().unwrap_or_else(PoisonError::into_inner).extend(dead);
        }
    }
    // Hand every window back before the interface starts painting them again.
    presenter.release_all();
    let dead = presenter.take_dead();
    inbox.graveyard.lock().unwrap_or_else(PoisonError::into_inner).extend(dead);
    inbox.alive.store(false, Ordering::Release);
}

/// Sleeps until `next`, then gives the slot after it (`step` seconds on). A slot missed by more than a step starts the
/// cadence again from now instead of racing to catch up.
fn wait_until(next: web_time::Instant, step: f64) -> web_time::Instant {
    let now = web_time::Instant::now();
    if next > now {
        std::thread::sleep(next - now);
    }
    let step = std::time::Duration::from_secs_f64(step);
    let after = next + step;
    let now = web_time::Instant::now();
    if after + step < now { now + step } else { after }
}

/// One frame: movies, modulators and the engine under the core's lock, then the output windows without it.
fn frame(job: &FrameJob, now: f64, core: &Core, presenter: &mut blit::Presenter) {
    let at = web_time::Instant::now();
    let pictures = {
        let mut held = core.lock();
        let RenderCore { engine, media, mod_phases, .. } = &mut *held;
        let Some(engine) = engine.as_mut() else { return };
        let comp = &*job.comp;
        media.sync(comp, engine);
        media.preload_clips(comp, job.clip_page, crate::panels::CLIP_PAGE_SIZE);
        media.update_video(comp, engine, now);
        let mut mods = job.inputs_at(now);
        mods.shared = phosphor_engine::evaluate(&comp.modulators, &mods, mod_phases);
        let t0 = web_time::Instant::now();
        let info = engine.render(comp, mods.time, &mods);
        let render_secs = t0.elapsed().as_secs_f64();
        crate::present::note_engine();
        #[cfg(target_os = "macos")]
        {
            let RenderCore { engine, syphon, .. } = &mut *held;
            crate::app::publish_syphon(syphon, engine.as_ref(), comp);
        }
        let Some(engine) = held.engine.as_ref() else { return };
        let gpu_ms = engine.gpu_ms().unwrap_or(0.0) as f64;
        let pictures: Vec<(OutputWindow, wgpu::TextureView)> =
            job.outputs.iter().filter_map(|o| Some((*o, engine.output_texture(o.id)?.view.clone()))).collect();
        held.drew(mods, FrameTiming { at, now, render_secs, passes: info.passes, gpu_ms });
        pictures
    };
    presenter.present(&pictures, job.dont_wait);
}

/// The thread that draws what the audience sees asks for the scheduler's attention (macOS: user-interactive QoS).
fn raise_priority() {
    #[cfg(target_os = "macos")]
    // SAFETY: sets the calling thread's own QoS class; no pointers involved.
    unsafe {
        libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn job(now: f64, beats: f64, bpm: f32) -> FrameJob {
        FrameJob {
            comp: Arc::new(Composition::default()),
            now,
            show_offset: 0.5,
            beats,
            bpm,
            heard: Heard { level: 0.25, ..Default::default() },
            clip_page: 0,
            fps: Some(60),
            outputs: Vec::new(),
            dont_wait: false,
        }
    }

    #[test]
    fn a_late_job_runs_the_beat_clock_on() {
        let j = job(10.0, 8.0, 120.0);
        // 120 BPM: two beats a second, from wherever the interface left it.
        assert!((j.beats_at(10.0) - 8.0).abs() < 1e-9);
        assert!((j.beats_at(10.25) - 8.5).abs() < 1e-9);
        let m = j.inputs_at(10.25);
        assert!((m.beat_phase - 0.5).abs() < 1e-6, "{}", m.beat_phase);
        assert!((m.time - 10.75).abs() < 1e-6, "show time = clock + the leader's offset");
        assert_eq!(m.audio_level, 0.25, "what was heard last stays until the next job");
    }

    #[test]
    fn pacing_keeps_its_cadence_and_restarts_after_a_stall() {
        let step = 0.004;
        let start = web_time::Instant::now();
        let mut next = start;
        for _ in 0..5 {
            next = wait_until(next, step);
        }
        let took = start.elapsed().as_secs_f64();
        assert!(took >= 4.0 * step, "four slots waited: {took}");
        // A stall longer than a step: the next slot is a step from now, not a burst of missed ones.
        std::thread::sleep(std::time::Duration::from_secs_f64(step * 5.0));
        let after = wait_until(next, step);
        let ahead = after.saturating_duration_since(web_time::Instant::now()).as_secs_f64();
        assert!(ahead > step * 0.5 && ahead <= step * 1.01, "{ahead}");
    }

    #[test]
    fn opengl_keeps_the_old_way() {
        assert!(unavailable(wgpu::Backend::Gl).is_some());
        assert!(unavailable(wgpu::Backend::Metal).is_none());
        assert!(unavailable(wgpu::Backend::Vulkan).is_none());
        assert!(unavailable(wgpu::Backend::Dx12).is_none());
    }
}
