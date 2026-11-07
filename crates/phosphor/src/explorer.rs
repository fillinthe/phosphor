//! Effects Explorer (live previews of every effect/generator), the Shader Editor, and the
//! ⌘K command palette.

use crate::panels::RightMenu;
use eframe::egui::{self, Color32, RichText, Sense, Stroke, StrokeKind};
use egui_phosphor::regular as icon;
use phosphor_engine::patch::Patch;
use phosphor_engine::{CompileError, EffectState, MASTER_ID, ParamMod, PreviewItem, custom, shaders};

use crate::app::State;
use crate::panels::{self, Tab, label};
use crate::theme::{accent, WARN};

/// Drag payload from the explorer: drop on a layer card, MASTER or a clip cell to apply.
#[derive(Clone, Copy, Debug)]
pub struct PaletteDrag {
    pub effect: Option<&'static str>,
    pub generator: Option<&'static str>,
}

const CATEGORY_ORDER: &[&str] =
    &["Color", "Film", "Blur", "Light", "Distort", "Stylize", "Retro", "Time", "3D", "Key", "Tracking", "Patterns", "Motion", "Points", "Utility", "Yours", "ISF", "FFGL", "Other"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PreviewMode {
    /// Every visible tile animates.
    Live,
    /// The tile under the mouse animates; the rest keep one still each.
    #[default]
    Hover,
    /// No previews.
    Off,
}

#[derive(Default)]
pub struct ExplorerState {
    pub preview_mode: PreviewMode,
    /// Recently added effects (most recent first).
    pub recent: Vec<String>,
    pub search: String,
    /// "" = all, "★" = favourites, "hidden" = the hidden ones, otherwise a category name.
    pub category: String,
    /// Show generators (sources) instead of effects.
    pub sources: bool,
    pub favorites: Vec<String>,
    /// Effects and sources hidden from the lists (shown only under "Hidden").
    pub hidden: Vec<String>,
    /// The tile clicked last: chosen, not yet added to anything.
    pub picked: Option<&'static str>,
    /// How the picked one is set up (its tile shows it; added or dragged with these settings).
    pub picked_values: Vec<f32>,
    pub picked_mods: Vec<ParamMod>,
    pub picked_mix: f32,
}

impl ExplorerState {
    /// Picks a tile; a different one starts from its defaults.
    pub fn pick(&mut self, desc: &'static shaders::ShaderDesc) {
        if self.picked != Some(desc.id) {
            self.picked = Some(desc.id);
            self.picked_values = desc.defaults();
            self.picked_mods.clear();
            self.picked_mix = 1.0;
        }
    }

    /// The picked effect / source as it's set up, if `id` is the picked one.
    pub fn setup_for(&self, id: &str) -> Option<(Vec<f32>, Vec<ParamMod>, f32)> {
        (self.picked == Some(id)).then(|| (self.picked_values.clone(), self.picked_mods.clone(), self.picked_mix))
    }
}

/// The picked tile's settings: every control of the effect / source and (for an effect) its mix, before it goes on
/// anything.
fn picked_setup(ui: &mut egui::Ui, st: &mut State, desc: &'static shaders::ShaderDesc) {
    let ex = &mut st.explorer;
    if desc.params.is_empty() && ex.sources {
        return;
    }
    crate::ui::column_scroll(ui).id_salt("picked_setup").max_height(200.0).show(ui, |ui| {
        for (i, p) in desc.params.iter().enumerate() {
            crate::ui::param_slider(ui, &mut ex.picked_values, &mut ex.picked_mods, i, p, None);
        }
        if !ex.sources {
            crate::ui::Level::percent(&mut ex.picked_mix, "Mix").tip("How much of the effect: 0% = bypassed").show(ui);
        }
    });
    ui.add_space(2.0);
}

/// Help and Reset for the picked tile's settings, in the header row (a row each was mostly empty space).
fn picked_setup_header(ui: &mut egui::Ui, ex: &mut ExplorerState, desc: &'static shaders::ShaderDesc) {
    if desc.params.is_empty() && ex.sources {
        return;
    }
    let changed = ex.picked_values != desc.defaults() || ex.picked_mix != 1.0 || !ex.picked_mods.is_empty();
    if changed && crate::ui::small_command(ui, icon::ARROW_COUNTER_CLOCKWISE).on_hover_text("Reset the settings to the effect's defaults").clicked() {
        ex.picked_values = desc.defaults();
        ex.picked_mods.clear();
        ex.picked_mix = 1.0;
    }
    crate::ui::help(ui, "Set it up here: the tile shows it, and it's added or dragged with these settings. Mix 0% adds it switched off, to bring in later from the Inspector");
}

fn matches(desc: &shaders::ShaderDesc, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    let (cat, about) = shaders::info(desc.id);
    let hay = format!("{} {} {} {}", desc.name, desc.id, cat, about).to_lowercase();
    query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
}

/// Stable fake effect ids for previews, so feedback effects keep their history between frames.
fn preview_fx(desc: &shaders::ShaderDesc, index: usize) -> EffectState {
    EffectState { id: 9_000_000 + index as u64, effect: desc.id.to_string(), enabled: true, values: desc.defaults(), mix: 1.0, mods: Vec::new(), blend: Default::default(), other: 0, light: 0 }
}

/// What a tile shows: the picked one as it's set up, the rest with their defaults.
fn tile_item(ex: &ExplorerState, desc: &'static shaders::ShaderDesc, index: usize) -> PreviewItem {
    match ex.setup_for(desc.id) {
        // A source's preview takes its values the way a clip's does.
        Some((values, _, _)) if ex.sources => PreviewItem::Clip(desc.id.to_string(), values),
        Some((values, mods, mix)) => PreviewItem::Effect(EffectState { values, mods, mix, ..preview_fx(desc, index) }),
        None if ex.sources => PreviewItem::Generator(desc.id.to_string()),
        None => PreviewItem::Effect(preview_fx(desc, index)),
    }
}

/// How the tiles work, for the help when nothing is picked.
fn tiles_help(sources: bool) -> String {
    let verb = if sources { "use it" } else { "add it" };
    format!("Click to pick and set it up · double-click or drag onto a layer, MASTER or a clip to {verb} · right-click for more")
}

/// Where the picked tile's Add buttons put it.
#[derive(Clone, Copy)]
enum AddTo {
    /// A layer (or MASTER); None = a new layer (sources).
    Layer(Option<u64>),
    /// A clip in the grid: layer and slot.
    Clip(u64, usize),
}

enum TileAction {
    Apply(&'static str),
    Favorite(String),
    Hide(String),
    EditCopy(&'static str),
    EditCustom(String),
}

pub fn explorer(ui: &mut egui::Ui, st: &mut State) {
    let aspect = st.aspect();
    // Search on its own row (nothing can cover it), then the rest wraps in narrow columns.
    ui.horizontal(|ui| {
        ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(Color32::GRAY));
        let clear = !st.explorer.search.is_empty();
        let w = ui.available_width() - if clear { 28.0 } else { 0.0 };
        ui.add(egui::TextEdit::singleline(&mut st.explorer.search).hint_text("Search effects, looks, categories…").desired_width(w));
        if clear && crate::ui::small_command(ui, icon::X).clicked() {
            st.explorer.search.clear();
        }
    });
    let list: Vec<&'static shaders::ShaderDesc> = if st.explorer.sources { shaders::all_generators() } else { shaders::all_effects() };
    let hidden = list.iter().filter(|d| st.explorer.hidden.iter().any(|h| h == d.id)).count();
    if hidden == 0 && st.explorer.category == "hidden" {
        st.explorer.category.clear();
    }
    // Effects / Sources, then ONE category menu (the owner didn't want rows of category pills), then the preview mode.
    ui.horizontal_wrapped(|ui| {
        crate::ui::pick_value(ui, &mut st.explorer.sources, false, label(icon::MAGIC_WAND, "Effects"));
        crate::ui::pick_value(ui, &mut st.explorer.sources, true, label(icon::SPARKLE, "Sources"));
        ui.separator();
        let shown = match st.explorer.category.as_str() {
            "" => "All categories".to_string(),
            "★" => format!("{} Favourites", icon::STAR),
            "hidden" => format!("{} Hidden ({hidden})", icon::EYE_SLASH),
            c => c.to_string(),
        };
        #[cfg(not(target_arch = "wasm32"))]
        let packs = {
            let isf = format!("ISF: {} imported, {} skipped (from /Library/Graphics/ISF and ~/Library/Graphics/ISF; skipped ones need multi-pass or feedback support)", st.isf.loaded, st.isf.skipped);
            #[cfg(target_os = "macos")]
            let ffgl = format!("\n{}", ffgl_summary(&st.ffgl));
            #[cfg(not(target_os = "macos"))]
            let ffgl = String::new();
            format!("{isf}{ffgl}")
        };
        #[cfg(target_arch = "wasm32")]
        let packs = String::new();
        let menu = crate::ui::dropdown(ui, "explorer_category", 130.0).selected_text(shown).show_ui(ui, |ui| {
            crate::ui::pick_value(ui, &mut st.explorer.category, String::new(), "All categories");
            crate::ui::pick_value(ui, &mut st.explorer.category, "★".to_string(), format!("{} Favourites", icon::STAR));
            if hidden > 0 {
                crate::ui::pick_value(ui, &mut st.explorer.category, "hidden".to_string(), label(icon::EYE_SLASH, &format!("Hidden ({hidden})")));
            }
            ui.separator();
            // Only categories that exist in the current list.
            for cat in CATEGORY_ORDER {
                if list.iter().any(|d| shaders::info(d.id).0 == *cat) {
                    crate::ui::pick_value(ui, &mut st.explorer.category, cat.to_string(), *cat);
                }
            }
            #[cfg(target_os = "macos")]
            {
                ui.separator();
                if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Open the FFGL folder")).on_hover_text("Put FFGL plugins (.bundle) here").clicked()
                    && let Some(d) = crate::show::storage_dir().map(|d| d.join("ffgl"))
                {
                    let _ = std::fs::create_dir_all(&d);
                    crate::os::open(&d);
                    ui.close();
                }
            }
        });
        if !packs.is_empty() {
            menu.response.on_hover_text(packs);
        }
        ui.separator();
        ui.label(RichText::new("Previews").small().color(Color32::GRAY));
        crate::ui::pick_value(ui, &mut st.explorer.preview_mode, PreviewMode::Hover, "Hover").on_hover_text("Animate the tile under the mouse and the picked one; the others keep one still (lightest)");
        crate::ui::pick_value(ui, &mut st.explorer.preview_mode, PreviewMode::Live, "Live").on_hover_text("Animate every visible tile");
        crate::ui::pick_value(ui, &mut st.explorer.preview_mode, PreviewMode::Off, "Off");
        // With nothing picked, how the tiles work (an icon here; a sentence gets its own row below).
        if st.explorer.picked.is_none() && crate::ui::help_icons() {
            crate::ui::help(ui, &tiles_help(st.explorer.sources));
        }
    });
    // Bench pictures: PHOSPHOR_EXPLORER_PICK=<effect or source id> picks that tile on the first frame (a source id
    // switches to the Sources tiles first).
    if st.explorer.picked.is_none()
        && let Ok(id) = std::env::var("PHOSPHOR_EXPLORER_PICK")
    {
        match list.iter().find(|d| d.id == id) {
            Some(d) => {
                st.explorer.pick(d);
                // PHOSPHOR_EXPLORER_MIX=0.4: a mix that isn't the default, so a picture can show the setup carried.
                if let Some(m) = std::env::var("PHOSPHOR_EXPLORER_MIX").ok().and_then(|m| m.parse::<f32>().ok()) {
                    st.explorer.picked_mix = m;
                }
            }
            None if !st.explorer.sources && shaders::generator(&id).is_some() => st.explorer.sources = true,
            None => {}
        }
    }
    // The picked tile and what to do with it; nothing changes on screen until you choose. Each button says where
    // it goes: the selected layer, the clip picked in the grid, or MASTER.
    let picked = st.explorer.picked.and_then(|id| list.iter().find(|d| d.id == id).copied());
    let layer_target = st.selected.and_then(|id| st.comp.layer(id)).map(|l| (l.id, l.name.clone()));
    let clip_target = st.selected_clip.and_then(|(l, c)| st.comp.layer(l).and_then(|l| l.clips.get(c)).filter(|k| !k.is_empty()).map(|k| (l, c, k.name.clone())));
    let mut add_to: Option<AddTo> = None;
    ui.horizontal_wrapped(|ui| {
        match picked {
            Some(d) => {
                ui.label(RichText::new(d.name).strong().color(accent()));
                if st.explorer.sources {
                    let text = match &layer_target {
                        Some((_, name)) => format!("Use on layer: {}", crate::ui::short_name(name, 24)),
                        None => "Use on a new layer".to_string(),
                    };
                    if crate::ui::command(ui, label(icon::SPARKLE, &text)).clicked() {
                        add_to = Some(AddTo::Layer(layer_target.as_ref().map(|(id, _)| *id)));
                    }
                } else {
                    if let Some((id, name)) = &layer_target
                        && crate::ui::command(ui, label(icon::PLUS, &format!("Add to layer: {}", crate::ui::short_name(name, 24)))).on_hover_text("The selected layer: on its own effects, under the clip's").clicked()
                    {
                        add_to = Some(AddTo::Layer(Some(*id)));
                    }
                    if let Some((l, c, name)) = &clip_target
                        && crate::ui::command(ui, label(icon::PLUS, &format!("Add to clip: {}", crate::ui::short_name(name, 24)))).on_hover_text("The clip picked in the grid: the effect belongs to the clip and comes back whenever it plays").clicked()
                    {
                        add_to = Some(AddTo::Clip(*l, *c));
                    }
                    if crate::ui::command(ui, label(icon::PLUS, "Add to MASTER")).on_hover_text("Over the whole mix").clicked() {
                        add_to = Some(AddTo::Layer(Some(MASTER_ID)));
                    }
                    if layer_target.is_none() && clip_target.is_none() {
                        crate::ui::hint(ui, "Select a layer or click a clip to add it there, or drag the tile onto one");
                    }
                }
                picked_setup_header(ui, &mut st.explorer, d);
                if crate::ui::small_command(ui, icon::X).on_hover_text("Unselect").clicked() {
                    st.explorer.picked = None;
                }
            }
            None => {
                if !crate::ui::help_icons() {
                    crate::ui::help(ui, &tiles_help(st.explorer.sources));
                }
            }
        }
    });
    if let Some(d) = picked {
        picked_setup(ui, st, d);
    }
    // Shader packs: a row only while ISF is still importing or an FFGL plugin can't be used (otherwise the counts
    // are in the category menu's tooltip, so a panel in use isn't wearing a status line).
    #[cfg(not(target_arch = "wasm32"))]
    {
        #[cfg(target_os = "macos")]
        let ffgl_bad = st.ffgl.entries.iter().any(|e| e.error.is_some());
        #[cfg(not(target_os = "macos"))]
        let ffgl_bad = false;
        if st.isf.scanning() || ffgl_bad {
            crate::ui::toolbar(ui, |ui| {
                if st.isf.scanning() {
                    crate::ui::hint(ui, format!("ISF: importing… {}", st.isf.loaded));
                }
                #[cfg(target_os = "macos")]
                ffgl_status(ui, &st.ffgl);
            });
            ui.add_space(2.0);
        }
    }

    // Live previews refresh ~30 times a second, spread over frames (a few tiles per frame) so the GPU work is even
    // instead of every tile landing on the same frame.
    let groups = (ui.ctx().input(|i| 1.0 / (30.0 * i.stable_dt.max(1e-3))).round() as u64).clamp(1, 8);
    let frame = ui.ctx().cumulative_pass_nr();
    let mut shown = 0u64;
    let filtered: Vec<(usize, &'static shaders::ShaderDesc)> = list
        .iter()
        .enumerate()
        .filter(|(_, d)| matches(d, &st.explorer.search))
        .filter(|(_, d)| (st.explorer.category == "hidden") == st.explorer.hidden.iter().any(|h| h == d.id))
        .filter(|(_, d)| match st.explorer.category.as_str() {
            "" | "hidden" => true,
            "★" => st.explorer.favorites.iter().any(|f| f == d.id),
            cat => shaders::info(d.id).0 == cat,
        })
        .map(|(i, d)| (i, *d))
        .collect();

    let mut action: Option<TileAction> = None;
    crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
        if filtered.is_empty() {
            crate::ui::help(ui, "Nothing matches. Clear the search or pick another category.");
        }
        let tile_w = 152.0;
        let tile = egui::vec2(tile_w, tile_w / aspect + 30.0);
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 8.0);
            for (i, desc) in filtered.iter().take(96) {
                let key = format!("{}:{}", if st.explorer.sources { "gen" } else { "fx" }, desc.id);
                let (rect, resp) = ui.allocate_exact_size(tile, Sense::click_and_drag());
                if !ui.is_rect_visible(rect) {
                    continue;
                }
                // Only tiles on screen get a preview; how often it refreshes depends on the mode. The picked one
                // animates so its settings show as they're changed.
                if st.explorer.preview_mode != PreviewMode::Off {
                    let item = tile_item(&st.explorer, desc, *i);
                    let live = (st.explorer.preview_mode == PreviewMode::Live && (frame + shown).is_multiple_of(groups)) || resp.hovered() || st.explorer.picked == Some(desc.id);
                    shown += 1;
                    st.preview_wants.push((key.clone(), item, live));
                }
                let pic = egui::Rect::from_min_size(rect.min, egui::vec2(tile_w, tile_w / aspect));
                let p = ui.painter();
                p.rect_filled(rect, 6.0, ui.visuals().faint_bg_color);
                match st.preview_tex(&key).filter(|_| st.explorer.preview_mode != PreviewMode::Off) {
                    Some(t) => panels::paint_texture(p, t, pic),
                    None => {
                        p.rect_filled(pic, 4.0, crate::theme::PICTURE_BG);
                        let glyph = if st.explorer.preview_mode == PreviewMode::Off { icon::MAGIC_WAND } else { "…" };
                        p.text(pic.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(18.0), Color32::GRAY);
                    }
                }
                let (cat, about) = shaders::info(desc.id);
                p.text(egui::pos2(rect.left() + 6.0, pic.bottom() + 4.0), egui::Align2::LEFT_TOP, desc.name, egui::FontId::proportional(12.5), Color32::WHITE);
                p.text(egui::pos2(rect.left() + 6.0, rect.bottom() - 3.0), egui::Align2::LEFT_BOTTOM, cat, egui::FontId::proportional(10.0), Color32::GRAY);
                let fav = st.explorer.favorites.iter().any(|f| f == desc.id);
                if fav {
                    p.text(pic.right_top() + egui::vec2(-6.0, 4.0), egui::Align2::RIGHT_TOP, icon::STAR, crate::theme::filled_icons(14.0), crate::theme::FAVOURITE);
                }
                let stroke = if st.explorer.picked == Some(desc.id) {
                    Stroke::new(2.5, accent())
                } else if resp.hovered() {
                    Stroke::new(1.5, accent())
                } else {
                    Stroke::new(1.0, crate::theme::line())
                };
                p.rect_stroke(rect, 6.0, stroke, StrokeKind::Inside);
                let payload = if st.explorer.sources {
                    PaletteDrag { effect: None, generator: Some(desc.id) }
                } else {
                    PaletteDrag { effect: Some(desc.id), generator: None }
                };
                resp.dnd_set_drag_payload(payload);
                if resp.double_clicked() {
                    action = Some(TileAction::Apply(desc.id));
                } else if resp.clicked() {
                    st.explorer.pick(desc);
                }
                let params: Vec<&str> = desc.params.iter().map(|p| p.name).collect();
                let resp = resp.on_hover_text(format!("{about}\n\nControls: {}", if params.is_empty() { "none".into() } else { params.join(", ") }));
                resp.right_menu(|ui| {
                    let text = if fav { label(icon::STAR, "Remove from favourites") } else { label(icon::STAR, "Add to favourites") };
                    if crate::ui::command(ui, text).clicked() {
                        action = Some(TileAction::Favorite(desc.id.to_string()));
                        ui.close();
                    }
                    let is_hidden = st.explorer.hidden.iter().any(|h| h == desc.id);
                    let text = if is_hidden { label(icon::EYE, "Show in the lists again") } else { label(icon::EYE_SLASH, "Hide from the lists") };
                    if crate::ui::command(ui, text).on_hover_text("Hidden ones only show under \"Hidden\" (and still work wherever they're used)").clicked() {
                        action = Some(TileAction::Hide(desc.id.to_string()));
                        ui.close();
                    }
                    if desc.id.starts_with(shaders::CUSTOM_EFFECT) || desc.id.starts_with(shaders::CUSTOM_GENERATOR) || desc.id.starts_with(shaders::CUSTOM_BLEND) {
                        if crate::ui::command(ui, label(icon::CODE, "Edit in Shader Editor")).clicked() {
                            action = Some(TileAction::EditCustom(desc.id.to_string()));
                            ui.close();
                        }
                    } else if custom::builtin_as_custom(desc.id).is_some() {
                        if crate::ui::command(ui, label(icon::CODE, "Edit a copy in Shader Editor")).clicked() {
                            action = Some(TileAction::EditCopy(desc.id));
                            ui.close();
                        }
                    } else {
                        crate::ui::hint(ui, "Multi-pass / 3D effect: not editable yet");
                    }
                });
            }
        });
    });

    // Added from the buttons: the target is selected and the Inspector opened on it, so its Mix is right there.
    if let (Some(d), Some(to)) = (picked, add_to) {
        match to {
            AddTo::Layer(target) => {
                apply_palette(st, PaletteDrag { effect: (!st.explorer.sources).then_some(d.id), generator: st.explorer.sources.then_some(d.id) }, target);
                if let Some(t) = target {
                    st.selected = Some(t);
                }
            }
            AddTo::Clip(layer, slot) => drop_on_clip(ui.ctx(), st, PaletteDrag { effect: Some(d.id), generator: None }, layer, slot),
        }
        st.open_tab = Some(Tab::Inspector);
    }
    match action {
        Some(TileAction::Apply(id)) => apply_palette(st, PaletteDrag { effect: (!st.explorer.sources).then_some(id), generator: st.explorer.sources.then_some(id) }, None),
        Some(TileAction::Hide(id)) => {
            if let Some(i) = st.explorer.hidden.iter().position(|h| *h == id) {
                st.explorer.hidden.remove(i);
            } else {
                st.explorer.hidden.push(id);
            }
        }
        Some(TileAction::Favorite(id)) => {
            if let Some(i) = st.explorer.favorites.iter().position(|f| *f == id) {
                st.explorer.favorites.remove(i);
            } else {
                st.explorer.favorites.push(id);
            }
        }
        Some(TileAction::EditCopy(id)) => {
            if let Some(src) = custom::builtin_as_custom(id) {
                new_shader(st, src);
                st.open_tab = Some(Tab::ShaderEditor);
            }
        }
        Some(TileAction::EditCustom(id)) => {
            st.editor.current = st.editor.shaders.iter().position(|s| s.id == id);
            st.open_tab = Some(Tab::ShaderEditor);
        }
        None => {}
    }
}

