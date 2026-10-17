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
   - [x] Layer thumbnails on the remote (mixer strips + the selected layer on Controls); Now / Beat / Bar button
   - [x] Audience play: QR code (also as a layer on the output), guests take timed turns at the Control Surface / columns / Surprise
   - [x] HTTPS session link (port + 1, self-signed) for phone mics and passwords
   - [x] Audience play: caption on the code, reconnect keeps the place, several players at once (one per Surface page)
   - [x] Audience play: player names (typed on the phone) on the output
   - [x] Audience play: rest between turns per phone ✔ (Session panel "Rest after", 2026-10-17)
   - [ ] Later: the full Phosphor UI (browser/wasm build) in "remote mode" mirroring the Mac's state
   - [ ] Later: native iPadOS/iOS app that can run shows on its own (same engine + UI)
3. ~~**Control Surface panel**~~ ✔ buttons/toggles/faders/sliders/XY pads, each driving many targets with ranges,
   mappable themselves; Mappings editor with Listen + ranges. on the phone/iPad remote ✔. Next: TouchOSC `.tosc` export, multiple surface pages
4. ~~**Mapping upgrades**~~ mesh warp ✔ (Catmull-Rom grid up to 8×8), masks ✔ (polygons, invert). Per-output colour ✔,
   blend curve ✔, mask feather ✔, circle/triangle masks ✔, smooth (Catmull-Rom) mask outlines ✔. Next: per-point bezier handles
5. **OBS / pro I/O**: Syphon out ✔, NDI out ✔, cameras/capture ✔, network streams in ✔, Syphon in ✔, NDI in ✔. Next:
   virtual camera out (needs a Developer ID); capture-card audio shows up as an input device already (Audio panel)
6. **Secure session links** (HTTPS) so viewers' mics work without the Chrome flag; WebRTC low-latency video
6b. ~~**Crash recovery**~~ ✔ rolling snapshot every 2 s; watchdog reopens Phosphor after a crash (stops after
    3 crashes in 2 min); restores project, layout, output windows on their screens, session, audio; offers
    Resume stream; a bug inside one frame is skipped instead of closing the app; crash.log for diagnosis
    - [x] Freeze detection: heartbeat from the render loop, watchdog restarts after 20 s (sleep-aware)
    - [x] Restore video playback positions (active clips were already saved with the project)
7. **Performance**: render thread decoupled from the UI (today a hidden window renders via a patched eframe —
   see the project instructions), Hap decoded on the GPU. (Bind-group caching was measured slower and dropped.)
8. **Accounts & sync**: projects, layouts, presets, shaders, control surfaces to your account
9. **ISF gaps**: ~~multi-pass / feedback / >16 params / imported images / 12 buffers / _imgRect / MadNoise / MadSDF~~ ✔
   (391/406 on this Mac: 324/327 ISF, 39/42 MadMapper Materials, 19/23 Surface FX, 9/14 generators; the MadMapper includes
   are clean-room). Next: a curve editor (curve inputs load with their default), vertex shaders that move geometry (6), Pixelize (naga internal error)
9b. ~~Export show~~ done (File → Export Show). ~~Locked "player only" mode~~ ✔ (View › Lock Controls, password,
    exports can open locked). Next: Windows/Linux players, notarized exports
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
- [ ] **CPU: 25–50 % of one core at 110–120 Hz** (bench, 2026-10-16; the figure swings with how the loop is paced).
      Profile: engine encoding ≈ 3 %, UI layout ≈ 4 %, UI drawing ≈ 4.5 %, Metal's queue threads ≈ 12 % (two
      submits per frame). Tried 2026-10-16: one GPU submission per frame (engine command buffer carried by an egui paint callback):
      no gain in three A/B runs (7.3 vs 7.5 ms CPU per frame), dropped. The bench's CPU figure still swings with
      display pacing (60 vs 120 Hz, vsync vs sleep); a fixed-rate bench mode would make small wins measurable.
      The render thread moves work off the UI but doesn't remove it. 2026-10-17: "Render at" 60 fps default (Output panel):
      41 → 31 % of a core on the 120 Hz display (30 fps: 17 %); the UI pass waits for its slot at a cap.

