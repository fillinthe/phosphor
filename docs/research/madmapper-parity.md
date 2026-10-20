# MadMapper vs Phosphor mapping (parity review)

Written 2026-10-20. Sources: MadMapper 6.2.0 Beta 1 and 6.1.7 on this Mac (guides read as text; app not launched) and Phosphor's own code. Items marked (?) are not confirmed by the bundle's guides (from general knowledge of MadMapper; no web page was fetched).

## 1. Summary

Phosphor already covers MadMapper's core stage job: corner-pinned quads with true perspective, mesh warp, polygon masks (curved, feathered, inverted), soft-edge blending with a gamma curve, several outputs, line surfaces, OBJ 3D models with a pin-based projector solve, a grid test pattern, venue presets, live sources, and Syphon/NDI in and out.

The biggest gaps are about speed and safety when aligning on a real stage:
1. No way to place points by typing numbers, and no lock for a surface. An accidental drag mid-show cannot be prevented.
2. No spatial scanner (camera plus projector builds a picture of the room to map on). This is MadMapper's signature feature.
3. Soft-edge is one value for all four sides. Real projector overlaps need left, right, top and bottom set separately.
4. The mapping cannot be driven by MIDI/OSC or stored in cues, so you cannot cue geometry changes ("scene 2 moves the screen") or move a surface from a controller.
5. No groups of surfaces, no background reference photo, no per-surface blend mode, and masks cannot be attached to a surface.
6. Mesh warp has no Bezier handles (Phosphor uses smooth Catmull-Rom curves) and tops out at 8x8 (the engine allows 16).

## 2. Feature table

| MadMapper feature | What it does | Phosphor today | Effort |
|---|---|---|---|
| Quad surface with perspective | 4 corner handles, perspective toggle, scale/rotate handles on the surface | Has: corners, true homography (`Surface.corners`, mapping.rs:9). Scale/rotate handles on a whole surface: partial (translate only, `Surface::translate`) | small |
| Triangle, Circle surfaces | Shape cut from a quad | Has: `SliceShape` Rectangle/Ellipse/Triangle (mapping.rs:455) | - |
| Line surface | Click points, Enter to finish; picture runs along it | Has: `Line` with width (mapping.rs:382) | - |
| Mesh warp, grid | Add points with Alt+click, grid subdivisions, remove a row/column by right-click, select a segment or whole line | Partial: grid up to 8x8 in UI, Catmull-Rom, Flatten (mapping.rs panel:674). No add/remove single points or lines, no row/column select | medium |
| Bezier mesh warp, "Precision" | Tangent handles per point, link/unlink, polygon precision | Missing (smooth curve only, no handles) | large |
| 3D surface (OBJ) | Import and calibrate; wireframe, lighting, edit UVs. FAQ says no snapping to 3D vertices and no edge blending on 3D | Partial: OBJ, virtual camera, pins solve the projector (`Model3d`, `Pin`, mapping.rs:46). No lighting, no UV editing | medium |
| Input side: input quad/crop | Same shape drawn in the Input view; scale, rotate, flip X/Y; input follows UVs | Partial: input rectangle only (`Surface.input`, x/y/w/h), dragged in the Studio input view. No rotate, flip, or non-rectangular input | medium |
| Media / generators / Syphon / NDI / live input / ISF / materials | Per-surface visual; materials render at output resolution | Has: layer or mix per surface (`Surface.layer`), live, Syphon/NDI in, ISF. Phosphor has more generators | - |
| Screen capture as input (?) | Capture a display as a media | Missing as a named source (live capture-card/camera exist) | small |
| Surface list: layer order, show, lock, rename | Stack order = layer order; lock avoids accidents | Partial: tree with show/hide, duplicate, delete, rename. No lock, no reorder (`studio.rs` tree) | small |
| Groups (any surface types, nested) | Group transforms/opacity multiply | Missing | medium |
| Per-surface blend mode (add, multiply...), opacity, colour (RGB/HSV), luminosity, response | Compositing of overlapping surfaces | Partial: opacity + `ColorGrade` per surface; no blend mode | small-medium |
| Surface FX (2D/3D/line FX folders) | Chroma key, pixelate, deform UVs, etc. on one surface | Missing per surface (effects live on layers) | medium |
| Soft-edge / edge blending | Per-side width, curve, gamma; sample projects with 2 and 4 outputs | Partial: single `feather` + `blend_gamma` per surface (mapping.rs:19-23). No per-side widths, no black-level lift | medium |
| Masks | Freehand draw, Bezier points, circle/square/triangle, stack order add/subtract, per-mask invert, feather centred/inside/outside, attach to Nothing/Quad/Input, "mask surface" | Partial: polygon masks, invert, feather (centred only), smooth + sharp corners, shape presets (`Mask`, mapping.rs:577). Missing: freehand draw, feather direction, attach-to-surface, mask-as-surface, mask copy/paste | medium |
| Projector (output) setup | Resolution, destination screen, flip, background image, PNG mask, publish to Syphon/NDI, per-output test pattern | Has most: outputs, screens, identify, colour, Syphon/NDI per output. Missing: background reference image, output PNG mask | small |
| Test patterns / grids | TestCard, Test Pattern, Grid Generator (10x23 etc.), shown on projector | Partial: one alignment grid per output (`test_pattern`, mapping.rs:885). Missing: choice of patterns, colour bars, white/black fill, pixel-grid, numbered corners | small |
| Spatial Scanner | Camera + projector capture pixel-exact view; result becomes output background. Works with Sony, Canon, webcam, NDI; 2D only | Missing | large |
| Fine tuning: numeric X/Y of selected handle | Typed output position in the view toolbar | Missing (arrow nudge exists) | small |
| Nudge keys | Arrow keys; Tab to next handle | Has nudge (1 px, Shift = 10, `studio.rs:301`). Tab-to-next-handle missing | small |
| Snapping | Toggle snap near other objects | Partial: snap to 1/64 grid only; no snap to other surfaces' corners/edges | small |
| Zoom/pan, fit, zoom to selection | | Has zoom/pan; fit/zoom-to-selection (?) not confirmed | small |
| Show only selected / preview groups | Filters the preview | Missing | small |
| Lock affine transforms (for mesh editing) | Stop whole-surface moves | Missing | small |
| Multi-select, common inspector | Shift-select, edit shared fields | Missing (one surface at a time) | medium |
| Undo / redo | Toolbar | Has (Undo mentioned in venue load, `mapping.rs:756`); granularity for point drags not checked (?) | - |
| Duplicate / copy-paste | | Has Duplicate; copy settings to other surfaces missing | small |
| Presets | Projects only (?) | Has: venues (outputs + mapping) | - |
| Scenes and Cues | Grid of cues storing any parameter incl. geometry, per-parameter fades, auto-play, schedulers | Partial: Phosphor has cues (`model/cues.rs`) but they do not store mapping geometry | medium-large |
| OSC/MIDI control of surfaces | Every parameter has an address, e.g. `/surfaces/Quad-1/opacity`, output handles x/y, input scale/rot; Controls list; OSC Query | Missing: `ControlTarget` (model/control.rs:27) has no surface/mapping targets | medium |
| Projector control (PJLink) | Power/mute projectors over network | Missing | medium |
| Light mapping (DMX/Art-Net/sACN), laser | Fixtures | Has DMX/LED (`panels::leds`); no laser | out of scope |
| Modules (audio player, schedulers, MIDI) | | Not mapping related | - |
| MiniMad hardware | Export to a small player | Partial: Export Show app | - |

