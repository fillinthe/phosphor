//! Online media (Library › Online): search free libraries of videos and pictures (`sources`) and bring files into the
//! media bin, each with its credit and licence (`MediaItem::credit`), so someone with no media of their own can start
//! a show. Searches and downloads run on their own threads; downloads go to Phosphor's downloads folder (`online/`,
//! one folder per library) with a `.credit.json` beside each file, and a file already downloaded isn't fetched again.

pub mod sources;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use phosphor_engine::Credit;
pub use sources::Source;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    Video,
    Picture,
    /// SVG icons and symbols.
    Icon,
    /// Surface textures (for 3D models and backgrounds).
    Texture,
    /// MilkDrop presets (they move to the music).
    Visualizer,
    /// Font families for text layers.
    Font,
}

/// One search result.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub source: Source,
    pub kind: Kind,
    /// The library's own id.
    pub id: String,
    pub title: String,
    /// A small picture of it (address).
    pub thumb: String,
    /// The file to download, when the search already says (else it's looked up in `list`).
    pub file: Option<String>,
    /// Where the item's files are listed (NASA, Internet Archive).
    pub list: String,
    pub size: Option<u64>,
    pub credit: Credit,
}

impl Item {
    pub fn key(&self) -> (Source, String) {
        (self.source, self.id.clone())
    }
}

/// Files bigger than this aren't downloaded.
const MAX_FILE: u64 = 2 << 30;
/// Search answers and thumbnails bigger than this are refused.
const MAX_SMALL: u64 = 16 << 20;

fn agent() -> &'static ureq::Agent {
    static AGENT: OnceLock<ureq::Agent> = OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(10))
            .timeout_read(Duration::from_secs(30))
            // Wikimedia asks every app to say who it is.
            .user_agent(concat!("Phosphor/", env!("CARGO_PKG_VERSION"), " (https://github.com/fillinthe/phosphor)"))
            .build()
    })
}

fn request(url: &str) -> Result<ureq::Response, String> {
    let host = url.split('/').nth(2).unwrap_or(url).to_string();
    agent().get(url).call().map_err(|e| match e {
        ureq::Error::Status(429, _) => format!("{host} is busy (too many searches): try again in a minute"),
        ureq::Error::Status(code, _) => format!("{host} answered with an error ({code})"),
        ureq::Error::Transport(t) => {
            log::warn!("online: {url}: {t}");
            format!("{host} didn't answer: it may be busy, or the internet connection is down")
        }
    })
}

/// A small download (a search answer, a thumbnail) into memory.
pub fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    request(url)?.into_reader().take(MAX_SMALL).read_to_end(&mut out).map_err(|e| format!("download stopped: {e}"))?;
    Ok(out)
}

/// `get_bytes` once per session for each address (a library's whole list, searched here).
fn get_cached(url: &str) -> Result<Vec<u8>, String> {
    static KEPT: OnceLock<Mutex<std::collections::HashMap<String, Arc<Vec<u8>>>>> = OnceLock::new();
    let kept = KEPT.get_or_init(Default::default);
    if let Some(b) = kept.lock().ok().and_then(|k| k.get(url).cloned()) {
        return Ok(b.to_vec());
    }
    let b = get_bytes(url)?;
    if let Ok(mut k) = kept.lock() {
        k.insert(url.to_string(), Arc::new(b.clone()));
    }
    Ok(b)
}

// ---- searching ----

enum Msg {
    Found { kind: Kind, items: Vec<Item> },
    Failed(String),
}

/// One search across several libraries: every (library, kind) asks on its own thread; results come in as they
/// arrive. `more` asks for the next page of each.
pub struct Search {
    pub query: String,
    sources: Vec<Source>,
    kinds: Vec<Kind>,
    commercial: bool,
    pub items: Vec<Item>,
    pub errors: Vec<String>,
    page: usize,
    pending: usize,
    /// Some library sent a full page (so there may be more).
    full: bool,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    ctx: Option<eframe::egui::Context>,
}

