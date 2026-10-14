# Phosphor — evaluation & roadmap

Goal: one app that does what **VDMX** (modular, build-your-own interface), **Resolume** (clip
triggering, polish) and **MadMapper** (easy projection mapping) do, on a single efficient engine
that runs on macOS, iPadOS/iOS, and in the browser.

Stack: Rust · wgpu (Metal natively, WebGPU in the browser) · egui UI (same UI everywhere).

---

## What to take from each

| | Take this | Avoid this |
|---|---|---|
| **VDMX** | Modular UI: every window is a plugin you place anywhere. Control surfaces you build yourself. Data-source routing (LFO, audio, MIDI, OSC → any parameter). ISF effects. | Hard-to-learn mapping. Hybrid OpenGL/Metal/QC/Vuo engine (628 MB app). Dated visual polish |
| **Resolume** | Clip grid (layers × columns), one-click triggering, BPM sync and tap tempo, clean deck/column workflow, Advanced Output (slices) | Fixed UI layout. Mapping is good but sits in a separate, complex window |
| **MadMapper** | **Easy mapping**: drag corner points directly on the output, quads and meshes, masks, edge blending, many surfaces per projector, a live view of the projector | Mapping-first: weak as a VJ performance tool |

**Installed reference apps (inspected read-only):** VDMX6 Plus 1.2.13 (628 MB, ObjC, GL+Metal+QC+Vuo),
MadMapper 6.1.7 (385 MB, Qt 6 + OpenGL, binary `.mad` projects, ISF-style `.fs` materials,
Python modules), TouchOSC 1.5.3 (84 MB, native Metal, `.tosc` = zlib-compressed XML, Lua 5.1 scripting,
OSC UDP/TCP + Zeroconf + MIDI).

**Positioning:** VDMX's flexibility + Resolume's speed of play + MadMapper's mapping ease, in one
app that's much smaller and lighter on the CPU.

---

## TODO

Legend: [x] done · [~] in progress · [ ] not started

### Phase 0: Foundation (now)
- [x] Teardown of VDMX6 Plus (`VDMX-REVIEW.md`)
- [x] Rust workspace: `phosphor-engine` (platform-independent) + `phosphor` (app)
- [x] wgpu render graph: one command encoder per frame, one submit
- [x] 12 built-in effects (WGSL) with per-effect Mix (dry/wet), 7 generators, 10 blend modes, fit/fill/stretch
- [x] Live thumbnails per layer, Presets panel (built-in + yours), duplicate layer, dark theme, Phosphor icon font
- [x] Layer workflow: drag thumbnails to reorder, Solo, FX bypass, right-click menu, Space/⌫/⌘D shortcuts
- [x] Effects workflow: collapsible cards, drag handle to reorder, copy/paste whole chains, double-click resets a slider
- [x] Panels can be closed and reopened (View ▸ Panels), layout presets, floating windows rescued on smaller screens
- [x] **Master effects**: a pinned MASTER card whose chain runs over the whole mix
- [x] **Rutt-Etra** rebuilt natively (v002-style scan processor): scanlines / square mesh / triangle mesh /
      vertical lines / **points** draw modes, luminance or **RGB→XYZ** displacement, 3D rotate/perspective,
      additive beam, line/point size — GPU-generated, ~0 CPU
- [x] 28 effects total: + Gaussian/Zoom/Directional blur, Glow, Trails/Feedback (zoom-rotate tunnel),
      Technicolor, Bleach Bypass, Vignette, Film Grain, CRT, Analog Glitch, Halftone, Sharpen, Emboss, Tile
- [x] Engine: multi-pass effects and per-effect history (feedback) targets
- [x] Shader validation test (no GPU required)
- [x] egui app: preview, layer stack, effect chains, dockable panels (egui_dock)
- [x] Output window (drag to projector, F = fullscreen)
- [x] Stats (fps, engine ms, process CPU, memory) + `--bench` mode with PNG snapshot
- [x] Images (drag and drop / Open… dialog), project Save/Open (.phosphor JSON), autosave
- [ ] **CPU: ~45–55% of one core at 120 Hz, nearly all UI redraw (engine is ~0.7 ms/frame).**
      Fix: engine on its own render thread + swapchain per output; UI repaints only on change

