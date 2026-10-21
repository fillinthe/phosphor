# Comps as separate canvases: design (draft for the owner, 2026-10-20; built in 0.2.90)

**As built (0.2.90):** layers stay in one list with `LayerState::comp` naming their comp (not moved into each
comp), so ids, controls, cues, slices and collections needed no changes; a comp's layers are kept together in
comp order (`normalize_comps`). A project without comps has one implicit "Main"; `+ Comp` makes "Main" real and
adds the new one. The three questions below were answered with the defaults (owner asked to go ahead,
2026-10-21): one shared tempo and launch settings; Surprise and column triggers act on the open comp; a new comp
starts at the master's size. Engine: a single plain comp renders exactly as before; otherwise each comp is drawn
at its size into its own stack (`comp_accum`), through its effects, then blended into the master with its fit.

You described the app as **master → comp → layer**, and chose comps as *separate canvases*. This is how I'd build
it; please read the "What you'll see" part and say what's off before I start (it touches the project format, the
engine and most panels, so it's a few days of work in steps that each ship).

## What you'll see

- **A comp is a canvas of its own**: its own layers (clip rows, collections, generators…), its own size (can differ
  from the master), its own effects and opacity/blend. A project has one or more comps; a new project has one
  ("Main"). Your existing projects open with everything in "Main".
- **The master mixes the comps**, in order, like layers (opacity, blend, effects), and that's the program feed.
- **Outputs show any comp, any layer in any comp, or the master**, as slices (what "Shows on" does today for layers).
  A comp can go to several outputs; an output can show several comps as separate slices.
- **A comp can feed another comp** (as a layer source, like "Feed: Program" today), for layered looks.
- **The Clips panel shows one comp at a time** (tabs across the top: Main, Side wall, …); Layers and Mixer too.
  The Inspector edits the selected comp, layer or group. The Studio strip shows every output; a **Stage view**
  shows every output on one screen (your "see the full output as is").
- **Groups stay** inside a comp for quick stacking (a group is not a comp).
- **Masters**: undecided. The design leaves room: a "master" would be a comp flagged as a program feed, with its
  own outputs. We can add that later without changing projects again.

## Model

```
Composition
  comps: Vec<Comp>          // at least one
  master: Master            // order of comps + master effects (today's master_effects / master_fade)
  outputs, media_bin, collections, control maps, cues… (unchanged, project-wide)

Comp
  id, name, width, height (0 = master size), enabled, opacity, opacity_mod, blend
  layers: Vec<LayerState>, groups: Vec<LayerGroup>   // moved from Composition
  effects: Vec<EffectState>, fx_bypass
  clip_columns, launch settings (per comp, with the project's as default)
```

Ids stay unique across the whole project (`new_id`), so slices, control targets, cues and collections keep
pointing at layers by id whatever comp they're in. `Composition::layers` / `groups` become helpers over every comp
(`all_layers()`, `layer_mut(id)` searches comps) so most code changes little.

**Migration**: a project without `comps` loads as one comp "Main" holding its `layers`, `groups`, `clip_columns`;
`FORMAT` bumps; a saved project from the new version doesn't open in old versions (noted in the changelog).

## Engine

Today: layers → groups → `current` accumulator → master effects → outputs. New: for each enabled comp, render its
stack into the comp's own target (its size), run its effects; then mix the comps into the master accumulator
(opacity, blend, transform none), master effects, outputs. Feeds (`wanted`) gain comp ids so slices and "Feed"
sources can take a comp's finished picture. Cost: one extra full-size pass per comp (a copy), nothing per layer.

## UI, in steps (each one ships)

1. Model + migration + engine, with one comp: nothing visible changes, projects still open. Tests: migration,
   render of one comp equals today's render.
2. Comp tabs in Clips / Layers / Mixer; "New comp", rename, size; Inspector for a comp (opacity, blend, effects,
   size); "Shows on" for comps; the Stage view (all outputs on one screen).
3. Comp as a layer source ("Feed: <comp>"); control targets for comp opacity/enabled; cues store the comp mix.
4. Later: masters (several program feeds), per-comp launch settings.

## Open questions for you

1. Should each comp have its own tempo/launch quantize, or share the project's? (I'd share.)
2. When you pick a comp tab, should the Surprise / Quick actions act on that comp only? (I'd say yes.)
3. Default size of a new comp: the master's, or ask? (I'd default to the master's and let you change it.)
