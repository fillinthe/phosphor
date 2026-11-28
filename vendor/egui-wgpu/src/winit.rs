#![expect(clippy::missing_errors_doc)]
#![expect(clippy::undocumented_unsafe_blocks)]
#![expect(clippy::unwrap_used)] // TODO(emilk): avoid unwraps
#![expect(unsafe_code)]

use crate::{RenderState, SurfaceConfig, SurfaceErrorAction, WgpuConfiguration, renderer};
use crate::{
    RendererOptions,
    capture::{CaptureReceiver, CaptureSender, CaptureState, capture_channel},
};
use core::num::NonZeroU32;
use egui::{Context, Event, UserData, ViewportId, ViewportIdMap, ViewportIdSet};
use std::sync::Arc;

/// PHOSPHOR PATCH: the present mode one window's surface uses, asked for each window when its surface is configured
/// and before each paint; `None` = [`crate::SurfaceConfig::present_mode`]. A changed answer reconfigures that surface. A
/// mode the surface can't do falls back to `SurfaceConfig::present_mode`. (Phosphor: outputs on several screens
/// mustn't each wait for their own screen's refresh, one after another, on the one thread that paints them.)
pub static PRESENT_MODE_FOR: std::sync::OnceLock<fn(egui::ViewportId) -> Option<wgpu::PresentMode>> = std::sync::OnceLock::new();

/// PHOSPHOR PATCH: called right after egui presents a window's frame (Phosphor's bench times the output windows).
pub static PRESENTED: std::sync::OnceLock<fn(egui::ViewportId)> = std::sync::OnceLock::new();

/// PHOSPHOR PATCH: a window's surface, shared so the app can present that window itself from another thread
/// (Phosphor's render thread draws the output windows there, so a slow interface frame never holds a projector up).
/// While [`SharedSurface::set_app_presents`] is on, egui still runs the window's interface and uploads its textures,
/// but never configures the surface, takes a texture from it or presents it. Both sides hold the surface's lock from
/// configuring or taking a texture until presenting, so the surface is never used by both at once.
pub struct SharedSurface {
    surface: wgpu::Surface<'static>,
    lock: std::sync::Mutex<()>,
    app_presents: std::sync::atomic::AtomicBool,
    /// The window's size in physical pixels as egui last heard it (width in the high half).
    size: std::sync::atomic::AtomicU64,
    /// Who configured the surface last (`BY_EGUI` / `BY_APP`): the other side configures again before using it.
    configured_by: std::sync::atomic::AtomicU8,
}

/// [`SharedSurface`] configured by egui.
const BY_EGUI: u8 = 1;
/// [`SharedSurface`] configured by the app.
const BY_APP: u8 = 2;

impl SharedSurface {
    fn new(surface: wgpu::Surface<'static>, width: u32, height: u32) -> Self {
        let shared = Self {
            surface,
            lock: std::sync::Mutex::new(()),
            app_presents: std::sync::atomic::AtomicBool::new(false),
            size: std::sync::atomic::AtomicU64::new(0),
            configured_by: std::sync::atomic::AtomicU8::new(0),
        };
        shared.set_size(width, height);
        shared
    }

