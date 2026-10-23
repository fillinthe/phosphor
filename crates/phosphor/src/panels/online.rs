//! Online (in the Library): search free libraries of videos and pictures (`crate::online`) and bring them into the
//! show. Click a result to download it into the media bin (with its credit); once it's here, click puts it on the
//! selected layer and it drags onto layers and clip cells like any file. The Credits list collects every credit.

use crate::online::{Download, Fetch, Item, Kind, Search, Source};

use std::collections::HashMap;

use super::*;

/// What to search for.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Want {
    #[default]
    Both,
    Videos,
    Pictures,
}

impl Want {
    fn kinds(self) -> &'static [Kind] {
        match self {
            Want::Both => &[Kind::Video, Kind::Picture],
            Want::Videos => &[Kind::Video],
            Want::Pictures => &[Kind::Picture],
        }
    }
}

/// Where a file goes once it's downloaded.
#[derive(Clone, Copy, PartialEq)]
enum Then {
    /// Just the media bin.
    Bin,
    Layer(u64),
    NewLayer,
}

/// The panel's state (in `State::online`).
#[derive(Default)]
pub struct Online {
    query: String,
    want: Want,
    /// Only results that may be used in paid work.
    commercial: bool,
    /// Libraries switched off.
    off: Vec<Source>,
    search: Option<Search>,
    downloads: Vec<(Download, Then)>,
    /// Files downloaded this session, by item.
    here: HashMap<(Source, String), String>,
    /// Bench: put the first picture found on a new layer.
    bench: bool,
}

impl Online {
    /// Bench `--quick online` (PHOSPHOR_ONLINE_QUERY): searches like the panel does.
    pub fn bench_search(&mut self, query: &str) {
        self.query = query.to_string();
        self.search = Some(Search::start(query, &Source::ALL, Want::Both.kinds(), false, None));
        self.bench = true;
    }

    /// Bench ONLINE line: results per library and errors so far.
    pub fn bench_line(&mut self) -> Option<String> {
        let s = self.search.as_mut()?;
        s.poll();
        let counts: Vec<String> = Source::ALL.iter().map(|src| format!("{}={}", src.name().replace(' ', "_"), s.items.iter().filter(|i| i.source == *src).count())).collect();
        let downloads: Vec<String> = self.downloads.iter().map(|(d, _)| format!("{:?}", d.status())).collect();
        Some(format!("ONLINE {} busy={} errors={:?} downloading={downloads:?} downloaded={}", counts.join(" "), s.busy(), s.errors, self.here.len()))
    }
}

/// Finishes downloads (into the media bin, onto a layer); call every frame so they land wherever you are.
pub(crate) fn online_poll(ctx: &egui::Context, st: &mut State) {
    if st.online.bench
        && let Some(s) = st.online.search.as_mut()
    {
        s.poll();
        // PHOSPHOR_ONLINE_PICK=video: the first video from the Internet Archive (small files) instead.
        let video = std::env::var("PHOSPHOR_ONLINE_PICK").is_ok_and(|k| k == "video");
        if let Some(item) = s.items.iter().find(|i| if video { i.source == Source::Archive } else { i.kind == Kind::Picture }).cloned() {
            st.online.bench = false;
            st.online.downloads.push((Download::start(item, None), Then::NewLayer));
        }
    }
    if st.online.downloads.is_empty() {
        return;
    }
    let now = ctx.input(|i| i.time);
    let mut finished = Vec::new();
    st.online.downloads.retain(|(d, then)| match d.status() {
        Fetch::Working { .. } => true,
        Fetch::Done(path) => {
            finished.push((d.item.clone(), path, *then));
            false
        }
        Fetch::Failed(e) => {
            st.notice = Some(format!("Couldn't download \"{}\": {e}", d.item.title));
            false
        }
    });
    for (item, path, then) in finished {
        let name = crate::media::clip_name(&path);
        st.comp.add_media(&name, &path);
        if let Some(m) = st.comp.media_bin.iter_mut().find(|m| m.path == path) {
            m.credit = Some(item.credit.clone());
        }
        st.online.here.insert(item.key(), path.clone());
        let layer = match then {
            Then::Bin => None,
            Then::Layer(id) => st.comp.layers.iter().any(|l| l.id == id).then_some(id),
            Then::NewLayer => Some(st.add_layer()),
        };
        if let Some(layer) = layer {
            st.selected = Some(layer);
            st.launch_source(layer, crate::media::spec_for_path(&path), Vec::new(), name, now);
        }
    }
}

