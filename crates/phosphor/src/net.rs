//! LAN session: a tiny HTTP server that streams the output as MJPEG to any browser (laptop,
//! phone, Raspberry Pi) and takes viewers' microphone audio back over a WebSocket, so the
//! visuals can react to the music where *they* are. WebRTC can replace the transport later.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex};

pub const DEFAULT_PORT: u16 = 8777;
const MAX_WIDTH: u32 = 1280;

type Frame = Arc<Vec<u8>>;

/// What a connected device is doing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PeerKind {
    Remote,
    Viewer,
    Audio,
    /// Audience member on the play page (takes turns; see `app::play`).
    Guest,
    /// Another Phosphor playing this show on its own outputs (`crate::follow`).
    Follower,
    /// Another Phosphor taking a show this one sends (`crate::show_send`).
    Receiver,
}

/// A phone/tablet/browser connected to the session.
#[derive(Clone, Debug)]
pub struct Peer {
    pub id: u64,
    pub kind: PeerKind,
    pub ip: String,
    pub device: String,
    pub since: web_time::Instant,
    /// Remote unlocked (or no password set).
    pub controlling: bool,
    /// Guests: the id their browser remembers, so a reconnect is the same person.
    pub token: String,
    /// Guests: the name they typed (shown in the panel and, if on, on the output).
    pub name: String,
    /// Audio senders: the level of what they're sending (0..1, smoothed), for the Devices panel meter.
    pub level: f32,
}

/// Everyone connected, plus disconnect requests and blocked addresses.
#[derive(Default)]
pub struct Registry {
    pub peers: Vec<Peer>,
    pub kicked: std::collections::HashSet<u64>,
    pub blocked: std::collections::BTreeSet<String>,
    next: u64,
    /// Wrong passwords by address (remote and following).
    guesses: Guesses,
    /// Open connections: in all, and by address (`Entry`).
    open: usize,
    open_by_ip: std::collections::HashMap<String, usize>,
    /// Guest ids this session handed out (a phone can only come back as one of these, never pick its own).
    issued: std::collections::HashSet<String>,
}

pub type Reg = Arc<Mutex<Registry>>;

/// Most connections the session serves at once, and from one address (a phone's page uses a few).
const MAX_CONNECTIONS: usize = 256;
const MAX_PER_ADDRESS: usize = 32;
/// Most guest ids remembered in one session.
const MAX_ISSUED: usize = 10_000;

/// Wrong passwords from one address: after each, that address waits longer before it may try again (half a
/// second, doubling, at most a minute), however many connections it opens. A right one clears it.
#[derive(Default)]
struct Guesses {
    by_ip: std::collections::HashMap<String, (u32, web_time::Instant)>,
}

impl Guesses {
    fn may_try(&self, ip: &str, now: web_time::Instant) -> bool {
        self.by_ip.get(ip).is_none_or(|(_, at)| now >= *at)
    }

    fn failed(&mut self, ip: &str, now: web_time::Instant) {
        if self.by_ip.len() > 1000 {
            self.by_ip.retain(|_, (_, at)| *at > now);
        }
        let e = self.by_ip.entry(ip.to_string()).or_insert((0, now));
        e.0 += 1;
        e.1 = now + std::time::Duration::from_millis((500u64 << (e.0 - 1).min(7)).min(60_000));
    }
}

impl Registry {
    /// Checks a password `tried` from `ip` against `current`, slowing down an address that keeps guessing
    /// (while it must wait, every try is wrong).
    fn try_password(&mut self, ip: &str, tried: &str, current: &str) -> bool {
        let now = web_time::Instant::now();
        if !self.guesses.may_try(ip, now) {
            return false;
        }
        if tried == current {
            self.guesses.by_ip.remove(ip);
            true
        } else {
            self.guesses.failed(ip, now);
            false
        }
    }

    /// The id a guest plays under: the one it asked for if this session gave it out, else a `fresh` one.
    fn claim(&mut self, asked: &str, fresh: impl FnOnce() -> String) -> String {
        if !asked.is_empty() && self.issued.contains(asked) {
            return asked.to_string();
        }
        let token = fresh();
        if self.issued.len() < MAX_ISSUED {
            self.issued.insert(token.clone());
        }
        token
    }
}

/// An open connection, counted while it lives (`Entry::open`); None when the session is full.
struct Entry {
    reg: Reg,
    ip: String,
}

impl Entry {
    fn open(reg: &Reg, ip: &str) -> Option<Self> {
        let mut r = reg.lock().unwrap();
        let mine = r.open_by_ip.get(ip).copied().unwrap_or(0);
        if r.open >= MAX_CONNECTIONS || mine >= MAX_PER_ADDRESS {
            return None;
        }
        r.open += 1;
        r.open_by_ip.insert(ip.to_string(), mine + 1);
        Some(Self { reg: reg.clone(), ip: ip.to_string() })
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        let mut r = self.reg.lock().unwrap();
        r.open = r.open.saturating_sub(1);
        if let Some(n) = r.open_by_ip.get_mut(&self.ip) {
            *n -= 1;
            if *n == 0 {
                r.open_by_ip.remove(&self.ip);
            }
        }
    }
}

/// Commands from one guest: at most this many a second on average, this many at once.
const GUEST_RATE: f32 = 120.0;
const GUEST_BURST: f32 = 240.0;

/// A token bucket: lets `rate` things a second through, `burst` at once.
struct Bucket {
    tokens: f32,
    last: web_time::Instant,
}

impl Bucket {
    fn new() -> Self {
        Self { tokens: GUEST_BURST, last: web_time::Instant::now() }
    }

