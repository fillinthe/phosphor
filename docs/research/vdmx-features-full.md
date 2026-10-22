# VDMX6 / VDMX6 Plus: full feature list

Researched 2026-10-22 against **VDMX6 1.3.8** (current download on vidvox.net; "VDMX 6.3" in the blog).
Sources, most trusted first: the VDMX user manual at docs.vidvox.net/vdmx/, the vidvox.net product page, the
VDMX6 release notes (version.vidvox.net), and the Vidvox blog/tutorials (vdmx.vidvox.net).

Where the manual (written partly for VDMX5) disagrees with VDMX6, the newer source wins and the item is
marked. Items marked **(app bundle)** were read from the installed `VDMX6 Plus.app` 1.3.8 and its
`~/Library/Application Support/VDMX` assets folder (file names, built-in template names, the app's own
"tips of the day"). They are real files, but no official page explains them. Items marked **(unverified)** were
inferred and do not appear in any official page.

**VDMX6 vs VDMX6 Plus:** the two editions are identical except that Plus adds TouchDesigner and Vuo support
($199 vs $349). Requires macOS 13+ on Intel or Apple Silicon (1.1.8 was the last build for macOS 12).

---

## 1. Architecture and workspace

Source: https://docs.vidvox.net/vdmx/vdmx_inspectors.html , https://docs.vidvox.net/vdmx/vdmx_rendering.html

- **Modular "build your own instrument" design:** you assemble layers, plugins and windows into your own performance interface.
- **Workspace Inspector:** one floating window with tabs for Files, Vid In, Assets, Presets, Layers and Plugins, where you create and manage everything high-level.
- **UI Inspector:** a context-sensitive floating window that shows settings for whatever slider, button, receiver, sender or preset you last clicked, and grows deeper as you drill into sub-items.
- **Inspectors designed to be closed:** both inspectors can be hidden in performance so the screen space goes to your custom layout.
- **Open any component in its own window:** layer source, FX chain, composition controls and plugins can all be moved out of the inspector into separate windows.
- **Tabbed windows:** any non-inspector window can hold many tabs; you can drag tabs out to make new windows and rearrange them.
- **Tabs triggered by MIDI/OSC/DMX/data sources:** each window tab has its own on/off receiver, so a controller can switch visible pages.
- **Magnetic window snapping:** windows snap to neighbours when dragged or resized.
- **Move groups of windows:** Option-drag moves every snapped window together; Cmd-drag moves the whole column.
- **Magnifying-glass button:** jumps from a window to that item in the Workspace Inspector.
- **Keep windows on main screen:** an option, on by default, that stops windows being dragged off the menu-bar display.
- **Lock Windows:** freezes the whole layout (no close/resize handles, no new windows).
- **Close hidden windows / Close windows not on main screen:** menu commands. (app bundle)
- **Per-item clipboards:** every kind of copyable thing (slider, button, sender, preset, layer, plugin) has its own clipboard, so copying one kind doesn't overwrite another.
- **Copy / Paste / Paste Local for UI items:** copy a control's full state (receivers, senders, presets) onto another; "Paste local" keeps the target's own presets.
- **Copy/paste layers, plugins and presets across projects.**
- **Undo/redo in the Workspace Inspector:** covers actions in the Layers, Plugins and Presets tabs, and can roll back preset triggers.
- **Data Source Mappings window:** a window listing data-source assignments, with "Inspect" links. (app bundle; mentioned in release notes)
- **Starting Point window on launch:** optional. Opening a new project from a template is also in the File menu.
- **Search field in the layer source and FX menus:** filters long menus as you type (new in VDMX6).
- **Pop-over "eye" previews:** click an eye icon anywhere in the processing chain to get a temporary live preview. FX get before/after side-by-side views, and "Open in a Preview" turns one into a permanent Preview plugin (6.2).
- **Live layer thumbnails in the Layers list:** RGB or RGB+Alpha, with adjustable size; can be turned off in Preferences (6.2).
- **Tips of the day / Starting Point window.** (app bundle)
- **Built-in bug reporter:** Help › Report Bug, with an option to attach the project. (app bundle + blog)
- **System Permissions status pane:** shows whether VDMX has camera, microphone, screen capture, disk, network and other permissions. Help › Check App Permissions (6.3).

## 2. Rendering engine, canvas, layers and groups

Source: https://docs.vidvox.net/vdmx/vdmx_rendering.html , https://vidvox.net (Tech Specs), https://vdmx.vidvox.net/blog/vdmx6

- **Metal rendering engine:** the whole pipeline moved from OpenGL to Metal in VDMX6.
- **Unlimited layers:** "work with any number of layers at a time".
- **One source per layer:** each layer plays one source at a time (movie, generator, live input and so on), followed by its FX chain, then composition.
- **Canvas / Main Output:** the top-level group with a set resolution, frame rate and its own FX chain; it can't be deleted or moved.
- **Default canvas resolution and FPS:** set in Preferences for new projects.
- **Master Fader:** fade to black at the top of the Workspace Inspector.
- **Layer list:** add, delete, rename and drag to reorder layers; drag them into and out of groups.
- **Layer groups:** several layers flatten into one picture that has its own FX chain, opacity, blend mode, size and position.
- **Per-group resolution and frame rate:** a group can render at its own pixel size and FPS.
- **Nested groups:** hierarchical compositing (groups inside groups). This follows from the "hierarchical" model the docs describe.
- **Group Selected Layers / Copy layers / Paste to new layers / Duplicate layer:** in the sprocket menu.
- **Per-layer render FPS:** each layer has an FPS field when multi-threaded rendering is on.
- **Resizable generator layers:** resolution-independent sources (ISF, CoreImage, QC and so on) have editable Size fields.
- **Eject:** clears a layer's source.
- **Pre-FX and post-FX taps:** any layer or group, including the canvas, can be used as a video source before or after its effects.
- **Video feedback:** a layer can use the Canvas/Main Output, or another layer, as its source (feedback loops).
- **Skip Canvas Render:** a speed option that draws top-level layers straight to the output. You lose the canvas preview, canvas FX, Syphon of the canvas and some blend modes.
- **Display Drives Canvas Rendering:** the canvas redraws at the display refresh rate; turn it off to set a fixed FPS on its own thread.
- **VBL sync, double-buffered output and output throttling modes** (Every refresh / DB skip-if-busy / SB skip-if-busy).
- **Render GPU selection:** pick the GPU in Rendering preferences. Multiple GPUs and eGPUs are supported.
- **Alpha-channel pipeline:** alpha is kept through layers, FX, recording and outputs.
- **QC antialiasing setting.** (legacy QC)
- **UI throttling:** caps how often UI controls redraw (Misc prefs) so the UI competes less with video.