impl Search {
    pub fn start(query: &str, sources: &[Source], kinds: &[Kind], commercial: bool, ctx: Option<eframe::egui::Context>) -> Search {
        let (tx, rx) = channel();
        let mut s = Search {
            query: query.trim().to_string(),
            sources: sources.to_vec(),
            kinds: kinds.to_vec(),
            commercial,
            items: Vec::new(),
            errors: Vec::new(),
            page: 0,
            pending: 0,
            full: false,
            tx,
            rx,
            ctx,
        };
        s.more();
        s
    }

    /// Asks every library for its next page.
    pub fn more(&mut self) {
        self.page += 1;
        self.full = false;
        for &source in &self.sources {
            for &kind in source.kinds().iter().filter(|k| self.kinds.contains(k)) {
                let url = source.search_url(&self.query, kind, self.page, self.commercial);
                let (tx, ctx, query, page) = (self.tx.clone(), self.ctx.clone(), self.query.clone(), self.page);
                self.pending += 1;
                let _ = std::thread::Builder::new().name("online search".into()).spawn(move || {
                    let body = if source.whole_list() { get_cached(&url) } else { get_bytes(&url) };
                    let msg = match body.and_then(|b| source.parse_page(kind, &query, page, &b)) {
                        Ok(items) => Msg::Found { kind, items },
                        Err(e) => Msg::Failed(format!("{}: {e}", source.name())),
                    };
                    let _ = tx.send(msg);
                    if let Some(c) = ctx {
                        c.request_repaint();
                    }
                });
            }
        }
    }

    /// Picks up results; call every frame while shown.
    pub fn poll(&mut self) {
        for msg in self.rx.try_iter() {
            self.pending = self.pending.saturating_sub(1);
            match msg {
                Msg::Found { kind, items } => {
                    self.full |= items.len() >= sources::PAGE / 2;
                    let fresh: Vec<Item> = items
                        .into_iter()
                        .filter(|i| i.kind == kind && (!self.commercial || i.credit.commercial()))
                        .filter(|i| !self.items.iter().any(|o| o.key() == i.key()))
                        .collect();
                    self.items.extend(fresh);
                }
                Msg::Failed(e) => self.errors.push(e),
            }
        }
    }

    pub fn busy(&self) -> bool {
        self.pending > 0
    }

    pub fn has_more(&self) -> bool {
        !self.busy() && self.full
    }
}

// ---- downloading ----

#[derive(Clone, Debug, PartialEq)]
pub enum Fetch {
    Working { done: u64, total: Option<u64> },
    /// The files (the first is the one to use).
    Done(Vec<String>),
    Failed(String),
}

/// One file coming in (or already here).
pub struct Download {
    pub item: Item,
    status: Arc<Mutex<Fetch>>,
}

impl Download {
    pub fn start(item: Item, ctx: Option<eframe::egui::Context>) -> Download {
        let status = Arc::new(Mutex::new(Fetch::Working { done: 0, total: item.size }));
        let (shared, it) = (status.clone(), item.clone());
        let _ = std::thread::Builder::new().name("online download".into()).spawn(move || {
            let set = |f: Fetch| {
                if let Ok(mut s) = shared.lock() {
                    *s = f;
                }
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            };
            let end = match fetch(&it, |done, total| set(Fetch::Working { done, total })) {
                Ok(paths) => Fetch::Done(paths.iter().map(|p| p.to_string_lossy().into_owned()).collect()),
                Err(e) => Fetch::Failed(e),
            };
            set(end);
        });
        Download { item, status }
    }

    pub fn status(&self) -> Fetch {
        self.status.lock().map(|s| s.clone()).unwrap_or(Fetch::Failed("stopped".into()))
    }
}

/// Where online files go.
pub fn folder() -> Option<PathBuf> {
    crate::web_media::downloads_dir().map(|d| d.join("online"))
}

