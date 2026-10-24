//! LAN session, phone remote and QR code.

use super::*;

pub(crate) fn network(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    follow_section(ui, st);
    let State { net, notice, .. } = &mut *st;
    crate::ui::heading(ui, "Session");
    hint(ui, "Anyone on your network opens the link and sees the output live; they can send their music back.");
    crate::ui::toolbar(ui, |ui| {
        ui.add_enabled_ui(!net.running(), |ui| {
            guarded!(ui, net.port, egui::DragValue::new(&mut net.port).range(1024..=65535).prefix("port "));
            crate::ui::dropdown(ui, "net_fps", 60.0).selected_text(format!("{} fps", net.fps)).show_ui(ui, |ui| {
                for f in [10, 15, 20, 30] {
                    crate::ui::pick_value(ui, &mut net.fps, f, format!("{f} fps"));
                }
            });
        });
        if net.running() {
            if crate::ui::command(ui, label(icon::STOP, "Stop session")).clicked() {
                net.stop();
            }
        } else if crate::ui::command(ui, label(icon::BROADCAST, "Start session")).clicked() {
            match net.start() {
                Ok(()) => *notice = None,
                Err(e) => *notice = Some(e),
            }
        }
    });
    if net.running() {
        let url = net.url();
        crate::ui::toolbar(ui, |ui| {
            ui.monospace(&url);
            if crate::ui::command(ui, icon::COPY).on_hover_text("Copy link").clicked() {
                ui.ctx().copy_text(url.clone());
            }
        });
        crate::ui::toolbar(ui, |ui| {
            ui.label("Easy to type");
            ui.monospace(net.local_url());
            hint(ui, "Bonjour name: works on iPhones, iPads, Macs and most Android phones on the same Wi-Fi");
        });
        if net.https {
            let secure = net.https_url();
            crate::ui::toolbar(ui, |ui| {
                ui.label(format!("{} Secure link", icon::LOCK));
                ui.monospace(&secure);
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy the secure link").clicked() {
                    ui.ctx().copy_text(secure.clone());
                }
            });
            hint(ui, "Phones only share their microphone on a secure page. The certificate is Phosphor's own: the phone warns once (Show details, then visit the site) and remembers.");
        }
        ui.separator();
        audience_play(ui, st);
        let State { net, remote_password, require_password, .. } = &mut *st;
        ui.separator();
        crate::ui::heading(ui, format!("{} Phone / tablet remote", icon::DEVICE_MOBILE));
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
        crate::ui::toolbar(ui, |ui| {
            qr_code(ui, &remote, 150.0);
            ui.vertical(|ui| {
                ui.monospace(&remote);
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy link").clicked() {
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
            if crate::ui::command(ui, label(icon::MICROPHONE, "React to viewers' audio")).clicked() {
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
    crate::ui::heading(ui, format!("{} Audience play", icon::GAME_CONTROLLER));
    hint(ui, "People scan the code, line up, and each gets the controls for a while: your Control Surface, clip columns, Surprise.");
    let play = &mut st.comp.play;
    crate::ui::toggle(ui, &mut play.enabled, "Let the audience play");
    if !play.enabled {
        return;
    }
    crate::ui::toolbar(ui, |ui| {
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
            crate::ui::pick_value(ui, &mut play.surface_page, None, "All pages");
            for (i, name) in pages.iter().enumerate() {
                crate::ui::pick_value(ui, &mut play.surface_page, Some(i), name);
            }
        });
        ui.label("Title");
        ui.add(egui::TextEdit::singleline(&mut play.title).desired_width(160.0));
    });
    // Several players: each place can have its own page (and so its own controls).
    if play.players > 1 {
        play.place_pages.resize(play.players as usize, None);
        ui.horizontal_wrapped(|ui| {
            let pages = &st.comp.surface.pages;
            for (i, slot) in play.place_pages.iter_mut().enumerate() {
                ui.label(format!("Player {}", i + 1));
                let current = slot.and_then(|p| pages.get(p)).cloned().unwrap_or_else(|| "usual".into());
                crate::ui::dropdown(ui, ("place_page", i), 90.0).selected_text(current).show_ui(ui, |ui| {
                    crate::ui::pick_value(ui, slot, None, "usual").on_hover_text("The page above, or page by place");
                    for (p, name) in pages.iter().enumerate() {
                        crate::ui::pick_value(ui, slot, Some(p), name);
                    }
                });
            }
        });
    }
    hint(ui, "Each control's guest settings (whether guests get it, how far they can push it) are in the Control Surface editor.");
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
    crate::ui::toolbar(ui, |ui| {
        qr_code(ui, &url, 150.0);
        ui.vertical(|ui| {
            ui.monospace(&url);
            crate::ui::toolbar(ui, |ui| {
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy link").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
                #[cfg(not(target_arch = "wasm32"))]
                let (off, on) = (label(icon::QR_CODE, "Take the code off the output"), label(icon::QR_CODE, "Show the code on the output"));
                if steady_button(ui, if on_output { off.clone() } else { on.clone() }, &[&off, &on]).clicked() {
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
                crate::ui::toolbar(ui, |ui| {
                    ui.colored_label(accent(), format!("{} Playing: {}{page}{left}", icon::HAND_WAVING, p.label));
                    if crate::ui::small_command(ui, "Skip").on_hover_text("End this turn now; the next person gets the controls").clicked() {
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

/// Follow another Phosphor: play its show (project, beat, clips) on this machine's own outputs (`crate::follow`).
#[cfg(not(target_arch = "wasm32"))]
fn follow_section(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::heading(ui, "Follow another Phosphor");
    let key = egui::Id::new("phosphor_follow_address");
    match &st.following {
        Some(l) => {
            let rtt = l.rtt().map(|r| format!(" · {:.0} ms away", r * 1000.0)).unwrap_or_default();
            let text = format!("{} {}: {}{rtt}", icon::LINK, l.address, l.status);
            let mut stop = false;
            crate::ui::toolbar(ui, |ui| {
                ui.colored_label(accent(), text);
                stop = crate::ui::command(ui, label(icon::STOP, "Stop following")).clicked();
            });
            if stop {
                st.following = None;
                st.follow_at_launch.clear();
                return;
            }
            // A player: a Mac (or PC) that only plays this show, from login, on its own projectors.
            let addr = st.following.as_ref().map(|l| l.address.clone()).unwrap_or_default();
            let mut always = st.follow_at_launch == addr;
            if crate::ui::toggle(ui, &mut always, "Every time Phosphor starts (a player)")
                .on_hover_text("This Mac becomes a player: it follows this leader at launch. With \"Start Phosphor when this Mac logs in\" and \"Open outputs on their screens\" (Output panel) it needs no one to set it up after a restart")
                .changed()
            {
                st.follow_at_launch = if always { addr } else { String::new() };
            }
            hint(ui, "This Mac plays the leader's show on its own outputs and mapping. Your changes here are replaced by the leader's; its media must be at the same paths here.");
        }
        None => {
            let mut addr: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 90.0).clamp(100.0, 260.0);
                crate::ui::text_field(ui, &mut addr, "leader's address (from its Session link)", Some(w));
                if crate::ui::command_if(ui, !addr.trim().is_empty(), label(icon::LINK, "Follow")).clicked() {
                    st.following = Some(crate::follow::Link::connect(&addr));
                }
            });
            hint(ui, "On the leading Mac, start its Session below. Then type its address here: both play the same show in sync, each on its own projectors.");
            ui.ctx().data_mut(|d| d.insert_temp(key, addr));
        }
    }
    ui.separator();
}
