//! Web app outputs: each open output in a browser window of its own (a pop-up), so it can be dragged to a projector
//! and made full screen there (double-click), instead of a panel inside the interface.
//!
//! The picture never leaves the graphics card: each output is drawn into a canvas on this page (WebGPU surface, one
//! blit a frame), and the pop-up plays that canvas as a live video (`captureStream`, same-origin window).
//! Desktop builds use real windows (`PhosphorApp::output_window`).

use std::collections::HashMap;

use eframe::egui_wgpu::{self, wgpu};
use wasm_bindgen::JsCast;
use wasm_bindgen::closure::Closure;

/// One output's pop-up window.
struct PopOut {
    popup: web_sys::Window,
    canvas: web_sys::HtmlCanvasElement,
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    alpha: wgpu::CompositeAlphaMode,
    size: (u32, u32),
    /// Double-click → full screen (kept alive while the window is).
    _full_screen: Closure<dyn FnMut()>,
}

impl Drop for PopOut {
    fn drop(&mut self) {
        let _ = self.popup.close();
        self.canvas.remove();
    }
}

#[derive(Default)]
pub struct WebOutputs {
    windows: HashMap<u64, PopOut>,
    blitters: HashMap<wgpu::TextureFormat, wgpu::util::TextureBlitter>,
}

/// What `show` found: outputs whose window the person closed, and whether the browser refused to open one.
#[derive(Default)]
pub struct Shown {
    pub closed: Vec<u64>,
    pub blocked: bool,
}

impl WebOutputs {
    /// Keeps a window open for each of `open` (id, name, picture size) and draws each one's picture into it; closes the
    /// rest. `picture` gives an output's texture this frame. Call once a frame, after the engine rendered.
    pub fn show<'a>(&mut self, rs: &egui_wgpu::RenderState, open: &[(u64, String, (u32, u32))], picture: impl Fn(u64) -> Option<&'a wgpu::Texture>) -> Shown {
        let mut shown = Shown::default();
        // Closed by the person (its window's close button) or no longer open here.
        self.windows.retain(|id, w| {
            let wanted = open.iter().any(|(o, ..)| o == id);
            if wanted && w.popup.closed().unwrap_or(true) {
                shown.closed.push(*id);
                return false;
            }
            wanted
        });
        for (id, name, size) in open {
            if !self.windows.contains_key(id) && !shown.closed.contains(id) {
                match open_window(rs, *id, name, *size) {
                    Some(w) => {
                        self.windows.insert(*id, w);
                    }
                    None => {
                        shown.blocked = true;
                        shown.closed.push(*id);
                    }
                }
            }
        }
        let mut encoder = rs.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("web outputs") });
        let mut frames = Vec::new();
        for (id, w) in &mut self.windows {
            let Some(texture) = picture(*id) else { continue };
            let size = (texture.width().max(1), texture.height().max(1));
            if size != w.size {
                w.size = size;
                w.canvas.set_width(size.0);
                w.canvas.set_height(size.1);
                configure(&w.surface, rs, w.format, w.alpha, size);
            }
            let frame = match w.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
                // Out of date / lost: set it up again and draw next frame; hidden / busy: skip this one.
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    configure(&w.surface, rs, w.format, w.alpha, w.size);
                    continue;
                }
                _ => continue,
            };
            let target = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            let source = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let blitter = self.blitters.entry(w.format).or_insert_with(|| wgpu::util::TextureBlitter::new(&rs.device, w.format));
            blitter.copy(&rs.device, &mut encoder, &source, &target);
            frames.push(frame);
        }
        if !frames.is_empty() {
            rs.queue.submit([encoder.finish()]);
            for f in frames {
                rs.queue.present(f);
            }
        }
        shown
    }

    /// Closes every output window (outputs switched off).
    pub fn close_all(&mut self) {
        self.windows.clear();
    }
}