## 3. Layer composition (blend, opacity, position, crop, warp)

Source: https://docs.vidvox.net/vdmx/vdmx_rendering.html ; comp-mode names from the assets folder (app bundle)

- **Layer Opacity slider.**
- **Show/Hide toggle:** separate from opacity.
- **Composition (blend) mode menu:** per layer and group.
- **Metal (MSL) composition modes:** about 54 ship with VDMX6, including Addition, Average, Color Burn/Dodge, Darken/Lighten, Difference, Divide, Exclusion, Hard/Soft/Linear/Vivid/Pin Light, Hard Mix, Hue/Saturation/Color/Luminosity, Multiply, Screen, Overlay, Subtract, Negation, Reflect, Glow, Phoenix, Maximum/Minimum, Mix and SourceAtop. (names: app bundle)
- **Transition-style comp modes:** Fade Center/Diagonal/Horizontal/Radial/Squares/Vertical, Wipe Heart/Horizontal/Star/Vertical, Blocks, Cross Hatch and Dissolve, all animated by the opacity slider. (names: app bundle)
- **CoreImage composite filters as blend modes.**
- **Custom composition modes:** install your own Metal shaders in `/mslCompModes`. VDMX5's GLSL comp modes must be converted.
- **QC composition modes:** top image + bottom image + opacity, useful for custom transitions. (deprecated)
- **Comp-mode parameter tab:** controls for CI/QC comp modes that have inputs.
- **Crop tab:** crop left, right, top and bottom of the source before placement.
- **Rect mode position:** X/Y sliders in pixels, with centre marks.
- **Position Mode:** Offscreen, Onscreen or Center sets the range of the X/Y sliders.
- **Width/Height multipliers:** marks at 1×.
- **Size Mode:** Fit, Fill, Stretch or Copy (1:1 pixels, no filtering).
- **Quad / perspective (corner-pin) mode:** drag four corners to keystone onto angled surfaces.
- **Quad corners are controllable:** type exact coordinates, nudge with arrow keys, or map MIDI/OSC/DMX/data sources to each corner's X/Y.
- **Quad view zoom and Reset.**
- **Layer Position FX / Rotate / 3D Rotate:** move and rotate through FX ("Geometry adjustment and rotation").

## 4. Effects (FX chains)

Source: https://docs.vidvox.net/vdmx/vdmx_rendering.html#layer-fx-chain , https://docs.vidvox.net/vdmx/vdmx_assets.html ; FX names from `/Library/Graphics/ISF` (app bundle)

### FX chain mechanics
- **FX chain per layer, per group and on the Canvas:** filters run in order.
- **Effect formats:** ISF (GLSL, converted to Metal), CoreImage, Vuo (Plus), TouchDesigner (Plus) and Quartz Composer (deprecated). **FFGL is not supported in VDMX6** (VDMX5 only).
- **Per-effect On/Off bypass:** a bypassed effect does no processing.
- **Per-effect Wet/Dry slider:** at 0 the effect is skipped entirely.
- **Per-effect blend mode:** the wet/dry mix can use any composition mode.
- **Load Asset menu:** load a saved effect or chain, with a **Replace / Add** toggle (default set in preferences).
- **Save Asset:** save the current chain as a reusable asset for all projects.
- **Section presets on the FX chain:** quick save and recall of the chain's contents.
- **Preview FX button:** opens the asset browser with this layer as the preview source; you can tweak, then drag the effect into the chain.
- **Drag effects between layers.**
- **Mouse interaction with ISF/QC FX:** a Preview window can pass mouse clicks to the FX in the previewed layer.
- **Audio-reactive ISF:** ISF effects and generators can take audio waveform and FFT images straight from any audio device (6.2), or from Audio Analysis.
- **ISF hot reload:** edited ISF files reload automatically. Compile errors show a clickable warning triangle and are logged to `ISFBinCache/ErrorLogs`.
- **Automatic GLSL→Metal conversion:** every installed ISF file is converted when VDMX launches.

