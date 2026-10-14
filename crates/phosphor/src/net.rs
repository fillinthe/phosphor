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
    fn join(reg: &Reg, kind: PeerKind, ip: &str, device: &str) -> Self {
        let mut r = reg.lock().unwrap();
        r.next += 1;
        let id = r.next;
        r.peers.push(Peer { id, kind, ip: ip.to_string(), device: device.to_string(), since: std::time::Instant::now(), controlling: false });
        Self { reg: reg.clone(), id }
    }

    fn kicked(&self) -> bool {
        self.reg.lock().unwrap().kicked.contains(&self.id)
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
    encoder: Option<SyncSender<(u32, u32, Vec<u8>)>>,
    pacer: crate::pacing::FramePacer,
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
            encoder: None,
            pacer: Default::default(),
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

    pub fn url(&self) -> String {
        let ip = crate::control::local_ip().unwrap_or_else(|| "localhost".into());
        format!("http://{ip}:{}/", self.port)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn start(&mut self) -> Result<(), String> {
        self.stop();
        let listener = std::net::TcpListener::bind(("0.0.0.0", self.port)).map_err(|e| format!("Port {}: {e}", self.port))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.running.store(true, Ordering::Relaxed);
        let running = self.running.clone();
        let latest = self.latest.clone();
        let viewers = self.viewers.clone();
        let peers = self.audio_peers.clone();
        let ring = self.audio_ring.clone();
        let remote = (self.remote_state.clone(), self.remote_cmds.clone(), self.remotes.clone(), self.password.clone());
        let registry = self.registry.clone();
        std::thread::Builder::new()
            .name("phosphor-net".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (running, latest, viewers, peers, ring, remote, registry) =
                                (running.clone(), latest.clone(), viewers.clone(), peers.clone(), ring.clone(), remote.clone(), registry.clone());
                            std::thread::spawn(move || native::serve(stream, running, latest, viewers, peers, ring, remote, registry));
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => std::thread::sleep(std::time::Duration::from_millis(30)),
                        Err(_) => break,
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
        self.status = format!("Serving {}", self.url());
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn start(&mut self) -> Result<(), String> {
        Err("Sessions are hosted from the desktop app".into())
    }

    pub fn stop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        self.encoder = None;
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

    use super::{Frame, MAX_WIDTH, Shared};

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
        mut stream: TcpStream,
        running: Arc<AtomicBool>,
        latest: Shared,
        viewers: Arc<AtomicUsize>,
        peers: Arc<AtomicUsize>,
        ring: Arc<Mutex<VecDeque<f32>>>,
        remote: super::RemoteHub,
        registry: super::Reg,
    ) {
        // Sockets accepted from a non-blocking listener inherit that flag on macOS/BSD.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = [0u8; 2048];
        let Ok(n) = stream.peek(&mut head) else { return };
        let request = String::from_utf8_lossy(&head[..n]).to_string();
        let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
        let ip = stream.peer_addr().map(|a| a.ip().to_string()).unwrap_or_default();
        if registry.lock().unwrap().blocked.contains(&ip) {
            let _ = stream.write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            return;
        }
        let device = super::device_label(&request);
        if path.starts_with("/control") {
            let seat = super::Seat::join(&registry, super::PeerKind::Remote, &ip, &device);
            control_socket(stream, running, remote, seat);
            return;
        }
        if path.starts_with("/audio") {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            let seat = super::Seat::join(&registry, super::PeerKind::Audio, &ip, &device);
            audio_socket(stream, running, peers, ring, seat);
            return;
        }
        // Consume the request head, then answer.
        let mut buf = vec![0u8; n];
        let _ = stream.read_exact(&mut buf);
        let _ = stream.set_nodelay(true);
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
            p if p.starts_with("/remote") => {
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
    fn control_socket(stream: TcpStream, running: Arc<AtomicBool>, remote: super::RemoteHub, seat: super::Seat) {
        let (state, cmds, count, password) = remote;
        let _ = stream.set_nodelay(true);
        let Ok(mut ws) = tungstenite::accept(stream) else { return };
        let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(30)));
        count.fetch_add(1, Ordering::Relaxed);
        let mut sent = u64::MAX;
        // The password this socket proved it knows (None = not authorised, unless none is set).
        let mut granted: Option<Option<String>> = None;
        let mut told: Option<bool> = None;
        while running.load(Ordering::Relaxed) && !seat.kicked() {
            let current = password.lock().unwrap().clone();
            let allowed = current.is_none() || granted.as_ref() == Some(&current);
            if told != Some(allowed) {
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
                    let Ok(v) = serde_json::from_str::<serde_json::Value>(t.as_str()) else { continue };
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
                        cmds.lock().unwrap().push(v);
                    }
                }
                Ok(tungstenite::Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        count.fetch_sub(1, Ordering::Relaxed);
    }

    /// Viewer microphone: first a text message `rate:<hz>`, then binary little-endian f32 mono
    /// chunks. Resampled to 48 kHz and queued for the audio analyser.
    fn audio_socket(stream: TcpStream, running: Arc<AtomicBool>, peers: Arc<AtomicUsize>, ring: Arc<Mutex<VecDeque<f32>>>, seat: super::Seat) {
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
