//! Your phosphorfx.com account (optional in the Mac app, required in the browser): projects and your layouts are
//! kept in it. Free accounts keep a few projects; Pro buys space.
//! One client for the Mac app and the browser (ehttp). Browser: same origin, the HttpOnly session cookie travels
//! with every request. Mac app: a bearer token from a "desktop" sign-in, kept in Phosphor's settings folder.
//! Server: the account service. Replies arrive on a channel; `poll` once per frame applies them.

use eframe::egui;
use serde::Deserialize;
use std::sync::mpsc::{Receiver, Sender, channel};

/// The server refuses bigger projects; said before a long upload.
const MAX_PROJECT_BYTES: usize = 20 << 20;
/// Name of the account entry holding all your layouts (server "layouts" list).
const LAYOUTS_DOC: &str = "Phosphor layouts";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Entitlements {
    #[serde(default)]
    pub plan: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct User {
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub role: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub updated: i64,
}

#[derive(Deserialize, Default)]
struct AuthReply {
    #[serde(default)]
    user: Option<User>,
    #[serde(default)]
    entitlements: Option<Entitlements>,
    #[serde(default)]
    token: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default, rename = "twoFactor")]
    two_factor: bool,
}

#[derive(Deserialize, Default)]
struct Doc {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    data: String,
    #[serde(default)]
    error: Option<String>,
}

enum Reply {
    Auth(AuthReply),
    SignedOut,
    Expired,
    Projects(Vec<ProjectInfo>),
    Storage(u64, u64),
    Saved(String, String),
    Opened(String, String, String),
    Deleted,
    /// The layouts entry: its id (None = not saved yet).
    LayoutsFound(Option<String>),
    LayoutsText(String),
    LayoutsSaved(String),
    Error(String),
}

/// What the app should do after `poll`.
pub enum Event {
    /// A project came back from the account: (id, name, project JSON).
    Opened(String, String, String),
    /// Saved: (id, name).
    Saved(String, String),
    /// Your layouts as stored in the account (RON text from `app::layout`).
    Layouts(String),
}

pub struct Account {
    pub base: String,
    pub user: Option<User>,
    pub ent: Entitlements,
    pub projects: Vec<ProjectInfo>,
    /// (bytes used, bytes allowed)
    pub storage: Option<(u64, u64)>,
    pub error: String,
    pub busy: u32,
    pub checked: bool,
    /// The last sign-in needs the authenticator code.
    pub need_code: bool,
    started: bool,
    /// Account entry of the layouts, once looked up (Some(None) = none saved yet).
    layouts_doc: Option<Option<String>>,
    /// Last layouts text uploaded or downloaded, so unchanged layouts aren't sent again.
    layouts_sent: String,
    /// The account's layouts are on their way: don't upload over them before they arrive.
    layouts_waiting: bool,
    token: String,
    tx: Sender<Reply>,
    rx: Receiver<Reply>,
}

impl Default for Account {
    fn default() -> Self {
        let (tx, rx) = channel();
        #[cfg(target_arch = "wasm32")]
        let base = web_sys::window().and_then(|w| w.location().origin().ok()).unwrap_or_default();
        #[cfg(not(target_arch = "wasm32"))]
        let base = std::env::var("PHOSPHOR_CLOUD_URL").unwrap_or_else(|_| "https://phosphorfx.com".into());
        let mut a = Self { base, user: None, ent: Entitlements::default(), projects: Vec::new(), storage: None, error: String::new(), busy: 0, checked: false, need_code: false, started: false, layouts_doc: None, layouts_sent: String::new(), layouts_waiting: false, token: String::new(), tx, rx };
        a.token = load_token();
        a
    }
}

impl Account {
    pub fn web() -> bool {
        cfg!(target_arch = "wasm32")
    }

    pub fn signed_in(&self) -> bool {
        self.user.is_some()
    }

    /// Pro (or an admin): more space in the account.
    pub fn pro(&self) -> bool {
        self.ent.plan == "pro" || self.user.as_ref().is_some_and(|u| u.role.contains("admin"))
    }

    /// Finds the layouts entry and downloads it (arrives as `Event::Layouts`).
    fn fetch_layouts(&mut self, ctx: &egui::Context) {
        self.request(ctx, "GET", "/api/layouts", None, |status, text| {
            let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
            if status >= 400 {
                return Reply::Error(error_of(status, &v));
            }
            let docs: Vec<ProjectInfo> = serde_json::from_value(v.get("layouts").cloned().unwrap_or_default()).unwrap_or_default();
            Reply::LayoutsFound(docs.into_iter().find(|d| d.name == LAYOUTS_DOC).map(|d| d.id))
        });
    }

