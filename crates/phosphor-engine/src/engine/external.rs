//! External effects and sources: pictures made outside the engine (FFGL plugins, drawn by the app in OpenGL).
//! Their shader entry starts with `ext:` (`shaders::is_external`). Each frame the engine lists what it needs
//! (`take_external_jobs`: key, size, parameter values) and, for an effect, copies the effect's input into the
//! key's `send` texture; the result is the key's `recv` texture, which the app filled after the last frame (one
//! frame late). Until the app has given textures for a key, an effect passes its picture through and a source
//! shows nothing. Costs nothing when no external effect is in use.

use super::*;

/// Textures not asked for in this many frames are dropped (the effect went away).
const KEEP_FRAMES: u64 = 120;

/// What the app should draw for one external effect or source this frame.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternalJob {
    /// Effect instance id (effects) or layer id (sources): one plugin instance each.
    pub key: u64,
    /// The effect / generator id (`ffgl:…`).
    pub id: &'static str,
    pub size: (u32, u32),
    /// Parameter values this frame (after modulation), in the shader description's order.
    pub params: Vec<f32>,
    /// Reads a picture (an effect; sources don't).
    pub has_input: bool,
}

struct Pair {
    send: Option<wgpu::TextureView>,
    /// None until the app's first result is ready (the picture passes through meanwhile).
    recv: Option<wgpu::TextureView>,
    size: (u32, u32),
    used: u64,
}

/// One plugin instance per effect *on each chain*: a "through an effect" transition runs the same effect on both
/// pictures, and its made-up effect id is shared by every layer.
pub(super) fn effect_key(res_id: u64, fx_id: u64) -> u64 {
    res_id.rotate_left(32) ^ fx_id ^ 0x5bd1_e995_0000_0000
}

#[derive(Default)]
pub(crate) struct Externals {
    pairs: HashMap<u64, Pair>,
    jobs: Vec<ExternalJob>,
    /// The last frame (`frame_index` after it) the GPU has finished.
    done: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Engine {
    /// The textures for `key` (from the app; they share memory with what it draws in): `send` gets the effect's
    /// input every frame (needs RENDER_ATTACHMENT), `recv` is shown as its result (None = not ready yet: the
    /// picture passes through).
    pub fn set_external_textures(&mut self, key: u64, send: Option<&wgpu::Texture>, recv: Option<&wgpu::Texture>) {
        let Some(size) = send.or(recv).map(|t| (t.width(), t.height())) else {
            self.externals.pairs.remove(&key);
            return;
        };
        let used = self.frame_index;
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        self.externals.pairs.insert(key, Pair { send: send.map(view), recv: recv.map(view), size, used });
    }

    /// The GPU has finished every frame submitted so far (non-blocking check): another API can now read what the
    /// engine drew and write what it last read.
    pub fn gpu_caught_up(&self) -> bool {
        let _ = self.gpu.device.poll(wgpu::PollType::Poll);
        self.externals.done.load(std::sync::atomic::Ordering::Acquire) >= self.frame_index
    }

    /// After a frame is submitted: note when the GPU is done with it (`gpu_caught_up`).
    pub(super) fn mark_external_frame(&self) {
        let done = self.externals.done.clone();
        let frame = self.frame_index + 1;
        self.gpu.queue.on_submitted_work_done(move || done.store(frame, std::sync::atomic::Ordering::Release));
    }

    /// What external effects and sources the last frame drew (take it after each render: the next render starts a
    /// new list).
    pub fn take_external_jobs(&mut self) -> Vec<ExternalJob> {
        std::mem::take(&mut self.externals.jobs)
    }

    /// A new frame: a new job list (jobs nobody took are dropped).
    pub(super) fn externals_new_frame(&mut self) {
        self.externals.jobs.clear();
    }

    /// Forgets textures nothing asked for lately (the effect went away).
    pub(super) fn sweep_externals(&mut self) {
        let now = self.frame_index;
        self.externals.pairs.retain(|_, p| now - p.used < KEEP_FRAMES);
    }

    /// An external effect on `input` (inside `apply_effects`): queues the job, copies the input to `send`, and
    /// returns the plugin's last result (None = nothing yet: pass the picture through).
    pub(super) fn external_effect(&mut self, res_id: u64, fx_id: u64, desc: &'static shaders::ShaderDesc, params: Vec<f32>, input: &wgpu::TextureView, size: (u32, u32), passes: &mut Vec<Pass>) -> Option<wgpu::TextureView> {
        let key = effect_key(res_id, fx_id);
        self.externals.jobs.push(ExternalJob { key, id: desc.id, size, params, has_input: true });
        let frame = self.frame_index;
        let pair = self.externals.pairs.get_mut(&key).filter(|p| p.size == size)?;
        pair.used = frame;
        if let Some(send) = pair.send.clone() {
            let src = Frame::Rgb { view: input.clone(), w: size.0, h: size.1 };
            push_fit_copy(send, &src, size, crate::FitMode::Stretch, size, passes);
        }
        pair.recv.clone()
    }

    /// An external source on layer `key`: queues the job and returns its last picture.
    pub(super) fn external_source(&mut self, key: u64, desc: &'static shaders::ShaderDesc, params: Vec<f32>, size: (u32, u32)) -> Option<Frame> {
        self.externals.jobs.push(ExternalJob { key, id: desc.id, size, params, has_input: false });
        let frame = self.frame_index;
        let pair = self.externals.pairs.get_mut(&key).filter(|p| p.size == size)?;
        pair.used = frame;
        Some(Frame::Rgb { view: pair.recv.clone()?, w: size.0, h: size.1 })
    }
}
