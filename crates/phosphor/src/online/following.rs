//! Following (Media panel): X and Instagram accounts whose newest clips come into the media bin on their own. Refresh
//! lists each account's posts with gallery-dl (your browser's sign-in: neither site lists posts to anyone signed out),
//! keeps its newest `per_account` clips (X: animated GIFs; Instagram: videos) and saves the missing ones into
//! `downloads/Following/@name (Site)/`, each with its credit. That folder is watched, so every account shows as a
//! group in Media, and the files are on this Mac: a show works offline.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Item, Kind, Source};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Site {
    #[default]
    X,
    Instagram,
}

impl Site {
    pub const ALL: [Site; 2] = [Site::X, Site::Instagram];

    pub fn name(self) -> &'static str {
        match self {
            Site::X => "X",
            Site::Instagram => "Instagram",
        }
    }
}

/// Which of an account's lists is followed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Feed {
    /// Its own posts.
    #[default]
    Posts,
    /// Posts you liked (X: only yours, through your sign-in).
    Likes,
    /// Your X bookmarks.
    Bookmarks,
    /// Your Instagram saved posts.
    Saved,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    pub site: Site,
    /// The account (for your likes / bookmarks on X: unused; for your Instagram saved posts: your name).
    pub name: String,
    pub feed: Feed,
}

impl Account {
    pub fn new(site: Site, name: &str) -> Account {
        Account { site, name: name.to_string(), feed: Feed::Posts }
    }

    /// One of your own lists: X likes / bookmarks, Instagram saved (needs your name).
    pub fn mine(site: Site, feed: Feed, name: &str) -> Account {
        Account { site, name: name.to_string(), feed }
    }

    /// Its folder's name and how it's listed: "@name (X)", "My likes (X)", "My saved (Instagram)".
    pub fn label(&self) -> String {
        match self.feed {
            Feed::Posts => format!("@{} ({})", self.name, self.site.name()),
            Feed::Likes => format!("My likes ({})", self.site.name()),
            Feed::Bookmarks => format!("My bookmarks ({})", self.site.name()),
            Feed::Saved => format!("My saved ({})", self.site.name()),
        }
    }

    /// The sidebar's words (after the site's logo).
    pub fn short(&self) -> String {
        match self.feed {
            Feed::Posts => self.name.clone(),
            Feed::Likes => "My likes".into(),
            Feed::Bookmarks => "My bookmarks".into(),
            Feed::Saved => "My saved".into(),
        }
    }

    fn profile(&self) -> String {
        match (self.site, self.feed) {
            (Site::X, Feed::Likes) => "https://x.com/i/history/likes".into(),
            (Site::X, Feed::Bookmarks) => "https://x.com/i/bookmarks".into(),
            (Site::X, _) => format!("https://x.com/{}/media", self.name),
            (Site::Instagram, Feed::Saved) => format!("https://www.instagram.com/{}/saved/", self.name),
            (Site::Instagram, _) => format!("https://www.instagram.com/{}/", self.name),
        }
    }

    /// Where its clips are saved (a subfolder of `folder`, so Media shows it as a group of its own).
    pub fn dir(&self) -> Option<PathBuf> {
        folder().map(|f| f.join(self.label()))
    }
}

/// Followed accounts, the browser whose sign-in lists their posts, and how many clips to keep per account (kept in
/// Phosphor's settings folder: the same list in every project).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Following {
    pub accounts: Vec<Account>,
    /// One of `web_media::BROWSERS`.
    pub browser: Option<String>,
    pub per_account: usize,
    /// Where "Keep in my library" copies clips to hold onto them (None = Movies/Phosphor Library).
    pub library: Option<String>,
}

impl Default for Following {
    fn default() -> Self {
        Following { accounts: Vec::new(), browser: None, per_account: 12, library: None }
    }
}

/// Movies/Phosphor Library: where kept clips go unless you pick another folder.
pub fn default_library() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Movies").join("Phosphor Library"))
}

impl Following {
    pub fn library_dir(&self) -> Option<PathBuf> {
        self.library.as_ref().map(PathBuf::from).or_else(default_library)
    }
}

