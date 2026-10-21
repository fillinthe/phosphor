//! Per-layer passes: resolving a source into a frame, transitions, multi-pass shaders and effect chains.

use super::*;

impl Engine {
    /// Resolves a source spec into a frame, rendering generators into the layer's slot
    /// (`outgoing` selects the transition slot).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn resolve_spec(
        &mut self,
        layer_id: u64,
        spec: &SourceSpec,
        values: &[f32],
        value_mods: &[crate::model::ParamMod],
        outgoing: bool,
        w: u32,
        h: u32,
        time: f32,
        mods: &ModInputs,
        passes: &mut Vec<Pass>,
    ) -> Option<Frame> {
        match spec {
            SourceSpec::None => None,
            SourceSpec::Generator(id) => {
                let desc = shaders::generator(id)?;
                if !self.ensure_module(desc.entry) {
                    return None;
                }
                let (tex0, tex1, tex2) = self.audio_extra(desc.entry);
                let device = &self.gpu.device;
                let res = self.layers.entry(layer_id).or_default();
                let slot = if outgoing { &mut res.generator_out } else { &mut res.generator };
                Target::ensure(slot, device, "generator", w, h);
                let target = slot.as_ref().unwrap();
                if let Some(plan) = shaders::pass_plan(desc.entry) {
                    let out = target.view.clone();
                    let params = resolve_params(desc, values, value_mods, mods);
                    let key = if outgoing { GENERATOR_OUT_KEY } else { GENERATOR_KEY };
                    let blank = tex0.unwrap_or_else(|| self.gpu.blank.clone());
                    let view = self.run_plan(layer_id, key, desc.entry, &plan, &params, blank, out, (w, h), time, passes);
                    return Some(Frame::Rgb { view, w, h });
                }
                let mut u = Uniforms::with_params(&resolve_params(desc, values, value_mods, mods));
                u.resolution = [w as f32, h as f32];
                u.time = time;
                passes.push(Pass {
                    entry: desc.entry,
                    target: target.view.clone(),
                    tex0: tex0.unwrap_or_else(|| self.gpu.blank.clone()),
                    tex1,
                    tex2,
                    uniforms: u, mesh: None, bufs: Vec::new() });
                Some(Frame::Rgb { view: target.view.clone(), w, h })
            }
            SourceSpec::Feed { id, .. } => {
                // Copied into the layer's own texture, so a layer can feed on itself (feedback) safely.
                let (view, fw, fh) = if *id == MASTER_ID {
                    self.output.as_ref().map(|t| (t.view.clone(), t.width, t.height))?
                } else {
                    self.layers.get(id)?.feed.as_ref().map(|t| (t.view.clone(), t.width, t.height))?
                };
                let device = &self.gpu.device;
                let res = self.layers.entry(layer_id).or_default();
                let slot = if outgoing { &mut res.generator_out } else { &mut res.generator };
                Target::ensure(slot, device, "feed copy", w, h);
                let target = slot.as_ref().unwrap().view.clone();
                push_fit_copy(target.clone(), &Frame::Rgb { view, w: fw, h: fh }, (w, h), crate::FitMode::Stretch, (w, h), passes);
                Some(Frame::Rgb { view: target, w, h })
            }
            SourceSpec::Image { .. } | SourceSpec::Movie { .. } | SourceSpec::Live { .. } | SourceSpec::Text(_) => {
                let res = self.layers.entry(layer_id).or_default();
                let slot = if outgoing { &res.media_out } else { &res.media };
                match slot.as_ref()? {
                    Media::Rgba(t) => Some(Frame::Rgb { view: t.view.clone(), w: t.width, h: t.height }),
                    Media::Nv12 { luma, chroma, full_range } => Some(Frame::Yuv {
                        luma: luma.view.clone(),
                        chroma: chroma.view.clone(),
                        w: luma.width,
                        h: luma.height,
                        full_range: *full_range,
                    }),
                    Media::Compressed { target: t, alpha, format } => Some(if *format == CompressedFormat::Bc3YCoCg {
                        Frame::YCoCg { view: t.view.clone(), alpha: alpha.as_ref().map(|a| a.view.clone()), w: t.width, h: t.height }
                    } else {
                        Frame::Rgb { view: t.view.clone(), w: t.width, h: t.height }
                    }),
                }
            }
        }
    }

    /// The audio pictures an ISF shader's texture slots 0, 1 and 2 want (None = nothing special there).
    /// Generators without an input picture get their first audio input at slot 0.
    fn audio_extra(&self, entry: &str) -> (Option<wgpu::TextureView>, Option<wgpu::TextureView>, Option<wgpu::TextureView>) {
        let Some(slots) = shaders::audio_slots(entry) else { return (None, None, None) };
        let pick = |slot: usize| {
            slots.iter().find(|(s, _)| *s == slot).map(|(_, kind)| {
                let t = match kind {
                    shaders::AudioTexture::Wave => self.audio_wave.as_ref(),
                    shaders::AudioTexture::Fft => self.audio_fft.as_ref(),
                };
                t.map_or_else(|| self.gpu.blank.clone(), |t| t.view.clone())
            })
        };
        (pick(0), pick(1), pick(2))
    }

    /// Blends the outgoing source into the incoming one at the canvas size.
    #[allow(clippy::too_many_arguments)]
    /// A shader transition: both pictures fitted to the canvas, then the transition's own pass with `progress`
    /// (startImage = the old picture in tex0, endImage = the new one in tex1).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn shader_transition(
        &mut self,
        layer_id: u64,
        incoming: Frame,
        outgoing: Frame,
        t: &crate::model::Transition,
        desc: &'static crate::shaders::ShaderDesc,
        time: f32,
        fit: FitMode,
        canvas: (u32, u32),
        passes: &mut Vec<Pass>,
    ) -> Frame {
        let (w, h) = canvas;
        let device = &self.gpu.device;
        let res = self.layers.entry(layer_id).or_default();
        let [slot_in, slot_out, slot_result] = &mut res.trans;
        for slot in [&mut *slot_in, &mut *slot_out, &mut *slot_result] {
            Target::ensure(slot, device, "transition", w, h);
        }
        let (end, start, result) = (slot_in.as_ref().unwrap().view.clone(), slot_out.as_ref().unwrap().view.clone(), slot_result.as_ref().unwrap().view.clone());
        super::fit::push_fit_copy(end.clone(), &incoming, canvas, fit, canvas, passes);
        super::fit::push_fit_copy(start.clone(), &outgoing, canvas, fit, canvas, passes);
        let mut values = if t.values.len() == desc.params.len() { t.values.clone() } else { desc.defaults() };
        if let Some(i) = crate::shaders::progress_index(desc) {
            values[i] = t.progress(time);
        }
        let mut u = Uniforms::with_params(&values);
        u.resolution = [w as f32, h as f32];
        u.time = time;
        u.opacity = 1.0;
        passes.push(Pass { entry: desc.entry, target: result.clone(), tex0: start, tex1: Some(end), tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
        Frame::Rgb { view: result, w, h }
    }

    pub(super) fn crossfade(
        &mut self,
        layer_id: u64,
        incoming: Frame,
        outgoing: Frame,
        t: &crate::model::Transition,
        time: f32,
        fit: FitMode,
        canvas: (u32, u32),
        passes: &mut Vec<Pass>,
    ) -> Frame {
        let (w, h) = canvas;
        let device = &self.gpu.device;
        let res = self.layers.entry(layer_id).or_default();
        let mut base = Uniforms::with_params(&[]);
        base.time = time;
        // YUV sources first become RGB at their own size.
        let to_rgb = |frame: Frame, slot: &mut Option<Target>, passes: &mut Vec<Pass>| -> (wgpu::TextureView, (u32, u32)) {
            match frame {
                Frame::Rgb { view, w, h } => (view, (w, h)),
                other => {
                    let (w, h) = other.size();
                    Target::ensure(slot, device, "transition", w, h);
                    let view = slot.as_ref().unwrap().view.clone();
                    let (tex0, tex2, yuv) = other.bind();
                    let mut u = base;
                    u.resolution = [w as f32, h as f32];
                    u.yuv = yuv;
                    passes.push(Pass { entry: "fs_to_rgb", target: view.clone(), tex0, tex1: None, tex2, uniforms: u, mesh: None, bufs: Vec::new() });
                    (view, (w, h))
                }
            }
        };
        let [slot_in, slot_out, slot_result] = &mut res.trans;
        let (in_view, in_size) = to_rgb(incoming, slot_in, passes);
        let (out_view, out_size) = to_rgb(outgoing, slot_out, passes);
        Target::ensure(slot_result, device, "transition", w, h);
        let result = slot_result.as_ref().unwrap().view.clone();
        let mut u = base;
        u.resolution = [w as f32, h as f32];
        u.opacity = t.progress(time);
        u.mode = t.kind.shader_index();
        u.p[1][0] = t.kind.blend().map_or(0, |b| b.shader_index()) as f32;
        u.uv_xform = fit_transform(in_size, canvas, fit);
        u.p[0] = fit_transform(out_size, canvas, fit);
        passes.push(Pass { entry: "fs_crossfade", target: result.clone(), tex0: in_view, tex1: Some(out_view), tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
        Frame::Rgb { view: result, w, h }
    }

    /// Runs a multi-pass shader (ISF PASSES) for one effect or generator: each pass renders into
    /// the back copy of its buffer (or `out`) while every pass reads the front copies of all
    /// buffers, so nothing is read and written at once and persistent buffers feed back.
    /// Returns the view with the result (`out`, or the last buffer if no pass targets the output).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn run_plan(
        &mut self,
        res_id: u64,
        key: u64,
        entry: &'static str,
        plan: &shaders::PassPlan,
        params: &[f32],
        input: wgpu::TextureView,
        out: wgpu::TextureView,
        (w, h): (u32, u32),
        time: f32,
        passes: &mut Vec<Pass>,
    ) -> wgpu::TextureView {
        let device = self.gpu.device.clone();
        let blank = self.gpu.blank.clone();
        let extra = self.audio_extra(entry);
        let frame = self.frame_index as f32;
        let n = plan.buffers.min(shaders::MAX_BUFFERS);
        let bufs = self.layers.entry(res_id).or_default().multipass.entry(key).or_default();
        bufs.slots.resize_with(n, Default::default);
        bufs.front.resize(n, 0);
        let var = |name: &str| -> Option<f64> {
            match name {
                "$WIDTH" => Some(w as f64),
                "$HEIGHT" => Some(h as f64),
                _ => plan.vars.iter().find(|(v, _)| v == name).and_then(|(_, i)| params.get(*i)).map(|v| *v as f64),
            }
        };
        let size = |e: &Option<String>, full: u32| e.as_deref().and_then(|e| crate::expr::eval(e, &var)).map_or(full, |v| (v.max(1.0) as u32).min(8192));
        let mut result = out.clone();
        for (k, pp) in plan.passes.iter().enumerate() {
            let views: Vec<wgpu::TextureView> = (0..n)
                .map(|b| {
                    if plan.images.contains(&b) {
                        return self.plan_images.get(&(entry, b)).map_or_else(|| blank.clone(), |t| t.view.clone());
                    }
                    bufs.slots[b][bufs.front[b]].as_ref().map_or_else(|| blank.clone(), |t| t.view.clone())
                })
                .collect();
            let (target, tw, th) = match pp.target {
                Some(b) if b < n => {
                    let (bw, bh) = (size(&pp.width, w), size(&pp.height, h));
                    // A buffer any pass asks to be float is float for all of them (16-bit, filterable).
                    let float = plan.passes.iter().any(|p| p.target == Some(b) && p.float);
                    let format = if float { wgpu::TextureFormat::Rgba16Float } else { crate::gpu::FORMAT };
                    for slot in bufs.slots[b].iter_mut() {
                        Target::ensure_format(slot, &device, "pass buffer", bw, bh, format);
                    }
                    let write = 1 - bufs.front[b];
                    bufs.front[b] = write;
                    let view = bufs.slots[b][write].as_ref().unwrap().view.clone();
                    (view, bw, bh)
                }
                _ => (out.clone(), w, h),
            };
            let mut u = Uniforms::with_params(params);
            u.resolution = [tw as f32, th as f32];
            u.time = time;
            // PASSINDEX, FRAMEINDEX, TIMEDELTA (the reserved last vec4).
            u.p[15] = [k as f32, frame, 1.0 / 60.0, 0.0];
            result = target.clone();
            let (_, tex1, tex2) = extra.clone();
            passes.push(Pass { entry, target, tex0: input.clone(), tex1, tex2, uniforms: u, mesh: None, bufs: views });
        }
        result
    }

    /// Runs the layer's enabled effects at the source's native resolution (ping-pong).
    pub(super) fn apply_effects(
        &mut self,
        res_id: u64,
        effects: &[EffectState],
        frame: Frame,
        time: f32,
        mods: &ModInputs,
        all_layers: &[LayerState],
        passes: &mut Vec<Pass>,
    ) -> Frame {
        let chain: Vec<_> = effects
            .iter()
            .filter(|e| e.enabled)
            .filter_map(|e| shaders::effect(&e.effect).map(|d| (d, e.id, &e.values, &e.mods, e.mix)))
            .collect();
        if chain.is_empty() {
            return frame;
        }
        let (w, h) = frame.size();
        let device = &self.gpu.device;
        let res = self.layers.entry(res_id).or_default();
        for slot in &mut res.fx {
            Target::ensure(slot, device, "fx", w, h);
        }
        let views: Vec<_> = res.fx.iter().map(|t| t.as_ref().unwrap().view.clone()).collect();
        let mut base = Uniforms::with_params(&[]);
        base.resolution = [w as f32, h as f32];
        base.time = time;

        // Ping-pong across three views, never writing the one being read or the one holding
        // the chain's dry input.
        let mut input_idx: Option<usize> = None;
        let mut input = match frame {
            Frame::Rgb { view, .. } => view,
            other => {
                let (tex0, tex2, yuv) = other.bind();
                passes.push(Pass {
                    entry: "fs_to_rgb",
                    target: views[0].clone(),
                    tex0,
                    tex1: None,
                    tex2,
                    uniforms: Uniforms { yuv, ..base }, mesh: None, bufs: Vec::new() });
                input_idx = Some(0);
                views[0].clone()
            }
        };
        let parity = (self.frame_index % 2) as usize;
        for (desc, fx_id, values, param_mods, mix) in chain {
            if !self.ensure_module(desc.entry) {
                continue;
            }
            let dry = input.clone();
            let dry_idx = input_idx;
            let params = resolve_params(desc, values, param_mods, mods);
            if let Some(plan) = shaders::pass_plan(desc.entry) {
                // Multi-pass shader (ISF PASSES): its own buffers, output into a free chain view.
                let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                let result = self.run_plan(res_id, fx_id, desc.entry, &plan, &params, input.clone(), views[i].clone(), (w, h), time, passes);
                input_idx = (result == views[i]).then_some(i);
                input = result;
            } else {
                let sub = shaders::multipass(desc.entry);
                let entries: Vec<&'static str> = if sub.is_empty() { vec![desc.entry] } else { sub.to_vec() };
                let last_index = entries.len() - 1;
                for (k, entry) in entries.into_iter().enumerate() {
                    let mut u = Uniforms::with_params(&params);
                    u.resolution = base.resolution;
                    u.time = time;
                    let mut tex1 = None;
                    let mut tex2 = None;
                    let mut next_idx: Option<usize> = None;
                    let target = if entry == "fs_rutt" || entry == "fs_rutt_points" {
                        let scale = params.get(6).copied().unwrap_or(2.0).max(1.0);
                        let (rw, rh) = ((w as f32 / scale) as u32, (h as f32 / scale) as u32);
                        // Optional displacement from another layer's picture (its live thumbnail).
                        let other = params.get(12).copied().unwrap_or(0.0) as usize;
                        if other > 0 && let Some(l) = all_layers.get(other - 1) {
                            tex2 = self.layers.get(&l.id).and_then(|r| r.thumbnail.as_ref()).map(|t| t.view.clone());
                        }
                        let res = self.layers.entry(res_id).or_default();
                        Target::ensure(&mut res.rutt, &self.gpu.device, "rutt", rw, rh);
                        u.resolution = [rw.max(1) as f32, rh.max(1) as f32];
                        res.rutt.as_ref().unwrap().view.clone()
                    } else if entry == "fx_trails" || entry == "fx_slitscan" || entry == "fx_flow_keep" {
                        // History effects write one slot while reading the other.
                        let res = self.layers.entry(res_id).or_default();
                        let slots = res.history.entry(fx_id).or_default();
                        for slot in slots.iter_mut() {
                            Target::ensure(slot, &self.gpu.device, "history", w, h);
                        }
                        tex1 = Some(slots[1 - parity].as_ref().unwrap().view.clone());
                        slots[parity].as_ref().unwrap().view.clone()
                    } else if entry == "fx_flow" {
                        // Motion flow: the previous frame's picture (kept by fx_flow_keep) on tex1, output into the chain.
                        let res = self.layers.entry(res_id).or_default();
                        let slots = res.history.entry(fx_id).or_default();
                        for slot in slots.iter_mut() {
                            Target::ensure(slot, &self.gpu.device, "history", w, h);
                        }
                        tex1 = Some(slots[1 - parity].as_ref().unwrap().view.clone());
                        let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                        next_idx = Some(i);
                        views[i].clone()
                    } else {
                        let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                        next_idx = Some(i);
                        views[i].clone()
                    };
                    if k == last_index && entry == "fx_glow_add" {
                        tex1 = Some(dry.clone());
                    }
                    if entry == "fx_cutout" {
                        // Remove background: the matte from the app on tex2; p[14].w says one is there yet.
                        tex2 = self.cutout_matte(res_id, &input, (w, h), passes);
                        u.p[14][3] = if tex2.is_some() { 1.0 } else { 0.0 };
                    }
                    let (_, a1, a2) = self.audio_extra(entry);
                    tex1 = tex1.or(a1);
                    tex2 = tex2.or(a2);
                    passes.push(Pass { entry, target: target.clone(), tex0: input, tex1, tex2, uniforms: u, mesh: None, bufs: Vec::new() });
                    input = target;
                    input_idx = next_idx;
                }
            }
            if mix < 0.999 {
                let i = (0..3).find(|i| Some(*i) != input_idx && Some(*i) != dry_idx).unwrap_or(0);
                let u = Uniforms { opacity: mix, ..base };
                passes.push(Pass { entry: "fs_mix", target: views[i].clone(), tex0: input, tex1: Some(dry), tex2: None, uniforms: u, mesh: None, bufs: Vec::new() });
                input = views[i].clone();
                input_idx = Some(i);
            }
        }
        Frame::Rgb { view: input, w, h }
    }
}
