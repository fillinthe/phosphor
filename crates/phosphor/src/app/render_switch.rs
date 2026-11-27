//! The render thread (`crate::render_thread`) on and off, and what the interface hands it: the Output panel's
//! "Render thread" switch or `PHOSPHOR_RENDER_THREAD`, a job each frame, the frames it drew taken back.

use super::*;
use super::outputs::output_viewport;
use crate::render_thread::{Anchor, FrameJob, Gpu, Heard, OutputWindow, RenderThread};

impl PhosphorApp {
    /// Starts or stops the render thread as asked (every frame; nothing to do when it already is as asked). It's
    /// stopped while a show renders offline (that draws a frame at a time on this thread), and stays stopped for the
    /// run once it failed.
    pub(crate) fn sync_render_thread(&mut self, ctx: &egui::Context) {
        if let Some(t) = &self.render_thread {
            t.bury();
        }
        let asked = crate::render_thread::forced().unwrap_or(self.state.render_thread);
        let failed = self.render_thread.as_ref().is_some_and(|t| !t.running());
        if failed {
            self.state.render_thread_note = Some("stopped after an internal error (details in crash.log): the outputs are drawn the usual way until Phosphor restarts".into());
            self.state.notice = Some(crate::ui::notice::problem("The render thread hit an internal error: the outputs are drawn the usual way again"));
        }
        let wanted = asked && self.state.offline.is_none() && self.state.render_thread_note.is_none();
        match (wanted, self.render_thread.is_some()) {
            (true, false) => self.start_render_thread(ctx),
            (false, true) => self.stop_render_thread(),
            _ => {}
        }
    }

    fn start_render_thread(&mut self, ctx: &egui::Context) {
        let Some(rs) = self.state.render_state.as_ref() else { return };
        if let Some(why) = crate::render_thread::unavailable(rs.adapter.get_info().backend) {
            self.state.render_thread_note = Some(why.into());
            return;
        }
        let gpu = Gpu { adapter: rs.adapter.clone(), device: rs.device.clone(), queue: rs.queue.clone(), format: rs.target_format };
        let clock = Anchor { at: web_time::Instant::now(), time: ctx.input(|i| i.time) };
        match RenderThread::start(self.state.core.clone(), gpu, clock) {
            Ok(t) => {
                self.render_thread = Some(t);
                self.seen_frames = self.state.core.lock().frames;
            }
            Err(e) => self.state.render_thread_note = Some(format!("couldn't start: {e}")),
        }
    }

    /// Stops the thread (it hands its windows back as it ends) and makes sure the interface paints them again.
    fn stop_render_thread(&mut self) {
        self.render_thread = None;
        let ids: Vec<u64> = self.state.output.lock().unwrap().keys().copied().collect();
        for id in ids {
            if let Some(s) = eframe::egui_wgpu::winit::shared_surface(output_viewport(id)) {
                s.set_app_presents(false);
            }
        }
        for o in self.state.output.lock().unwrap().values_mut() {
            o.thread_presents = false;
        }
    }

    /// What the render thread draws from next: the project now, the beat, the sound heard, the open output windows.
    pub(super) fn publish_frame(&mut self, now: f64, show: f64, heard: Heard) {
        let Some(thread) = &self.render_thread else { return };
        let st = &self.state;
        let canvas = (st.comp.width, st.comp.height);
        let outputs: Vec<OutputWindow> = {
            let map = st.output.lock().unwrap();
            st.comp
                .outputs
                .iter()
                .filter(|o| o.enabled && map.get(&o.id).is_some_and(|w| w.open))
                .map(|o| {
                    let (w, h) = o.size(canvas);
                    OutputWindow { id: o.id, viewport: output_viewport(o.id), aspect: w as f32 / h.max(1) as f32 }
                })
                .collect()
        };
        thread.publish(FrameJob {
            comp: Arc::new(st.comp.clone()),
            now,
            show_offset: show - now,
            beats: st.clock.beats(now),
            bpm: st.clock.bpm,
            heard,
            clip_page: st.clip_page,
            fps: st.render_rate.fps(),
            outputs,
            dont_wait: st.screens_dont_wait,
        });
    }

    /// The frames the render thread drew since the last call, or None when it drew none (nothing new to read back
    /// or show in the interface: the rest of `render` waits for one). The modulators' values shown are its newest.
    pub(super) fn take_thread_frames(&mut self) -> Option<Vec<super::render_core::FrameTiming>> {
        let mut core = self.state.core.lock();
        if core.frames == self.seen_frames {
            return None;
        }
        self.seen_frames = core.frames;
        self.state.mod_inputs.shared = core.mods.shared;
        Some(core.take_timings())
    }
}