## 3. Recommended order (for aligning projectors on a stage)

1. Lock surface, numeric corner entry, Tab to next point, snap to other corners. Small, and the most used fine-tuning. Prevents wrecked alignment during a show.
2. Per-side soft edge (left/right/top/bottom) plus a "show overlap" view. Projector overlap is the most common stage job; today one value for all sides is not enough.
3. Test patterns: several patterns, per-output, with numbered corners and white fill. Cheap and needed before any alignment.
4. Background reference image per output (photo of the stage). Greatly speeds up aligning off-site and pairs with mask drawing.
5. Control of mapping by MIDI/OSC (opacity, corners, enabled) and storing mapping in cues. Lets a VJ switch layouts between songs. Needs new `ControlTarget` variants and cue fields.
6. Groups, multi-select, copy settings between surfaces, per-surface blend mode.
7. Mask upgrades: freehand draw, feather direction, attach to surface, Bezier handles on mask and mesh points.
8. Mesh warp point editing (add/remove single points, select rows), then Bezier handles.
9. Spatial scanner (camera + projector). Large, needs camera control and a structured-light pass; high wow factor but not blocking.
10. Screen capture source, surface FX, PJLink: later.

## 4. Sources

Bundle (`/Applications/MadMapper 6.2.0 Beta 1.app/Contents/Resources/`, same guides in 6.1.7):
- `Guides/Introduction to the User Interface.pdf` (surface types, Input/Output views, lock, snap, groups, projector tab, background image, mask PNG, test pattern, Syphon/NDI publish)
- `Guides/Mesh Warping.pdf` (Alt+click points, remove row/column, Bezier, Precision)
- `Guides/Masking.pdf` (freehand/Bezier, feather direction, attachment Nothing/Quad/Input, mask surface)
- `Guides/Space Scanner Guide.pdf`, `Guides/Knowledge Base.pdf` (scanner is 2D only)
- `Guides/Scenes and Cues.pdf`, `Guides/OSC Channels.pdf` (surface OSC addresses), `Guides/MadProjectorControl Guide.pdf` (PJLink)
- `Guides/The MadMapper FAQ.pdf` (no snapping or edge blend on 3D surfaces)
- `FactoryGenerators/{TestCard,Test Pattern,Grid Generator}`, `FactorySurface2DFX/`, `SampleProjects/SoftEdge Blending {2,4} Outputs Example.madproject` (project files not parsed; per-side soft-edge settings are not confirmed from them (?))

Online: madmapper.com docs were not fetched; nothing was downloaded or posted. Per-side soft-edge, the gamma/black-level controls, screen capture input and snapping details are from general knowledge (?).

Phosphor files: `crates/phosphor-engine/src/mapping.rs`, `crates/phosphor/src/panels/mapping.rs`, `crates/phosphor/src/panels/studio.rs`, `crates/phosphor-engine/src/model/control.rs`, `crates/phosphor-engine/src/model/cues.rs`, `ROADMAP.md` (Phase 4, lines 244-262).
