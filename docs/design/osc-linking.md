# Draft to-do: OSC, linking and programmable controls (VDMX / Resolume style)

For the owner. Started 2026-11-15, more 2026-11-18: [x] = built, [~] = partly (what's left is in the note), [ ] = not started.

**Where Phosphor is today.** More of this already exists than it may seem. There are shared modulators
(`model/modulators.rs`, the "LFOs & Modulators" panel in `panels/modulators.rs`). Controller mappings carry a
value-effect chain: smooth, steps, curve, `$VAL` formula, delay, drawn curve, endless knob, dead band (`ValueFx` in
`model/control.rs`). There's soft takeover, feedback to controllers, and built-in `/phosphor/...` OSC addresses by id
or name with wildcards (`control/targets.rs`, `control/mod.rs`). There's an OSC Query server and client, TouchOSC
export and import (`tosc.rs`), mapping presets (`presets::mappings`), and a Control Surface designer with 11 widget
kinds, pages, docked "My panel" tabs and Computed formula widgets.

**What's missing** is mostly the *workflow* and *depth* that make VDMX feel programmable:
- linking starts from the control's right-click menu, not from the source;
- one fader driving many things can't give each target its own slice or curve;
- the LFO is a small form, not an editor window;
- the OSC scheme uses internal ids, not positions, and has no query or relative values.

Legend: V = VDMX has it, R = Resolume has it. Size: S = a day or two, M = about a week, L = several weeks.

## Linking & programmable controls

- [x] **Per-target input slice ("dial range")** (**built 2026-11-15, tested: each target's "part of the fader" in the Control Surface target list and in Links**) (V, R): each target of a multi-target control listens to only part
  of the fader, such as 0-33% drives blur and 33-100% drives zoom, so one knob plays a staged look. Today a
  `Binding` (`model/control.rs`) has only an output `ValueRange` (min / max / invert). The input side isn't there for
  surface widgets, and `ValueFx::Clamp` only exists per controller mapping. Size S.
- [~] **Per-target curves and value effects on every link** (**built 2026-11-15, tested: the instant ones (steps, curve, formula, threshold, limit, repeat, randomize, drawn curve); smooth / delay / fall stay on controller mappings**) (V): give `Binding` the same `fx: Vec<ValueFx>` that
  `ControlMap` has, so each target of a surface fader or modulator can have its own curve, steps, delay or formula.
  Reuse `value_fx_menu` (`panels/control.rs`) as a shared component. Size M.
- [x] **Link from the source: drag onto any control** (**built 2026-11-18, tested: `link_roles`, dials as drivers, the
  drop rules; the drag itself not yet seen on screen**) (V): drag a modulator (its link handle in Modulators or its
  window), a Control Surface widget (link handle beside "Drives", each bar, an XY pad's Y), a row's input in the MIDI /
  OSC monitor, a dial, or any slider with Alt held, onto any slider (`ui::Level`): it lights while it can take the
  drop. A modulator becomes the slider's modulation (sweeping the whole range when it had none); a widget or dial
  drives it (one more target in its list); a slider dropped on a dial is driven by the dial; two plain controls: the
  one landed on follows the one carried; a controller input is mapped. `ui::link` (`LinkSource`, `link_handle`,
  `link_drag_text`, `carry_from_slider`, `link_drop`), app `State::link`, engine `link_roles` / `is_driver` /
  `Composition::link_driver`. Size M.
- [~] **"Links" view: one source, every target** (**built 2026-11-15: MIDI / OSC › Links; follows and surface targets edited in place, controller inputs and modulator followers listed. Seen on screen empty only**) (V: Data Source Mappings window): a single list grouped by source
  (a MIDI knob, an OSC address, a modulator, a surface widget). It shows every target with its range and curve,
  editable in place, with "jump to" links. Today `mappings_editor` (`panels/control.rs`) lists one row per pairing,
  and modulator followers are listed nowhere. Size M.
- [x] **Any control can follow any other control** (**built 2026-11-15, tested: right-click › Follow another control; `Composition::follows`, `control::follow_sends`. The menu itself was not seen on screen**) (V: every UI item is a data source): for example, layer 2's
  opacity follows layer 1's speed, inverted. Add a `ModSource` / `link` to a `ControlTarget`, read with
  `control::get_target`. Today only shared modulators and surface widgets can be sources; the Computed widget reads
  controls but only on a surface. Size M.
- [x] **Macro knobs on a layer, effect or clip (dashboard)** (**built 2026-11-18, tested: dials on layers, groups,
  clips and MASTER, their targets follow only when a dial moves, old saves open without them, a duplicated layer's
  dials drive the copy. The Inspector section not yet seen on screen**) (R): up to eight named dials (`MacroDial`,
  `model/macros.rs`; `LayerState.dials`, `LayerGroup.dials`, `Clip.dials`, `Composition.master_dials`, all with serde
  defaults) shown under the layer's opacity in the Inspector, in a group's and MASTER's Inspector and in the picked
  clip's section (`panels::dials_section`). Each dial is a control (`ControlTarget::Macro`, OSC
  `/phosphor/layer/<id>/dial/N`, `/phosphor/master/dial/N`): map a knob to it, Alt-drag a slider onto it (or it onto a
  slider), or pick settings in its menu, each with its own part of the dial, curve and range (`binding_list`).
  `State::run_dials` hands a dial's moves on (nothing runs without dials). Not yet: dials on an effect. Size M.
- [~] **Copy / paste a control's links** (**was already there: Copy / Paste mappings in the right-click menu, Copy / Paste the motion. Not yet: onto all of a layer's sliders at once**) (V: Copy / Paste Local): copy a slider's mappings, modulation and value
  effects onto another control, or onto all of a layer's sliders at once. Size S.

## OSC

- [~] **Position-based addresses and "selected" addresses** (R) (**built 2026-11-15**, tested: `/phosphor/layers/N/…`
  = the Nth layer from the bottom of the open comp, `/phosphor/selected/layer/…`, and `fx/K/…` after either;
  `control::placed_address`. Left: clips by column under a placed layer already work (`clip/3`); groups, slices and
  "selected clip" by place): `/phosphor/layer/1/opacity` = the first layer in the
  stack, `/phosphor/selected/layer/opacity`, and `/phosphor/layer/1/clip/3/play`. A TouchOSC page then works on any
  project, and a single "selected layer" strip works on whatever is clicked. Today addresses use internal ids or
  names (`default_osc_address`, `named_address` in `control/targets.rs`). Size M.
- [~] **Query, relative and typed values** (**built 2026-11-15, tested over a real socket: "?" answered on the feedback port, "+" "-" "*", whole number = menu item, name = menu item, blend mode is a control (`control::ask`). Not yet: colors as the `r` type on built-in addresses**) (R): sending `"?"` returns the current value. `"+" 0.1`, `"-"` and `"*"`
  nudge it. Ints pick menu items, strings pick by name (blend "Add"), and colors use the `r` type. Today a value is a
  0..1 float only (`(String, f32)` in `control/mod.rs`), except for text. Size M.
- [~] **OSC output: send every change, or only the controls you pick** (**built 2026-11-15, tested: "Send all to" host:port in MIDI / OSC; per-control Send out with its own address and host was already there. Not yet: Bonjour list, bundles**) (R, V: senders on every control): the "Output
  all" mode and a per-control "send OSC" switch (right-click). Each choice has a custom address, a destination
  (Bonjour list / IP) and optional bundles. Today feedback only goes back to devices that sent to us
  (`ControlHub::feedback`), and only surface widgets have an `osc_out` address, all to a single `osc_host`.
  Size M.
- [~] **Show the OSC address on any control** (**was already there: the address with a Copy button is in every control's right-click menu. Not yet: an "OSC mode" that labels every slider**) (R: Shortcuts › Edit OSC): an "OSC mode" that labels every slider with
  its address, plus a right-click "Copy OSC address" command. Today you find it in OSC Query or the code. Size S.
- [x] **Detect range: map a whole row at once** (**built 2026-11-18, tested: rows of CCs, notes, OSC addresses (also
  /fader1 to /fader12), DMX channels, Stream Deck keys; wrong pairs refused; a row learned again replaces, not doubles.
  The menu seen on screen, the listening not tried with a real controller**) (V): MIDI / OSC › Mappings › Learn a row:
  pick clip columns, the selected layer's clips, every layer's opacity or the selected layer's dials and how many, press
  Listen, move the first knob then the last; the ones between are worked out and each control gets one
  (`ControlSource::row`, `Composition::{row_targets, learn_row}`, `model/detect_range.rs`). Size S.
- [x] **String and list receivers** (**built 2026-11-15: a name picks a menu item; text layers and `<address>/<k>` list items were already there**) (V): an OSC string sets text (exists for text layers), picks a menu item by name,
  or one item of an OSC list feeds a control. Size S.
- [ ] **Per-surface OSC Query publishing and a browser remote per surface** (V): each surface or page is published on
  its own and served as its own web page. Today the session server and the remote pages exist (`net.rs`, `remote`)
  but are not per surface. Check what the remote already shows before building this. Size M.

## LFO / modulator window

- [x] **A real LFO editor window** (**built 2026-11-18, tested: pause, reverse, retrigger / scrub in the engine
  (`Modulator.{paused, reverse}`, `ModPhases::set_cycle`); the window itself not yet seen on screen**) (V: LFO plugin):
  the Modulators panel's open button puts a modulator in a window of its own (`panels::modulator_window`, any number
  open, drawn app-level): a big live wave with its playhead (click or drag on it to scrub), pause, reverse, retrigger,
  its rate / sync / shape settings, and "Followed by" with each control's own Amount (or From / To when it sweeps)
  and invert (`Composition::linked_modulations_mut`). Length in beats / bars comes from Sync. The owner's ask: "an LFO
  window, not in a right-click menu". Size M.
- [~] **More wave shapes and wave settings** (**built 2026-11-15, tested: phase offset, pulse width, ramp down, exponential, smooth random, noise. Not yet: several waves in one LFO**) (V): phase offset (so several LFOs can be out of step), pulse width,
  ramp down, smooth random, noise, exponential; several waves per LFO, each its own source (multiphase). Today there
  are sine / triangle / saw / square / random / drawn (8 points) and no phase field (`ModSource`, `Modulation` in
  `model/layer.rs`). Size M.
- [ ] **Curve-editor waves and record-a-wave** (V): draw the wave with points and handles (more than 8 steps), mix
  shapes in one cycle, and record a wave from a knob or any source. The Data Looper (`Looper`) and `ValueFx::Lookup`
  are the parts to build on. Size M.
- [~] **Lift the 16-modulator limit and the "earlier only" chain** (**partly, 2026-11-15: 32 a project. Not yet: grow on demand, several inputs**) (V): `MAX_SHARED = 16`
  (`phosphor-engine/src/control.rs`) caps a show. Chains can only take from modulators above them in the list, with
  one input each. Allow more (or grow on demand) and let a modulator mix two or more inputs. The Designer's Number
  boxes already do the many-input case. Size M.
- [x] **LFO templates** (**built 2026-11-15, tested: Modulators › Saved, four starters**) (V): save a modulator (or a set) by name and add it again in any project, alongside the
  built-in starters (beat pumper, slow drift, strobe on 1/16). Use `presets::PresetStore` (`named_saves`). Size S.

## Custom control windows

- [ ] **Surface pages as floating windows** (V: every window is free-floating): pop a Control Surface page out into
  its own window (second screen, iPad sidecar) as well as a docked "My panel" tab (`Tab::MyPanel`). Size M.
- [ ] **More widget kinds and options** (V): a button grid (one control, many buttons), radio groups across buttons,
  a dropdown widget, a meter / value display, a rotary knob, a step / sequencer widget, and labels that show a live
  value. Choose "publish as index / one switch per item / name" for pick-one widgets. Today: `WidgetKind` in
  `model/surface.rs` (11 kinds). Size M.
- [ ] **Stronger expressions** (V: ExprTk in 6.3): Computed widgets with if / else, variables, and outputs that are a
  2D point or a color, reading any control by name (not only `a`, `b`, `c` slots). Today `WidgetKind::Computed` takes
  a formula over listed inputs, and `ValueFx::Formula` has only `$VAL`; neither has `$MIN` / `$MAX`. Size M.
- [ ] **Surface presets inside the surface** (V: section presets): a preset bar on the surface (and on any panel) that
  snapshots its widget values and recalls them with a fade time, mappable and nextable. Today `presets::surfaces`
  saves the layout, not snapshots of values. Size M.

## Other

- [ ] **Built-in controller profiles** (R: shortcut presets ship for common gear): ready-made mapping sets for APC40,
  APC mini, Launchpad, MIDI Fighter Twister, nanoKONTROL and TouchOSC templates, with lit-pad feedback. Today
  `presets::mappings` has no built-ins. Size M (S per device once the first one is done).
- [ ] **Per-control soft takeover and echo settings** (V): set pickup / fade / immediate per mapping and per kind
  (sliders vs buttons, MIDI vs internal). Today soft takeover is one app-wide setting (`panels/control.rs`). Check
  first how fine-grained it already is. Size S.
- [ ] **Scripting (bigger idea, owner to decide)** (V: JavaScript in 6.x is limited; mostly expressions): a small
  script per surface or modulator (Rhai or Lua) that reads and writes any control by name. Partly covered today by the
  MCP server (`mcp.rs`), Designer data boxes and formulas. Size L.

## Sources

- Phosphor: `docs/research/vdmx-features-full.md` (§7-9, LFO, Control Surface), `docs/research/vdmx-gaps.md`,
  `docs/PARITY.md`, and code as cited.
- Resolume OSC: https://resolume.com/support/en/osc (addresses, selected layer, `?` query, `+ - *` relative, typed
  values, output-all vs custom, wildcards, presets).
- Resolume Dashboard: https://resolume.com/support/en/dashboard (dials, many parameters per dial, in/out, dial range,
  invert).
- Resolume parameter-to-parameter linking is not native (forum requests for "Global Parameters"), so that item is
  VDMX-only.
