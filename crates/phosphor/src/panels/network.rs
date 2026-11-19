//! LAN session, phone remote and QR code.

use super::*;

pub(crate) fn network(ui: &mut egui::Ui, st: &mut State) {
    #[cfg(not(target_arch = "wasm32"))]
    follow_section(ui, st);
    #[cfg(not(target_arch = "wasm32"))]
    send_section(ui, st);
    // The router's opening goes with the Session.
    #[cfg(not(target_arch = "wasm32"))]
    if !st.net.running() {
        st.internet = None;
    }
    let State { net, notice, .. } = &mut *st;
    crate::ui::heading_help(ui, "Session", "Anyone on your network opens the link and sees the output live; they can send their music back.");
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
        crate::ui::field(ui, "Easy to type", |ui| {
            ui.monospace(net.local_url());
            help(ui, "Bonjour name: works on iPhones, iPads, Macs and most Android phones on the same Wi-Fi");
        });
        if net.https {
            let secure = net.https_url();
            crate::ui::toolbar(ui, |ui| {
                ui.label(format!("{} Secure link", icon::LOCK));
                ui.monospace(&secure);
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy the secure link").clicked() {
                    ui.ctx().copy_text(secure.clone());
                }
                help(ui, "Phones only share their microphone on a secure page. The certificate is Phosphor's own: the phone warns once (Show details, then visit the site) and remembers.");
            });
        }
        #[cfg(not(target_arch = "wasm32"))]
        internet_row(ui, st);
        ui.separator();
        audience_play(ui, st);
        let State { net, remote_password, require_password, .. } = &mut *st;
        ui.separator();
        crate::ui::heading_help(ui, format!("{} Phone / tablet remote", icon::DEVICE_MOBILE), "Scan with your phone or iPad camera: faders, clips, tap tempo and the selected layer's controls, synced both ways. On the phone, Share › Add to Home Screen keeps it as an app with Phosphor's icon, full screen.");
        ui.horizontal(|ui| {
            crate::ui::toggle(ui, require_password, label(icon::LOCK, "Password to control"))
                .on_hover_text("Without it, people who open the link can only watch the output. Another Phosphor that follows this show needs it too");
            ui.add_enabled_ui(*require_password, |ui| crate::ui::password_field(ui, remote_password, "password", 140.0));
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
                help(ui, &format!("Tip: Share {} Add to Home Screen makes it full-screen like an app.", icon::CARET_RIGHT));
            });
        });
        let State { net, audio, .. } = st;
        ui.separator();
        ui.monospace(format!(
            "viewers {} · sending audio {}",
            net.viewers.load(std::sync::atomic::Ordering::Relaxed),
            net.audio_peers.load(std::sync::atomic::Ordering::Relaxed)
        ));
        crate::ui::toolbar(ui, |ui| {
            if !audio.is_network() {
                if crate::ui::command(ui, label(icon::MICROPHONE, "React to viewers' audio")).clicked() {
                    audio.use_network();
                }
            } else {
                ui.colored_label(accent(), "Audio analysis is listening to session viewers");
            }
            help(ui, "Picture: MJPEG at up to 1280 px (works in any browser, Pi included). Mic sending needs a secure page: use http://localhost or add this origin to chrome://flags/#unsafely-treat-insecure-origin-as-secure on the viewer.");
        });
    }
    if let Some(msg) = &st.notice {
        ui.colored_label(WARN, msg.as_str());
    }
}

/// The Session from the internet: the router opens its port (UPnP) and says the public address, for another Phosphor
/// somewhere else to follow or to show the picture of. Needs the control password first (strangers could watch, never
/// control).
#[cfg(not(target_arch = "wasm32"))]
fn internet_row(ui: &mut egui::Ui, st: &mut State) {
    let locked = st.require_password && !st.remote_password.is_empty();
    let mut on = st.internet.is_some();
    crate::ui::toolbar(ui, |ui| {
        let r = ui.add_enabled_ui(locked || on, |ui| crate::ui::toggle(ui, &mut on, label(icon::GLOBE, "Reach from the internet"))).inner;
        if r.on_hover_text("Asks your router to let this Session in from outside, so a Phosphor somewhere else can follow this show or show its picture. Off again closes it").changed() {
            st.internet = on.then(|| {
                let mut ports = vec![st.net.port];
                if st.net.https {
                    ports.push(st.net.port + 1);
                }
                crate::internet::Opening::open(ports)
            });
        }
        help(ui, "For a Phosphor in another place: on that one, type the address below into Follow another Phosphor (it plays this show; its media must be there too) or press Show its picture (this output, live, as a layer: nothing else needed there).");
    });
    if !locked && !on {
        hint(ui, "Set a password to control (below) first: anyone could find an open Session.");
    }
    let port = st.net.port;
    let Some(open) = &mut st.internet else { return };
    open.poll();
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(500));
    match open.status.clone() {
        crate::internet::Status::Asking => {
            ui.horizontal(|ui| {
                ui.spinner();
                hint(ui, "asking the router…");
            });
        }
        crate::internet::Status::Open { public, .. } => {
            let addr = open.address(port).unwrap_or_default();
            crate::ui::field(ui, "From anywhere", |ui| {
                ui.monospace(&addr);
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy the address for the other Phosphor").clicked() {
                    ui.ctx().copy_text(addr.clone());
                }
            });
            if !crate::internet::is_public(public) {
                ui.colored_label(WARN, format!("{} Your router's own address ({public}) isn't a public one: it sits behind another router (often a phone or mobile connection), so this won't reach you from outside.", icon::WARNING));
            }
        }
        crate::internet::Status::Failed(why) => {
            let here = crate::control::local_ip().unwrap_or_default();
            ui.colored_label(WARN, format!("{} {why}. In your router's settings, forward TCP port {port} to this computer ({here}), then share your public address with :{port}.", icon::WARNING));
        }
    }
}

