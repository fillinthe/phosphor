use std::{sync::Arc, time::Instant};

use winit::{
    event_loop::ActiveEventLoop,
    window::{Window, WindowId},
};

use egui::ViewportId;
#[cfg(feature = "accesskit")]
use egui_winit::accesskit_winit;

/// PHOSPHOR PATCH (macOS): true while a window whose redraws don't come is being painted directly (see
/// `redraw_lost` in run.rs).
pub static DIRECT_PAINT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// PHOSPHOR PATCH: a direct paint that waited this long was waiting for a picture the screen never gave (Metal's
/// wait for the next drawable gives up after 1 s).
const NO_PICTURE: std::time::Duration = std::time::Duration::from_millis(900);

/// PHOSPHOR PATCH: per window, until when its direct paints are left out, and the wait that grows while it lasts.
static NO_PICTURE_UNTIL: std::sync::Mutex<Vec<(ViewportId, Instant, std::time::Duration)>> = std::sync::Mutex::new(Vec::new());

/// PHOSPHOR PATCH (macOS): should this window's picture be left out of a direct paint (its UI and the app's logic
/// still run)? A covered window gets no picture from macOS: asking for one made the main thread wait a second per
/// frame, so the whole show ran at one frame a second and looked frozen (reports 2026-11-25 on 0.5.24: runs ended
/// from outside with the main thread waiting in `get_current_texture`, the control window under a full-screen
/// output). Left out while macOS says it's covered, and for a while after a paint that waited the whole second.
pub fn skip_direct_paint(id: ViewportId, occluded: Option<bool>) -> bool {
    if !DIRECT_PAINT.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
    occluded == Some(true) || NO_PICTURE_UNTIL.lock().is_ok_and(|l| l.iter().any(|(v, until, _)| *v == id && Instant::now() < *until))
}

/// PHOSPHOR PATCH (macOS): a window was painted and it took this long. A direct paint that waited for a picture
/// that never came is left out for 2 s, then 4 s, up to 30 s; any paint that goes through clears that.
pub fn painted(id: ViewportId, took: std::time::Duration) {
    let direct = DIRECT_PAINT.load(std::sync::atomic::Ordering::Relaxed);
    let Ok(mut list) = NO_PICTURE_UNTIL.lock() else { return };
    let at = list.iter().position(|(v, _, _)| *v == id);
    if direct && took >= NO_PICTURE {
        let wait = no_picture_wait(at.map(|i| list[i].2));
        if at.is_none() {
            log::warn!("a window's picture didn't come ({} ms): its drawing is left out while that lasts", took.as_millis());
        }
        let entry = (id, Instant::now() + wait, wait);
        match at {
            Some(i) => list[i] = entry,
            None => list.push(entry),
        }
    } else if let Some(i) = at {
        list.swap_remove(i);
    }
}

/// PHOSPHOR PATCH: how long direct paints are left out after one that got no picture: 2 s, doubling to 30 s.
fn no_picture_wait(last: Option<std::time::Duration>) -> std::time::Duration {
    last.map_or(std::time::Duration::from_secs(2), |w| (w * 2).min(std::time::Duration::from_secs(30)))
}

/// Returns `true` if the window is invisible or minimized.
///
/// These windows don't receive `RedrawRequested` events on Windows,
/// so they need special handling to keep processing viewport commands.
pub fn is_invisible_or_minimized(window: &Window) -> bool {
    window.is_visible() == Some(false) || window.is_minimized() == Some(true)
}

/// On Mac, a minimized window uses up all CPU:
/// <https://github.com/emilk/egui/issues/325>
///
/// On Windows, an invisible window also uses up all CPU:
/// <https://github.com/emilk/egui/issues/7776>
pub fn sleep_if_invisible_or_minimized(window: Option<&Window>) {
    if window.is_some_and(is_invisible_or_minimized) {
        profiling::scope!("minimized_sleep");
        // PHOSPHOR PATCH: short sleep on macOS so a hidden window can pace the show at 60 fps
        // (the app paces itself); 10 ms elsewhere as upstream.
        std::thread::sleep(core::time::Duration::from_millis(if cfg!(target_os = "macos") { 2 } else { 10 }));
    }
}

/// Create an egui context, restoring it from storage if possible.
pub fn create_egui_context(storage: Option<&dyn crate::Storage>) -> egui::Context {
    profiling::function_scope!();

    pub const IS_DESKTOP: bool = cfg!(any(
        target_os = "freebsd",
        target_os = "linux",
        target_os = "macos",
        target_os = "openbsd",
        target_os = "windows",
    ));

    let egui_ctx = egui::Context::default();

    egui_ctx.set_embed_viewports(!IS_DESKTOP);

    egui_ctx.options_mut(|o| {
        // eframe supports multi-pass (Context::request_discard).
        #[expect(clippy::unwrap_used)]
        {
            o.max_passes = 2.try_into().unwrap();
        }
    });

    let memory = crate::native::epi_integration::load_egui_memory(storage).unwrap_or_default();
    egui_ctx.memory_mut(|mem| *mem = memory);

    egui_ctx
}