pub(crate) fn online_panel(ui: &mut egui::Ui, st: &mut State) {
    let o = &mut st.online;
    let mut go = false;
    ui.horizontal(|ui| {
        let r = crate::ui::text_field(ui, &mut o.query, "Search: aurora, city lights, waves, fire…", Some((ui.available_width() - 34.0).max(80.0)));
        go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        go |= ui.add_enabled(!o.query.trim().is_empty(), egui::Button::new(icon::MAGNIFYING_GLASS)).on_hover_text("Search every library that's on").clicked();
    });
    ui.horizontal_wrapped(|ui| {
        let opts = [(Want::Both, "Both".to_string()), (Want::Videos, "Videos".to_string()), (Want::Pictures, "Pictures".to_string())];
        go |= segmented(ui, &mut o.want, &opts, |_, _, _| {}) && o.search.is_some();
        go |= toggle(ui, &mut o.commercial, "Paid work OK").on_hover_text("Only results whose licence allows use in paid work (no \"NC\" licences)").changed() && o.search.is_some();
        ui.menu_button(label(icon::BOOKS, "Libraries"), |ui| {
            for s in Source::ALL {
                let mut on = !o.off.contains(&s);
                if toggle(ui, &mut on, s.name()).on_hover_text(s.about()).changed() {
                    o.off.retain(|x| *x != s);
                    if !on {
                        o.off.push(s);
                    }
                }
            }
        });
    });
    if go && !o.query.trim().is_empty() {
        let sources: Vec<Source> = Source::ALL.into_iter().filter(|s| !o.off.contains(s)).collect();
        o.search = Some(Search::start(&o.query, &sources, o.want.kinds(), o.commercial, Some(ui.ctx().clone())));
    }
    let selected = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
    let o = &mut st.online;
    let Some(search) = o.search.as_mut() else {
        hint(ui, "Free videos and pictures to start a show with: NASA, Wikimedia Commons, the Internet Archive's old films, Openverse's Creative Commons photos and the Art Institute of Chicago. Downloads go into the media bin with their credit (Credits, below).");
        credits(ui, &st.comp);
        return;
    };
    search.poll();
    ui.horizontal(|ui| {
        if search.busy() {
            ui.spinner();
        }
        let n = search.items.len();
        hint(ui, if n == 0 && search.busy() { "Searching…".to_string() } else { format!("{n} found · click: download · right-click: more") });
    });
    for e in &search.errors {
        ui.colored_label(WARN, e.as_str());
    }
    let mut picked: Option<(Item, Then)> = None;
    let busy_keys: Vec<((Source, String), f32)> = o
        .downloads
        .iter()
        .map(|(d, _)| {
            let part = match d.status() {
                Fetch::Working { done, total: Some(t) } if t > 0 => done as f32 / t as f32,
                _ => 0.0,
            };
            (d.item.key(), part)
        })
        .collect();
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        let w = 150.0f32.min(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
            for item in &search.items {
                let key = item.key();
                let local = o.here.get(&key).cloned();
                let progress = busy_keys.iter().find(|(k, _)| *k == key).map(|(_, p)| *p);
                if let Some(then) = result_tile(ui, item, w, &mut st.thumbs, local.as_deref(), progress, selected) {
                    picked = Some((item.clone(), then));
                }
            }
        });
        if search.has_more() && ui.button(label(icon::ARROW_DOWN, "More")).clicked() {
            search.more();
        }
        ui.add_space(8.0);
        credits(ui, &st.comp);
    });
    if let Some((item, then)) = picked {
        let now = ui.ctx().input(|i| i.time);
        match o.here.get(&item.key()).cloned() {
            // Already here: straight onto the layer.
            Some(path) => {
                let layer = match then {
                    Then::Layer(id) => Some(id),
                    Then::NewLayer => None,
                    Then::Bin => return,
                };
                let layer = layer.unwrap_or_else(|| st.add_layer());
                st.selected = Some(layer);
                st.launch_source(layer, crate::media::spec_for_path(&path), Vec::new(), crate::media::clip_name(&path), now);
            }
            None if !o.downloads.iter().any(|(d, _)| d.item.key() == item.key()) => o.downloads.push((Download::start(item, Some(ui.ctx().clone())), then)),
            None => {}
        }
    }
}