/// Applies an explorer item to `target` (a layer id or MASTER), or to the selection.
/// "FFGL: N plugins": how many were found, which can't be used and why, and where they're looked for.
#[cfg(target_os = "macos")]
fn ffgl_summary(ffgl: &crate::ffgl::Ffgl) -> String {
    let bad = ffgl.entries.iter().filter(|e| e.error.is_some()).count();
    match (ffgl.entries.len(), bad) {
        (0, _) => "FFGL: no plugins".to_string(),
        (n, 0) => format!("FFGL: {n} plugins"),
        (n, b) => format!("FFGL: {} plugins, {b} can't be used", n - b),
    }
}

#[cfg(target_os = "macos")]
fn ffgl_status(ui: &mut egui::Ui, ffgl: &crate::ffgl::Ffgl) {
    let bad: Vec<&crate::ffgl::Entry> = ffgl.entries.iter().filter(|e| e.error.is_some()).collect();
    let r = crate::ui::hint(ui, ffgl_summary(ffgl)).on_hover_ui(|ui| {
        ui.label("FFGL effect and source plugins (.bundle), from:");
        for d in crate::ffgl::folders() {
            ui.monospace(d.to_string_lossy());
        }
        if let Some(d) = crate::show::storage_dir() {
            ui.label(format!("Put your own in {} (right-click: open it).", d.join("ffgl").to_string_lossy()));
        }
        for e in bad {
            ui.colored_label(WARN, format!("{}: {}", e.name, e.error.clone().unwrap_or_default()));
        }
    });
    r.context_menu(|ui| {
        if crate::ui::command(ui, label(icon::FOLDER_OPEN, "Open the FFGL folder")).clicked()
            && let Some(d) = crate::show::storage_dir().map(|d| d.join("ffgl"))
        {
            let _ = std::fs::create_dir_all(&d);
            crate::os::open(&d);
            ui.close();
        }
    });
}

