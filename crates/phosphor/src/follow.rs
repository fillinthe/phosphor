//! Several Phosphors, one show: a follower plays what a leader plays, on its own outputs.
//!
//! The leader runs a Session (Network panel); followers connect to its `/follow` socket. The leader sends its
//! project whenever it changes (checked 10 times a second) and a clock message every frame: its show time, beats,
//! tempo and each movie's position. A follower takes the project but keeps its own outputs and mapping, measures
//! the leader's time with pings (`Link::show_time`), renders at that time (shaders, fades, modulators line up),
//! sets its beat clock to the leader's, and nudges movies that drift. Media must be at the same paths on both.

use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use phosphor_engine::Composition;
use serde_json::{Value, json};

/// How often the leader checks its project for changes.
const PROJECT_EVERY: f64 = 0.1;
/// A movie further than this from the leader's position is moved (seconds).
const DRIFT: f64 = 0.15;

/// What followers get from the leader's session server: the project (version, JSON) and the latest clock
/// message (version, JSON, when it was made, the leader's show time then).
#[derive(Default)]
pub struct Feed {
    pub project: (u64, Arc<String>),
    pub clock: (u64, Arc<String>),
    /// Leader show time when the clock message was made, and when (for pongs between frames).
    pub stamp: Option<(f64, Instant)>,
}

pub type SharedFeed = Arc<Mutex<Feed>>;

/// Leader side, once per frame: fills the feed while anyone follows.
#[derive(Default)]
pub struct Leader {
    next_check: f64,
    last_json: Arc<String>,
}

impl Leader {
    /// `positions`: (layer, seconds) of each playing movie.
    pub fn publish(&mut self, feed: &SharedFeed, comp: &Composition, now: f64, beats: f64, bpm: f32, positions: &[(u64, f64)]) {
        let clock = json!({"t": "clock", "now": now, "beats": beats, "bpm": bpm, "pos": positions}).to_string();
        let mut f = feed.lock().unwrap();
        f.clock = (f.clock.0 + 1, Arc::new(clock));
        f.stamp = Some((now, Instant::now()));
        if now >= self.next_check {
            self.next_check = now + PROJECT_EVERY;
            let project = comp.to_json();
            if *self.last_json != project {
                self.last_json = Arc::new(project);
                f.project = (f.project.0 + 1, self.last_json.clone());
            }
        }
    }
}

