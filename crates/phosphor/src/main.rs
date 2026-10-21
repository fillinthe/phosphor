mod account;
mod text;
#[cfg(target_arch = "wasm32")]
mod web_files;
mod app;
mod audio;
mod bench;
mod clock;
mod control;
#[cfg(not(target_arch = "wasm32"))]
mod dialogs;
#[cfg(not(target_arch = "wasm32"))]
mod dmx;
mod explorer;
#[cfg(not(target_arch = "wasm32"))]
mod folders;
#[cfg(not(target_arch = "wasm32"))]
mod follow;
#[cfg(not(target_arch = "wasm32"))]
mod isf_lib;
mod media;
mod net;
#[cfg(not(target_arch = "wasm32"))]
mod net_tls;
#[cfg(not(target_arch = "wasm32"))]
mod link;
mod ltc;
#[cfg(not(target_arch = "wasm32"))]
mod os;
#[cfg(not(target_arch = "wasm32"))]
mod session_log;
#[cfg(not(target_arch = "wasm32"))]
mod convert;
mod lut;
mod pacing;
mod panels;
mod ui;
mod presets;
#[cfg(not(target_arch = "wasm32"))]
mod recovery;
mod remote;
mod screens;
mod stats;
mod support;
#[cfg(any(target_os = "macos", windows))]
mod login_item;
mod stream;
#[cfg(not(target_arch = "wasm32"))]
mod stream_audio;
#[cfg(not(target_arch = "wasm32"))]
mod show;
#[cfg(not(target_arch = "wasm32"))]
mod export;
#[cfg(not(target_arch = "wasm32"))]
mod import;
mod history;
mod menu;
#[cfg(target_os = "macos")]
mod syphon;
#[cfg(target_os = "macos")]
mod screen_capture;
#[cfg(target_os = "macos")]
mod ffgl;
#[cfg(target_os = "macos")]
mod menu_test;
#[cfg(not(target_arch = "wasm32"))]
mod tracking;
#[cfg(not(target_arch = "wasm32"))]
mod pjlink;
mod scopes;
#[cfg(not(target_arch = "wasm32"))]
mod web_media;
#[cfg(not(target_arch = "wasm32"))]
mod crash_report;
#[cfg(not(target_arch = "wasm32"))]
mod ai_shader;
#[cfg(not(target_arch = "wasm32"))]
mod webrtc;
#[cfg(not(target_arch = "wasm32"))]
mod scanner;
#[cfg(not(target_arch = "wasm32"))]
mod dylib;
#[cfg(any(target_os = "macos", windows))]
mod ndi;
#[cfg(windows)]
mod spout;
mod thumbs;
#[cfg(not(target_arch = "wasm32"))]
mod tosc;
mod update;
mod theme;

/// eframe's GPU setup plus block-compressed textures when the GPU has them (Hap movies decode on the GPU).
#[cfg(not(target_arch = "wasm32"))]
fn gpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{WgpuConfiguration, WgpuSetup, WgpuSetupCreateNew};
    let mut setup = WgpuSetupCreateNew::without_display_handle();
    let base = setup.device_descriptor.clone();
    setup.device_descriptor = std::sync::Arc::new(move |adapter| {
        let mut d = base(adapter);
        d.required_features |= adapter.features() & (eframe::wgpu::Features::TEXTURE_COMPRESSION_BC | eframe::wgpu::Features::TEXTURE_FORMAT_16BIT_NORM | eframe::wgpu::Features::TIMESTAMP_QUERY);
        d
    });
    WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
}

