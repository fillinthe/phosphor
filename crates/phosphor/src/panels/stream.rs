//! Streaming and recording.

use super::*;

pub(crate) fn stream(ui: &mut egui::Ui, st: &mut State) {
    use crate::stream::Destination;
    let now = ui.ctx().input(|i| i.time);
    let (cw, ch) = (st.comp.width, st.comp.height);
    let mut choose_file = false;
    let mut toggle_now = false;
    // What can be recorded on its own: layers, groups and comps.
    let sources: Vec<(u64, String)> = st
        .comp
        .layers
        .iter()
        .map(|l| (l.id, l.name.clone()))
        .chain(st.comp.groups.iter().map(|g| (g.id, format!("Group: {}", g.name))))
        .chain(st.comp.comps.iter().filter(|_| st.comp.comps.len() > 1).map(|c| (c.id, format!("Comp: {}", c.name))))
        .collect();
    let waiting = st.stream_bar.is_some();
    let State { stream, notice, stream_audio_devices, .. } = st;
    let running = stream.running();
    ui.add_enabled_ui(!running, |ui| {
        let s = &mut stream.settings;
        crate::ui::toolbar(ui, |ui| {
            ui.label("Destination");
            crate::ui::dropdown(ui, "stream_dest", 160.0).selected_text(s.destination.name()).show_ui(ui, |ui| {
                for d in Destination::ALL {
                    crate::ui::pick_value(ui, &mut s.destination, d, d.name());
                }
            });
        });
        if !matches!(s.destination, Destination::File | Destination::Whip) {
            ui.horizontal(|ui| {
                crate::ui::toggle(ui, &mut s.second_enabled, "Also send to").on_hover_text("A second live destination from the same encode (YouTube + Twitch at once)");
                if s.second_enabled {
                    crate::ui::dropdown(ui, "stream_second", 130.0).selected_text(s.second_destination.name()).show_ui(ui, |ui| {
                        for d in [Destination::YouTube, Destination::Twitch, Destination::CustomRtmp] {
                            crate::ui::pick_value(ui, &mut s.second_destination, d, d.name());
                        }
                    });
                    if s.second_destination == Destination::CustomRtmp {
                        ui.add(egui::TextEdit::singleline(&mut s.second_url).hint_text("rtmp://…").desired_width(160.0));
                    }
                    ui.add(egui::TextEdit::singleline(&mut s.second_key).password(true).hint_text("stream key").desired_width(160.0));
                }
            });
            ui.horizontal(|ui| {
                crate::ui::toggle(ui, &mut s.copy_enabled, "Also record a copy").on_hover_text("An H.264 .mp4 of the stream, written while it goes out");
                if s.copy_enabled {
                    ui.add(egui::TextEdit::singleline(&mut s.copy_path).hint_text("…/stream-copy.mp4").desired_width(240.0));
                }
            });
        }
        match s.destination {
            Destination::YouTube | Destination::Twitch => {
                ui.horizontal(|ui| {
                    ui.label("Stream key");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).desired_width(260.0));
                    crate::ui::toggle(ui, &mut s.secure, "Secure (RTMPS)").on_hover_text("Encrypted ingest (rtmps://, port 443): what YouTube and Twitch recommend; turn off only if a network blocks it");
                });
            }
            Destination::CustomRtmp => {
                ui.horizontal(|ui| {
                    ui.label("RTMP URL");
                    ui.add(egui::TextEdit::singleline(&mut s.rtmp_url).desired_width(260.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Stream key");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).desired_width(260.0));
                });
            }
            Destination::Srt => {
                ui.horizontal(|ui| {
                    ui.label("SRT URL");
                    ui.add(egui::TextEdit::singleline(&mut s.srt_url).desired_width(300.0));
                });
            }
            Destination::Whip => {
                ui.horizontal(|ui| {
                    ui.label("WHIP URL");
                    ui.add(egui::TextEdit::singleline(&mut s.whip_url).hint_text("https://…/whip").desired_width(300.0));
                });
                ui.horizontal(|ui| {
                    ui.label("Token");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).hint_text("bearer token, if the service gives one").desired_width(260.0));
                });
                help(ui, "WebRTC: under a second of delay. H.264 Baseline video, Opus audio. Goes out on its own (no second destination or copy).");
            }
            Destination::File => {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut s.file_path).hint_text("…/recording.mp4 or .mov (ProRes)").desired_width(260.0));
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui::command(ui, "Choose…").clicked() {
                        choose_file = true;
                    }
                });
                crate::ui::toolbar(ui, |ui| {
                    ui.label("Format");
                    choice_combo(ui, "record_format", &mut s.file_format, 200.0, false);
                    if s.file_format != crate::stream::FileFormat::Auto {
                        help(ui, "Hap is encoded by Phosphor at the full canvas size; the movie plays back with almost no CPU in Phosphor, VDMX, Resolume and MadMapper.");
                    }
                });
            }
        }
        crate::ui::toolbar(ui, |ui| {
            ui.label("Size");
            let current = if s.height == 0 { format!("Canvas ({cw}×{ch})") } else { format!("{}p", s.height) };
            ComboBox::from_id_salt("stream_size").selected_text(current).show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut s.height, 0, format!("Canvas ({cw}×{ch})"));
                for h in [720, 1080, 1440, 2160] {
                    crate::ui::pick_value(ui, &mut s.height, h, format!("{h}p"));
                }
            });
            ui.label("fps");
            crate::ui::dropdown(ui, "stream_fps", 60.0).selected_text(s.fps.to_string()).show_ui(ui, |ui| {
                for f in [24, 25, 30, 50, 60] {
                    crate::ui::pick_value(ui, &mut s.fps, f, f.to_string());
                }
            });
        });
        let mut kbps = s.kbps as f32;
        if ui.horizontal(|ui| Level::range(&mut kbps, 1000.0, 30000.0, "Bitrate (kbps)").default(6000.0).log().width(320.0).show(ui)).inner.changed() {
            s.kbps = kbps.round() as u32;
        }
        #[cfg(not(target_arch = "wasm32"))]
        crate::ui::toolbar(ui, |ui| {
            ui.label("Audio");
            let current = if s.audio_device < 0 {
                "None".to_string()
            } else {
                stream_audio_devices.iter().find(|(i, _)| *i == s.audio_device).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("Device {}", s.audio_device))
            };
            crate::ui::dropdown(ui, "stream_audio", 220.0).selected_text(current).show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut s.audio_device, -1, "None");
                for (i, name) in stream_audio_devices.iter() {
                    crate::ui::pick_value(ui, &mut s.audio_device, *i, name);
                }
            });
            if crate::ui::command(ui, icon::ARROWS_CLOCKWISE).on_hover_text("Scan audio devices (mic, interface, BlackHole loopback)").clicked() {
                *stream_audio_devices = crate::stream::list_audio_devices();
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        toggle(ui, &mut s.program_sound, label(icon::FILM_STRIP, "Movie sound")).on_hover_text("Phosphor's own sound (the movies playing) in the stream or recording, mixed with the audio device above when one is chosen");
    });
    ui.add_space(4.0);
    crate::ui::toolbar(ui, |ui| {
        if running {
            let (later, stop_now) = (label(icon::STOP, "Stopping on the bar…"), label(icon::STOP, "Stop"));
            if steady_button(ui, if waiting { later.clone() } else { stop_now.clone() }, &[&later, &stop_now]).clicked() {
                toggle_now = true;
            }
            ui.colored_label(accent(), format!("{} LIVE {:02}:{:02}", icon::RECORD, (stream.elapsed(now) / 60.0) as u32, stream.elapsed(now) as u32 % 60));
            stream_health(ui, stream);
        } else if crate::ui::command(ui, label(icon::BROADCAST, if waiting { "Starting on the bar…" } else { "Start" })).on_hover_text(if waiting { "Press again to cancel" } else { "Start now (or on the next bar, if that's on)" }).clicked() {
            toggle_now = true;
        }
        ui.add_enabled_ui(!running, |ui| {
            crate::ui::toggle(ui, &mut stream.settings.on_bar, "On the bar").on_hover_text("Start and stop on the next bar of the clock (4 beats)");
            let s = &mut stream.settings;
            let current = if s.source == 0 { "Program".to_string() } else { sources.iter().find(|(id, _)| *id == s.source).map_or("(gone)".into(), |x| x.1.clone()) };
            crate::ui::dropdown(ui, "stream_source", 150.0).selected_text(current).show_ui(ui, |ui| {
                crate::ui::pick_value(ui, &mut s.source, 0, "Program (what the output shows)");
                for (id, name) in &sources {
                    crate::ui::pick_value(ui, &mut s.source, *id, name);
                }
            })
            .response
            .on_hover_text("What's sent: the program, or one layer, group or comp on its own");
        });
    });
    let (status, error, sent, dropped, log) = stream.status();
    if !status.is_empty() {
        ui.colored_label(if error { WARN } else { Color32::GRAY }, status);
    }
    if running || sent > 0 {
        ui.monospace(format!("frames sent {sent} · dropped {dropped}"));
    }
    for line in log {
        ui.colored_label(WARN, line);
    }
    if let Some(msg) = notice {
        ui.colored_label(WARN, msg.as_str());
    }
    ui.add_space(6.0);
    help(ui, "Hardware H.264 via VideoToolbox. For the DJ's music in the stream, pick a loopback device (BlackHole) or your interface as Audio. Movie sound adds the sound of the movies playing in Phosphor (both mix together).");
    #[cfg(not(target_arch = "wasm32"))]
    if choose_file {
        st.dialogs.save_file(crate::dialogs::Purpose::StreamFile, ("Video", &["mp4", "mov"]), "Phosphor recording.mp4");
    }
    #[cfg(target_arch = "wasm32")]
    let _ = choose_file;
    if toggle_now {
        st.stream_toggle(now);
    }
    ui.add_space(8.0);
    more_recorders(ui, st, &sources, now);
}

