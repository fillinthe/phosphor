//! The menu bar: one list of actions, shown as File / Edit / View / Output / Help inside the
//! window (every platform) and in the macOS system menu bar, with the same shortcuts.

use eframe::egui;
use egui::Key;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    NewProject,
    Open,
    Save,
    SaveAs,
    SaveToAccount,
    OpenFromAccount,
    Import,
    ImportResolume,
    FindMissing,
    WatchFolder,
    Snapshot,
    ExportShow,
    Quit,
    Undo,
    Redo,
    AddLayer,
    DuplicateLayer,
    DeleteLayer,
    Group,
    Ungroup,
    Palette,
    MapMode,
    ResetLayout,
    AddMonitor,
    Gather,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleOutput,
    Identify,
    Syphon,
    WebControl,
    MuteAll,
    Ndi,
    Notes,
    Roadmap,
    Website,
    /// View ▸ Panels (index into `Tab::ALL`).
    Panel(usize),
    /// View ▸ Layouts (index into the layout list).
    Layout(usize),
    /// One of the three areas (Show, Outputs, Control).
    Area(usize),
    SaveLayout,
    /// Lock / unlock the controls (player mode).
    Lock,
    Donate,
    ReportBug,
    CheckUpdates,
    /// Quick menu: one-click visuals.
    Quick(crate::app::Quick),
    /// Reveals the folder Phosphor scans for your own ISF shaders.
    ShaderFolder,
    LutFolder,
}

/// (shift, key) with ⌘ (Ctrl on Windows/Linux).
pub type Shortcut = Option<(bool, Key)>;

