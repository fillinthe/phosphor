//! The Find LEDs with a camera window (`crate::led_scan`): pick the camera and the fixtures, start, watch the lights
//! blink; the fixtures move to where the camera saw their lights.

use crate::led_scan::Status;

use super::*;

/// Temp-data key: the window is open.
const LED_SCAN_WINDOW: &str = "phosphor_led_scan_window";

pub(crate) fn open_led_scan_window(ctx: &egui::Context) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::new(LED_SCAN_WINDOW), true));
}

/// The window, while open.
pub(crate) fn led_scan_window(ctx: &egui::Context, st: &mut State) {
    if !ctx.data(|d| d.get_temp::<bool>(egui::Id::new(LED_SCAN_WINDOW))).unwrap_or(false) {
        return;
    }
    let cameras = camera_layers(st);
    let camera_key = egui::Id::new("phosphor_led_scan_camera");
    let only_key = egui::Id::new("phosphor_led_scan_only");
    let mut camera: u64 = ctx.data(|d| d.get_temp(camera_key)).filter(|c| cameras.iter().any(|(id, _)| id == c)).or(cameras.first().map(|c| c.0)).unwrap_or(0);
    let mut only: Option<u64> = ctx.data(|d| d.get_temp(only_key)).flatten().filter(|id| st.comp.dmx.fixtures.iter().any(|f| f.id == *id));
    let busy = st.led_scan.busy();
    let mut open = true;
    egui::Window::new(label(icon::SCAN, "Find LEDs with a camera")).open(&mut open).default_width(420.0).show(ctx, |ui| {
        hint(ui, "The lights blink in coded patterns while a camera watches them; each light is placed where the camera saw it. Then the picture lands on the lights the way the camera sees them, however they're hung. Aim the camera at all the lights, keep it still and darken the room.");
        ui.add_enabled_ui(!busy, |ui| {
            if cameras.is_empty() {
                hint(ui, "Add a layer showing your camera first (Live input), aimed at the lights. It can stay hidden.");
            } else {
                camera_combo(ui, "led_scan_camera", &cameras, &mut camera);
            }
            crate::ui::toolbar(ui, |ui| {
                ui.label("Lights");
                let name = |id: Option<u64>| match id.and_then(|id| st.comp.dmx.fixtures.iter().find(|f| f.id == id)) {
                    Some(f) => f.name.clone(),
                    None => "Every LED fixture that's on".to_string(),
                };
                crate::ui::dropdown(ui, "led_scan_only", 260.0).selected_text(name(only)).show_ui(ui, |ui| {
                    crate::ui::pick_value(ui, &mut only, None, name(None));
                    for f in st.comp.dmx.fixtures.iter().filter(|f| f.shape != phosphor_engine::FixtureShape::MovingHead) {
                        crate::ui::pick_value(ui, &mut only, Some(f.id), &f.name);
                    }
                });
            });
            ui.horizontal(|ui| {
                let mut ms = (st.led_scan.settle * 1000.0) as f32;
                if Level::range(&mut ms, 100.0, 1500.0, "Wait per picture (ms)").default(300.0).tip("Longer if lights or the camera are slow to react (wireless controllers, auto-exposure)").show(ui).changed() {
                    st.led_scan.settle = ms as f64 / 1000.0;
                }
            });
        });
        ui.horizontal(|ui| {
            if busy {
                if danger_button(ui, label(icon::STOP, "Stop")).clicked() {
                    st.led_scan.cancel();
                }
            } else if ui.add_enabled_ui(!cameras.is_empty(), |ui| action_button(ui, Act::Go, label(icon::SCAN, "Find"))).inner.clicked() {
                let aspect = st.comp.width as f32 / st.comp.height.max(1) as f32;
                match st.led_scan.start(&st.comp.dmx, only, camera, aspect) {
                    // The lights have to be driven to blink.
                    Ok(()) => st.comp.dmx.enabled = true,
                    Err(e) => st.led_scan.status = Status::Failed(e),
                }
            }
            match &st.led_scan.status {
                Status::Idle => hint(ui, "The fixtures you pick become \"Found by camera\" (wiring kept)."),
                Status::Scanning { step, of } => ui.add(crate::ui::meter(*step as f32 / (*of).max(1) as f32).text(format!("picture {} of {of}", step + 1))),
                Status::Decoding => hint(ui, "Working it out…"),
                Status::Done { seen, total, .. } if seen == total => ui.colored_label(accent(), format!("{} Found all {total} lights.", icon::CHECK)),
                Status::Done { placed, fixtures, seen, total } => ui.colored_label(
                    WARN,
                    format!(
                        "Found {seen} of {total} lights; the others were placed between their neighbours. {placed} of {fixtures} fixtures moved (one moves when a quarter of its lights were seen). Check them on the picture."
                    ),
                ),
                Status::Failed(e) => ui.colored_label(WARN, e),
            };
        });
    });
    ctx.data_mut(|d| {
        d.insert_temp(camera_key, camera);
        d.insert_temp(only_key, only);
    });
    if !open {
        st.led_scan.cancel();
        ctx.data_mut(|d| d.insert_temp(egui::Id::new(LED_SCAN_WINDOW), false));
    }
}
