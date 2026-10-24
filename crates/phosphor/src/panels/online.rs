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
    Icons,
    Textures,
    Visualizers,
    Fonts,
    Music,
    Shaders,
    XGifs,
}

impl Want {
    fn kinds(self) -> &'static [Kind] {
        match self {
            Want::Both => &[Kind::Video, Kind::Picture],
            Want::Videos => &[Kind::Video],
            Want::Pictures => &[Kind::Picture],
            Want::Icons => &[Kind::Icon],
            Want::Textures => &[Kind::Texture],
            Want::Visualizers => &[Kind::Visualizer],
            Want::Fonts => &[Kind::Font],
            Want::Music => &[Kind::Music],
            Want::Shaders => &[Kind::Shader],
            Want::XGifs => &[Kind::Gif],
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
    /// A starter show's clip cell (the first column also starts playing).
    Cell(u64, usize),
}

/// A starter show's layer being filled: its search, how many results were tried, and the cells still to fill (a
/// download that fails or is too big gives its cell back).
struct Filling {
    layer: u64,
    search: Search,
    tried: usize,
    free: Vec<usize>,
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
    /// A shader just downloaded, to put on a layer once it's loaded: (slug, layer, give up at).
    apply: Option<(String, Option<u64>, f64)>,
    /// GIFs from X: saved accounts and the browser to sign in with (read when first shown), and whether gallery-dl
    /// is installed (looked for once).
    x: Option<crate::online::x::XSettings>,
    has_gallery_dl: Option<bool>,
    /// A starter show being built.
    filling: Vec<Filling>,
    starter: Option<&'static str>,
}

impl Online {
    /// Bench `--quick online` (PHOSPHOR_ONLINE_QUERY): searches like the panel does.
    pub fn bench_search(&mut self, query: &str) {
        self.query = query.to_string();
        // (Not X: it needs your sign-in.)
        let kinds = [Kind::Video, Kind::Picture, Kind::Icon, Kind::Texture, Kind::Visualizer, Kind::Font, Kind::Music, Kind::Shader];
        self.search = Some(Search::start(query, &Source::ALL, &kinds, false, None));
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
        // PHOSPHOR_ONLINE_PICK=video (an Internet Archive film: small files), icon or texture instead of a picture.
        let pick = std::env::var("PHOSPHOR_ONLINE_PICK").unwrap_or_default();
        let wanted = |i: &&Item| match pick.as_str() {
            "video" => i.source == Source::Archive && i.kind == Kind::Video,
            "icon" => i.kind == Kind::Icon,
            "texture" => i.kind == Kind::Texture,
            "visualizer" => i.kind == Kind::Visualizer,
            "font" => i.kind == Kind::Font,
            "music" => i.kind == Kind::Music,
            "shader" => i.kind == Kind::Shader,
            _ => i.kind == Kind::Picture,
        };
        if let Some(item) = s.items.iter().find(wanted).cloned() {
            st.online.bench = false;
            if item.kind == Kind::Font {
                // A text layer to put it on.
                let id = st.add_layer();
                if let Some(l) = st.comp.layer_mut(id) {
                    l.source = SourceSpec::Text(phosphor_engine::TextStyle { text: "PHOSPHOR".into(), ..crate::text::default_style() });
                }
                st.selected = Some(id);
            }
            st.online.downloads.push((Download::start(item, None), Then::NewLayer));
        }
    }
    fill_starter(st);
    apply_shader(st, ctx.input(|i| i.time));
    if st.online.downloads.is_empty() {
        return;
    }
    let now = ctx.input(|i| i.time);
    let mut finished = Vec::new();
    st.online.downloads.retain(|(d, then)| match d.status() {
        Fetch::Working { .. } => true,
        Fetch::Done(paths) => {
            finished.push((d.item.clone(), paths, *then));
            false
        }
        Fetch::Failed(e) => {
            match then {
                // A starter cell tries the next result instead.
                Then::Cell(layer, slot) => {
                    if let Some(f) = st.online.filling.iter_mut().find(|f| f.layer == *layer) {
                        f.free.push(*slot);
                        f.free.sort();
                    }
                }
                _ => st.notice = Some(format!("Couldn't download \"{}\": {e}", d.item.title)),
            }
            false
        }
    });
    for (item, paths, then) in finished {
        let Some(path) = paths.first().cloned() else { continue };
        if item.kind == Kind::Font {
            font_arrived(st, &item, &paths);
            continue;
        }
        if item.kind == Kind::Shader {
            shader_arrived(st, &item, &path, then, now);
            continue;
        }
        // Its title from the library reads better than the file name.
        let name = if item.credit.title.trim().is_empty() { crate::media::clip_name(&path) } else { item.credit.title.chars().take(60).collect() };
        st.comp.add_media(&name, &path);
        if let Some(m) = st.comp.media_bin.iter_mut().find(|m| m.path == path) {
            m.credit = Some(item.credit.clone());
        }
        st.online.here.insert(item.key(), path.clone());
        if let Then::Cell(layer, slot) = then {
            let spec = crate::media::spec_for_path(&path);
            st.comp.place_clips(layer, slot, vec![(name.clone(), spec.clone())]);
            // The first clip to arrive on a layer starts playing (so every layer shows something soon).
            if let Some(keep) = st.comp.layer(layer).filter(|l| l.source == SourceSpec::None).map(|l| l.name.clone()) {
                st.trigger(crate::app::Trigger::Clip { layer, slot }, now);
                // The layer keeps its starter name ("Nebulas"), not the clip's.
                if let Some(l) = st.comp.layer_mut(layer) {
                    l.name = keep;
                }
            }
            continue;
        }
        let layer = match then {
            Then::Bin | Then::Cell(..) => None,
            Then::Layer(id) => st.comp.layers.iter().any(|l| l.id == id).then_some(id),
            Then::NewLayer => Some(st.add_layer()),
        };
        if let Some(layer) = layer {
            st.selected = Some(layer);
            st.launch_source(layer, crate::media::spec_for_path(&path), Vec::new(), name, now);
        }
    }
}

/// GIFs from X: saved accounts (click one to browse it), save the one typed, and the browser whose X sign-in is used.
/// Returns true when an account was clicked (browse it now).
fn x_accounts(ui: &mut egui::Ui, o: &mut Online) -> bool {
    use crate::online::x;
    let mut browse = false;
    let mut changed = false;
    let xs = o.x.get_or_insert_with(x::XSettings::load);
    ui.horizontal_wrapped(|ui| {
        let mut gone = None;
        for acc in &xs.accounts {
            let r = crate::ui::command(ui, format!("@{acc}")).on_hover_text("Browse this account's GIFs · right-click: remove");
            if r.clicked() {
                o.query = format!("@{acc}");
                browse = true;
            }
            r.right_menu(|ui| {
                if danger_button(ui, label(icon::TRASH, "Remove")).clicked() {
                    gone = Some(acc.clone());
                    ui.close();
                }
            });
        }
        if let Some(acc) = gone {
            xs.accounts.retain(|a| *a != acc);
            changed = true;
        }
        if let Some(h) = x::handle(&o.query).filter(|h| !xs.accounts.contains(h))
            && crate::ui::command(ui, label(icon::PLUS, &format!("Save @{h}"))).on_hover_text("Keep this account here to browse with one click").clicked()
        {
            xs.accounts.push(h);
            changed = true;
        }
    });
    crate::ui::toolbar(ui, |ui| {
        ui.label("Signed in to X in");
        let current = xs.browser.clone().unwrap_or_else(|| "pick a browser".into());
        crate::ui::dropdown(ui, "x_browser", 110.0).selected_text(current).show_ui(ui, |ui| {
            for b in crate::web_media::BROWSERS {
                if crate::ui::pick_item(ui, xs.browser.as_deref() == Some(b), b).clicked() {
                    xs.browser = Some(b.to_string());
                    changed = true;
                }
            }
        })
        .response
        .on_hover_text("X only shows an account's posts to people signed in: gallery-dl uses that browser's X sign-in (only to list the posts)");
    });
    if !*o.has_gallery_dl.get_or_insert_with(|| crate::web_media::find_gallery_dl().is_some()) {
        ui.colored_label(WARN, x::NO_TOOL);
    }
    if changed {
        xs.save();
    }
    browse
}

/// Builds a starter show: replaces the layers with the starter's three (outputs, mapping and controls stay) and
/// starts their searches; `fill_starter` downloads into their cells as results come.
fn start_show(st: &mut State, starter: &crate::online::starter::Starter, ctx: &egui::Context) {
    st.comp.layers.clear();
    st.comp.groups.clear();
    st.comp.launch.fade = true;
    st.online.filling.clear();
    for row in &starter.rows {
        let l = st.comp.add_layer();
        l.name = row.layer.to_string();
        (l.blend, l.opacity) = (row.blend, row.opacity);
        let layer = l.id;
        let search = Search::start(row.query, &[row.source], &[row.kind], false, Some(ctx.clone()));
        st.online.filling.push(Filling { layer, search, tried: 0, free: (0..crate::online::starter::CLIPS).collect() });
    }
    st.comp.clip_columns = st.comp.clip_columns.max(crate::online::starter::CLIPS);
    st.online.starter = Some(starter.name);
    st.notice = Some(format!("Building the {} show: clips arrive in the Clips grid as they download", starter.name));
}

/// Bench `--quick starter` (PHOSPHOR_STARTER = a mood's name): builds that starter show.
pub(crate) fn bench_starter(st: &mut State, name: &str, ctx: &egui::Context) -> bool {
    let Some(s) = crate::online::starter::STARTERS.iter().find(|s| s.name.eq_ignore_ascii_case(name)) else { return false };
    start_show(st, s, ctx);
    true
}

/// Starts downloads for a starter show's cells as search results come in.
fn fill_starter(st: &mut State) {
    use crate::online::starter::{CLIPS, MAX_SIZE};
    let o = &mut st.online;
    for f in &mut o.filling {
        f.search.poll();
        while !f.free.is_empty() && f.tried < f.search.items.len() {
            let item = f.search.items[f.tried].clone();
            f.tried += 1;
            // Too big (when the library says) or already coming: the next one.
            if item.size.is_some_and(|s| s > MAX_SIZE) || o.downloads.iter().any(|(d, _)| d.item.key() == item.key()) {
                continue;
            }
            let slot = f.free.remove(0);
            o.downloads.push((Download::start_limited(item, MAX_SIZE, None), Then::Cell(f.layer, slot)));
        }
        // More results when this page ran out.
        if !f.free.is_empty() && f.tried >= f.search.items.len() && f.search.has_more() && f.tried < CLIPS * 6 {
            f.search.more();
        }
    }
    // Kept while its downloads run (a failed one gives its cell back) or it has cells to fill and results to try.
    let coming: Vec<u64> = o.downloads.iter().filter_map(|(_, t)| if let Then::Cell(l, _) = t { Some(*l) } else { None }).collect();
    o.filling.retain(|f| coming.contains(&f.layer) || (!f.free.is_empty() && (f.search.busy() || f.tried < f.search.items.len() || f.search.has_more())));
    if o.filling.is_empty() && !o.downloads.iter().any(|(_, t)| matches!(t, Then::Cell(..))) {
        o.starter = None;
    }
}

/// A downloaded ISF shader: the library scans again (it takes a few seconds to appear), and once it's in it goes on
/// the layer it was meant for (an effect added, or a generator as its source; `apply_shader`).
fn shader_arrived(st: &mut State, item: &Item, path: &str, then: Then, now: f64) {
    st.online.here.insert(item.key(), path.to_string());
    st.isf = crate::isf_lib::IsfLibrary::scan();
    let stem = std::path::Path::new(path).file_stem().map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let layer = match then {
        Then::Layer(id) => Some(id),
        Then::NewLayer => Some(st.add_layer()),
        Then::Bin | Then::Cell(..) => st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id)),
    };
    st.online.apply = Some((phosphor_engine::custom::slug(&stem), layer, now + 30.0));
    st.notice = Some(format!("{} is loading: it appears in Effects / Sources (ISF) in a few seconds", item.title));
}