pub fn apply_palette(st: &mut State, drag: PaletteDrag, target: Option<u64>) {
    let target = target.or(st.selected);
    // An effect goes onto anything with an effect chain: MASTER, a layer, a group or a comp (it then runs over
    // everything under it). A source only goes on a layer.
    let chain_target = target.filter(|id| *id == MASTER_ID || st.comp.effects(*id).is_some());
    let layer_target = target.filter(|id| st.comp.layers.iter().any(|l| l.id == *id));
    if let Some(fx) = drag.effect {
        // Nothing to put it on: say so (a new layer of its own would have no source, so nothing to see).
        match chain_target {
            Some(t) => panels::add_effect_to(st, t, fx),
            None => st.notice = Some("Select a layer, group, comp or MASTER first to put the effect on it (or drag the effect onto one)".into()),
        }
    }
    if let Some(g) = drag.generator {
        let t = match layer_target {
            Some(t) => t,
            None => st.add_layer(),
        };
        if let Some(desc) = shaders::generator(g) {
            let now = st.mod_inputs.time as f64;
            let values = st.explorer.setup_for(g).map_or_else(|| desc.defaults(), |(v, _, _)| v);
            st.launch_source(t, phosphor_engine::SourceSpec::Generator(g.to_string()), values, desc.name.to_string(), now);
        }
    }
}

