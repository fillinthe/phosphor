# Phosphor design system

The rules every screen, panel and control follows. They are enforced where a test can enforce them
(`crates/phosphor/src/ui/design_rules.rs`); the rest is the reviewer's checklist. Change a rule here first, then
the code, never the other way round.

## 1. One direction: "Phosphor CRT", dark technical

A VJ tool is used in the dark, next to a projector, by someone looking at the output, not at the app. So the
interface is a **blue-black control surface with one lit colour**: the look of a phosphor monitor at rest, where
only the thing that matters glows. Everything else is flat, quiet and square.

- **Dominant:** near-black charcoal with a hint of blue (never pure grey, never pure black except the darkest step).
- **Accent:** one phosphor green, used only for *on / playing / selected / go*. Nothing decorative is green.
- **Kind hues:** six muted colours say what a thing *is* (a movie, a generator, a camera, text, an output, a
  controller), as a tab colour or a stripe. They never mean "on".
- **Warnings** are orange, **record / delete** red, **mapped** gold. Those four meanings own those colours.
- **Shape:** sliders are square bars, controls have 5-point corners, pills only for on/off toggles, nothing casts a
  shadow except a floating window or menu.
- **Atmosphere** comes from depth, not decoration: three tones of charcoal (panel, window, well), one accent, the
  picture itself. No patterns, no gradients, no textures, no illustration.

Not this: anything that reads as a website. No hero sections, no card grids for the sake of it, no gradients,
no gradient text, no emoji, no purple-to-indigo anything, no "modern clean" grey-on-white.

## 2. Type