/// More recorders: each records its own source (the program, a layer, group or comp) to its own file, all at once.
fn more_recorders(ui: &mut egui::Ui, st: &mut State, sources: &[(u64, String)], now: f64) {
    use crate::app::recorders::MAX;
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, "More recorders");
        if st.recorders.len() < MAX && crate::ui::command(ui, label(icon::PLUS, "Recorder")).on_hover_text("Record another picture to its own file at the same time (a layer on its own, a comp, the program)").clicked() {
            st.recorder_add();
        }
    });
    let (mut toggle, mut remove, mut choose) = (None, None, None);
    for (i, r) in st.recorders.iter_mut().enumerate() {
        let running = r.running();
        card_frame(ui, false).show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let s = &mut r.settings;
                ui.add_enabled_ui(!running, |ui| {
                    let current = if s.source == 0 { "Program".to_string() } else { sources.iter().find(|(id, _)| *id == s.source).map_or("(gone)".into(), |x| x.1.clone()) };
                    crate::ui::dropdown(ui, ("rec_source", i), 130.0).selected_text(current).show_ui(ui, |ui| {
                        crate::ui::pick_value(ui, &mut s.source, 0, "Program (the mix)");
                        for (id, name) in sources {
                            crate::ui::pick_value(ui, &mut s.source, *id, name);
                        }
                    });
                    crate::ui::text_field(ui, &mut s.file_path, "…/take.mp4 or .mov", Some(180.0));
                    #[cfg(not(target_arch = "wasm32"))]
                    if crate::ui::command(ui, "Choose…").clicked() {
                        choose = Some(i);
                    }
                    choice_combo(ui, ("rec_format", i), &mut s.file_format, 150.0, false);
                    crate::ui::dropdown(ui, ("rec_fps", i), 50.0).selected_text(format!("{} fps", s.fps)).show_ui(ui, |ui| {
                        for f in [24, 25, 30, 50, 60] {
                            crate::ui::pick_value(ui, &mut s.fps, f, f.to_string());
                        }
                    });
                });
                if running {
                    if action_button(ui, Act::Record, label(icon::STOP, "Stop")).clicked() {
                        toggle = Some(i);
                    }
                    ui.colored_label(accent(), format!("{} {:02}:{:02}", icon::RECORD, (r.elapsed(now) / 60.0) as u32, r.elapsed(now) as u32 % 60));
                } else {
                    if action_button(ui, Act::Record, label(icon::RECORD, "Record")).clicked() {
                        toggle = Some(i);
                    }
                    if crate::ui::small_command(ui, icon::TRASH).on_hover_text("Remove this recorder").clicked() {
                        remove = Some(i);
                    }
                }
            });
            let (status, error, ..) = r.status();
            if running && !status.is_empty() {
                ui.colored_label(if error { WARN } else { Color32::GRAY }, status);
            }
        });
    }
    if let Some(i) = toggle {
        st.recorder_toggle(i, now);
    }
    if let Some(i) = remove {
        st.recorders.remove(i);
    }
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(i) = choose {
        st.dialogs.save_file(crate::dialogs::Purpose::RecorderFile(i), ("Video", &["mp4", "mov"]), "Phosphor take.mp4");
    }
    #[cfg(target_arch = "wasm32")]
    let _: Option<usize> = choose;
}