/// A tile dropped on a clip cell (or Add to clip): an effect joins that clip's own chain, a source becomes the clip,
/// both with the picked tile's settings. The clip is selected and the Inspector opens on it, so its Mix is right there.
pub fn drop_on_clip(ctx: &egui::Context, st: &mut State, drag: PaletteDrag, layer: u64, slot: usize) {
    if let Some(fx) = drag.effect
        && let Some(desc) = shaders::effect(fx)
    {
        let (values, mods, mix) = st.explorer.setup_for(fx).unwrap_or_else(|| (desc.defaults(), Vec::new(), 1.0));
        match panels::add_effect_to_clip(st, layer, slot, fx, values, mods, mix) {
            Some(true) => st.notice = Some(format!("{} added to the clip: it's in the Inspector under Clip effects", desc.name)),
            Some(false) => st.notice = Some(format!("{} added to the clip: it comes in when the clip plays", desc.name)),
            None => {
                st.notice = Some("Drop an effect on a clip that has something in it".into());
                return;
            }
        }
    }
    if let Some(g) = drag.generator
        && let Some(desc) = shaders::generator(g)
    {
        st.comp.place_clips(layer, slot, vec![(desc.name.to_string(), phosphor_engine::SourceSpec::Generator(g.to_string()))]);
        if let Some((values, mods, _)) = st.explorer.setup_for(g)
            && let Some(l) = st.comp.layer_mut(layer)
        {
            let k = l.clip_slot_mut(slot);
            k.generator_values = values;
            k.mods = mods;
        }
        st.notice = Some(format!("{} is in the clip: click it to play", desc.name));
    }
    st.selected = Some(layer);
    st.selected_clip = Some((layer, slot));
    st.open_tab = Some(Tab::Inspector);
    if drag.effect.is_some() {
        panels::reveal_clip_effects(ctx);
    }
}

// -------------------------------------------------------------------------------------------
// Shader Editor

pub struct UserShader {
    /// `custom:<slug>` or `customgen:<slug>`.
    pub id: String,
    /// File stem in the user effects folder.
    pub slug: String,
    pub source: String,
    pub error: Option<CompileError>,
    pub compiled: bool,
    /// Made with + Patch: the blocks (the source is compiled from them; see `phosphor_engine::patch`).
    pub patch: Option<Patch>,
    /// A patch that doesn't compile yet (no shader is made until it does).
    pub patch_error: Option<String>,
}

impl UserShader {
    fn new(slug: String, source: String) -> UserShader {
        UserShader { id: id_for(&slug, &source), patch: Patch::from_source(&source), patch_error: None, slug, source, error: None, compiled: false }
    }
}

#[derive(Default)]
pub struct EditorState {
    pub shaders: Vec<UserShader>,
    pub current: Option<usize>,
    dirty_at: Option<f64>,
    loaded: bool,
    /// The Import from Shadertoy window, while open.
    shadertoy: Option<ShadertoyImport>,
}

/// Opens Import from Shadertoy (bench `--quick shadertoy`).
pub fn open_shadertoy(st: &mut State) {
    st.editor.shadertoy.get_or_insert_with(Default::default);
}

/// What's typed into Import from Shadertoy.
#[derive(Default)]
struct ShadertoyImport {
    name: String,
    credit: String,
    code: String,
    error: Option<String>,
}