pub enum Entry {
    Item(Action, &'static str, Shortcut),
    Sep,
    /// Filled in at runtime (recent files, panels, layouts, screens).
    Dynamic(&'static str),
}

use Action as A;
use crate::app::Quick as Q;
use Entry::{Dynamic as D, Item as I, Sep as S};

pub const MENUS: &[(&str, &[Entry])] = &[
    (
        "File",
        &[
            I(A::NewProject, "New Project", Some((false, Key::N))),
            I(A::Open, "Open Project…", Some((false, Key::O))),
            D("Open Recent"),
            S,
            I(A::Save, "Save", Some((false, Key::S))),
            I(A::SaveAs, "Save As…", Some((true, Key::S))),
            I(A::SaveToAccount, "Save to Account", None),
            I(A::OpenFromAccount, "Open from Account…", None),
            S,
            I(A::Import, "Import Media…", Some((false, Key::I))),
            I(A::ImportResolume, "Import Resolume / VDMX Project…", None),
            I(A::FindMissing, "Find Missing Media…", None),
            I(A::WatchFolder, "Watch Folder…", None),
            S,
            I(A::Snapshot, "Save Snapshot (PNG)…", None),
            I(A::ExportShow, "Export Show…", Some((true, Key::E))),
            S,
            I(A::Quit, "Quit Phosphor", Some((false, Key::Q))),
        ],
    ),
    (
        "Edit",
        &[
            I(A::Undo, "Undo", Some((false, Key::Z))),
            I(A::Redo, "Redo", Some((true, Key::Z))),
            S,
            I(A::AddLayer, "Add Layer", Some((true, Key::N))),
            I(A::DuplicateLayer, "Duplicate Layer", Some((false, Key::D))),
            I(A::DeleteLayer, "Delete Layer", None),
            I(A::Group, "Group with the Layer Below", Some((false, Key::G))),
            I(A::Ungroup, "Ungroup", Some((true, Key::G))),
            S,
            I(A::Palette, "Command Palette…", Some((false, Key::K))),
            I(A::MapMode, "Map Mode (MIDI / OSC / Keys)", Some((true, Key::M))),
        ],
    ),
    (
        "Quick",
        &[
            I(A::Quick(Q::Surprise), Q::Surprise.label(), Some((false, Key::R))),
            I(A::Quick(Q::SurpriseLayer), Q::SurpriseLayer.label(), Some((true, Key::R))),
            I(A::Quick(Q::Shuffle), Q::Shuffle.label(), Some((false, Key::J))),
            I(A::Quick(Q::React), Q::React.label(), Some((true, Key::J))),
            S,
            I(A::Quick(Q::Fill), Q::Fill.label(), None),
            I(A::Quick(Q::InstantSet), Q::InstantSet.label(), None),
        ],
    ),
    (
        "View",
        &[
            I(A::Area(0), "Show", Some((false, Key::Num1))),
            I(A::Area(1), "Outputs & Inputs", Some((false, Key::Num2))),
            I(A::Area(2), "Control", Some((false, Key::Num3))),
            S,
            D("Panels"),
            D("Layouts"),
            I(A::SaveLayout, "Save Current Layout…", None),
            I(A::ResetLayout, "Reset Layout", None),
            I(A::AddMonitor, "New Preview Window", None),
            I(A::Gather, "Gather Floating Windows", None),
            S,
            I(A::Lock, "Lock Controls… / Unlock", Some((true, Key::L))),
            S,
            I(A::ZoomIn, "Zoom In", Some((false, Key::Equals))),
            I(A::ZoomOut, "Zoom Out", Some((false, Key::Minus))),
            I(A::ZoomReset, "Actual Size", Some((false, Key::Num0))),
        ],
    ),
    (
        "Output",
        &[
            I(A::ToggleOutput, "Show / Hide Output Windows", Some((false, Key::Enter))),
            D("Send Output To"),
            I(A::Identify, "Identify Outputs", None),
            S,
            I(A::Syphon, if cfg!(windows) { "Spout On / Off" } else { "Syphon On / Off" }, None),
            I(A::Ndi, "NDI On / Off", None),
            S,
            I(A::MuteAll, "Mute All Sound On / Off", None),
            I(A::WebControl, "Web Control On / Off (Take Over)", None),
        ],
    ),
    (
        "Help",
        &[
            I(A::Notes, "Notes — What's New", None),
            I(A::Roadmap, "Roadmap", None),
            I(A::Website, "phosphorfx.com", None),
            I(A::ShaderFolder, "Open ISF Shader Folder", None),
            I(A::LutFolder, "Open LUT Folder (colour grades)", None),
            I(A::Donate, "Donate to Phosphor…", None),
            S,
            I(A::ReportBug, "Report a Bug…", None),
            I(A::CheckUpdates, "Check for Updates…", None),
        ],
    ),
];

/// "⌘S" / "Shift ⌘S" (⇧ has no glyph in the UI font; or "Ctrl+S" off the Mac).
pub fn shortcut_text(s: Shortcut) -> String {
    let Some((shift, key)) = s else { return String::new() };
    let key = match key {
        Key::Equals => "+".to_string(),
        Key::Enter => "Return".to_string(),
        k => k.symbol_or_name().to_string(),
    };
    if cfg!(target_os = "macos") {
        format!("{}⌘{key}", if shift { "Shift " } else { "" })
    } else {
        format!("Ctrl+{}{key}", if shift { "Shift+" } else { "" })
    }
}

/// Shortcuts typed into the window (the macOS menu bar handles its own and swallows the keys).
pub fn pressed(ctx: &egui::Context) -> Vec<Action> {
    let typing = ctx.memory(|m| m.focused().is_some());
    let mut out = Vec::new();
    // Shift variants first, so ⇧⌘S isn't also read as ⌘S.
    for shift in [true, false] {
        for (_, entries) in MENUS {
            for e in entries.iter() {
                let I(action, _, Some((s, key))) = e else { continue };
                // Zoom keys are egui's own; text fields keep their own undo.
                if *s != shift || matches!(action, A::ZoomIn | A::ZoomOut | A::ZoomReset) || (typing && matches!(action, A::Undo | A::Redo)) {
                    continue;
                }
                let mods = if shift { egui::Modifiers::COMMAND | egui::Modifiers::SHIFT } else { egui::Modifiers::COMMAND };
                if ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(mods, *key))) {
                    out.push(*action);
                }
            }
        }
    }
    out
}

#[cfg(target_os = "macos")]
pub mod native {
    //! The same menus in the macOS menu bar.
    use muda::accelerator::{Accelerator, Code, Modifiers};
    use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

    use eframe::egui;

    use super::{Action, Entry, MENUS};

