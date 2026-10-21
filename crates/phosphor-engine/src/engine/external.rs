//! External effects and sources: pictures made outside the engine (FFGL plugins, drawn by the app in OpenGL).
//! Their shader entry starts with `ext:` (`shaders::is_external`). Each frame the engine lists what it needs
//! (`take_external_jobs`: key, size, parameter values) and, for an effect, copies the effect's input into the
//! key's `send` texture; the result is the key's `recv` texture, which the app filled after the last frame (one
//! frame late). Until the app has given textures for a key, an effect passes its picture through and a source
//! shows nothing. A mixer (two pictures: the stack below and a layer) blends a layer in its own way
//! (`external_mix`); until its first result it blends normally. Costs nothing when nothing external is in use.

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
    /// Pictures it reads: 0 (a source), 1 (an effect) or 2 (a mixer: the stack below, then the layer).
    pub inputs: u32,
}

struct Pair {
    /// One per input picture.
    send: Vec<wgpu::TextureView>,
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
    /// The textures for `key` (from the app; they share memory with what it draws in): `send` gets the input
    /// pictures every frame (needs RENDER_ATTACHMENT), `recv` is shown as its result (None = not ready yet: the
    /// picture passes through, a mixer blends normally).
    pub fn set_external_textures(&mut self, key: u64, send: &[&wgpu::Texture], recv: Option<&wgpu::Texture>) {
        let Some(size) = send.first().copied().or(recv).map(|t| (t.width(), t.height())) else {
            self.externals.pairs.remove(&key);
            return;
        };
        let used = self.frame_index;
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        self.externals.pairs.insert(key, Pair { send: send.iter().map(|t| view(t)).collect(), recv: recv.map(view), size, used });
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
        self.externals.jobs.push(ExternalJob { key, id: desc.id, size, params, inputs: 1 });
        let frame = self.frame_index;
        let pair = self.externals.pairs.get_mut(&key).filter(|p| p.size == size)?;
        pair.used = frame;
        if let Some(send) = pair.send.first().cloned() {
            let src = Frame::Rgb { view: input.clone(), w: size.0, h: size.1 };
            push_fit_copy(send, &src, size, crate::FitMode::Stretch, size, passes);
        }
        pair.recv.clone()
    }

    /// An external source on layer `key`: queues the job and returns its last picture.
    pub(super) fn external_source(&mut self, key: u64, desc: &'static shaders::ShaderDesc, params: Vec<f32>, size: (u32, u32)) -> Option<Frame> {
        self.externals.jobs.push(ExternalJob { key, id: desc.id, size, params, inputs: 0 });
        let frame = self.frame_index;
        let pair = self.externals.pairs.get_mut(&key).filter(|p| p.size == size)?;
        pair.used = frame;
        Some(Frame::Rgb { view: pair.recv.clone()?, w: size.0, h: size.1 })
    }
}

/// Key salt for a layer's mixer instance (one per layer).
const MIXER_SALT: u64 = 0x6d69_7865_7200_0000;

impl Engine {
    /// A layer blended by a mixer plugin (inside `mix_stack`): queues the job, copies the stack so far (`below`,
    /// None = nothing yet) and the layer's picture (fitted) to its inputs, and when the plugin's last result is
    /// there, puts it on the stack at `opacity` (`current` moves on). Returns false when there's no result yet
    /// (blend the layer normally this frame).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn external_mix(&mut self, layer: u64, desc: &'static shaders::ShaderDesc, params: Vec<f32>, frame: &Frame, fit: FitMode, opacity: f32, base: usize, current: &mut Option<usize>, canvas: (u32, u32), passes: &mut Vec<Pass>) -> bool {
        let key = effect_key(layer, MIXER_SALT);
        // Like Resolume: a mixer with an "Opacity" setting gets the layer's opacity there (and isn't faded again).
        let mut params = params;
        let own = desc.params.iter().position(|p| p.name.eq_ignore_ascii_case("opacity"));
        if let Some(i) = own
            && let Some(v) = params.get_mut(i)
        {
            *v = opacity;
        }
        let opacity = if own.is_some() { 1.0 } else { opacity };
        self.externals.jobs.push(ExternalJob { key, id: desc.id, size: canvas, params, inputs: 2 });
        let frame_index = self.frame_index;
        let Some(pair) = self.externals.pairs.get_mut(&key).filter(|p| p.size == canvas && p.send.len() == 2) else { return false };
        pair.used = frame_index;
        let (send, recv) = (pair.send.clone(), pair.recv.clone());
        let below = current.map_or_else(|| self.gpu.blank.clone(), |c| self.accum[base + c].as_ref().unwrap().view.clone());
        push_fit_copy(send[0].clone(), &Frame::Rgb { view: below.clone(), w: canvas.0, h: canvas.1 }, canvas, FitMode::Stretch, canvas, passes);
        push_fit_copy(send[1].clone(), frame, canvas, fit, canvas, passes);
        let Some(result) = recv else { return false };
        // The plugin's picture over the stack at the layer's opacity (fs_mix: tex1 = what's below).
        let next = current.map_or(0, |c| 1 - c);
        let mut u = Uniforms::with_params(&[]);
        u.resolution = [canvas.0 as f32, canvas.1 as f32];
        u.opacity = opacity;
        passes.push(Pass { entry: "fs_mix", target: self.accum[base + next].as_ref().unwrap().view.clone(), tex0: result, tex1: Some(below), tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
        *current = Some(next);
        true
    }
}
