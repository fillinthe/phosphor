//! Sending a show to another Phosphor on the network. The sender packs the show (`pack`), hands the file out from
//! its own Session server under a one-time token, and offers it to the other Phosphor's Session
//! (`POST /show-offer`). The other one asks its user ("… wants to send you a show. Accept?"); nothing is taken
//! without a yes. On a yes it pulls the file from the address that made the offer (never anywhere else), with
//! the sender's session password when it has one, unpacks it and opens it. Both sides show progress and can cancel.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::job::{self, Shared};

/// Where offers go, and where packs are handed out (`/pack/<token>`).
pub const OFFER_PATH: &str = "/show-offer";
pub const PACK_PATH: &str = "/pack/";
/// The sender's session password, when it has one.
pub const PASSWORD_HEADER: &str = "X-Phosphor-Password";

/// Most offers waiting for an answer at once (from different addresses).
const MAX_WAITING: usize = 4;
/// An offer nobody answered is dropped after this long; an answered one is forgotten after it too.
const KEEP: Duration = Duration::from_secs(15 * 60);
/// Most bytes an offer's body may have.
pub const MAX_OFFER_BODY: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OfferState {
    Waiting,
    Accepted,
    Declined,
    /// The sender took it back, or the receiver canceled the download.
    Stopped,
}

impl OfferState {
    fn word(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Stopped => "stopped",
        }
    }

    fn from_word(w: &str) -> Option<Self> {
        [Self::Waiting, Self::Accepted, Self::Declined, Self::Stopped].into_iter().find(|s| s.word() == w)
    }
}

/// A show another Phosphor wants to send here.
#[derive(Clone, Debug)]
pub struct Offer {
    pub id: u64,
    /// "Phosphor on Studio Mac".
    pub from: String,
    /// The show's name.
    pub name: String,
    pub bytes: u64,
    /// The address the offer came from, and its Session's port: the pack is pulled from there and nowhere else.
    pub ip: String,
    pub port: u16,
    pub token: String,
    /// The sender's session has a password (asked for before downloading).
    pub locked: bool,
    pub state: OfferState,
    at: Instant,
}

impl Offer {
    pub fn pack_url(&self) -> String {
        let host = if self.ip.contains(':') { format!("[{}]", self.ip) } else { self.ip.clone() };
        format!("http://{host}:{}{PACK_PATH}{}", self.port, self.token)
    }
}

/// Offers made to this Phosphor. Nothing here ever accepts one: only `answer` (the user) does.
#[derive(Default)]
pub struct Inbox {
    offers: Vec<Offer>,
    next: u64,
}

pub type SharedInbox = Arc<Mutex<Inbox>>;

fn clean(text: &str, max: usize) -> String {
    text.chars().filter(|c| !c.is_control()).take(max).collect::<String>().trim().to_string()
}

