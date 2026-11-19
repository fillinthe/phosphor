//! Packed shows and shows sent between Phosphors, on screen: File › Save Packed Show…, opening a `.phosphorpack`
//! (where to unpack it; then its project opens), the Session panel's "Send this show", and the question when another
//! Phosphor offers one. The work runs on threads of its own (`crate::pack`, `crate::show_send`, `crate::job`); this
//! draws it and finishes it.

use std::path::{Path, PathBuf};

use crate::cache_store::size_text;
use crate::job::{self, Shared};
use crate::show_send::{Offer, Target};

use super::*;

/// What's going on (in `State::packs`).
#[derive(Default)]
pub struct Packs {
    saving: Option<Shared>,
    pub opening: Option<Opening>,
    /// Who to, and the job.
    sending: Option<(String, Shared)>,
    receiving: Option<(Offer, Shared)>,
    /// The sender's session password, typed when its offer is answered.
    password: String,
    /// An accepted offer whose download was refused for its password: asked again.
    retry: Option<Offer>,
}

/// A packed show chosen to open: where it unpacks (beside it unless another folder is picked), then the unpacking.
pub struct Opening {
    pub file: PathBuf,
    pub into: PathBuf,
    size: u64,
    job: Option<Shared>,
}

impl Opening {
    /// Unpack into a folder of its own inside `folder` instead.
    pub fn choose(&mut self, folder: &Path) {
        if self.job.is_none() {
            let stem = self.file.file_stem().map_or_else(|| "Show".into(), |s| s.to_string_lossy().into_owned());
            self.into = crate::pack::free_path(folder, &stem, "");
        }
    }
}

/// The show's name: its project file's, else "Show".
fn show_name(st: &State) -> String {
    st.project_path.as_deref().and_then(|p| Path::new(p).file_stem()).map_or_else(|| "Show".into(), |s| s.to_string_lossy().into_owned())
}

/// File › Save Packed Show…: asks where.
pub(crate) fn ask_save_pack(st: &mut State) {
    let name = format!("{}.{}", crate::pack::file_name(&show_name(st)), crate::pack::EXT);
    st.dialogs.save_file(crate::dialogs::Purpose::SavePackedShow, ("Packed show", &[crate::pack::EXT]), &name);
}

/// Packs the show into `dest` (on its own thread; the window shows how far).
pub(crate) fn save_pack(st: &mut State, dest: PathBuf) {
    if st.packs.saving.as_ref().is_some_and(|p| !p.lock().unwrap().done) {
        st.notice = Some("A packed show is being saved already".into());
        return;
    }
    let (comp, name) = (st.comp.clone(), show_name(st));
    st.packs.saving = Some(job::spawn("phosphor-pack", move |p| crate::pack::write(comp, &name, &dest, p)));
}

/// A packed show (or a Morpholio Trace drawing) to open: asks where to unpack it first (beside it, or a folder you
/// pick).
pub(crate) fn open_pack(st: &mut State, file: PathBuf) {
    if st.packs.opening.as_ref().is_some_and(|o| o.job.is_some()) {
        st.notice = Some("A show is being opened already".into());
        return;
    }
    let size = std::fs::metadata(&file).map(|m| m.len()).unwrap_or(0);
    // Beside it, unless that's somewhere that can't be written (a read-only disk): then Documents › Phosphor Shows.
    let beside = file.parent().is_some_and(|d| !std::fs::metadata(d).is_ok_and(|m| m.permissions().readonly()));
    let into = if beside { crate::pack::folder_beside(&file) } else { crate::pack::free_path(&crate::pack::shows_dir(), &crate::pack::file_name(&file.file_stem().unwrap_or_default().to_string_lossy()), "") };
    st.packs.opening = Some(Opening { file, into, size, job: None });
}

/// Session › Send this show: packs it and offers it to `to` (this Session starts if it's off: the other Phosphor
/// takes the show from it).
pub(crate) fn send_show(st: &mut State, to: Target) {
    if !st.net.running()
        && let Err(e) = st.net.start()
    {
        st.notice = Some(format!("Couldn't start the Session to send from: {e}"));
        return;
    }
    let who = to.name.clone();
    let send = crate::show_send::Send {
        comp: st.comp.clone(),
        name: show_name(st),
        to,
        port: st.net.port,
        locked: st.net.has_password(),
        me: crate::control::discovery::identity().0,
        hub: st.net.shows.clone(),
    };
    st.packs.sending = Some((who, crate::show_send::send(send)));
}