/// Copies downloaded files (and their credits) into `library/<their folder's name>/`, so they're yours to keep
/// whatever happens to the downloads. Returns how many were copied (already there: skipped). Runs on a thread.
pub fn keep(files: Vec<PathBuf>, library: PathBuf, ctx: Option<eframe::egui::Context>, done: Arc<Mutex<Option<String>>>) {
    let _ = std::thread::Builder::new().name("keep".into()).spawn(move || {
        let mut copied = 0;
        let mut problem = None;
        for f in &files {
            let group = f.parent().and_then(|p| p.file_name()).map_or_else(|| "Kept".into(), |n| n.to_string_lossy().into_owned());
            let dir = library.join(group);
            let Some(name) = f.file_name() else { continue };
            let to = dir.join(name);
            if to.exists() {
                continue;
            }
            let r = std::fs::create_dir_all(&dir).and_then(|_| std::fs::copy(f, &to));
            match r {
                Ok(_) => {
                    copied += 1;
                    let mut credit = f.as_os_str().to_owned();
                    credit.push(".credit.json");
                    let mut credit_to = to.as_os_str().to_owned();
                    credit_to.push(".credit.json");
                    let _ = std::fs::copy(PathBuf::from(credit), PathBuf::from(credit_to));
                }
                Err(e) => problem = Some(format!("Couldn't keep {}: {e}", f.display())),
            }
        }
        if let Ok(mut d) = done.lock() {
            *d = Some(problem.unwrap_or_else(|| format!("Kept {copied} in {} (it's under This Mac in Media)", library.display())));
        }
        if let Some(c) = ctx {
            c.request_repaint();
        }
    });
}

/// The folder every followed account's clips go in (watched by the media bin).
pub fn folder() -> Option<PathBuf> {
    crate::web_media::downloads_dir().map(|d| d.join("Following"))
}

fn settings_file() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("following.json"))
}

/// The X-only list from before Following existed (its accounts and browser carry over).
#[derive(Default, Deserialize)]
#[serde(default)]
struct OldXSettings {
    accounts: Vec<String>,
    browser: Option<String>,
}

impl Following {
    pub fn load() -> Following {
        if let Some(f) = settings_file().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()) {
            return f;
        }
        let old: OldXSettings = crate::show::storage_dir().and_then(|d| std::fs::read(d.join("x_accounts.json")).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        Following { accounts: old.accounts.into_iter().map(|name| Account::new(Site::X, &name)).collect(), browser: old.browser, ..Default::default() }
    }

    pub fn save(&self) {
        if let Some(p) = settings_file() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, serde_json::to_vec_pretty(self).unwrap_or_default());
        }
    }

    /// Follows an account (no duplicates). Returns false if it's already followed.
    #[cfg(test)]
    pub fn follow(&mut self, site: Site, name: &str) -> bool {
        self.add(Account::new(site, name))
    }

    /// Adds an account or list (no duplicates). Returns false if it's already here.
    pub fn add(&mut self, a: Account) -> bool {
        if self.accounts.contains(&a) {
            return false;
        }
        self.accounts.push(a);
        true
    }
}

/// An account name from what was typed: "@nasa", "nasa", "x.com/nasa", "instagram.com/nasa/" → "nasa" (and the site
/// when a link says which).
pub fn handle(typed: &str) -> Option<(Option<Site>, String)> {
    let t = typed.trim().trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    let (site, rest) = if let Some(r) = t.strip_prefix("x.com/").or_else(|| t.strip_prefix("twitter.com/")) {
        (Some(Site::X), r)
    } else if let Some(r) = t.strip_prefix("instagram.com/") {
        (Some(Site::Instagram), r)
    } else {
        (None, t)
    };
    let h = rest.trim_start_matches('@').split(['/', '?']).next().unwrap_or("");
    // X names: up to 15 letters, digits and _; Instagram: up to 30, and dots too.
    let ok = !h.is_empty() && h.len() <= 30 && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    ok.then(|| (site, h.to_string()))
}

pub const NO_TOOL: &str = "Needs gallery-dl (free): in Terminal, brew install gallery-dl";
pub const NO_BROWSER: &str = "Pick the browser you're signed in with (X and Instagram only list an account's posts to people signed in)";