/// The custom even `eframe` uses with the [`winit`] event loop.
#[derive(Debug)]
pub enum UserEvent {
    /// A repaint is requested.
    RequestRepaint {
        /// What to repaint.
        viewport_id: ViewportId,

        /// When to repaint.
        when: Instant,

        /// What the cumulative pass number was when the repaint was _requested_.
        cumulative_pass_nr: u64,
    },

    /// A request related to [`accesskit`](https://accesskit.dev/).
    #[cfg(feature = "accesskit")]
    AccessKitActionRequest(accesskit_winit::Event),
}

#[cfg(feature = "accesskit")]
impl From<accesskit_winit::Event> for UserEvent {
    fn from(inner: accesskit_winit::Event) -> Self {
        Self::AccessKitActionRequest(inner)
    }
}

pub trait WinitApp {
    fn egui_ctx(&self) -> Option<&egui::Context>;

    fn window(&self, window_id: WindowId) -> Option<Arc<Window>>;

    fn window_id_from_viewport_id(&self, id: ViewportId) -> Option<WindowId>;

    fn save(&mut self);

    fn save_and_destroy(&mut self);

    fn run_ui_and_paint(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
    ) -> crate::Result<EventResult>;

    fn suspended(&mut self, event_loop: &ActiveEventLoop) -> crate::Result<EventResult>;

    fn resumed(&mut self, event_loop: &ActiveEventLoop) -> crate::Result<EventResult>;

    fn device_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) -> crate::Result<EventResult>;

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: winit::event::WindowEvent,
    ) -> crate::Result<EventResult>;

    #[cfg(feature = "accesskit")]
    fn on_accesskit_event(&mut self, event: accesskit_winit::Event) -> crate::Result<EventResult>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    Wait,

    /// Causes a synchronous repaint inside the event handler. This should only
    /// be used in special situations if the window must be repainted while
    /// handling a specific event. This occurs on Windows when handling resizes.
    ///
    /// `RepaintNow` creates a new frame synchronously, and should therefore
    /// only be used for extremely urgent repaints.
    RepaintNow(WindowId),

    /// Queues a repaint for once the event loop handles its next redraw. Exists
    /// so that multiple input events can be handled in one frame. Does not
    /// cause any delay like `RepaintNow`.
    RepaintNext(WindowId),

    RepaintAt(WindowId, Instant),

    /// Causes a save of the client state when the persistence feature is enabled.
    Save,

    /// Starts the process of ending eframe execution whilst allowing for proper
    /// clean up of resources.
    ///
    /// # Warning
    /// This event **must** occur before [`Exit`] to correctly exit eframe code.
    /// If in doubt, return this event.
    ///
    /// [`Exit`]: [EventResult::Exit]
    CloseRequested,

    /// The event loop will exit, now.
    /// The correct circumstance to return this event is in response to a winit "Destroyed" event.
    ///
    /// # Warning
    /// The [`CloseRequested`] **must** occur before this event to ensure that winit
    /// is able to remove any open windows. Otherwise the window(s) will remain open
    /// until the program terminates.
    ///
    /// [`CloseRequested`]: EventResult::CloseRequested
    Exit,
}

#[cfg(feature = "accesskit")]
pub(crate) fn on_accesskit_window_event(
    egui_winit: &mut egui_winit::State,
    window_id: WindowId,
    event: &accesskit_winit::WindowEvent,
) -> EventResult {
    match event {
        accesskit_winit::WindowEvent::InitialTreeRequested => {
            egui_winit.egui_ctx().enable_accesskit();
            // Because we can't provide the initial tree synchronously
            // (because that would require the activation handler to access
            // the same mutable state as the winit event handler), some
            // AccessKit platform adapters will use a placeholder tree
            // until we send the first tree update. To minimize the possible
            // bad effects of that workaround, repaint and send the tree
            // immediately.
            EventResult::RepaintNow(window_id)
        }
        accesskit_winit::WindowEvent::ActionRequested(request) => {
            egui_winit.on_accesskit_action_request(request.clone());
            // As a form of user input, accessibility actions should cause
            // a repaint, but not until the next regular frame.
            EventResult::RepaintNext(window_id)
        }
        accesskit_winit::WindowEvent::AccessibilityDeactivated => {
            egui_winit.egui_ctx().disable_accesskit();
            // Disabling AccessKit support should have no visible effect,
            // so there's no need to repaint.
            EventResult::Wait
        }
    }
}
