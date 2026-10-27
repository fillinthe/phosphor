//! File info (right-click a media file › File info…): everything about one file in the bin as text you can select and
//! copy (what it is, where it is, what plays with it), and its own effects, each one copyable into any effects list.
//! Desktop only (it reads the file with FFmpeg).

use eframe::egui;
use egui_phosphor::regular as icon;
use phosphor_engine::{Composition, EffectState, MediaItem, shaders};

use crate::app::State;

type Probed = std::sync::Arc<std::sync::Mutex<Option<Result<crate::convert::Probe, String>>>>;

/// The open window: which file, and what reading it found (read on its own thread).
pub(crate) struct FileInfo {
    path: String,
    probe: Probed,
}

impl FileInfo {
    pub(crate) fn open(path: &str) -> Self {
        let probe: Probed = Default::default();
        let (p, out) = (path.to_string(), probe.clone());
        std::thread::spawn(move || {
            let found = crate::convert::probe(&p);
            *out.lock().unwrap() = Some(found);
        });
        FileInfo { path: path.to_string(), probe }
    }
}

fn size_text(bytes: u64) -> String {
    match bytes {
        b if b >= 1 << 30 => format!("{:.2} GB", b as f64 / (1u64 << 30) as f64),
        b if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        b => format!("{:.0} KB", b as f64 / 1024.0),
    }
}

fn length_text(s: f64) -> String {
    let m = (s / 60.0).floor();
    format!("{m:.0}:{:05.2}", s - m * 60.0)
}

/// One effect as a line: its name, then each setting ("Amount 0.50", menus by their choice).
pub(crate) fn effect_line(e: &EffectState) -> String {
    let Some(d) = shaders::effect(&e.effect) else { return e.effect.clone() };
    let values: Vec<String> = d
        .params
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let v = e.values.get(i).copied().unwrap_or(p.default);
            match p.labels.get((v - p.min).round().max(0.0) as usize).filter(|_| !p.labels.is_empty()) {
                Some(l) => format!("{} {l}", p.name),
                None => format!("{} {v:.2}", p.name),
            }
        })
        .collect();
    let off = if e.enabled { "" } else { " (off)" };
    let mix = if e.mix < 1.0 { format!(", mix {:.0}%", e.mix * 100.0) } else { String::new() };
    if values.is_empty() { format!("{}{off}{mix}", d.name) } else { format!("{}{off}: {}{mix}", d.name, values.join(", ")) }
}

/// How many layers and clips play this file.
fn uses(comp: &Composition, path: &str) -> usize {
    comp.layers
        .iter()
        .map(|l| usize::from(phosphor_engine::file_of(&l.source) == Some(path)) + l.clips.iter().filter(|k| phosphor_engine::file_of(&k.source) == Some(path)).count())
        .sum()
}

/// Everything about the file as plain text.
pub(crate) fn describe(item: &MediaItem, probe: Option<&Result<crate::convert::Probe, String>>, used: usize) -> String {
    let mut t = vec![format!("Name: {}", item.name), format!("File: {}", item.path)];
    match probe {
        None => t.push("Reading the file…".into()),
        Some(Err(e)) => t.push(format!("Couldn't read it: {e}")),
        Some(Ok(p)) => {
            t.push(format!("Size on disk: {}", size_text(p.bytes)));
            t.push(format!("Picture: {}×{} · {}", p.width, p.height, p.codec));
            if p.seconds > 0.0 {
                t.push(format!("Length: {} · {:.2} fps", length_text(p.seconds), p.fps));
            }
            t.push(format!("Sound: {}", if p.has_audio { if item.muted { "yes (muted)" } else { "yes" } } else { "none" }));
        }
    }
    if !item.folder.is_empty() {
        t.push(format!("From watched folder: {}", item.folder));
    }
    if item.favorite {
        t.push("A favourite".into());
    }
    if !item.collections.is_empty() {
        t.push(format!("Collections: {}", item.collections.join(", ")));
    }
    t.push(match used {
        0 => "Not used by any layer or clip".into(),
        1 => "Used by 1 layer or clip".into(),
        n => format!("Used by {n} layers and clips"),
    });
    for (_, words) in super::media_extras(item).into_iter().filter(|(i, _)| *i != icon::MAGIC_WAND) {
        t.push(words);
    }
    if let Some(c) = &item.credit {
        t.push(format!("Credit: {}", c.line()));
    }
    if !item.effects.is_empty() {
        t.push(String::new());
        t.push("Its own effects (in order):".into());
        t.extend(item.effects.iter().enumerate().map(|(i, e)| format!("{}. {}", i + 1, effect_line(e))));
    }
    t.join("\n")
}

