//! Presenting the output windows from the render thread: each window whose surface the interface handed over gets
//! the output's picture, letterboxed on black exactly as the interface draws it (`ui::widgets::fit_rect`, egui's
//! colour handling and dithering), then presented.

use std::collections::HashMap;
use std::sync::Arc;

use eframe::egui::ViewportId;
use eframe::egui_wgpu::winit::{SharedSurface, shared_surface};
use phosphor_engine::wgpu;

use super::{Gpu, OutputWindow};

/// The output picture onto a window: a triangle over the letterbox rectangle (the viewport), sampled as egui samples a
/// picture (no sRGB decoding; dithered to 8 bits; written out linear on an sRGB surface).
const SHADER: &str = r"
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) uv: vec2<f32> };

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: VsOut;
    out.pos = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var pick: sampler;

fn dither(rgb: vec3<f32>, at: vec4<f32>) -> vec3<f32> {
    let f = 0.06711056 * at.x + 0.00583715 * at.y;
    let noise = (fract(52.9829189 * fract(f)) - 0.5) * 0.95;
    return rgb + noise / 255.0;
}

fn linear_from_gamma(srgb: vec3<f32>) -> vec3<f32> {
    let lower = srgb / vec3<f32>(12.92);
    let higher = pow((srgb + vec3<f32>(0.055)) / vec3<f32>(1.055), vec3<f32>(2.4));
    return select(higher, lower, srgb < vec3<f32>(0.04045));
}

@fragment
fn fs_gamma(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(dither(textureSample(picture, pick, in.uv).rgb, in.pos), 1.0);
}

@fragment
fn fs_linear(in: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(linear_from_gamma(dither(textureSample(picture, pick, in.uv).rgb, in.pos)), 1.0);
}
";

/// How one window's surface is set up now (configured again when any of it changes).
#[derive(Clone, Copy, PartialEq, Debug)]
struct Setup {
    width: u32,
    height: u32,
    mode: wgpu::PresentMode,
}

pub(super) struct Presenter {
    gpu: Gpu,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    setups: HashMap<ViewportId, Setup>,
    /// Windows handed over to this thread (to hand back when it stops).
    held: HashMap<ViewportId, Arc<SharedSurface>>,
    /// The last frame waited for the first window's refresh (it paces the thread at the display's rate).
    vsync_paced: bool,
    /// Windows this thread let go of while holding their last reference: dropped on the main thread.
    dead: Vec<Arc<SharedSurface>>,
    /// Windows whose surface wouldn't set up (said once each).
    refused: std::collections::HashSet<ViewportId>,
}