/// How a job is doing: its step, a bar (bytes done of how many) or a spinner. True = Cancel was pressed.
fn job_row(ui: &mut egui::Ui, p: &job::Progress) -> bool {
    ui.label(&p.step);
    match p.fraction() {
        Some(f) => {
            ui.add(crate::ui::meter(f).desired_width(360.0).text(format!("{} of {}", size_text(p.bytes), size_text(p.total))));
        }
        None => {
            ui.spinner();
        }
    }
    crate::ui::command(ui, "Cancel").clicked()
}

/// How a finished job ended: what it made (with a Finder button), or why it didn't. True = closed.
fn job_end(ui: &mut egui::Ui, p: &job::Progress, done: &str) -> bool {
    let canceled = p.error.as_deref() == Some(job::CANCELED);
    if let Some(out) = p.output.as_ref().filter(|_| !canceled) {
        ui.label(egui::RichText::new(format!("{} {done}: {}", icon::CHECK_CIRCLE, out.file_name().unwrap_or_default().to_string_lossy())).strong().color(accent()));
    }
    if let Some(e) = p.error.as_ref().filter(|_| !canceled) {
        ui.colored_label(WARN, e);
    }
    let mut close = canceled;
    crate::ui::toolbar(ui, |ui| {
        if let Some(out) = &p.output
            && crate::ui::command(ui, label(icon::FOLDER_OPEN, if cfg!(windows) { "Show in Explorer" } else { "Show in Finder" })).clicked()
        {
            crate::os::reveal(out);
        }
        close |= crate::ui::command(ui, "Close").clicked();
    });
    close
}

/// Every window this needs, and the jobs it finishes (a project opens when its unpacking is done). Call every frame.
pub(crate) fn packs_window(ctx: &egui::Context, st: &mut State) {
    // Bench pictures: PHOSPHOR_PACK_OPEN=<file> asks where to unpack it; PHOSPHOR_SHOW_OFFER=1 (with --net) shows
    // the question an offered show asks.
    static PICTURES: std::sync::Once = std::sync::Once::new();
    PICTURES.call_once(|| {
        if let Some(file) = std::env::var_os("PHOSPHOR_PACK_OPEN") {
            open_pack(st, PathBuf::from(file));
        }
        if std::env::var_os("PHOSPHOR_SHOW_OFFER").is_some() {
            let body = serde_json::json!({ "from": "Phosphor on Stage Mac", "name": "Club Night", "bytes": 2_300_000_000u64, "port": 8777, "token": crate::net::new_token(), "locked": true });
            let _ = st.net.shows.inbox.lock().unwrap().offered("192.168.1.20", body.to_string().as_bytes(), std::time::Instant::now());
        }
    });
    let busy = [st.packs.saving.as_ref(), st.packs.opening.as_ref().and_then(|o| o.job.as_ref()), st.packs.sending.as_ref().map(|s| &s.1), st.packs.receiving.as_ref().map(|r| &r.1)]
        .into_iter()
        .flatten()
        .any(|p| !p.lock().unwrap().done);
    if busy {
        ctx.request_repaint_after(std::time::Duration::from_millis(150));
    }
    saving_window(ctx, st);
    opening_window(ctx, st);
    sending_window(ctx, st);
    receiving_window(ctx, st);
    offer_prompt(ctx, st);
}

fn saving_window(ctx: &egui::Context, st: &mut State) {
    let Some(p) = st.packs.saving.as_ref().map(|p| p.lock().unwrap().clone()) else { return };
    let mut close = false;
    crate::ui::dialog(ctx, format!("{} Save Packed Show", icon::PACKAGE)).id(egui::Id::new("pack_saving")).resizable(false).default_width(420.0).show(ctx, |ui| {
        if !p.done {
            if job_row(ui, &p)
                && let Some(p) = &st.packs.saving
            {
                job::cancel(p);
            }
            crate::ui::hint(ui, "The show keeps running while this works.");
        } else {
            if p.output.is_some() {
                crate::ui::help(ui, "Copy this one file to the other computer and open it there with File › Open (or drop it on Phosphor): it unpacks into a folder and the show opens with its media.");
            }
            close = job_end(ui, &p, "Saved");
        }
    });
    if close {
        st.packs.saving = None;
    }
}