/// The File info window (drawn app-level while open).
pub(crate) fn file_info_window(ctx: &egui::Context, st: &mut State) {
    let Some(info) = &st.file_info else { return };
    let Some(item) = st.comp.media_bin.iter().find(|m| m.path == info.path).cloned() else {
        st.file_info = None;
        return;
    };
    let probe = info.probe.lock().unwrap().clone();
    if probe.is_none() {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
    let text = describe(&item, probe.as_ref(), uses(&st.comp, &item.path));
    let mut open = true;
    let mut copy: Option<Vec<EffectState>> = None;
    egui::Window::new(format!("{} File info", icon::INFO)).id(egui::Id::new("file_info")).open(&mut open).default_width(420.0).resizable(true).collapsible(false).show(ctx, |ui| {
        crate::ui::heading(ui, &item.name);
        crate::ui::toolbar(ui, |ui| {
            if crate::ui::command(ui, crate::ui::label(icon::COPY, "Copy all text")).clicked() {
                ui.ctx().copy_text(text.clone());
                st.notice = Some("File info copied".into());
            }
            #[cfg(target_os = "macos")]
            if crate::ui::command(ui, crate::ui::label(icon::FOLDER_OPEN, "Reveal in Finder")).clicked() {
                crate::os::reveal(&item.path);
            }
        });
        crate::ui::column_scroll(ui).max_height(420.0).show(ui, |ui| {
            // Read-only, but selectable: drag over any part to copy it.
            let mut shown = text.as_str();
            ui.add(egui::TextEdit::multiline(&mut shown).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            if !item.effects.is_empty() {
                ui.separator();
                crate::ui::toolbar(ui, |ui| {
                    crate::ui::heading(ui, "Its effects");
                    if crate::ui::command(ui, crate::ui::label(icon::COPY, "Copy all")).on_hover_text("Then Paste in any effects list's menu (a layer, a clip, a file, MASTER)").clicked() {
                        copy = Some(item.effects.clone());
                    }
                });
                for e in &item.effects {
                    crate::ui::toolbar(ui, |ui| {
                        if crate::ui::small_command(ui, icon::COPY).on_hover_text("Copy this effect (with its settings), then Paste in any effects list's menu").clicked() {
                            copy = Some(vec![e.clone()]);
                        }
                        crate::ui::hint(ui, effect_line(e));
                    });
                }
            }
        });
    });
    if let Some(fx) = copy {
        st.notice = Some(if fx.len() == 1 { "Effect copied: Paste it in any effects list's menu".into() } else { format!("{} effects copied: Paste them in any effects list's menu", fx.len()) });
        st.fx_clipboard = fx;
    }
    if !open {
        st.file_info = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_info_reads_like_a_list() {
        let mut item: MediaItem = serde_json::from_str(r#"{"name":"a.mov","path":"/x/a.mov"}"#).unwrap();
        let invert = shaders::all_effects().first().expect("an effect").id;
        item.effects.push(EffectState { id: 1, effect: invert.into(), enabled: true, values: Vec::new(), mix: 1.0, mods: Vec::new(), blend: Default::default() });
        let probe = Ok(crate::convert::Probe { width: 1920, height: 1080, fps: 30.0, seconds: 75.5, bytes: 3 << 20, codec: "hap".into(), has_audio: false });
        let t = describe(&item, Some(&probe), 2);
        for want in ["Name: a.mov", "File: /x/a.mov", "1920×1080 · hap", "Length: 1:15.50", "Size on disk: 3.0 MB", "Used by 2 layers", "Its own effects", "1. "] {
            assert!(t.contains(want), "{want} missing from:\n{t}");
        }
        assert!(describe(&item, None, 0).contains("Reading the file"));
    }
}