    fn hold(&self) -> std::sync::MutexGuard<'_, ()> {
        self.lock.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The lock if nobody holds it. egui runs on the main thread and never waits for the app's thread here: the app
    /// holds the lock while it waits for a picture from the screen, and the main thread waiting behind that would be
    /// the interface frozen for as long as the screen takes.
    fn try_hold(&self) -> Option<std::sync::MutexGuard<'_, ()>> {
        match self.lock.try_lock() {
            Ok(guard) => Some(guard),
            Err(std::sync::TryLockError::Poisoned(p)) => Some(p.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    fn set_size(&self, width: u32, height: u32) {
        self.size.store((u64::from(width) << 32) | u64::from(height), std::sync::atomic::Ordering::Release);
    }

    /// The window's size in physical pixels, as egui last heard it.
    pub fn size(&self) -> (u32, u32) {
        let v = self.size.load(std::sync::atomic::Ordering::Acquire);
        ((v >> 32) as u32, v as u32)
    }

    /// The app presents this window from now on (`true`), or egui paints it again (`false`).
    pub fn set_app_presents(&self, on: bool) {
        self.app_presents.store(on, std::sync::atomic::Ordering::Release);
    }

    pub fn app_presents(&self) -> bool {
        self.app_presents.load(std::sync::atomic::Ordering::Acquire)
    }

    /// For the app to present this window: the lock (hold it until the frame is presented), the surface, and whether
    /// it must be configured first (egui configured it since the app last did).
    pub fn take(&self) -> (std::sync::MutexGuard<'_, ()>, &wgpu::Surface<'static>, bool) {
        let guard = self.hold();
        let configure = self.configured_by.load(std::sync::atomic::Ordering::Acquire) != BY_APP;
        (guard, &self.surface, configure)
    }

    /// The app configured the surface (call while holding the lock from [`SharedSurface::take`]).
    pub fn app_configured(&self) {
        self.configured_by.store(BY_APP, std::sync::atomic::Ordering::Release);
        // A picture presented "with the transaction" (egui's live-resize mode, `on_window_resize_state_change`) is
        // only shown once that thread's Core Animation transaction is committed, which the app's thread (no run loop)
        // never does: its pictures would pile up unseen until the screen has none left to give. The app's frames go
        // out on their own. (egui doesn't turn the mode on again while the app presents the window.)
        #[cfg(all(target_os = "macos", feature = "macos-window-resize-jitter-fix"))]
        // SAFETY: `as_hal::<Metal>()` is None unless this surface is a Metal one.
        if let Some(hal) = unsafe { self.surface.as_hal::<wgpu::hal::api::Metal>() } {
            hal.render_layer().lock().setPresentsWithTransaction(false);
        }
    }
}

/// Every window's [`SharedSurface`] by viewport (dropped ones are left out on the next registration).
static SHARED_SURFACES: std::sync::Mutex<Vec<(ViewportId, std::sync::Weak<SharedSurface>)>> = std::sync::Mutex::new(Vec::new());

fn register_shared(viewport_id: ViewportId, shared: &Arc<SharedSurface>) {
    let mut all = SHARED_SURFACES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    all.retain(|(id, s)| *id != viewport_id && s.strong_count() > 0);
    all.push((viewport_id, Arc::downgrade(shared)));
}

/// PHOSPHOR PATCH: the surface of one window, while that window is open (see [`SharedSurface`]). The app must never
/// let the last reference go on another thread: the window would then close on that thread (winit wants the main one).
pub fn shared_surface(viewport_id: ViewportId) -> Option<Arc<SharedSurface>> {
    let all = SHARED_SURFACES.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    all.iter().find(|(id, _)| *id == viewport_id).and_then(|(_, s)| s.upgrade())
}

/// PHOSPHOR PATCH: configures a surface inside an error scope (error scopes are per thread) and gives the error, if
/// any. wgpu waits for the whole device to go idle before configuring, and refuses ("Failed to wait for GPU to come
/// idle") when another thread submitted meanwhile: with the app presenting from its own thread that's a race, not a
/// refusal, so it's tried again a few times.
pub fn configure_retrying(device: &wgpu::Device, surface: &wgpu::Surface<'static>, config: &wgpu::SurfaceConfiguration) -> Option<wgpu::Error> {
    let mut tries = 0;
    loop {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        surface.configure(device, config);
        let error = {
            let mut fut = std::pin::pin!(scope.pop());
            match std::future::Future::poll(fut.as_mut(), &mut std::task::Context::from_waker(std::task::Waker::noop())) {
                std::task::Poll::Ready(e) => e,
                std::task::Poll::Pending => None,
            }
        };
        tries += 1;
        let raced = matches!(&error, Some(wgpu::Error::Validation { description, .. }) if description.contains("come idle"));
        if !raced || tries >= 5 {
            if error.is_none() {
                bound_picture_wait(surface);
            }
            return error;
        }
    }
}

/// PHOSPHOR PATCH (macOS): asking a window for a picture gives up after a second instead of waiting for ever.
/// wgpu-hal turns Metal's limit off each time it configures a surface (`setAllowsNextDrawableTimeout(false)` in
/// `metal/surface.rs`), so `get_current_texture` waited for as long as macOS kept the window's pictures: reports
/// 2026-11-27 on 0.5.26 (M1 Pro / M2 / M3 / Intel, macOS 15.7 to 27.2) ended from outside with the main thread inside
/// `Surface::acquire_texture`, one session with a 257 s frame whose own drawing took 2.4 ms. With the limit back the
/// wait ends after a second with `Timeout`, and the window is left out a while (`NoPicture`). Called after every
/// configure that worked (egui's and the app's both go through [`configure_retrying`]).
pub fn bound_picture_wait(surface: &wgpu::Surface<'static>) {
    #[cfg(all(target_os = "macos", feature = "macos-window-resize-jitter-fix"))]
    // SAFETY: `as_hal::<Metal>()` is None unless this surface is a Metal one.
    if let Some(hal) = unsafe { surface.as_hal::<wgpu::hal::api::Metal>() } {
        hal.render_layer().lock().setAllowsNextDrawableTimeout(true);
    }
    #[cfg(not(all(target_os = "macos", feature = "macos-window-resize-jitter-fix")))]
    let _ = surface;
}

/// PHOSPHOR PATCH: a wait for a window's picture this long means the screen didn't give one (Metal gives up after
/// 1 s, see [`bound_picture_wait`]).
const NO_PICTURE: std::time::Duration = std::time::Duration::from_millis(900);

/// PHOSPHOR PATCH: a wait for a window's picture this long is written in the log (at most once every 10 s), so a
/// report shows when the screen, not Phosphor, is what's slow.
const SLOW_PICTURE: std::time::Duration = std::time::Duration::from_millis(150);

/// PHOSPHOR PATCH: one window's pictures left out after the screen didn't give one: its interface still runs and its
/// textures still upload, only no picture is asked for until `until`; 2 s after the first miss, doubling to 30 s while
/// the misses go on, and cleared by the first picture that comes. (0.5.25 had this in eframe for direct paints only,
/// timed from the whole paint: a wait that never ended never got there.)
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct NoPicture {
    until: Option<web_time::Instant>,
    wait: std::time::Duration,
}

impl NoPicture {
    const FIRST: std::time::Duration = std::time::Duration::from_secs(2);
    const LONGEST: std::time::Duration = std::time::Duration::from_secs(30);

    /// How long until a picture may be asked for again; None = ask now.
    fn left(&self, now: web_time::Instant) -> Option<std::time::Duration> {
        self.until.and_then(|u| u.checked_duration_since(now)).filter(|d| !d.is_zero())
    }

    /// A picture was asked for at `now` and the wait took `waited`; `came` = the screen gave one. True when this
    /// starts a stretch of misses (the moment to log it).
    fn asked(&mut self, now: web_time::Instant, waited: std::time::Duration, came: bool) -> bool {
        if came && waited < NO_PICTURE {
            *self = Self::default();
            return false;
        }
        if waited < NO_PICTURE {
            // Refused quickly (covered, out of date…): not the screen keeping its pictures; wgpu's answer handles it.
            return false;
        }
        let first = self.until.is_none();
        self.wait = if first { Self::FIRST } else { (self.wait * 2).min(Self::LONGEST) };
        self.until = Some(now + self.wait);
        first
    }
}

struct SurfaceState {
    surface: Arc<SharedSurface>,
    alpha_mode: wgpu::CompositeAlphaMode,
    width: u32,
    height: u32,
    resizing: bool,
    needs_reconfigure: bool,
    needs_recreate: bool,
    // PHOSPHOR PATCH: whether the last configure worked, and when it was tried. A surface the driver refused
    // (OpenGL on old Intel graphics, Windows, 2026-11-23) made `get_current_texture` end the app; now that window
    // skips its frames and configure is tried again every 2 s.
    configured: bool,
    configure_tried: Option<web_time::Instant>,
    // PHOSPHOR PATCH: whose surface this is, and the present mode last asked for it (`PRESENT_MODE_FOR`).
    viewport_id: ViewportId,
    present_mode_asked: Option<wgpu::PresentMode>,
    // PHOSPHOR PATCH: pictures left out after the screen didn't give one (`NoPicture`), and when a slow wait was
    // last logged.
    no_picture: NoPicture,
    slow_logged: Option<web_time::Instant>,
}

/// PHOSPHOR PATCH: the present mode `PRESENT_MODE_FOR` asks for this window (None = the configured one).
fn present_mode_asked(viewport_id: ViewportId) -> Option<wgpu::PresentMode> {
    PRESENT_MODE_FOR.get().and_then(|f| f(viewport_id))
}

/// Everything you need to paint egui with [`wgpu`] on [`winit`].
///
/// Alternatively you can use [`crate::Renderer`] directly.
///
/// NOTE: all egui viewports share the same painter.
pub struct Painter {
    context: Context,
    config: WgpuConfiguration,
    options: RendererOptions,
    support_transparent_backbuffer: bool,
    screen_capture_state: Option<CaptureState>,

    instance: wgpu::Instance,
    render_state: Option<RenderState>,

    // Per viewport/window:
    depth_texture_view: ViewportIdMap<wgpu::TextureView>,
    msaa_texture_view: ViewportIdMap<wgpu::TextureView>,
    surfaces: ViewportIdMap<SurfaceState>,
    capture_tx: CaptureSender,
    capture_rx: CaptureReceiver,
}

impl Painter {
    /// Manages [`wgpu`] state, including surface state, required to render egui.
    ///
    /// Only the [`wgpu::Instance`] is initialized here. Device selection and the initialization
    /// of render + surface state is deferred until the painter is given its first window target
    /// via [`set_window()`](Self::set_window). (Ensuring that a device that's compatible with the
    /// native window is chosen)
    ///
    /// Before calling [`paint_and_update_textures()`](Self::paint_and_update_textures) a
    /// [`wgpu::Surface`] must be initialized (and corresponding render state) by calling
    /// [`set_window()`](Self::set_window) once you have
    /// a [`winit::window::Window`] with a valid `.raw_window_handle()`
    /// associated.
    pub async fn new(
        context: Context,
        config: WgpuConfiguration,
        support_transparent_backbuffer: bool,
        options: RendererOptions,
    ) -> Self {
        let (capture_tx, capture_rx) = capture_channel();
        let instance = config.wgpu_setup.new_instance().await;

        Self {
            context,
            config,
            options,
            support_transparent_backbuffer,
            screen_capture_state: None,

            instance,
            render_state: None,

            depth_texture_view: Default::default(),
            surfaces: Default::default(),
            msaa_texture_view: Default::default(),

            capture_tx,
            capture_rx,
        }
    }

    /// Get the [`RenderState`].
    ///
    /// Will return [`None`] if the render state has not been initialized yet.
    pub fn render_state(&self) -> Option<RenderState> {
        self.render_state.clone()
    }

    /// PHOSPHOR PATCH: configures a window's surface outside a paint: not while the app presents that window (egui
    /// configures it again when it paints it next), and holding the surface's lock otherwise.
    fn configure_surface(
        surface_state: &mut SurfaceState,
        render_state: &RenderState,
        config: &SurfaceConfig,
    ) {
        let shared = Arc::clone(&surface_state.surface);
        shared.set_size(surface_state.width, surface_state.height);
        // Not while the app presents it, nor while the app's thread holds it (it may be waiting for a picture):
        // configured at the next paint instead.
        let held = if shared.app_presents() { None } else { shared.try_hold() };
        let Some(_held) = held else {
            surface_state.needs_reconfigure = true;
            return;
        };
        Self::configure_held(surface_state, render_state, config);
    }

    /// Configures a window's surface; the caller holds its lock.
    fn configure_held(
        surface_state: &mut SurfaceState,
        render_state: &RenderState,
        config: &SurfaceConfig,
    ) {
        profiling::function_scope!();

        let SurfaceConfig {
            present_mode,
            desired_maximum_frame_latency,
        } = *config;

        // Transaction presentation can hold a drawable during AppKit live resize. Keep the
        // configured low-latency path normally, but use three Metal drawables while resizing.
        #[cfg(all(target_os = "macos", feature = "macos-window-resize-jitter-fix"))]
        let desired_maximum_frame_latency = if surface_state.resizing {
            Some(desired_maximum_frame_latency.unwrap_or(2).max(2))
        } else {
            desired_maximum_frame_latency
        };

        let width = surface_state.width;
        let height = surface_state.height;

        // PHOSPHOR PATCH: this window's own present mode, when one is asked for and its surface can do it.
        let asked = present_mode_asked(surface_state.viewport_id);
        surface_state.present_mode_asked = asked;
        let present_mode = match asked {
            Some(m @ (wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync)) => m,
            Some(m) if surface_state.surface.surface.get_capabilities(&render_state.adapter).present_modes.contains(&m) => m,
            _ => present_mode,
        };

        let mut surf_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: render_state.target_format,
            present_mode,
            alpha_mode: surface_state.alpha_mode,
            view_formats: vec![render_state.target_format],
            ..surface_state
                .surface
                .surface
                .get_default_config(&render_state.adapter, width, height)
                .expect("The surface isn't supported by this adapter")
        };

        if let Some(desired_maximum_frame_latency) = desired_maximum_frame_latency {
            surf_config.desired_maximum_frame_latency = desired_maximum_frame_latency;
        }

        // PHOSPHOR PATCH: configure inside an error scope, so a refusal is known here (and logged whole) instead
        // of the next `get_current_texture` panicking on an unconfigured surface.
        let error = configure_retrying(&render_state.device, &surface_state.surface.surface, &surf_config);
        surface_state.surface.configured_by.store(BY_EGUI, std::sync::atomic::Ordering::Release);
        surface_state.configure_tried = Some(web_time::Instant::now());
        surface_state.configured = match error {
            None => true,
            Some(e) => {
                let whole = e.to_string().split_whitespace().collect::<Vec<_>>().join(" ");
                log::error!("a window's drawing surface couldn't be set up ({width}x{height}, {:?}): {whole}", render_state.target_format);
                false
            }
        };
    }

    /// Drop the existing [`wgpu::Surface`] for `viewport_id` and create a fresh one for the
    /// given window via [`wgpu::Instance::create_surface`], then configure it.
    ///
    /// Used to recover from [`wgpu::CurrentSurfaceTexture::Lost`], where reconfiguring the
    /// existing surface object cannot recover.
    fn recreate_surface(
        &mut self,
        viewport_id: ViewportId,
        window: &Arc<winit::window::Window>,
    ) -> Result<(), crate::WgpuError> {
        profiling::function_scope!();

        let Some(old_state) = self.surfaces.remove(&viewport_id) else {
            return Ok(());
        };

        let surface = self.instance.create_surface(Arc::clone(window))?;
        self.install_surface(
            surface,
            viewport_id,
            old_state.width,
            old_state.height,
            old_state.resizing,
        );
        Ok(())
    }

    /// Updates (or clears) the [`winit::window::Window`] associated with the [`Painter`]
    ///
    /// This creates a [`wgpu::Surface`] for the given Window (as well as initializing render
    /// state if needed) that is used for egui rendering.
    ///
    /// This must be called before trying to render via
    /// [`paint_and_update_textures`](Self::paint_and_update_textures)
    ///
    /// # Portability
    ///
    /// _In particular it's important to note that on Android a it's only possible to create
    /// a window surface between `Resumed` and `Paused` lifecycle events, and Winit will panic on
    /// attempts to query the raw window handle while paused._
    ///
    /// On Android [`set_window`](Self::set_window) should be called with `Some(window)` for each
    /// `Resumed` event and `None` for each `Paused` event. Currently, on all other platforms
    /// [`set_window`](Self::set_window) may be called with `Some(window)` as soon as you have a
    /// valid [`winit::window::Window`].
    ///
    /// # Errors
    /// If the provided wgpu configuration does not match an available device.
    pub async fn set_window(
        &mut self,
        viewport_id: ViewportId,
        window: Option<Arc<winit::window::Window>>,
    ) -> Result<(), crate::WgpuError> {
        profiling::scope!("Painter::set_window"); // profile_function gives bad names for async functions

        if let Some(window) = window {
            let size = window.inner_size();
            if !self.surfaces.contains_key(&viewport_id) {
                let surface = self.instance.create_surface(window)?;
                self.add_surface(surface, viewport_id, size).await?;
            }
        } else {
            log::warn!("No window - clearing all surfaces");
            self.surfaces.clear();
        }
        Ok(())
    }

    /// Updates (or clears) the [`winit::window::Window`] associated with the [`Painter`] without taking ownership of the window.
    ///
    /// Like [`set_window`](Self::set_window) except:
    ///
    /// # Safety
    /// The user is responsible for ensuring that the window is alive for as long as it is set.
    pub async unsafe fn set_window_unsafe(
        &mut self,
        viewport_id: ViewportId,
        window: Option<&winit::window::Window>,
    ) -> Result<(), crate::WgpuError> {
        profiling::scope!("Painter::set_window_unsafe"); // profile_function gives bad names for async functions

        if let Some(window) = window {
            let size = window.inner_size();
            if !self.surfaces.contains_key(&viewport_id) {
                let surface = unsafe {
                    self.instance
                        .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::from_window(&window)?)?
                };
                self.add_surface(surface, viewport_id, size).await?;
            }
        } else {
            log::warn!("No window - clearing all surfaces");
            self.surfaces.clear();
        }
        Ok(())
    }