    /// Uploads your layouts if they changed since the last upload (call when they change; cheap otherwise).
    pub fn push_layouts(&mut self, ctx: &egui::Context, text: &str) {
        let Some(doc) = self.layouts_doc.clone() else { return }; // not looked up yet (or signed out)
        if self.user.is_none() || self.layouts_waiting || text == self.layouts_sent {
            return;
        }
        self.layouts_sent = text.to_string();
        let body = serde_json::json!({"name": LAYOUTS_DOC, "data": serde_json::to_string(text).unwrap_or_default()});
        let (method, path) = match doc {
            Some(id) => ("PUT", format!("/api/layouts/{id}")),
            None => {
                self.layouts_doc = None; // no second POST (duplicate entry) until this one answers
                ("POST", "/api/layouts".to_string())
            }
        };
        self.request(ctx, method, &path, Some(body), |status, text| match doc_of(status, text) {
            Ok(d) => Reply::LayoutsSaved(d.id),
            Err(e) => Reply::Error(e),
        });
    }

    fn request(&mut self, ctx: &egui::Context, method: &str, path: &str, body: Option<serde_json::Value>, on: impl 'static + Send + FnOnce(u16, &str) -> Reply) {
        let url = format!("{}{}", self.base, path);
        let mut req = match body {
            Some(b) => ehttp::Request::post(url, serde_json::to_vec(&b).unwrap_or_default()),
            None => ehttp::Request::get(url),
        };
        req.method = method.to_string();
        req.headers.insert("Content-Type", "application/json");
        if !self.token.is_empty() {
            req.headers.insert("Authorization", format!("Bearer {}", self.token));
        }
        #[cfg(target_arch = "wasm32")]
        {
            req.mode = ehttp::Mode::SameOrigin;
        }
        self.busy += 1;
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        let auth = path.starts_with("/api/auth/");
        ehttp::fetch(req, move |r| {
            let reply = match r {
                Ok(resp) if resp.status == 401 && !auth => Reply::Expired,
                Ok(resp) => on(resp.status, resp.text().unwrap_or("")),
                Err(e) => Reply::Error(format!("Can't reach phosphorfx.com right now ({e}). Your project is still open here.")),
            };
            let _ = tx.send(reply);
            ctx.request_repaint();
        });
    }

    fn auth_reply(status: u16, text: &str) -> Reply {
        let mut a: AuthReply = serde_json::from_str(text).unwrap_or_default();
        if status >= 400 && a.error.is_none() {
            a.error = Some(format!("Request failed ({status})"));
        }
        Reply::Auth(a)
    }

    /// Who is signed in (browser: the session cookie; Mac: the stored token). Call once at start.
    pub fn check_session(&mut self, ctx: &egui::Context) {
        if !Self::web() && self.token.is_empty() {
            self.checked = true;
            return;
        }
        self.request(ctx, "GET", "/api/auth/me", None, Self::auth_reply);
    }

    pub fn sign_in(&mut self, ctx: &egui::Context, email: &str, password: &str, code: &str) {
        let client = if Self::web() { "web" } else { "desktop" };
        let body = serde_json::json!({"email": email.trim(), "password": password, "code": code.trim(), "client": client, "label": "Phosphor"});
        self.request(ctx, "POST", "/api/auth/login", Some(body), Self::auth_reply);
    }

    pub fn sign_out(&mut self, ctx: &egui::Context) {
        self.request(ctx, "POST", "/api/auth/logout", Some(serde_json::json!({})), |_, _| Reply::SignedOut);
    }