/// Puts a just-downloaded shader on its layer once the ISF scan has it (gives up after 30 s).
fn apply_shader(st: &mut State, now: f64) {
    let Some((slug, layer, until)) = st.online.apply.clone() else { return };
    if now > until {
        st.online.apply = None;
        return;
    }
    let effect = format!("{}{slug}", phosphor_engine::shaders::ISF_EFFECT);
    let generator = format!("{}{slug}", phosphor_engine::shaders::ISF_GENERATOR);
    if phosphor_engine::shaders::effect(&effect).is_some() {
        st.online.apply = None;
        if let Some(id) = layer {
            super::effects::add_effect_to(st, id, &effect);
            st.notice = Some(format!("Added the {slug} effect to {}", crate::ui::layer_name(&st.comp.layers, id)));
        }
    } else if let Some(desc) = phosphor_engine::shaders::generator(&generator) {
        st.online.apply = None;
        if let Some(id) = layer {
            st.launch_source(id, SourceSpec::Generator(generator.clone()), Vec::new(), desc.name.to_string(), now);
        }
    }
}

/// A downloaded font family joins the font list; the selected text layer (if any) takes it on.
fn font_arrived(st: &mut State, item: &Item, paths: &[String]) {
    let faces: Vec<String> = paths.iter().flat_map(|p| crate::text::add_font_file(std::path::Path::new(p))).collect();
    st.online.here.insert(item.key(), paths[0].clone());
    let all = crate::text::fonts();
    let first = faces.first().and_then(|n| all.iter().find(|f| &f.name == n)).map(|f| f.id.clone());
    let layer = st.selected.and_then(|id| st.comp.layers.iter_mut().find(|l| l.id == id));
    match (layer, first) {
        (Some(l), Some(font)) if matches!(l.source, SourceSpec::Text(_)) => {
            if let SourceSpec::Text(style) = &mut l.source {
                style.font = font;
            }
            st.notice = Some(format!("{} is on the selected text layer (more styles in its Font list)", item.credit.title));
        }
        _ => st.notice = Some(format!("Added the font {}: pick it in any text layer's Font list", item.credit.title)),
    }
}