/// A file name from the title and id, with the file's own extension (else one that suits the kind).
pub fn file_name(item: &Item, url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let last = path.rsplit('/').next().unwrap_or("");
    let ext = last.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).filter(|e| (2..=4).contains(&e.len()) && e.chars().all(|c| c.is_ascii_alphanumeric()));
    let ext = ext.unwrap_or_else(|| {
        match item.kind {
            Kind::Video => "mp4",
            Kind::Icon => "svg",
            Kind::Visualizer => "milk",
            Kind::Font => "ttf",
            Kind::Picture | Kind::Texture => "jpg",
        }
        .into()
    });
    let title: String = crate::show::slug(&item.title).chars().take(48).collect();
    // The id's end tells files apart (ids can be long; their ends differ).
    // (An id that's a file path loses its extension; an id the title already holds isn't repeated.)
    let bare = item.id.rsplit_once('.').filter(|(_, e)| e.len() <= 5 && !e.contains('/')).map_or(item.id.as_str(), |(b, _)| b);
    let id = crate::show::slug(bare);
    let id = id.get(id.len().saturating_sub(24)..).unwrap_or(&id).trim_start_matches('-');
    if title.contains(id) { format!("{title}.{ext}") } else { format!("{title}-{id}.{ext}") }
}

fn sidecar(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(".credit.json");
    PathBuf::from(s)
}

