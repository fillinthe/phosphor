# Designer: a TouchDesigner-style section for Phosphor (notes, 2026-10-28)

Asked: "maybe we should do the TouchDesigner side as well… it would be another section of the app."
Status: phases 1–4 built 2026-10-29 (Designer area, Picture / Effect / Mix / Output boxes backed by hidden layers,
shared node canvas with Patches; Number boxes; boxes of boxes; points). See docs/NOTES.md for what was checked.

Phase 4 as built: points travel as **point pictures** (one point a texel: xyz in rgb, brightness in alpha), which
the engine keeps 16-bit float and at their own size (`LayerState::is_data`, `gpu::DATA_FORMAT`, a raw copy for
feeds). So every points box is an ordinary shader (`points.wgsl`: `pts_shape` generator, `pts_relief`, `pts_move`,
`pts_particles` effects with history, `pts_dots` / `pts_lines` / `pts_mesh` renders with a vertex stage, camera and
light inside, coloured from `EffectState::other`), a Model box is a layer whose source is `Live { format: "model" }`
(`models::Models` uploads the OBJ's vertices once), and the Designer only adds a Points port kind. No depth buffer yet.

## What TouchDesigner is, in short

You wire small boxes ("operators") together on a canvas. Each box does one job and passes its result down the wire.
Every box shows a live preview of what comes out of it. Operators come in families, by the kind of data they carry:

| Family | Carries | Typical boxes |
|---|---|---|
| **TOP** (texture) | pictures, on the GPU | Movie In, Noise, Blur, Level, Composite, Feedback, Displace, GLSL |
| **CHOP** (channel) | numbers over time | LFO, Audio In / Spectrum, MIDI In, OSC In, Math, Filter (smooth), Lag, Trigger, Logic |
| **SOP** (surface) | 3D geometry, CPU | Sphere, Grid, Noise, Transform, Copy, Merge |
| **POP** (point, new in 2025) | points / particles / lines on the GPU | Particles, point clouds, splines, Trace (image → lines), DMX fixtures |
| **DAT** (data) | text, tables, scripts | Table, Text, Web, Python scripts, Execute (run on events) |
| **COMP** (component) | containers and UI | Container, Base (a reusable box made of boxes), Window, Camera, Light, Geometry, panels / buttons |
| **MAT** (material) | 3D surface looks | Phong, PBR, GLSL MAT |

Things that make it what it is:
- **Pull, not push:** a box only works when something downstream (an output, a preview you're looking at) needs it.
  Nothing on screen = nothing runs.
- **Any setting can be an expression or a wire** from a CHOP ("export"): an LFO drags a blur's size along.
- **Networks inside boxes:** a COMP is a box that holds its own network; save it as a .tox and reuse it anywhere.
- **Feedback** is a first-class box (the last frame comes back in).
- **Python everywhere** (scripts, callbacks, expressions). This is where TouchDesigner's power and its learning
  curve both come from.
- **Licence:** free non-commercial version limited to 1280×1280; paid versions for work.

## What Phosphor already has that a node section would build on

Phosphor isn't starting from zero. Most of the pieces exist; they just aren't shown as one wired canvas.

| TouchDesigner idea | Already in Phosphor |
|---|---|
| TOP boxes | Every effect (several hundred ISF / built-in / lostjared / FFGL), generators, movies, cameras, NDI, Syphon, screen capture, text, Lottie, MilkDrop, G-Drop |
| TOP chains | Layer / group / comp / master effect chains, Feed sources (another layer's picture, its own previous frame) |
| GLSL TOP | Shader Editor with `@feedback` and `@pass` (multi-pass) |
| A node canvas | **Patches** inside the Shader Editor: 40 blocks wired into one shader (`phosphor_engine::patch`, `panels::patch::patch_canvas`) |
| CHOPs | Modulators: LFOs, audio level / bands / beat, random, steps / sequencer, drawn curves, looper, hand / face / body tracking, live data (weather, ISS, earthquakes) |
| CHOP → parameter exports | Modulation on any value, MIDI / OSC / DMX / Stream Deck mappings, controller value shaping (smooth, steps, curve, formula) |
| SOPs | 3D model slices (OBJ), mapping meshes |
| POP DMX fixtures | LED / DMX fixtures, Open Fixture Library, LED scanner |
| COMP UI panels | Control Surface (buttons, faders, XY pads, TouchOSC export) |
| Outputs | Mapped outputs, Syphon / Spout / NDI out, streaming, recording, DMX out |

So the realistic job is a **new view and a graph that runs things Phosphor can already do**, plus a handful of new
box kinds (numbers math, logic, points), not a second engine.

## Open-source projects worth learning from (research before building)

- **TiXL** (formerly Tooll3, MIT, C#): node graph for real-time motion graphics, aimed at VJ content, with a timeline
  and keyframes. Closest in spirit; good for how it previews boxes, handles types and mixes keyframes with wires.
- **vvvv gamma** (free non-commercial, .NET), **cables.gl** (MIT, in the browser), **Nodes.io**: other node-based
  visual tools; cables in particular shows a clean "ops + ports + types" model.
- **egui-snarl** (Rust, egui): a ready-made typed, serializable node-graph canvas for egui. Worth trying before
  growing our own Patches canvas further. **egui_node_graph** is an older alternative.
- Our own **Patches** canvas: already works, already in Phosphor's UI kit. Decide early whether to grow it or switch.

## A recommendation

Make it a **Designer area** (owner's name, 2026-10-28: a fourth top-bar area next to Show / Outputs / Control), where one Phosphor **comp** can be
built as a network instead of a layer stack. Keep it Phosphor-shaped rather than cloning TouchDesigner:

1. **Two wire kinds to start: pictures and numbers.** Picture boxes = anything a layer can show or any effect;
   number boxes = modulators plus math. Leave 3D, points and tables for later.
2. **Boxes are the existing things.** A "Blur" box *is* the Blur effect; a "Movie" box *is* a movie source; an "LFO"
   box *is* a modulator. One registry, so every new effect shows up as a box for free.
3. **The network compiles to the engine's existing passes** (the engine already runs chains, feeds, multi-pass
   plans and feedback buffers). A box only renders when something on an output or an open preview needs it (pull),
   matching the "outputs do no work while nobody's watching" rule.
4. **A network is a source.** Its end box can be any layer's source, or a whole comp. So you can build a look as
   nodes and still launch it from clips, map it, and cue it.
5. **Any setting can take a number wire.** Same path as modulation today, so MIDI / OSC / DMX / Learn all keep
   working on every box's settings.
6. **Boxes inside boxes** (a group box with its own network, saved and reused like a preset). This is what makes a
   node tool scale; plan the data model for it from day one even if the UI comes later.
7. **No Python.** Formulas already exist for controller values (`ValueFx::Formula`); grow that into an expression
   box rather than embedding a scripting language.

Saved projects: a network is a new, optional part of a comp (empty by default), so old files keep opening.

## Phases, with rough sizes

| Phase | What you'd get | Size |
|---|---|---|
| 1. Picture network | Designer area; source / effect / mix / feedback / output boxes; live preview on each box; drives one comp | large (about a week of work) |
| 2. Number network | Modulators, audio, MIDI / OSC in, math, smooth, logic, trigger boxes; wire a number into any setting | medium |
| 3. Reusable groups | Box-of-boxes, saved by name, shared between projects; settings shown on the outside of the box | medium |
| 4. Points and 3D | Particles / point clouds / lines on the GPU, model loading, camera and lights (overlaps the 3D model slices). *Built: shapes, relief, model, move, particles, dots / lines / mesh with camera and light.* | large |
| 5. Data | Tables, text, web / JSON in (the live data feeds as boxes), timers and events | medium |

Phase 1 is the decision point: once a network can stand in for a layer stack, the rest is adding boxes.

## Questions for you before starting

- Should a network replace a comp's layer stack, sit beside it as a source, or both? (Recommended: both, starting
  with "a network is a source".)
- Grow our own Patches canvas, or try egui-snarl? (I'd try egui-snarl for a day first.)
- Which first: pictures only (phase 1), or pictures and numbers together?
- Any TouchDesigner networks you use now that you'd like to rebuild here? They'd make good first test cases.

Sources:
- [What's new in the 2025 TouchDesigner release (Derivative)](https://derivative.ca/community-post/whats-new-2025-touchdesigner-release/73348)
- [2025 Official update (Derivative)](https://derivative.ca/community-post/2025-official-update/73153)
- [New features, 2025 official devs' choice (Derivative)](https://derivative.ca/community-post/new-features-2025official-devs-choice/74550)
- [What's new in the 2025 TouchDesigner release (Interactive & Immersive HQ)](https://interactiveimmersive.io/blog/touchdesigner-resources/whats-new-in-the-2025-touchdesigner-release/)
- [TiXL (Tooll3) on GitHub](https://github.com/tixl3d/tixl)
- [egui-snarl docs](https://docs.rs/egui-snarl/)
- [egui_node_graph](https://github.com/ealmloff/egui_node_graph)
