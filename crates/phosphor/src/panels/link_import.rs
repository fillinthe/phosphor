//! "From a link…" (Media panel): the video or GIF in a post on X, Instagram, TikTok, YouTube… into Phosphor
//! (`crate::web_media`): downloaded into the media bin, or played live from the link.

use crate::web_media::{BROWSERS, Job, Status};

use super::*;

/// The window's state (in `State::link_import`).
#[derive(Default)]
pub struct LinkImport {
    pub open: bool,
    url: String,
    /// Use this browser's sign-in (`BROWSERS`), for Instagram and private posts.
    browser: Option<usize>,
    /// Put what comes in on the selected layer too.
    onto_layer: bool,
    job: Option<Job>,
    /// The last job was "play from the link".
    live: bool,
}

/// The window, while open; also finishes a job that's running (call every frame).
pub(crate) fn link_window(ctx: &egui::Context, st: &mut State) {
    let now = ctx.input(|i| i.time);
    // A finished job: files into the media bin (and onto the layer), or the live link onto the layer.
    if let Some(job) = st.link_import.job.as_mut()
        && job.poll()
    {
        let target = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id)).or_else(|| st.comp.layers.last().map(|l| l.id));
        match job.status.clone() {
            Status::Downloaded(files) => {
                for f in &files {
                    let name = std::path::Path::new(f).file_name().map_or_else(|| f.clone(), |n| n.to_string_lossy().to_string());
                    st.comp.add_media(&name, f);
                }
                if st.link_import.onto_layer
                    && let (Some(layer), Some(f)) = (target, files.first())
                {
                    let name = std::path::Path::new(f).file_stem().map_or_else(|| f.clone(), |n| n.to_string_lossy().to_string());
                    st.launch_source(layer, crate::media::spec_for_path(f), Vec::new(), name, now);
                }
                st.notice = Some(format!("Downloaded {} video{} into the media bin", files.len(), if files.len() == 1 { "" } else { "s" }));
            }
            Status::Playable { url, name } => {
                if let Some(layer) = target {
                    let mut spec = crate::media::live::stream_source(&url);
                    if let SourceSpec::Live { name: n, .. } = &mut spec {
                        *n = format!("{name} (from the link)");
                    }
                    st.launch_source(layer, spec, Vec::new(), name, now);
                }
            }
            _ => {}
        }
    }
    let li = &mut st.link_import;
    if !li.open {
        return;
    }
    let mut open = true;
    let have_tool = crate::web_media::find_ytdlp().is_some();
    egui::Window::new(label(icon::LINK, "From a link")).open(&mut open).default_width(460.0).show(ctx, |ui| {
        hint(ui, "Paste the link to a post on X, Instagram, TikTok, YouTube, Vimeo… Its video or GIF comes into Phosphor. Only use clips you have the right to show.");
        crate::ui::text_field(ui, &mut li.url, "https://x.com/…/status/…", None);
        ui.horizontal(|ui| {
            let current = li.browser.map_or("No sign-in".to_string(), |i| format!("Sign-in from {}", BROWSERS[i]));
            ComboBox::from_id_salt("link_browser").width(170.0).selected_text(current).show_ui(ui, |ui| {
                ui.selectable_value(&mut li.browser, None, "No sign-in");
                for (i, b) in BROWSERS.iter().enumerate() {
                    ui.selectable_value(&mut li.browser, Some(i), format!("Sign-in from {b}"));
                }
            })
            .response
            .on_hover_text("Instagram and private posts need you signed in: yt-dlp then uses that browser's sign-in (only for this link)");
            toggle(ui, &mut li.onto_layer, "Onto the selected layer");
        });
        let busy = li.job.as_ref().is_some_and(|j| matches!(j.status, Status::Working(_)));
        let ready = have_tool && !busy && li.url.trim().starts_with("http");
        ui.horizontal(|ui| {
            let browser = li.browser.map(|i| BROWSERS[i]);
            if ui.add_enabled(ready, egui::Button::new(label(icon::DOWNLOAD_SIMPLE, "Download"))).on_hover_text("Into Phosphor's downloads folder and the media bin: a clip that loops and works offline").clicked() {
                li.job = Some(Job::download(&li.url, browser));
                li.live = false;
            }
            if ui.add_enabled(ready, egui::Button::new(label(icon::BROADCAST, "Play from the link"))).on_hover_text("Plays it live on the selected layer without saving it (the site's link stops working after a while)").clicked() {
                li.job = Some(Job::stream(&li.url, browser));
                li.live = true;
            }
            if busy {
                ui.spinner();
            }
        });
        match li.job.as_ref().map(|j| &j.status) {
            Some(Status::Working(s)) => hint(ui, s.as_str()),
            Some(Status::Downloaded(f)) => ui.colored_label(accent(), format!("{} Downloaded: {}", icon::CHECK, f.iter().filter_map(|p| std::path::Path::new(p).file_name()).map(|n| n.to_string_lossy().to_string()).collect::<Vec<_>>().join(", "))),
            Some(Status::Playable { name, .. }) => ui.colored_label(accent(), format!("{} Playing {name} from the link", icon::CHECK)),
            Some(Status::Failed(e)) => ui.colored_label(WARN, e.as_str()),
            None => ui.label(""),
        };
        if !have_tool {
            ui.colored_label(WARN, "Needs yt-dlp (free): in Terminal, brew install yt-dlp, or get it from github.com/yt-dlp/yt-dlp (put it in Phosphor's bin folder).");
        }
        if let Some(d) = crate::web_media::downloads_dir()
            && ui.small_button(label(icon::FOLDER_OPEN, "Downloads folder")).clicked()
        {
            let _ = std::fs::create_dir_all(&d);
            crate::os::open(&d);
        }
    });
    if !open {
        li.open = false;
    }
}
