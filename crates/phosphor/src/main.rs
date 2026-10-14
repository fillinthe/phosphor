mod app;
mod audio;
mod bench;
mod clock;
mod control;
mod explorer;
#[cfg(not(target_arch = "wasm32"))]
mod isf_lib;
mod media;
mod net;
mod panels;
mod presets;
mod stats;
mod stream;
mod thumbs;
mod theme;

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result {
    env_logger::init();
    // Converter crashes on odd third-party shaders are caught and skipped; keep them out of the log.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let isf = std::thread::current().name() == Some("phosphor-isf");
        if !isf {
            default_hook(info);
        }
    }));
    let bench = bench::BenchConfig::from_args(std::env::args().skip(1));
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Phosphor")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([960.0, 600.0]),
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