/// gallery-dl's listing (JSON) of the first `count` media of `url`, using `browser`'s sign-in. At most two minutes.
pub fn gallery_list(url: &str, first: usize, count: usize, browser: &str) -> Result<Vec<u8>, String> {
    let tool = crate::web_media::find_gallery_dl().ok_or(NO_TOOL)?;
    let mut child = Command::new(tool)
        .args(["-j", "--range", &format!("{first}-{}", first + count.max(1) - 1), "--cookies-from-browser", browser])
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("couldn't start gallery-dl: {e}"))?;
    // Its output is read on a thread so a full pipe never stalls it; it's stopped if it takes too long.
    let mut out = child.stdout.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut b);
        b
    });
    let until = Instant::now() + Duration::from_secs(120);
    loop {
        if child.try_wait().map_err(|e| e.to_string())?.is_some() {
            break;
        }
        if Instant::now() > until {
            let _ = child.kill();
            return Err("the site took too long to answer: try again".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    reader.join().map_err(|_| "gallery-dl's output was lost".to_string())
}

/// The clips in gallery-dl's listing of an account: X's animated GIFs, Instagram's videos.
pub fn parse(account: &Account, body: &[u8]) -> Result<Vec<Item>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "gallery-dl sent something unreadable".to_string())?;
    let entries = v.as_array().ok_or("gallery-dl sent something unreadable")?;
    let site = account.site.name();
    let mut out = Vec::new();
    for e in entries {
        let kind = e.get(0).and_then(Value::as_i64).unwrap_or(0);
        if kind == -1 {
            let msg = e[1]["message"].as_str().unwrap_or("");
            let err = e[1]["error"].as_str().unwrap_or("");
            return Err(if err == "AuthRequired" || msg.contains("cookies") || msg.contains("login") {
                format!("{site} wants you signed in: sign in to {site} in the browser picked here, then refresh")
            } else if msg.is_empty() {
                format!("{site} said no ({err})")
            } else {
                format!("{site} said no: {msg}")
            });
        }
        if kind != 3 {
            continue;
        }
        let (Some(url), info) = (e.get(1).and_then(Value::as_str), &e[2]) else { continue };
        if !url.starts_with("https://") {
            continue;
        }
        // Who posted it (likes and saved posts are other people's).
        let author = info["author"]["name"].as_str().or(info["owner"]["username"].as_str()).or(info["username"].as_str()).unwrap_or(&account.name).to_string();
        let text = |k: &str| info[k].as_str().map(str::to_string).or_else(|| info[k].as_u64().map(|n| n.to_string())).unwrap_or_default();
        let (wanted, post, page, words) = match account.site {
            Site::X => {
                let post = text("tweet_id");
                (info["type"].as_str() == Some("animated_gif"), post.clone(), format!("https://x.com/{}/status/{post}", author), text("content"))
            }
            Site::Instagram => {
                let code = text("post_shortcode");
                let code = if code.is_empty() { text("shortcode") } else { code };
                let is_video = info["extension"].as_str() == Some("mp4") || info["video_url"].is_string();
                (is_video, if code.is_empty() { text("media_id") } else { code.clone() }, format!("https://www.instagram.com/p/{code}/"), text("description"))
            }
        };
        if !wanted {
            continue;
        }
        let num = info["num"].as_u64().unwrap_or(1);
        let words: String = words.split_whitespace().filter(|w| !w.starts_with("https://")).collect::<Vec<_>>().join(" ");
        let title = if words.is_empty() { format!("{} from @{author}", if account.site == Site::X { "GIF" } else { "Video" }) } else { words.chars().take(60).collect() };
        out.push(Item {
            source: Source::X,
            kind: Kind::Gif,
            id: format!("{post}-{num}"),
            title: title.clone(),
            thumb: String::new(),
            // (Instagram's file links are signed: kept whole.)
            file: Some(if account.site == Site::X { url.split('?').next().unwrap_or(url).to_string() } else { url.to_string() }),
            list: String::new(),
            size: None,
            credit: phosphor_engine::Credit {
                title,
                author: format!("@{author}"),
                licence: format!("Unknown (posted on {site}: ask before showing it in public)"),
                licence_url: String::new(),
                page,
                source: site.into(),
            },
        });
    }
    Ok(out)
}

