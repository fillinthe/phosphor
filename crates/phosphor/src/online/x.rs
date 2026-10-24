//! GIFs from X accounts (Online › GIFs from X): an account's animated GIFs to browse and bring in, without
//! downloading them yourself. X has no free way to list an account's posts, so this uses gallery-dl (free, the same
//! kind of helper as yt-dlp for "From a link…") with your browser's X sign-in. X keeps GIFs as small MP4s
//! (video.twimg.com), which download without a sign-in and loop like any clip.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Item, Kind, Source};

/// Posts' media looked at per page (only the GIFs among them are shown).
const PER_PAGE: usize = 100;

/// Saved accounts and the browser whose X sign-in gallery-dl uses (kept in Phosphor's settings folder).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct XSettings {
    pub accounts: Vec<String>,
    /// One of `web_media::BROWSERS`.
    pub browser: Option<String>,
}

fn settings_file() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("x_accounts.json"))
}

impl XSettings {
    pub fn load() -> XSettings {
        settings_file().and_then(|p| std::fs::read(p).ok()).and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default()
    }

    pub fn save(&self) {
        if let Some(p) = settings_file() {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            let _ = std::fs::write(p, serde_json::to_vec_pretty(self).unwrap_or_default());
        }
    }
}

/// An account name from what was typed: "@nasa", "nasa", "x.com/nasa", "https://twitter.com/nasa/media" → "nasa".
pub fn handle(typed: &str) -> Option<String> {
    let t = typed.trim().trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    let t = t.strip_prefix("x.com/").or_else(|| t.strip_prefix("twitter.com/")).unwrap_or(t);
    let h = t.trim_start_matches('@').split(['/', '?']).next().unwrap_or("");
    (!h.is_empty() && h.len() <= 15 && h.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')).then(|| h.to_string())
}

/// gallery-dl's listing of an account's media (posts `page` × `PER_PAGE`), as its JSON. Runs for at most two minutes.
pub fn list(handle: &str, page: usize) -> Result<Vec<u8>, String> {
    let tool = crate::web_media::find_gallery_dl().ok_or(NO_TOOL)?;
    let settings = XSettings::load();
    let Some(browser) = settings.browser else { return Err(NO_BROWSER.into()) };
    let first = (page.max(1) - 1) * PER_PAGE + 1;
    let mut child = Command::new(tool)
        .args(["-j", "--range", &format!("{first}-{}", first + PER_PAGE - 1), "--cookies-from-browser", &browser])
        .arg(format!("https://x.com/{handle}/media"))
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
            return Err("X took too long to answer: try again".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    reader.join().map_err(|_| "gallery-dl's output was lost".to_string())
}

pub const NO_TOOL: &str = "Needs gallery-dl (free): in Terminal, brew install gallery-dl";
pub const NO_BROWSER: &str = "Pick the browser you're signed in to X with (X only shows an account's posts to people signed in)";

/// The animated GIFs in gallery-dl's listing (`[3, url, info]` entries whose info says `animated_gif`).
pub fn parse(handle: &str, body: &[u8]) -> Result<Vec<Item>, String> {
    let v: Value = serde_json::from_slice(body).map_err(|_| "gallery-dl sent something unreadable".to_string())?;
    let entries = v.as_array().ok_or("gallery-dl sent something unreadable")?;
    let mut out = Vec::new();
    for e in entries {
        let kind = e.get(0).and_then(Value::as_i64).unwrap_or(0);
        if kind == -1 {
            let msg = e[1]["message"].as_str().unwrap_or("");
            let err = e[1]["error"].as_str().unwrap_or("");
            return Err(if err == "AuthRequired" || msg.contains("cookies") {
                "X wants you signed in: sign in to X in the browser picked here, then try again".into()
            } else if msg.is_empty() {
                format!("X said no ({err})")
            } else {
                format!("X said no: {msg}")
            });
        }
        if kind != 3 {
            continue;
        }
        let (Some(url), info) = (e.get(1).and_then(Value::as_str), &e[2]) else { continue };
        if info["type"].as_str() != Some("animated_gif") || !url.starts_with("https://") {
            continue;
        }
        let post = info["tweet_id"].as_u64().map(|n| n.to_string()).or_else(|| info["tweet_id"].as_str().map(str::to_string)).unwrap_or_default();
        let num = info["num"].as_u64().unwrap_or(1);
        let words: String = info["content"].as_str().unwrap_or("").split_whitespace().filter(|w| !w.starts_with("https://t.co/")).collect::<Vec<_>>().join(" ");
        let title = if words.is_empty() { format!("GIF from @{handle}") } else { words.chars().take(60).collect() };
        let author = info["author"]["name"].as_str().or(info["user"]["name"].as_str()).unwrap_or(handle).to_string();
        out.push(Item {
            source: Source::X,
            kind: Kind::Gif,
            id: format!("{post}-{num}"),
            title: title.clone(),
            thumb: thumb_of(url),
            file: Some(url.split('?').next().unwrap_or(url).to_string()),
            list: String::new(),
            size: None,
            credit: phosphor_engine::Credit {
                title,
                author: format!("@{author}"),
                licence: "Unknown (posted on X: ask before showing it in public)".into(),
                licence_url: String::new(),
                page: format!("https://x.com/{handle}/status/{post}"),
                source: "X".into(),
            },
        });
    }
    Ok(out)
}

/// X's still picture for a GIF: ".../tweet_video/ABC.mp4" → ".../tweet_video_thumb/ABC.jpg".
fn thumb_of(url: &str) -> String {
    let name = url.split('?').next().unwrap_or(url).rsplit('/').next().unwrap_or("").trim_end_matches(".mp4");
    if url.contains("/tweet_video/") && !name.is_empty() { format!("https://pbs.twimg.com/tweet_video_thumb/{name}.jpg") } else { String::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_names_are_read_from_what_was_typed() {
        for t in ["@nasa", "nasa", "x.com/nasa", "https://twitter.com/nasa/media", "https://www.x.com/nasa?lang=en"] {
            assert_eq!(handle(t).as_deref(), Some("nasa"), "{t}");
        }
        assert_eq!(handle("not an account!"), None);
        assert_eq!(handle("@way_too_long_for_an_x_name"), None);
    }

    #[test]
    fn only_the_gifs_come_back() {
        let listing = br#"[
            [2, {"tweet_id": 5}],
            [3, "https://video.twimg.com/tweet_video/AbC123.mp4", {"tweet_id": 5, "num": 1, "type": "animated_gif", "content": "dance loop https://t.co/xyz", "author": {"name": "giphy"}}],
            [3, "https://pbs.twimg.com/media/Photo.jpg?name=orig", {"tweet_id": 6, "num": 1, "type": "photo"}],
            [3, "https://video.twimg.com/ext_tw_video/1/pu/vid/720x720/V.mp4", {"tweet_id": 7, "num": 1, "type": "video"}]
        ]"#;
        let gifs = parse("giphy", listing).unwrap();
        assert_eq!(gifs.len(), 1, "photos and videos are left out");
        let g = &gifs[0];
        assert_eq!((g.title.as_str(), g.kind), ("dance loop", Kind::Gif));
        assert_eq!(g.thumb, "https://pbs.twimg.com/tweet_video_thumb/AbC123.jpg");
        assert_eq!(g.credit.page, "https://x.com/giphy/status/5");
        assert!(!g.credit.commercial());
        let signed_out = br#"[[-1, {"error": "AuthRequired", "message": "authenticated cookies needed to access this timeline"}]]"#;
        assert!(parse("giphy", signed_out).unwrap_err().contains("signed in"));
    }

    #[test]
    fn settings_survive_a_save() {
        let s = XSettings { accounts: vec!["nasa".into()], browser: Some("safari".into()) };
        let back: XSettings = serde_json::from_str(&serde_json::to_string(&s).unwrap()).unwrap();
        assert_eq!(back, s);
    }
}