    fn code(key: egui::Key) -> Option<Code> {
        use eframe::egui::Key as K;
        Some(match key {
            K::N => Code::KeyN,
            K::O => Code::KeyO,
            K::S => Code::KeyS,
            K::I => Code::KeyI,
            K::E => Code::KeyE,
            K::Q => Code::KeyQ,
            K::Z => Code::KeyZ,
            K::D => Code::KeyD,
            K::G => Code::KeyG,
            K::K => Code::KeyK,
            K::R => Code::KeyR,
            K::J => Code::KeyJ,
            K::M => Code::KeyM,
            K::Equals => Code::Equal,
            K::Minus => Code::Minus,
            K::Num0 => Code::Digit0,
            K::Enter => Code::Enter,
            _ => return None,
        })
    }

    fn id(a: Action) -> String {
        format!("{a:?}")
    }

    /// Builds and installs the menu bar. Keep the returned menu alive.
    pub fn install(panels: &[String], layouts: &[&str]) -> Option<(Menu, Vec<Action>)> {
        let menu = Menu::new();
        let mut actions: Vec<Action> = Vec::new();
        let item = |a: Action, label: &str, sc: super::Shortcut, actions: &mut Vec<Action>| {
            actions.push(a);
            let accel = sc.and_then(|(shift, k)| {
                let mods = if shift { Modifiers::META | Modifiers::SHIFT } else { Modifiers::META };
                code(k).map(|c| Accelerator::new(mods, c))
            });
            MenuItem::with_id(id(a), label, true, accel)
        };

        let app = Submenu::new("Phosphor", true);
        let about = muda::AboutMetadata { name: Some("Phosphor".into()), version: Some(env!("CARGO_PKG_VERSION").into()), website: Some("https://phosphorfx.com".into()), ..Default::default() };
        app.append_items(&[
            &PredefinedMenuItem::about(Some("About Phosphor"), Some(about)),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            // Our own Quit (not the stock one) so the project is saved and the crash watchdog
            // knows it was on purpose.
            &item(Action::Quit, "Quit Phosphor", Some((false, egui::Key::Q)), &mut actions),
        ])
        .ok()?;
        menu.append(&app).ok()?;

        for (title, entries) in MENUS {
            let sub = Submenu::new(*title, true);
            for e in entries.iter() {
                match e {
                    Entry::Item(Action::Quit, ..) => {}
                    Entry::Item(a, label, sc) => {
                        sub.append(&item(*a, label, *sc, &mut actions)).ok()?;
                    }
                    Entry::Sep => {
                        sub.append(&PredefinedMenuItem::separator()).ok()?;
                    }
                    Entry::Dynamic("Panels") => {
                        let s = Submenu::new("Panels", true);
                        for (i, name) in panels.iter().enumerate() {
                            s.append(&item(Action::Panel(i), name, None, &mut actions)).ok()?;
                        }
                        sub.append(&s).ok()?;
                    }
                    Entry::Dynamic("Layouts") => {
                        let s = Submenu::new("Layouts", true);
                        for (i, name) in layouts.iter().enumerate() {
                            s.append(&item(Action::Layout(i), name, None, &mut actions)).ok()?;
                        }
                        sub.append(&s).ok()?;
                    }
                    // Recent files and screens change while running; they live in the in-window menu.
                    Entry::Dynamic(_) => {}
                }
            }
            if *title == "Help" {
                let window = Submenu::new("Window", true);
                window
                    .append_items(&[&PredefinedMenuItem::minimize(None), &PredefinedMenuItem::fullscreen(None)])
                    .ok()?;
                menu.append(&window).ok()?;
                window.set_as_windows_menu_for_nsapp();
                sub.set_as_help_menu_for_nsapp();
            }
            menu.append(&sub).ok()?;
        }
        menu.init_for_nsapp();
        Some((menu, actions))
    }

    /// Menu bar clicks since last frame.
    pub fn poll(actions: &[Action]) -> Vec<Action> {
        let mut out = Vec::new();
        while let Ok(e) = MenuEvent::receiver().try_recv() {
            if let Some(a) = actions.iter().find(|a| id(**a) == e.id().0) {
                out.push(*a);
            }
        }
        out
    }
}