/// The Import from Shadertoy window: paste the Image tab's code, name it, credit its author; it's saved as an ISF
/// shader in your ISF folder and shows up in Effects / Sources (ISF › Shadertoy) once the library has rescanned.
#[cfg(not(target_arch = "wasm32"))]
fn shadertoy_window(ctx: &egui::Context, st: &mut State) {
    let Some(imp) = st.editor.shadertoy.as_mut() else { return };
    let mut open = true;
    let mut done: Option<String> = None;
    crate::ui::dialog(ctx, label(icon::DOWNLOAD_SIMPLE, "Import from Shadertoy")).open(&mut open).default_width(560.0).show(ctx, |ui| {
        crate::ui::help(ui, "Open a shader on shadertoy.com, copy all the code in its Image tab and paste it here. Shaders that read iChannel0 become effects on a layer's picture; the rest are sources. Check its licence before you share a show with it.");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut imp.name).hint_text("e.g. Star Nest").desired_width(180.0));
            ui.label("By");
            ui.add(egui::TextEdit::singleline(&mut imp.credit).hint_text("its author").desired_width(160.0));
        });
        crate::ui::column_scroll(ui).max_height(360.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut imp.code).code_editor().desired_rows(16).desired_width(f32::INFINITY).hint_text("void mainImage( out vec4 fragColor, in vec2 fragCoord ) { … }"));
        });
        if let Some(e) = &imp.error {
            ui.colored_label(WARN, e);
        }
        let ready = !imp.name.trim().is_empty() && !imp.code.trim().is_empty();
        if crate::ui::command_if(ui, ready, label(icon::CHECK, "Import")).clicked() {
            let credit = if imp.credit.trim().is_empty() { "Shadertoy".to_string() } else { format!("{} (Shadertoy)", imp.credit.trim()) };
            let result = phosphor_isf::shadertoy::to_isf(&imp.code, &credit).and_then(|isf| {
                // Check it converts before saving (same converter the library uses).
                let slug = custom::slug(imp.name.trim());
                match phosphor_isf::import(&slug, &isf, None) {
                    Ok(sh) if sh.unsupported.is_none() => Ok(isf),
                    Ok(sh) => Err(format!("Can't use it yet: {}", sh.unsupported.unwrap_or_default())),
                    Err(e) => Err(format!("Doesn't convert: {e:?}")),
                }
            });
            match result {
                Ok(isf) => match crate::show::storage_dir().map(|d| d.join("isf")) {
                    Some(dir) => {
                        let file = dir.join(format!("{}.fs", imp.name.trim().replace(['/', '\\', ':'], "-")));
                        match std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, isf)) {
                            Ok(()) => done = Some(imp.name.trim().to_string()),
                            Err(e) => imp.error = Some(format!("Couldn't save it: {e}")),
                        }
                    }
                    None => imp.error = Some("No folder to save it in".into()),
                },
                Err(e) => imp.error = Some(e),
            }
        }
    });
    if let Some(name) = done {
        st.editor.shadertoy = None;
        st.isf = crate::isf_lib::IsfLibrary::scan();
        st.notice = Some(format!("Imported \"{name}\": it appears in Effects / Sources (ISF › Shadertoy) in a few seconds"));
    } else if !open {
        st.editor.shadertoy = None;
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn effects_dir() -> Option<std::path::PathBuf> {
    crate::show::storage_dir().map(|d| d.join("effects"))
}

fn id_for(slug: &str, source: &str) -> String {
    let header = custom::parse_header(source).ok();
    let prefix = match header {
        Some(h) if h.blend => shaders::CUSTOM_BLEND,
        Some(h) if h.generator => shaders::CUSTOM_GENERATOR,
        _ => shaders::CUSTOM_EFFECT,
    };
    format!("{prefix}{slug}")
}

/// Compiles every saved user shader once the engine exists.
pub fn load_user_shaders(st: &mut State) {
    if st.editor.loaded || st.engine.is_none() {
        return;
    }
    st.editor.loaded = true;
    #[cfg(not(target_arch = "wasm32"))]
    for dir in effects_dir().into_iter().chain(crate::show::shader_dir("effects")) {
        let Ok(entries) = std::fs::read_dir(dir) else { continue };
        let mut files: Vec<_> = entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "wgsl")).collect();
        files.sort();
        for path in files {
            let (Some(stem), Ok(source)) = (path.file_stem(), std::fs::read_to_string(&path)) else { continue };
            let slug = stem.to_string_lossy().to_string();
            st.editor.shaders.push(UserShader::new(slug, source));
            let i = st.editor.shaders.len() - 1;
            compile(st, i, false);
        }
    }
}

fn compile(st: &mut State, index: usize, save: bool) {
    let Some(engine) = st.engine.as_mut() else { return };
    let sh = &mut st.editor.shaders[index];
    let new_id = id_for(&sh.slug, &sh.source);
    if new_id != sh.id {
        shaders::unregister_custom(&sh.id);
        sh.id = new_id;
    }
    // Its parameters before, so whatever already uses it keeps its settings by name (`Composition::remap_params`).
    let old: Option<Vec<&'static str>> = shaders::effect(&sh.id).or_else(|| shaders::generator(&sh.id)).map(|d| d.params.iter().map(|p| p.name).collect());
    match engine.compile_custom(&sh.id, &sh.source) {
        Ok(_) => {
            sh.error = None;
            sh.compiled = true;
            if let (Some(old), Some(d)) = (old, shaders::effect(&sh.id).or_else(|| shaders::generator(&sh.id))) {
                let new: Vec<(&str, f32)> = d.params.iter().map(|p| (p.name, p.default)).collect();
                st.comp.remap_params(&sh.id.clone(), &old, &new);
            }
            let sh = &mut st.editor.shaders[index];
            #[cfg(not(target_arch = "wasm32"))]
            if save && let Some(dir) = effects_dir() {
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join(format!("{}.wgsl", sh.slug)), &sh.source);
            }
        }
        Err(e) => {
            sh.error = Some(e);
        }
    }
    #[cfg(target_arch = "wasm32")]
    let _ = save;
}

/// Bench `--quick templateN`: compiles template N (not saved to your effects folder) and puts it on the top
/// layer (a generator replaces its source; an effect is added). Returns its name.
pub fn bench_template(st: &mut State, n: usize) -> Option<String> {
    let t = custom::TEMPLATES.get(n)?;
    bench_shader(st, format!("bench_template_{n}"), t.source.to_string(), t.name)
}

/// Bench `--quick patch`: a patch (Input, Kaleidoscope, Glow, mixed with its own Last frame for trails) on the
/// top layer and open in the Shader Editor.
pub fn bench_patch(st: &mut State) -> Option<String> {
    let mut p = Patch::kaleido_glow_trails();
    p.name = "Bench Patch".into();
    let src = p.compile().ok()?;
    let name = bench_shader(st, "bench_patch".into(), src, "patch");
    st.editor.current = Some(st.editor.shaders.len() - 1);
    name
}

fn bench_shader(st: &mut State, slug: String, source: String, name: &str) -> Option<String> {
    st.editor.shaders.push(UserShader::new(slug, source));
    let i = st.editor.shaders.len() - 1;
    compile(st, i, false);
    let sh = &st.editor.shaders[i];
    if let Some(e) = &sh.error {
        return Some(format!("{name}: ERROR {e}"));
    }
    let id: &'static str = shaders::effect(&sh.id).or_else(|| shaders::generator(&sh.id)).map(|d| d.id)?;
    let layer = st.comp.layers.last().map(|l| l.id)?;
    apply_palette(st, PaletteDrag { effect: shaders::effect(id).map(|d| d.id), generator: shaders::generator(id).map(|d| d.id) }, Some(layer));
    Some(name.to_string())
}

/// Puts new source into shader `index` (Describe it… › Change the open shader), compiles and saves it, and opens it.
pub fn replace_source(st: &mut State, index: usize, source: String) {
    let Some(sh) = st.editor.shaders.get_mut(index) else { return };
    sh.source = source;
    st.editor.current = Some(index);
    compile(st, index, true);
}

pub fn new_shader(st: &mut State, source: String) {
    let name = custom::parse_header(&source).map(|h| h.name).unwrap_or_else(|_| "shader".into());
    let base = custom::slug(&name);
    let base = base.trim_matches('_').to_string();
    let mut slug = if base.is_empty() { "shader".to_string() } else { base };
    let mut n = 2;
    while st.editor.shaders.iter().any(|s| s.slug == slug) {
        slug = format!("{}_{n}", slug.trim_end_matches(|c: char| c.is_ascii_digit() || c == '_'));
        n += 1;
    }
    st.editor.shaders.push(UserShader::new(slug, source));
    let i = st.editor.shaders.len() - 1;
    st.editor.current = Some(i);
    compile(st, i, true);
}