    fn take(&mut self, now: web_time::Instant) -> bool {
        self.tokens = (self.tokens + now.saturating_duration_since(self.last).as_secs_f32() * GUEST_RATE).min(GUEST_BURST);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

/// Most commands waiting for the app (it takes them every frame; more means it stalled or someone floods).
const MAX_QUEUED: usize = 4096;

/// One header's value from a request head (name without the colon, any case).
fn header<'a>(request: &'a str, name: &str) -> Option<&'a str> {
    request.lines().skip(1).find_map(|l| {
        let (k, v) = l.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

/// A request a web page on another site made (its Origin isn't this server): such a page could drive the show
/// from any browser on the network. Requests without an Origin (our own app, other programs) are fine.
fn cross_site(request: &str) -> bool {
    let Some(origin) = header(request, "Origin") else { return false };
    let Some(host) = origin.strip_prefix("http://").or_else(|| origin.strip_prefix("https://")) else { return true };
    header(request, "Host").is_none_or(|h| !h.eq_ignore_ascii_case(host.trim_end_matches('/')))
}

#[cfg(test)]
mod limit_tests {
    use super::*;

    #[test]
    fn guessing_passwords_from_one_address_slows_down() {
        let mut r = Registry::default();
        assert!(!r.try_password("10.0.0.9", "nope", "secret"));
        // Straight away, even the right password is refused for that address (another address may try).
        assert!(!r.try_password("10.0.0.9", "secret", "secret"));
        assert!(r.try_password("10.0.0.8", "secret", "secret"));
        let now = web_time::Instant::now();
        for _ in 0..20 {
            r.guesses.failed("10.0.0.9", now);
        }
        let (_, at) = r.guesses.by_ip["10.0.0.9"];
        assert!(at - now <= std::time::Duration::from_secs(60), "never more than a minute");
        assert!(!r.guesses.may_try("10.0.0.9", now + std::time::Duration::from_secs(59)));
        assert!(r.guesses.may_try("10.0.0.9", now + std::time::Duration::from_secs(61)));
    }

    #[test]
    fn connections_are_capped_in_all_and_per_address() {
        let reg: Reg = Default::default();
        let mine: Vec<Entry> = (0..MAX_PER_ADDRESS).map_while(|_| Entry::open(&reg, "10.0.0.2")).collect();
        assert_eq!(mine.len(), MAX_PER_ADDRESS);
        assert!(Entry::open(&reg, "10.0.0.2").is_none(), "one address can't take every thread");
        assert!(Entry::open(&reg, "10.0.0.3").is_some());
        drop(mine);
        assert!(Entry::open(&reg, "10.0.0.2").is_some(), "closed connections free their place");
        let all: Vec<Entry> = (0..MAX_CONNECTIONS * 2).filter_map(|i| Entry::open(&reg, &format!("10.0.{}.{}", i / 200, i % 200))).collect();
        assert_eq!(all.len(), MAX_CONNECTIONS);
        drop(all);
        assert_eq!(reg.lock().unwrap().open, 0);
        assert!(reg.lock().unwrap().open_by_ip.is_empty());
    }

    #[test]
    fn a_guest_only_gets_an_id_the_session_gave_out() {
        let mut r = Registry::default();
        let first = r.claim("made-up-by-the-phone", || "server1".into());
        assert_eq!(first, "server1", "a phone can't choose who it is");
        assert_eq!(r.claim("server1", || "server2".into()), "server1", "a reconnect keeps its place");
        assert_eq!(r.claim("", || "server3".into()), "server3");
    }

    #[test]
    fn a_guest_flooding_commands_is_held_to_a_rate() {
        let mut b = Bucket::new();
        let now = b.last;
        let through = (0..1000).filter(|_| b.take(now)).count();
        assert_eq!(through, GUEST_BURST as usize);
        let later = now + std::time::Duration::from_millis(500);
        let through = (0..1000).filter(|_| b.take(later)).count();
        assert_eq!(through, (GUEST_RATE / 2.0) as usize);
    }

    #[test]
    fn pages_from_other_sites_cant_drive_the_show() {
        let req = |origin: &str| format!("GET /control HTTP/1.1\r\nHost: 192.168.1.5:8777\r\n{origin}Upgrade: websocket\r\n\r\n");
        assert!(!cross_site(&req("")), "our app and other programs send no Origin");
        assert!(!cross_site(&req("Origin: http://192.168.1.5:8777\r\n")), "the remote page itself");
        assert!(!cross_site(&req("origin: HTTP://192.168.1.5:8777\r\n".to_lowercase().as_str())));
        assert!(cross_site(&req("Origin: http://evil.example\r\n")));
        assert!(cross_site(&req("Origin: null\r\n")));
        assert!(cross_site(&req("Origin: http://192.168.1.5:8778\r\n")), "another port is another site");
        let https = "GET /control HTTP/1.1\r\nhost: phosphor.local:8778\r\nOrigin: https://phosphor.local:8778\r\n\r\n";
        assert!(!cross_site(https));
    }
}

/// Registers a connection for as long as it lives; leaving happens on drop.
struct Seat {
    reg: Reg,
    id: u64,
    ip: String,
}

impl Seat {
    /// Audio senders: remember how loud the latest chunk was (peaks snap up, then ease down).
    fn set_level(&self, rms: f32) {
        if let Ok(mut r) = self.reg.lock()
            && let Some(p) = r.peers.iter_mut().find(|p| p.id == self.id)
        {
            let v = (rms * 4.0).min(1.0);
            p.level = if v > p.level { v } else { p.level * 0.8 + v * 0.2 };
        }
    }

    fn join(reg: &Reg, kind: PeerKind, ip: &str, device: &str) -> Self {
        let mut r = reg.lock().unwrap();
        r.next += 1;
        let id = r.next;
        r.peers.push(Peer { id, kind, ip: ip.to_string(), device: device.to_string(), since: web_time::Instant::now(), controlling: false, token: String::new(), name: String::new(), level: 0.0 });
        Self { reg: reg.clone(), id, ip: ip.to_string() }
    }

    /// A password this connection tried (see `Registry::try_password`).
    fn try_password(&self, tried: &str, current: &str) -> bool {
        self.reg.lock().unwrap().try_password(&self.ip, tried, current)
    }

    fn kicked(&self) -> bool {
        self.reg.lock().unwrap().kicked.contains(&self.id)
    }

    /// A guest's hello: the id it asked to come back as (kept only if this session gave it out, else a `fresh`
    /// one) and its name. Returns the id it plays under (the phone keeps it for next time).
    fn set_token(&self, asked: &str, name: &str, fresh: impl FnOnce() -> String) -> String {
        let mut r = self.reg.lock().unwrap();
        let token = r.claim(asked, fresh);
        if let Some(p) = r.peers.iter_mut().find(|p| p.id == self.id) {
            p.token.clone_from(&token);
            p.name = name.chars().filter(|c| !c.is_control()).take(24).collect::<String>().trim().to_string();
        }
        token
    }

    fn set_controlling(&self, on: bool) {
        if let Some(p) = self.reg.lock().unwrap().peers.iter_mut().find(|p| p.id == self.id) {
            p.controlling = on;
        }
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        let mut r = self.reg.lock().unwrap();
        r.peers.retain(|p| p.id != self.id);
        r.kicked.remove(&self.id);
    }
}

/// The sample rate a phone says its microphone sends, kept to rates a microphone has (a zero or a wild number
/// would make the resampler ask for endless sound).
fn audio_rate(text: &str) -> f32 {
    text.trim().parse::<f32>().ok().filter(|r| r.is_finite()).map_or(48_000.0, |r| r.clamp(8_000.0, 192_000.0))
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    #[test]
    fn the_output_is_captured_only_while_someone_watches() {
        let mut s = NetSession::new();
        assert!(!s.frame_due(1.0), "not running");
        s.running.store(true, Ordering::Relaxed);
        assert!(!s.frame_due(2.0), "running, nobody watching: nothing read back");
        s.viewers.store(1, Ordering::Relaxed);
        assert!(s.frame_due(3.0), "an MJPEG viewer");
        s.viewers.store(0, Ordering::Relaxed);
        assert!(!s.frame_due(4.0));
        s.webrtc.viewers.store(1, Ordering::Relaxed);
        assert!(s.frame_due(5.0), "a WebRTC viewer");
        s.webrtc.viewers.store(0, Ordering::Relaxed);
        s.snapshot_asked.store(true, Ordering::Relaxed);
        assert!(s.frame_due(6.0), "a snapshot waiting");
        // The frame goes to the JPEG thread (none here: dropped) and the request stays until one is taken.
        s.push_frame(2, 2, vec![0; 16]);
        assert!(s.snapshot_asked.load(Ordering::Relaxed));
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        s.encoder = Some(tx);
        s.push_frame(2, 2, vec![0; 16]);
        assert!(!s.snapshot_asked.load(Ordering::Relaxed), "taken for the snapshot");
        assert_eq!(rx.try_recv().map(|(w, h, px)| (w, h, px.len())), Ok((2, 2, 16)));
        assert!(!s.frame_due(7.0));
        // Nobody watching: a frame pushed anyway isn't encoded.
        s.push_frame(2, 2, vec![0; 16]);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn a_snapshot_waits_for_a_fresh_picture_or_takes_the_last() {
        let latest: Shared = Arc::new((Mutex::new(None), Condvar::new()));
        let asked = Arc::new(AtomicBool::new(false));
        assert!(fresh_snapshot(&latest, &asked, std::time::Duration::from_millis(20)).is_none(), "nothing yet");
        assert!(asked.load(Ordering::Relaxed), "a picture was asked for");
        let old: Frame = Arc::new(vec![1]);
        *latest.0.lock().unwrap() = Some(old.clone());
        let stale = fresh_snapshot(&latest, &asked, std::time::Duration::from_millis(20)).unwrap();
        assert!(Arc::ptr_eq(&stale, &old), "none came: the last one");
        let (l, a) = (latest.clone(), asked.clone());
        let painter = std::thread::spawn(move || {
            while !a.load(Ordering::Relaxed) {
                std::thread::yield_now();
            }
            std::thread::sleep(std::time::Duration::from_millis(30));
            *l.0.lock().unwrap() = Some(Arc::new(vec![2]));
            l.1.notify_all();
        });
        asked.store(false, Ordering::Relaxed);
        let fresh = fresh_snapshot(&latest, &asked, std::time::Duration::from_secs(5)).unwrap();
        painter.join().unwrap();
        assert_eq!(*fresh, vec![2], "the new picture, not the old one");
    }
}

#[cfg(test)]
mod audio_rate_tests {
    #[test]
    fn a_phone_cant_send_an_impossible_sample_rate() {
        assert_eq!(super::audio_rate(" 44100 "), 44_100.0);
        for bad in ["0", "-5", "1", "nan", "inf", "words", "1e30"] {
            let r = super::audio_rate(bad);
            assert!((8_000.0..=192_000.0).contains(&r), "{bad} gives {r}");
        }
    }
}

/// "iPhone · Safari" style label from a User-Agent header.
fn device_label(request: &str) -> String {
    let ua = header(request, "User-Agent").unwrap_or("");
    let os = if ua.contains("iPhone") {
        "iPhone"
    } else if ua.contains("iPad") || (ua.contains("Macintosh") && ua.contains("Mobile")) {
        "iPad"
    } else if ua.contains("Android") {
        "Android"
    } else if ua.contains("Macintosh") {
        "Mac"
    } else if ua.contains("Windows") {
        "Windows"
    } else if ua.contains("Linux") {
        "Linux"
    } else if ua.is_empty() {
        "Unknown"
    } else {
        "Device"
    };
    let browser = if ua.contains("Edg/") {
        "Edge"
    } else if ua.contains("Chrome/") || ua.contains("CriOS") {
        "Chrome"
    } else if ua.contains("Firefox/") || ua.contains("FxiOS") {
        "Firefox"
    } else if ua.contains("Safari/") {
        "Safari"
    } else {
        ""
    };
    if browser.is_empty() { os.to_string() } else { format!("{os} · {browser}") }
}

/// The remote-control page's view of the project: (version, JSON). Bumped when anything changes.
pub type RemoteState = Arc<Mutex<(u64, Arc<String>)>>;
/// Commands from phones/tablets, drained by the app each frame.
pub type RemoteCmds = Arc<Mutex<Vec<serde_json::Value>>>;
/// Password required to control (None = anyone on the network may control).
pub type RemotePassword = Arc<Mutex<Option<String>>>;
/// What the control socket needs: state, command queue, connected count, password.
pub type RemoteHub = (RemoteState, RemoteCmds, Arc<AtomicUsize>, RemotePassword);
type Shared = Arc<(Mutex<Option<Frame>>, Condvar)>;
/// Layer thumbnails for the remote page: layer id → (version, JPEG).
pub type Thumbs = Arc<Mutex<std::collections::HashMap<u64, (u64, Frame)>>>;
/// Files the session hands out by path (the TouchOSC layout for a phone): path → (content type, file name, bytes).
pub type Files = Arc<Mutex<std::collections::HashMap<String, (&'static str, String, Vec<u8>)>>>;

/// The output's picture as a JPEG for `/snapshot.jpg`: nothing is captured while nobody watches, so one is asked
/// for (`asked`, read by `NetSession::frame_due`) and waited for up to `wait`; the last one there was if none comes.
#[cfg(not(target_arch = "wasm32"))]
fn fresh_snapshot(latest: &Shared, asked: &AtomicBool, wait: std::time::Duration) -> Option<Frame> {
    let (lock, cv) = &**latest;
    let mut guard = lock.lock().unwrap();
    let before = guard.clone();
    asked.store(true, Ordering::Relaxed);
    let deadline = std::time::Instant::now() + wait;
    let same = |now: &Option<Frame>| match (now, &before) {
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        (None, None) => true,
        _ => false,
    };
    while same(&guard) {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break;
        }
        guard = cv.wait_timeout(guard, left).unwrap().0;
    }
    guard.clone()
}

pub struct NetSession {
    pub port: u16,
    pub fps: u32,
    running: Arc<AtomicBool>,
    latest: Shared,
    pub viewers: Arc<AtomicUsize>,
    pub audio_peers: Arc<AtomicUsize>,
    audio_ring: Arc<Mutex<VecDeque<f32>>>,
    remote_state: RemoteState,
    remote_cmds: RemoteCmds,
    /// Phones/tablets connected to the remote page.
    pub remotes: Arc<AtomicUsize>,
    password: RemotePassword,
    registry: Reg,
    /// Bonjour: "phosphor.local" + a "Phosphor Session" HTTP service while the session runs.
    #[cfg(not(target_arch = "wasm32"))]
    mdns: Option<mdns_sd::ServiceDaemon>,
    encoder: Option<SyncSender<(u32, u32, Vec<u8>)>>,
    pacer: crate::pacing::FramePacer,
    /// A `/snapshot.jpg` request is waiting for a fresh picture (one is captured for it, then this clears).
    snapshot_asked: Arc<AtomicBool>,
    /// Frames were being captured for the session last frame (someone watched): `frame_due`.
    capturing: bool,
    /// Layer thumbnails shown on remotes, and the round-robin that refreshes them.
    thumbs: Thumbs,
    files: Files,
    thumb_encoder: Option<SyncSender<(u64, u32, u32, Vec<u8>)>>,
    thumb_pacer: crate::pacing::FramePacer,
    thumb_next: usize,
    /// What following Phosphors get (`crate::follow`), and how many follow.
    #[cfg(not(target_arch = "wasm32"))]
    pub follow: crate::follow::SharedFeed,
    pub followers: Arc<AtomicUsize>,
    /// WebRTC viewers (`/whep`): low-latency H.264 instead of MJPEG.
    #[cfg(not(target_arch = "wasm32"))]
    pub webrtc: crate::webrtc::Hub,
    /// The secure listener (port + 1) is up.
    pub https: bool,
    pub status: String,
    /// Shows sent to and from other Phosphors (`crate::show_send`): offers made here, the pack handed out.
    #[cfg(not(target_arch = "wasm32"))]
    pub shows: crate::show_send::Hub,
}

impl NetSession {
    pub fn new() -> Self {
        Self {
            port: DEFAULT_PORT,
            fps: 20,
            running: Arc::new(AtomicBool::new(false)),
            latest: Arc::new((Mutex::new(None), Condvar::new())),
            viewers: Arc::new(AtomicUsize::new(0)),
            audio_peers: Arc::new(AtomicUsize::new(0)),
            audio_ring: Arc::new(Mutex::new(VecDeque::new())),
            remote_state: Arc::new(Mutex::new((0, Arc::new(String::new())))),
            remote_cmds: Arc::new(Mutex::new(Vec::new())),
            remotes: Arc::new(AtomicUsize::new(0)),
            password: Arc::new(Mutex::new(None)),
            registry: Arc::new(Mutex::new(Registry::default())),
            #[cfg(not(target_arch = "wasm32"))]
            mdns: None,
            encoder: None,
            pacer: Default::default(),
            snapshot_asked: Arc::new(AtomicBool::new(false)),
            capturing: false,
            thumbs: Default::default(),
            files: Default::default(),
            thumb_encoder: None,
            thumb_pacer: Default::default(),
            thumb_next: 0,
            #[cfg(not(target_arch = "wasm32"))]
            follow: Default::default(),
            followers: Arc::new(AtomicUsize::new(0)),
            #[cfg(not(target_arch = "wasm32"))]
            webrtc: Default::default(),
            https: false,
            status: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            shows: Default::default(),
        }
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Hands a file out at `path` (e.g. "/Phosphor.tosc") while the session runs: phones download it from
    /// `url()` + path. Offering the same path again replaces it.
    pub fn offer(&self, path: &str, content_type: &'static str, file_name: &str, bytes: Vec<u8>) {
        self.files.lock().unwrap().insert(path.to_string(), (content_type, file_name.to_string(), bytes));
    }

    /// Publishes the project state for remotes (only when it changed).
    pub fn set_remote_state(&self, json: String) {
        let mut s = self.remote_state.lock().unwrap();
        if *s.1 != json {
            s.0 += 1;
            s.1 = Arc::new(json);
        }
    }

    pub fn take_remote_commands(&self) -> Vec<serde_json::Value> {
        std::mem::take(&mut *self.remote_cmds.lock().unwrap())
    }

    /// Sets (or clears) the password needed to control from a phone; viewing never needs it.
    pub fn set_password(&self, password: Option<String>) {
        *self.password.lock().unwrap() = password.filter(|p| !p.is_empty());
    }

    /// The session has a password (other Phosphors need it to follow, or to take a show sent from here).
    pub fn has_password(&self) -> bool {
        self.password.lock().unwrap().is_some()
    }

    /// Everyone connected right now.
    pub fn peers(&self) -> Vec<Peer> {
        self.registry.lock().unwrap().peers.clone()
    }

    /// Drops a connection (it may reconnect unless its address is blocked).
    pub fn disconnect(&self, id: u64) {
        self.registry.lock().unwrap().kicked.insert(id);
    }

    /// Blocks an address: its current connections drop and new ones are refused.
    pub fn block(&self, ip: &str) {
        let mut r = self.registry.lock().unwrap();
        r.blocked.insert(ip.to_string());
        let ids: Vec<u64> = r.peers.iter().filter(|p| p.ip == ip).map(|p| p.id).collect();
        r.kicked.extend(ids);
    }

    pub fn unblock(&self, ip: &str) {
        self.registry.lock().unwrap().blocked.remove(ip);
    }

    pub fn blocked(&self) -> Vec<String> {
        self.registry.lock().unwrap().blocked.iter().cloned().collect()
    }

    pub fn remote_url(&self) -> String {
        format!("{}remote", self.url())
    }

    /// The audience page (QR code for the room).
    pub fn play_url(&self) -> String {
        format!("{}play", self.url())
    }

    pub fn url(&self) -> String {
        let ip = crate::control::local_ip().unwrap_or_else(|| "localhost".into());
        format!("http://{ip}:{}/", self.port)
    }

    /// The same page by Bonjour name (Apple devices and most phones resolve it; easier to type than an address).
    pub fn local_url(&self) -> String {
        format!("http://phosphor.local:{}/", self.port)
    }

    /// The secure link (phones share their mic only on https; they warn once about the certificate).
    pub fn https_url(&self) -> String {
        let ip = crate::control::local_ip().unwrap_or_else(|| "localhost".into());
        format!("https://{ip}:{}/", self.https_port().unwrap_or(self.port))
    }

    /// The secure listener's port: the next one up (none when the Session is on the last port there is).
    pub fn https_port(&self) -> Option<u16> {
        self.port.checked_add(1)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(&mut self) -> Result<(), String> {
        self.stop();
        // A flag of its own for this run: the last run's listener and connections saw theirs go off in `stop` and
        // end (sharing one, they never saw it: it was back on before they looked, and kept the port).
        self.running = Arc::new(AtomicBool::new(false));
        // (The last run's listener lets go of the port at its next wake-up.)
        let listener = crate::net_util::bind_retrying(|| std::net::TcpListener::bind(("0.0.0.0", self.port))).map_err(|e| format!("Port {}: {e}", self.port))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        // HTTPS on the next port (phones only share their mic on a secure page); a self-signed certificate.
        let tls = match crate::net_tls::server_config() {
            Ok(cfg) => self.https_port().and_then(|port| crate::net_util::bind_retrying(|| std::net::TcpListener::bind(("0.0.0.0", port))).ok()).and_then(|l| l.set_nonblocking(true).ok().map(|_| (l, cfg))),
            Err(e) => {
                eprintln!("session: no HTTPS ({e})");
                None
            }
        };
        self.https = tls.is_some();
        // Bonjour: phones on the Wi-Fi can type phosphor.local instead of the address; other Phosphors find this
        // Session by its name and `app=phosphor` (to send it a show). (Not from tests: nothing goes on the network.)
        self.mdns = crate::control::local_ip().filter(|_| !cfg!(test)).and_then(|ip| {
            let daemon = mdns_sd::ServiceDaemon::new().ok()?;
            let name = crate::control::discovery::identity().0;
            let info = mdns_sd::ServiceInfo::new(crate::control::discovery::SESSION_SERVICE, &name, "phosphor.local.", ip.as_str(), self.port, crate::control::discovery::properties()).ok()?;
            daemon.register(info).ok()?;
            Some(daemon)
        });
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let latest = (self.latest.clone(), self.snapshot_asked.clone());
        let viewers = self.viewers.clone();
        let peers = self.audio_peers.clone();
        let ring = self.audio_ring.clone();
        let remote = (self.remote_state.clone(), self.remote_cmds.clone(), self.remotes.clone(), self.password.clone());
        let registry = self.registry.clone();
        let thumbs = self.thumbs.clone();
        let files = self.files.clone();
        let follow = (self.follow.clone(), self.followers.clone(), self.webrtc.clone());
        let shows = self.shows.clone();
        self.webrtc.set_fps(self.fps);
        std::thread::Builder::new()
            .name("phosphor-net".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    let mut idle = true;
                    // A thread per connection, as many as the session allows (a full session closes new ones at once).
                    let take = |stream: std::net::TcpStream, from: std::net::SocketAddr, cfg: Option<Arc<rustls::ServerConfig>>| {
                        let Some(entry) = Entry::open(&registry, &from.ip().to_string()) else { return };
                        let (running, latest, viewers, peers, ring, remote, registry, thumbs, files, follow, shows) =
                            (running.clone(), latest.clone(), viewers.clone(), peers.clone(), ring.clone(), remote.clone(), registry.clone(), thumbs.clone(), files.clone(), follow.clone(), shows.clone());
                        let _ = std::thread::Builder::new().name("phosphor-net-conn".into()).spawn(move || {
                            let _entry = entry;
                            native::serve(stream, cfg, running, latest, viewers, peers, ring, remote, registry, thumbs, files, follow, shows);
                        });
                    };
                    // A failed accept (a phone that hung up before it was taken, or too many files open for a
                    // moment) is that one connection's problem: the session keeps listening (it used to stop for good).
                    if let Ok((stream, from)) = listener.accept() {
                        idle = false;
                        take(stream, from, None);
                    }
                    if let Some((l, cfg)) = &tls
                        && let Ok((stream, from)) = l.accept()
                    {
                        idle = false;
                        take(stream, from, Some(cfg.clone()));
                    }
                    if idle {
                        std::thread::sleep(std::time::Duration::from_millis(30));
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        // JPEG encoding happens off the UI thread; at most one frame waits in the queue.
        let (tx, rx) = std::sync::mpsc::sync_channel::<(u32, u32, Vec<u8>)>(1);
        let latest = self.latest.clone();
        std::thread::Builder::new()
            .name("phosphor-jpeg".into())
            .spawn(move || {
                for (w, h, rgba) in rx {
                    if let Some(jpeg) = native::encode(w, h, rgba) {
                        let (lock, cv) = &*latest;
                        *lock.lock().unwrap() = Some(Arc::new(jpeg));
                        cv.notify_all();
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        self.encoder = Some(tx);
        // Layer thumbnails: tiny pictures, a few a second at most, on their own thread so a slow one never delays the stream.
        let (tx, rx) = std::sync::mpsc::sync_channel::<(u64, u32, u32, Vec<u8>)>(8);
        let thumbs = self.thumbs.clone();
        std::thread::Builder::new()
            .name("phosphor-thumb-jpeg".into())
            .spawn(move || {
                for (id, w, h, rgba) in rx {
                    if let Some(jpeg) = native::encode(w, h, rgba) {
                        let mut map = thumbs.lock().unwrap();
                        let e = map.entry(id).or_insert((0, Arc::new(Vec::new())));
                        e.0 += 1;
                        e.1 = Arc::new(jpeg);
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        self.thumb_encoder = Some(tx);
        self.status = format!("Serving {}", self.url());
        crate::mcp::note_session_port(Some(self.port));
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn start(&mut self) -> Result<(), String> {
        Err("Sessions are hosted from the desktop app".into())
    }

    pub fn stop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(d) = self.mdns.take() {
            let _ = d.shutdown();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if self.running.load(Ordering::Relaxed) {
            crate::mcp::note_session_port(None);
        }
        self.running.store(false, Ordering::Relaxed);
        self.encoder = None;
        self.thumb_encoder = None;
        self.thumbs.lock().unwrap().clear();
        self.viewers.store(0, Ordering::Relaxed);
        self.audio_peers.store(0, Ordering::Relaxed);
        self.status.clear();
    }

    /// True when the output should be captured for the session now: only while someone watches (the MJPEG picture,
    /// WebRTC) or a `/snapshot.jpg` waits, at the session's rate. A session with nobody watching used to read the
    /// output back and JPEG-encode it 20 times a second for no one.
    pub fn frame_due(&mut self, now: f64) -> bool {
        if !self.running() {
            return false;
        }
        if !self.jpeg_wanted() && !self.rtc_watched() {
            if self.capturing {
                self.capturing = false;
                #[cfg(not(target_arch = "wasm32"))]
                self.webrtc.stop_unwatched();
            }
            return false;
        }
        self.capturing = true;
        self.pacer.due(now, self.fps)
    }

    /// A JPEG of the output is wanted: an MJPEG viewer (the player page, a remote's preview, a following Phosphor)
    /// or a snapshot request.
    fn jpeg_wanted(&self) -> bool {
        self.viewers.load(Ordering::Relaxed) > 0 || self.snapshot_asked.load(Ordering::Relaxed)
    }

    fn rtc_watched(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        return self.webrtc.watched();
        #[cfg(target_arch = "wasm32")]
        return false;
    }

    /// Hands a captured frame (moved, not copied, when only one kind of viewer wants it) to the WebRTC encoder and
    /// the JPEG encoder, to each only while it's wanted; dropped if an encoder is still busy.
    pub fn push_frame(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        let jpeg = self.jpeg_wanted();
        #[cfg(not(target_arch = "wasm32"))]
        let rgba = match (self.webrtc.watched(), jpeg) {
            (true, false) => return self.webrtc.push(w, h, rgba),
            (true, true) => {
                self.webrtc.push(w, h, rgba.clone());
                rgba
            }
            (false, _) => rgba,
        };
        if !jpeg {
            return;
        }
        match self.encoder.as_ref().map(|tx| tx.try_send((w, h, rgba))) {
            // A snapshot waiting gets this frame (the JPEG thread wakes it).
            Some(Ok(())) => self.snapshot_asked.store(false, Ordering::Relaxed),
            Some(Err(TrySendError::Disconnected(_))) => self.encoder = None,
            Some(Err(TrySendError::Full(_))) | None => {}
        }
    }

    /// Which of `n` layers to copy a thumbnail of now, cycling so each refreshes about once a
    /// second (at most 8 a second). None while no remote is connected: nothing is read back then.
    pub fn thumb_due(&mut self, now: f64, n: usize) -> Option<usize> {
        if n == 0 || !self.running() || self.remotes.load(Ordering::Relaxed) == 0 {
            return None;
        }
        if !self.thumb_pacer.due(now, n.min(8) as u32) {
            return None;
        }
        self.thumb_next = (self.thumb_next + 1) % n;
        Some(self.thumb_next)
    }

    /// Hands a layer's thumbnail to its encoder; dropped if the queue is full (it comes round again).
    pub fn push_thumb(&mut self, layer: u64, w: u32, h: u32, rgba: Vec<u8>) {
        if let Some(tx) = &self.thumb_encoder
            && let Err(TrySendError::Disconnected(_)) = tx.try_send((layer, w, h, rgba))
        {
            self.thumb_encoder = None;
        }
    }

    /// Current version of every layer thumbnail (the remote page reloads a picture when its number changes).
    pub fn thumb_versions(&self) -> std::collections::HashMap<u64, u64> {
        self.thumbs.lock().unwrap().iter().map(|(k, v)| (*k, v.0)).collect()
    }

    /// Forgets thumbnails of layers that no longer exist.
    pub fn retain_thumbs(&self, alive: &[u64]) {
        let mut map = self.thumbs.lock().unwrap();
        if map.len() > alive.len() {
            map.retain(|k, _| alive.contains(k));
        }
    }

    /// Drains audio sent by viewers (mono f32 at 48 kHz).
    pub fn take_audio(&self, out: &mut Vec<f32>) {
        let mut ring = self.audio_ring.lock().unwrap();
        out.extend(ring.drain(..));
    }
}

/// An id nobody can guess (16 random bytes as hex): guest ids, the token a sent show is handed out under.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn new_token() -> String {
    native::new_token()
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::TcpStream;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use super::{Frame, MAX_WIDTH, Shared, Thumbs};

    pub const PLAYER_HTML: &str = include_str!("../player.html");
    pub const REMOTE_HTML: &str = include_str!("../remote.html");
    /// The app icon for a phone's home screen (the remote / audience page saved as a web app).
    const ICON_192: &[u8] = include_bytes!("../../../assets/icon-192.png");
    const ICON_512: &[u8] = include_bytes!("../../../assets/icon-512.png");

    /// One whole HTTP 200 answer: `kind` = content type, `headers` = any more (Cache-Control, Content-Disposition…).
    fn respond_ok(stream: &mut impl Write, kind: &str, headers: &[(&str, &str)], body: &[u8]) {
        let more: String = headers.iter().map(|(k, v)| format!("{k}: {v}\r\n")).collect();
        let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\n{more}Content-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
        let _ = stream.write_all(body);
    }

    /// The web app manifest for the remote (or, `play`, the audience) page: "Add to Home Screen" then opens it
    /// full-screen with Phosphor's icon, like an app.
    fn manifest(play: bool) -> String {
        let (name, short, start) = if play { ("Phosphor Play", "Play", "/play") } else { ("Phosphor Remote", "Phosphor", "/remote") };
        serde_json::json!({
            "name": name,
            "short_name": short,
            "start_url": start,
            "scope": "/",
            "display": "standalone",
            "orientation": "any",
            "background_color": "#141618",
            "theme_color": "#141618",
            "icons": [
                { "src": "/icon-192.png", "sizes": "192x192", "type": "image/png" },
                { "src": "/icon-512.png", "sizes": "512x512", "type": "image/png", "purpose": "any maskable" }
            ]
        })
        .to_string()
    }

    pub fn encode(w: u32, h: u32, rgba: Vec<u8>) -> Option<Vec<u8>> {
        let img = image::RgbaImage::from_raw(w, h, rgba)?;
        let rgb = image::DynamicImage::ImageRgba8(img).to_rgb8();
        let rgb = if w > MAX_WIDTH {
            image::imageops::resize(&rgb, MAX_WIDTH, h * MAX_WIDTH / w, image::imageops::FilterType::Triangle)
        } else {
            rgb
        };
        let mut out = Vec::with_capacity(200_000);
        let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 72);
        enc.encode_image(&rgb).ok()?;
        Some(out)
    }

    pub fn serve(
        stream: TcpStream,
        tls: Option<Arc<rustls::ServerConfig>>,
        running: Arc<AtomicBool>,
        latest: (Shared, Arc<AtomicBool>),
        viewers: Arc<AtomicUsize>,
        peers: Arc<AtomicUsize>,
        ring: Arc<Mutex<VecDeque<f32>>>,
        remote: super::RemoteHub,
        registry: super::Reg,
        thumbs: Thumbs,
        files: super::Files,
        follow: (crate::follow::SharedFeed, Arc<AtomicUsize>, crate::webrtc::Hub),
        shows: crate::show_send::Hub,
    ) {
        // Sockets accepted from a non-blocking listener inherit that flag on macOS/BSD.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        // A device that stops reading (a phone that walked away mid-picture) lets its connection go instead of
        // holding the thread.
        let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
        let Ok(tcp) = stream.try_clone() else { return };
        let Ok(mut conn) = crate::net_tls::Conn::new(stream, tls.as_ref()) else { return };
        // The request head (up to the blank line), read once; WebSocket paths get it replayed. A sender that
        // trickles it in gets 10 seconds in all.
        let mut head: Vec<u8> = Vec::with_capacity(2048);
        let mut chunk = [0u8; 512];
        let began = std::time::Instant::now();
        while head.len() < 8192 && !head.windows(4).any(|w| w == b"\r\n\r\n") && began.elapsed() < Duration::from_secs(10) {
            match conn.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => head.extend_from_slice(&chunk[..n]),
            }
        }
        if head.is_empty() {
            return;
        }
        let request = String::from_utf8_lossy(&head).to_string();
        let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
        let ip = tcp.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
        let mut stream = crate::net_tls::Replay::new(head, conn);
        if registry.lock().unwrap().blocked.contains(&ip) {
            let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
        }
        let device = super::device_label(&request);
        let controls = ["/control", "/guest", "/audio", "/follow", crate::show_send::OFFER_PATH].iter().any(|p| path.starts_with(p)) || (path.starts_with("/whep") && request.starts_with("POST"));
        if controls && super::cross_site(&request) {
            let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
        }
        if path.starts_with("/control") {
            let seat = super::Seat::join(&registry, super::PeerKind::Remote, &ip, &device);
            control_socket(stream, tcp, running, remote, seat, false);
            return;
        }
        if path.starts_with("/guest") {
            let seat = super::Seat::join(&registry, super::PeerKind::Guest, &ip, &device);
            control_socket(stream, tcp, running, remote, seat, true);
            return;
        }
        // WebRTC (WHEP): the browser posts its offer (SDP), gets the answer; the picture then comes over UDP.
        if path.starts_with("/whep") && request.starts_with("POST") {
            let body = read_body(&mut stream, &request, 64 * 1024);
            let host = tcp.local_addr().map(|a| a.ip()).unwrap_or(std::net::IpAddr::from([127, 0, 0, 1]));
            // Each WebRTC viewer is listed in Devices (and can be disconnected or blocked there) for as long as it watches.
            let seat = super::Seat::join(&registry, super::PeerKind::Viewer, &ip, &device);
            let stay = move || running.load(Ordering::Relaxed) && !seat.kicked();
            let reply = match follow.2.answer(&String::from_utf8_lossy(&body), host, stay) {
                Ok(sdp) => format!("HTTP/1.1 201 Created\r\nContent-Type: application/sdp\r\nLocation: /whep/viewer\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{sdp}", sdp.len()),
                Err(e) => format!("HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{e}", e.len()),
            };
            let _ = stream.write_all(reply.as_bytes());
            return;
        }
        // Another Phosphor following this show.
        if path.starts_with("/follow") {
            let seat = super::Seat::join(&registry, super::PeerKind::Follower, &ip, "Phosphor (following)");
            let _ = tcp.set_nodelay(true);
            let Ok(mut ws) = tungstenite::accept(stream) else { return };
            // With a session password, a follower must know it (the project is the whole show).
            let current = remote.3.lock().unwrap().clone();
            if let Some(current) = current
                && !crate::follow::admit(&mut ws, |tried| seat.try_password(tried, &current))
            {
                return;
            }
            let _ = tcp.set_read_timeout(Some(Duration::from_millis(15)));
            follow.1.fetch_add(1, Ordering::Relaxed);
            crate::follow::serve(ws, follow.0, || running.load(Ordering::Relaxed) && !seat.kicked());
            follow.1.fetch_sub(1, Ordering::Relaxed);
            return;
        }
        // Shows sent between Phosphors: offers made to this one, and the pack this one hands out.
        if path.starts_with(crate::show_send::OFFER_PATH) || path.starts_with(crate::show_send::PACK_PATH) {
            let post = request.starts_with("POST");
            let body = if post { read_body(&mut stream, &request, crate::show_send::MAX_OFFER_BODY) } else { Vec::new() };
            let taking = path.starts_with(crate::show_send::PACK_PATH);
            // Only a download is someone connected (listed in Devices, can be dropped or blocked there).
            let seat = taking.then(|| super::Seat::join(&registry, super::PeerKind::Receiver, &ip, "Phosphor (taking a show)"));
            let current = remote.3.lock().unwrap().clone();
            let tried = super::header(&request, crate::show_send::PASSWORD_HEADER).unwrap_or("").to_string();
            let password_ok = || match (&current, &seat) {
                (Some(p), Some(seat)) => seat.try_password(&tried, p),
                (Some(_), None) => false,
                (None, _) => true,
            };
            let stay = || running.load(Ordering::Relaxed) && !seat.as_ref().is_some_and(|s| s.kicked());
            crate::show_send::serve(&mut stream, post, &path, &ip, &body, &shows, password_ok, stay);
            return;
        }
        if path.starts_with("/audio") {
            let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));
            let seat = super::Seat::join(&registry, super::PeerKind::Audio, &ip, &device);
            audio_socket(stream, tcp, running, peers, ring, seat);
            return;
        }
        let _ = tcp.set_nodelay(true);
        // A file the app offered (one lock, the copy taken before answering).
        let offered = files.lock().unwrap().get(path.as_str()).cloned();
        match path.as_str() {
            "/stream.mjpeg" => {
                let seat = super::Seat::join(&registry, super::PeerKind::Viewer, &ip, &device);
                viewers.fetch_add(1, Ordering::Relaxed);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                );
                let mut last: Option<Frame> = None;
                while running.load(Ordering::Relaxed) && !seat.kicked() {
                    let frame = {
                        let (lock, cv) = &*latest.0;
                        let guard = lock.lock().unwrap();
                        let (guard, _) = cv.wait_timeout(guard, Duration::from_millis(200)).unwrap();
                        guard.clone()
                    };
                    let Some(frame) = frame else { continue };
                    if last.as_ref().is_some_and(|l| Arc::ptr_eq(l, &frame)) {
                        continue;
                    }
                    let header = format!("--frame\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\n\r\n", frame.len());
                    if stream.write_all(header.as_bytes()).is_err() || stream.write_all(&frame).is_err() || stream.write_all(b"\r\n").is_err() {
                        break;
                    }
                    last = Some(frame);
                }
                viewers.fetch_sub(1, Ordering::Relaxed);
            }
            "/snapshot.jpg" => {
                let frame = super::fresh_snapshot(&latest.0, &latest.1, Duration::from_secs(2));
                match frame {
                    Some(f) => {
                        respond_ok(&mut stream, "image/jpeg", &[], &f);
                    }
                    None => {
                        let _ = stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            // /thumb/<layer id>.jpg?v=<version>: one layer's current picture.
            p if p.starts_with("/thumb/") => {
                let id = p["/thumb/".len()..].split(['.', '?']).next().and_then(|s| s.parse::<u64>().ok());
                let jpeg = id.and_then(|id| thumbs.lock().unwrap().get(&id).map(|(_, f)| f.clone()));
                match jpeg {
                    Some(f) => {
                        respond_ok(&mut stream, "image/jpeg", &[("Cache-Control", "no-cache")], &f);
                    }
                    None => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            // A file the app offered (the TouchOSC layout): downloaded, not shown.
            _ if let Some((kind, name, bytes)) = &offered => {
                let disposition = format!("attachment; filename=\"{name}\"");
                respond_ok(&mut stream, kind, &[("Content-Disposition", &disposition), ("Cache-Control", "no-cache")], bytes);
            }
            // The remote page as a home-screen web app: its manifest and icon.
            p if p.starts_with("/manifest.webmanifest") => {
                respond_ok(&mut stream, "application/manifest+json", &[("Cache-Control", "no-cache")], manifest(p.contains("play")).as_bytes());
            }
            "/icon-192.png" | "/icon-512.png" | "/apple-touch-icon.png" => {
                respond_ok(&mut stream, "image/png", &[("Cache-Control", "max-age=86400")], if path == "/icon-512.png" { ICON_512 } else { ICON_192 });
            }
            // The remote page doubles as the audience page (it reads its own path).
            p if p.starts_with("/remote") || p.starts_with("/play") => {
                respond_ok(&mut stream, "text/html; charset=utf-8", &[("Cache-Control", "no-cache")], REMOTE_HTML.as_bytes());
            }
            _ => {
                respond_ok(&mut stream, "text/html; charset=utf-8", &[], PLAYER_HTML.as_bytes());
            }
        }
    }

    /// Hands a command to the app (dropped while too many wait: the app stalled or someone floods).
    fn queue(cmds: &super::RemoteCmds, v: serde_json::Value) {
        let mut q = cmds.lock().unwrap();
        if q.len() < super::MAX_QUEUED {
            q.push(v);
        }
    }

    /// The body of a POST (at most `max` bytes): the connection replays the request head first, so head + body are
    /// read and what's after the head is kept.
    fn read_body(stream: &mut impl Read, request: &str, max: usize) -> Vec<u8> {
        let len = super::header(request, "Content-Length").and_then(|v| v.parse::<usize>().ok()).unwrap_or(0).min(max);
        let head_len = request.find("\r\n\r\n").map_or(request.len(), |i| i + 4);
        let mut all = Vec::with_capacity(head_len + len);
        let mut chunk = [0u8; 4096];
        while all.len() < head_len + len {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => all.extend_from_slice(&chunk[..n]),
            }
        }
        all.get(head_len..head_len + len).or_else(|| all.get(head_len..)).unwrap_or_default().to_vec()
    }

    /// A guest id nobody can guess: 16 random bytes as hex.
    pub(super) fn new_token() -> String {
        let mut bytes = [0u8; 16];
        if rustls::crypto::ring::default_provider().secure_random.fill(&mut bytes).is_err() {
            // (Never expected; the time still makes a different id each call.)
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
            bytes = t.to_le_bytes();
        }
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Remote control: pushes the project state whenever it changes and queues the commands the
    /// page sends back (JSON objects like {"t":"opacity","layer":3,"v":0.5}).
    /// A guest (audience play) needs no password: it learns its seat number, every command it sends
    /// carries it as `_from`, and the Mac decides whose turn it is.
    fn control_socket(stream: crate::net_tls::Replay, tcp: TcpStream, running: Arc<AtomicBool>, remote: super::RemoteHub, seat: super::Seat, guest: bool) {
        let (state, cmds, count, password) = remote;
        let _ = tcp.set_nodelay(true);
        let Ok(mut ws) = tungstenite::accept(stream) else { return };
        let _ = tcp.set_read_timeout(Some(Duration::from_millis(30)));
        // Guests aren't "remotes": they don't unlock layer thumbnails or count in the Session panel.
        if !guest {
            count.fetch_add(1, Ordering::Relaxed);
        }
        let mut sent = u64::MAX;
        // The password this socket proved it knows (None = not authorised, unless none is set).
        let mut granted: Option<Option<String>> = None;
        let mut told: Option<bool> = None;
        let mut bucket = super::Bucket::new();
        if guest && ws.send(tungstenite::Message::Text(serde_json::json!({ "you": seat.id }).to_string().into())).is_err() {
            return;
        }
        while running.load(Ordering::Relaxed) && !seat.kicked() {
            let current = password.lock().unwrap().clone();
            let allowed = guest || current.is_none() || granted.as_ref() == Some(&current);
            if !guest && told != Some(allowed) {
                seat.set_controlling(allowed);
                let msg = serde_json::json!({ "auth": if allowed { "ok" } else { "required" } }).to_string();
                if ws.send(tungstenite::Message::Text(msg.as_str().into())).is_err() {
                    break;
                }
                told = Some(allowed);
            }
            let (version, json) = state.lock().unwrap().clone();
            if version != sent && !json.is_empty() {
                if ws.send(tungstenite::Message::Text(json.as_str().into())).is_err() {
                    break;
                }
                sent = version;
            }
            match ws.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    let Ok(mut v) = serde_json::from_str::<serde_json::Value>(t.as_str()) else { continue };
                    if guest {
                        // A phone sending more than any hand could is ignored until it slows down.
                        if !bucket.take(web_time::Instant::now()) {
                            continue;
                        }
                        if v.get("t").and_then(|t| t.as_str()) == Some("hello") {
                            // The id it plays under comes from here (a phone can't take another's place in line).
                            let token = seat.set_token(v.get("token").and_then(|t| t.as_str()).unwrap_or(""), v.get("name").and_then(|t| t.as_str()).unwrap_or(""), new_token);
                            if ws.send(tungstenite::Message::Text(serde_json::json!({ "you": seat.id, "token": token }).to_string().into())).is_err() {
                                break;
                            }
                            continue;
                        }
                        if let Some(o) = v.as_object_mut() {
                            o.insert("_from".into(), serde_json::json!(seat.id));
                            queue(&cmds, v);
                        }
                        continue;
                    }
                    if v.get("t").and_then(|t| t.as_str()) == Some("auth") {
                        let tried = v.get("password").and_then(|p| p.as_str()).unwrap_or("");
                        if current.as_deref().is_some_and(|p| seat.try_password(tried, p)) {
                            granted = Some(current.clone());
                        } else {
                            let _ = ws.send(tungstenite::Message::Text(r#"{"auth":"bad"}"#.into()));
                            told = Some(false);
                        }
                        continue;
                    }
                    if allowed {
                        // Who sent it, for "… is controlling" on the Mac (guests carry `_from` instead).
                        if let Some(o) = v.as_object_mut() {
                            o.insert("_by".into(), serde_json::json!(seat.id));
                        }
                        queue(&cmds, v);
                    }
                }
                Ok(tungstenite::Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        if !guest {
            count.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Viewer microphone: first a text message `rate:<hz>`, then binary little-endian f32 mono
    /// chunks. Resampled to 48 kHz and queued for the audio analyzer.
    fn audio_socket(stream: crate::net_tls::Replay, tcp: TcpStream, running: Arc<AtomicBool>, peers: Arc<AtomicUsize>, ring: Arc<Mutex<VecDeque<f32>>>, seat: super::Seat) {
        let _ = &tcp;
        let Ok(mut ws) = tungstenite::accept(stream) else { return };
        peers.fetch_add(1, Ordering::Relaxed);
        let mut rate = 48_000.0f32;
        while running.load(Ordering::Relaxed) && !seat.kicked() {
            let msg = match ws.read() {
                Ok(m) => m,
                Err(_) => break,
            };
            match msg {
                tungstenite::Message::Text(t) => {
                    if let Some(r) = t.as_str().strip_prefix("rate:") {
                        rate = super::audio_rate(r);
                    }
                }
                tungstenite::Message::Binary(b) => {
                    let samples: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    if !samples.is_empty() {
                        seat.set_level((samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt());
                    }
                    let ratio = rate / 48_000.0;
                    // (Never more than the second of sound the ring keeps.)
                    let out_len = ((samples.len() as f32 / ratio) as usize).min(48_000);
                    let mut q = ring.lock().unwrap();
                    for i in 0..out_len {
                        let pos = i as f32 * ratio;
                        let k = pos as usize;
                        let f = pos - k as f32;
                        let a = samples.get(k).copied().unwrap_or(0.0);
                        let b2 = samples.get(k + 1).copied().unwrap_or(a);
                        q.push_back(a + (b2 - a) * f);
                    }
                    while q.len() > 48_000 {
                        q.pop_front();
                    }
                }
                tungstenite::Message::Close(_) => break,
                _ => {}
            }
        }
        peers.fetch_sub(1, Ordering::Relaxed);
    }
}