impl Presenter {
    pub(super) fn new(gpu: Gpu) -> Self {
        let device = &gpu.device;
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("output present"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("output present"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture { sample_type: wgpu::TextureSampleType::Float { filterable: true }, view_dimension: wgpu::TextureViewDimension::D2, multisampled: false },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry { binding: 1, visibility: wgpu::ShaderStages::FRAGMENT, ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering), count: None },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: Some("output present"), bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
        let entry = if gpu.format.is_srgb() { "fs_linear" } else { "fs_gamma" };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("output present"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: Some("vs_main"), compilation_options: Default::default(), buffers: &[] },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: gpu.format, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("output present"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self { gpu, pipeline, layout, sampler, setups: HashMap::new(), held: HashMap::new(), vsync_paced: false, dead: Vec::new(), refused: Default::default() }
    }

    pub(super) fn vsync_paced(&self) -> bool {
        self.vsync_paced
    }

    /// Window surfaces this thread let go of last (the caller hands them to the main thread).
    pub(super) fn take_dead(&mut self) -> Vec<Arc<SharedSurface>> {
        std::mem::take(&mut self.dead)
    }

    /// Lets go of a surface: on the main thread if this was its last reference (its window closes then).
    fn let_go(&mut self, surface: Arc<SharedSurface>) {
        if Arc::strong_count(&surface) == 1 {
            self.dead.push(surface);
        }
    }

    /// Shows each output's picture in its window, the first window waiting for its screen's refresh; the others too
    /// unless the screens don't wait for each other (then the newest picture at their own refresh, where they can).
    pub(super) fn present(&mut self, pictures: &[(OutputWindow, wgpu::TextureView)], dont_wait: bool) {
        let mut paced = false;
        let mut first = true;
        for (out, view) in pictures {
            let Some(surface) = shared_surface(out.viewport) else { continue };
            if !surface.app_presents() {
                // The interface paints this window now (its mapping handles are out, or it's being looked at).
                self.setups.remove(&out.viewport);
                if let Some(s) = self.held.remove(&out.viewport) {
                    self.let_go(s);
                }
                self.let_go(surface);
                continue;
            }
            let mode = if first || !dont_wait { wgpu::PresentMode::AutoVsync } else { wgpu::PresentMode::Mailbox };
            if self.show(out, view, &surface, mode) && first {
                paced = true;
            }
            first = false;
            if let Some(old) = self.held.insert(out.viewport, surface) {
                self.let_go(old);
            }
        }
        // Windows no longer shown (closed, or their output turned off): the interface paints them again.
        let shown: Vec<ViewportId> = pictures.iter().map(|(o, _)| o.viewport).collect();
        let gone: Vec<ViewportId> = self.held.keys().filter(|v| !shown.contains(v)).copied().collect();
        for v in gone {
            if let Some(s) = self.held.remove(&v) {
                s.set_app_presents(false);
                self.let_go(s);
            }
            self.setups.remove(&v);
        }
        self.vsync_paced = paced;
    }

    /// Every window back to the interface (the thread is stopping).
    pub(super) fn release_all(&mut self) {
        let held: Vec<Arc<SharedSurface>> = self.held.drain().map(|(_, s)| s).collect();
        for s in held {
            s.set_app_presents(false);
            self.let_go(s);
        }
        self.setups.clear();
    }

    /// One window's frame. True if it waited for the screen (a frame was presented with vsync).
    fn show(&mut self, out: &OutputWindow, view: &wgpu::TextureView, shared: &SharedSurface, mode: wgpu::PresentMode) -> bool {
        let (width, height) = shared.size();
        if width == 0 || height == 0 {
            return false;
        }
        let (_held, surface, must_configure) = shared.take();
        // (The interface may have taken the window back while this thread waited for the lock.)
        if !shared.app_presents() {
            return false;
        }
        let caps = surface.get_capabilities(&self.gpu.adapter);
        let mode = match mode {
            wgpu::PresentMode::Mailbox if !caps.present_modes.contains(&wgpu::PresentMode::Mailbox) => wgpu::PresentMode::AutoVsync,
            m => m,
        };
        let setup = Setup { width, height, mode };
        if must_configure || self.setups.get(&out.viewport) != Some(&setup) {
            if !self.configure(surface, setup, out.viewport) {
                return false;
            }
            shared.app_configured();
            self.setups.insert(out.viewport, setup);
        }
        let frame = match surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => f,
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => {
                // Shown, and set up again next frame.
                self.setups.remove(&out.viewport);
                f
            }
            // Covered, minimized, or out of date (moved to another screen): set up again and try next frame.
            _ => {
                self.setups.remove(&out.viewport);
                return false;
            }
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.draw(&target, (width, height), view, out.aspect);
        self.gpu.queue.present(frame);
        crate::present::note(out.viewport);
        mode != wgpu::PresentMode::Mailbox
    }

    /// The picture onto `target` (`size` pixels): black, with the picture letterboxed at `aspect`.
    fn draw(&self, target: &wgpu::TextureView, size: (u32, u32), picture: &wgpu::TextureView, aspect: f32) {
        let bind = self.gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("output present"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(picture) }, wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) }],
        });
        let mut encoder = self.gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("output present") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("output present"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: wgpu::StoreOp::Store },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            let [x, y, w, h] = letterbox(size.0 as f32, size.1 as f32, aspect);
            if w >= 1.0 && h >= 1.0 {
                pass.set_viewport(x, y, w, h, 0.0, 1.0);
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &bind, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.gpu.queue.submit([encoder.finish()]);
    }

    /// Sets a window's surface up for this thread. False (said once) if the driver refuses.
    fn configure(&mut self, surface: &wgpu::Surface<'static>, setup: Setup, viewport: ViewportId) -> bool {
        let Some(base) = surface.get_default_config(&self.gpu.adapter, setup.width, setup.height) else { return false };
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: self.gpu.format,
            present_mode: setup.mode,
            view_formats: vec![self.gpu.format],
            desired_maximum_frame_latency: 2,
            ..base
        };
        // (Tried again if another thread submitted while wgpu waited for the device to go idle.)
        let error = eframe::egui_wgpu::winit::configure_retrying(&self.gpu.device, surface, &config);
        match error {
            None => {
                self.refused.remove(&viewport);
                true
            }
            Some(e) => {
                if self.refused.insert(viewport) {
                    let detail = match &e {
                        wgpu::Error::Validation { description, .. } => description.split_whitespace().collect::<Vec<_>>().join(" "),
                        other => other.to_string(),
                    };
                    log::error!("an output window's surface couldn't be set up for the render thread ({}x{}): {detail}", setup.width, setup.height);
                }
                false
            }
        }
    }
}

