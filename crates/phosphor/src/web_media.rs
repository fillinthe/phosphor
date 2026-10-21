//! Media from a link: the video or GIF in a post on X, Instagram, TikTok, YouTube, Vimeo… pulled out with yt-dlp
//! (open source, Unlicense; Homebrew or its own download). Download = the file lands in Phosphor's `downloads`
//! folder and the media bin (a normal clip: loops, works offline); Play from the link = the site's direct video
//! link as a live layer (nothing saved; those links expire after a while). Instagram often needs a sign-in: yt-dlp
//! can use the browser's (only when you pick one). All of it runs off the UI thread.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc;

/// Browsers yt-dlp can take a sign-in from (`--cookies-from-browser`).
pub const BROWSERS: [&str; 5] = ["safari", "chrome", "brave", "firefox", "edge"];

/// yt-dlp: Phosphor's own copy, Homebrew's, or one on the PATH.
pub fn find_ytdlp() -> Option<PathBuf> {
    let own = crate::show::storage_dir().map(|d| d.join("bin").join("yt-dlp"));
    let fixed = ["/opt/homebrew/bin/yt-dlp", "/usr/local/bin/yt-dlp", "/opt/local/bin/yt-dlp"].map(PathBuf::from);
    own.into_iter().chain(fixed).find(|p| p.exists()).or_else(|| {
        Command::new("yt-dlp").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().ok().filter(|s| s.success()).map(|_| PathBuf::from("yt-dlp"))
    })
}

/// Where downloads go.
pub fn downloads_dir() -> Option<PathBuf> {
    crate::show::storage_dir().map(|d| d.join("downloads"))
}

/// What a link job is doing.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Working(String),
    /// Downloaded files (a post can hold several videos).
    Downloaded(Vec<String>),
    /// A direct video link to play live, and a name for it.
    Playable { url: String, name: String },
    Failed(String),
}

/// The arguments for a download: best MP4 video with its sound (merged by `ffmpeg`), every video in the post, the
/// file named after the account and post, each finished file's path printed on its own line.
pub fn download_args(url: &str, dir: &str, browser: Option<&str>, ffmpeg: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "--newline".into(),
        "--no-mtime".into(),
        "-f".into(),
        "bv*[ext=mp4]+ba[ext=m4a]/b[ext=mp4]/bv*+ba/b".into(),
        "--merge-output-format".into(),
        "mp4".into(),
        "-o".into(),
        format!("{dir}/%(uploader_id,uploader|link)s-%(id)s-%(autonumber)s.%(ext)s"),
        "--print".into(),
        "after_move:filepath".into(),
        "--progress-template".into(),
        "download:PROGRESS %(progress._percent_str)s".into(),
    ];
    if let Some(f) = ffmpeg {
        a.extend(["--ffmpeg-location".into(), f.to_string()]);
    }
    if let Some(b) = browser {
        a.extend(["--cookies-from-browser".into(), b.to_string()]);
    }
    a.push(url.to_string());
    a
}

/// Reads one line of yt-dlp's output during a download: progress, a finished file, or nothing.
pub fn read_line(line: &str) -> Option<Status> {
    let line = line.trim();
    if let Some(p) = line.strip_prefix("PROGRESS") {
        return Some(Status::Working(format!("downloading {}", p.trim())));
    }
    (line.starts_with('/') || line.contains(":\\")).then(|| Status::Downloaded(vec![line.to_string()]))
}

/// The short reason from yt-dlp's errors ("ERROR: [twitter] …: No video could be found in this tweet").
fn reason(stderr: &str) -> String {
    let e = stderr.lines().rev().find(|l| l.contains("ERROR")).unwrap_or("it didn't work").trim();
    let e = e.trim_start_matches("ERROR:").trim();
    if e.contains("login") || e.contains("cookies") || e.contains("rate-limit") {
        format!("{e} (try \"Use my browser's sign-in\")")
    } else {
        e.to_string()
    }
}

/// One link job on its own thread; `poll` gives its updates.
pub struct Job {
    rx: mpsc::Receiver<Status>,
    pub status: Status,
}