    pub fn refresh(&mut self, ctx: &egui::Context) {
        self.request(ctx, "GET", "/api/projects", None, |status, text| {
            let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
            if status >= 400 {
                return Reply::Error(error_of(status, &v));
            }
            Reply::Projects(serde_json::from_value(v.get("projects").cloned().unwrap_or_default()).unwrap_or_default())
        });
        self.request(ctx, "GET", "/api/account/storage", None, |status, text| {
            let v: serde_json::Value = serde_json::from_str(text).unwrap_or_default();
            let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0);
            if status < 400 { Reply::Storage(n("usedBytes"), n("maxBytes")) } else { Reply::Error(error_of(status, &v)) }
        });
    }

    /// Saves `json` as a new project (`id` empty) or over an existing one.
    pub fn save(&mut self, ctx: &egui::Context, id: &str, name: &str, json: &str) {
        if json.len() > MAX_PROJECT_BYTES {
            self.error = format!("This project is {:.1} MB; projects in your account can be at most {} MB.", json.len() as f64 / 1_048_576.0, MAX_PROJECT_BYTES >> 20);
            return;
        }
        let body = serde_json::json!({"name": name, "data": json});
        let (method, path) = if id.is_empty() { ("POST", "/api/projects".to_string()) } else { ("PUT", format!("/api/projects/{id}")) };
        self.request(ctx, method, &path, Some(body), |status, text| match doc_of(status, text) {
            Ok(d) => Reply::Saved(d.id, d.name),
            Err(e) => Reply::Error(e),
        });
    }

    pub fn open(&mut self, ctx: &egui::Context, id: &str) {
        self.request(ctx, "GET", &format!("/api/projects/{id}"), None, |status, text| match doc_of(status, text) {
            Ok(d) => Reply::Opened(d.id, d.name, d.data),
            Err(e) => Reply::Error(e),
        });
    }

    pub fn delete(&mut self, ctx: &egui::Context, id: &str) {
        self.request(ctx, "DELETE", &format!("/api/projects/{id}"), None, |status, text| {
            if status < 400 { Reply::Deleted } else { Reply::Error(error_of(status, &serde_json::from_str(text).unwrap_or_default())) }
        });
    }

    /// Once per frame: checks the session the first time, then applies replies.
    pub fn tick(&mut self, ctx: &egui::Context) -> Vec<Event> {
        if !self.started {
            self.started = true;
            self.check_session(ctx);
        }
        self.poll(ctx)
    }

    /// Applies replies; returns what the app has to act on. Cheap when nothing arrived.
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<Event> {
        let mut out = Vec::new();
        let replies: Vec<Reply> = self.rx.try_iter().collect();
        for r in replies {
            self.busy = self.busy.saturating_sub(1);
            match r {
                Reply::Auth(a) => {
                    self.checked = true;
                    self.need_code = a.two_factor;
                    if let Some(e) = a.error {
                        if self.user.take().is_some() || !self.token.is_empty() {
                            self.token.clear();
                            store_token(&self.token);
                        }
                        self.error = e;
                    } else {
                        self.user = a.user;
                        self.ent = a.entitlements.unwrap_or_default();
                        if let Some(t) = a.token {
                            self.token = t;
                            store_token(&self.token);
                        }
                        self.error.clear();
                        self.refresh(ctx);
                        self.fetch_layouts(ctx);
                    }
                }
                Reply::SignedOut | Reply::Expired => {
                    let expired = matches!(r, Reply::Expired) && self.user.is_some();
                    self.user = None;
                    self.projects.clear();
                    self.storage = None;
                    self.layouts_doc = None;
                    self.layouts_sent.clear();
                    self.token.clear();
                    store_token(&self.token);
                    if expired {
                        self.error = "You were signed out. Sign in again to use your account.".into();
                    }
                }
                Reply::Projects(p) => self.projects = p,
                Reply::Storage(used, max) => self.storage = Some((used, max)),
                Reply::Saved(id, name) => {
                    self.error.clear();
                    self.refresh(ctx);
                    out.push(Event::Saved(id, name));
                }
                Reply::Opened(id, name, json) => out.push(Event::Opened(id, name, json)),
                Reply::Deleted => self.refresh(ctx),
                Reply::LayoutsFound(id) => {
                    if let Some(id) = &id {
                        self.layouts_waiting = true;
                        self.request(ctx, "GET", &format!("/api/layouts/{id}"), None, |status, text| match doc_of(status, text) {
                            Ok(d) => Reply::LayoutsText(serde_json::from_str::<String>(&d.data).unwrap_or_default()),
                            Err(e) => Reply::Error(e),
                        });
                    }
                    self.layouts_doc = Some(id);
                }
                Reply::LayoutsText(text) => {
                    self.layouts_waiting = false;
                    self.layouts_sent = text.clone();
                    out.push(Event::Layouts(text));
                }
                Reply::LayoutsSaved(id) => self.layouts_doc = Some(Some(id)),
                Reply::Error(e) => {
                    self.checked = true;
                    self.error = e;
                }
            }
        }
        out
    }

    pub fn open_web(&self, ctx: &egui::Context, path: &str) {
        ctx.open_url(egui::OpenUrl::new_tab(format!("{}{}", self.base, path)));
    }
}

