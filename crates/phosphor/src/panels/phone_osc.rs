//! Phone or tablet over OSC (TouchOSC and friends): where to point the app, how to get the layout onto the
//! device, and who is sending. Lives in the MIDI / OSC panel.

use super::*;

/// Where the session hands the layout out.
const LAYOUT_PATH: &str = "/Phosphor.tosc";

pub(crate) fn phone_section(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading_help(ui, format!("{} Phone or tablet", icon::DEVICE_MOBILE), "TouchOSC (or any OSC app) on the same Wi-Fi sends to this Mac and gets the values back. Three steps: point the app here, put the layout on it, move a control.");
    });
    let ip = st.control.local_ip.clone().unwrap_or_else(|| "this Mac's address".to_string());
    // 1. Where to point the app.
    crate::ui::toolbar(ui, |ui| {
        ui.label(RichText::new("1").strong().color(accent()));
        ui.label("In the app's OSC connection:");
        ui.monospace(format!("host {ip}"));
        if crate::ui::small_command(ui, icon::COPY).on_hover_text("Copy this Mac's address").clicked() {
            ui.ctx().copy_text(ip.clone());
        }
        ui.monospace(format!("send to port {}", st.control.osc_port));
        ui.monospace(format!("receive on port {}", st.control.feedback_port));
        help(ui, "TouchOSC: Connections › OSC. Its Browse list shows Phosphor (Zeroconf) and fills these in. Receive = the feedback port below, so the phone shows Phosphor's values.");
    });
    // 2. The layout.
    crate::ui::toolbar(ui, |ui| {
        ui.label(RichText::new("2").strong().color(accent()));
        ui.label("The layout (your Control Surface as TouchOSC pages):");
        help(ui, "Nothing built yet? The layout starts with the master, each layer's opacity and the first clip columns. Any other OSC app works too: right-click a control › Listen shows its address.");
        #[cfg(not(target_arch = "wasm32"))]
        {
            layout_menu(ui, st);
            if crate::ui::command(ui, label(icon::EXPORT, "Save as a file…")).on_hover_text("A .tosc file: AirDrop or copy it to the device and open it in TouchOSC (or in its editor)").clicked() {
                st.dialogs.save_file(crate::dialogs::Purpose::TouchOsc, ("TouchOSC layout", &["tosc"]), "Phosphor.tosc");
            }
            if crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Import a layout…")).on_hover_text("Bring a TouchOSC layout in as Control Surface pages: same controls in the same places, each control's OSC address already mapped").clicked() {
                st.dialogs.pick_file(crate::dialogs::Purpose::ImportTouchOsc, Some(("TouchOSC layout", &["tosc", "xml"])));
            }
        }
    });
    // 3. Who's sending.
    crate::ui::toolbar(ui, |ui| {
        ui.label(RichText::new("3").strong().color(accent()));
        let senders: Vec<crate::control::OscSender> = st.control.activity.lock().map(|a| a.osc.values().cloned().collect()).unwrap_or_default();
        let mut live: Vec<_> = senders.iter().filter(|s| s.last_seen.elapsed().as_secs() < 120).collect();
        live.sort_by_key(|s| s.last_seen.elapsed());
        if live.is_empty() {
            hint(ui, "Nothing from the network yet: move a control on the phone. The Devices panel lists every sender.");
        } else {
            let words: Vec<String> = live
                .iter()
                .take(3)
                .map(|s| {
                    let ago = s.last_seen.elapsed().as_secs();
                    let when = if ago < 3 { "now".to_string() } else { format!("{ago} s ago") };
                    format!("{} ({}, {when})", s.ip, if s.uses_builtin { "Phosphor's addresses" } else { s.last_address.as_str() })
                })
                .collect();
            ui.label(RichText::new(format!("{} Hearing from {}", icon::BROADCAST, words.join(" · "))).color(accent()));
        }
    });
}

/// "Get it on the phone": a QR code (and link) the phone opens to download the layout from the session. Starts
/// the session if it's off. The layout is rebuilt whenever the surface changes while the menu is open.
#[cfg(not(target_arch = "wasm32"))]
fn layout_menu(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::field_menu(ui, label(icon::QR_CODE, "Get it on the phone"), |ui| {
        ui.set_min_width(260.0);
        if !st.net.running() {
            help(ui, "The phone downloads the layout from this Mac over the session (the Session panel's web server).");
            if crate::ui::command(ui, label(icon::PLAY, "Start the session")).clicked()
                && let Err(e) = st.net.start()
            {
                st.notice = Some(format!("Couldn't start the session: {e}"));
            }
            return;
        }
        // Rebuild only when the layout changed (its XML is cheap to make; compressing it is not per frame).
        let xml = crate::tosc::layout_xml(&st.comp);
        let key = egui::Id::new("tosc_offered");
        if ui.ctx().data(|d| d.get_temp::<String>(key)).as_deref() != Some(xml.as_str()) {
            st.net.offer(LAYOUT_PATH, "application/octet-stream", "Phosphor.tosc", crate::tosc::export(&st.comp));
            ui.ctx().data_mut(|d| d.insert_temp(key, xml));
        }
        let url = format!("{}{}", st.net.url(), &LAYOUT_PATH[1..]);
        ui.vertical_centered(|ui| {
            crate::ui::qr_code(ui, &url, 200.0);
        });
        crate::ui::toolbar(ui, |ui| {
            ui.monospace(&url);
            if crate::ui::small_command(ui, icon::COPY).on_hover_text("Copy the link").clicked() {
                ui.ctx().copy_text(url.clone());
            }
        });
        help(ui, "Scan it with the phone's camera (same Wi-Fi). The file downloads; open it in TouchOSC (Files › Phosphor.tosc › share › TouchOSC). The connection settings above still go in by hand.");
    });
}