impl Inbox {
    /// An offer arrived from `ip` (the JSON body: from, name, bytes, port, token, locked). Refused when it's
    /// malformed, when that address already has one waiting, or when too many wait.
    pub fn offered(&mut self, ip: &str, body: &[u8], now: Instant) -> Result<u64, &'static str> {
        self.prune(now);
        let v: Value = serde_json::from_slice(body).map_err(|_| "not an offer")?;
        let token = v["token"].as_str().unwrap_or_default();
        let port = v["port"].as_u64().filter(|p| (1..=65535).contains(p)).ok_or("no port")? as u16;
        let bytes = v["bytes"].as_u64().filter(|b| *b > 0).ok_or("no size")?;
        if token.len() != 32 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("bad token");
        }
        let waiting = self.offers.iter().filter(|o| o.state == OfferState::Waiting);
        if waiting.clone().any(|o| o.ip == ip) {
            return Err("an offer from there is already waiting");
        }
        if waiting.count() >= MAX_WAITING {
            return Err("too many offers waiting");
        }
        self.next += 1;
        let from = clean(v["from"].as_str().unwrap_or_default(), 80);
        let name = clean(v["name"].as_str().unwrap_or_default(), 120);
        self.offers.push(Offer {
            id: self.next,
            from: if from.is_empty() { format!("A Phosphor at {ip}") } else { from },
            name: if name.is_empty() { "Show".into() } else { name },
            bytes,
            ip: ip.to_string(),
            port,
            token: token.to_string(),
            locked: v["locked"].as_bool().unwrap_or(false),
            state: OfferState::Waiting,
            at: now,
        });
        Ok(self.next)
    }

    /// The oldest offer still waiting for an answer.
    pub fn waiting(&self) -> Option<Offer> {
        self.offers.iter().find(|o| o.state == OfferState::Waiting).cloned()
    }

    /// The user's answer to a waiting offer; the offer when it was one.
    pub fn answer(&mut self, id: u64, accept: bool) -> Option<Offer> {
        let o = self.offers.iter_mut().find(|o| o.id == id && o.state == OfferState::Waiting)?;
        o.state = if accept { OfferState::Accepted } else { OfferState::Declined };
        Some(o.clone())
    }

    /// The download of an accepted offer was canceled here (the sender sees it stop).
    pub fn stopped(&mut self, id: u64) {
        if let Some(o) = self.offers.iter_mut().find(|o| o.id == id) {
            o.state = OfferState::Stopped;
        }
    }

    /// The sender takes its offer back (it must know the offer's token).
    pub fn withdraw(&mut self, id: u64, token: &str) -> bool {
        match self.offers.iter_mut().find(|o| o.id == id && o.token == token) {
            Some(o) => {
                if o.state == OfferState::Waiting {
                    o.state = OfferState::Stopped;
                }
                true
            }
            None => false,
        }
    }

    /// Where an offer stands, for its sender (who must know the token).
    pub fn state(&self, id: u64, token: &str) -> Option<OfferState> {
        self.offers.iter().find(|o| o.id == id && o.token == token).map(|o| o.state)
    }

    fn prune(&mut self, now: Instant) {
        self.offers.retain(|o| now.duration_since(o.at) < KEEP);
    }
}

/// A pack this Phosphor hands out (one at a time).
pub struct Outgoing {
    pub token: String,
    pub path: PathBuf,
    pub size: u64,
    /// Bytes of the current (or last) download so far.
    pub sent: AtomicU64,
    /// A download got the whole file.
    pub complete: AtomicBool,
    /// Stop handing it out (the sender canceled).
    pub stop: AtomicBool,
}

/// What the Session server shares with the app for show transfers.
#[derive(Clone, Default)]
pub struct Hub {
    pub inbox: SharedInbox,
    pub outgoing: Arc<Mutex<Option<Arc<Outgoing>>>>,
}