/// What a refresh is doing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Progress {
    pub running: bool,
    /// Per account: what happened ("3 new", or a problem).
    pub lines: Vec<(String, String)>,
    pub new_files: usize,
}

/// Refreshes every followed account on a thread: lists its newest clips and saves the ones not here yet.
pub fn refresh(f: &Following, ctx: Option<eframe::egui::Context>) -> Arc<Mutex<Progress>> {
    let wants = f.accounts.iter().map(|a| (a.clone(), f.per_account)).collect();
    fetch_accounts(f, wants, ctx)
}

/// Load more: the next `per_account` older clips of one account (it has `have`).
pub fn more(f: &Following, account: &Account, have: usize, ctx: Option<eframe::egui::Context>) -> Arc<Mutex<Progress>> {
    fetch_accounts(f, vec![(account.clone(), have + f.per_account)], ctx)
}

/// Most of an account's posts' media looked through at once (Load more goes this far back).
const MAX_LOOK: usize = 900;

/// For each account, makes sure its newest `want` clips are here (on a thread, one fetch at a time).
fn fetch_accounts(f: &Following, wants: Vec<(Account, usize)>, ctx: Option<eframe::egui::Context>) -> Arc<Mutex<Progress>> {
    let shared = Arc::new(Mutex::new(Progress { running: true, ..Default::default() }));
    let (out, f) = (shared.clone(), f.clone());
    let _ = std::thread::Builder::new().name("following".into()).spawn(move || {
        // One refresh at a time (following an account while Refresh runs): two would write the same files.
        static ONE: Mutex<()> = Mutex::new(());
        let _turn = ONE.lock().unwrap_or_else(|e| e.into_inner());
        let set = |who: String, what: String, new: usize| {
            if let Ok(mut p) = out.lock() {
                p.lines.retain(|(w, _)| *w != who);
                p.lines.push((who, what));
                p.new_files += new;
            }
            if let Some(c) = &ctx {
                c.request_repaint();
            }
        };
        let Some(browser) = f.browser.clone() else {
            set("Following".into(), NO_BROWSER.into(), 0);
            if let Ok(mut p) = out.lock() {
                p.running = false;
            }
            return;
        };
        for (a, want) in &wants {
            set(a.label(), "looking…".into(), 0);
            // Posts carry several media, and not all are clips: look a little further than needed.
            let found = gallery_list(&a.profile(), 1, (want * 3).min(MAX_LOOK), &browser).and_then(|b| parse(a, &b));
            match found.and_then(|items| save_newest(a, &items, *want)) {
                Ok((0, kept)) if kept < *want => set(a.label(), format!("that's all ({kept})"), 0),
                Ok((new, kept)) => set(a.label(), if new == 0 { format!("up to date ({kept})") } else { format!("{new} new") }, new),
                Err(e) => set(a.label(), e, 0),
            }
        }
        if let Ok(mut p) = out.lock() {
            p.running = false;
        }
        if let Some(c) = &ctx {
            c.request_repaint();
        }
    });
    shared
}

