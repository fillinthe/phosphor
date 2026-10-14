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

## Next up (priority order)

1. ~~**Multiple outputs**~~ ✔ any number of outputs, each with its own size (incl. portrait), mapping
   surfaces, window, screen and Identify; the main output is now a clean program feed
2. **Phone / iPad remote** (piggyback on the Mac over the network, no OSC layouts to build):
   - [x] Session server serves a touch **control page** generated from the live project: master fader,
         layer faders/on/solo, clip grid + column triggers, tap tempo, effect sliders for the selected layer
   - [x] Changes go back over a WebSocket and the page stays in sync both ways (moves when the Mac moves)
   - [x] QR code in the Session panel to open it instantly; works on iPhone, iPad, Android, any browser
   - [x] Optional password to control (Session panel); without it a phone only watches the output
   - [ ] Selected-layer thumbnails on the remote; quantize toggle; HTTPS so passwords aren't sent in clear text
   - [ ] Later: the full Phosphor UI (browser/wasm build) in "remote mode" mirroring the Mac's state
   - [ ] Later: native iPadOS/iOS app that can run shows on its own (same engine + UI)
3. ~~**Control Surface panel**~~ ✔ buttons/toggles/faders/sliders/XY pads, each driving many targets with ranges,
   mappable themselves; Mappings editor with Listen + ranges. Next: show it on the phone/iPad remote, TouchOSC `.tosc` export
4. **Mapping upgrades**: mesh warp (grid of bezier points), masks, per-output colour/brightness
5. **OBS / pro I/O**: Syphon and NDI out, virtual camera, NDI in
6. **Secure session links** (HTTPS) so viewers' mics work without the Chrome flag; WebRTC low-latency video
6b. ~~**Crash recovery**~~ ✔ rolling snapshot every 2 s; watchdog reopens Phosphor after a crash (stops after
    3 crashes in 2 min); restores project, layout, output windows on their screens, session, audio; offers
    Resume stream; a bug inside one frame is skipped instead of closing the app; crash.log for diagnosis
    - [ ] Freeze detection (app alive but hung) — needs care so modal file dialogs don't count as hangs
    - [ ] Restore video playback positions and active clips exactly
7. **Performance**: cache GPU bind groups (explorer previews cost ~2 ms CPU/frame at 40 tiles), render
   thread decoupled from UI, Hap decoded on the GPU
8. **Accounts & sync**: projects, layouts, presets, shaders, control surfaces to your account
9. **ISF gaps**: multi-pass / feedback / >16 params (142 shaders), City Lights generator renders black,
   Broken LCD looks unchanged at defaults
9b. ~~Export show~~ done (File → Export Show). Next: a locked "player only" mode (hide editing, password to
    unlock), Windows/Linux players, and notarized exports
9c. **Devices panel**: every connected phone/remote, viewer, audio sender, MIDI device and OSC sender, with
    disconnect / block / disable
9d. ~~Syphon out~~ ~~NDI out~~ done — next: NDI per output, NDI audio, NDI *input* (receive sources as layers), bundle the NDI SDK runtime
9e. **Online media library on phosphorfx.com**: browse/search a hosted catalogue of clips, loops, shaders,
    ISF and presets from inside the Media panel, sorted by newest/popular; download on demand into the bin
    (cached, with thumbnails), new-drops badge; later user uploads/packs tied to the account
