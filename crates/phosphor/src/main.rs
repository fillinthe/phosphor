mod app;
mod audio;
mod bench;
mod clock;
mod control;
#[cfg(not(target_arch = "wasm32"))]
mod dialogs;
mod explorer;
#[cfg(not(target_arch = "wasm32"))]
mod folders;
#[cfg(not(target_arch = "wasm32"))]
mod isf_lib;
mod media;
mod net;
mod panels;
mod presets;
#[cfg(not(target_arch = "wasm32"))]
mod recovery;
mod remote;
mod screens;
mod stats;
mod stream;
mod thumbs;
mod theme;

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
    env_logger::init();
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
    if bench.is_none() {
        recovery::spawn_watchdog();
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Phosphor")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0])
            .with_icon(app_icon()),
        // A benchmark must start from a clean slate, not yesterday's project.
        persist_window: bench.is_none(),
        ..Default::default()
    };
    eframe::run_native("Phosphor", options, Box::new(move |cc| Ok(Box::new(app::PhosphorApp::new(cc, bench)))))
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