pub fn shader_editor(ui: &mut egui::Ui, st: &mut State) {
    let now = ui.ctx().input(|i| i.time);
    #[cfg(not(target_arch = "wasm32"))]
    shadertoy_window(&ui.ctx().clone(), st);
    let mut delete: Option<usize> = None;
    let mut create: Option<String> = None;
    egui::Panel::left("shader_list").resizable(true).default_size(190.0).show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            if crate::ui::command(ui, label(icon::PLUS, "Effect")).on_hover_text("New effect (processes a layer)").clicked() {
                create = Some(custom::template(false));
            }
            if crate::ui::command(ui, label(icon::PLUS, "Generator")).on_hover_text("New generator (makes its own picture)").clicked() {
                create = Some(custom::template(true));
            }
            ui.menu_button(label(icon::GRAPH, "Patch"), |ui| {
                for (text, generator) in [("Effect patch", false), ("Generator patch", true)] {
                    if crate::ui::command(ui, text).clicked() {
                        create = Patch::new(if generator { "My Patch Generator" } else { "My Patch" }, generator).compile().ok();
                        ui.close();
                    }
                }
                ui.separator();
                crate::ui::hint(ui, "Examples: open one and change any block");
                for (about, example) in Patch::examples() {
                    if crate::ui::command(ui, example.name.clone()).on_hover_text(about).clicked() {
                        create = example.compile().ok();
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Build an effect from blocks wired together (blur, warp, colour, mix, trails): no code");
        });
        #[cfg(not(target_arch = "wasm32"))]
        if crate::ui::command(ui, label(icon::MAGIC_WAND, "Describe it…")).on_hover_text("Say what it should look like and the AI writes the shader (your own Anthropic API key)").clicked() {
            st.ai_shader.open = true;
        }
        #[cfg(not(target_arch = "wasm32"))]
        if crate::ui::command(ui, label(icon::DOWNLOAD_SIMPLE, "Import from Shadertoy…")).on_hover_text("Paste a shader from shadertoy.com").clicked() {
            st.editor.shadertoy.get_or_insert_with(Default::default);
        }
        ui.menu_button(label(icon::SPARKLE, "New from template…"), |ui| {
            ui.set_min_width(260.0);
            for t in custom::TEMPLATES {
                if crate::ui::command(ui, t.name).on_hover_text(t.about).clicked() {
                    create = Some(t.source.to_string());
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Starters that each do one thing (colour, warp, pixelate, mirror, mask, glow, rings, clouds), every line explained");
        ui.menu_button(label(icon::COPY, "Copy a built-in…"), |ui| {
            crate::ui::column_scroll(ui).max_height(360.0).show(ui, |ui| {
                for d in shaders::EFFECTS.iter().chain(shaders::GENERATORS) {
                    if let Some(src) = custom::builtin_as_custom(d.id)
                        && crate::ui::command(ui, d.name).clicked()
                    {
                        create = Some(src);
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
            if st.editor.shaders.is_empty() {
                crate::ui::hint(ui, "No shaders yet.");
            }
            for (i, sh) in st.editor.shaders.iter().enumerate() {
                let name = custom::parse_header(&sh.source).map(|h| h.name).unwrap_or_else(|_| sh.slug.clone());
                let dot = if sh.error.is_some() { RichText::new(icon::RECORD).color(WARN) } else { RichText::new(icon::RECORD).color(accent()) };
                crate::ui::toolbar(ui, |ui| {
                    ui.label(dot.small());
                    if crate::ui::pick_item(ui, st.editor.current == Some(i), name).clicked() {
                        st.editor.current = Some(i);
                    }
                });
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = effects_dir() {
            ui.separator();
            if crate::ui::small_command(ui, label(icon::FOLDER_OPEN, "Show folder")).clicked() {
                let _ = std::fs::create_dir_all(&dir);
                crate::os::open(&dir);
            }
        }
    });
    if let Some(src) = create {
        new_shader(st, src);
    }

    let Some(idx) = st.editor.current.filter(|i| *i < st.editor.shaders.len()) else {
        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.label(RichText::new("Shader Editor").strong().size(16.0));
            crate::ui::help(ui, "Write your own effects and generators in WGSL. They compile as you type and show up in the Effects explorer.");
            crate::ui::help(ui, &format!("Start from a template (New from template…), with + Effect, or right-click any effect in the explorer {} Edit a copy.", icon::ARROW_RIGHT));
        });
        return;
    };

    let is_generator = custom::parse_header(&st.editor.shaders[idx].source).map(|h| h.generator).unwrap_or(false);
    let id = st.editor.shaders[idx].id.clone();
    let compiled = st.editor.shaders[idx].compiled;
    // Live preview of this shader on the selected layer (or as a generator).
    if compiled {
        let item = if is_generator {
            PreviewItem::Generator(id.clone())
        } else if let Some(d) = shaders::effect(&id) {
            let mut fx = preview_fx(d, 50_000 + idx);
            // A patch's preview follows its sliders at once (the shader is rebuilt a moment later).
            if let Some(p) = &st.editor.shaders[idx].patch {
                fx.values = p.param_slots().iter().map(|(n, i)| p.node(*n).and_then(|n| n.values.get(*i)).copied().unwrap_or(0.0)).collect();
            }
            PreviewItem::Effect(fx)
        } else {
            PreviewItem::Generator(String::new())
        };
        st.preview_wants.push((format!("edit:{id}"), item, true));
    }
    let aspect = st.aspect();
    egui::Panel::right("shader_preview").resizable(true).default_size(260.0).show(ui, |ui| {
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(w, w / aspect), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, crate::theme::PICTURE_BG);
        if let Some(t) = st.preview_tex(&format!("edit:{id}")) {
            panels::paint_texture(ui.painter(), t, rect);
        }
        crate::ui::hint(ui, if is_generator { "Live preview" } else { "Live preview on the selected layer" });
        ui.add_space(4.0);
        let ok = compiled && st.editor.shaders[idx].error.is_none();
        ui.add_enabled_ui(ok, |ui| {
            if is_generator {
                if crate::ui::command(ui, label(icon::SPARKLE, "Use as source of selected layer")).clicked() {
                    let g: &'static str = shaders::generator(&id).map_or("", |d| d.id);
                    apply_palette(st, PaletteDrag { effect: None, generator: Some(g) }, None);
                }
            } else {
                if crate::ui::command(ui, label(icon::PLUS, "Add to selected layer")).clicked() {
                    let e: &'static str = shaders::effect(&id).map_or("", |d| d.id);
                    apply_palette(st, PaletteDrag { effect: Some(e), generator: None }, None);
                }
                if crate::ui::command(ui, label(icon::PLUS, "Add to MASTER")).clicked() {
                    let e: &'static str = shaders::effect(&id).map_or("", |d| d.id);
                    apply_palette(st, PaletteDrag { effect: Some(e), generator: None }, Some(MASTER_ID));
                }
            }
        });
        ui.add_space(8.0);
        ui.collapsing("Cheat sheet", |ui| {
            ui.monospace("src(uv) -> vec4f     input picture");
            ui.monospace("P(0u), P(1u)…         your @params");
            ui.monospace("u.time  u.resolution");
            ui.monospace("in.uv (0..1, y down)");
            ui.monospace("luma rgb2hsv hsv2rgb");
            ui.monospace("palette hash21 centered");
            ui.monospace("aspect()  TAU");
        });
        ui.add_space(8.0);
        if crate::ui::command(ui, label(icon::TRASH, "Delete shader")).clicked() {
            delete = Some(idx);
        }
    });

    // The space left between the list and the preview (so nothing draws under either).
    let sh = &mut st.editor.shaders[idx];
    let dirty_at = &mut st.editor.dirty_at;
    egui::CentralPanel::no_frame().show(ui, |ui| {
        if sh.patch.is_some() {
            patch_view(ui, sh, dirty_at, now);
        } else {
            code_view(ui, sh, dirty_at, now, is_generator);
        }
    });
    if st.editor.dirty_at.is_some_and(|t| now - t > 0.35) {
        st.editor.dirty_at = None;
        compile(st, idx, true);
    }
    if let Some(i) = delete {
        let sh = st.editor.shaders.remove(i);
        shaders::unregister_custom(&sh.id);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = effects_dir() {
            let _ = std::fs::remove_file(dir.join(format!("{}.wgsl", sh.slug)));
        }
        st.editor.current = None;
    }
}

/// A shader made with + Patch: its name, the block canvas, and the code it makes (read-only).
fn patch_view(ui: &mut egui::Ui, sh: &mut UserShader, dirty_at: &mut Option<f64>, now: f64) {
    let Some(patch) = sh.patch.as_mut() else { return };
    let code_id = egui::Id::new(("patch_code", &sh.slug));
    let mut show_code: bool = ui.ctx().data(|d| d.get_temp(code_id)).unwrap_or(false);
    let mut changed = false;
    let mut to_code = false;
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, if patch.generator { "Generator patch" } else { "Effect patch" });
        changed |= crate::ui::text_field(ui, &mut patch.name, "Name", Some(160.0)).changed();
        if let Some(name) = patch.look_at.and_then(|id| patch.node(id)).and_then(|n| phosphor_engine::patch::block(&n.block)).map(|b| b.name) {
            if crate::ui::small_command(ui, label(icon::EYE, &format!("Showing {name}"))).on_hover_text("The picture shows this block, not the Output: click to show the Output again").clicked() {
                patch.set_look_at(None);
                changed = true;
            }
        }
        crate::ui::toggle(ui, &mut show_code, label(icon::CODE, "Code")).on_hover_text("The shader these blocks make");
        if crate::ui::command(ui, label(icon::CODE, "Turn into code")).on_hover_text("Keep editing it as code from here on (the blocks are dropped)").clicked() {
            to_code = true;
        }
        match (&sh.patch_error, &sh.error) {
            (Some(e), _) => ui.colored_label(WARN, format!("{} {e}", icon::RECORD)),
            (None, Some(e)) => ui.colored_label(WARN, format!("{} {e}", icon::RECORD)),
            (None, None) if sh.compiled => ui.colored_label(accent(), format!("{} compiled · saved", icon::RECORD)),
            _ => crate::ui::hint(ui, "not compiled"),
        };
        crate::ui::help(ui, "Right-click the canvas (or drop a wire on it) to add a block: type to find one · drag from a block's right dot to another's left dot to wire it, or onto the dot beside a setting to drive it with a Numbers block · right-click a wire to put a block on it · drag a band to select several; Delete, Cmd+D, Cmd+C / V, arrows, Cmd+Z · right-click a block: Switch off, Look at this box, Reset settings · every slider becomes a parameter of the effect (MIDI, modulation); here they set its starting values, a layer already using it keeps its own (Inspector)");
    });
    ui.ctx().data_mut(|d| d.insert_temp(code_id, show_code));
    if show_code {
        egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut sh.source.as_str()).code_editor().font(egui::FontId::monospace(12.0)).desired_width(f32::INFINITY));
        });
    } else {
        changed |= panels::patch_canvas(ui, patch, &sh.slug);
    }
    if to_code {
        sh.source = sh.source.lines().filter(|l| !l.starts_with("// @patch ") && !l.starts_with("// Made in the patch editor")).collect::<Vec<_>>().join("\n") + "\n";
        sh.patch = None;
        *dirty_at = Some(now);
        return;
    }
    if changed {
        match patch.compile() {
            Ok(src) => {
                sh.source = src;
                sh.patch_error = None;
                *dirty_at = Some(now);
            }
            Err(e) => sh.patch_error = Some(e),
        }
    }
}

/// A shader written as code: the text editor with line numbers and its compile state.
fn code_view(ui: &mut egui::Ui, sh: &mut UserShader, dirty_at: &mut Option<f64>, now: f64, is_generator: bool) {
    crate::ui::toolbar(ui, |ui| {
        crate::ui::heading(ui, if is_generator { "Generator" } else { "Effect" });
        ui.monospace(&sh.slug);
        match &sh.error {
            Some(_) => ui.colored_label(WARN, format!("{} error", icon::RECORD)),
            None if sh.compiled => ui.colored_label(accent(), format!("{} compiled · saved", icon::RECORD)),
            None => crate::ui::hint(ui, "not compiled"),
        };
    });
    if let Some(e) = &sh.error {
        egui::Frame::new().fill(crate::theme::PROBLEM_FILL).corner_radius(4).inner_margin(6).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.colored_label(WARN, e.to_string());
        });
    }
    let err_line = sh.error.as_ref().and_then(|e| e.line);
    egui::ScrollArea::both().auto_shrink(false).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Line-number gutter, with the error line highlighted.
            let lines = sh.source.lines().count().max(1);
            let mut gutter = egui::text::LayoutJob::default();
            for n in 1..=lines {
                let color = if Some(n as u32) == err_line { WARN } else { crate::theme::ON_PICTURE.gamma_multiply(0.45) };
                gutter.append(&format!("{n:>3}\n"), 0.0, egui::TextFormat { font_id: egui::FontId::monospace(13.0), color, ..Default::default() });
            }
            ui.add(egui::Label::new(gutter).selectable(false));
            let edit = egui::TextEdit::multiline(&mut sh.source)
                .code_editor()
                .font(egui::FontId::monospace(13.0))
                .desired_width(f32::INFINITY)
                .desired_rows(28)
                .lock_focus(true);
            if ui.add(edit).changed() {
                *dirty_at = Some(now);
            }
        });
    });
}