fn opening_window(ctx: &egui::Context, st: &mut State) {
    let Some(o) = st.packs.opening.as_ref() else { return };
    let progress = o.job.as_ref().map(|p| p.lock().unwrap().clone());
    // Done: its project opens, and any shaders it brought are loaded.
    if let Some(p) = progress.as_ref().filter(|p| p.done && p.error.is_none()) {
        if let Some(project) = p.output.clone() {
            finish_open(st, &project, None);
        }
        st.packs.opening = None;
        return;
    }
    let (file, into, size) = (o.file.clone(), o.into.clone(), o.size);
    let trace = crate::import::morpholio::is_trace(&file);
    let (mut start, mut close, mut change) = (false, false, false);
    let title = if trace { format!("{} Open Morpholio Trace Drawing", icon::PENCIL_SIMPLE) } else { format!("{} Open Packed Show", icon::PACKAGE) };
    crate::ui::dialog(ctx, title).id(egui::Id::new("pack_opening")).resizable(false).default_width(440.0).show(ctx, |ui| {
        ui.label(egui::RichText::new(file.file_name().unwrap_or_default().to_string_lossy()).strong());
        match &progress {
            None => {
                crate::ui::hint(ui, size_text(size));
                crate::ui::field(ui, if trace { "Make it in" } else { "Unpack into" }, |ui| {
                    ui.monospace(into.to_string_lossy());
                    crate::ui::help(
                        ui,
                        if trace {
                            "A new folder with a picture of each drawing layer (placed as in Trace) and a project with a layer for each: names, hidden layers and opacity kept. It opens in place of this one."
                        } else {
                            "A new folder with the project, its media and shaders. The project in it opens, in place of this one."
                        },
                    );
                });
                crate::ui::toolbar(ui, |ui| {
                    start = crate::ui::action_button(ui, crate::ui::Act::Go, label(icon::FOLDER_OPEN, if trace { "Make and open" } else { "Unpack and open" })).clicked();
                    change = crate::ui::command(ui, "Another folder…").clicked();
                    close = crate::ui::command(ui, "Cancel").clicked();
                });
            }
            Some(p) if !p.done => {
                if job_row(ui, p)
                    && let Some(j) = st.packs.opening.as_ref().and_then(|o| o.job.as_ref())
                {
                    job::cancel(j);
                }
            }
            Some(p) => close = job_end(ui, p, if trace { "Made" } else { "Unpacked" }),
        }
    });
    if change {
        st.dialogs.pick_folder(crate::dialogs::Purpose::UnpackInto);
    }
    if close {
        st.packs.opening = None;
    } else if start && let Some(o) = st.packs.opening.as_mut() {
        let (file, into) = (o.file.clone(), o.into.clone());
        o.job = Some(if crate::import::morpholio::is_trace(&file) {
            job::spawn("phosphor-trace", move |p| crate::import::morpholio::convert(&file, &into, p).map(|_| ()))
        } else {
            job::spawn("phosphor-unpack", move |p| crate::pack::unpack(&file, &into, p).map(|_| ()))
        });
    }
}

/// Opens a project that was just unpacked (and loads the user shaders it brought).
fn finish_open(st: &mut State, project: &Path, from: Option<&str>) {
    if st.open_project_file(&project.to_string_lossy()) {
        crate::explorer::load_new_user_shaders(st);
        let folder = project.parent().unwrap_or(project).display();
        st.notice = Some(match from {
            Some(who) => format!("Opened the show from {who} (it's in {folder})"),
            None => format!("Opened it (the files are in {folder})"),
        });
    }
}

fn sending_window(ctx: &egui::Context, st: &mut State) {
    let Some((who, p)) = st.packs.sending.as_ref().map(|(w, p)| (w.clone(), p.lock().unwrap().clone())) else { return };
    let mut close = false;
    crate::ui::dialog(ctx, format!("{} Send This Show", icon::PAPER_PLANE_TILT)).id(egui::Id::new("pack_sending")).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.label(format!("To {who}"));
        if !p.done {
            if job_row(ui, &p)
                && let Some((_, j)) = &st.packs.sending
            {
                job::cancel(j);
            }
        } else {
            if p.error.is_none() {
                ui.label(egui::RichText::new(format!("{} {}", icon::CHECK_CIRCLE, p.step)).strong().color(accent()));
            }
            // (The pack was a temporary file: nothing to show in the Finder.)
            let ended = job::Progress { output: None, ..p.clone() };
            close = job_end(ui, &ended, "Sent");
        }
    });
    if close {
        st.packs.sending = None;
    }
}