/// Saves the newest `count` of an account's clips that aren't here yet. Returns (new, how many it has).
fn save_newest(a: &Account, items: &[Item], count: usize) -> Result<(usize, usize), String> {
    let dir = a.dir().ok_or("no downloads folder")?;
    let mut new = 0;
    for item in items.iter().take(count) {
        if super::save_into(item, &dir)?.1 {
            new += 1;
        }
    }
    Ok((new, items.len().min(count)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_names_and_sites_are_read_from_what_was_typed() {
        assert_eq!(handle("@nasa"), Some((None, "nasa".into())));
        assert_eq!(handle("https://x.com/nasa/media"), Some((Some(Site::X), "nasa".into())));
        assert_eq!(handle("instagram.com/some.artist/"), Some((Some(Site::Instagram), "some.artist".into())));
        assert_eq!(handle("not an account!"), None);
    }

    #[test]
    fn x_gifs_and_instagram_videos_come_back() {
        let x = Account::new(Site::X, "giphy");
        let listing = br#"[[3, "https://video.twimg.com/tweet_video/AbC.mp4", {"tweet_id": 5, "num": 1, "type": "animated_gif", "content": "dance loop https://t.co/x"}],
            [3, "https://pbs.twimg.com/media/P.jpg?name=orig", {"tweet_id": 6, "type": "photo"}]]"#;
        let g = parse(&x, listing).unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!((g[0].title.as_str(), g[0].credit.page.as_str()), ("dance loop", "https://x.com/giphy/status/5"));
        let ig = Account::new(Site::Instagram, "artist");
        let listing = br#"[[3, "https://scontent.cdninstagram.com/v.mp4?sig=1", {"post_shortcode": "Cx1", "num": 1, "extension": "mp4", "description": "new loop"}],
            [3, "https://scontent.cdninstagram.com/p.jpg?sig=2", {"post_shortcode": "Cx2", "extension": "jpg"}]]"#;
        let v = parse(&ig, listing).unwrap();
        assert_eq!(v.len(), 1, "pictures are left out");
        assert_eq!(v[0].file.as_deref(), Some("https://scontent.cdninstagram.com/v.mp4?sig=1"), "signed link kept whole");
        assert_eq!(v[0].credit.page, "https://www.instagram.com/p/Cx1/");
        let out = br#"[[-1, {"error": "AuthRequired", "message": "authenticated cookies needed"}]]"#;
        assert!(parse(&ig, out).unwrap_err().contains("Instagram wants you signed in"));
    }

    #[test]
    fn the_old_x_list_carries_over_and_follows_have_no_duplicates() {
        let mut f = Following::default();
        assert!(f.follow(Site::X, "nasa"));
        assert!(!f.follow(Site::X, "nasa"));
        assert!(f.follow(Site::Instagram, "nasa"), "same name, other site");
        assert_eq!(f.accounts[1].label(), "@nasa (Instagram)");
        let back: Following = serde_json::from_str(&serde_json::to_string(&f).unwrap()).unwrap();
        assert_eq!(back, f);
    }

    #[test]
    fn likes_and_saved_lists_have_their_own_folder_and_address() {
        let likes = Account::mine(Site::X, Feed::Likes, "");
        assert_eq!((likes.label().as_str(), likes.profile().as_str()), ("My likes (X)", "https://x.com/i/history/likes"));
        let saved = Account::mine(Site::Instagram, Feed::Saved, "me");
        assert_eq!(saved.profile(), "https://www.instagram.com/me/saved/");
        // A liked post is someone else's: its credit says who.
        let listing = br#"[[3, "https://video.twimg.com/tweet_video/Q.mp4", {"tweet_id": 9, "type": "animated_gif", "content": "loop", "author": {"name": "artist"}}]]"#;
        let g = parse(&likes, listing).unwrap();
        assert_eq!((g[0].credit.author.as_str(), g[0].credit.page.as_str()), ("@artist", "https://x.com/artist/status/9"));
        // Old saved lists (no feed) still read: they're an account's posts.
        let old: Account = serde_json::from_str(r#"{"site":"X","name":"nasa"}"#).unwrap();
        assert_eq!(old, Account::new(Site::X, "nasa"));
    }

    #[test]
    fn keeping_copies_clips_and_credits_once() {
        let base = std::env::temp_dir().join(format!("phosphor-keep-{}", std::process::id()));
        let src = base.join("@a (X)");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("c.mp4"), b"movie").unwrap();
        std::fs::write(src.join("c.mp4.credit.json"), b"{}").unwrap();
        let lib = base.join("lib");
        for _ in 0..2 {
            let done = Arc::new(Mutex::new(None));
            keep(vec![src.join("c.mp4")], lib.clone(), None, done.clone());
            let t = Instant::now();
            while done.lock().unwrap().is_none() && t.elapsed() < Duration::from_secs(5) {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(done.lock().unwrap().as_deref().is_some_and(|m| m.starts_with("Kept")));
        }
        assert_eq!(std::fs::read(lib.join("@a (X)/c.mp4")).unwrap(), b"movie");
        assert!(lib.join("@a (X)/c.mp4.credit.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}