// -------------------------------------------------------------------------------------------
// Command palette

#[derive(Default)]
pub struct PaletteState {
    pub open: bool,
    pub query: String,
    pub selected: usize,
    focus: bool,
}

impl PaletteState {
    pub fn toggle(&mut self) {
        self.open = !self.open;
        self.query.clear();
        self.selected = 0;
        self.focus = true;
    }
}

#[derive(Clone, Debug)]
pub enum Command {
    AddEffect(&'static str),
    SetGenerator(&'static str),
    OpenTab(Tab),
    TriggerColumn(usize),
    ToggleOutput,
    ResetLayout,
    NewLayer,
    TapTempo,
    NewShader(bool),
    ToggleMapping,
    ToggleSession,
    AddMonitor,
    Quick(crate::app::Quick),
}

fn commands(st: &State) -> Vec<(String, Command)> {
    let mut out = vec![
        (format!("{} New layer", icon::PLUS), Command::NewLayer),
        (format!("{} Output window", icon::MONITOR), Command::ToggleOutput),
        (format!("{} Tap tempo", icon::METRONOME), Command::TapTempo),
        (format!("{} Mapping on/off", icon::PROJECTOR_SCREEN), Command::ToggleMapping),
        (format!("{} Session on/off (browser viewers)", icon::BROADCAST), Command::ToggleSession),
        (format!("{} Reset layout", icon::LAYOUT), Command::ResetLayout),
        (format!("{} Add monitor (preview panel)", icon::MONITOR), Command::AddMonitor),
        (format!("{} New effect shader", icon::CODE), Command::NewShader(false)),
        (format!("{} New generator shader", icon::CODE), Command::NewShader(true)),
    ];
    for q in crate::app::Quick::ALL {
        out.push((format!("{} {}", q.icon(), q.label()), Command::Quick(q)));
    }
    for tab in Tab::ALL {
        out.push((format!("{} Show panel: {}", icon::APP_WINDOW, tab.title()), Command::OpenTab(tab)));
    }
    for c in 0..st.comp.clip_columns {
        out.push((format!("{} Trigger column {}", icon::PLAY, c + 1), Command::TriggerColumn(c)));
    }
    for d in shaders::all_effects() {
        out.push((format!("{} Add effect: {}  · {}", icon::MAGIC_WAND, d.name, shaders::info(d.id).0), Command::AddEffect(d.id)));
    }
    for d in shaders::all_generators() {
        out.push((format!("{} Source: {}", icon::SPARKLE, d.name), Command::SetGenerator(d.id)));
    }
    out
}

/// Subsequence match with a bonus for contiguous runs; `None` if not all characters appear.
fn score(text: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let text = text.to_lowercase();
    let mut score = 0;
    let mut last: Option<usize> = None;
    let mut from = 0;
    for qc in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let pos = text[from..].find(qc)? + from;
        score += match last {
            Some(l) if pos == l + qc.len_utf8() => 5,
            _ => 1,
        };
        last = Some(pos);
        from = pos + qc.len_utf8();
    }
    if text.contains(&query.to_lowercase()) {
        score += 20;
    }
    Some(score)
}

pub fn palette(ctx: &egui::Context, st: &mut State) -> Option<Command> {
    if !st.palette.open {
        return None;
    }
    let mut ranked: Vec<(i32, String, Command)> =
        commands(st).into_iter().filter_map(|(t, c)| score(&t, &st.palette.query).map(|s| (s, t, c))).collect();
    ranked.sort_by(|a, b| b.0.cmp(&a.0));
    ranked.truncate(12);
    let (down, up, enter, esc) = ctx.input(|i| {
        (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape))
    });
    if down {
        st.palette.selected = (st.palette.selected + 1).min(ranked.len().saturating_sub(1));
    }
    if up {
        st.palette.selected = st.palette.selected.saturating_sub(1);
    }
    let mut chosen: Option<Command> = None;
    if esc {
        st.palette.open = false;
        return None;
    }
    let screen = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("palette_dim"))).rect_filled(screen, 0.0, Color32::from_black_alpha(120));
    egui::Area::new(egui::Id::new("command_palette"))
        .order(egui::Order::Tooltip)
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 90.0))
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).inner_margin(10).show(ui, |ui| {
                ui.set_width(520.0);
                let edit = ui.add(egui::TextEdit::singleline(&mut st.palette.query).hint_text("Type a command, effect, panel…").desired_width(f32::INFINITY));
                if st.palette.focus {
                    edit.request_focus();
                    st.palette.focus = false;
                }
                if edit.changed() {
                    st.palette.selected = 0;
                }
                ui.separator();
                for (i, (_, text, cmd)) in ranked.iter().enumerate() {
                    let sel = i == st.palette.selected;
                    if crate::ui::pick_item(ui, sel, text).clicked() {
                        chosen = Some(cmd.clone());
                    }
                }
                if ranked.is_empty() {
                    crate::ui::hint(ui, "No matches");
                }
                ui.add_space(2.0);
                crate::ui::hint(ui, format!("{}{} to move · Enter to run · Esc to close", icon::ARROW_UP, icon::ARROW_DOWN));
            });
        });
    if enter && let Some((_, _, cmd)) = ranked.get(st.palette.selected) {
        chosen = Some(cmd.clone());
    }
    if chosen.is_some() {
        st.palette.open = false;
    }
    chosen
}