/// Audience play: a QR code for the room; people scan it and take turns at the chosen controls.
fn audience_play(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.input(|i| i.time);
    crate::ui::heading_help(ui, format!("{} Audience play", icon::GAME_CONTROLLER), "People scan the code, line up, and each gets the controls for a while: your Control Surface, clip columns, Surprise.");
    let play = &mut st.comp.play;
    crate::ui::toggle(ui, &mut play.enabled, "Let the audience play");
    if !play.enabled {
        return;
    }
    crate::ui::field(ui, "Each turn", |ui| {
        guarded!(ui, play.turn_secs, egui::DragValue::new(&mut play.turn_secs).range(10..=900).suffix(" s"));
    });
    crate::ui::field(ui, "Rest after", |ui| {
        guarded!(ui, play.rest_secs, egui::DragValue::new(&mut play.rest_secs).range(0..=600).suffix(" s")).on_hover_text("How long a phone sits out after its turn before it can play again. 0: with nobody waiting, a player keeps the controls.");
    });
    crate::ui::field(ui, "Players at once", |ui| {
        guarded!(ui, play.players, egui::DragValue::new(&mut play.players).range(1..=8)).on_hover_text("More than one: each player gets their own Surface page (page 1, 2, …) unless you pick one page for all");
    });
    // What the play page offers, in one row: a switch off takes it off the page and its presses are refused.
    // The game pad: presses are controls here (Learn, then press on the phone) and OSC to a 3D project.
    crate::ui::field(ui, "Guests get", |ui| {
        crate::ui::toggle(ui, &mut play.columns, "Clip columns").on_hover_text("Off: no clip buttons on the play page, and guests can't launch clips");
        crate::ui::toggle(ui, &mut play.surprise, "Surprise button").on_hover_text("Off: no \"Surprise me\" on the play page, and guests can't change every layer's look");
        crate::ui::toggle(ui, &mut play.pad.on, "Game pad").on_hover_text("Up / down / left / right, A and B on the play page, held like a real pad. Map a button to anything: right-click a control › Learn, then press it on the phone. Each press also goes out as OSC (/pad/1/up 1 or 0, /pad/1/stick x y) to fly a camera in Unreal or Unity");
    });
    if play.pad.on {
        crate::ui::field(ui, "Send pad to", |ui| {
            crate::ui::text_field(ui, &mut play.pad.host, "the 3D computer's address", Some(150.0)).on_hover_text("The computer running the Unreal / Unity project (127.0.0.1 = this one). Empty = nowhere");
            guarded!(ui, play.pad.port, egui::DragValue::new(&mut play.pad.port).range(1..=65535)).on_hover_text("The OSC port the project listens on (the kit's scripts use 9100)");
            #[cfg(not(target_arch = "wasm32"))]
            if crate::ui::command(ui, label(icon::EXPORT, "Kit for 3D artists…")).on_hover_text("A folder to send whoever built the 3D world: a Unity script, an Unreal component and a page of steps. Their project keeps its lighting and look; the script only moves the camera").clicked() {
                st.notice = Some(match crate::pad_kit::export(play.pad.port) {
                    Ok(dir) => {
                        crate::os::reveal(&dir);
                        format!("Kit saved: {}", dir.display())
                    }
                    Err(e) => format!("Couldn't save the kit: {e}"),
                });
            }
        });
    }
    crate::ui::toggle(ui, &mut play.show_names, "Show the players' names on the output").on_hover_text("A \"Now playing: …\" text layer along the bottom; guests type their name on the play page");
    crate::ui::field(ui, "Surface page", |ui| {
        let pages = &st.comp.surface.pages;
        let current = play.surface_page.and_then(|p| pages.get(p)).cloned().unwrap_or_else(|| "All pages".into());
        crate::ui::dropdown(ui, "play_page", 160.0).selected_text(current).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut play.surface_page, None, "All pages");
            for (i, name) in pages.iter().enumerate() {
                crate::ui::pick_value(ui, &mut play.surface_page, Some(i), name);
            }
        });
    });
    crate::ui::field(ui, "Title", |ui| {
        crate::ui::text_field(ui, &mut play.title, "", Some(160.0));
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
    help(ui, "Each control's guest settings (whether guests get it, how far they can push it) are in the Control Surface editor.");
    let style_changed = crate::ui::field(ui, "Code on the output", |ui| {
        choice_combo(ui, "qr_style", &mut play.qr_style, 190.0, false).changed() | crate::ui::toggle(ui, &mut play.qr_caption, "Words under it").changed()
    });
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
                if crate::ui::command(ui, icon::COPY).on_hover_text("Copy link (also what to write on an NFC tag: a tap opens the play page). The link holds this computer's Wi-Fi address: give it a fixed address on the venue's router, or the tags need writing again").clicked() {
                    ui.ctx().copy_text(url.clone());
                }
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

/// Where another Phosphor's Session hands out its output picture (`http://host:port/stream.mjpeg`), from what was typed.
#[cfg(not(target_arch = "wasm32"))]
fn leader_picture(typed: &str) -> String {
    let host = typed.trim().trim_start_matches("http://").trim_start_matches("https://").split('/').next().unwrap_or_default().to_string();
    let host = if host.contains(':') { host } else { format!("{host}:{}", crate::net::DEFAULT_PORT) };
    format!("http://{host}/stream.mjpeg")
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
            crate::ui::toolbar(ui, |ui| {
                if crate::ui::toggle(ui, &mut always, "Every time Phosphor starts (a player)")
                    .on_hover_text("This Mac becomes a player: it follows this leader at launch. With \"Start Phosphor when this Mac logs in\" and \"Open outputs on their screens\" (Output panel) it needs no one to set it up after a restart")
                    .changed()
                {
                    st.follow_at_launch = if always { addr } else { String::new() };
                }
                help(ui, "This Mac plays the leader's show on its own outputs and mapping. Your changes here are replaced by the leader's; its media must be at the same paths here.");
            });
        }
        None => {
            let mut addr: String = ui.ctx().data(|d| d.get_temp(key)).unwrap_or_default();
            ui.horizontal_wrapped(|ui| {
                let w = (ui.available_width() - 150.0).clamp(100.0, 260.0);
                crate::ui::text_field(ui, &mut addr, "leader's address (from its Session link)", Some(w));
                crate::ui::password_field(ui, &mut st.follow_password, "its password, if any", 130.0)
                    .on_hover_text("The leader's \"Password to control\" (Session panel), when it has one");
                if crate::ui::command_if(ui, !addr.trim().is_empty(), label(icon::LINK, "Follow")).clicked() {
                    st.following = Some(crate::follow::Link::connect(&addr, &st.follow_password));
                }
                if crate::ui::command_if(ui, !addr.trim().is_empty(), label(icon::MONITOR_PLAY, "Show its picture"))
                    .on_hover_text("A new layer with the other Phosphor's output, live (it needs nothing else here: no project, no media). Works across the internet when that one is set to Reach from the internet")
                    .clicked()
                {
                    let now = ui.input(|i| i.time);
                    let source = crate::media::live::stream_source(&leader_picture(&addr));
                    let l = st.comp.add_layer();
                    l.name = format!("From {}", addr.trim()).chars().take(32).collect();
                    let layer = l.id;
                    st.selected = Some(layer);
                    st.launch_source(layer, source, Vec::new(), addr.trim().to_string(), now);
                }
                help(ui, "On the leading Mac, start its Session below. Then type its address here (and its password to control, if it has one): both play the same show in sync, each on its own projectors.");
            });
            ui.ctx().data_mut(|d| d.insert_temp(key, addr));
        }
    }
    ui.separator();
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    #[test]
    fn the_picture_address_comes_from_what_was_typed() {
        assert_eq!(super::leader_picture("203.0.113.9:9000"), "http://203.0.113.9:9000/stream.mjpeg");
        assert_eq!(super::leader_picture(" http://studio.local/ "), format!("http://studio.local:{}/stream.mjpeg", crate::net::DEFAULT_PORT));
    }
}