fn configure(surface: &wgpu::Surface<'static>, rs: &egui_wgpu::RenderState, format: wgpu::TextureFormat, alpha: wgpu::CompositeAlphaMode, (w, h): (u32, u32)) {
    surface.configure(
        &rs.device,
        &wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: alpha,
            view_formats: Vec::new(),
            desired_maximum_frame_latency: 2,
            color_space: Default::default(),
        },
    );
}

/// Opens output `id`'s pop-up and the canvas it plays. None = the browser refused (pop-ups blocked) or no WebGPU canvas.
fn open_window(rs: &egui_wgpu::RenderState, id: u64, name: &str, size: (u32, u32)) -> Option<PopOut> {
    let page = web_sys::window()?;
    let popup = page.open_with_url_and_target_and_features("", &format!("phosphor_output_{id}"), "popup,width=960,height=540").ok()??;
    let doc = popup.document()?;
    doc.set_title(&format!("Phosphor · {name}"));
    let body = doc.body()?;
    let _ = body.set_attribute("style", "margin:0;background:#000;overflow:hidden");
    body.set_inner_html("");
    // (Made in the pop-up's own document: its type belongs to that window, so a type check against this page's
    // always fails. It is a <video>.)
    let video: web_sys::HtmlVideoElement = doc.create_element("video").ok()?.unchecked_into();
    let _ = video.set_attribute("style", "position:fixed;inset:0;width:100vw;height:100vh;object-fit:contain;background:#000");
    let _ = video.set_attribute("playsinline", "");
    video.set_autoplay(true);
    video.set_muted(true);
    body.append_child(&video).ok()?;
    // How to use it, fading out by itself.
    let hint = doc.create_element("div").ok()?;
    let _ = hint.set_attribute(
        "style",
        "position:fixed;left:50%;bottom:24px;transform:translateX(-50%);padding:8px 14px;border-radius:6px;background:rgba(0,0,0,.7);color:#ddd;font:14px system-ui,sans-serif;pointer-events:none;transition:opacity 1s;opacity:1",
    );
    hint.set_text_content(Some(&crate::i18n::tr("Drag this window to the projector, then double-click for full screen (Esc leaves it).")));
    body.append_child(&hint).ok()?;
    let fade = hint.clone();
    let fade_out = Closure::once_into_js(move || {
        let _ = fade.set_attribute("style", &format!("{};opacity:0", fade.get_attribute("style").unwrap_or_default()));
    });
    let _ = popup.set_timeout_with_callback_and_timeout_and_arguments_0(fade_out.unchecked_ref(), 5000);
    let target = video.clone();
    let full_screen = Closure::<dyn FnMut()>::new(move || {
        let _ = target.request_fullscreen();
    });
    let _ = video.add_event_listener_with_callback("dblclick", full_screen.as_ref().unchecked_ref());

    // The canvas the output is drawn into: on this page (a WebGPU surface belongs to the page that made the device),
    // out of sight. Its stream plays in the pop-up.
    let canvas: web_sys::HtmlCanvasElement = page.document()?.create_element("canvas").ok()?.dyn_into().ok()?;
    let _ = canvas.set_attribute("style", "position:fixed;left:0;top:0;width:2px;height:2px;opacity:0;pointer-events:none");
    canvas.set_width(size.0.max(1));
    canvas.set_height(size.1.max(1));
    page.document()?.body()?.append_child(&canvas).ok()?;
    let surface = match rs.instance.create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone())) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("output window: no canvas surface ({e})");
            canvas.remove();
            let _ = popup.close();
            return None;
        }
    };
    let caps = surface.get_capabilities(&rs.adapter);
    let format = caps.formats.iter().copied().find(|f| !f.is_srgb()).or_else(|| caps.formats.first().copied())?;
    let alpha = caps.alpha_modes.iter().copied().find(|a| *a == wgpu::CompositeAlphaMode::Opaque).or_else(|| caps.alpha_modes.first().copied())?;
    configure(&surface, rs, format, alpha, size);
    let stream = canvas.capture_stream().ok()?;
    video.set_src_object(Some(&stream));
    let _ = video.play();
    Some(PopOut { popup, canvas, surface, format, alpha, size, _full_screen: full_screen })
}