### Phase 1: Media
- [x] Video playback via FFmpeg 9 + VideoToolbox hardware decode (H.264, HEVC, ProRes, VP9, AV1),
      software fallback for everything else (MPEG-4, GIF, alpha formats), seamless looping,
      bounded 3-frame queue, pause = zero CPU
- [x] Animated GIF, Hap / Hap Alpha / Hap Q (CPU DXT decode for now)
- [ ] Hap on the GPU: upload DXT blocks as BC1/BC3 textures (cuts Hap Q CPU from ~44% to ~0)
- [ ] Zero-copy video: VideoToolbox IOSurface → Metal texture → wgpu (skip the CPU copy)
- [ ] Ship FFmpeg inside the .app (LGPL build, decoders only) instead of relying on Homebrew
- [ ] Browser video: `<video>` → `copy_external_image_to_texture`
- [ ] Live camera / capture card input
- [x] Media bin with thumbnails (background FFmpeg frame grabs), import files/folders, click or drag onto layers
- [x] Seamless looping, speed 0.1–4×
- [ ] Scrubbing, reverse, in/out points, beat-length quantised loops

### Phase 2: Performance (Resolume-style play)
- [x] Clips grid: layers × columns; click a cell, a column header, keys 1–9, or a MIDI/OSC-learned trigger
- [x] Drop media-bin items onto cells; store a layer's current source into a slot
- [x] Transitions per layer: crossfade / dip to black / wipe with duration; the outgoing clip keeps playing
- [x] BPM clock: tap tempo, resync, beat indicator, **Quantize** (triggers wait for the next beat),
      "Beat" modulation source for beat-synced parameters
- [x] Clip banks A–D (pages of 9 columns), bar-quantise
- [x] Detach any panel into its own OS window (tab menu → Detach; closing it docks it back)
- [ ] Ableton Link, MIDI clock in, effect-based transitions
- [ ] Master crossfader (A/B groups)
- [ ] Layer groups

### Phase 3: Modular control (VDMX-style)
- [x] Dockable, rearrangeable panels; layout saved with the project
- [ ] **Control surfaces**: freeform canvases where you place buttons, sliders, XY pads and
      color pickers anywhere, and resize them
- [x] Data sources: audio input analysis (level, low/mid/high, 32-band spectrum) + LFOs (sine/tri/saw/square)
- [x] `~` button on every slider and layer opacity: modulate by any data source with amount/rate
- [ ] Step sequencers, beat detection / BPM-synced LFOs, envelope followers with attack
- [ ] Map any data source to any parameter (right-click → "Control with…")
- [x] MIDI in (all ports, CC + notes) and OSC in (UDP, any numeric arg) with **Learn**: pick a target
      (master fade, layer opacity/on-off, any generator/effect parameter, mix, on/off), press Learn, move a control
- [x] OSC port advertised over Zeroconf (`_osc._udp`) so TouchOSC lists "Phosphor"
- [ ] OSC out / feedback (so TouchOSC faders follow Phosphor), OSC Query, MIDI out
- [ ] **TouchOSC integration**: OSC in/out with Zeroconf (`_osc._udp`), OSC Query, and a `.tosc`
      exporter (zlib XML, `<lexml version='4'>` node tree) that generates a TouchOSC layout from any
      Phosphor control surface; `.tosc` importer to map an existing layout
- [ ] Keyboard mapping
- [ ] Presets / snapshots per layer, effect, and whole project