/// One result: its picture, kind and licence; a bar while it downloads, a tick once it's here. Returns what was asked.
fn result_tile(ui: &mut egui::Ui, item: &Item, w: f32, thumbs: &mut crate::thumbs::Thumbs, local: Option<&str>, progress: Option<f32>, selected: Option<u64>) -> Option<Then> {
    let h = w * 9.0 / 16.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, h + 18.0), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return None;
    }
    let thumb_rect = egui::Rect::from_min_size(rect.min, egui::vec2(w, h));
    let p = ui.painter();
    p.rect_filled(thumb_rect, 4.0, Color32::from_gray(14));
    match (item.thumb.starts_with("https://")).then(|| thumbs.get(&item.thumb)).flatten() {
        Some(tex) => paint_texture(p, tex, thumb_rect),
        None => {
            let text = if thumbs.is_pending(&item.thumb) { "…" } else if item.kind == Kind::Video { icon::FILM_STRIP } else { icon::IMAGE };
            p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(22.0), Color32::GRAY);
        }
    }
    let stroke = if local.is_some() { Stroke::new(2.0, accent()) } else if resp.hovered() { Stroke::new(1.5, accent()) } else { Stroke::new(1.0, Color32::from_gray(50)) };
    p.rect_stroke(thumb_rect, 4.0, stroke, StrokeKind::Inside);
    // Kind, and a tick once it's in the media bin.
    let corner = thumb_rect.left_top() + egui::vec2(4.0, 3.0);
    let badge = if local.is_some() { format!("{} {}", icon::CHECK, if item.kind == Kind::Video { icon::FILM_STRIP } else { icon::IMAGE }) } else if item.kind == Kind::Video { icon::FILM_STRIP.to_string() } else { String::new() };
    p.text(corner, egui::Align2::LEFT_TOP, badge, egui::FontId::proportional(12.0), Color32::from_white_alpha(220));
    if !item.credit.commercial() {
        p.text(thumb_rect.right_top() + egui::vec2(-4.0, 3.0), egui::Align2::RIGHT_TOP, "NC", egui::FontId::proportional(10.0), WARN);
    }
    if let Some(part) = progress {
        let bar = egui::Rect::from_min_size(thumb_rect.left_bottom() + egui::vec2(0.0, -4.0), egui::vec2(w * part.max(0.04), 4.0));
        p.rect_filled(bar, 0.0, accent());
    }
    let max_chars = ((w / 6.5) as usize).max(8);
    let mut name = item.title.clone();
    if name.chars().count() > max_chars {
        name = format!("{}…", name.chars().take(max_chars - 1).collect::<String>());
    }
    p.text(egui::pos2(rect.left() + 2.0, rect.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, name, egui::FontId::proportional(11.0), Color32::LIGHT_GRAY);
    if let Some(path) = local {
        resp.dnd_set_drag_payload(SourceDrag { name: crate::media::clip_name(path), source: crate::media::spec_for_path(path) });
    }
    let size = item.size.map_or(String::new(), |s| format!("\n{:.1} MB", s as f64 / 1e6));
    let doing = match (local, progress) {
        (Some(_), _) => "In the media bin · click: on the selected layer · drag onto a layer or clip cell",
        (None, Some(_)) => "Downloading…",
        (None, None) => "Click: download into the media bin · right-click: put it on a layer",
    };
    let c = &item.credit;
    let by = if c.author.is_empty() { String::new() } else { format!("\nBy {}", c.author) };
    let mut out = None;
    if resp.clicked() {
        out = Some(match (local, selected) {
            (Some(_), Some(id)) => Then::Layer(id),
            (Some(_), None) => Then::NewLayer,
            (None, _) => Then::Bin,
        });
    }
    resp.on_hover_text(format!("{}{by}\n{} · {}{size}\n{doing}", item.title, c.licence, c.source)).right_menu(|ui| {
        if local.is_none() && ui.button(label(icon::DOWNLOAD_SIMPLE, "Download into the media bin")).clicked() {
            out = Some(Then::Bin);
            ui.close();
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new(label(icon::ARROW_RIGHT, "On the selected layer"))).on_disabled_hover_text("Select a layer first").clicked() {
            out = selected.map(Then::Layer);
            ui.close();
        }
        if ui.button(label(icon::PLUS, "On a new layer")).clicked() {
            out = Some(Then::NewLayer);
            ui.close();
        }
        ui.separator();
        if !c.page.is_empty() && ui.button(label(icon::GLOBE, &format!("Open its page ({})", c.source))).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(&c.page));
            ui.close();
        }
        if ui.button(label(icon::COPY, "Copy the credit")).clicked() {
            ui.ctx().copy_text(c.line());
            ui.close();
        }
    });
    out
}

/// Every credit in the show, to paste into a programme or a video's description.
fn credits(ui: &mut egui::Ui, comp: &phosphor_engine::Composition) {
    let n = comp.media_bin.iter().filter(|m| m.credit.is_some()).count();
    egui::CollapsingHeader::new(format!("{} Credits ({n})", icon::IDENTIFICATION_CARD)).id_salt("online_credits").show(ui, |ui| {
        if n == 0 {
            hint(ui, "Files downloaded here keep who made them and their licence. Most need a credit when you show them in public: copy them all from here.");
            return;
        }
        if ui.button(label(icon::COPY, "Copy all credits")).clicked() {
            ui.ctx().copy_text(crate::online::credits_text(comp));
        }
        for m in comp.media_bin.iter().filter(|m| m.credit.is_some()) {
            let c = m.credit.as_ref().expect("filtered");
            let line = ui.add(egui::Label::new(format!("{} {}", if c.commercial() { icon::CHECK_CIRCLE } else { icon::WARNING }, c.line())).truncate());
            line.on_hover_text(if c.commercial() { "May be used in paid work (credit as shown)" } else { "Non-commercial licence: not for paid work" });
        }
    });
}
