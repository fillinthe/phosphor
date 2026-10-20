//! LAN session, phone remote and QR code.

use super::*;

pub(crate) fn network(ui: &mut egui::Ui, st: &mut State) {
    let State { net, notice, .. } = &mut *st;
    ui.label(RichText::new("Session").strong());
    hint(ui, "Anyone on your network opens the link and sees the output live; they can send their music back.");
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
        ui.horizontal(|ui| {
            ui.label("Easy to type");
            ui.monospace(net.local_url());
            hint(ui, "Bonjour name: works on iPhones, iPads, Macs and most Android phones on the same Wi-Fi");
        });
        if net.https {
            let secure = net.https_url();
            ui.horizontal(|ui| {
                ui.label(format!("{} Secure link", icon::LOCK));
                ui.monospace(&secure);
                if ui.button(icon::COPY).on_hover_text("Copy the secure link").clicked() {
                    ui.ctx().copy_text(secure.clone());
                }
            });
            hint(ui, "Phones only share their microphone on a secure page. The certificate is Phosphor's own: the phone warns once (Show details, then visit the site) and remembers.");
        }
        ui.separator();
        audience_play(ui, st);
        let State { net, remote_password, require_password, .. } = &mut *st;
        ui.separator();
        ui.label(RichText::new(format!("{} Phone / tablet remote", icon::DEVICE_MOBILE)).strong());
        hint(ui, "Scan with your phone or iPad camera: faders, clips, tap tempo and the selected layer's controls, synced both ways.");
        ui.horizontal(|ui| {
            crate::ui::toggle(ui, require_password, label(icon::LOCK, "Password to control"))
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
                hint(ui, format!("Tip: Share {} Add to Home Screen makes it full-screen like an app.", icon::CARET_RIGHT));
            });
        });
        let State { net, audio, .. } = st;
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
            ui.colored_label(accent(), "Audio analysis is listening to session viewers");
        }
        ui.add_space(4.0);
        hint(ui, "Picture: MJPEG at up to 1280 px (works in any browser, Pi included). Mic sending needs a secure page: use http://localhost or add this origin to chrome://flags/#unsafely-treat-insecure-origin-as-secure on the viewer.");
    }
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg.as_str());
    }
}

/// Audience play: a QR code for the room; people scan it and take turns at the chosen controls.
fn audience_play(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.input(|i| i.time);
    ui.label(RichText::new(format!("{} Audience play", icon::GAME_CONTROLLER)).strong());
    hint(ui, "People scan the code, line up, and each gets the controls for a while: your Control Surface, clip columns, Surprise.");
    let play = &mut st.comp.play;
    crate::ui::toggle(ui, &mut play.enabled, "Let the audience play");
    if !play.enabled {
        return;
    }
    ui.horizontal(|ui| {
        ui.label("Each turn");
        guarded!(ui, play.turn_secs, egui::DragValue::new(&mut play.turn_secs).range(10..=900).suffix(" s"));
        ui.label("Rest after");
        guarded!(ui, play.rest_secs, egui::DragValue::new(&mut play.rest_secs).range(0..=600).suffix(" s")).on_hover_text("How long a phone sits out after its turn before it can play again. 0: with nobody waiting, a player keeps the controls.");
        ui.label("Players at once");
        guarded!(ui, play.players, egui::DragValue::new(&mut play.players).range(1..=8)).on_hover_text("More than one: each player gets their own Surface page (page 1, 2, …) unless you pick one page for all");
        crate::ui::toggle(ui, &mut play.columns, "Clip columns");
        crate::ui::toggle(ui, &mut play.surprise, "Surprise button");
    });
    crate::ui::toggle(ui, &mut play.show_names, "Show the players' names on the output").on_hover_text("A \"Now playing: …\" text layer along the bottom; guests type their name on the play page");
    ui.horizontal(|ui| {
        ui.label("Surface page");
        let pages = &st.comp.surface.pages;
        let current = play.surface_page.and_then(|p| pages.get(p)).cloned().unwrap_or_else(|| "All pages".into());
        ComboBox::from_id_salt("play_page").selected_text(current).show_ui(ui, |ui| {
            ui.selectable_value(&mut play.surface_page, None, "All pages");
            for (i, name) in pages.iter().enumerate() {
                ui.selectable_value(&mut play.surface_page, Some(i), name);
            }
        });
        ui.label("Title");
        ui.add(egui::TextEdit::singleline(&mut play.title).desired_width(160.0));
    });
    let style_changed = ui
        .horizontal(|ui| {
            ui.label("Code on the output");
            choice_combo(ui, "qr_style", &mut play.qr_style, 190.0, false).changed() | crate::ui::toggle(ui, &mut play.qr_caption, "Words under it").changed()
        })
        .inner;
    #[cfg(not(target_arch = "wasm32"))]
    if style_changed {
        st.refresh_play_qr(now);
    }
    #[cfg(target_arch = "wasm32")]
    let _ = style_changed;
    let url = st.net.play_url();
    let on_output = st.comp.layers.iter().any(|l| l.name == crate::app::play::QR_LAYER);
    let mut toggle = false;
    ui.horizontal(|ui| {
        qr_code(ui, &url, 150.0);
        ui.vertical(|ui| {
            ui.monospace(&url);
            ui.horizontal(|ui| {
                if ui.button(icon::COPY).on_hover_text("Copy link").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
                #[cfg(not(target_arch = "wasm32"))]
                if ui.button(label(icon::QR_CODE, if on_output { "Take the code off the output" } else { "Show the code on the output" })).clicked() {
                    toggle = true;
                }
            });
            let waiting = st.play_queue(now).len();
            let players = st.players(now);
            if players.is_empty() {
                hint(ui, "Nobody playing yet.");
            }
            let pages = st.comp.surface.pages.clone();
            let mut skip: Option<String> = None;
            for p in &players {
                let left = p.left.map(|s| format!(" · {}:{:02} left", s as u32 / 60, s as u32 % 60)).unwrap_or_default();
                let page = p.page.and_then(|i| pages.get(i)).map(|n| format!(" · {n}")).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.colored_label(accent(), format!("{} Playing: {}{page}{left}", icon::HAND_WAVING, p.label));
                    if ui.small_button("Skip").on_hover_text("End this turn now; the next person gets the controls").clicked() {
                        skip = Some(p.who.clone());
                    }
                });
            }
            if let Some(who) = skip {
                st.skip_turn(Some(&who), now);
            }
            ui.monospace(format!("{waiting} waiting"));
            let _ = on_output;
        });
    });
    #[cfg(target_arch = "wasm32")]
    let _ = toggle;
    #[cfg(not(target_arch = "wasm32"))]
    if toggle {
        st.toggle_play_qr(now);
    }
}
