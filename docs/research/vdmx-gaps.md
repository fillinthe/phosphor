# VDMX features Phosphor does not have yet

Date: 2026-10-20. Method: read-only look at `/Applications/VDMX6 Plus.app` (resource names, Core Image
kernels, the tips-of-the-day file) plus Vidvox's online docs, compared against `docs/PARITY.md`, `ROADMAP.md`,
the project instructions and a code search of `crates/`. VDMX was not launched. Items marked (?) are uncertain: I could
not confirm the exact VDMX behaviour or whether Phosphor already has a hidden version of it.

## 1. Summary for the owner

Phosphor already matches or beats VDMX on the big things: layers, groups, mattes, feedback, modulators, control
surfaces, MIDI/OSC learn, cues, timecode, mapping, recording, Syphon/NDI. What is left are mostly
"nice to have" VDMX plugins that fill the gaps around the edges. The ones that would change a live show most:

1. Camera-based tracking (hands, faces, body) and "remove background", which VDMX does with Apple's Vision.
2. Capturing another app's window or a whole display as a layer.
3. LUT colour looks (.cube files) and a few "mask" and utility effects.
4. Gamepad / joystick input and a recorder for knob moves (Data Looper).
5. Small helpers VDMX has: a MIDI/OSC monitor, a scopes view, sticky notes, extra preview windows.

Nothing here is a blocker. Most items are small to medium.

## 2. Gap table