/// The `/follow` socket (leader side, on the session server's thread for this connection).
pub fn serve(mut ws: tungstenite::WebSocket<crate::net_tls::Replay>, feed: SharedFeed, running: impl Fn() -> bool) {
    let (mut sent_project, mut sent_clock) = (u64::MAX, u64::MAX);
    while running() {
        let (project, clock) = {
            let f = feed.lock().unwrap();
            (f.project.clone(), f.clock.clone())
        };
        if project.0 != sent_project && !project.1.is_empty() {
            let msg = format!(r#"{{"t":"project","json":{}}}"#, project.1);
            if ws.send(tungstenite::Message::Text(msg.into())).is_err() {
                break;
            }
            sent_project = project.0;
        }
        if clock.0 != sent_clock && !clock.1.is_empty() {
            if ws.send(tungstenite::Message::Text(clock.1.as_str().into())).is_err() {
                break;
            }
            sent_clock = clock.0;
        }
        match ws.read() {
            Ok(tungstenite::Message::Text(t)) => {
                let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                if v.get("t").and_then(Value::as_str) == Some("ping") {
                    // The leader's show time right now: the last frame's, plus the time since.
                    let now = feed.lock().unwrap().stamp.map(|(t, at)| t + at.elapsed().as_secs_f64());
                    let pong = json!({"t": "pong", "at": v.get("at"), "now": now}).to_string();
                    if ws.send(tungstenite::Message::Text(pong.into())).is_err() {
                        break;
                    }
                }
            }
            Ok(tungstenite::Message::Close(_)) => break,
            Ok(_) => {}
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
            Err(_) => break,
        }
    }
}

/// The leader's time as the follower measured it: at `at` (local) it was `leader` seconds; best round trip so far.
#[derive(Clone, Copy, Debug)]
struct TimeFix {
    at: Instant,
    leader: f64,
    rtt: f64,
}

enum Incoming {
    Project(String),
    Clock(Value),
    Status(String),
}

/// Follower side: the connection to a leader.
pub struct Link {
    pub address: String,
    rx: mpsc::Receiver<Incoming>,
    fix: Arc<Mutex<Option<TimeFix>>>,
    stop: Arc<std::sync::atomic::AtomicBool>,
    pub status: String,
    /// The last clock message (for beats and movie positions).
    clock: Option<Value>,
    /// Movies moved to the leader's position lately: (layer, when).
    nudged: Vec<(u64, f64)>,
    /// Projects received.
    pub projects: u64,
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl Link {
    /// Starts following the Phosphor at `address` ("host" or "host:port"; the session's port by default).
    pub fn connect(address: &str) -> Link {
        let address = address.trim().trim_start_matches("http://").trim_end_matches('/').to_string();
        let host = if address.contains(':') { address.clone() } else { format!("{address}:{}", crate::net::DEFAULT_PORT) };
        let (tx, rx) = mpsc::channel();
        let fix: Arc<Mutex<Option<TimeFix>>> = Arc::new(Mutex::new(None));
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let (fix2, stop2) = (fix.clone(), stop.clone());
        let _ = std::thread::Builder::new().name("follow".into()).spawn(move || run(&host, tx, fix2, stop2));
        Link { address, rx, fix, stop, status: "connecting…".into(), clock: None, nudged: Vec::new(), projects: 0 }
    }

    /// The leader's show time now (None until the first ping came back).
    pub fn show_time(&self) -> Option<f64> {
        let f = (*self.fix.lock().unwrap())?;
        Some(f.leader + f.at.elapsed().as_secs_f64())
    }

    /// Round trip to the leader, in seconds (the best seen).
    pub fn rtt(&self) -> Option<f64> {
        self.fix.lock().unwrap().map(|f| f.rtt)
    }

    /// Once per frame: the newest project (to load), and the clock messages applied (`clock_now`).
    pub fn poll(&mut self) -> Option<String> {
        let mut project = None;
        for m in self.rx.try_iter() {
            match m {
                Incoming::Project(p) => {
                    self.projects += 1;
                    project = Some(p);
                }
                Incoming::Clock(c) => self.clock = Some(c),
                Incoming::Status(s) => self.status = s,
            }
        }
        project
    }

    /// The leader's tempo and its beat count at show time `show` (from the last clock message).
    pub fn beats(&self, show: f64) -> Option<(f32, f64)> {
        let c = self.clock.as_ref()?;
        let bpm = c.get("bpm")?.as_f64()? as f32;
        let (at, beats) = (c.get("now")?.as_f64()?, c.get("beats")?.as_f64()?);
        Some((bpm, beats + (show - at) * bpm as f64 / 60.0))
    }

    /// Movies to move to the leader's position: (layer, seconds), given where ours are (`ours`), at show time `show`.
    pub fn drifted(&mut self, ours: &[(u64, f64)], show: f64) -> Vec<(u64, f64)> {
        let Some(c) = &self.clock else { return Vec::new() };
        let at = c.get("now").and_then(Value::as_f64).unwrap_or(show);
        let pos: Vec<(u64, f64)> = c.get("pos").and_then(|p| serde_json::from_value(p.clone()).ok()).unwrap_or_default();
        self.nudged.retain(|(_, t)| show - t < 2.0);
        let mut out = Vec::new();
        for (layer, p) in pos {
            let want = p + (show - at).clamp(0.0, 0.5);
            if let Some((_, mine)) = ours.iter().find(|(l, _)| *l == layer)
                && (mine - want).abs() > DRIFT
                && !self.nudged.iter().any(|(l, _)| *l == layer)
            {
                self.nudged.push((layer, show));
                out.push((layer, want));
            }
        }
        out
    }
}

/// The connection thread: connect (and reconnect), pass messages on, ping every second to measure the time.
fn run(host: &str, tx: mpsc::Sender<Incoming>, fix: Arc<Mutex<Option<TimeFix>>>, stop: Arc<std::sync::atomic::AtomicBool>) {
    let start = Instant::now();
    while !stop.load(std::sync::atomic::Ordering::Relaxed) {
        let stream = match std::net::TcpStream::connect_timeout(&match host.parse() {
            Ok(a) => a,
            Err(_) => match std::net::ToSocketAddrs::to_socket_addrs(host).ok().and_then(|mut a| a.next()) {
                Some(a) => a,
                None => {
                    let _ = tx.send(Incoming::Status(format!("can't find {host}")));
                    std::thread::sleep(Duration::from_secs(2));
                    continue;
                }
            },
        }, Duration::from_secs(3))
        {
            Ok(s) => s,
            Err(e) => {
                let _ = tx.send(Incoming::Status(format!("can't reach {host} ({e}): is its Session on?")));
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let _ = stream.set_nodelay(true);
        let Ok((mut ws, _)) = tungstenite::client(format!("ws://{host}/follow"), stream.try_clone().unwrap()) else {
            let _ = tx.send(Incoming::Status(format!("{host} didn't answer as a Phosphor")));
            std::thread::sleep(Duration::from_secs(2));
            continue;
        };
        let _ = stream.set_read_timeout(Some(Duration::from_millis(20)));
        let _ = tx.send(Incoming::Status("following".into()));
        let mut next_ping = Instant::now();
        while !stop.load(std::sync::atomic::Ordering::Relaxed) {
            if Instant::now() >= next_ping {
                next_ping = Instant::now() + Duration::from_millis(1000);
                let ping = json!({"t": "ping", "at": start.elapsed().as_secs_f64()}).to_string();
                if ws.send(tungstenite::Message::Text(ping.into())).is_err() {
                    break;
                }
            }
            match ws.read() {
                Ok(tungstenite::Message::Text(t)) => {
                    let Ok(v) = serde_json::from_str::<Value>(t.as_str()) else { continue };
                    match v.get("t").and_then(Value::as_str) {
                        Some("project") => {
                            if let Some(j) = v.get("json") {
                                let _ = tx.send(Incoming::Project(j.to_string()));
                            }
                        }
                        Some("clock") => {
                            let _ = tx.send(Incoming::Clock(v));
                        }
                        Some("pong") => {
                            let (Some(sent), Some(leader)) = (v.get("at").and_then(Value::as_f64), v.get("now").and_then(Value::as_f64)) else { continue };
                            let rtt = (start.elapsed().as_secs_f64() - sent).max(0.0);
                            let mut f = fix.lock().unwrap();
                            // Keep the measurement with the shortest round trip (the most exact), refreshed now and
                            // then so a clock that slowly drifts is followed.
                            let better = f.is_none_or(|old| rtt <= old.rtt * 1.5 || old.at.elapsed() > Duration::from_secs(20));
                            if better {
                                *f = Some(TimeFix { at: Instant::now(), leader: leader + rtt / 2.0, rtt });
                            }
                        }
                        _ => {}
                    }
                }
                Ok(tungstenite::Message::Close(_)) => break,
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        let _ = tx.send(Incoming::Status("lost the leader: reconnecting…".into()));
        std::thread::sleep(Duration::from_millis(500));
    }
}

/// The leader's project with this machine's own outputs (and the settings that belong to this machine's room).
pub fn merge(mut leader: Composition, local: &Composition) -> Composition {
    leader.outputs = local.outputs.clone();
    leader.mapping = local.mapping.clone();
    leader.syphon = local.syphon;
    leader.spout = local.spout;
    leader.ndi = local.ndi.clone();
    leader.dmx = local.dmx.clone();
    leader.outputs_on_open = local.outputs_on_open;
    leader.next_id = leader.next_id.max(local.next_id);
    leader
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_follower_keeps_its_own_outputs() {
        let mut leader = Composition::default();
        leader.add_layer();
        leader.ensure_outputs();
        let mut local = Composition::default();
        local.ensure_outputs();
        local.outputs[0].name = "Left wall".into();
        let merged = merge(leader.clone(), &local);
        assert_eq!(merged.layers.len(), 1);
        assert_eq!(merged.outputs[0].name, "Left wall");
    }

    #[test]
    fn drift_is_corrected_once_in_a_while() {
        let (_tx, rx) = mpsc::channel();
        let mut l = Link { address: String::new(), rx, fix: Arc::new(Mutex::new(None)), stop: Default::default(), status: String::new(), clock: Some(json!({"t":"clock","now": 10.0, "beats": 20.0, "bpm": 120.0, "pos": [[7, 3.0]]})), nudged: Vec::new(), projects: 0 };
        assert_eq!(l.beats(11.0), Some((120.0, 22.0)));
        assert!(l.drifted(&[(7, 3.05)], 10.0).is_empty(), "close enough");
        assert_eq!(l.drifted(&[(7, 2.5)], 10.0), vec![(7, 3.0)]);
        assert!(l.drifted(&[(7, 2.5)], 10.5).is_empty(), "not again straight away");
    }
}

impl crate::app::State {
    /// Once per rendered frame: a follower loads the leader's latest project, follows its beat and movies and
    /// returns the leader's show time; a leader with followers publishes. Returns the time to render at.
    pub fn follow_frame(&mut self, now: f64) -> f64 {
        if let Some(link) = self.following.as_mut() {
            if let Some(json) = link.poll() {
                match Composition::from_json(&json) {
                    Ok(c) => self.comp = merge(c, &self.comp),
                    Err(e) => link.status = format!("the leader's project didn't load: {e}"),
                }
            }
            let Some(show) = link.show_time() else { return now };
            if let Some((bpm, beats)) = link.beats(show) {
                self.clock.follow_audio = false;
                self.clock.follow_midi = false;
                self.clock.bpm = bpm;
                self.clock.set_beats(now, beats);
            }
            for (layer, t) in link.drifted(&self.media.positions(), show) {
                self.media.seek(layer, t);
            }
            return show;
        }
        if self.net.followers.load(std::sync::atomic::Ordering::Relaxed) > 0 {
            let positions = self.media.positions();
            self.leader.publish(&self.net.follow, &self.comp, now, self.clock.beats(now), self.clock.bpm, &positions);
        }
        now
    }
}
