//! The Convert window (right-click a media file > Convert…): pick a format, how much to compress and a size, see
//! what it will take on disk, convert, and (by default) use the new file in place of the old one in this project.

use eframe::egui::{self, RichText};
use egui_phosphor::regular as icon;

use super::PhosphorApp;
use crate::convert::{Format, Quality, Size};
use crate::theme;

/// The window's state.
#[derive(Default)]
pub(crate) struct ConvertUi {
    pub open: bool,
    pub src: String,
    pub name: String,
    pub probe: Option<Result<crate::convert::Probe, String>>,
    pub format: Format,
    pub quality: Quality,
    pub size: Size,
    pub progress: Option<crate::convert::Shared>,
    /// Use the converted file in place of the original in this project when done.
    pub swap: bool,
    swapped: bool,
}

impl ConvertUi {
    /// Opens the window for one file.
    pub fn open_for(&mut self, path: &str, name: &str) {
        *self = ConvertUi { open: true, src: path.to_string(), name: name.to_string(), probe: Some(crate::convert::probe(path)), swap: true, ..Default::default() };
    }
}

fn mb(bytes: u64) -> String {
    if bytes >= 1 << 30 { format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64) } else { format!("{:.0} MB", bytes as f64 / (1u64 << 20) as f64) }
}

impl PhosphorApp {
    pub(crate) fn convert_window(&mut self, ctx: &egui::Context) {
        let c = &mut self.state.convert;
        if !c.open {
            return;
        }
        let mut open = true;
        let mut done_output: Option<String> = None;
        egui::Window::new(format!("{} Convert", icon::FILM_STRIP)).open(&mut open).collapsible(false).resizable(false).default_width(440.0).show(ctx, |ui| {
            ui.label(RichText::new(&c.name).strong());
            let probe = match &c.probe {
                Some(Ok(p)) => p.clone(),
                Some(Err(e)) => {
                    ui.colored_label(theme::WARN, e);
                    return;
                }
                None => return,
            };
            crate::ui::hint(ui, format!("Now: {} · {}×{} · {:.0} fps · {:.1} s · {}", probe.codec, probe.width, probe.height, probe.fps, probe.seconds, mb(probe.bytes)));
            ui.add_space(6.0);
            let progress = c.progress.as_ref().map(|p| p.lock().unwrap().clone());
            let busy = progress.as_ref().is_some_and(|p| !p.finished);
            ui.add_enabled_ui(!busy, |ui| {
                ui.label(RichText::new("Format").strong());
                for f in Format::ALL {
                    ui.radio_value(&mut c.format, f, f.name());
                }
                crate::ui::hint(ui, c.format.about());
                if c.format.has_quality() {
                    ui.horizontal(|ui| {
                        for q in Quality::ALL {
                            ui.selectable_value(&mut c.quality, q, q.name());
                        }
                    });
                }
                ui.horizontal(|ui| {
                    for s in Size::ALL {
                        ui.selectable_value(&mut c.size, s, s.name());
                    }
                });
            });
            ui.add_space(6.0);
            let estimate = crate::convert::estimate(&probe, c.format, c.quality, c.size);
            let (w, h) = c.size.apply(probe.width, probe.height);
            ui.label(RichText::new(format!("About {} ({}×{})", mb(estimate), w, h)).strong().size(14.0));
            match crate::convert::free_space(&c.src) {
                Some(free) if free < estimate + (512 << 20) => {
                    ui.colored_label(theme::WARN, format!("Only {} free on that disk", mb(free)));
                }
                Some(free) => {
                    crate::ui::hint(ui, format!("{} free on that disk", mb(free)));
                }
                None => {}
            };
            if probe.has_audio && matches!(c.format, Format::Hap | Format::HapAlpha) {
                crate::ui::hint(ui, "Hap has no sound: the converted file will be silent.");
            }
            crate::ui::toggle(ui, &mut c.swap, "Use the converted file in this project (in place of the original)");
            ui.add_space(6.0);
            match progress {
                None => {
                    if ui.button(RichText::new(format!("{} Convert", icon::FILM_STRIP)).strong()).clicked() {
                        c.progress = Some(crate::convert::start(crate::convert::Job { src: c.src.clone(), format: c.format, quality: c.quality, size: c.size, probe: probe.clone() }));
                        c.swapped = false;
                    }
                    crate::ui::hint(ui, "The new file goes next to the original; the original stays as it is.");
                }
                Some(p) if !p.finished => {
                    ui.add(egui::ProgressBar::new(p.done).show_percentage());
                    ctx.request_repaint_after(std::time::Duration::from_millis(200));
                }
                Some(p) => match &p.error {
                    Some(e) => {
                        ui.colored_label(theme::WARN, e);
                        if ui.button("Try again").clicked() {
                            c.progress = None;
                        }
                    }
                    None => {
                        let size = std::fs::metadata(&p.output).map_or(0, |m| m.len());
                        ui.label(RichText::new(format!("{} Done: {}", icon::CHECK_CIRCLE, mb(size))).color(theme::accent()));
                        crate::ui::hint(ui, &p.output);
                        if !c.swapped {
                            c.swapped = true;
                            done_output = Some(p.output.clone());
                        }
                        ui.horizontal(|ui| {
                            if ui.button(crate::ui::label(icon::FOLDER_OPEN, "Show the file")).clicked() {
                                crate::os::reveal(&p.output);
                            }
                            if ui.button("Convert another way").clicked() {
                                c.progress = None;
                            }
                        });
                    }
                },
            }
        });
        if !open {
            self.state.convert.open = false;
        }
        // Add the new file to the media bin; use it in place of the original when asked.
        if let Some(out) = done_output {
            let st = &mut self.state;
            let name = std::path::Path::new(&out).file_name().map_or(out.clone(), |n| n.to_string_lossy().into_owned());
            st.comp.add_media(&name, &out);
            if st.convert.swap {
                let src = st.convert.src.clone();
                st.comp.map_paths(|p| if p == src { out.clone() } else { p.to_string() });
                // Keep what the original remembered (settings, collections).
                if let Some(old) = st.comp.media_bin.iter().find(|m| m.path == src).cloned()
                    && let Some(new) = st.comp.media_bin.iter_mut().find(|m| m.path == out)
                {
                    new.settings = old.settings;
                    new.muted = old.muted;
                    new.collections = old.collections;
                    new.favorite = old.favorite;
                }
                st.notice = Some(format!("Converted: this project now uses {name}"));
            } else {
                st.notice = Some(format!("Converted: {name} is in the media bin"));
            }
        }
    }
}