/// Removes downloads left half-done (Phosphor quit while they ran) more than an hour ago.
fn sweep_parts(dir: &Path) {
    let Ok(list) = std::fs::read_dir(dir) else { return };
    for e in list.flatten() {
        let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age > Duration::from_secs(3600));
        if old && e.path().extension().is_some_and(|x| x == "part") {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// The credit saved beside a downloaded file.
pub fn saved_credit(path: &Path) -> Option<Credit> {
    serde_json::from_slice(&std::fs::read(sidecar(path)).ok()?).ok()
}

/// Downloads an item's files (streamed to disk, `progress` as it goes) and saves its credit beside the first.
/// Most items are one file; a font family is a few (regular, bold), saved where text layers find fonts.
fn fetch(item: &Item, progress: impl Fn(u64, Option<u64>)) -> Result<Vec<PathBuf>, String> {
    if item.kind == Kind::Font {
        let dir = crate::text::fonts_dir().ok_or("no fonts folder")?;
        std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
        let files = sources::font_files(&get_bytes(&item.list)?)?;
        let mut out = Vec::new();
        for (name, url) in files {
            let path = dir.join(name);
            if !path.exists() {
                download_to(&url, &path, None, &progress)?;
            }
            out.push(path);
        }
        if let Some(first) = out.first() {
            let _ = std::fs::write(sidecar(first), serde_json::to_vec_pretty(&item.credit).unwrap_or_default());
        }
        return Ok(out);
    }
    let (url, size) = match &item.file {
        Some(f) => (f.clone(), item.size),
        None => sources::pick_file(item, &get_bytes(&item.list)?)?,
    };
    let dir = folder().ok_or("no downloads folder")?.join(crate::show::slug(item.source.name()));
    std::fs::create_dir_all(&dir).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
    sweep_parts(&dir);
    let path = dir.join(file_name(item, &url));
    if !(path.exists() && saved_credit(&path).is_some()) {
        download_to(&url, &path, size, &progress)?;
        let _ = std::fs::write(sidecar(&path), serde_json::to_vec_pretty(&item.credit).unwrap_or_default());
    }
    Ok(vec![path])
}

/// One file streamed to `path` (through a `.part` file, so a stopped download never looks finished).
fn download_to(url: &str, path: &Path, size: Option<u64>, progress: &impl Fn(u64, Option<u64>)) -> Result<(), String> {
    let response = request(url)?;
    let total = response.header("content-length").and_then(|l| l.parse().ok()).or(size);
    if total.is_some_and(|t| t > MAX_FILE) {
        return Err("That file is over 2 GB: too big to bring in".into());
    }
    let part = path.with_extension("part");
    let result = (|| {
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        let mut reader = response.into_reader().take(MAX_FILE);
        let mut buf = vec![0u8; 256 << 10];
        let mut done = 0u64;
        loop {
            let n = reader.read(&mut buf).map_err(|e| format!("download stopped: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            done += n as u64;
            progress(done, total);
        }
        if done == 0 {
            return Err("the file was empty".to_string());
        }
        Ok(())
    })();
    if let Err(e) = result {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, path).map_err(|e| e.to_string())
}

/// The show's credits: every media bin file that has one, as lines to paste into a programme or a video's notes.
pub fn credits_text(comp: &phosphor_engine::Composition) -> String {
    let mut lines: Vec<String> = comp.media_bin.iter().filter_map(|m| m.credit.as_ref()).map(Credit::line).collect();
    lines.dedup();
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(kind: Kind) -> Item {
        Item {
            source: Source::Nasa,
            kind,
            id: "ksc_061404_t-nebula".into(),
            title: "Trifid Nebula, seen by Hubble!".into(),
            thumb: String::new(),
            file: None,
            list: String::new(),
            size: None,
            credit: Credit { title: "Trifid".into(), licence: "Public domain".into(), source: "NASA".into(), ..Default::default() },
        }
    }

    #[test]
    fn file_names_are_safe_and_keep_the_extension() {
        let v = item(Kind::Video);
        assert_eq!(file_name(&v, "https://x/y/ksc~orig.mp4"), "trifid-nebula-seen-by-hubble-ksc-061404-t-nebula.mp4");
        assert_eq!(file_name(&v, "https://upload.wikimedia.org/a/b/Clip.WEBM?utm=1"), "trifid-nebula-seen-by-hubble-ksc-061404-t-nebula.webm");
        assert!(file_name(&item(Kind::Picture), "https://live.staticflickr.com/thumb/").ends_with(".jpg"), "no extension: one for the kind");
        assert!(file_name(&item(Kind::Picture), "https://www.artic.edu/iiif/2/99af/full/1686,/0/default.jpg").ends_with(".jpg"));
        let mut milk = item(Kind::Visualizer);
        (milk.id, milk.title) = ("Dancer/Geiss - Spiral.milk".into(), "Geiss - Spiral (Dancer)".into());
        assert_eq!(file_name(&milk, "https://raw.githubusercontent.com/x/master/Dancer/Geiss%20-%20Spiral.milk"), "geiss-spiral-dancer-dancer-geiss-spiral.milk");
    }

    #[test]
    fn credits_are_listed_once_each() {
        let mut comp = phosphor_engine::Composition::default();
        comp.add_media("a", "/a.mp4");
        comp.add_media("b", "/b.mp4");
        comp.add_media("c", "/c.mp4");
        let credit = Credit { title: "Trifid".into(), author: "NASA".into(), licence: "Public domain".into(), source: "NASA".into(), page: "https://images.nasa.gov/details/x".into(), ..Default::default() };
        comp.media_bin[0].credit = Some(credit.clone());
        comp.media_bin[1].credit = Some(credit);
        assert_eq!(credits_text(&comp), "\"Trifid\" by NASA (Public domain), NASA: https://images.nasa.gov/details/x");
        // Saved projects keep the credit, and old ones (no credit) still open.
        let json = serde_json::to_string(&comp.media_bin[0]).unwrap();
        let back: phosphor_engine::MediaItem = serde_json::from_str(&json).unwrap();
        assert_eq!(back, comp.media_bin[0]);
        let old: phosphor_engine::MediaItem = serde_json::from_str(r#"{"name":"x","path":"/x.mov"}"#).unwrap();
        assert!(old.credit.is_none());
        assert!(!serde_json::to_string(&comp.media_bin[2]).unwrap().contains("credit"), "nothing written for files without one");
    }

    /// The real libraries, over the internet: `PHOSPHOR_ONLINE_TEST=1 cargo test online_live -- --nocapture`.
    #[test]
    fn online_live() {
        if std::env::var_os("PHOSPHOR_ONLINE_TEST").is_none() {
            return;
        }
        let _ = env_logger::builder().is_test(true).try_init();
        let mut s = Search::start("aurora", &Source::ALL, &[Kind::Video, Kind::Picture], false, None);
        while s.busy() {
            std::thread::sleep(Duration::from_millis(100));
            s.poll();
        }
        for src in Source::ALL {
            let n = s.items.iter().filter(|i| i.source == src).count();
            println!("ONLINE {} {n}", src.name());
        }
        println!("ONLINE errors {:?}", s.errors);
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        // A NASA picture's file is found through its list and downloads with its credit.
        let pic = s.items.iter().find(|i| i.source == Source::Nasa && i.kind == Kind::Picture).expect("a NASA picture").clone();
        let path = fetch(&pic, |_, _| {}).expect("download").remove(0);
        assert!(saved_credit(&path).is_some());
        assert!(crate::media::decode_still(&path.to_string_lossy()).is_ok(), "a picture that opens");
        let thumb = get_bytes(&pic.thumb).expect("thumbnail");
        assert!(image::load_from_memory(&thumb).is_ok());
        let _ = std::fs::remove_file(sidecar(&path));
        let _ = std::fs::remove_file(&path);
        // Videos listed separately (NASA, Internet Archive) find a playable file (not downloaded: they're big).
        for src in [Source::Nasa, Source::Archive] {
            let v = s.items.iter().find(|i| i.source == src && i.kind == Kind::Video).expect("a video").clone();
            let (url, _) = sources::pick_file(&v, &get_bytes(&v.list).expect("file list")).expect("a file");
            println!("ONLINE {} video {url}", src.name());
            assert!(url.starts_with("https://") && url.to_ascii_lowercase().ends_with(".mp4"), "{url}");
        }
        // Icons and textures: an icon opens as an SVG picture; a texture finds its 2K colour picture.
        let mut s = Search::start("stone", &[Source::Iconify, Source::PolyHaven], &[Kind::Icon, Kind::Texture], false, None);
        while s.busy() {
            std::thread::sleep(Duration::from_millis(100));
            s.poll();
        }
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        let icon = s.items.iter().find(|i| i.kind == Kind::Icon).expect("an icon").clone();
        let svg = get_bytes(icon.file.as_deref().unwrap()).expect("svg");
        assert!(crate::media::render_svg(&svg).is_ok(), "the icon draws");
        let tex = s.items.iter().find(|i| i.kind == Kind::Texture).expect("a texture").clone();
        let (url, size) = sources::pick_file(&tex, &get_bytes(&tex.list).expect("files")).expect("a picture");
        println!("ONLINE icon {} texture {url} {size:?}", icon.id);
        assert!(url.ends_with(".jpg"));
        // A MilkDrop preset from the pack.
        let mut s = Search::start("aurora", &[Source::MilkDrop], &[Kind::Visualizer], false, None);
        while s.busy() {
            std::thread::sleep(Duration::from_millis(100));
            s.poll();
        }
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        let milk = s.items.first().expect("a preset").clone();
        let body = get_bytes(milk.file.as_deref().unwrap()).expect("preset");
        println!("ONLINE milkdrop {} presets, first {} ({} bytes)", s.items.len(), milk.title, body.len());
        assert!(String::from_utf8_lossy(&body).contains("per_frame") || String::from_utf8_lossy(&body).contains("[preset"), "a preset file");
        // A font family: its files are found and one draws text.
        let mut s = Search::start("bebas", &[Source::GoogleFonts], &[Kind::Font], false, None);
        while s.busy() {
            std::thread::sleep(Duration::from_millis(100));
            s.poll();
        }
        assert!(s.errors.is_empty(), "{:?}", s.errors);
        let font = s.items.first().expect("a font").clone();
        let files = sources::font_files(&get_bytes(&font.list).expect("file list")).expect("files");
        let bytes = get_bytes(&files[0].1).expect("font");
        println!("ONLINE font {} files {:?} ({} bytes)", font.id, files.iter().map(|f| &f.0).collect::<Vec<_>>(), bytes.len());
        assert!(ttf_parser::Face::parse(&bytes, 0).is_ok(), "a font file");
    }
}