| Role | Font | Where it lives | Use |
|---|---|---|---|
| Wordmark | **Orbitron Bold** (OFL) | `assets/fonts/logo.ttf`, `theme::logo` | Only "PHOSPHOR VJ". Never for anything else. |
| Body / buttons / headings | **Ubuntu Light** (egui's bundled proportional font) | egui default, sized in `theme::apply` | Everything you read. |
| Code / numbers that line up | **Hack** (egui's bundled monospace) | `TextStyle::Monospace` | Shader code, OSC addresses, timecode, stats. |
| Icons | **Phosphor Icons** (regular; fill for "solid") | `egui_phosphor`, `theme::filled_icons` | Every icon. Never a Unicode glyph (`theme::ui_text_has_glyphs` test). |
| CJK fallback | Hiragino (the Mac's own), loaded only while a CJK language is on | `theme::set_fonts(cjk)` | Japanese / Chinese text. |

Sizes (points, `theme::apply`): Small 9 · Body 11.5 · Button 11.5 · Heading 14 · Monospace 10.5. Headings are
`ui::heading` (strong Body) or a `section` title; there is no larger display size in panels.

**Never:** Inter, Roboto, Open Sans, Arial, Helvetica, SF / system-ui, Segoe, Noto Sans as a body font. Any new
font must be OFL or similarly free, bundled in `assets/fonts` with its licence file, loaded in `theme::set_fonts`,
and have a role in this table first.

**Case:** sentence case everywhere ("Add effect", "Stream key"). All caps only for the wordmark and single-word
state badges of eight letters or fewer (MASTER, GRID, ON, LASERS ON). No all-caps eyebrows or section labels.

## 3. Colour tokens

All colours come from `theme` (or egui's `Visuals`, which `theme::set_brightness` fills). Panels never write a
`Color32::from_rgb` of their own: the design test ratchets the count of raw colours outside `theme.rs` and the kit
down, never up.

Values at the default brightness (0.35, "Dark"); `tone(g)` = `(g-3, g-1, g)`, the blue hint.

| Token | Value | Meaning |
|---|---|---|
| `panel_fill` | tone(21) `#121415` | A panel's background. |
| `window_fill` | tone(27) `#181A1B` | Windows, menus, boxes on a canvas. |
| `extreme_bg_color` | tone(13) `#0A0C0D` | Wells: text fields, the canvas, the clip grid's empty cells, behind sliders. |
| `faint_bg_color` | tone(31) `#1C1E1F` | A row that's hovered, a stripe. |
| `widgets.inactive.bg_fill` | tone(45) `#2A2C2D` | A button at rest. |
| `widgets.hovered.bg_fill` | tone(60) `#393B3C` | A button under the pointer (plus a 60 % accent outline). |
| `widgets.active.bg_fill` | accent × 0.35 | A button pressed, a segment picked. |
| `selection` | accent × 0.32 fill, accent stroke | Selected rows, cards, boxes. |
| **accent** (`theme::accent`) | Green `#78FFAA` (dark) / `#108A4A` (light) | On, playing, selected, Go. The user may pick Cyan, Violet, Pink or Ice instead; code never assumes green. |
| `theme::WARN` | `#FF8C5A` | Warnings, errors, Delete text. |
| `Act::Record` fill | `#E1463C` | Record, Arm. White ink. |
| `ui::learn::MAPPED` | `#FFC446` | Mapped to a controller (badge, stripe). |
| `Hue::Media` | `#6996FF` / `#2850BE` | Movies, pictures, the media bin. |
| `Hue::Effect` | `#B48CFF` / `#6941BE` | Generators, effects, shaders. |
| `Hue::Live` | `#46CDBE` / `#0A786E` | Cameras, screens, streams, NDI / Syphon in. |
| `Hue::Text` | `#E1D2AA` / `#786432` | Text layers. |
| `Hue::Output` | `#F57DAA` / `#AA285F` | Outputs, mapping, LEDs, streaming. |
| `Hue::Control` | `#5FCDFF` / `#006EA0` | MIDI, OSC, modulators, the control surface, number wires. |
| Ink | egui `text_color` / `strong_text_color` / `weak_text_color` | Words. `hint` = weak. Never a hand-picked grey for text. |

Accent on a fill: `theme::tint(base, amount)`. Dimmer accent: `accent().gamma_multiply(k)`. Never a second
green, never a lighter copy of a hue by hand.

## 4. Space, shape, depth

| Token | Value | Where |
|---|---|---|
| `ui::CONTROL_H` | 22 | Every control's height: buttons, toggles, fields, sliders, list rows. One row = 22 + 4 spacing. |
| `item_spacing` | 6 × 4 | Between controls in a row, between rows. |
| `button_padding` | 7 × 3 (small commands 4 × 3) | Inside buttons. |
| `ui::LABEL_W` | 96 | The label column of a `field` row, so settings line up down a panel. |
| `menu_margin` | 6 | Inside menus. |
| Node canvas | `NODE_W` 190 · `TITLE_H` 22 · `INPUT_H` 18 · setting row 22 · column pitch `NODE_W` + 70 | Boxes and their layout (`graph::tidy`). |
| Corner radius | controls 5 · menus 6 · windows 8 · action buttons 6 · segmented frame 6, segment 4 · list rows 3 · toggles full pill · **sliders and meters 0** | The owner's choice: square bars. |
| Outlines | 1 point, same width in every state (a resting outline is transparent) so nothing moves on hover | `theme::set_brightness` |
| Shadows | egui's window and popup shadow only | Nothing in a panel casts a shadow. |
| Expansion on hover / press | 0 | Buttons stay put; the state shows in colour and outline. |

No other numbers. A new size is a named constant in `ui/kit.rs` with a comment saying what it's for.

## 5. Motion

- Only state changes animate, and only through egui's own transitions (about 100 ms): hover colour, a collapsing
  section opening, a menu appearing. Nothing moves on its own to decorate.
- Live things that *should* move (a meter, a modulation position on a slider, a scope) move because the data does.
- No staggered reveals, no bouncing, no spinners except `ui.spinner()` beside words that say what's loading.
- Pictures from the show are never slowed or faded by the interface.

## 6. Components (the kit, `crates/phosphor/src/ui/kit.rs`)

Every control is one of these. A panel never hand-rolls a button, slider, field, menu or list.

| Job | Component | Variants / sizes | States |
|---|---|---|---|
| Makes something happen now | `action_button(Act::Go)` (accent, dark ink) · `Act::Record` (red, white ink) | `action_button_sized`, `steady_action_button` (width held while the label changes) | hover: white outline · disabled: `command_if` style grey · mapped: gold badge |
| A command (opens, edits, adds) | `command` · `small_command` (tight row) · `command_sized` · `wide_button` · `callout_button` (Donate, an update) | `*_if(enabled)` greys it out until it can be used | hover: lifted fill + accent outline · pressed: accent fill · disabled: grey |
| Delete / remove | `danger_button` (red words) | — | confirm only when a lot is lost |
| On / off | `toggle` (pill with a light) · `toggle_mapped` · `icon_toggle` (compact) · `eye_toggle` (show / hide) | `toggle_locked` | on: accent light and outline · off: hollow |
| Pick one of a few | `segmented` · `choice_buttons` · `choice_mapped` · `segmented_list` | wraps in a narrow panel | picked: raised segment |
| Pick one of many | `dropdown(ui, id, width)` (makes room so it never overflows) · `pick_item` / `pick_value` entries | — | picked entry framed |
| A level or parameter | `ui::Level` (square bar, name and value inside; `.log()`, `.vertical(h)`, `.max()`, `.modulation()`) | `ui::meter` for read-only | drag, double-click reset, right-click exact value / Learn / Modulate |
| One-line text | `text_field(ui, text, hint, width)` · `rename_menu` / `new_name_menu` · `list_field` | — | hint while empty |
| Label: control row | `field(ui, "Label", …)` (labels line up in one column) · `subsection` · `section` · `section_help` | — | long labels cut, whole on hover |
| Help | `help` (a sentence, or an (i) when help icons are on; **beside its row, never a row of its own**) · `heading_help` · `name_row` · `hint` (status words that must stay visible) | — | — |
| Lists and rows | `list_row(picked, text, right, badge)` · `counts` | — | hovered fill, picked = selection |
| Empty, loading, error | `empty_note` ("Nothing saved yet. Name the current look above and save it.") · `ui.spinner()` + `hint` · `colored_label(WARN, …)` with what to do | — | every panel says what to do next when empty |
| Menus with fields | `field_menu` / `right_field_menu` (stay open while you type); commands in them `ui.close()` | — | — |
| Windows | `dialog` (opens centred) | — | — |
| Area tabs | `section_tab` (an area in its hue; active = filled with black words) | — | — |
| Any box graph | `ui::nodes::node_canvas` + a `Graph` impl | — | band select, keys, wire menus, Tidy |

Accessibility on a desktop: every control is reachable by the pointer with a 22-point target and names itself
(`widget_info`) for the mapping system; keyboard shortcuts come from `menu::MENUS`; contrast: words are white or
`weak_text_color` on charcoal, dark on the accent (AA at these sizes); the light side flips ink and deepens every
colour. The phone pages (`remote.html`, `player.html`) use 44-pixel targets.

## 7. Anti-patterns (the test fails on the ones it can see)

- `egui::Slider`, round knobs, `ui.toggle_value`, `ui.selectable_value`, `ui.radio_value`: use the kit.
- `ui.weak`: use `hint`. `ComboBox::from_label` / `from_id_salt` in a panel: use `dropdown`.
- A help (i) alone on a row; a heading followed by a help row (`heading_help`); help as a section's first row
  (`section_help`).
- A bare `ui.label("Name")` before a control in a toolbar: use `field`.
- A `Color32::from_rgb` in a panel; a second accent; a lighter hue made by hand.
- Unicode symbols as icons (⇧ → ● ↔ render as boxes); emoji anywhere.
- `Command::new("open")`: `os::open` / `os::reveal`. `Response::dnd_release_payload`: `ui::drop_payload`.
- A menu with a field that isn't a `field_menu`.
- Per-frame work that nothing needs (scans, string building, allocation in a draw closure).
- Website patterns: hero blocks, three centred cards, gradient text, purple gradients, all-caps eyebrows, cookie
  banners, marketing copy. Panels are tools.

## 8. Dark and light

One slider, "Brightness" (0 darkest … 1 light; default 0.35), not a dark / light switch. Below the middle the
text is light on charcoal; above it dark on soft grey up to white. The step at 0.5 is deliberate: **there is no
mid-grey**, where neither ink reads. On the light side the accent and every kind hue switch to their deeper
variant (`theme::accent`, `Hue::color`) so they still read as text. Anything that draws a colour asks `theme` at
draw time, never caches it (the user can change brightness or accent at any moment). Tested in
`theme::brightness_tests`.

## 9. How a feature is built (the architecture template, in this app's terms)

1. **Model** (`phosphor-engine`): the data, with serde defaults on every new field (old saves must open; a
   fixture per release in `tests/old_projects`), and the pure logic with its tests. Nothing about the UI.
2. **State** (`crates/phosphor/src/app`): what the app does with it each frame (`State::run_*`), one entry point
   per action (`State::set_control`, `State::quick`), background work on threads with a `Deferred` result.
3. **Panel** (`crates/phosphor/src/panels`): one module per panel, under ~1,000 lines, drawing with the kit only;
   local UI state in `ctx.data` keyed by the panel and item (scroll, open sections, a search query), never in the
   model. Shared drawing goes into `ui/` as a component the moment a second panel wants it.
4. **Data flow**: the panel reads `&State`, writes through the `State` entry points or a returned action enum
   (`FxAction`, `ClipAct`, `Act`), applied after drawing, so the model never changes mid-frame.
5. **Cost**: nothing per frame unless it must; thumbnails and stills are cached (`Thumbs`, `Stills`); previews
   are live only while hovered; outputs do no work with nobody receiving. Measure with `--bench`.
6. **Empty, loading, error**: the panel says what to do when empty (`empty_note`), shows a spinner and words while
   waiting, and an orange sentence with the fix when something failed (`LAYER_ERROR` lines in the bench).
7. **Check**: `cargo test` (engine, app), `scripts/panel-check.sh` (every panel draws), a bench picture
   (`--ui-shot`) looked at, strings extracted and translated (`scripts/i18n`).

Worked example, the patch editor (`panels::patch`): model `patch::Patch` (blocks, wires, `look_at`, `bypassed`,
`text`, all with defaults) and its compiler, tested per block; the canvas is the shared `ui::nodes::node_canvas`
with a `Graph` impl (no hand-drawn boxes); undo history and the search query live in `ctx.data`; a change returns
`true` and the Shader Editor recompiles; cost: nothing until something changes; empty = Input wired to Output;
errors = the compile message in orange above the canvas.