/// Beside LIVE: the last two minutes (bitrate line, red where frames were dropped) and a short verdict.
fn stream_health(ui: &mut egui::Ui, stream: &crate::stream::Streamer) {
    use crate::stream::Health;
    let history = stream.history();
    if history.is_empty() {
        return;
    }
    let kbps: Vec<f32> = history.iter().map(|s| s.kbps).collect();
    let mut prev = history.front().map_or(0, |s| s.dropped);
    let marks: Vec<bool> = history
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let bad = s.dropped > prev || (i >= crate::stream::WARM_UP && s.speed < 0.95);
            prev = s.dropped;
            bad
        })
        .collect();
    let last = history.back().copied().unwrap_or_default();
    crate::ui::history_graph(ui, egui::vec2(160.0, 20.0), &kbps, &marks, crate::stream::HISTORY)
        .on_hover_text("Last two minutes: bitrate (line); red = frames dropped or falling behind real time");
    let (color, text) = match stream.health() {
        Health::Starting => (Color32::GRAY, "Measuring…".to_string()),
        Health::Good => (Color32::GRAY, format!("{} Healthy · {:.1} Mbit/s", icon::CHECK_CIRCLE, last.kbps / 1000.0)),
        Health::Struggling { dropped, speed } => (
            WARN,
            if dropped > 0 {
                format!("{} {dropped} frames dropped (10 s)", icon::WARNING)
            } else {
                format!("{} Falling behind ({speed:.2}x)", icon::WARNING)
            },
        ),
    };
    let r = ui.colored_label(color, text);
    if !matches!(stream.health(), Health::Good | Health::Starting) {
        r.on_hover_text("The connection or the computer can't keep up: lower the bitrate, frame rate or size, or check the network");
    }
}