/// The id (and `?token=`) in "/show-offer/12?token=…" or "/show-offer/12/withdraw?token=…".
fn offer_ref(path: &str) -> Option<(u64, String, bool)> {
    let rest = path.strip_prefix(OFFER_PATH)?.strip_prefix('/')?;
    let (head, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (id, withdraw) = match head.split_once('/') {
        Some((id, "withdraw")) => (id, true),
        Some(_) => return None,
        None => (head, false),
    };
    let token = query.split('&').find_map(|kv| kv.strip_prefix("token="))?.to_string();
    Some((id.parse().ok()?, token, withdraw))
}

fn reply(stream: &mut impl Write, status: &str, kind: &str, body: &[u8]) {
    let _ = stream.write_all(format!("HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).as_bytes());
    let _ = stream.write_all(body);
}

/// The Session server's part: offers (`POST /show-offer`, `GET /show-offer/<id>?token=`, `POST …/withdraw`) and
/// handing out the pack (`GET /pack/<token>`). `password_ok` checks the password a request carries against the
/// session's (true when it has none); `stay` = keep sending.
pub fn serve(stream: &mut impl Write, post: bool, path: &str, ip: &str, body: &[u8], hub: &Hub, password_ok: impl FnOnce() -> bool, stay: impl Fn() -> bool) {
    if let Some(token) = path.strip_prefix(PACK_PATH) {
        let out = hub.outgoing.lock().unwrap().clone().filter(|o| o.token == token && !o.stop.load(Ordering::Relaxed));
        let Some(out) = out else { return reply(stream, "404 Not Found", "text/plain", b"not offered") };
        if !password_ok() {
            return reply(stream, "401 Unauthorized", "text/plain", b"password");
        }
        send_file(stream, &out, stay);
        return;
    }
    if path == OFFER_PATH && post {
        let answer = hub.inbox.lock().unwrap().offered(ip, body, Instant::now());
        return match answer {
            Ok(id) => reply(stream, "202 Accepted", "application/json", json!({ "id": id }).to_string().as_bytes()),
            Err(why) => reply(stream, "429 Too Many Requests", "text/plain", why.as_bytes()),
        };
    }
    let Some((id, token, withdraw)) = offer_ref(path) else { return reply(stream, "404 Not Found", "text/plain", b"") };
    let mut inbox = hub.inbox.lock().unwrap();
    if withdraw && post {
        let ok = inbox.withdraw(id, &token);
        drop(inbox);
        return reply(stream, if ok { "200 OK" } else { "404 Not Found" }, "text/plain", b"");
    }
    match inbox.state(id, &token) {
        Some(s) => reply(stream, "200 OK", "application/json", json!({ "state": s.word() }).to_string().as_bytes()),
        None => reply(stream, "404 Not Found", "text/plain", b""),
    }
}

fn send_file(stream: &mut impl Write, out: &Outgoing, stay: impl Fn() -> bool) {
    let Ok(mut file) = std::fs::File::open(&out.path) else { return reply(stream, "410 Gone", "text/plain", b"") };
    out.sent.store(0, Ordering::Relaxed);
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: application/zip\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", out.size);
    if stream.write_all(head.as_bytes()).is_err() {
        return;
    }
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        if out.stop.load(Ordering::Relaxed) || !stay() {
            return;
        }
        let n = match std::io::Read::read(&mut file, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => return,
        };
        if stream.write_all(&buf[..n]).is_err() {
            return;
        }
        out.sent.fetch_add(n as u64, Ordering::Relaxed);
    }
    let _ = stream.flush();
    if out.sent.load(Ordering::Relaxed) >= out.size {
        out.complete.store(true, Ordering::Relaxed);
    }
}

/// Another Phosphor to send to: its Session's address and port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub name: String,
    pub address: String,
    pub port: u16,
}

impl Target {
    fn url(&self, path: &str) -> String {
        let host = if self.address.contains(':') { format!("[{}]", self.address) } else { self.address.clone() };
        format!("http://{host}:{}{path}", self.port)
    }
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(5)).timeout_read(Duration::from_secs(30)).timeout_write(Duration::from_secs(30)).build()
}

/// What sending needs from the app: the show, its name, who to, this Session's port, whether it has a password,
/// what this Phosphor is called, and the Session's hub (where the pack is handed out).
pub struct Send {
    pub comp: phosphor_engine::Composition,
    pub name: String,
    pub to: Target,
    pub port: u16,
    pub locked: bool,
    pub me: String,
    pub hub: Hub,
}

/// Packs the show, offers it, and hands it out until the other Phosphor has it all (or says no, or anyone stops).
pub fn send(s: Send) -> Shared {
    job::spawn("phosphor-send-show", move |p| {
        let dir = std::env::temp_dir().join(format!("phosphor-send-{}", std::process::id()));
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let file = crate::pack::free_path(&dir, &crate::pack::file_name(&s.name), crate::pack::EXT);
        let result = send_packed(&s, &file, p);
        // The pack was only for this: never left in the temp folder.
        let _ = std::fs::remove_file(&file);
        *s.hub.outgoing.lock().unwrap() = None;
        result
    })
}