### Phase 4: Projection mapping (MadMapper-easy)
- [x] Surfaces: corner-pinned quads with true perspective (homography), any number per output
- [x] Direct manipulation: drag corners/centre in the Mapping panel **or on the projector output** (E)
- [x] Input region per surface (slice of the master), per-surface opacity, soft edge for projector blending
- [x] Alignment grid test pattern with orientation marker
- [ ] Multiple outputs (one window per projector), each with its own surfaces
- [ ] Mesh warp (bezier grid), triangle, circle, bezier masks
- [ ] Per-output identify / colour correction
- [ ] Per-surface color correction / brightness
- [ ] Save mapping with the project; quick-recall mapping presets per venue

### Phase 5: Effects & interop
- [ ] ISF import: GLSL → WGSL via naga's GLSL front end; cache the result. Same loader reads
      MadMapper materials/generators (ISF `.fs` + JSON header, optional `.vs`)
- [~] v002 ports: Blurs ✔, Glitch ✔ (analog), Film Effects ✔ (Technicolor, Bleach, Vignette, Grain); still to do:
      Optical Flow, Rutt-Etra displacement-image mode and lit (normals) rendering
- [x] Multi-pass effects (blur, glow, feedback/trails)
- [ ] Text / title source
- [ ] Syphon in/out (macOS), NDI in/out (optional plugin)
- [ ] Recording to ProRes / Hap
- [ ] Spout (Windows, if we ship there)

### Phase 5b: Streaming & recording (no OBS required)
- [x] Non-blocking GPU readback ring → FFmpeg pipe → VideoToolbox H.264 (never stalls rendering)
- [x] Stream out directly: YouTube Live, Twitch, custom RTMP, SRT — pick size/fps/bitrate, Start
- [x] Record to disk: .mp4 (H.264) or .mov (ProRes 422 HQ, VideoToolbox)
- [ ] Audio in the stream (program audio / mic via AAC); HEVC option; RTMPS; WHIP (WebRTC)
- [ ] Multiple simultaneous destinations; connection health graph; auto-reconnect
- [ ] OBS-friendly: Syphon (macOS) and NDI outputs, plus a virtual camera (CoreMediaIO extension)

### Phase 5c: Network players & remote audio (play *through* other machines)
- [ ] **Browser player**: anyone opens a link and sees the output live (WebRTC via WHEP;
      sub-second latency; works on laptops, phones, a Raspberry Pi running Chromium)
- [ ] **Audio back-channel**: the same WebRTC session carries the viewer's mic/line-in back to
      Phosphor, so you can react to *their* music (FFT bands become data sources, see Phase 3)
- [ ] **Phosphor Player** for Raspberry Pi / small PCs: a headless build that either plays the
      stream or renders the project itself from a synced clock (tiny bandwidth, perfect sync)
- [ ] Session page: QR code / short link, viewer list, per-viewer audio level meters
- [ ] Local network discovery (Zeroconf) for players; NDI/Syphon remain for pro setups
- [ ] Multiple players in sync (shared timecode), each with its own output mapping (Phase 4)

### Phase 6: Accounts & sync
- [ ] Project format: versioned JSON (layout + composition + mappings), already serde
- [ ] Phosphor account service: sign-in, project/layout storage
- [ ] Sync projects, UI layouts, control surfaces and mappings across devices
- [ ] Media handling: references plus an optional cloud media library

### Phase 7: Platforms
- [~] macOS (Apple Silicon + Intel), macOS 14+
- [x] **Browser build runs** (WebGPU, Chrome): `cd crates/phosphor && trunk build --release --dist ../../dist`,
      then serve `dist/`. Same engine + UI; 9 MB wasm. No video/audio/MIDI in the browser yet
- [ ] iPadOS / iOS (same engine + UI; touch-sized controls)
- [ ] Older macOS (11–13): test wgpu Metal backend; likely only small changes needed

### Engineering
- [ ] Render on its own thread / own swapchain per output, decoupled from UI repaint
- [ ] GPU timestamp queries for real GPU frame time
- [ ] Cache bind groups instead of creating them per pass
- [ ] Benchmark suite vs VDMX (CPU, memory, frame time, same clips)
- [ ] Crash reporting (opt-in), auto-update
- [ ] Code signing + notarization
