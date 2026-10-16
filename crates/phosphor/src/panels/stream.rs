//! Streaming and recording.

use super::*;

pub(crate) fn stream(ui: &mut egui::Ui, st: &mut State) {
    use crate::stream::Destination;
    let now = ui.ctx().input(|i| i.time);
    let (cw, ch) = (st.comp.width, st.comp.height);
    let mut choose_file = false;
    // A finished recording goes into the media bin, ready to play.
    let mut recorded: Option<String> = None;
    let State { stream, notice, stream_audio_devices, .. } = st;
    let running = stream.running();
    ui.add_enabled_ui(!running, |ui| {
        let s = &mut stream.settings;
        ui.horizontal(|ui| {
            ui.label("Destination");
            ComboBox::from_id_salt("stream_dest").width(160.0).selected_text(s.destination.name()).show_ui(ui, |ui| {
                for d in Destination::ALL {
                    ui.selectable_value(&mut s.destination, d, d.name());
                }
            });
        });
        match s.destination {
            Destination::YouTube | Destination::Twitch => {
                ui.horizontal(|ui| {
                    ui.label("Stream key");
                    ui.add(egui::TextEdit::singleline(&mut s.stream_key).password(true).desired_width(260.0));
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
            Destination::File => {
                ui.horizontal(|ui| {
                    ui.label("File");
                    ui.add(egui::TextEdit::singleline(&mut s.file_path).hint_text("…/recording.mp4 or .mov (ProRes)").desired_width(260.0));
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button("Choose…").clicked() {
                        choose_file = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Format");
                    choice_combo(ui, "record_format", &mut s.file_format, 200.0, false);
                    if s.file_format != crate::stream::FileFormat::Auto {
                        hint(ui, "Hap is encoded by Phosphor at the full canvas size; the movie plays back with almost no CPU in Phosphor, VDMX, Resolume and MadMapper.");
                    }
                });
            }
        }
        ui.horizontal(|ui| {
            ui.label("Size");
            let current = if s.height == 0 { format!("Canvas ({cw}×{ch})") } else { format!("{}p", s.height) };
            ComboBox::from_id_salt("stream_size").selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.height, 0, format!("Canvas ({cw}×{ch})"));
                for h in [720, 1080, 1440, 2160] {
                    ui.selectable_value(&mut s.height, h, format!("{h}p"));
                }
            });
            ui.label("fps");
            ComboBox::from_id_salt("stream_fps").width(60.0).selected_text(s.fps.to_string()).show_ui(ui, |ui| {
                for f in [24, 25, 30, 50, 60] {
                    ui.selectable_value(&mut s.fps, f, f.to_string());
                }
            });
        });
        guarded!(ui, s.kbps, Slider::new(&mut s.kbps, 1000..=30000).text("Bitrate (kbps)").logarithmic(true));
        #[cfg(not(target_arch = "wasm32"))]
        ui.horizontal(|ui| {
            ui.label("Audio");
            let current = if s.audio_device < 0 {
                "None".to_string()
            } else {
                stream_audio_devices.iter().find(|(i, _)| *i == s.audio_device).map(|(_, n)| n.clone()).unwrap_or_else(|| format!("Device {}", s.audio_device))
            };
            ComboBox::from_id_salt("stream_audio").width(220.0).selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut s.audio_device, -1, "None");
                for (i, name) in stream_audio_devices.iter() {
                    ui.selectable_value(&mut s.audio_device, *i, name);
                }
            });
            if ui.button(icon::ARROWS_CLOCKWISE).on_hover_text("Scan audio devices (mic, interface, BlackHole loopback)").clicked() {
                *stream_audio_devices = crate::stream::list_audio_devices();
            }
        });
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if running {
            if ui.button(label(icon::STOP, "Stop")).clicked() {
                stream.stop();
                if stream.settings.destination == crate::stream::Destination::File {
                    recorded = Some(stream.settings.recorded_path());
                }
            }
            ui.colored_label(ACCENT, format!("{} LIVE {:02}:{:02}", icon::RECORD, (stream.elapsed(now) / 60.0) as u32, stream.elapsed(now) as u32 % 60));
        } else if ui.button(label(icon::BROADCAST, "Start")).clicked() {
            match stream.start(cw, ch, now) {
                Ok(()) => *notice = None,
                Err(e) => *notice = Some(e),
            }
        }
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
    hint(ui, "Hardware H.264 via VideoToolbox. For the DJ's music in the stream, pick a loopback device (BlackHole) or your interface as Audio.");
    #[cfg(not(target_arch = "wasm32"))]
    if choose_file {
        st.dialogs.save_file(crate::dialogs::Purpose::StreamFile, ("Video", &["mp4", "mov"]), "Phosphor recording.mp4");
    }
    #[cfg(target_arch = "wasm32")]
    let _ = choose_file;
    if let Some(path) = recorded {
        let name = std::path::Path::new(&path).file_name().map_or(path.clone(), |n| n.to_string_lossy().to_string());
        st.comp.add_media(&name, &path);
        st.notice = Some(format!("Recording saved: {path} (in the media bin)"));
    }
}