fn error_of(status: u16, v: &serde_json::Value) -> String {
    let e = v.get("error").and_then(serde_json::Value::as_str).unwrap_or_default();
    let mut c = e.chars();
    match c.next() {
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
        None => format!("Request failed ({status})"),
    }
}

fn doc_of(status: u16, text: &str) -> Result<Doc, String> {
    let d: Doc = serde_json::from_str(text).unwrap_or_default();
    if status >= 400 {
        return Err(error_of(status, &serde_json::json!({ "error": d.error })));
    }
    Ok(d)
}

#[cfg(not(target_arch = "wasm32"))]
fn token_path() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("account.token"))
}

fn load_token() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = token_path() {
        return std::fs::read_to_string(p).unwrap_or_default().trim().to_string();
    }
    String::new()
}

fn store_token(token: &str) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(p) = token_path() {
        if token.is_empty() {
            let _ = std::fs::remove_file(&p);
        } else if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&p, token);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
            }
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = token;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pump(a: &mut Account, ctx: &egui::Context, until: impl Fn(&Account) -> bool) -> Vec<Event> {
        let mut events = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while std::time::Instant::now() < deadline {
            events.extend(a.poll(ctx));
            if a.busy == 0 && until(a) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        events
    }

    /// Against a running phosphor-api (PHOSPHOR_CLOUD_URL, PHOSPHOR_TEST_EMAIL / _PASSWORD of a Pro account):
    /// sign in, save, list, open, delete, sign out. Run with PHOSPHOR_PROFILE=test so the token stays out of your settings.
    #[test]
    #[ignore]
    fn save_open_delete_round_trip() {
        let (Ok(email), Ok(password)) = (std::env::var("PHOSPHOR_TEST_EMAIL"), std::env::var("PHOSPHOR_TEST_PASSWORD")) else { return };
        let ctx = egui::Context::default();
        let mut a = Account { token: String::new(), ..Default::default() };
        a.sign_in(&ctx, &email, &password, "");
        pump(&mut a, &ctx, |a| a.user.is_some() || !a.error.is_empty());
        assert!(a.user.is_some(), "sign in: {}", a.error);
        let json = phosphor_engine::Composition::starter().to_json();
        a.save(&ctx, "", "Round trip", &json);
        let ev = pump(&mut a, &ctx, |a| a.projects.iter().any(|p| p.name == "Round trip"));
        let id = ev.iter().find_map(|e| if let Event::Saved(id, _) = e { Some(id.clone()) } else { None }).expect("saved");
        a.open(&ctx, &id);
        let ev = pump(&mut a, &ctx, |_| true);
        let back = ev.iter().find_map(|e| if let Event::Opened(_, _, j) = e { Some(j.clone()) } else { None }).expect("opened");
        assert_eq!(phosphor_engine::Composition::from_json(&back).unwrap(), phosphor_engine::Composition::from_json(&json).unwrap());
        a.delete(&ctx, &id);
        pump(&mut a, &ctx, |a| !a.projects.iter().any(|p| p.id == id));
        assert!(!a.projects.iter().any(|p| p.id == id), "deleted");
        a.sign_out(&ctx);
        pump(&mut a, &ctx, |a| a.user.is_none());
        assert!(a.user.is_none());
    }

    /// Layouts saved from one computer arrive on another after signing in (live server, as above).
    #[test]
    #[ignore]
    fn layouts_follow_the_account() {
        let (Ok(email), Ok(password)) = (std::env::var("PHOSPHOR_TEST_EMAIL"), std::env::var("PHOSPHOR_TEST_PASSWORD")) else { return };
        let ctx = egui::Context::default();
        let text = format!("(test: {})", web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).unwrap().as_millis());
        let mut first = Account { token: String::new(), ..Default::default() };
        first.sign_in(&ctx, &email, &password, "");
        pump(&mut first, &ctx, |a| a.layouts_doc.is_some() && !a.layouts_waiting);
        first.push_layouts(&ctx, &text);
        pump(&mut first, &ctx, |a| matches!(a.layouts_doc, Some(Some(_))));
        let mut second = Account { token: String::new(), ..Default::default() };
        second.sign_in(&ctx, &email, &password, "");
        let ev = pump(&mut second, &ctx, |a| a.layouts_doc.is_some() && !a.layouts_waiting);
        let got = ev.iter().find_map(|e| if let Event::Layouts(t) = e { Some(t.clone()) } else { None });
        assert_eq!(got.as_deref(), Some(text.as_str()));
        // unchanged text isn't uploaded again
        let busy = second.busy;
        second.push_layouts(&ctx, &text);
        assert_eq!(second.busy, busy);
    }
}
