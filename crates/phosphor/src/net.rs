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
}

/// A phone/tablet/browser connected to the session.
#[derive(Clone, Debug)]
pub struct Peer {
    pub id: u64,
    pub kind: PeerKind,
    pub ip: String,
    pub device: String,
    pub since: std::time::Instant,
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
}

pub type Reg = Arc<Mutex<Registry>>;

/// Registers a connection for as long as it lives; leaving happens on drop.
struct Seat {
    reg: Reg,
    id: u64,
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
        r.peers.push(Peer { id, kind, ip: ip.to_string(), device: device.to_string(), since: std::time::Instant::now(), controlling: false, token: String::new(), name: String::new(), level: 0.0 });
        Self { reg: reg.clone(), id }
    }

    fn kicked(&self) -> bool {
        self.reg.lock().unwrap().kicked.contains(&self.id)
    }

    fn set_token(&self, token: &str, name: &str) {
        if let Some(p) = self.reg.lock().unwrap().peers.iter_mut().find(|p| p.id == self.id) {
            p.token = token.chars().filter(|c| c.is_ascii_alphanumeric()).take(40).collect();
            p.name = name.chars().filter(|c| !c.is_control()).take(24).collect::<String>().trim().to_string();
        }
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

/// "iPhone · Safari" style label from a User-Agent header.
fn device_label(request: &str) -> String {
    let ua = request.lines().find_map(|l| l.strip_prefix("User-Agent:").or_else(|| l.strip_prefix("user-agent:"))).unwrap_or("").trim();
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
    /// Layer thumbnails shown on remotes, and the round-robin that refreshes them.
    thumbs: Thumbs,
    thumb_encoder: Option<SyncSender<(u64, u32, u32, Vec<u8>)>>,
    thumb_pacer: crate::pacing::FramePacer,
    thumb_next: usize,
    /// The secure listener (port + 1) is up.
    pub https: bool,
    pub status: String,
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
            thumbs: Default::default(),
            thumb_encoder: None,
            thumb_pacer: Default::default(),
            thumb_next: 0,
            https: false,
            status: String::new(),
        }
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
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
        format!("https://{ip}:{}/", self.port + 1)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(&mut self) -> Result<(), String> {
        self.stop();
        let listener = std::net::TcpListener::bind(("0.0.0.0", self.port)).map_err(|e| format!("Port {}: {e}", self.port))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        // HTTPS on the next port (phones only share their mic on a secure page); a self-signed certificate.
        let tls = match crate::net_tls::server_config() {
            Ok(cfg) => std::net::TcpListener::bind(("0.0.0.0", self.port + 1)).ok().and_then(|l| l.set_nonblocking(true).ok().map(|_| (l, cfg))),
            Err(e) => {
                eprintln!("session: no HTTPS ({e})");
                None
            }
        };
        self.https = tls.is_some();
        // Bonjour: phones on the Wi-Fi can type phosphor.local instead of the address.
        self.mdns = crate::control::local_ip().and_then(|ip| {
            let daemon = mdns_sd::ServiceDaemon::new().ok()?;
            let info = mdns_sd::ServiceInfo::new("_http._tcp.local.", "Phosphor Session", "phosphor.local.", ip.as_str(), self.port, std::collections::HashMap::<String, String>::new()).ok()?;
            daemon.register(info).ok()?;
            Some(daemon)
        });
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let latest = self.latest.clone();
        let viewers = self.viewers.clone();
        let peers = self.audio_peers.clone();
        let ring = self.audio_ring.clone();
        let remote = (self.remote_state.clone(), self.remote_cmds.clone(), self.remotes.clone(), self.password.clone());
        let registry = self.registry.clone();
        let thumbs = self.thumbs.clone();
        std::thread::Builder::new()
            .name("phosphor-net".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    let mut idle = true;
                    let take = |stream: std::net::TcpStream, cfg: Option<Arc<rustls::ServerConfig>>| {
                        let (running, latest, viewers, peers, ring, remote, registry, thumbs) =
                            (running.clone(), latest.clone(), viewers.clone(), peers.clone(), ring.clone(), remote.clone(), registry.clone(), thumbs.clone());
                        std::thread::spawn(move || native::serve(stream, cfg, running, latest, viewers, peers, ring, remote, registry, thumbs));
                    };
                    match listener.accept() {
                        Ok((stream, _)) => {
                            idle = false;
                            take(stream, None);
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                        Err(_) => break,
                    }
                    if let Some((l, cfg)) = &tls {
                        match l.accept() {
                            Ok((stream, _)) => {
                                idle = false;
                                take(stream, Some(cfg.clone()));
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => break,
                        }
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
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn start(&mut self) -> Result<(), String> {
        Err("Sessions are hosted from the desktop app".into())
    }

    pub fn stop(&mut self) {
        if let Some(d) = self.mdns.take() {
            let _ = d.shutdown();
        }
        self.running.store(false, Ordering::Relaxed);
        self.encoder = None;
        self.thumb_encoder = None;
        self.thumbs.lock().unwrap().clear();
        self.viewers.store(0, Ordering::Relaxed);
        self.audio_peers.store(0, Ordering::Relaxed);
        self.status.clear();
    }

    pub fn frame_due(&mut self, now: f64) -> bool {
        self.running() && self.pacer.due(now, self.fps)
    }

    /// Hands a captured frame to the encoder; drops it if the encoder is still busy.
    pub fn push_frame(&mut self, w: u32, h: u32, rgba: Vec<u8>) {
        if let Some(tx) = &self.encoder
            && let Err(TrySendError::Disconnected(_)) = tx.try_send((w, h, rgba))
        {
            self.encoder = None;
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
        latest: Shared,
        viewers: Arc<AtomicUsize>,
        peers: Arc<AtomicUsize>,
        ring: Arc<Mutex<VecDeque<f32>>>,
        remote: super::RemoteHub,
        registry: super::Reg,
        thumbs: Thumbs,
    ) {
        // Sockets accepted from a non-blocking listener inherit that flag on macOS/BSD.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let Ok(tcp) = stream.try_clone() else { return };
        let Ok(mut conn) = crate::net_tls::Conn::new(stream, tls.as_ref()) else { return };
        // The request head (up to the blank line), read once; WebSocket paths get it replayed.
        let mut head: Vec<u8> = Vec::with_capacity(2048);
        let mut chunk = [0u8; 512];
        while head.len() < 8192 && !head.windows(4).any(|w| w == b"\r\n\r\n") {
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
        if path.starts_with("/audio") {
            let _ = tcp.set_read_timeout(Some(Duration::from_secs(30)));
            let seat = super::Seat::join(&registry, super::PeerKind::Audio, &ip, &device);
            audio_socket(stream, tcp, running, peers, ring, seat);
            return;
        }
        let _ = tcp.set_nodelay(true);
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
                        let (lock, cv) = &*latest;
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
                let frame = latest.0.lock().unwrap().clone();
                match frame {
                    Some(f) => {
                        let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", f.len()).as_bytes());
                        let _ = stream.write_all(&f);
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
                        let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: image/jpeg\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", f.len()).as_bytes());
                        let _ = stream.write_all(&f);
                    }
                    None => {
                        let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
                    }
                }
            }
            // The remote page doubles as the audience page (it reads its own path).
            p if p.starts_with("/remote") || p.starts_with("/play") => {
                let body = REMOTE_HTML.as_bytes();
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nCache-Control: no-cache\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                let _ = stream.write_all(body);
            }
            _ => {
                let body = PLAYER_HTML.as_bytes();
                let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
                let _ = stream.write_all(body);
            }
        }
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
                        if v.get("t").and_then(|t| t.as_str()) == Some("hello") {
                            seat.set_token(v.get("token").and_then(|t| t.as_str()).unwrap_or(""), v.get("name").and_then(|t| t.as_str()).unwrap_or(""));
                            continue;
                        }
                        if let Some(o) = v.as_object_mut() {
                            o.insert("_from".into(), serde_json::json!(seat.id));
                            cmds.lock().unwrap().push(v);
                        }
                        continue;
                    }
                    if v.get("t").and_then(|t| t.as_str()) == Some("auth") {
                        let tried = v.get("password").and_then(|p| p.as_str()).unwrap_or("");
                        if current.as_deref().is_some_and(|p| p == tried) {
                            granted = Some(current.clone());
                        } else {
                            // Slow down guessing a little.
                            std::thread::sleep(Duration::from_millis(400));
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
                        cmds.lock().unwrap().push(v);
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
    /// chunks. Resampled to 48 kHz and queued for the audio analyser.
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
                        rate = r.trim().parse().unwrap_or(48_000.0);
                    }
                }
                tungstenite::Message::Binary(b) => {
                    let samples: Vec<f32> = b.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
                    if !samples.is_empty() {
                        seat.set_level((samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt());
                    }
                    let ratio = rate / 48_000.0;
                    let out_len = (samples.len() as f32 / ratio) as usize;
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