#[cfg(not(target_arch = "wasm32"))]
fn app_icon() -> std::sync::Arc<eframe::egui::IconData> {
    let rgba = image::load_from_memory(include_bytes!("../../../assets/icon-1024.png")).map(|i| i.to_rgba8()).ok();
    std::sync::Arc::new(match rgba {
        Some(img) => eframe::egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() },
        None => eframe::egui::IconData::default(),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--watchdog") {
        if let Some(pid) = args.get(2).and_then(|p| p.parse().ok()) {
            recovery::run_watchdog(pid);
        }
        return Ok(());
    }
    // Opens one FFGL plugin and prints what it says (run by the plugin scan, so a bad plugin only takes this down).
    #[cfg(target_os = "macos")]
    if args.get(1).map(String::as_str) == Some("--ffgl-probe") {
        if let Some(path) = args.get(2) {
            println!("{}", ffgl::probe_main(std::path::Path::new(path)));
        }
        return Ok(());
    }
    // Converter crashes on odd third-party shaders are caught and skipped; keep them out of the log.
    // Everything else is also written to crash.log so problems can be diagnosed after the fact.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let isf = std::thread::current().name() == Some("phosphor-isf");
        if isf {
            return;
        }
        if let Some(path) = recovery::crash_log_path() {
            use std::io::Write;
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
                let when = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
                let _ = writeln!(f, "[{when}] {info}");
            }
        }
        default_hook(info);
    }));
    let bench = bench::BenchConfig::from_args(std::env::args().skip(1));
    session_log::init(bench.is_none());
    // A live show must never be slowed down by App Nap or timer coalescing (e.g. while the
    // control window is minimized or covered and the outputs are on a projector).
    #[cfg(target_os = "macos")]
    let _show_activity = keep_awake_for_show();
    if bench.is_none() {
        recovery::spawn_watchdog();
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Phosphor")
            .with_inner_size(bench.as_ref().and_then(|b| b.window).map_or([1440.0, 900.0], |(w, h)| [w, h]))
            .with_min_inner_size([960.0, 600.0])
            .with_icon(app_icon()),
        // A benchmark must start from a clean slate, not yesterday's project.
        persist_window: bench.is_none(),
        // …and must never save over the user's settings: its own throwaway storage.
        persistence_path: bench.is_some().then(|| std::env::temp_dir().join(format!("phosphor-bench-{}", std::process::id())).join("app.ron")),
        wgpu_options: gpu_options(),
        ..Default::default()
    };
    eframe::run_native(show::storage_name(), options, Box::new(move |cc| Ok(Box::new(app::PhosphorApp::new(cc, bench)))))
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    wasm_bindgen_futures::spawn_local(async {
        let canvas = web_sys::window()
            .and_then(|w| w.document())
            .and_then(|d| d.get_element_by_id("phosphor_canvas"))
            .and_then(|c| c.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("index.html must contain <canvas id=\"phosphor_canvas\">");
        eframe::WebRunner::new()
            .start(canvas, eframe::WebOptions::default(), Box::new(|cc| Ok(Box::new(app::PhosphorApp::new(cc, None)))))
            .await
            .expect("failed to start Phosphor");
    });
}

/// Tells macOS this is user-initiated, latency-critical work (a live show), which turns off App
/// Nap and timer throttling for the app's lifetime. Keep the returned token alive.
#[cfg(target_os = "macos")]
fn keep_awake_for_show() -> Option<objc2::rc::Retained<objc2::runtime::AnyObject>> {
    use objc2::msg_send;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    // NSActivityUserInitiated | NSActivityLatencyCritical (no idle-system-sleep prevention: the
    // Mac can still sleep when the user wants).
    const OPTIONS: u64 = 0x00FF_FFFF | 0xFF_0000_0000;
    let class = AnyClass::get(c"NSProcessInfo")?;
    let reason = objc2_foundation::NSString::from_str("Phosphor is running a live show");
    // SAFETY: +processInfo and -beginActivityWithOptions:reason: per NSProcessInfo.h.
    unsafe {
        let info: Option<Retained<AnyObject>> = msg_send![class, processInfo];
        let info = info?;
        msg_send![&*info, beginActivityWithOptions: OPTIONS, reason: &*reason]
    }
}
