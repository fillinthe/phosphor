//! The Scan the stage window (`crate::scanner`): pick the camera, start, watch it work; the photo it makes
//! becomes the output's stage photo in the Mapping Studio.

use crate::scanner::Status;

use super::*;

/// Temp-data key: the output whose scan window is open.
pub(crate) const SCAN_WINDOW: &str = "phosphor_scan_window";

/// Opens the scan window for output `output` (from the Stage photo menu).
pub(crate) fn open_scan_window(ctx: &egui::Context, output: u64) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(SCAN_WINDOW), Some(output)));
}

/// The window, while open.
pub(crate) fn scan_window(ctx: &egui::Context, st: &mut State) {
    let Some(oid) = ctx.data(|d| d.get_temp::<Option<u64>>(egui::Id::new(SCAN_WINDOW))).flatten() else { return };
    let Some(o) = st.comp.outputs.iter().find(|o| o.id == oid) else {
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(SCAN_WINDOW), None::<u64>));
        return;
    };
    let (name, size) = (o.name.clone(), o.size((st.comp.width, st.comp.height)));
    let camera_key = egui::Id::new("phosphor_scan_camera");
    // Layers that show a camera (or any live input), and the output itself for a test without one.
    let cameras: Vec<(u64, String)> = st
        .comp
        .layers
        .iter()
        .filter(|l| matches!(l.source, SourceSpec::Live { .. }))
        .map(|l| (l.id, l.name.clone()))
        .chain(std::iter::once((oid, format!("{name} itself (a test: no camera needed)"))))
        .collect();
    let mut camera: u64 = ctx.data(|d| d.get_temp(camera_key)).filter(|c| cameras.iter().any(|(id, _)| id == c)).unwrap_or(cameras[0].0);
    let mut open = true;
    let busy = st.scanner.scan.is_some() || st.scanner.status == Status::Decoding;
    egui::Window::new(label(icon::SCAN, &format!("Scan the stage: {name}"))).open(&mut open).default_width(420.0).show(ctx, |ui| {
        hint(ui, "The projector shows black, white and striped pictures while a camera watches the stage. Phosphor works out which projector pixel lit each point the camera sees and draws the stage as the projector sees it: that becomes this output's stage photo, to line slices up with. Darken the room; keep the camera still.");
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| {
                ui.label("Camera");
                let current = cameras.iter().find(|(id, _)| *id == camera).map(|(_, n)| n.clone()).unwrap_or_default();
                ComboBox::from_id_salt("scan_camera").width(260.0).selected_text(current).show_ui(ui, |ui| {
                    for (id, n) in &cameras {
                        ui.selectable_value(&mut camera, *id, n);
                    }
                });
            });
            if cameras.len() == 1 {
                hint(ui, "Add a layer showing your camera first (Live input), aimed at the stage. It can stay hidden.");
            }
            ui.horizontal(|ui| {
                let mut ms = (st.scanner.settle * 1000.0) as f32;
                if Level::range(&mut ms, 100.0, 1500.0, "Wait per picture (ms)").default(400.0).show(ui).changed() {
                    st.scanner.settle = ms as f64 / 1000.0;
                }
            });
        });
        ui.horizontal(|ui| {
            if busy {
                if danger_button(ui, label(icon::STOP, "Stop")).clicked() {
                    st.scanner.cancel();
                }
            } else if action_button(ui, Act::Go, label(icon::SCAN, "Scan")).clicked() {
                let aspect = if camera == oid { size.0 as f32 / size.1.max(1) as f32 } else { st.comp.width as f32 / st.comp.height.max(1) as f32 };
                st.scanner.start(oid, size, camera, aspect);
            }
            match &st.scanner.status {
                Status::Idle => hint(ui, format!("{}×{} projector", size.0, size.1)),
                Status::Scanning { step, of } => ui.add(egui::ProgressBar::new(*step as f32 / (*of).max(1) as f32).text(format!("picture {} of {of}", step + 1))),
                Status::Decoding => hint(ui, "Working it out…"),
                Status::Done { coverage, error: Some(e), .. } => ui.colored_label(accent(), format!("{} Test passed: {:.0}% decoded, every point within {e:.1} projector pixels.", icon::CHECK, coverage * 100.0)),
                Status::Done { coverage, .. } => ui.colored_label(accent(), format!("{} Done: the camera saw the projector on {:.0}% of its picture. It's this output's stage photo now.", icon::CHECK, coverage * 100.0)),
                Status::Failed(e) => ui.colored_label(WARN, e),
            };
        });
    });
    ctx.data_mut(|d| d.insert_temp(camera_key, camera));
    if !open {
        st.scanner.cancel();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(SCAN_WINDOW), None::<u64>));
    }
}