// -------------------------------------------------------------------------------------------
// "+ Add effect" / "Browse sources" picker

pub struct FxBrowser {
    /// Layer id or MASTER_ID.
    pub target: u64,
    /// Pick a generator (layer source) instead of an effect.
    pub sources: bool,
    pub pos: Option<egui::Pos2>,
    search: String,
    category: String,
    focus: bool,
}

impl FxBrowser {
    pub fn new(target: u64, sources: bool, pos: Option<egui::Pos2>) -> Self {
        Self { target, sources, pos, search: String::new(), category: String::new(), focus: true }
    }
}

/// The pop-up effect/source browser. Click adds and closes, Shift-click adds and stays open,
/// Enter adds the first match, Esc closes.
pub fn fx_browser_window(ctx: &egui::Context, st: &mut State) {
    let Some(mut b) = st.fx_browser.take() else { return };
    let aspect = st.aspect();
    let target_name = if b.target == MASTER_ID {
        "MASTER".to_string()
    } else {
        st.comp.layers.iter().find(|l| l.id == b.target).map_or("layer".into(), |l| l.name.clone())
    };
    let title = if b.sources { format!("Source for {target_name}") } else { format!("Add effect to {target_name}") };
    let list: Vec<&'static shaders::ShaderDesc> = if b.sources { shaders::all_generators() } else { shaders::all_effects() };
    let mut open = true;
    let mut picked: Option<(&'static str, bool)> = None;
    let mut window = egui::Window::new(title)
        .id(egui::Id::new("fx_browser"))
        .open(&mut open)
        .collapsible(false)
        .resizable(true)
        .default_size([600.0, 480.0]);
    if let Some(p) = b.pos.take() {
        window = window.default_pos(p - egui::vec2(300.0, 0.0)).current_pos(p - egui::vec2(300.0, 0.0));
    }
    window.show(ctx, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(icon::MAGNIFYING_GLASS).color(Color32::GRAY));
            let edit = ui.add(egui::TextEdit::singleline(&mut b.search).hint_text("Type to search…  Enter adds the first match").desired_width(f32::INFINITY));
            if b.focus {
                edit.request_focus();
                b.focus = false;
            }
        });
        ui.horizontal_wrapped(|ui| {
            crate::ui::pick_value(ui, &mut b.category, String::new(), "All");
            if !b.sources {
                crate::ui::pick_value(ui, &mut b.category, "recent".to_string(), format!("{} Recent", icon::CLOCK_COUNTER_CLOCKWISE));
            }
            crate::ui::pick_value(ui, &mut b.category, "★".to_string(), format!("{} Favourites", icon::STAR));
            for cat in CATEGORY_ORDER {
                if list.iter().any(|d| shaders::info(d.id).0 == *cat) {
                    crate::ui::pick_value(ui, &mut b.category, cat.to_string(), *cat);
                }
            }
        });
        ui.separator();
        let mut shown: Vec<&'static shaders::ShaderDesc> = list
            .iter()
            .copied()
            .filter(|d| matches(d, &b.search))
            .filter(|d| !st.explorer.hidden.iter().any(|h| h == d.id))
            .filter(|d| match b.category.as_str() {
                "" => true,
                "★" => st.explorer.favorites.iter().any(|f| f == d.id),
                "recent" => st.explorer.recent.iter().any(|r| r == d.id),
                cat => shaders::info(d.id).0 == cat,
            })
            .collect();
        if b.category == "recent" {
            shown.sort_by_key(|d| st.explorer.recent.iter().position(|r| r == d.id).unwrap_or(usize::MAX));
        }
        if ui.input(|i| i.key_pressed(egui::Key::Enter)) && let Some(d) = shown.first() {
            picked = Some((d.id, false));
        }
        crate::ui::column_scroll(ui).auto_shrink(false).show(ui, |ui| {
            if shown.is_empty() {
                crate::ui::hint(ui, "Nothing matches.");
            }
            let tile_w = 128.0;
            let tile = egui::vec2(tile_w, tile_w / aspect + 20.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
                for (i, d) in shown.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(tile, Sense::click());
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let key = format!("pick:{}", d.id);
                    if st.explorer.preview_mode != PreviewMode::Off {
                        let item = if b.sources { PreviewItem::Generator(d.id.to_string()) } else { PreviewItem::Effect(preview_fx(d, 20_000 + i)) };
                        let live = st.explorer.preview_mode == PreviewMode::Live || resp.hovered();
                        st.preview_wants.push((key.clone(), item, live));
                    }
                    let pic = egui::Rect::from_min_size(rect.min, egui::vec2(tile_w, tile_w / aspect));
                    let p = ui.painter();
                    p.rect_filled(rect, 5.0, ui.visuals().faint_bg_color);
                    match st.preview_tex(&key) {
                        Some(t) => panels::paint_texture(p, t, pic),
                        None => {
                            p.rect_filled(pic, 4.0, crate::theme::PICTURE_BG);
                        }
                    }
                    p.text(egui::pos2(rect.left() + 5.0, rect.bottom() - 3.0), egui::Align2::LEFT_BOTTOM, d.name, egui::FontId::proportional(11.5), Color32::WHITE);
                    let first = i == 0 && !b.search.is_empty();
                    let stroke = if resp.hovered() || first { Stroke::new(1.5, accent()) } else { Stroke::new(1.0, crate::theme::line()) };
                    p.rect_stroke(rect, 5.0, stroke, StrokeKind::Inside);
                    if resp.clicked() {
                        picked = Some((d.id, ui.input(|i| i.modifiers.shift)));
                    }
                    let (cat, about) = shaders::info(d.id);
                    resp.on_hover_text(format!("{}  ·  {cat}\n{about}", d.name));
                }
            });
        });
        crate::ui::help(ui, "Click adds and closes · Shift-click adds more · Esc closes");
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        open = false;
    }
    if let Some((id, keep)) = picked {
        if b.sources {
            apply_palette(st, PaletteDrag { effect: None, generator: Some(id) }, Some(b.target));
        } else {
            apply_palette(st, PaletteDrag { effect: Some(id), generator: None }, Some(b.target));
        }
        if !keep {
            open = false;
        }
    }
    if open {
        st.fx_browser = Some(b);
    }
}