    async fn add_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        viewport_id: ViewportId,
        size: winit::dpi::PhysicalSize<u32>,
    ) -> Result<(), crate::WgpuError> {
        if self.render_state.is_none() {
            let render_state =
                RenderState::create(&self.config, &self.instance, Some(&surface), self.options)
                    .await?;
            self.render_state = Some(render_state);
        }
        self.install_surface(surface, viewport_id, size.width, size.height, false);
        Ok(())
    }

    /// Inserts a freshly created surface into [`Self::surfaces`] and configures it.
    ///
    /// Render state must already be initialised before calling this.
    // NOTE: The same assumption is already required by `resize_and_generate_depth_texture_view_and_msaa_view`.
    fn install_surface(
        &mut self,
        surface: wgpu::Surface<'static>,
        viewport_id: ViewportId,
        width: u32,
        height: u32,
        resizing: bool,
    ) {
        let alpha_mode = {
            // Panic: We use the same failure mode as `resize_and_generate_depth_texture_view_and_msaa_view`
            let render_state = self
                .render_state
                .as_ref()
                .expect("install_surface called before render_state initialization");
            if self.support_transparent_backbuffer {
                let supported_alpha_modes =
                    surface.get_capabilities(&render_state.adapter).alpha_modes;
                // Prefer pre multiplied over post multiplied!
                if supported_alpha_modes.contains(&wgpu::CompositeAlphaMode::PreMultiplied) {
                    wgpu::CompositeAlphaMode::PreMultiplied
                } else if supported_alpha_modes.contains(&wgpu::CompositeAlphaMode::PostMultiplied)
                {
                    wgpu::CompositeAlphaMode::PostMultiplied
                } else {
                    log::warn!(
                        "Transparent window was requested, but the active wgpu surface does not support a `CompositeAlphaMode` with transparency."
                    );
                    wgpu::CompositeAlphaMode::Auto
                }
            } else {
                wgpu::CompositeAlphaMode::Auto
            }
        };
        // PHOSPHOR PATCH: shared with the app (`shared_surface`).
        let surface = Arc::new(SharedSurface::new(surface, width, height));
        register_shared(viewport_id, &surface);
        self.surfaces.insert(
            viewport_id,
            SurfaceState {
                surface,
                width,
                height,
                alpha_mode,
                resizing,
                needs_reconfigure: false,
                needs_recreate: false,
                configured: false,
                configure_tried: None,
                viewport_id,
                present_mode_asked: None,
                no_picture: NoPicture::default(),
                slow_logged: None,
            },
        );
        let Some(width) = NonZeroU32::new(width) else {
            log::debug!("The window width was zero; skipping generate textures");
            return;
        };
        let Some(height) = NonZeroU32::new(height) else {
            log::debug!("The window height was zero; skipping generate textures");
            return;
        };
        self.resize_and_generate_depth_texture_view_and_msaa_view(viewport_id, width, height);
    }

    /// Returns the maximum texture dimension supported if known
    ///
    /// This API will only return a known dimension after `set_window()` has been called
    /// at least once, since the underlying device and render state are initialized lazily
    /// once we have a window (that may determine the choice of adapter/device).
    pub fn max_texture_side(&self) -> Option<usize> {
        self.render_state
            .as_ref()
            .map(|rs| rs.device.limits().max_texture_dimension_2d as usize)
    }

    fn resize_and_generate_depth_texture_view_and_msaa_view(
        &mut self,
        viewport_id: ViewportId,
        width_in_pixels: NonZeroU32,
        height_in_pixels: NonZeroU32,
    ) {
        profiling::function_scope!();

        let width = width_in_pixels.get();
        let height = height_in_pixels.get();

        let render_state = self.render_state.as_ref().unwrap();
        let surface_state = self.surfaces.get_mut(&viewport_id).unwrap();

        surface_state.width = width;
        surface_state.height = height;

        Self::configure_surface(surface_state, render_state, &self.config.surface);

        if let Some(depth_format) = self.options.depth_stencil_format {
            self.depth_texture_view.insert(
                viewport_id,
                render_state
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("egui_depth_texture"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: self.options.msaa_samples.max(1),
                        dimension: wgpu::TextureDimension::D2,
                        format: depth_format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                            | wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[depth_format],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            );
        }

        if let Some(render_state) = (self.options.msaa_samples > 1)
            .then_some(self.render_state.as_ref())
            .flatten()
        {
            let texture_format = render_state.target_format;
            self.msaa_texture_view.insert(
                viewport_id,
                render_state
                    .device
                    .create_texture(&wgpu::TextureDescriptor {
                        label: Some("egui_msaa_texture"),
                        size: wgpu::Extent3d {
                            width,
                            height,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: self.options.msaa_samples.max(1),
                        dimension: wgpu::TextureDimension::D2,
                        format: texture_format,
                        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                        view_formats: &[texture_format],
                    })
                    .create_view(&wgpu::TextureViewDescriptor::default()),
            );
        }
    }

    /// Handles changes of the resizing state.
    ///
    /// Should be called prior to the first [`Painter::on_window_resized`] call and after the last in
    /// the chain. Used to apply platform-specific logic, e.g. OSX Metal window resize jitter fix.
    pub fn on_window_resize_state_change(&mut self, viewport_id: ViewportId, resizing: bool) {
        profiling::function_scope!();

        let Some(state) = self.surfaces.get_mut(&viewport_id) else {
            return;
        };
        if state.resizing == resizing {
            if resizing {
                log::debug!(
                    "Painter::on_window_resize_state_change() redundant call while resizing"
                );
            } else {
                log::debug!(
                    "Painter::on_window_resize_state_change() redundant call after resizing"
                );
            }
            return;
        }

        // Set before reconfiguring so macOS live resize uses the temporary latency bump above.
        state.resizing = resizing;

        // Resizing is a bit tricky on macOS.
        // It requires enabling ["present_with_transaction"](https://developer.apple.com/documentation/quartzcore/cametallayer/presentswithtransaction)
        // flag to avoid jittering during the resize. Even though resize jittering on macOS
        // is common across rendering backends, the solution for wgpu/metal is known.
        //
        // See https://github.com/emilk/egui/issues/903
        #[cfg(all(target_os = "macos", feature = "macos-window-resize-jitter-fix"))]
        {
            // SAFETY: `as_hal::<Metal>()` returns `None` unless this surface is backed by wgpu's
            // Metal backend.
            // PHOSPHOR PATCH: the shared surface (the app may be presenting this window).
            let shared = Arc::clone(&state.surface);
            unsafe {
                if let (Some(render_state), Some(hal_surface)) = (
                    self.render_state.as_ref(),
                    shared.surface.as_hal::<wgpu::hal::api::Metal>(),
                ) {
                    // PHOSPHOR PATCH: never for a window the app presents from its own thread (see
                    // `SharedSurface::app_configured`).
                    hal_surface
                        .render_layer()
                        .lock()
                        .setPresentsWithTransaction(resizing && !shared.app_presents());

                    Self::configure_surface(state, render_state, &self.config.surface);
                }
            }
        }
    }

    pub fn on_window_resized(
        &mut self,
        viewport_id: ViewportId,
        width_in_pixels: NonZeroU32,
        height_in_pixels: NonZeroU32,
    ) {
        profiling::function_scope!();

        if self.surfaces.contains_key(&viewport_id) {
            self.resize_and_generate_depth_texture_view_and_msaa_view(
                viewport_id,
                width_in_pixels,
                height_in_pixels,
            );
        } else {
            log::warn!(
                "Ignoring window resize notification with no surface created via Painter::set_window()"
            );
        }
    }

    /// Returns two things:
    ///
    /// The approximate number of seconds spent on vsync-waiting (if any),
    /// and the captures captured screenshot if it was requested.
    ///
    /// If `capture_data` isn't empty, a screenshot will be captured.
    #[expect(clippy::too_many_arguments)]
    pub fn paint_and_update_textures(
        &mut self,
        viewport_id: ViewportId,
        pixels_per_point: f32,
        clear_color: [f32; 4],
        clipped_primitives: &[epaint::ClippedPrimitive],
        textures_delta: &mut epaint::textures::TexturesDelta,
        capture_data: Vec<UserData>,
        window: &Arc<winit::window::Window>,
    ) -> f32 {
        profiling::function_scope!();

        /// Guard to ensure that commands are always submitted to the renderer queue
        /// so that calls to [`write_buffer()`](https://docs.rs/wgpu/latest/wgpu/struct.Queue.html#method.write_buffer)
        /// are completed even if we take a codepath which doesn't submit commands and avoids
        /// internal buffers growing indefinitely.
        ///
        /// This may happen, for example, if no output frame is resolved.
        /// See <https://github.com/emilk/egui/pull/7928> for full context.
        struct RendererQueueGuard<'q> {
            queue: &'q wgpu::Queue,
            commands_submitted: bool,
        }

        impl Drop for RendererQueueGuard<'_> {
            fn drop(&mut self) {
                // Only submit an empty command buffer array if no commands were
                // explicitly submitted.
                if !self.commands_submitted {
                    self.queue.submit([]);
                }
            }
        }

        let capture = !capture_data.is_empty();
        let mut vsync_sec = 0.0;

        // If the previous frame produced `CurrentSurfaceTexture::Lost`, the action match
        // below set `needs_recreate`. Recreate the surface now, before re-borrowing
        // `self.render_state` / `self.surfaces` for the rest of the paint.
        if self
            .surfaces
            .get(&viewport_id)
            .is_some_and(|s| s.needs_recreate)
            && let Err(err) = self.recreate_surface(viewport_id, window)
        {
            log::error!("Failed to recreate surface for {viewport_id:?}: {err}");
            return vsync_sec;
        }

        // Apply any runtime changes requested via `RenderState::surface_config`.
        // We diff against the already-applied values in `self.config.surface`
        // and, if anything differs, mark every surface as needing reconfiguration so
        // the existing `needs_reconfigure` pathway below picks them up.
        if let Some(render_state) = self.render_state.as_ref()
            && render_state.surface_config != self.config.surface
        {
            self.config.surface = render_state.surface_config;
            #[expect(clippy::iter_over_hash_type)]
            for surface in self.surfaces.values_mut() {
                surface.needs_reconfigure = true;
            }
        }

        let Some(render_state) = self.render_state.as_mut() else {
            return vsync_sec;
        };

        let mut render_queue_guard = RendererQueueGuard {
            queue: &render_state.queue,
            commands_submitted: false,
        };

        let Some(surface_state) = self.surfaces.get_mut(&viewport_id) else {
            return vsync_sec;
        };

        let mut encoder =
            render_state
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("encoder"),
                });

        // Upload all resources for the GPU.
        let screen_descriptor = renderer::ScreenDescriptor {
            size_in_pixels: [surface_state.width, surface_state.height],
            pixels_per_point,
        };

        let user_cmd_bufs = {
            let mut renderer = render_state.renderer.write();
            #[expect(clippy::iter_over_hash_type)] // Order doesn't matter here
            for (id, image_deltas) in textures_delta.set.drain() {
                for image_delta in image_deltas {
                    renderer.update_texture(
                        &render_state.device,
                        &render_state.queue,
                        id,
                        &image_delta,
                    );
                }
            }

            renderer.update_buffers(
                &render_state.device,
                &render_state.queue,
                &mut encoder,
                clipped_primitives,
                &screen_descriptor,
            )
        };

        // PHOSPHOR PATCH: a window the app presents itself (`SharedSurface`).
        let shared = Arc::clone(&surface_state.surface);
        if shared.app_presents() {
            Self::without_picture(render_state, encoder, user_cmd_bufs, textures_delta);
            render_queue_guard.commands_submitted = true;
            return vsync_sec;
        }
        // PHOSPHOR PATCH: the screen didn't give this window a picture lately (`NoPicture`): none asked for until then.
        if let Some(left) = surface_state.no_picture.left(web_time::Instant::now()) {
            Self::without_picture(render_state, encoder, user_cmd_bufs, textures_delta);
            render_queue_guard.commands_submitted = true;
            self.context.request_repaint_after_for(left, viewport_id);
            return vsync_sec;
        }
        // Held until the frame is presented; the app configured the surface last: configured again here first.
        // PHOSPHOR PATCH: never waited for: the app's thread holds it while it waits for a picture of its own (up to a
        // second), and the main thread waiting behind it would freeze the interface. Tried again in a moment.
        let Some(_held) = shared.try_hold() else {
            Self::without_picture(render_state, encoder, user_cmd_bufs, textures_delta);
            render_queue_guard.commands_submitted = true;
            self.context.request_repaint_after_for(std::time::Duration::from_millis(5), viewport_id);
            return vsync_sec;
        };
        if shared.configured_by.load(std::sync::atomic::Ordering::Acquire) == BY_APP {
            surface_state.needs_reconfigure = true;
        }

        // PHOSPHOR PATCH: a window asked for another present mode since its surface was configured.
        if surface_state.configured && present_mode_asked(viewport_id) != surface_state.present_mode_asked {
            surface_state.needs_reconfigure = true;
        }

        if surface_state.needs_reconfigure {
            Self::configure_held(surface_state, render_state, &self.config.surface);
            surface_state.needs_reconfigure = false;
        }

        // PHOSPHOR PATCH: never ask an unconfigured surface for a frame (wgpu ends the app); try configuring again
        // every 2 s (and when the window changes size) and skip this window's frame meanwhile.
        if !surface_state.configured {
            let due = surface_state.configure_tried.is_none_or(|t| t.elapsed().as_secs_f32() >= 2.0);
            if due && surface_state.width > 0 && surface_state.height > 0 {
                Self::configure_held(surface_state, render_state, &self.config.surface);
            }
            if !surface_state.configured {
                self.context.request_repaint_after_for(std::time::Duration::from_secs(2), viewport_id);
                return vsync_sec;
            }
        }

        let output_frame = {
            profiling::scope!("get_current_texture");
            // This is what vsync-waiting happens on my Mac.
            let start = web_time::Instant::now();
            let output_frame = shared.surface.get_current_texture();
            let waited = start.elapsed();
            vsync_sec += waited.as_secs_f32();
            // PHOSPHOR PATCH: a wait the screen made long is logged, and one that got no picture leaves this window's
            // pictures out a while (`NoPicture`).
            let came = matches!(output_frame, wgpu::CurrentSurfaceTexture::Success(_) | wgpu::CurrentSurfaceTexture::Suboptimal(_));
            let now = web_time::Instant::now();
            if surface_state.no_picture.asked(now, waited, came) {
                log::warn!(
                    "a window's picture didn't come ({} ms): its drawing is left out while that lasts",
                    waited.as_millis()
                );
            } else if waited >= SLOW_PICTURE && surface_state.slow_logged.is_none_or(|t| now.duration_since(t).as_secs() >= 10) {
                surface_state.slow_logged = Some(now);
                log::warn!("a window waited {} ms for its picture from the screen", waited.as_millis());
            }
            output_frame
        };

        let output_frame = match output_frame {
            wgpu::CurrentSurfaceTexture::Success(frame) => frame,
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                surface_state.needs_reconfigure = true;
                frame
            }
            other => {
                match (*self.config.on_surface_status)(&other) {
                    SurfaceErrorAction::Reconfigure => {
                        Self::configure_held(surface_state, render_state, &self.config.surface);
                        self.context.request_repaint_of(viewport_id);
                    }
                    SurfaceErrorAction::RecreateSurface => {
                        // Because of ownership, I could not find an easy way to do a full recovery here,
                        // as that would involve dropping the old surface and creating a new one.
                        // For now, we defer the recreation to the beginning of the next frame (which
                        // we ensure to arrive via `request_repaint_of`). A cleaner solution would be
                        // to untangle the ownership of `RenderState`.
                        surface_state.needs_recreate = true;
                        self.context.request_repaint_of(viewport_id);
                    }
                    SurfaceErrorAction::SkipFrame => {}
                }
                return vsync_sec;
            }
        };

        let mut capture_buffer = None;
        {
            let renderer = render_state.renderer.read();

            let target_texture = if capture {
                let capture_state = self.screen_capture_state.get_or_insert_with(|| {
                    CaptureState::new(&render_state.device, &output_frame.texture)
                });
                capture_state.update(&render_state.device, &output_frame.texture);

                &capture_state.texture
            } else {
                &output_frame.texture
            };
            let target_view = target_texture.create_view(&wgpu::TextureViewDescriptor::default());

            let (view, resolve_target) = (self.options.msaa_samples > 1)
                .then_some(self.msaa_texture_view.get(&viewport_id))
                .flatten()
                .map_or((&target_view, None), |texture_view| {
                    (texture_view, Some(&target_view))
                });

            let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("egui_render"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: clear_color[0] as f64,
                            g: clear_color[1] as f64,
                            b: clear_color[2] as f64,
                            a: clear_color[3] as f64,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: self.depth_texture_view.get(&viewport_id).map(|view| {
                    wgpu::RenderPassDepthStencilAttachment {
                        view,
                        depth_ops: self
                            .options
                            .depth_stencil_format
                            .is_some_and(|depth_stencil_format| {
                                depth_stencil_format.has_depth_aspect()
                            })
                            .then_some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                // It is very unlikely that the depth buffer is needed after egui finished rendering
                                // so no need to store it. (this can improve performance on tiling GPUs like mobile chips or Apple Silicon)
                                store: wgpu::StoreOp::Discard,
                            }),
                        stencil_ops: self
                            .options
                            .depth_stencil_format
                            .is_some_and(|depth_stencil_format| {
                                depth_stencil_format.has_stencil_aspect()
                            })
                            .then_some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(0),
                                store: wgpu::StoreOp::Discard,
                            }),
                    }
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            // Forgetting the pass' lifetime means that we are no longer compile-time protected from
            // runtime errors caused by accessing the parent encoder before the render pass is dropped.
            // Since we don't pass it on to the renderer, we should be perfectly safe against this mistake here!
            renderer.render(
                &mut render_pass.forget_lifetime(),
                clipped_primitives,
                &screen_descriptor,
            );

            if capture && let Some(capture_state) = &mut self.screen_capture_state {
                capture_buffer = Some(capture_state.copy_textures(
                    &render_state.device,
                    &output_frame,
                    &mut encoder,
                ));
            }
        }

        let encoded = {
            profiling::scope!("CommandEncoder::finish");
            encoder.finish()
        };

        // Submit the commands: both the main buffer and user-defined ones.
        {
            profiling::scope!("Queue::submit");
            // wgpu doesn't document where vsync can happen. Maybe here?
            let start = web_time::Instant::now();
            render_state
                .queue
                .submit(core::iter::chain(user_cmd_bufs, [encoded]));
            vsync_sec += start.elapsed().as_secs_f32();
        };

        // Ensure that the queue guard does not do unnecessary work when dropped
        render_queue_guard.commands_submitted = true;

        // Free textures marked for destruction **after** queue submit since they might still be used in the current frame.
        // Calling `wgpu::Texture::destroy` on a texture that is still in use would invalidate the command buffer(s) it is used in.
        // However, once we called `wgpu::Queue::submit`, it is up for wgpu to determine how long the underlying gpu resource has to live.
        {
            let mut renderer = render_state.renderer.write();
            #[expect(clippy::iter_over_hash_type)] // Order doesn't matter here
            for id in textures_delta.free.drain() {
                renderer.free_texture(&id);
            }
        }

        if let Some(capture_buffer) = capture_buffer
            && let Some(screen_capture_state) = &mut self.screen_capture_state
        {
            screen_capture_state.read_screen_rgba(
                self.context.clone(),
                capture_buffer,
                capture_data,
                self.capture_tx.clone(),
                viewport_id,
            );
        }

        window.pre_present_notify();

        {
            profiling::scope!("present");
            // wgpu doesn't document where vsync can happen. Maybe here?
            let start = web_time::Instant::now();
            render_state.queue.present(output_frame);
            vsync_sec += start.elapsed().as_secs_f32();
        }
        if let Some(presented) = PRESENTED.get() {
            presented(viewport_id);
        }

        vsync_sec
    }

    /// PHOSPHOR PATCH: a paint with no picture for its window this time: the uploads still go (every window shares
    /// the textures), and nothing is drawn to its surface.
    fn without_picture(
        render_state: &RenderState,
        encoder: wgpu::CommandEncoder,
        user_cmd_bufs: Vec<wgpu::CommandBuffer>,
        textures_delta: &mut epaint::textures::TexturesDelta,
    ) {
        render_state.queue.submit(core::iter::chain(user_cmd_bufs, [encoder.finish()]));
        let mut renderer = render_state.renderer.write();
        #[expect(clippy::iter_over_hash_type)] // Order doesn't matter here
        for id in textures_delta.free.drain() {
            renderer.free_texture(&id);
        }
    }

    /// Call this at the beginning of each frame to receive the requested screenshots.
    pub fn handle_screenshots(&self, events: &mut Vec<Event>) {
        for (viewport_id, user_data, screenshot) in self.capture_rx.try_iter() {
            let screenshot = Arc::new(screenshot);
            for data in user_data {
                events.push(Event::Screenshot {
                    viewport_id,
                    user_data: data,
                    image: Arc::clone(&screenshot),
                });
            }
        }
    }

    pub fn gc_viewports(&mut self, active_viewports: &ViewportIdSet) {
        self.surfaces.retain(|id, _| active_viewports.contains(id));
        self.depth_texture_view
            .retain(|id, _| active_viewports.contains(id));
        self.msaa_texture_view
            .retain(|id, _| active_viewports.contains(id));
    }

    #[expect(clippy::needless_pass_by_ref_mut, clippy::unused_self)]
    pub fn destroy(&mut self) {
        // TODO(emilk): something here?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_picture_that_never_came_leaves_the_window_out_longer_each_time_until_one_comes() {
        let t0 = web_time::Instant::now();
        let mut n = NoPicture::default();
        assert_eq!(n.left(t0), None, "a new window asks for its picture");
        // Pictures that come, and quick refusals (covered, out of date), change nothing.
        assert!(!n.asked(t0, Duration::from_millis(16), true));
        assert!(!n.asked(t0, Duration::from_millis(1), false));
        assert_eq!(n, NoPicture::default());
        // The screen kept its pictures for the whole second: left out 2 s, logged once.
        assert!(n.asked(t0, Duration::from_millis(1000), false));
        assert_eq!(n.left(t0), Some(Duration::from_secs(2)));
        assert_eq!(n.left(t0 + Duration::from_secs(2)), None, "asked again when the time is up");
        // Still nothing: 4 s, 8 s, 16 s, then never more than 30 s; not logged again.
        let mut t = t0 + Duration::from_secs(2);
        for want in [4, 8, 16, 30, 30] {
            assert!(!n.asked(t, Duration::from_millis(1000), false));
            assert_eq!(n.left(t), Some(Duration::from_secs(want)));
            t += Duration::from_secs(want);
        }
        // A picture that came only at the limit is no better than none.
        assert!(!n.asked(t, Duration::from_millis(950), true));
        assert!(n.left(t).is_some());
        // The first one that comes in time puts the window back at once.
        assert!(!n.asked(t, Duration::from_millis(16), true));
        assert_eq!(n, NoPicture::default());
        assert_eq!(n.left(t), None);
    }
}