fn receiving_window(ctx: &egui::Context, st: &mut State) {
    let Some((offer, p)) = st.packs.receiving.as_ref().map(|(o, p)| (o.clone(), p.lock().unwrap().clone())) else { return };
    if p.done && p.error.is_none() {
        if let Some(project) = p.output.clone() {
            finish_open(st, &project, Some(&offer.from));
        }
        st.packs.receiving = None;
        return;
    }
    // Refused for its password: asked again (the offer stands).
    if p.done && p.error.as_deref() == Some(crate::show_send::WRONG_PASSWORD) {
        st.packs.receiving = None;
        st.packs.retry = Some(offer);
        return;
    }
    let mut close = false;
    crate::ui::dialog(ctx, format!("{} Incoming Show", icon::DOWNLOAD_SIMPLE)).id(egui::Id::new("pack_receiving")).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.label(format!("\"{}\" from {}", offer.name, offer.from));
        if !p.done {
            if job_row(ui, &p)
                && let Some((_, j)) = &st.packs.receiving
            {
                job::cancel(j);
            }
        } else {
            close = job_end(ui, &p, "Received");
        }
    });
    if close {
        st.packs.receiving = None;
    }
}

/// Another Phosphor offers a show: the question, never answered by itself.
fn offer_prompt(ctx: &egui::Context, st: &mut State) {
    if st.packs.receiving.is_some() || !st.net.running() {
        return;
    }
    let retry = st.packs.retry.clone();
    let Some(offer) = retry.clone().or_else(|| st.net.shows.inbox.lock().unwrap().waiting()) else { return };
    // (Offers arrive while nothing else moves the screen: look again now and then.)
    ctx.request_repaint_after(std::time::Duration::from_millis(500));
    let (mut accept, mut decline) = (false, false);
    crate::ui::dialog(ctx, format!("{} Incoming Show", icon::DOWNLOAD_SIMPLE)).id(egui::Id::new(("pack_offer", offer.id))).resizable(false).default_width(420.0).show(ctx, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{} wants to send you a show: \"{}\" ({}). Accept?", offer.from, offer.name, size_text(offer.bytes))).strong());
            crate::ui::help(ui, &format!("It goes into Documents › Phosphor Shows and opens when it arrives, in place of this project (save yours first). From {}.", offer.ip));
        });
        if offer.locked {
            if retry.is_some() {
                ui.colored_label(WARN, "That password wasn't right.");
            }
            crate::ui::field(ui, "Its password", |ui| {
                crate::ui::password_field(ui, &mut st.packs.password, "the sender's Session password", 200.0);
            });
        }
        crate::ui::toolbar(ui, |ui| {
            accept = crate::ui::action_button(ui, crate::ui::Act::Go, label(icon::DOWNLOAD_SIMPLE, "Accept")).clicked();
            decline = crate::ui::command(ui, if retry.is_some() { "Cancel" } else { "Decline" }).clicked();
        });
    });
    let inbox = st.net.shows.inbox.clone();
    if decline {
        st.packs.retry = None;
        st.packs.password.clear();
        if retry.is_some() {
            inbox.lock().unwrap().stopped(offer.id);
        } else {
            inbox.lock().unwrap().answer(offer.id, false);
        }
    } else if accept {
        let offer = if retry.is_some() { Some(offer) } else { inbox.lock().unwrap().answer(offer.id, true) };
        if let Some(offer) = offer {
            st.packs.retry = None;
            let password = std::mem::take(&mut st.packs.password);
            let job = crate::show_send::receive(offer.clone(), password, crate::pack::shows_dir(), inbox);
            st.packs.receiving = Some((offer, job));
        }
    }
}

/// Session panel: other Phosphors with their Session on, each with "Send this show".
pub(crate) fn send_section(ui: &mut egui::Ui, st: &mut State) {
    crate::ui::heading_help(ui, "Send this show", "Hands this show (project, media and shaders, packed into one file) to another Phosphor on your network. It's asked there first: nothing arrives without a yes. Its Session must be on; this one starts if it's off.");
    let busy = st.packs.sending.as_ref().is_some_and(|(_, p)| !p.lock().unwrap().done);
    let players: Vec<_> = crate::control::discovery::players().into_iter().filter(|p| p.session.is_some()).collect();
    if players.is_empty() {
        crate::ui::hint(ui, "No other Phosphor with its Session on was heard on this network.");
    }
    let mut send = None;
    for p in &players {
        crate::ui::toolbar(ui, |ui| {
            ui.label(&p.name);
            crate::ui::hint(ui, format!("{}:{}", p.address, p.session.unwrap_or_default()));
            if crate::ui::command_if(ui, !busy, label(icon::PAPER_PLANE_TILT, "Send this show")).clicked() {
                send = Some(Target { name: p.name.clone(), address: p.address.clone(), port: p.session.unwrap_or_default() });
            }
        });
    }
    if busy {
        crate::ui::hint(ui, "Sending: see its window.");
    }
    if let Some(to) = send {
        send_show(st, to);
    }
    ui.separator();
}