### Phase 1: Media
- [x] Video playback via FFmpeg 9 + VideoToolbox hardware decode (H.264, HEVC, ProRes, VP9, AV1),
      software fallback for everything else (MPEG-4, GIF, alpha formats), seamless looping,
      bounded 3-frame queue, pause = zero CPU
- [x] Animated GIF, Hap / Hap Alpha / Hap Q (CPU DXT decode for now)
- [x] Hap on the GPU: DXT blocks uploaded as BC1/BC3 textures, Hap Q converted in the shader (720p Hap 25% → 1.2% CPU, Hap Q 39% → 2.8%)
- [x] Hap R (BC7) on the GPU
- [ ] Hap Q Alpha on the GPU (still decodes on the CPU)
- [x] Zero-copy video: VideoToolbox IOSurface → Metal textures → wgpu (8-bit 4:2:0; 4K H.264 38% → 16% CPU)
- [x] Zero-copy for 10-bit (P010) and full-range video
- [x] FFmpeg libraries ship inside the .app (Homebrew's GPL build, relinked; checked 2026-10-17: no Homebrew paths left). LGPL build: not needed (GPL app)
- [ ] Browser video: `<video>` → `copy_external_image_to_texture`
- [x] Live camera / capture card input, network streams (RTSP/SRT/HLS), Syphon In
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
- [x] Scrubbing (frame-exact seek, Inspector scrub bar)
- [x] Reverse, ping-pong, in/out points (drag on the scrub bar), beat-length loops; all mappable

### Phase 2: Performance (Resolume-style play)
- [x] Clips grid: layers × columns; click a cell, a column header, keys 1–9, or a MIDI/OSC-learned trigger
- [x] Drop media-bin items onto cells; store a layer's current source into a slot
- [x] Transitions per layer: crossfade / dip to black / wipe with duration; the outgoing clip keeps playing
- [x] BPM clock: tap tempo, resync, beat indicator, **Quantize** (triggers wait for the next beat),
      "Beat" modulation source for beat-synced parameters
- [x] Clip banks A–D (pages of 9 columns), bar-quantise
- [x] Detach any panel into its own OS window (tab menu → Detach; closing it docks it back)
- [x] MIDI clock in (tempo + beat after Start)
- [x] Blend transitions (Add, Screen, Subtract, Multiply, Lighten, Darken, Difference, Overlay)
- [x] Ableton Link ✔, effect-based transitions ✔ (Fade style "Through an effect", 2026-10-17)
- [x] Master crossfader (A/B groups)
- [x] Layer groups: mixed first, then opacity/blend/effects like one layer; fold, move as a block, slice source
- [x] Groups: controller targets (group opacity/on/bypass), opacity modulation, group strip in the Mixer, ⌘G / ⇧⌘G

### Phase 3: Modular control (VDMX-style)
- [x] Dockable, rearrangeable panels; layout saved with the project
- [x] **Control surfaces**: buttons, toggles, faders, sliders, XY pads, labels; each drives many targets with ranges
- [x] Control surface: multiple pages (Mac + phone, page switch mappable)
- [x] Control surface: colour pickers
- [x] Data sources: audio input analysis (level, low/mid/high, 32-band spectrum) + LFOs (sine/tri/ramp/square),
      random (S&H), 8-step sequencer, beat; **tempo sync** (1/4 beat … 8 bars), Add or **Sweep a range** (from/to),
      invert, live meter; a green tick on every modulated slider shows where it really is
- [x] `~` button on every slider and layer opacity: modulate by any data source with amount/rate
- [x] Step sequencers, beat detection / BPM-synced LFOs, envelope follower (attack + release on the audio levels)
- [x] Map any data source to any parameter (the ~ button on every slider)
- [x] MIDI in (all ports, CC + notes) and OSC in (UDP, any numeric arg) with **Learn**: pick a target
      (master fade, layer opacity/on-off, any generator/effect parameter, mix, on/off), press Learn, move a control
- [x] OSC port advertised over Zeroconf (`_osc._udp`) so TouchOSC lists "Phosphor"
- [x] OSC feedback (TouchOSC faders follow Phosphor) and MIDI out to the same devices (motor faders, LED rings)
- [x] OSC Query (`_oscjson._tcp`, namespace with values/ranges)
- [x] **TouchOSC integration** (export done; .tosc import still to do): OSC in/out with Zeroconf (`_osc._udp`), OSC Query, and a `.tosc`
      exporter (zlib XML, `<lexml version='4'>` node tree) that generates a TouchOSC layout from any
      Phosphor control surface; `.tosc` importer to map an existing layout
- [x] Right-click any control → Learn, or type a mapping (OSC address, MIDI CC/note, key); every control also
      answers a built-in OSC address with no setup (/phosphor/master, /phosphor/layer/<id>/opacity, /phosphor/fx/<id>/<n>…)
- [x] Keyboard mapping (Learn or type a key)
- [x] Presets per layer, effect chain and whole project (Presets panel)

### Phase 4: Projection mapping (MadMapper-easy)
- [x] Surfaces: corner-pinned quads with true perspective (homography), any number per output
- [x] Direct manipulation: drag corners/centre in the Mapping panel **or on the projector output** (E)
- [x] Input region per surface (slice of the master), per-surface opacity, soft edge for projector blending
- [x] Alignment grid test pattern with orientation marker
- [x] Choose the output screen: Output ▾ menu / Screens list → window moves there and goes fullscreen (no menu bar)
- [x] Multiple outputs (one window per projector), each with its own surfaces
- [x] Mesh warp (Catmull-Rom grid up to 8×8), polygon masks with invert
- [x] Mask feather (soft edges)
- [x] Circle and triangle masks (Mapping › + Mask)
- [x] Surface (slice) sources: each surface shows the mix or one layer; "Also in the mix" per layer
- [x] Outputs remember their screen (by name, numbered for identical projectors); "All to their screens"
- [x] Mapping: slice "input from another output"
- [x] Open outputs on their saved screens automatically when a project opens (per-project setting, off by default)
- [ ] Bezier mask edges
- [x] Per-output identify / colour correction, surface blend curve
- [x] Per-surface color correction / brightness
- [x] Save mapping with the project
- [x] Quick-recall mapping presets per venue (Mapping › Venues)

### Phase 5: Effects & interop
- [x] ISF import (`crates/phosphor-isf`): GLSL → WGSL via naga; 264 of 406 shaders on this Mac convert
      (233/327 of /Library/Graphics/ISF); converted in the background, compiled lazily (2 per frame)
- [x] ISF multi-pass / persistent buffers, up to 44 params (341/406 convert)
- [x] ISF imported images ✔, MadMapper includes ✔ (MadNoise, MadSDF, MadCommon), curve/audio inputs ✔; FLOAT buffers still 8-bit
- [~] v002 ports: Blurs ✔, Glitch ✔ (analog), Film Effects ✔ (Technicolor, Bleach, Vignette, Grain); still to do:
      Optical Flow and lit (normals) Rutt-Etra rendering. Rutt-Etra can now displace from another layer's picture
- [x] Multi-pass effects (blur, glow, feedback/trails)
- [x] Text / title source (any installed font, size, colour, alignment, outline)
- [x] Syphon in/out, NDI out
- [x] NDI in
- [x] Recording to ProRes / Hap (own Hap encoder: DXT1/DXT5 + Snappy, muxed in-process)
- [ ] Spout (Windows, if we ship there)

### Phase 5b: Streaming & recording (no OBS required)
- [x] Non-blocking GPU readback ring → FFmpeg pipe → VideoToolbox H.264 (never stalls rendering)
- [x] Stream out directly: YouTube Live, Twitch, custom RTMP, SRT — pick size/fps/bitrate, Start
- [x] Record to disk: .mp4 (H.264) or .mov (ProRes 422 HQ, VideoToolbox)
- [x] Audio in the stream: pick any AVFoundation input (mic, interface, BlackHole loopback) → AAC 160k
- [ ] HEVC option; RTMPS; WHIP (WebRTC)
- [x] Auto-reconnect for streams (2/4/8/15 s back-off)
- [ ] Multiple simultaneous destinations; connection health graph
- [x] OBS-friendly: Syphon and NDI outputs
- [ ] Virtual camera (CoreMediaIO extension; needs an Apple Developer ID to install)

### Phase 5c: Network players & remote audio (play *through* other machines)
- [x] **Session** (v1, LAN): built-in server; any browser (laptop, phone, Pi) opens the link and sees the
      output live as MJPEG (≤1280 px, 10–30 fps); Start/Stop + copy link in the Session panel
- [x] **Audio back-channel**: the player page's "Send my audio" streams the viewer's mic over a WebSocket
      into Phosphor's audio analysis ("Session viewers" source), so visuals react to *their* music
- [ ] Upgrade transport to WebRTC (WHEP) for sub-second latency and HTTPS/TLS so mic access works
      without the Chrome insecure-origin flag
- [ ] **Phosphor Player** for Raspberry Pi / small PCs: a headless build that either plays the
      stream or renders the project itself from a synced clock (tiny bandwidth, perfect sync)
- [x] Session page: QR code, viewer list (Devices panel)
- [ ] Per-viewer audio level meters
- [ ] Local network discovery (Zeroconf) for players; NDI/Syphon remain for pro setups
- [ ] Multiple players in sync (shared timecode), each with its own output mapping (Phase 4)

### Phase 6: Accounts & sync
- [ ] Project format: versioned JSON (layout + composition + mappings), already serde
- [x] Phosphor account service (phosphorfx.com): sign-in, Pro project storage, in-app Account panel
- [x] Layouts in the account (sync)
- [ ] Control surfaces in the account
- [ ] Sync projects, UI layouts, control surfaces and mappings across devices
- [ ] Media handling: references plus an optional cloud media library

### Phase 7: Platforms
- [x] macOS app bundle with icon (`scripts/bundle-mac.sh` → /Applications/Phosphor.app), opens .phosphor files
- [~] macOS (Apple Silicon + Intel), macOS 14+
- [x] **Browser build runs** (WebGPU, Chrome): `cd crates/phosphor && trunk build --release --dist ../../dist`,
      then serve `dist/`. Same engine + UI; 9 MB wasm. No video/audio/MIDI in the browser yet
- [ ] iPadOS / iOS (same engine + UI; touch-sized controls)
- [x] **Mac: File / Edit / View… only in the macOS menu bar** ✔ 2026-10-17 (in-window menus stay for the browser build)
- [ ] Older macOS (11–13): test wgpu Metal backend; likely only small changes needed

### Engineering
- [ ] Render on its own thread / own swapchain per output, decoupled from UI repaint
- [ ] GPU timestamp queries for real GPU frame time
- [x] Cache bind groups (`BindCache`): 40 previews ~95% → ~88% app CPU; render time unchanged (~2.1 ms)
- [x] Import VDMX projects (.vdmx5 XML plist: media-bin pages → banks + collections)
- [x] Import Resolume Arena compositions (.avc is XML) — layers, clips, decks, tempo, size; + Find Missing Media: layers, clips (file paths) into clip columns, decks → pages,
  opacity/blend/BPM; effects matched by name where we have one, others listed as "not imported".
  Advanced Output XML → output surfaces. Owner has real files in ~/Documents/Resolume Arena to test with.
- [x] 40-preview cost found: ~13 µs of wgpu bookkeeping per drawing step at `encoder.finish()` (not our code).
  Live previews now refresh in staggered groups (still 30/s each), so the cost is spread instead of a 2 ms spike.
      CPU/frame (30% → 105% of a core), nearly all pass/bind-group setup; explorer defaults to Hover previews
- [ ] Benchmark suite vs VDMX (CPU, memory, frame time, same clips)
- [x] In-app bug report (GitHub issue / clipboard), in-app update from GitHub releases (`scripts/release-github.sh` publishes); Donate in the top bar
- [ ] Crash reporting that sends on its own (opt-in); a private bug-report endpoint
- [ ] Code signing + notarization: scripts ready (`SIGN_ID=… scripts/bundle-mac.sh`, `scripts/notarize-mac.sh`); needs the owner's Developer ID certificate + notary login