### Built-in effect categories (bundled ISF, about 330 .fs files in total)
- **Blur (20):** Gaussian, motion, diagonal, edge, bloom, gloom, median, dilate/erode, frosted glass and more.
- **Color Adjustment (13):** Color Controls (hue/sat/brightness/contrast), Levels, LGG, RGB EQ, Exposure, Gamma, Vibrance, White Point, Set/Apply Alpha and more.
- **Color Effect (39):** posterize, invert, monochrome, duotone/tritone, false color, solarize, strobe, RGB trails, hue shift, thermal camera, motion heat map and more.
- **Distortion (24):** bump, twirl, ripples, shockwave, displace, noise displace, optical-flow distort, sphere map, trapezoid, CRT displacement, analog glitch and more.
- **Feedback (9):** zooming feedback, echo trace, comet tails, long exposure, FastMosh, sorting smear, feedback masks.
- **Film (10):** lens flare, dirty lens, light leak, bleach bypass, Technicolor, vignette.
- **Geometry Adjustment (20):** 3D rotate, rotate, zoom, XY zoom, flip H/V, mirror, crop and feather, quad mask, shake, scrollers, vertex manipulator, layer position.
- **Glitch (26):** Bad TV, VHS, JPEG block corruption, keyframe artifacts, interlace, slit scan, random freeze, time glitch, pattern/stylize glitch, vertical tearing.
- **Halftone (9):** CMYK/RGB halftone, dot/line/circular screen, Bayer dither, crosshatch.
- **Kaleidoscope (4)** and **Tile (11):** kaleidoscope, radial replicate, quad tile, replicate, video bricks, MultiFrame 2×2/3×3, video snake.
- **Masking (11):** Layer Mask, Chroma Mask (chroma key), Chroma Desaturation Mask, Motion Mask, Shape Mask, Quad Mask, Slit Scan Mask, Random Squares Mask, Apply Alpha.
- **Noise (4), Sharpen (3), Retro (19), Stylize (39):** ASCII art, edges, emboss, glow, god rays, neon, night vision, toon, sketch, pixellate, triangles, triangle mosaic and more.
- **Overlay (4):** Cursor, Doodler (draw on video), Highlighter, Tiny Date/Time overlays (new in VDMX6).
- **Utility (23):** Freeze Frame, Deinterlace, Show Alpha, RGB↔HSV, Optical Flow Generator and more.
- **Transitions as ISF:** about 75 two-input transition shaders (dissolve, wipe, distortion, retro, glitch). (app bundle; names only, so how they're used in VDMX is unverified)
- **Luma key / chroma key compositing:** ships with templates (Luma Key Example, Chroma Key Example).

### Vision / CoreML-powered FX (VDMX6)
- **Remove Background:** person segmentation removes the background (or, inverted, the foreground) without a green screen.
- **Blur Faces / Pixellate Faces:** obscures detected faces; can be reversed to blur the background instead.
- **Face Overlay:** draws a second image over detected faces.
- **Segmented Blur:** different blur amounts for foreground and background (macOS 14+).
- **Color Transfer:** matches a layer's colour and brightness to a reference image or layer, with chroma and luma amounts.
- **Segmented Color Transfer:** separate references for foreground and background (macOS 14+).
- **kMeans Palettize:** reduces the picture to its dominant colours, found by k-means.
- **QR Code Overlay FX:** stamps a QR code over a layer.

### LUTs
- **LUT FX:** applies a `.cube` 3D LUT; custom LUTs go in the Assets/LUTs folder.
- **LUT Mixer:** crossfades between two LUTs.
- **LUT Mask Mixer:** mixes two LUTs per pixel using a mask image.
- **Bundled "MovieLook" LUTs:** Action, Adventure, Drama, Dystopia, Indie, War, Western. (app bundle)

### Other built-ins
- **VV Rutt Etra:** built-in scan-line effect (1.0.7).
- **v002 effects:** Glitch Analog, CRT, bleach bypass and others.
- **CoreImage system filters:** usable as FX (convolution filters that don't work are hidden).

## 5. Sources (what a layer can play)

Source: https://docs.vidvox.net/vdmx/vdmx_media_types.html , https://docs.vidvox.net/vdmx/vdmx_supported_features.html , https://vidvox.net

### Use Source menu
- **"Use Source" menu on every layer:** one list of every generator, live input, Syphon/NDI feed, window or screen, and every layer or group (pre/post FX). It is searchable.
- **Add built-in sources to Media Bin pages:** sources become triggerable clips (right-click › Add Clips From Built-In Sources).

### Movies
- **AVFoundation movie playback:** H.264, H.265/HEVC, ProRes (with alpha), Photo-JPEG.
- **HAP family on the GPU:** HAP, HAP Alpha, HAP Q, HAP Q Alpha and **HAP R (HAP 7A, BC7)**, which gives better-than-Q quality plus alpha at about the same size (new in VDMX6).
- **Alpha channels in media:** PNG, HAP Alpha/Q Alpha/R, ProRes 4444.
- **Time slider with in/out handles:** scrub and set loop points.
- **Set / Reset buttons for in and out points.**
- **Chapter marks:** jump to chapters in a movie (named in the tutorial; FIMovieChapters is in the app bundle).
- **Pause toggle:** pause without losing the rate.
- **Rate slider:** speed and direction.
- **Reverse toggle:** flips direction, keeps speed.
- **Tempo-quantized playback ("Q"):** stretches the clip to N measures of a chosen Clock, so it follows BPM changes.
- **Sync button:** snaps the movie position to the clock's measure position.
- **Back/Fwd jump buttons:** jump size set in seconds, frames or beats.
- **Scratch slider:** relative scrub with endless mouse drag.
- **Loop behaviour:** cut to black, hold last frame, loop, or ping-pong.
- **Movie audio:** volume, mute, choice of output device and stereo channel pair per layer.
- **Resume from last playback time / sync several movies:** tutorial topics. (unverified as distinct settings)

### Still images
- **Still images and GIFs:** direct playback.
- **Max image size auto-downscale:** oversized images are resized on the fly.
- **Image player compositions:** run images through a patch such as Image Zoomer or Sphere Map before FX (QC-based).

### Generators
- **ISF generators:** about 70 bundled. They cover Audio Visualizer (FFT spectrogram, waveform shapes, VU meter), Color/Pattern (gradients, checkerboards, stripes, solid colour), Geometry (shapes, stars, spirals, lines, truchet), Noise (Simplex, Worley, Ridgelines, Life), Drawing (Doodler, Etch-a-Sketch, Circle Trails), Test Pattern (Color Bars, test grid) and Utility (Digital Clock, Histogram Viewer).
- **Interactive ISF:** mouse clicks in a Preview window can be passed to an ISF generator.
- **CoreImage generators:** system CI generators, including the QR Code Generator.
- **QR Code Generator source.**
- **Auto-built parameter UI:** a generator's inputs become controls automatically: number→slider, bool→button, index→pop-up, colour→colour picker, point→2D pad, image→video-source pop-up.
- **Custom layout for a generator's UI:** an "Inspect" button lets you rearrange the controls.
- **Section presets for generator settings.**
- **Generator resolution and FPS:** per clip, or default to canvas size, layer size or a fixed size.

### Text
- **Text-file layers:** a text file is broken up by character, word, sentence or line and shown chunk by chunk.
- **Prev/Next chunk buttons and loop behaviour.**
- **Built-in text editor:** double-click a chunk to queue it next.
- **Text styles (renderers):** Beat Text, Block Text, Fade Text and Scrolling Text (QC-based; QC must be enabled). Plus users can write renderers in Vuo or TouchDesigner. (style names: app bundle)
- **Text generator receives strings:** its text view can take string data sources or OSC, for example from OCR or QR codes.

### Live inputs
- **AVFoundation cameras and capture cards:** any number of them.
- **iPhone / Apple TV as a wireless camera:** Continuity devices appear under Vid In (VDMX6).
- **Blackmagic capture:** native low-latency video and audio, auto format detect, "continuous detect" hot-swap, input port choice (SDI/HDMI…), FPS smoothing, full duplex, and keys used as alpha.
- **Syphon In:** any number of feeds, with no setup.
- **NDI In:** any number of streams; RGB (high-quality) receive option; audio play-through with volume and mute.
- **Window capture:** any titled window from another app, even when covered, with crop, FPS and a size limit.
- **Display capture:** a whole screen as a source, with resolution and FPS.
- **Per-input recording:** record or snapshot straight from a digitizer to disk, optionally adding to a page; device codec or transcode.
- **Per-input resolution and FPS requests, plus an audio source, play-through, volume and mute.**
- **Inputs are off until used:** enabled automatically when picked.

### Node-based sources
- **Quartz Composer compositions** as generators, FX, comp modes, text/image renderers and plugins. Deprecated, off by default, and can be turned back on in Rendering prefs.
- **Vuo compositions (Plus):** generators, FX, text renderers and plugins; run in-process or out-of-process; special inputs (VDMXBPM, MeasurePosition, FontMenu).
- **TouchDesigner .tox (Plus):** generators, FX, text renderers and plugins. Needs a paid TD licence on the same Mac.

### Removed in VDMX6
- **Web page source:** "The built in web source has been removed" in VDMX6, although the manual still describes it.
- **FreeFrame/FFGL and Flash:** removed in VDMX6. The manual still mentions them in places.

## 6. Media Bin and clip triggering

Source: https://docs.vidvox.net/vdmx/vdmx_plugins.html#media-bin , https://docs.vidvox.net/vdmx/vdmx_media_management.html

- **Pages of clips:** any number of pages, each holding any number of clips; there's a page list (table of contents) and a page menu.
- **Multiple Media Bins:** each has its own target layer and settings ("New Media Bin" in the menu).
- **Add, rename, reorder and delete pages; drag files or folders in to make pages.**
- **Search/filter a page:** by name or metadata.
- **Sort clips:** by name or path.
- **Click to trigger:** on the bin's target layer.
- **Trigger by keyboard/MIDI/OSC/DMX/data source:** per-cell receivers, with "detect" learning.
- **Detect range:** map a whole row of MIDI notes or OSC addresses by sending only the first and last.
- **Transpose up/down/by index:** shifts the trigger mapping across the page, like banking.
- **Prev / Next / Random clip buttons.**
- **Trigger by index or by float:** an integer or 0–1 value selects a clip.
- **Voice modes:** Monophonic (one target), Cycle (each trigger moves to the next layer) or Polyphonic (next free layer).
- **Auto-eject on Note Off:** clip plays only while the key is held.
- **Empty-cell eject:** triggering an empty cell clears the layer.
- **Quantized triggering:** Immediately, Movie End, Next On Movie End, Random On Movie End, or Manual (a queue fired by a button or data source).
- **Target layer menu:** which layers it lists is configurable.
- **Echo triggers out:** send the clip index over MIDI (0–127), DMX (0–255) or OSC (float, int or clip-address string).
- **Hardware device sync (LED feedback):** sends each cell's state (empty, available, queued, playing, playing on target) back to a grid controller, with per-state colour/velocity messages.
- **Preload media:** clips prepare ahead of time for instant triggering, with a max-preloaded-per-page preference.
- **Thumbnail modes:** icons, names, or both; aspect ratio; auto or fixed column count; show/hide shortcut labels.
- **Media bin themes:** cell background colours for loaded cells.
- **Missing-media icon** on cells whose file can't be found.

### Clips and pages
- **Per-clip overrides:** a clip can carry its own source settings (rate, volume, in/out, FPS; generator inputs and resolution), its own FX chain and its own composition settings (opacity, blend, size/position modes, X/Y, W/H).
- **Override value vs. full control settings:** per attribute.
- **Clip preview and Set Icon:** grab the current frame as the clip's thumbnail.
- **Lock State pref:** new clips can then only be edited in the File Inspector.
- **Move / duplicate (Option-drag) / delete clips.**
- **Export clips or pages:** as a file collection with settings, which can be re-imported.
- **Watch-folder pages:** a page stays in sync with a folder on disk, with a file-type filter and optional auto-delete.
- **Import Media (⌘I) and "load files from disk."**
- **Built-in file browser:** local drives, favourites, recents, built-in sources and Spotlight search.
- **Relink Missing Media:** manual Find/Browse/Skip, Find All, auto-resolve sibling files.
- **Fast Relinking:** background Spotlight search, best match chosen from stored metadata.
- **Clear thumbnail cache:** in Files prefs.

## 7. UI items (sliders, buttons, menus, presets)

Source: https://docs.vidvox.net/vdmx/vdmx_ui_controls.html

- **Every control is mappable:** sliders, buttons, pop-ups and preset bars all receive and send MIDI/OSC/DMX/data sources.
- **Shift-click to inspect a control without changing it.**
- **Right-click a control** for detect and data-source menus.
- **Slider min/max envelopes:** limit a slider's usable range by Option/Cmd-dragging or typing.
- **Lock slider UI:** ignores mouse.
- **Linear or logarithmic sliders.**
- **Multiple receivers per control.**
- **Receiver-enabled colour cue:** controls change colour when they're mapped.
- **"Ignore received values if not visible":** multiplexes one hardware knob across tabs.
- **Multiple senders per control.**
- **Slider marks:** snap points; each mark can be triggered by a data source, and marks can be dragged to new positions.
- **Per-control local presets:** each preset is triggerable and can be scoped (for example, recall only the receivers, which is how multiplexing works).
- **Slider smoothing:** per slider, or a global smoothing value.
- **Toggle vs. momentary buttons.**
- **Pop-up buttons with per-item receivers:** each menu item can be triggered directly.
- **Pop-up navigation receivers:** next/prev, random, by normalized value, by index, or by name string.
- **Section Preset bars:** save and recall the state of one area (a plugin or FX chain), with an R/W toggle, paging, and navigation receivers (next/prev/random/by name).
- **Soft takeover:** Off, Immediate, Pickup (with tolerance) or Fade (with speed), set separately for MIDI/OSC and internal sources, and separately for sliders and buttons.
- **Hardware Learn Mode (⌘L):** every mappable control turns blue; click one and wiggle a hardware control.
- **Single-shot Detect (⌘D).**
- **Ctrl/right-click-drag from plugins onto controls:** creates the mapping. Works from Clock, Control Surface, Cue List, Data Looper, LFO and Step Sequencer.
- **Echo mode:** controls send their value back to the MIDI/OSC device they're mapped to (motor faders, LED grids, TouchOSC), with custom note velocity for LED colours.
- **Colour picker / colour wheel controls:** RGBA/HSVA.
- **Clicking a preview window samples its colour into colour pickers.**
- **Preview windows publish click position as a 2D point data source.** (app bundle tips)

## 8. Control data: receivers, senders, number FX

Source: https://docs.vidvox.net/vdmx/vdmx_data_io.html , https://docs.vidvox.net/vdmx/vdmx_assets.html#assets-number-fx-chains

- **Four receiver types:** boolean, float, string and colour.
- **Bool receivers:** threshold; toggle on rise and/or fall, or value-from-input with invert; ignore repeats; pick an item from an OSC list.
- **Float receivers:** scale into the slider's envelope, invert, skip duplicates, ignore small changes (dead-band), and normalized vs. raw values.
- **Endless-encoder support:** increment/decrement MIDI/OSC messages with a step size.
- **"Do Math" expressions on receivers:** DDMathParser with `$VAL`, `$MIN` and `$MAX`.
- **Number FX chains on receivers:** chain processors that reshape incoming values; they're saved and reused as assets.
- **Number FX modules:** Average With, Delay, Fall, Min/Max, Modulus, Multiply, Offset, Offset Wrap, Randomize, Smooth, Smooth Wrap, Snap, Threshold and Lookup Curve. (names: app bundle)
- **MIDI senders:** note (threshold on/off; fixed or value velocity), note range (with mutex note-off) or CC, on any channel.
- **OSC senders:** bool, int (range), float (normalized or raw), string or colour.
- **DMX senders:** number (0–255, with range and invert) or colour (RGB/CMY/RGBW) from a start channel.
- **RGBW colour output:** Accurate, Brightest, No White or Use Alpha as White conversion, plus a global default (1.2.9).
- **Dedicated Controls:** MIDI or OSC sources that act only as data sources and never trigger Detect.

## 9. MIDI, OSC, OSCQuery, DMX, HID and other protocols

Source: https://docs.vidvox.net/vdmx/vdmx_preferences.html , https://docs.vidvox.net/vdmx/vdmx_oscquery.html , https://docs.vidvox.net/vdmx/vdmx_plugins.html

### MIDI
- **CoreMIDI input from all devices:** each can be enabled or disabled.
- **MIDI output:** to any device or IAC bus.
- **Natural note names display option.**
- **Default MIDI clock source preference.**
- **MTC received directly by sliders** (no plugin needed).

### OSC
- **Multiple OSC input ports:** default 1234, advertised over Bonjour.
- **Multiple OSC outputs:** Bonjour auto-discovery or manual.
- **Regex wildcard OSC address matching:** optional.
- **OSC list-item extraction:** in receivers.

### OSCQuery
- **OSCQuery server:** Control Surfaces publish their controls (per surface, with per-control publish settings).
- **Built-in web controller:** every Control Surface becomes a phone/tablet/browser remote at `http://ip:port/index.html?HTML`, two-way ("Listen for OSC"), with dark/light themes.
- **OSCQuery Client plugin:** browse another app's parameters and build remote controls for them automatically, two-way where supported.
- **OSCQuery Browser window (⌘5):** browse and search remote namespaces, send test values, and drag endpoints onto controls to set up OSC senders.
- **Companion apps:** OSCQuery Browser, OSCQuery Helper (JSON namespace for apps with plain OSC) and MIDI OSCQuery Helper (bridges to MIDI, parses Ableton .als sets), in Extras.

### DMX / Art-Net
- **Art-Net in and out:** choose the network interface, set the number of input and output universes (subnet/universe each), and use a network browser with auto-setup.
- **"Media server and lighting console":** any control can send or receive DMX.
- **DMX Fixtures plugin / Lighting prefs pane:** nib files exist in the bundle. (unverified; not documented)

### HID / game controllers
- **HID Input plugin:** gamepads and joysticks publish every control as a data source, with per-control calibration; buttons work with Detect.
- **Wiimote plugin:** Wiimote with Nunchuk or Classic controller, with pitch/yaw/roll tuning.
- **PS3 controller template.** (app bundle)

### Sync
- **Ableton Link:** tempo and phase sync in the Clock plugin.
- **MIDI clock in** (follow) **and MIDI clock out**, including Start/Stop mode and extra start/stop messages on downbeat for Eurorack and hardware sync.
- **MTC and LTC in/out** (Timecode plugin).

## 10. Plugins

Source: https://docs.vidvox.net/vdmx/vdmx_plugins.html

General:
- **Any number of instances of every plugin:** each with its own settings and section presets.
- **Create/delete/rename/copy/paste/duplicate plugins:** in the Plugins tab.
- **Plugin Templates:** save any plugin's state and load it into new or existing plugins, with categories, thumbnails and descriptions in the Templates Manager. Stored as files you can share (6.3). Ships with example templates (Math, Colors and Routing expressions; MTC/LTC follow/send; Ableton Link; Drum Kit audio filters; HEVC/ProRes recorders…).

### Audio Analysis
- **Analyses any audio input or channel (or a sum of channels)** with input gain.
- **Any number of named frequency-band filters:** drag to set centre and width; each band is a data source.
- **Per-filter receivers** for centre frequency, range, gain and smoothing.
- **Audio play-through to an output** (also usable as a plain audio router).
- **On/off to save CPU; section presets for filter sets.**
- **Raw audio and FFT for ISF visualizers.**
- **Capture audio directly from other apps:** Music, Spotify, browsers, DAWs, with no BlackHole/Soundflower needed (6.2).
- **"All System Audio" source.** (plugin template name, app bundle)

### Clock
- **Internal BPM:** type it, tap tempo, ×2, ÷2, nudge up/down.
- **MIDI Follow mode.**
- **Automatic BPM detection from audio:** a new tracker running natively on Apple Silicon, with source, gain and silence threshold.
- **Skip to next downbeat / restart measure.**
- **Beats per measure and subdivisions;** display as BPM or seconds.
- **Publishes beat, downbeat and multi-bar data sources** and the measure position.
- **Pause/resume or start/stop behaviour.**
- **Ableton Link and MIDI Clock Out.**
- **Used by other plugins for quantization** (movies, media bins, LFO, step sequencer, data looper, recorder).

### LFO
- **Many waveforms per LFO:** each one is a data source.
- **Function generators:** sine, cosine, triangle, ramp and random (also usable as a random-number source).
- **Custom bezier-curve waveforms:** edit nodes and handles in a curve editor.
- **Piecewise waveforms:** mix different function types within one wave.
- **Record a wave from a data source:** record mode draws a new bezier from incoming data.
- **Length in seconds, or in measures when synced to a Clock.**
- **Transport controls:** rate, scrub time, pause, loop mode, reverse, jump.
- **Lock or hide individual waves; grid display options.**
- **Templates:** multiphase sines/triangles/ramps, random numbers, two-point bezier.

### Step Sequencer
- **Multiple tracks in sync:** index, number, boolean or colour values.
- **Skip steps** (Cmd-click).
- **Overview mode:** all tracks stacked.
- **Duration in seconds or clock measures;** set the number of columns (steps).
- **Per-step jump receivers;** per-column value receivers, which turn it into a data router or switcher.
- **Transport controls:** time slider, pause, scratch, jump, loop/once/ping-pong, rate, reverse.
- **Interpolation (glide) between steps.**
- **Normalized or raw output;** colour tracks also publish RGBA/HSVA channels.

### Data Looper
- **Records gestures (data) into tracks, quantized to a clock,** and loops them back as data sources.
- **Modes:** Solo, Record New, Record Add (overdub at end), Record Over, Play, and Play-once-then-solo.
- **Track editor:** change length (whole measures), warp points to stretch, slide and copy data, with undo/redo.
- **Options:** start recording on the next measure, keep recording to the end of the measure, snap warp points to peaks, auto-crop a short last measure.

### Cue List
- **Timed cues that trigger clips on layers or set data-source values.**
- **Time formats:** index, beats, measures, SMPTE (with chosen fps) or seconds, plus a start-time offset.
- **Cue columns:** jump button, time, target, value, title, enabled and lock.
- **Cue groups,** expand/collapse, and bulk enable/lock.
- **Keyboard shortcuts** for quick cue entry and nudging.
- **Transport controls:** time slider, restart, previous, pause, next, random cue.
- **Cue-published data sources:** float, index, boolean, event, colour or string, with interpolation (Jump, Linear or Smooth In/Out/In-Out).
- **Cue data sent out over MIDI/OSC/DMX** through the senders on each data source.
- **Jump to a cue by index, float or name string.**
- **Follow a Clock or Timecode plugin,** with offset and "lock to source".
- **Loop playback;** time-slider marks follow the cues.
- **Display options:** current time, countdown to next cue, time remaining and so on; clips shown as thumbnails or names; auto-scroll.
- **Preload the media for upcoming cues.**
- **CSV import/export of cues.**

### Timecode
- **Receive MTC, LTC (audio) or any data source** (OSC seconds, a backing-track movie's time) — one source per plugin.
- **Generate timecode locally** at a chosen frame rate.
- **Time-of-day mode** (12h/24h).
- **Reference times:** publishes time since the last reference, plus its index and name.
- **Send timecode out over MTC, LTC and OSC,** to several destinations at once.
- **Drive Cue Lists, LFOs or movie positions from timecode.**

### Control Surface
- **Custom control panels:** sliders, buttons, pop-ups (with editable items), colour wheel, text field, 2D position pad, multi-slider and multi-button grid.
- **Every element becomes a data source** and can also send MIDI/OSC/DMX.
- **Per-element settings:** toggle/momentary, mutually exclusive groups, ranges, publish normalized, pop-up publish mode (index, many BOOLs or string), colour as RGBA/HSVA channels, grid sizes, custom labels.
- **Custom layout editing.** (tutorial: "customizing the layout of a control surface")
- **JSON import/export of layouts.**
- **Import TouchOSC layouts:** .touchosc, and .tosc since 6.3.
- **Expressions:** float, bool, 2D point and RGBA colour data sources written in ExprTk math/logic (conditionals, switch, loops, local variables, arrays), referencing the surface's own controls (6.3).
- **Per-surface OSCQuery publishing** and the browser web controller.
- **Section presets** for the whole surface.

### OSCQuery Client
- **Remote-control panel built automatically from another app's OSCQuery namespace.**

### Preview
- **Preview any source:** layer (pre/post FX), canvas, input, Syphon, plugin output.
- **Per-preview FPS cap** to save GPU.
- **Layer quick controls:** opacity and blend mode, shown when previewing a layer.
- **Show size and FPS of the source.**
- **Jog/scratch the previewed movie by dragging.**
- **Pass mouse events to ISF/QC sources and FX** in the previewed layer.
- **Clicking a preview selects that layer's tab or a Media Bin's target layer.**
- **Resize to the source's aspect ratio.**

### Two Channel Mixer
- **Crossfader between two layers' opacity,** with Cut and Fade buttons and a fade duration.
- **Auto-Fade:** fades to a layer when a clip is triggered on it.
- **Opacity lookup curves:** custom non-linear crossfade curves per side.

### Movie Recorder
- **Records any video source plus an audio source** to a QuickTime movie.
- **Still-image snapshots** of any stream.
- **Codecs:** H.264, HEVC (new in VDMX6), ProRes 422/4444 and the HAP family, with or without alpha, and quality settings.
- **Hardware H.264/HEVC encoding** when available.
- **Throttle recording FPS; record at native size or resize.**
- **Audio:** Linear PCM, AAC or Apple Lossless.
- **Quantized start/stop:** held until a "Now" trigger, for example the clock's beat 1.
- **Auto-import recordings into a page:** optionally with empty FX chains, quantized playback set from the current BPM, or ignoring workspace presets.
- **Custom save location per recorder.**
- **Low-disk warning at 25 GB; stops recording at 15 GB.**
- **Several recorders at once** ("capture multiple streams … simultaneously").

### Outputs as plugins
- **Syphon Output:** publish any number of streams (layers, groups, canvas).
- **NDI Output:** video plus audio, alpha option, RGB high-quality mode, crop, resize, FPS throttle, black and mute buttons.
- **Blackmagic Output:** any source to SDI/HDMI hardware, with audio, black, mute, port selection, output format, crop, and internal/external keying with key level.
- **Video To DMX:** map pixels to Art-Net (LED walls and fixtures): crop, output size, first-pixel corner, read direction, wrap or snake, start universe and channel, Gray/RGB/HSV/CMYK/RGBW. It also publishes its picture as a video stream for preview.

### Analysis plugins
- **Video Tracking:** human bodies (box centre, size and corners), faces, hands (max count; left, right, all or pairs), each with a Detected boolean. Prev/next/random or click to pick a subject. Masks: person mask (accurate/balanced/fast), attention saliency and object saliency. Overlay preview streams and a minimum-latency option.
- **OCR:** scans a frame for text (with a language choice) or QR codes and publishes the strings. Scans can be triggered from a clock.
- **Scopes:** waveform/parade, point scope and vectorscope (YCbCr or HSV); each scope is publishable as a video source, with graticule and colour options, and can be turned on or off individually.

### Utility plugins
- **Comm Display:** live monitor of incoming MIDI, OSC and HID, with pause and clear.
- **Sticky:** rich-text notes with images, lockable.
- **QC / Vuo / TouchDesigner plugins:** a patch's published inputs become controls; its outputs become data sources or video sources.

## 11. Output and display

Source: https://docs.vidvox.net/vdmx/vdmx_video_display.html , https://vdmx.vidvox.net/blog/vdmx-6-2-update

- **Output Options window (⌘F) and fullscreen toggle (⌘⇧F); Esc releases the main screen.**
- **Window mode:** the canvas in a resizable window.
- **Fullscreen mode:** span the canvas across any set of displays.
- **Per-display flip H/V** in fullscreen (6.2).
- **Repeat the canvas on several display groups** (6.2).
- **Advanced mode:** any number of output windows, each with its own source (layer, group or canvas), source sub-rectangle, display, position, size, scaling mode and crop-to-display. Good for video walls, multi-projector setups and per-layer outputs.
- **Advanced layouts export and import as CSV.**
- **Scaling modes:** Fit, Fill, Stretch, Copy (Copy re-added in 1.2.9).
- **Startup output mode:** "use as default when launching" checkbox.
- **Fullscreen covers menu bars and alerts.**
- **Hide dock completely while VDMX is in front.**
- **Multi-screen templates:** Dual and Triple screen examples.
- **Other outputs:** Syphon, NDI (video and audio), Blackmagic SDI/HDMI with keying, Art-Net pixel mapping, and recording (see Plugins).
- **Streaming:** done through OBS via Syphon/NDI (tutorial); there is no built-in RTMP.

Not found in VDMX docs: edge blending, mesh warping and soft-edge (only per-layer 4-corner quad warp exists), and projector control.

## 12. Presets, projects and templates

Source: https://docs.vidvox.net/vdmx/vdmx_saving.html , https://docs.vidvox.net/vdmx/vdmx_templates.html , https://docs.vidvox.net/vdmx/vdmx_plugin_templates.html

- **Project file (.vdmx6):** pages, clips with settings, and workspace presets.
- **"Update Preset and Save" (⌘S)** vs. plain Save, and Save As in both versions; Revert to Saved; Show Project in Finder.
- **Optionally trigger the first preset on open.**
- **VDMX5 projects auto-converted** (as a copy).
- **Workspace presets:** snapshots of every layer, plugin and control, each with a thumbnail, a trigger receiver and a description.
- **Nested preset hierarchy;** drag to reorder; Trigger, Prev, Next; copy/paste between projects.
- **Per-preset restore scope:** layers and/or plugins, and per component Contents + Window position, Contents only, or Window position only.
- **Lock Preset; Update Thumbnail.**
- **Preset fade-in/fade-out times.** (app tips)
- **Section presets and per-control presets** (see section 7): three levels of presets.
- **Workspace Templates:** layer, plugin and window layouts without media; create from the current workspace and manage in a Templates Manager. About 36 ship, including Simple Player, Simple Mixer, 4-Layer VJ Starter, 4 Channel Mixer, Syphon Player, Dual/Triple Screen, Chroma/Luma Key, Audio Spectrogram, Audio To Color, BPM Detection, Ableton Link, DMX Video Mixer and DMX Color Cues, NDI Output, OCR, QR, Scopes, Face+Hand Tracking, OSCQuery mixer/player, Video Feedback, and controller layouts (APC Mini/20, nanoKONTROL, OhmRGB, PS3, TouchOSC, VIDVOX Grid2). (list: app bundle)
- **Plugin Templates** (see section 10).
- **Assets that persist across projects:** FX chains, number FX chains, comp modes, text/image styles.

## 13. Assets management

Source: https://docs.vidvox.net/vdmx/vdmx_assets.html

- **Asset types:** Video FX and chains, composition modes, text/image generator styles, number FX and chains.
- **Enable or disable individual assets** (hides them from menus).
- **Custom categories / Favorites** for organizing FX.
- **Duplicate an asset with custom defaults,** including the default on/off and wet/dry.
- **Build named FX chains as assets.**
- **Live asset preview** from any source; comp modes get a top/bottom source plus an opacity test slider.
- **"Open Assets Folder in Finder"** from the Help menu; ISF in `~/Library/Graphics/ISF`, which is shared with other apps.

## 14. Extensibility

Source: https://docs.vidvox.net/vdmx/vdmx_isf.html , vdmx_vuo.html , vdmx_touchdesigner.html , vdmx_quartz_composer.html

- **ISF (Interactive Shader Format):** GLSL plus a JSON header for generators, filters and transitions, with multi-pass and persistent buffers. Shared with other apps, and the ISF Editor ships in Extras.
- **Audio inputs in ISF:** waveform and FFT image inputs.
- **Imported images in ISF** (for example, font atlases).
- **CoreImage kernels/filters** as generators, FX and comp modes.
- **Metal composition-mode shaders** (`mslCompModes`).
- **Vuo (Plus):** Image Generator / Image Filter protocols, published inputs become UI, plugins with data/video outputs, text renderers, special BPM and measure-position inputs, and a separate-process option.
- **TouchDesigner (Plus):** .tox generators, FX, text sources and plugins; published parameters become UI; a Rendersize parameter; reserved names.
- **Quartz Composer:** deprecated and off by default; still needed for the built-in text styles in some builds.
- **No JavaScript scripting plugin was found in the VDMX6 docs.** The nearest things are Control Surface Expressions (ExprTk) and receiver "Do Math" (DDMathParser).

## 15. Preferences

Source: https://docs.vidvox.net/vdmx/vdmx_preferences.html ; pane names from the app bundle

- **Themes / Colors:** hundreds of UI colours by state (on, transition, off, inspected, mapped); save, export and import themes; several themes are included.
- **Files:** lock clip state, default render size for generators, default text/image style, max preloaded files per page, max image size, clear thumbnail cache.
- **MIDI:** device enable list, default clock source, dedicated controls, note-name display.
- **OSC:** input ports, output ports, regex matching, dedicated controls, OSCQuery URL.
- **DMX / Lighting:** Art-Net interface, input and output universes, network browser and auto-setup, RGBW conversion default.
- **NDI prefs pane.** (app bundle; contents unverified)
- **Project Files:** trigger the first preset on open.
- **Recording:** defaults for recording from digitizers.
- **Rendering:** canvas default size and FPS, display-driven rendering, VBL sync, skip canvas, double-buffer, throttling, GPU choice, QC on/off and antialiasing, default layer FPS. There's also an advanced rendering pane (app bundle).
- **Sliders / Buttons:** soft-takeover modes and global smoothing.
- **Section Presets:** auto-disable write after updating.
- **User Paths:** location of the VDMX resources folder.
- **Vuo:** in-process or separate-process.
- **Permissions:** status of every macOS permission (6.3).
- **General:** live layer thumbnails on/off.
- **Misc:** hide dock, Add/Replace default for FX, starting-point window, UI redraw FPS throttle.

## 16. Performance features

Source: https://docs.vidvox.net/vdmx/vdmx_video_codecs.html , vdmx_gpu.html , vdmx_troubleshooting.html

- **HAP GPU-compressed playback,** including HAP R.
- **Hardware decode of H.264/HEVC; hardware encode** for one or two streams.
- **Media preloading:** in Media Bin and Cue List, with a per-page limit.
- **Inputs, Audio Analysis, Comm Display and scopes can be switched off** to save resources.
- **Per-layer and per-group render FPS; per-preview FPS caps; UI redraw throttle.**
- **Skip-canvas and output throttling modes.**
- **Multi-GPU / eGPU support** with a GPU choice.
- **Background (Spotlight) media relinking.**

Not found: RAM caching of whole clips. Only "preload" is documented.

## 17. Bundled extras (on the installer DMG)

Source: https://docs.vidvox.net/vdmx/vdmx_oscquery.html , https://vdmx.vidvox.net/blog/vdmx-6-2-update , https://docs.vidvox.net

- **ISF Editor:** with a QuickLook plugin for .fs thumbnails.
- **AVF Batch Exporter:** transcodes to HAP, including HAP R.
- **ProjectMilkSyphon:** a MilkDrop visualizer sent out over Syphon, with app-audio capture.
- **OSCQuery Browser, OSCQuery Helper and MIDI OSCQuery Helper.**
- **Example TouchDesigner and Vuo compositions; sample media downloads.**

---

## Coverage notes

- **Well documented:** plugins, UI items, receivers and senders, media bin, presets, output modes, preferences (VDMX5-era wording).
- **Thin or outdated:** the manual still describes VDMX5 features that VDMX6 dropped: web source, FFGL/FreeFrame, Flash, GLSL comp modes and the OpenGL comp-mode wording. The release notes and the "Updating from VDMX5" FAQ are the authority there. The preferences page doesn't cover the newer panes (Permissions, NDI, Lighting, Vuo, Themes, General).
- **Not documented anywhere official:** the built-in FX list (taken from the installed ISF folder), Number FX modules (bundle file names), the "DMX Fixtures" plugin and Lighting prefs (nibs only), and the full comp-mode list.
- **Not found at all:** JavaScript scripting, edge blending, mesh warping, projector control, built-in RTMP streaming and RAM clip caching. The task brief listed some of these (JavaScript plugin, Alpha Mixer, Switcher, Spectrum, Comments, Text plugin), but none of those exist as VDMX6 plugins in the docs. The nearest equivalents are Sticky (comments), the Text source and ISF FFT generators (spectrum), and the Step Sequencer or Expressions used as a switcher.
