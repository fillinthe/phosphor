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

/// gallery-dl's listing (JSON) of `count` media of `url` (from the start, or from `cursor`: where an earlier listing
/// got to), using `browser`'s sign-in, and the page cursors it reported on the way (`resume_from` picks one). At most
/// two minutes.
pub fn gallery_list(url: &str, count: usize, browser: &str, cursor: Option<&str>) -> Result<(Vec<u8>, Vec<String>), String> {
    let tool = crate::web_media::find_gallery_dl().ok_or(NO_TOOL)?;
    let mut cmd = Command::new(tool);
    // -v: gallery-dl says "Cursor: …" for each page it asks for (debug lines, on stderr).
    cmd.args(["-j", "-v", "--range", &format!("1-{}", count.max(1)), "--cookies-from-browser", browser]);
    if let Some(c) = cursor {
        cmd.args(["-o", &format!("cursor={c}")]);
    }
    let mut child = cmd.arg(url).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(|e| format!("couldn't start gallery-dl: {e}"))?;
    // Both outputs are read on threads so a full pipe never stalls it; it's stopped if it takes too long.
    let mut out = child.stdout.take().ok_or("no output")?;
    let err = child.stderr.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut b);
        b
    });
    let cursors = std::thread::spawn(move || {
        use std::io::BufRead;
        std::io::BufReader::new(err).lines().map_while(Result::ok).filter_map(|l| l.split_once("Cursor: ").map(|(_, c)| c.trim().to_string())).filter(|c| !c.is_empty()).collect::<Vec<String>>()
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
    let body = reader.join().map_err(|_| "gallery-dl's output was lost".to_string())?;
    Ok((body, cursors.join().unwrap_or_default()))
}

/// Where the next "Load more" should start: the cursor of the last page a listing began (not the one after it: the
/// listing may have stopped partway through that page, and starting after it would skip the rest of it).
pub fn resume_from(cursors: &[String]) -> Option<String> {
    match cursors {
        [] => None,
        [only] => Some(only.clone()),
        [.., before_last, _] => Some(before_last.clone()),
    }
}

/// How many media a listing held (any kind): none at all means the account has nothing older.
pub fn media_count(body: &[u8]) -> usize {
    serde_json::from_slice::<Value>(body).ok().and_then(|v| v.as_array().map(|a| a.iter().filter(|e| e.get(0).and_then(Value::as_i64) == Some(3)).count())).unwrap_or(0)
}

/// Each account's place for "Load more" (its own file, written only by the fetch thread: it can't race with the
/// settings the panel saves).
fn cursors_file() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("following_cursors.json"))
}

fn load_cursors() -> std::collections::HashMap<String, String> {
    cursors_file().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
}

fn save_cursors(c: &std::collections::HashMap<String, String>) {
    if let Some(p) = cursors_file() {
        let _ = std::fs::write(p, serde_json::to_vec_pretty(c).unwrap_or_default());
    }
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
    let wants = f.accounts.iter().map(|a| (a.clone(), Fetch::Newest)).collect();
    fetch_accounts(f, wants, ctx)
}

/// Load more: the next older clips of one account, from where the last listing got to.
pub fn more(f: &Following, account: &Account, ctx: Option<eframe::egui::Context>) -> Arc<Mutex<Progress>> {
    fetch_accounts(f, vec![(account.clone(), Fetch::More)], ctx)
}

/// What a fetch is for.
#[derive(Clone, Copy, PartialEq)]
enum Fetch {
    /// The newest clips (Refresh, a new follow).
    Newest,
    /// The next page of older posts (Load more).
    More,
}

/// Media looked at for the newest clips (posts carry photos and videos too, so a few times what's kept).
const NEWEST_LOOK: usize = 60;
/// One "Load more" page: the same size every time (it continues where the last one stopped, never from the top).
const PAGE: usize = 150;

fn fetch_accounts(f: &Following, wants: Vec<(Account, Fetch)>, ctx: Option<eframe::egui::Context>) -> Arc<Mutex<Progress>> {
    let shared = Arc::new(Mutex::new(Progress { running: true, ..Default::default() }));
    let (out, f) = (shared.clone(), f.clone());
    let _ = std::thread::Builder::new().name("following".into()).spawn(move || {
        // One fetch at a time (following an account while Refresh runs): two would write the same files.
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
        let mut cursors = load_cursors();
        for (a, fetch) in &wants {
            set(a.label(), "looking…".into(), 0);
            let from = if *fetch == Fetch::More { cursors.get(&a.label()).cloned() } else { None };
            let look = if *fetch == Fetch::More { PAGE } else { (f.per_account * 5).max(NEWEST_LOOK) };
            let listed = gallery_list(&a.profile(), look, &browser, from.as_deref());
            let result = listed.and_then(|(body, pages)| {
                let media = media_count(&body);
                let items = parse(a, &body)?;
                // Where the next Load more starts (a Refresh only sets it the first time: it may be further on).
                if let Some(next) = resume_from(&pages)
                    && (*fetch == Fetch::More || !cursors.contains_key(&a.label()))
                {
                    cursors.insert(a.label(), next);
                }
                let new = save_new(a, &items, f.per_account)?;
                Ok((media, new))
            });
            match result {
                Ok((0, _)) if *fetch == Fetch::More => set(a.label(), "that's all: nothing older".into(), 0),
                Ok((_, 0)) if *fetch == Fetch::More => set(a.label(), "no new clips on that page: press Load more again".into(), 0),
                Ok((_, 0)) => set(a.label(), "up to date".into(), 0),
                Ok((_, new)) => set(a.label(), format!("{new} new"), new),
                Err(e) => set(a.label(), e, 0),
            }
        }
        save_cursors(&cursors);
        if let Ok(mut p) = out.lock() {
            p.running = false;
        }
        if let Some(c) = &ctx {
            c.request_repaint();
        }
    });
    shared
}

/// Saves up to `count` of these clips that aren't here yet (in listing order: newest first). Returns how many.
fn save_new(a: &Account, items: &[Item], count: usize) -> Result<usize, String> {
    let dir = a.dir().ok_or("no downloads folder")?;
    let mut new = 0;
    for item in items {
        if new >= count {
            break;
        }
        if super::save_into(item, &dir)?.1 {
            new += 1;
        }
    }
    Ok(new)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Load more starts at the last page a listing began (so a page cut short isn't skipped), and an empty listing
    /// means there's nothing older.
    #[test]
    fn load_more_continues_where_the_last_listing_got_to() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(resume_from(&s(&[])), None);
        assert_eq!(resume_from(&s(&["p2"])), Some("p2".into()));
        assert_eq!(resume_from(&s(&["p2", "p3", "p4"])), Some("p3".into()));
        assert_eq!(media_count(br#"[[2, {}], [3, "https://a/1.mp4", {}], [3, "https://a/2.jpg", {}]]"#), 2);
        assert_eq!(media_count(b"[]"), 0);
    }

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