pub(crate) fn online_panel(ui: &mut egui::Ui, st: &mut State) {
    let o = &mut st.online;
    let mut go = false;
    ui.horizontal(|ui| {
        let prompt = if o.want == Want::XGifs { "An X account: @giphy, @nasa…" } else { "Search: aurora, city lights, waves, fire…" };
        let r = crate::ui::text_field(ui, &mut o.query, prompt, Some((ui.available_width() - if o.search.is_some() { 64.0 } else { 34.0 }).max(80.0)));
        go |= r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        go |= crate::ui::command_if(ui, !o.query.trim().is_empty(), icon::MAGNIFYING_GLASS).on_hover_text("Search every library that's on").clicked();
        if o.search.is_some() && crate::ui::command(ui, icon::X).on_hover_text("Clear the results (back to Start a show)").clicked() {
            o.search = None;
            o.query.clear();
        }
    });
    ui.horizontal_wrapped(|ui| {
        let opts = [
            (Want::Both, "Videos & pictures".to_string()),
            (Want::Videos, "Videos".to_string()),
            (Want::Pictures, "Pictures".to_string()),
            (Want::Icons, "Icons".to_string()),
            (Want::Textures, "Textures".to_string()),
            (Want::Visualizers, "Visualizers".to_string()),
            (Want::Fonts, "Fonts".to_string()),
            (Want::Music, "Music".to_string()),
            (Want::Shaders, "Shaders (ISF)".to_string()),
            (Want::XGifs, "GIFs from X".to_string()),
        ];
        let before = o.want;
        ComboBox::from_id_salt("online_want").selected_text(opts.iter().find(|(w, _)| *w == o.want).map_or("", |(_, t)| t.as_str())).show_ui(ui, |ui| {
            for (w, text) in &opts {
                crate::ui::pick_value(ui, &mut o.want, *w, text);
            }
        });
        go |= o.want != before && o.search.is_some();
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
    if o.want == Want::XGifs {
        go |= x_accounts(ui, o);
    }
    if go && !o.query.trim().is_empty() {
        let sources: Vec<Source> = Source::ALL.into_iter().filter(|s| !o.off.contains(s)).collect();
        // (X posts have no licence to filter by.)
        let commercial = o.commercial && o.want != Want::XGifs;
        o.search = Some(Search::start(&o.query, &sources, o.want.kinds(), commercial, Some(ui.ctx().clone())));
    }
    let selected = st.selected.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
    let o = &mut st.online;
    let Some(search) = o.search.as_mut() else {
        crate::ui::heading(ui, label(icon::SPARKLE, "Start a show"));
        hint(ui, "One click fills three layers of clips from these libraries for a mood (replaces the layers; outputs and mapping stay).");
        ui.horizontal_wrapped(|ui| {
            for s in &crate::online::starter::STARTERS {
                if crate::ui::command_if(ui, st.online.starter.is_none(), s.name).on_hover_text(s.about).clicked() {
                    start_show(st, s, ui.ctx());
                }
            }
        });
        if let Some(name) = st.online.starter {
            let coming = st.online.downloads.iter().filter(|(_, t)| matches!(t, Then::Cell(..))).count();
            crate::ui::toolbar(ui, |ui| {
                ui.spinner();
                hint(ui, format!("Building {name}: {coming} clips downloading"));
            });
        }
        ui.add_space(6.0);
        hint(ui, "Free videos, pictures, icons and textures to start a show with: NASA, Wikimedia Commons, the Internet Archive's old films, Openverse's Creative Commons photos, the Art Institute of Chicago, Iconify's icons, Poly Haven's textures and nearly 10,000 MilkDrop visualisers. Downloads go into the media bin with their credit (Credits, below).");
        credits(ui, &st.comp);
        return;
    };
    search.poll();
    crate::ui::toolbar(ui, |ui| {
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
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        // As many columns of at least 120 points as fit, filling the width.
        let (gap, room) = (6.0, ui.available_width() - 2.0);
        let cols = ((room + gap) / (120.0 + gap)).floor().max(1.0);
        let w = ((room - gap * (cols - 1.0)) / cols).floor();
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
        if search.has_more() && crate::ui::command(ui, label(icon::ARROW_DOWN, "More")).clicked() {
            search.more();
        }
        ui.add_space(8.0);
        credits(ui, &st.comp);
    });
    if let Some((item, then)) = picked {
        let now = ui.ctx().input(|i| i.time);
        match o.here.get(&item.key()).cloned() {
            // Already here: straight onto the layer.
            // A font that's here: onto the selected text layer again.
            Some(path) if item.kind == Kind::Font => font_arrived(st, &item, &[path]),
            Some(path) => {
                let layer = match then {
                    Then::Layer(id) => Some(id),
                    Then::NewLayer => None,
                    Then::Bin | Then::Cell(..) => return,
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
        Some(tex) => paint_texture(p, tex, fit_rect(thumb_rect.shrink(1.0), thumbs.aspect(&item.thumb).unwrap_or(16.0 / 9.0))),
        None => {
            let text = if thumbs.is_pending(&item.thumb) { "…" } else { kind_icon(item.kind) };
            p.text(thumb_rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(22.0), Color32::GRAY);
        }
    }
    let stroke = if local.is_some() { Stroke::new(2.0, accent()) } else if resp.hovered() { Stroke::new(1.5, accent()) } else { Stroke::new(1.0, Color32::from_gray(50)) };
    p.rect_stroke(thumb_rect, 4.0, stroke, StrokeKind::Inside);
    // Kind, and a tick once it's in the media bin.
    let corner = thumb_rect.left_top() + egui::vec2(4.0, 3.0);
    let badge = if local.is_some() { format!("{} {}", icon::CHECK, kind_icon(item.kind)) } else if item.kind == Kind::Picture { String::new() } else { kind_icon(item.kind).to_string() };
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
        if item.kind != Kind::Font {
            resp.dnd_set_drag_payload(SourceDrag { name: crate::media::clip_name(path), source: crate::media::spec_for_path(path) });
        }
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
        if local.is_none() && crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Download into the media bin")).clicked() {
            out = Some(Then::Bin);
            ui.close();
        }
        if crate::ui::command_if(ui, selected.is_some(), label(icon::ARROW_RIGHT, "On the selected layer")).on_disabled_hover_text("Select a layer first").clicked() {
            out = selected.map(Then::Layer);
            ui.close();
        }
        if crate::ui::command(ui, label(icon::PLUS, "On a new layer")).clicked() {
            out = Some(Then::NewLayer);
            ui.close();
        }
        ui.separator();
        if !c.page.is_empty() && crate::ui::command(ui, label(icon::GLOBE, &format!("Open its page ({})", c.source))).clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(&c.page));
            ui.close();
        }
        if crate::ui::command(ui, label(icon::COPY, "Copy the credit")).clicked() {
            ui.ctx().copy_text(c.line());
            ui.close();
        }
    });
    out
}