| VDMX feature | What it does | Phosphor | Effort | Value (live VJ) |
|---|---|---|---|---|
| Window Capture / Display Capture (Vid In, ScreenCaptureKit) | Any app window or a screen becomes a layer source | missing (cameras/streams/Syphon/NDI only; no screen source found in `media/live.rs`) | medium | high |
| Video Tracking plugin (human, face, hand tracking; human mask; attention / object saliency) | Body/hand/face positions become data sources, masks drive effects | missing | large | high |
| Remove Background FX (`VVCIRemoveBackgroundFilter`) | Cuts the person out of live camera | missing | medium (Apple Vision person segmentation) | high |
| 3D LUT FX, LUT Mixer, LUT Mask Mixer (`3DLUT*.fs`, user `.cube` files in Assets folder) | Colour grading looks, blend two LUTs | missing (colour grade exists for outputs only) | small to medium | medium |
| Data Looper plugin | Record knob/fader/XY moves into quantized tracks, edit warp points, loop back as a data source | missing (modulators and cues exist, no gesture recording) | medium | high |
| HID Input plugin (gamepads, joysticks) | Controller buttons/axes become data sources, calibration, detect mode | missing (no gamepad code) | small to medium | medium |
| Wiimote plugin | Wiimote / Nunchuk motion as data | missing, niche | small | low |
| Comm Display plugin | Live monitor of incoming MIDI/OSC/HID messages | missing as a panel (?) (only status/activity in `app/status.rs`) | small | medium (great for setup and debugging) |
| Scopes plugin (waveform, parade, vectorscope, point; can be a layer source) | Colour measurement of a video | missing | medium | low |
| OCR plugin (text and QR reading into string data) | Read text / QR codes from video | missing (QR generate exists, not read) | medium | low |
| Video to DMX plugin | Sample video pixels along a line/grid into DMX channels | partial (Phosphor's LED/DMX out samples points; no simple "line of pixels, snake, start channel" mode confirmed) (?) | small | medium |
| DMX input as control data | Lighting desk drives parameters | missing (DMX out only) | medium | medium |
| Step Sequencer extras: Index/Number/Boolean/Colour tracks, several tracks in sync, per-step skip, clock-synced length | Pattern sequencing of values and colours | partial (steps modulator exists; multi-track colour/index tracks (?)) | medium | medium |
| Preview plugin (many preview windows, throttled fps, click position as 2D data source, preview a layer/FX alone) | Extra monitors and mouse-to-data | partial (previews/thumbs exist, no extra detachable multi-preview or click-as-data) | medium | medium |
| Two Channel Mixer: Cut and Fade buttons, Auto-Fade, custom opacity curve | A/B fade with timed auto fade | partial (A/B crossfader exists; auto-fade and curve (?)) | small | medium |
| Media Bin voice modes (mono / cycle / polyphonic), "auto-eject on note-off / empty cell", trigger by index or float | How clips fire from MIDI and data | partial (trigger modes normal/toggle/flash/piano exist) | small | medium |
| Media Bin quantized trigger "on movie end / next / random on movie end", manual | Launch rules relative to the playing clip | partial (autopilot and beat quantize exist) | small | medium |
| Media preload option, hover-scrub style previews, thumbnail layouts | Faster launches, quick looks | partial (`Thumbs`, previews while hovered exist; preload not found) (?) | small | medium |
| Layer Composition: Size modes Fit/Fill/Stretch/Copy pixels, Position modes (Offscreen/Center/Onscreen), crop L/R/T/B | Per-layer sizing and crop | partial (fit/transform exist; per-layer crop and Copy-pixels mode not found) (?) | small | medium |
| Layer Quad mode (4 corner perspective warp per layer, corners driveable by data) | Corner-pin one layer | partial (mapping slices do this, layer-level quad not confirmed) (?) | medium | medium |
| Composition modes beyond blend (Source Atop, custom GLSL/Metal modes, per-effect composition mode) | Alpha-aware compositing, per-effect blend | partial (10 blends + mattes; per-effect blend menu and Source-Atop style mode missing (?)) | small | medium |
| FX set from VDMX Core Image kernels: Frame Hold, Strobe, Motion Blur, RGB Trails, Duotone (B&W, two colour), Channel Slide, Fade To Black, Add/Modulus, Luma key, Alpha mask, Un-premultiply, XY Zoom, Flip H/V, Crop | Handy stock effects | partial (34 built in + ISF; several of these are likely in the ISF import, not built in) (?) | small each | medium |
| Chroma Mask FX | Key out a colour | has (Chroma key effect) | n/a | n/a |
| Blur and Pixelate Faces FX, QR Code Overlay FX | Privacy and QR overlay | missing / partial (QR overlay quick action exists) | small once tracking exists | low |
| Number FX chains on every data receiver (smooth, snap, randomize, offset, wrap, threshold, multiply, modulus, min/max, delay, lookup curve, OCR... visible as nibs `Smooth`, `Snap`, `Randomize`, `OffsetWrap`, `LookupCurve`, `MinMax`, `Threshold`, `DelayNumFX`, `AverageWith`) | Shape a control value with a small chain | partial (ranges, invert, modulators; no general per-target chain with smooth/snap/delay/lookup curve) | medium | high |
| Math expression on receivers (`$VAL`, `$MIN`, `$MAX`) | Typed formula on any control | partial (`expr::eval` is for sizes only) | small | medium |
| Slider smoothing per control and global rate | Smooth jumpy MIDI | partial (?) | small | medium |
| Soft takeover for MIDI/OSC (sliders and buttons) | No value jump when a fader differs from the screen | missing (?) (only a hit in a bench file) | small | high |
| Echo Mode (state talks back to MIDI/OSC controllers) | Motor faders, lit pads | has (`ControlHub::feedback`) | n/a | n/a |
| Slider "marks" (snap points, jump to a mark by data) | Named points on a slider | missing | small | low |
| Send MIDI / OSC / DMX from any UI item | Any control also transmits | partial (OSC out on the Control Surface; every parameter sending MIDI/OSC/DMX not found) | medium | medium |
| Piecewise / custom curve LFO, LFO as random generator | Drawn waveforms | partial (?) (`NOTES.md` mentions a curve idea) | medium | medium |
| Local preset bars on every plugin and layer window; top-level workspace presets with fade in/out times, copy between projects | Per-panel snapshots, fading between whole-workspace states | partial (looks/layers/chains/venues, cues with fades; per-panel preset bar missing) | medium | medium |
| Project templates | Start from a saved starter project | partial (?) (venues and layouts exist) | small | low |
| Sticky plugin (notes in the layout) | On-screen notes with images | missing | small | low |
| Themes (colour themes for the UI) | Re-skin | partial (single theme found) | small | low |
| Window snapping / column dragging | Window handling | n/a (Phosphor uses docking) | n/a | low |
| Blackmagic input and output (Decklink) | SDI capture and output | missing (not planned in ROADMAP besides RAW codecs) | large (SDK) | medium |
| Audio Unit plugins (VVAUKit) as audio FX / Audio Analysis play-through | Process the audio | missing | large | low |
| Movie chapters / markers (`FIMovieChapters`) | Jump to chapters in a file | missing | small | low |
| Image sequences, web page and text file as layer source | Odd sources | missing / partial (text layers yes) | medium | low |
| Movie Recorder snapshots (still image of a layer or output) | One-click stills | partial (?) | small | low |
| Advanced Fullscreen Output CSV import/export | Bulk output configs | partial (saved screens/venues) | small | low |
| FreeFrame, Quartz Composer, Vuo, TouchDesigner plugins | Third-party plugin hosts | missing, not worth it (QC dead, Vuo is 482 MB) | large | low |
| Synopsis (visual content search of clips; `SynopsisIcon`) | Find clips by look/colour | missing | large | low (?) |

## 3. Recommended build order (top 10)

1. Soft takeover for MIDI/OSC faders - tiny, and every hardware user feels it immediately.
2. Window / Display capture as a layer source - high wow, medium work, reuses the live-source path.
3. Number FX chain on controls (smooth, snap, delay, lookup curve, `$VAL` expression) - VDMX's core power; reuses `ValueRange`/`Binding`.
4. Data Looper (record knob moves, loop, quantize) - big performance feature; builds on modulators and the clock.
5. Comm Display (MIDI/OSC/HID monitor panel) - small and removes guesswork when mapping controllers.
6. LUT effects (.cube, mixer between two LUTs) - one new shader plus a file picker; looks great.
7. Gamepad/HID input - small, plugs into the existing mapping system as a new source.
8. Video Tracking + Remove Background (Apple Vision, macOS only) - large but the most striking VDMX-only feature; hand/body data feeds modulation.
9. Media bin voice modes and "on movie end" launch rules, plus layer crop and Copy/Fill size modes - cheap polish for clip playing.
10. Multi-track Step Sequencer (index, colour, boolean tracks) and extra preview windows with click-as-data - fills out the data-source side.

Not recommended: QC/Vuo/TD/FreeFrame hosts, Wiimote, Synopsis, Blackmagic (SDK needed) until asked for.

## 4. Sources

Bundle (`/Applications/VDMX6 Plus.app/Contents/`):
- `Resources/VDMX6_tips_of_the_day.txt` (feature list: Video Tracking, OCR, Scopes, Data Looper, HID, Window Capture, LUT FX, Remove Background, Echo Mode, soft takeover, marks, Preview as data source, Sticky, Comm Display, Video to DMX).
- `Resources/*.nib` names: `DataLooper`, `HIDInputOptionsInspector`, `VideoTracking`, `VideoToDMX`, `OCR`, `Scopes`, `CommDisplay`, `StickyPlugin`, `TwoChannelMixer`, `StepSequencer`, `WiiMote`, `WindowCaptureViewController`, `DisplayCaptureViewController`, `BlackMagicOutput`, `LookupCurve`, `Smooth`, `Snap`, `Randomize`, `DelayNumFX`, `AverageWith`, `MinMax`, `Modulus`, `OffsetWrap`.
- `Resources/3DLUT.fs`, `3DLUTMixer.fs`, `3DLUTMaskMixer.fs`; `Frameworks/CustomFilters.bundle/Contents/Resources/*.cikernel` (FrameHold, Strobe, MotionBlur, RGB_Trails, ChromaMask, VVCIRemoveBackgroundFilter, VVCIQRCodeGeneratorResize...).
- `Frameworks/` names: VVAUKit, VVBlackmagicKit, WiimoteKit-Pro, VVVuoKit, VVTDKit, VVLightingKit.

Docs (fetched 2026-10-20):
- https://docs.vidvox.net/vdmx_plugins.html (plugin list and options)
- https://docs.vidvox.net/vdmx_rendering.html (layer composition, sizing/position modes, crop, quad)
- https://docs.vidvox.net/vdmx_data_io.html (receivers, `$VAL` expressions, sending MIDI/OSC/DMX)
- Pages linked from the tips file: vdmx_ui_controls, vdmx_assets, vdmx_templates (not read).

Phosphor side: `docs/PARITY.md`, `docs/VDMX-REVIEW.md`, the project instructions, code searches in `crates/phosphor-engine/src` and `crates/phosphor/src` (EFFECTS list in `shaders.rs:33`, no hits for LUT, gamepad/HID, screen capture, OCR, scopes, tracking).
