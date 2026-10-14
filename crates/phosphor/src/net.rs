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

/// The remote-control page's view of the project: (version, JSON). Bumped when anything changes.
pub type RemoteState = Arc<Mutex<(u64, Arc<String>)>>;
/// Commands from phones/tablets, drained by the app each frame.
pub type RemoteCmds = Arc<Mutex<Vec<serde_json::Value>>>;
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
    encoder: Option<SyncSender<(u32, u32, Vec<u8>)>>,
    next_frame: f64,
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
            encoder: None,
            next_frame: 0.0,
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
        let remote = (self.remote_state.clone(), self.remote_cmds.clone(), self.remotes.clone());
        std::thread::Builder::new()
            .name("phosphor-net".into())
            .spawn(move || {
                while running.load(Ordering::Relaxed) {
                    match listener.accept() {
                        Ok((stream, _)) => {
                            let (running, latest, viewers, peers, ring, remote) =
                                (running.clone(), latest.clone(), viewers.clone(), peers.clone(), ring.clone(), remote.clone());
                            std::thread::spawn(move || native::serve(stream, running, latest, viewers, peers, ring, remote));
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
        if !self.running() || now < self.next_frame {
            return false;
        }
        let step = 1.0 / self.fps.max(1) as f64;
        self.next_frame = (self.next_frame + step).max(now - step);
        true
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
        remote: (super::RemoteState, super::RemoteCmds, Arc<AtomicUsize>),
    ) {
        // Sockets accepted from a non-blocking listener inherit that flag on macOS/BSD.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut head = [0u8; 2048];
        let Ok(n) = stream.peek(&mut head) else { return };
        let request = String::from_utf8_lossy(&head[..n]).to_string();
        let path = request.split_whitespace().nth(1).unwrap_or("/").to_string();
        if path.starts_with("/control") {
            control_socket(stream, running, remote);
            return;
        }
        if path.starts_with("/audio") {
            let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
            audio_socket(stream, running, peers, ring);
            return;
        }
        // Consume the request head, then answer.
        let mut buf = vec![0u8; n];
        let _ = stream.read_exact(&mut buf);
        let _ = stream.set_nodelay(true);
        match path.as_str() {
            "/stream.mjpeg" => {
                viewers.fetch_add(1, Ordering::Relaxed);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: multipart/x-mixed-replace; boundary=frame\r\nCache-Control: no-cache\r\nConnection: close\r\n\r\n",
                );
                let mut last: Option<Frame> = None;
                while running.load(Ordering::Relaxed) {
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
    fn control_socket(stream: TcpStream, running: Arc<AtomicBool>, remote: (super::RemoteState, super::RemoteCmds, Arc<AtomicUsize>)) {
        let (state, cmds, count) = remote;
        let _ = stream.set_nodelay(true);
        let Ok(mut ws) = tungstenite::accept(stream) else { return };
        let _ = ws.get_mut().set_read_timeout(Some(Duration::from_millis(30)));
        count.fetch_add(1, Ordering::Relaxed);
        let mut sent = u64::MAX;
        while running.load(Ordering::Relaxed) {
            let (version, json) = state.lock().unwrap().clone();
            if version != sent && !json.is_empty() {
                if ws.send(tungstenite::Message::Text(json.as_str().into())).is_err() {
                    break;
                }
                sent = version;
            }
            match ws.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(t.as_str()) {
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
    fn audio_socket(stream: TcpStream, running: Arc<AtomicBool>, peers: Arc<AtomicUsize>, ring: Arc<Mutex<VecDeque<f32>>>) {
        let Ok(mut ws) = tungstenite::accept(stream) else { return };
        peers.fetch_add(1, Ordering::Relaxed);
        let mut rate = 48_000.0f32;
        while running.load(Ordering::Relaxed) {
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