impl Job {
    /// Downloads the post's videos into `downloads_dir`.
    pub fn download(url: &str, browser: Option<&str>) -> Job {
        let (tx, rx) = mpsc::channel();
        let (u, b) = (url.trim().to_string(), browser.map(str::to_string));
        std::thread::spawn(move || {
            let send = |s| {
                let _ = tx.send(s);
            };
            let (Some(tool), Some(dir)) = (find_ytdlp(), downloads_dir()) else {
                send(Status::Failed("yt-dlp isn't installed (see the hint below)".into()));
                return;
            };
            if std::fs::create_dir_all(&dir).is_err() {
                send(Status::Failed("can't make the downloads folder".into()));
                return;
            }
            let ffmpeg = crate::stream::ffmpeg_path();
            let args = download_args(&u, &dir.to_string_lossy(), b.as_deref(), ffmpeg.as_deref());
            let child = Command::new(tool).args(args).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn();
            let Ok(mut child) = child else {
                send(Status::Failed("couldn't start yt-dlp".into()));
                return;
            };
            let stderr = child.stderr.take();
            let errs = std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(mut e) = stderr {
                    use std::io::Read;
                    let _ = e.read_to_string(&mut s);
                }
                s
            });
            let mut files = Vec::new();
            if let Some(out) = child.stdout.take() {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    match read_line(&line) {
                        Some(Status::Downloaded(f)) => files.extend(f),
                        Some(s) => send(s),
                        None => {}
                    }
                }
            }
            let ok = child.wait().is_ok_and(|s| s.success());
            let err = errs.join().unwrap_or_default();
            files.retain(|f| std::path::Path::new(f).exists());
            send(if !files.is_empty() { Status::Downloaded(files) } else if ok { Status::Failed("no video in that post".into()) } else { Status::Failed(reason(&err)) });
        });
        Job { rx, status: Status::Working("starting…".into()) }
    }

    /// Finds the post's direct video link (to play live, nothing saved).
    pub fn stream(url: &str, browser: Option<&str>) -> Job {
        let (tx, rx) = mpsc::channel();
        let (u, b) = (url.trim().to_string(), browser.map(str::to_string));
        std::thread::spawn(move || {
            let Some(tool) = find_ytdlp() else {
                let _ = tx.send(Status::Failed("yt-dlp isn't installed (see the hint below)".into()));
                return;
            };
            let mut args: Vec<String> = vec!["-f".into(), "b[ext=mp4]/b".into(), "--print".into(), "%(uploader_id,uploader|link)s|%(url)s".into()];
            if let Some(b) = b {
                args.extend(["--cookies-from-browser".into(), b]);
            }
            args.push(u);
            let out = Command::new(tool).args(args).output();
            let status = match out {
                Ok(o) if o.status.success() => {
                    let text = String::from_utf8_lossy(&o.stdout).to_string();
                    match text.lines().next().and_then(|l| l.split_once('|')) {
                        Some((name, url)) if url.starts_with("http") => Status::Playable { url: url.to_string(), name: name.to_string() },
                        _ => Status::Failed("no video in that post".into()),
                    }
                }
                Ok(o) => Status::Failed(reason(&String::from_utf8_lossy(&o.stderr))),
                Err(_) => Status::Failed("couldn't start yt-dlp".into()),
            };
            let _ = tx.send(status);
        });
        Job { rx, status: Status::Working("finding the video…".into()) }
    }

    /// Takes the latest update. Returns true when the job just finished (downloaded, playable or failed).
    pub fn poll(&mut self) -> bool {
        let mut finished = false;
        for s in self.rx.try_iter() {
            finished = !matches!(s, Status::Working(_));
            self.status = s;
        }
        finished
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_download_asks_for_mp4_with_sound_and_prints_each_file() {
        let a = download_args("https://x.com/a/status/1", "/tmp/dl", Some("safari"), Some("/x/ffmpeg"));
        assert!(a.windows(2).any(|w| w[0] == "--cookies-from-browser" && w[1] == "safari"));
        assert!(a.windows(2).any(|w| w[0] == "--ffmpeg-location" && w[1] == "/x/ffmpeg"));
        assert!(a.windows(2).any(|w| w[0] == "--print" && w[1] == "after_move:filepath"));
        assert_eq!(a.last().unwrap(), "https://x.com/a/status/1");
        assert!(!download_args("u", "/d", None, None).iter().any(|x| x == "--cookies-from-browser"), "no sign-in unless asked");
    }

    #[test]
    fn progress_and_files_are_read_from_the_output() {
        assert_eq!(read_line("PROGRESS  42.0%"), Some(Status::Working("downloading 42.0%".into())));
        assert_eq!(read_line("/Users/me/dl/psaltix-1-00001.mp4"), Some(Status::Downloaded(vec!["/Users/me/dl/psaltix-1-00001.mp4".into()])));
        assert_eq!(read_line("[twitter] Extracting URL"), None);
        assert!(reason("WARNING: x\nERROR: [Instagram] abc: Requested content is not available, rate-limit reached or login required").contains("browser's sign-in"));
    }
}