fn kind_icon(kind: Kind) -> &'static str {
    match kind {
        Kind::Video => icon::FILM_STRIP,
        Kind::Picture => icon::IMAGE,
        Kind::Icon => icon::SHAPES,
        Kind::Texture => icon::CHECKERBOARD,
        Kind::Visualizer => icon::WAVEFORM,
        Kind::Font => icon::TEXT_AA,
        Kind::Music => icon::MUSIC_NOTES,
        Kind::Shader => icon::SPARKLE,
        Kind::Gif => icon::GIF,
    }
}

/// Every credit in the show, to paste into a programme or a video's description.
fn credits(ui: &mut egui::Ui, comp: &phosphor_engine::Composition) {
    let n = comp.media_bin.iter().filter(|m| m.credit.is_some()).count();
    crate::ui::section(ui, "online_credits", format!("{} Credits ({n})", icon::IDENTIFICATION_CARD), false, |ui| {
        if n == 0 {
            hint(ui, "Files downloaded here keep who made them and their licence. Most need a credit when you show them in public: copy them all from here.");
            return;
        }
        if crate::ui::command(ui, label(icon::COPY, "Copy all credits")).clicked() {
            ui.ctx().copy_text(crate::online::credits_text(comp));
        }
        for m in comp.media_bin.iter().filter(|m| m.credit.is_some()) {
            let c = m.credit.as_ref().expect("filtered");
            let line = ui.add(egui::Label::new(format!("{} {}", if c.commercial() { icon::CHECK_CIRCLE } else { icon::WARNING }, c.line())).truncate());
            line.on_hover_text(if c.commercial() { "May be used in paid work (credit as shown)" } else { "Non-commercial licence: not for paid work" });
        }
    });
}