fn send_packed(s: &Send, file: &Path, p: &Shared) -> Result<(), String> {
    crate::pack::write(s.comp.clone(), &s.name, file, p)?;
    // (Missing media was said while packing; the show still goes.)
    let missing = p.lock().unwrap().error.take();
    let size = std::fs::metadata(file).map_err(|e| e.to_string())?.len();
    let token = crate::net::new_token();
    let out = Arc::new(Outgoing { token: token.clone(), path: file.to_path_buf(), size, sent: AtomicU64::new(0), complete: AtomicBool::new(false), stop: AtomicBool::new(false) });
    *s.hub.outgoing.lock().unwrap() = Some(out.clone());
    {
        let mut g = p.lock().unwrap();
        (g.bytes, g.total, g.step) = (0, size, format!("Asking {}…", s.to.name));
    }
    let agent = agent();
    let offer = json!({ "from": s.me, "name": s.name, "bytes": size, "port": s.port, "token": token, "locked": s.locked }).to_string();
    let answer = agent.post(&s.to.url(OFFER_PATH)).timeout(Duration::from_secs(10)).set("Content-Type", "application/json").send_string(&offer);
    let id = match answer {
        Ok(r) => r.into_string().ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["id"].as_u64()).ok_or("The other Phosphor gave an odd answer")?,
        Err(ureq::Error::Status(_, r)) => return Err(format!("{} can't take a show now ({})", s.to.name, r.into_string().unwrap_or_default())),
        Err(e) => return Err(format!("Couldn't reach {}: {e}", s.to.name)),
    };
    let status_url = s.to.url(&format!("{OFFER_PATH}/{id}?token={token}"));
    let withdraw = || {
        out.stop.store(true, Ordering::Relaxed);
        let _ = agent.post(&s.to.url(&format!("{OFFER_PATH}/{id}/withdraw?token={token}"))).timeout(Duration::from_secs(3)).send_string("");
    };
    let began = Instant::now();
    let mut asked = Instant::now() - Duration::from_secs(5);
    let mut state = OfferState::Waiting;
    loop {
        if job::canceled(p) {
            withdraw();
            return Err(job::CANCELED.into());
        }
        if out.complete.load(Ordering::Relaxed) {
            let mut g = p.lock().unwrap();
            g.bytes = size;
            g.step = format!("Sent to {}", s.to.name);
            g.error = missing;
            return Ok(());
        }
        if asked.elapsed() >= Duration::from_secs(1) {
            asked = Instant::now();
            if let Some(now) = agent.get(&status_url).timeout(Duration::from_secs(4)).call().ok().and_then(|r| r.into_string().ok()).and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v["state"].as_str().and_then(OfferState::from_word)) {
                state = now;
            }
        }
        match state {
            OfferState::Declined => return Err(format!("{} said no", s.to.name)),
            OfferState::Stopped => return Err(format!("{} stopped taking the show", s.to.name)),
            OfferState::Waiting if began.elapsed() > KEEP => {
                withdraw();
                return Err(format!("No answer from {}", s.to.name));
            }
            OfferState::Waiting => job::step(p, format!("Waiting for {} to say yes…", s.to.name)),
            OfferState::Accepted => {
                let mut g = p.lock().unwrap();
                g.bytes = out.sent.load(Ordering::Relaxed);
                g.step = format!("Sending to {}…", s.to.name);
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Takes an accepted offer: downloads the pack into `dir` (with `password` when the sender has one), unpacks it
/// there and leaves the project to open in `output`. A cancel tells the sender.
pub fn receive(offer: Offer, password: String, dir: PathBuf, inbox: SharedInbox) -> Shared {
    job::spawn("phosphor-receive-show", move |p| {
        let result = receive_into(&offer, &password, &dir, p);
        if result.as_ref().is_err_and(|e| e == job::CANCELED) {
            inbox.lock().unwrap().stopped(offer.id);
        }
        result
    })
}

fn receive_into(offer: &Offer, password: &str, dir: &Path, p: &Shared) -> Result<(), String> {
    job::step(p, format!("Asking {} for \"{}\"…", offer.from, offer.name));
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let answer = agent().get(&offer.pack_url()).set(PASSWORD_HEADER, password).call();
    let r = match answer {
        Ok(r) => r,
        Err(ureq::Error::Status(401, _)) => return Err(WRONG_PASSWORD.into()),
        Err(ureq::Error::Status(404 | 410, _)) => return Err(format!("{} isn't offering the show anymore", offer.from)),
        Err(e) => return Err(format!("Couldn't reach {}: {e}", offer.from)),
    };
    let size = r.header("Content-Length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(offer.bytes);
    {
        let mut g = p.lock().unwrap();
        (g.bytes, g.total) = (0, size);
        g.step = format!("Receiving \"{}\" from {}…", offer.name, offer.from);
    }
    let name = crate::pack::file_name(&offer.name);
    let pack = crate::pack::free_path(dir, &name, crate::pack::EXT);
    let part = dir.join(format!(".{name}.{}.part", offer.id));
    let got = (|| {
        let mut out = std::io::BufWriter::with_capacity(1 << 20, std::fs::File::create(&part).map_err(|e| e.to_string())?);
        let n = job::copy(&mut r.into_reader(), &mut out, p)?;
        out.flush().map_err(|e| e.to_string())?;
        if n != size {
            return Err(format!("The show stopped arriving ({} of {})", crate::cache_store::size_text(n), crate::cache_store::size_text(size)));
        }
        std::fs::rename(&part, &pack).map_err(|e| e.to_string())
    })();
    if let Err(e) = got {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    {
        let mut g = p.lock().unwrap();
        (g.bytes, g.total) = (0, 0);
    }
    let into = crate::pack::folder_beside(&pack);
    let opened = crate::pack::unpack(&pack, &into, p);
    // The folder holds everything now; the pack would only take the space twice.
    let _ = std::fs::remove_file(&pack);
    opened.map(|_| ())
}

/// What a download with the wrong (or no) password ends with: the prompt asks for it again.
pub const WRONG_PASSWORD: &str = "Wrong password for the sender's Session";

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn body(token: &str) -> Vec<u8> {
        json!({ "from": "Phosphor on Stage Mac", "name": "Club Night", "bytes": 2_300_000_000u64, "port": 8777, "token": token, "locked": true }).to_string().into_bytes()
    }

    #[test]
    fn an_offer_waits_for_a_yes_and_is_never_taken_by_itself() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        let id = inbox.offered("10.0.0.5", &body(TOKEN), now).unwrap();
        let o = inbox.waiting().unwrap();
        assert_eq!((o.id, o.from.as_str(), o.name.as_str(), o.bytes, o.locked), (id, "Phosphor on Stage Mac", "Club Night", 2_300_000_000, true));
        assert_eq!(o.pack_url(), format!("http://10.0.0.5:8777/pack/{TOKEN}"), "pulled from the address that offered, nowhere else");
        assert_eq!(inbox.state(id, TOKEN), Some(OfferState::Waiting));
        // The same address can't pile up offers; nobody can look without the token.
        assert!(inbox.offered("10.0.0.5", &body(TOKEN), now).is_err());
        assert_eq!(inbox.state(id, "ffffffffffffffffffffffffffffffff"), None);
        // No → declined, and it no longer asks.
        assert_eq!(inbox.answer(id, false).map(|o| o.state), Some(OfferState::Declined));
        assert!(inbox.waiting().is_none());
        assert!(inbox.answer(id, true).is_none(), "an answered offer can't be answered again");
        assert_eq!(inbox.state(id, TOKEN), Some(OfferState::Declined));
        // A new one from there can come now; yes → accepted.
        let id2 = inbox.offered("10.0.0.5", &body(TOKEN), now).unwrap();
        assert_eq!(inbox.answer(id2, true).map(|o| o.state), Some(OfferState::Accepted));
        inbox.stopped(id2);
        assert_eq!(inbox.state(id2, TOKEN), Some(OfferState::Stopped));
    }

    #[test]
    fn bad_offers_are_refused_and_old_ones_go() {
        let mut inbox = Inbox::default();
        let now = Instant::now();
        assert!(inbox.offered("10.0.0.5", b"not json", now).is_err());
        assert!(inbox.offered("10.0.0.5", &body("short"), now).is_err());
        assert!(inbox.offered("10.0.0.5", br#"{"token":"0123456789abcdef0123456789abcdef","port":8777,"bytes":0}"#, now).is_err());
        for i in 0..MAX_WAITING {
            inbox.offered(&format!("10.0.0.{i}"), &body(TOKEN), now).unwrap();
        }
        assert!(inbox.offered("10.0.0.99", &body(TOKEN), now).is_err(), "too many waiting");
        // Taking an offer back needs its token.
        let id = inbox.waiting().unwrap().id;
        assert!(!inbox.withdraw(id, "ffffffffffffffffffffffffffffffff"));
        assert!(inbox.withdraw(id, TOKEN));
        assert_eq!(inbox.state(id, TOKEN), Some(OfferState::Stopped));
        // Unanswered for long enough: gone.
        assert!(inbox.offered("10.0.0.99", &body(TOKEN), now + KEEP + Duration::from_secs(1)).is_ok());
        assert_eq!(inbox.offers.len(), 1);
    }

    #[test]
    fn offer_paths_read_back() {
        assert_eq!(offer_ref(&format!("/show-offer/12?token={TOKEN}")), Some((12, TOKEN.to_string(), false)));
        assert_eq!(offer_ref(&format!("/show-offer/12/withdraw?token={TOKEN}")), Some((12, TOKEN.to_string(), true)));
        assert_eq!(offer_ref("/show-offer/12"), None);
        assert_eq!(offer_ref("/show-offer/x?token=a"), None);
    }

    fn wait(p: &Shared, secs: u64) {
        let began = Instant::now();
        while !p.lock().unwrap().done {
            assert!(began.elapsed() < Duration::from_secs(secs), "timed out: {:?}", p.lock().unwrap());
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Two ends over real HTTP (one Session plays both): offered, asked, refused without the password, then sent,
    /// unpacked and found.
    #[test]
    fn a_show_goes_from_one_phosphor_to_another() {
        let root = std::env::temp_dir().join(format!("phosphor-send-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let clip = root.join("clip.mov");
        let data: Vec<u8> = (0..(2 << 20) + 5).map(|i| (i % 253) as u8).collect();
        std::fs::write(&clip, &data).unwrap();
        let mut comp = phosphor_engine::Composition::default();
        comp.add_media("clip.mov", &clip.to_string_lossy());

        let mut net = crate::net::NetSession::new();
        let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        net.port = port;
        net.start().unwrap();
        net.set_password(Some("secret".into()));
        let hub = net.shows.clone();
        let sending = send(Send {
            comp,
            name: "Club Night".into(),
            to: Target { name: "the other Mac".into(), address: "127.0.0.1".into(), port },
            port,
            locked: true,
            me: "Phosphor on This Mac".into(),
            hub: hub.clone(),
        });
        // The offer arrives and waits; the sender waits too.
        let began = Instant::now();
        let offer = loop {
            if let Some(o) = hub.inbox.lock().unwrap().waiting() {
                break o;
            }
            assert!(began.elapsed() < Duration::from_secs(20) && !sending.lock().unwrap().done, "no offer: {:?}", sending.lock().unwrap());
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!((offer.name.as_str(), offer.from.as_str(), offer.locked), ("Club Night", "Phosphor on This Mac", true));
        std::thread::sleep(Duration::from_millis(300));
        assert!(!sending.lock().unwrap().done, "nothing goes before a yes");
        let offer = hub.inbox.lock().unwrap().answer(offer.id, true).unwrap();
        let dir = root.join("Phosphor Shows");
        let wrong = receive(offer.clone(), "nope".into(), dir.clone(), hub.inbox.clone());
        wait(&wrong, 20);
        assert_eq!(wrong.lock().unwrap().error.as_deref(), Some(WRONG_PASSWORD));
        // (Each wrong password makes that address wait a moment before the next try.)
        std::thread::sleep(Duration::from_millis(700));
        let right = receive(offer, "secret".into(), dir.clone(), hub.inbox.clone());
        wait(&right, 30);
        let got = right.lock().unwrap().clone();
        assert!(got.error.is_none(), "{got:?}");
        let project = got.output.unwrap();
        assert_eq!(project, dir.join("Club Night").join("Club Night.phosphor"));
        let back = phosphor_engine::Composition::from_json(&std::fs::read_to_string(&project).unwrap()).unwrap();
        let media = back.media_paths();
        assert_eq!(media.len(), 1);
        assert_eq!(std::fs::read(&media[0]).unwrap(), data);
        assert!(!dir.join("Club Night.phosphorpack").exists(), "the pack is cleared away once unpacked");
        wait(&sending, 20);
        let sent = sending.lock().unwrap().clone();
        assert!(sent.error.is_none() && sent.step.starts_with("Sent"), "{sent:?}");
        assert!(hub.outgoing.lock().unwrap().is_none(), "nothing is handed out after");
        net.stop();
        let _ = std::fs::remove_dir_all(&root);
    }
}
