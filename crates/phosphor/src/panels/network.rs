//! LAN session, phone remote and QR code.

use super::*;

pub(crate) fn network(ui: &mut egui::Ui, st: &mut State) {
    let State { net, notice, audio, remote_password, require_password, .. } = st;
    ui.label(RichText::new("Session").strong());
    ui.weak("Anyone on your network opens the link and sees the output live; they can send their music back.");
    ui.horizontal(|ui| {
        ui.add_enabled_ui(!net.running(), |ui| {
            guarded!(ui, net.port, egui::DragValue::new(&mut net.port).range(1024..=65535).prefix("port "));
            ComboBox::from_id_salt("net_fps").width(60.0).selected_text(format!("{} fps", net.fps)).show_ui(ui, |ui| {
                for f in [10, 15, 20, 30] {
                    ui.selectable_value(&mut net.fps, f, format!("{f} fps"));
                }
            });
        });
        if net.running() {
            if ui.button(label(icon::STOP, "Stop session")).clicked() {
                net.stop();
            }
        } else if ui.button(label(icon::BROADCAST, "Start session")).clicked() {
            match net.start() {
                Ok(()) => *notice = None,
                Err(e) => *notice = Some(e),
            }
        }
    });
    if net.running() {
        let url = net.url();
        ui.horizontal(|ui| {
            ui.monospace(&url);
            if ui.button(icon::COPY).on_hover_text("Copy link").clicked() {
                ui.ctx().copy_text(url.clone());
            }
        });
        ui.separator();
        ui.label(RichText::new(format!("{} Phone / tablet remote", icon::DEVICE_MOBILE)).strong());
        ui.weak("Scan with your phone or iPad camera: faders, clips, tap tempo and the selected layer's controls, synced both ways.");
        ui.horizontal(|ui| {
            ui.checkbox(require_password, label(icon::LOCK, "Password to control"))
                .on_hover_text("Without it, people who open the link can only watch the output");
            ui.add_enabled(
                *require_password,
                egui::TextEdit::singleline(remote_password).password(true).hint_text("password").desired_width(140.0),
            );
        });
        if *require_password && remote_password.is_empty() {
            ui.colored_label(WARN, "Type a password, or anyone can control.");
        }
        let remote = net.remote_url();
        ui.horizontal(|ui| {
            qr_code(ui, &remote, 150.0);
            ui.vertical(|ui| {
                ui.monospace(&remote);
                if ui.button(icon::COPY).on_hover_text("Copy link").clicked() {
                    ui.ctx().copy_text(remote.clone());
                }
                ui.monospace(format!("remotes connected: {}", net.remotes.load(std::sync::atomic::Ordering::Relaxed)));
                ui.weak("Tip: Share ▸ Add to Home Screen makes it full-screen like an app.");
            });
        });
        ui.separator();
        ui.monospace(format!(
            "viewers {} · sending audio {}",
            net.viewers.load(std::sync::atomic::Ordering::Relaxed),
            net.audio_peers.load(std::sync::atomic::Ordering::Relaxed)
        ));
        if !audio.is_network() {
            if ui.button(label(icon::MICROPHONE, "React to viewers' audio")).clicked() {
                audio.use_network();
            }
        } else {
            ui.colored_label(ACCENT, "Audio analysis is listening to session viewers");
        }
        ui.add_space(4.0);
        ui.weak("Picture: MJPEG at up to 1280 px (works in any browser, Pi included). Mic sending needs a secure page: use http://localhost or add this origin to chrome://flags/#unsafely-treat-insecure-origin-as-secure on the viewer.");
    }
    if let Some(msg) = notice {
        ui.colored_label(WARN, msg.as_str());
    }
}