10. **Distribution**: LGPL FFmpeg build (current bundled one is Homebrew's GPL build) · bundle FFmpeg inside the .app (LGPL build), notarize, auto-update, Intel test

## TODO

Legend: [x] done · [~] in progress · [ ] not started

### Phase 0: Foundation (now)
- [x] Teardown of VDMX6 Plus (`VDMX-REVIEW.md`)
- [x] Rust workspace: `phosphor-engine` (platform-independent) + `phosphor` (app)
- [x] wgpu render graph: one command encoder per frame, one submit
- [x] 12 built-in effects (WGSL) with per-effect Mix (dry/wet), 7 generators, 10 blend modes, fit/fill/stretch
- [x] Live thumbnails per layer, Presets panel (built-in + yours), duplicate layer, dark theme, Phosphor icon font
- [x] Layer workflow: drag thumbnails to reorder, Solo, FX bypass, right-click menu, Space/⌫/⌘D shortcuts
- [x] **Effects explorer**: live previews of every effect on the selected layer, categories, search, favourites,
      click or drag onto any layer/MASTER, Sources view for generators
- [x] **Shader Editor**: write WGSL effects/generators in-app, compile-as-you-type with line-accurate errors,
      live preview, "Edit a copy" of built-ins, saved to ~/Library/Application Support/Phosphor/effects
- [x] **⌘K command palette**: every effect, source, panel and action, fuzzy search
- [x] **+ Add effect browser**: pop-up with search (Enter adds first match), categories, Recent, Favourites,
      previews on that layer; Shift-click adds several. Same browser for layer sources (✦ button)
- [x] **Monitors**: any number of preview panels (View ▸ Add monitor), each showing Output, Mix, or one
      layer; dock/float/detach anywhere; optional thirds grid
- [x] Effects workflow: collapsible cards, drag handle to reorder, copy/paste whole chains, double-click resets a slider
- [x] Panels can be closed and reopened (View ▸ Panels), layout presets, floating windows rescued on smaller screens
- [x] **Master effects**: a pinned MASTER card whose chain runs over the whole mix
- [x] **Rutt-Etra** rebuilt natively (v002-style scan processor): scanlines / square mesh / triangle mesh /
      vertical lines / **points** draw modes, luminance or **RGB→XYZ** displacement, 3D rotate/perspective,
      additive beam, line/point size — GPU-generated, ~0 CPU
- [x] 33 effects: + Gaussian/Zoom/Directional blur, Glow, Edge Glow, Trails/Feedback (zoom-rotate tunnel),
      Slit-scan, Chroma Key (transparency), Gamma/Lift/Gain, Solarize, Technicolor, Bleach Bypass, Vignette,
      Film Grain, CRT, Analog Glitch, Halftone, Sharpen, Emboss, Tile
- [x] 12 generators: + Tunnel, Starfield, Spiral, Voronoi Cells, Oscilloscope Lines
- [x] Engine: multi-pass effects and per-effect history (feedback) targets
- [x] Shader validation test (no GPU required)
- [x] egui app: preview, layer stack, effect chains, dockable panels (egui_dock)
- [x] Output window: borderless (no title/menu bar), drag by its surface to the projector, F or double-click = fullscreen, cursor hidden
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
- [x] Media bin with thumbnails (background FFmpeg frame grabs), import files, click or drag onto layers
- [x] Media organising: favourites, your own collections, search, filter (videos/images/favourites/collection),
      sort (name/newest/type), tile size, rename
- [x] **Launch settings** for everything (clicks, drags, clips, MIDI/OSC triggers): Cut or Fade (time), Now / On beat / On bar
- [x] **Audio beat detector**: kick detection + BPM estimate with confidence (verified 128.6 BPM on a 128 BPM track);
      Follow audio locks tempo/beat to the music; "Audio beat (kick)" modulation source
- [x] **Syphon output**: composition published as "Phosphor" (+ optional per-output sources after mapping);
      zero cost until an app (MadMapper, Resolume, OBS, VDMX) connects; verified with a Syphon client
- [x] **Devices panel**: phones/browsers (disconnect, block), MIDI inputs on/off with activity, OSC senders (block)
- [x] **Menu bar** (macOS system menu + in-window): File/Edit/View/Output/Help, Save to current file, Open Recent,
      **Undo/Redo** (100 steps), zoom
- [x] **Export Show**: standalone .app (+ zip) with project, media, used shaders, mappings; FFmpeg bundled in the app
- [x] Right-click never moves sliders; app-wide controls mappable (tempo, launch, audio, bypass, Syphon, speed)
- [x] **NDI output** (runtime-loaded, sends only while watched; verified 60 fps to a receiver)
- [x] Effects explorer: click picks, double-click/drag adds; new effects fade in with Launch → Fade
- [x] **Map mode** + right-click menus fixed on sliders
- [x] **Watch folders**: every clip in a folder (+ subfolders) loads into the bin, new files appear automatically
      (rescan every 10 s, background thread); bin grouped by folder; "Fill clips of the selected layer" from a folder
- [x] File dialogs never pause the show (sheets; results arrive next frame); images decode in the background
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
- [x] Data sources: audio input analysis (level, low/mid/high, 32-band spectrum) + LFOs (sine/tri/ramp/square),
      random (S&H), 8-step sequencer, beat; **tempo sync** (1/4 beat … 8 bars), Add or **Sweep a range** (from/to),
      invert, live meter; a green tick on every modulated slider shows where it really is
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
- [x] Right-click any control → Learn, or type a mapping (OSC address, MIDI CC/note, key); every control also
      answers a built-in OSC address with no setup (/phosphor/master, /phosphor/layer/<id>/opacity, /phosphor/fx/<id>/<n>…)
- [ ] Keyboard mapping
- [ ] Presets / snapshots per layer, effect, and whole project

### Phase 4: Projection mapping (MadMapper-easy)
- [x] Surfaces: corner-pinned quads with true perspective (homography), any number per output
- [x] Direct manipulation: drag corners/centre in the Mapping panel **or on the projector output** (E)
- [x] Input region per surface (slice of the master), per-surface opacity, soft edge for projector blending
- [x] Alignment grid test pattern with orientation marker
- [x] Choose the output screen: Output ▾ menu / Screens list → window moves there and goes fullscreen (no menu bar)
- [ ] Multiple outputs (one window per projector), each with its own surfaces
- [ ] Mesh warp (bezier grid), triangle, circle, bezier masks
- [ ] Per-output identify / colour correction
- [ ] Per-surface color correction / brightness
- [ ] Save mapping with the project; quick-recall mapping presets per venue

### Phase 5: Effects & interop
- [x] ISF import (`crates/phosphor-isf`): GLSL → WGSL via naga; 264 of 406 shaders on this Mac convert
      (233/327 of /Library/Graphics/ISF); converted in the background, compiled lazily (2 per frame)
- [ ] ISF multi-pass / persistent buffers, >16 params, imported images (the 131 skipped shaders)
- [~] v002 ports: Blurs ✔, Glitch ✔ (analog), Film Effects ✔ (Technicolor, Bleach, Vignette, Grain); still to do:
      Optical Flow and lit (normals) Rutt-Etra rendering. Rutt-Etra can now displace from another layer's picture
- [x] Multi-pass effects (blur, glow, feedback/trails)
- [ ] Text / title source
- [ ] Syphon in/out (macOS), NDI in/out (optional plugin)
- [ ] Recording to ProRes / Hap
- [ ] Spout (Windows, if we ship there)

### Phase 5b: Streaming & recording (no OBS required)
- [x] Non-blocking GPU readback ring → FFmpeg pipe → VideoToolbox H.264 (never stalls rendering)
- [x] Stream out directly: YouTube Live, Twitch, custom RTMP, SRT — pick size/fps/bitrate, Start
- [x] Record to disk: .mp4 (H.264) or .mov (ProRes 422 HQ, VideoToolbox)
- [x] Audio in the stream: pick any AVFoundation input (mic, interface, BlackHole loopback) → AAC 160k
- [ ] HEVC option; RTMPS; WHIP (WebRTC)
- [ ] Multiple simultaneous destinations; connection health graph; auto-reconnect
- [ ] OBS-friendly: Syphon (macOS) and NDI outputs, plus a virtual camera (CoreMediaIO extension)

### Phase 5c: Network players & remote audio (play *through* other machines)
- [x] **Session** (v1, LAN): built-in server; any browser (laptop, phone, Pi) opens the link and sees the
      output live as MJPEG (≤1280 px, 10–30 fps); Start/Stop + copy link in the Session panel
- [x] **Audio back-channel**: the player page's "Send my audio" streams the viewer's mic over a WebSocket
      into Phosphor's audio analysis ("Session viewers" source), so visuals react to *their* music
- [ ] Upgrade transport to WebRTC (WHEP) for sub-second latency and HTTPS/TLS so mic access works
      without the Chrome insecure-origin flag
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
- [x] macOS app bundle with icon (`scripts/bundle-mac.sh` → /Applications/Phosphor.app), opens .phosphor files
- [~] macOS (Apple Silicon + Intel), macOS 14+
- [x] **Browser build runs** (WebGPU, Chrome): `cd crates/phosphor && trunk build --release --dist ../../dist`,
      then serve `dist/`. Same engine + UI; 9 MB wasm. No video/audio/MIDI in the browser yet
- [ ] iPadOS / iOS (same engine + UI; touch-sized controls)
- [ ] Older macOS (11–13): test wgpu Metal backend; likely only small changes needed

### Engineering
- [ ] Render on its own thread / own swapchain per output, decoupled from UI repaint
- [ ] GPU timestamp queries for real GPU frame time
- [ ] Cache bind groups instead of creating them per pass — measured: 40 live explorer previews cost ~2 ms
      CPU/frame (30% → 105% of a core), nearly all pass/bind-group setup; explorer defaults to Hover previews
- [ ] Benchmark suite vs VDMX (CPU, memory, frame time, same clips)
- [ ] Crash reporting (opt-in), auto-update
- [ ] Code signing + notarization