/// Where the picture goes in a `width` × `height` window (pixels: x, y, w, h): as large as fits at `aspect`, centred
/// (`ui::widgets::fit_rect`, the same in pixels as in points).
fn letterbox(width: f32, height: f32, aspect: f32) -> [f32; 4] {
    let aspect = if aspect.is_finite() && aspect > 0.0 { aspect } else { width / height.max(1.0) };
    let (w, h) = if width / height.max(1.0) > aspect { (height * aspect, height) } else { (width, width / aspect) };
    [(width - w) / 2.0, (height - h) / 2.0, w, h]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::fit_rect;
    use eframe::egui;

    /// A device to draw on, or None (no GPU here: CI without one skips).
    fn gpu(format: wgpu::TextureFormat) -> Option<Gpu> {
        let instance = wgpu::Instance::default();
        let adapter = pollster_wait(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
        let (device, queue) = pollster_wait(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
        Some(Gpu { adapter, device, queue, format })
    }

    fn pollster_wait<F: std::future::Future>(fut: F) -> F::Output {
        let mut fut = std::pin::pin!(fut);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        loop {
            if let std::task::Poll::Ready(v) = fut.as_mut().poll(&mut cx) {
                return v;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    /// What `Presenter::draw` puts in a `w` × `h` window for a one-colour picture at `aspect`.
    fn drawn(format: wgpu::TextureFormat, w: u32, h: u32, colour: [u8; 4], aspect: f32) -> Option<Vec<u8>> {
        let gpu = gpu(format)?;
        let p = Presenter::new(gpu.clone());
        let tex = |w: u32, h: u32, format, usage| {
            gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: None,
                size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let picture = tex(4, 2, wgpu::TextureFormat::Rgba8Unorm, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
        let pixels: Vec<u8> = colour.iter().copied().cycle().take(4 * 2 * 4).collect();
        gpu.queue.write_texture(picture.as_image_copy(), &pixels, wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(16), rows_per_image: None }, picture.size());
        let target = tex(w, h, format, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
        p.draw(&target.create_view(&Default::default()), (w, h), &picture.create_view(&Default::default()), aspect);
        let row = (w * 4).div_ceil(256) * 256;
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * h) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = gpu.device.create_command_encoder(&Default::default());
        enc.copy_texture_to_buffer(
            target.as_image_copy(),
            wgpu::TexelCopyBufferInfo { buffer: &buffer, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(row), rows_per_image: None } },
            target.size(),
        );
        gpu.queue.submit([enc.finish()]);
        buffer.slice(..).map_async(wgpu::MapMode::Read, |_| {});
        gpu.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;
        let data = buffer.slice(..).get_mapped_range().ok()?;
        Some((0..h).flat_map(|y| data[(y * row) as usize..(y * row + w * 4) as usize].to_vec()).collect())
    }

    #[test]
    fn the_picture_is_letterboxed_on_black_as_the_interface_draws_it() {
        // A 2:1 picture in a 64 × 64 window: black bands of 16 rows above and below.
        let colour = [200, 100, 50, 255];
        let Some(px) = drawn(wgpu::TextureFormat::Rgba8Unorm, 64, 64, colour, 2.0) else { return };
        let at = |x: usize, y: usize| &px[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        for (x, y) in [(32, 32), (0, 16), (63, 47), (10, 40)] {
            let p = at(x, y);
            for c in 0..3 {
                assert!((p[c] as i32 - colour[c] as i32).abs() <= 1, "({x},{y}) = {p:?}: the picture's colour, dithered by at most one step");
            }
            assert_eq!(p[3], 255, "opaque");
        }
        for (x, y) in [(32, 0), (32, 15), (0, 48), (63, 63)] {
            assert_eq!(at(x, y), &[0, 0, 0, 255], "({x},{y}): black around the picture");
        }
    }

    #[test]
    fn an_srgb_window_gets_the_same_colour() {
        // egui writes linear values to an sRGB window (it encodes them back): the same bytes come out.
        let colour = [200, 100, 50, 255];
        let Some(px) = drawn(wgpu::TextureFormat::Rgba8UnormSrgb, 32, 32, colour, 1.0) else { return };
        let p = &px[(16 * 32 + 16) * 4..(16 * 32 + 16) * 4 + 4];
        for c in 0..3 {
            assert!((p[c] as i32 - colour[c] as i32).abs() <= 1, "{p:?}");
        }
    }

    #[test]
    fn letterbox_matches_the_interface() {
        for (w, h, aspect) in [(1920.0, 1080.0, 16.0 / 9.0), (1920.0, 1200.0, 16.0 / 9.0), (1080.0, 1920.0, 16.0 / 9.0), (1000.0, 1000.0, 0.5), (800.0, 600.0, 4.0 / 3.0)] {
            let r = fit_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(w, h)), aspect);
            let [x, y, bw, bh] = letterbox(w, h, aspect);
            for (a, b) in [(x, r.min.x), (y, r.min.y), (bw, r.width()), (bh, r.height())] {
                assert!((a - b).abs() < 0.01, "{w}x{h} @ {aspect}: {:?} vs {r:?}", [x, y, bw, bh]);
            }
        }
        // A broken aspect fills the window instead of drawing nothing.
        assert_eq!(letterbox(640.0, 360.0, 0.0), [0.0, 0.0, 640.0, 360.0]);
    }
}
