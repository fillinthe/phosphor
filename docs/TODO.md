# To do

Updated as each one moves. `[ ]` not started · `[~]` working on it · `[x]` done (with what was checked).

## VDMX + MadMapper parity (researched 2026-10-22), easiest first

Every feature of VDMX6 (about 430) and MadMapper 6 (about 490) was listed from their manuals and release notes
(`docs/research/vdmx-features-full.md`, `docs/research/madmapper-features-full.md`), then each one Phosphor might
lack was looked for in the code. This is what's missing or only partly there. **V** = VDMX has it, **M** = MadMapper.
Sizes are my estimate: **S** = an hour or two, **M** = half a day to two days, **L** = several days or more.
Hardware items can be built but not tested here without the device.

Items 1–22: in **0.3.0**; 23–35 in **0.3.1**. Medium items ticked below are done and tested but not released yet.

### Small
- [x] 1. **Copy size mode** for layers: 1:1 pixels, no scaling (V). Has Fit / Fill / Stretch.
      Checked: unit test (a half-size picture fills the middle).
- [x] 2. **Cut to black** when a clip ends (V). Has loop, hold last frame, ping-pong.
      Shown next to Loop when Loop is off. Checked: player test (finished at the end, not after seeking back).
- [x] 3. **Random frames** play mode (M).
      A fourth direction, "Random frames". Checked: player test (jumps all over the loop region, never leaves it); 1280×720 H.264 jumps ~75 times a second.
- [x] 4. **Jump back / forward** buttons: N seconds or N beats (V).
      Jump row in Playback: back / forward, step in beats (at the layer's speed) or seconds; both mappable. Checked: unit test of the step.
- [x] 5. **Reload a media file** from disk after you've edited it (M).
      Media bin right-click › Reload, and a Reload button in Playback; keeps the place in a movie. Not tried with a file edited while playing.
- [x] 6. **Media info**: fps, bit rate, colour space, sound; **used by** count (which layers/clips use a file) (M). Has codec, size, length.
      Playback line now shows fps, bit rate, pixel format, colour space (when the file says) and sound; media tiles say how many layers and clips use the file. Checked: unit test + screenshot.
- [x] 7. **More blend modes**: Soft/Hard/Vivid/Linear/Pin Light, Hard Mix, Colour Burn/Dodge, Divide, Hue, Saturation, Colour, Luminosity (V). Has 10 + 3 masks.
      13 more (Soft / Hard / Vivid / Linear / Pin Light, Hard Mix, Colour Burn / Dodge, Divide, Hue, Saturation, Colour, Luminosity). Checked: GPU test of every mode's formula.
- [x] 8. **Crossfader curve** (V). Fixed straight line now.
      Mixer: Both full in the middle (as before) / Smooth (no dip) / Straight / Cut. Checked: unit test + screenshot.
- [x] 9. **Soft takeover settings**: pickup distance, and "glide to the controller" (V). Has pickup at a fixed distance.
      Control panel: Glide (the value comes to the fader) and the pickup distance. Checked: tests of both.
- [x] 10. **More mapping value effects**: threshold, randomize, clamp, wrap, peak-hold (fall), average, drawn lookup curve (V). Has smooth, steps, curve, formula, delay.
      Threshold, Limit, Repeat, Jitter, Fall (peak hold), Drawn curve. Average left out (Smooth does that job). Checked: tests.
- [x] 11. **Dead-band and endless encoders** (relative MIDI knobs) on mappings (V).
      Value effects "Endless knob" (relative MIDI, two's complement) and "Dead band". Checked: tests.
- [x] 12. **Controller inputs**: turn single MIDI devices on/off, several OSC ports, wildcard OSC addresses (V, M).
      MIDI devices could already be switched off one by one (Devices). New: more OSC ports ("also on ports…"), and wildcard OSC addresses on mappings (`*`, `?`, `[1-4]`, `{a,b}`). Checked: matching test + a real UDP message on the extra port.
- [x] 13. **Mappings list: jump to the control** it drives (V). The list itself exists.
      Crosshair button on each mapping: selects its layer / group / slice and opens its panel. Checked: test.
- [x] 14. **Search in the layer source menu** (V). Explorer and ⌘K already search.
      A search field at the top of the layer source menu finds any generator (built-in and imported). Not checked on screen.
- [x] 15. **Copy / paste a control's mappings and modulation** onto another control (V, M).
      Right-click any control › Map: Copy (its mappings) and Paste mappings here. Sliders with modulation also get Copy / Paste the motion. Checked: test of the paste.
- [x] 16. **Project notes** panel (V, M).
      A Notes panel (View › Panels), saved in the project file. Checked: panel check.
- [x] 17. **Freeze outputs**: hold the picture while you change things behind it (M).
      FREEZE on each output (Output panel), and a mappable "Freeze outputs" for all of them. Not saved: a show opens live. Checked: GPU test.
- [x] 18. **Flip an output** horizontally / vertically, for rear projection (V, M).
      Flip left-right / top-bottom on each output (Output panel). Checked: GPU test. The output window's edit mode doesn't mirror its handles yet: map before flipping.
- [x] 19. **Output delay** in frames, to line up mixed projectors and LED processors (M).
      Delay 0–10 frames per output. Checked: GPU test (2 frames late shows the frame from 2 frames before).
- [x] 20. **Snapshot** any output, layer or comp to a picture file (V, M). Has a snapshot of the mix.
      Partly: a camera button on each output saves a picture of it after mapping (the mix snapshot already existed). Layers, groups and comps: right-click › Save a picture….
- [x] 21. **Hide effects** you never use from the menus (V). Favourites exist.
      Right-click an effect or source › Hide from the lists; they show under "Hidden" (to bring back) and still work where used. Kept between launches.
- [x] 22. **Slice: match the media's size / shape** (M).
      Mapping Studio: "Match the picture's shape" (keeps the height and centre, sets the width). Checked: test.
- [x] 23. **Cue list extras**: random cue, countdown to the next cue, CSV import/export, cue groups (V).
      Random cue (button + mappable), a countdown to the next cue that fires by itself, CSV out and in (names and timings; the looks stay recorded). Cue groups: not done. Checked: CSV test, bench run (countdown right, random never repeats), screenshot.
- [x] 24. **Permissions panel** (Mac): camera, microphone, screen recording, network at a glance (V).
      Devices panel: camera, microphone and screen recording, each with a System Settings button. Checked: screenshot (all three read right here).
- [x] 25. **Choose the graphics card** on machines with more than one (V).
      Output panel › Graphics card (shown when there's more than one, or one was chosen); used from the next start. Checked: test of the choice. Not tried on a Mac with two GPUs (none here).
- [x] 26. **Step sequencer colour and index tracks** (V). Has value and gate tracks.
      Index tracks: "Choices" on a level track, each step picks one of N (a clip, blend mode, direction). Colour tracks: a colour on each step (rainbow to start; Glide fades between them), painted into any colour settings (pick the colour; its red, green and blue all follow). Checked: tests; the real app (a white solid layer painted blue by a track).
- [x] 27. **DMX monitor**: live view of the channels being sent (M).
      LED / DMX panel › Monitor: one universe's 512 channels as a grid, value on hover; copied only while it's open. Checked: test.
- [x] 28. **Colour swatches** saved and usable on every colour control (M).
      A colour button with saved swatches: on text colour and on every shader colour input (ISF colours). Kept between launches. Not checked on screen.
- [x] 29. **More picture formats**: EXR and HDR (float), PSD, HEIC as a layer (M). Has PNG, JPEG, WebP, BMP, TIFF, GIF.
      EXR, HDR, TGA, QOI through the image library, PSD and HEIC through FFmpeg. Also fixed: BMP and TIFF were offered but couldn't open. Checked: test making each format with macOS's sips / FFmpeg.
- [x] 30. **Look fade time**: a saved look fades in instead of cutting (V).
      Moved to medium: a look replaces the whole project, so a fade means holding the old picture over the new one.
- [x] 31. **Autosave interval and backups**: keep the last few versions on every save (M). Crash recovery exists.
      Every save first copies the old file into "<name> Backups" (last 10 kept); File › Autosave On / Off (every 5 minutes when changed), File › Show Backups. Checked: tests.
- [x] 32. **ISF files reload when edited** on disk (V). Loaded once at start now.
      Shader files are watched (every 2 s): a saved edit replaces the shader on screen. Checked: GPU test (black → red after the edit) and a bench run with a file edited mid-run.
- [x] 33. **LUT mask mixer** (two LUTs mixed by a mask) and a few **built-in LUT looks** (V). LUT Mix exists.
      8 looks ship (Warm, Cool, Teal and Orange, Bleach Bypass, Mono Contrast, Cross Process, Faded Film, Night; made by scripts/make-luts.py), and LUT Split: two LUTs by brightness, left/right, top/bottom or from the centre. LUT Split › Split by › Mask layer: another layer's brightness picks LUT B (bright) or LUT A (dark). Checked: tests; GPU test (a black mask gives LUT A, a white one LUT B).
- [x] 34. **Saved "show only these" slice sets** while mapping (M).
      Mapping Studio › Sets: save the selected slices as a named set, pick it to select them and show only them. Saved with the project.
- [x] 35. **Slider limits** (use only part of a slider's range) and **"ignore the controller while this control is hidden"** for one knob across pages (V).
      Slider limits (right-click › Limits…): the mouse stays inside, a controller's whole travel covers just that part, outside is shaded. "Ignore while hidden": not done. Checked: tests.

### Medium
- [x] 36. **A layer's picture before its effects** as another layer's source (V). After effects exists.
      Source menu › From another layer › "(before effects)". Checked: GPU test (0.2 before an invert, 0.8 after).
- [x] 37. **Blend mode per effect**: how an effect's output mixes back with its input (V). Dry/wet exists.
      A blend menu beside each effect's Mix. Checked: GPU test (Invert with Multiply).
- [x] 38. **Wipe blend modes** driven by opacity (radial, star, blocks…) (V). Wipes exist as clip transitions only.
      Six blend modes: Wipe left to right, top down, from the centre, diamond, blocks, and Dissolve; opacity = how far it's come in. Checked: GPU tests.
- [x] 39. **Your own blend modes** written as shaders (V).
- [x] 40. **Generator size per layer / clip** (V). Generators render at the comp's size.
      Inspector › Render size 100 / 75 / 50 / 25% for generators (effects run at that size too). Checked: GPU test.
- [x] 41. **Frame rate per layer / comp**, to save GPU (V).
      Inspector › Frame rate: every frame / 30 / 15 / 10 / 5, the picture held between. Checked: GPU test.
- [x] 42. **Before / after preview of one effect** in a chain (V).
- [x] 43. **Mouse into shaders**: click and drag in a preview drives the shader (iMouse, point inputs) (V). Works today only by mapping the preview's XY.
      Preview bar: a mouse button; dragging moves the selected layer's point settings (ISF point2D). Shadertoy shaders that read iMouse get a Mouse point and a Mouse down button: dragging in the preview moves it and holds the button. Checked: test of the point mapping; GPU test (the mouse's place and button reach the shader).
- [x] 44. **Previews**: drag to scrub the movie, frame-rate cap (V).
- [x] 45. **Eyedropper**: pick a colour from a preview (V).
- [x] 46. **Endless scratch** control for movies (V).
      Playback › Scratch strip (drag; wraps at the ends).
- [x] 47. **Movie chapter markers**: jump to chapters stored in the file (V).
      Playback › Chapters menu when the file has chapters. Checked: test with a movie made with two chapters.
- [x] 48. **Text from a file**, shown word / line / sentence at a time with next / previous; **animated text styles** (beat, scrolling, fade); **text set by OSC** (V).
      Text › Parts: from a file or by splitting, by lines / sentences / words / paragraphs; next / previous (mappable). Animated text styles: not yet. Checked: tests.
- [x] 49. **Strings and colours from controllers** (OSC text and colour values) (V).
      OSC words into text layers (`/phosphor/layer/<id>/text`, or any mapped address). Colours from controllers: not done. Checked: test.
- [x] 50. **Any control sends MIDI / OSC / DMX** when it changes (V). Feedback to its own controller exists.
      Right-click any control › Send out: OSC to the Control Surface's OSC-out host, MIDI CC or note. Checked: real UDP test.
- [x] 51. **Control Surface widgets**: text field, multi-slider, button grid, pop-up menu, colour wheel, computed values (V).
      Control Surface: a Choices widget (named buttons, one picked; also on the phone remote and in the TouchOSC export). Text field (onto a text layer; Mac and phone). A button grid or pop-up menu = Choices (+ "pick a clip by value"); the colour wheel = the Colour widget. Multi-slider (bars with their own targets; draw across them; Mac, phone, TouchOSC export). Computed value: a formula of other controls (a, b, c…, time, beat) that drives its targets. Checked: tests.
- [x] 52. **Import TouchOSC layouts** (V). Export exists.
      MIDI / OSC panel › Import TouchOSC layout: pages, controls in place, OSC addresses mapped. Checked: round-trip test with the export.
- [x] 53. **Clip grid lights on Launchpad / APC**: pad colours for empty / loaded / playing / queued (V, M).
      MIDI / OSC panel › Clip pad lights (presets for Launchpad / APC mini mk2 and the first APC mini). Checked: test of the states; not tried on a real Launchpad.
- [x] 54. **Map a whole row of clips at once** (first and last note), bank / transpose the mapping, pick a clip by index or value (V).
      Clip row name menu › MIDI notes: map the row from a note, shift by ±1 / ±8; "Pick a clip by value" (mappable). Checked: tests.
- [x] 55. **Export / import a bank of clips** with settings; **set a clip's thumbnail** from the current frame (V).
- [x] 56. **Recorder**: record any layer, comp or camera (not only an output), start/stop on the beat, several recorders at once (V). Done: record a layer / group / comp, start and stop on the bar, and up to 8 more recorders at once (Stream panel › More recorders: each its own source, file, format and frame rate). Checked: bench (main + 2 more recorders at once: 177 frames each).
- [x] 57. **Scopes as a layer source** (V).
- [x] 58. **Drawn LFO shapes** (V). Has six fixed shapes.
      "Drawn wave" modulation source (a smooth loop through eight points). Checked: test.
- [x] 59. **Audio**: your own named frequency bands as modulation sources, sound play-through, capture another app's sound with no loopback driver (V).
- [x] 60. **Sound routing**: per-layer output device and channels, multichannel outputs (V, M). Everything mixes to one stereo output now. Done: each layer on a pair of channels of the sound output, and a different sound device per layer (Inspector › Sound device: its own output, mixing only those layers). Checked: tests (routing, stretching to the device's rate). Not heard: tests stay silent.
- [x] 61. **Masters**: separate video, sound and DMX masters, master speed, sound EQ (M). Has a master fade. Done: DMX master, master movie speed, sound EQ (master volume was already there).
- [x] 62. **Send timecode and clock**: MIDI clock out, MTC and LTC out, time of day as timecode (V, M). Receiving exists.
      MIDI / OSC panel › Clock out to a chosen MIDI output (24 a beat, Start / Stop, locked to the beat). MTC out (MIDI) and LTC out (audio on a chosen pair), from the show clock or time of day. Checked: test with a virtual MIDI destination (24 pulses in half a second at 120).
- [x] 63. **Cues that store only chosen settings** while media keeps playing (M). Cues store everything now.
- [x] 64. **Schedules**: calendar (.ics) events trigger cues, an idle cue after N minutes of nothing, a startup cue (M). Daily times exist.
- [x] 65. **Combine controls**: mix several sources into one (max, add, multiply…) (M).
- [x] 66. **Readable OSC addresses** by name (`/layer/Intro/opacity`) and wildcards (M). Addresses use ids now.
- [x] 67. **Merge another project** into this one (M).
- [x] 68. **New project from a template** (simple player, mixer, 4 projectors…) (V, M).
- [x] 69. **Background picture on an output** (M).
- [x] 70. **Mapping**: drop a file on the stage to make a slice at its shape; draw a line slice freehand (M).
- [x] 71. **Automatic soft edges** from overlapping slices (M). Set by hand now.
- [x] 72. **One canvas across several displays** (2×2 etc.) and **one output shown on several displays** (V, M).
- [x] 73. **LED fixtures**: circle and curve shapes, sampling (centre / average), brightness curve, CSV import, video-wall template, more wiring patterns, custom channels (M). Done: ring, curve, column wiring, CSV import, master, Average (each light averages its own patch: 9 samples), Video wall… (tiles across × down covering the picture, each patched on fresh universes). Checked: tests (CPU and GPU).
- [x] 74. **Vision effects**: blur / pixelate faces, picture over faces, colour match to a reference, palette (k-means), QR overlay, different blur for person and background (V). Remove Background exists.
- [x] 75. **Generators**: countdown, stopwatch, GPU particles (M).
- [~] 76. **NDI sound** in and out (V). Done: NDI sound out. Left: NDI sound in.
- [x] 77. **Edit several clips / layers at once** (M). Slices already multi-select.
- [x] 78. **Stream Deck** buttons for cues, clips and settings (M). Keys + Stream Deck + dials as input, key pictures; not tried with a real deck.
- [x] 79. **OSC Query client**: control another app from Phosphor by browsing its settings (V).
- [x] 80. **SVG**: export the mapping as SVG / PNG / PDF; import / export masks and lines as SVG (M). Done: SVG, PNG and PDF export (Save as › .pdf: a vector page the output's size), masks and lines from SVG. Checked: tests; the PDF passes qpdf's check and was looked at.
- [x] 81. **SVG and Lottie** (After Effects) animations as clips (M). SVG; Lottie .json (not yet .lottie zips).
- [x] 82. **Lock the panel layout** only (not the whole player mode) (V).
      View › Lock Panels On / Off.
- [x] 83. **Interface colour themes** (V). Has a brightness slider.
- [x] 84. **Picture depth**: 32-bit float buffers, anti-aliasing per output (M). Done: smooth edges (anti-aliasing) per output; Deep colour (Output panel): layers mixed and effects worked out at 16 bits a channel, same looks, no banding (on cards that can draw into 16-bit textures). Checked: GPU tests (fine detail kept: 33 vs 8-bit's 64 for a true 33; every saved test project looks the same with it on).
- [x] 85. **Several clip grids aimed at different layers**, and voice modes: next layer each time, or next free layer (V).

### Large
- [x] 86. **Keyframe timelines** (MadMapper 6's Conductor): keyframe any setting, record by moving controls, hold / linear / curve, markers (pause, loop, go to), sound track with waveform, OSC / MIDI tracks, a clip edit track with crossfades (M). The biggest one. Done: tracks for any control, keys (hold / line / smooth), markers (pause, loop back), Timeline panel. Record by moving controls: done. Events (play a clip, fire a column or cue, set anything, send OSC / MIDI as the playhead passes; through loops): done, which also covers OSC / MIDI and clip tracks (crossfades = the layer's transition). Sound track with its waveform: done (Timeline › Sound…; drag to place it, right-click for volume). Checked: tests; bench (the sound stays with the playhead).
- [x] 87. **Render a show to a movie** offline, with the sound mix (M). Needs 86.
      Timeline › Render…: file, format (H.264, ProRes, Hap, HEVC), fps, length; the show's clock steps a frame at a time and every movie waits for its picture, so nothing drops or repeats. No faster than real time. Sound: the timeline's sound track goes in, in step (movies' own sound doesn't; Hap renders have none). Checked: bench `--render` with a counting movie (1080p H.264 and 4K ProRes: all 60 frames, in order, 5 runs out of 5).
- [x] 88. **Record DMX** output and play it back (M).
- [x] 89. **3D models**: lights, shadows, wireframe, UV editing (M). OBJ + calibration, Light, glowing Edges, Shadows, picture wrapping (UV mode + move / size / turn).
      Done: Light (faces shaded by a light you place: the object looks solid) and Edges (glowing lines along its corners and outline, any width: the wireframe look). Left: shadows, UV editing. Checked: tests (a turned cube: three faces, three brightnesses, nine edges) and a GPU test (looked at the render).
- [x] 90. **Curved mesh warp** with handles on every point; separate input and output meshes (M). Grid mesh and curved edges exist. Done: separate input mesh (Studio input view); handles on each mesh point (select a point: drag its tips across and down; right-click › Straighten). Checked: tests (no handles = the same surface as before; GPU: a handle moves the picture, straightening restores it).
- [x] 91. **Read text and QR codes** from video into data (V).
- [x] 92. **LED scanner**: a camera finds each LED and places the fixtures (M). Built; not tried with real lights and a camera yet.
- [x] 93. **MilkDrop visualiser** (projectM) (V). .milk / .milk2 presets as layers (lactea, on our wgpu device). Presets not bundled (O10).
- [ ] 94. **Online shader library**: browse, install, publish (M). **Needs you**: a server.
- [x] 95. **USB DMX** interfaces (Enttec Pro / Open DMX) (M). Both built; not tried on real hardware yet.
      Done: Enttec DMX USB Pro and compatibles (LEDs panel › USB (Enttec Pro) › its port; one universe). Checked: tests (packet; a pseudo-terminal gets the bytes unchanged). Not tried with the real box. Open DMX (the cheap kind without its own timing): not done.
- [ ] 96. **Blackmagic** capture and SDI / HDMI output (V, M). Hardware + SDK.
- [x] 97. **Arduino** (Firmata): sensors in, LEDs / servos out (M). Hardware.
      MIDI / OSC panel › Arduino › its port (StandardFirmata): analog inputs and digital pins map like MIDI (Learn works); Send out › Arduino drives PWM pins. Checked: tests (Firmata in and out; a pretend board on a pseudo-terminal). Not tried with a real board.
- [ ] 98. **Depth cameras** (Kinect v2, RealSense) (M). Hardware.
- [~] 99. **Lasers** (ILDA files, Ether Dream, Helios) (M). Done: ILDA files play as clips. Left: sending to a real laser (Ether Dream, Helios): hardware, and laser safety.

### Not planned (say if you want any)
Quartz Composer / Vuo / TouchDesigner hosting (QC is dead, the others need their own apps), Core Image filters,
Wiimote, Leap Motion, Makey Makey, weather data, Canon / Sony DSLR inputs, MiniMad hardware export, licences / demo mode.

Already in Phosphor and not listed: everything else in the two research files at headline level (see `docs/PARITY.md`).

## Online media: start a show with no media of your own (planned 2026-10-23, after the parity list)

Goal: someone with nothing on their computer can open Phosphor and fill a show from free, openly licensed sources
online. One **Online** panel in the Library searches every source at once: thumbnails, preview on hover, drag onto a
clip cell or layer. Files download into the project's media folder (so the show works offline and in exports), each
with its licence and credit saved next to it (a Credits list for the show; "only ones I can use commercially"
filter). Sources that need an API key: a key field per source (like the AI key), or later through a phosphorfx.com
proxy (**needs you**: the server).

**Started 2026-10-23:** the Online panel (Library) searches NASA, Wikimedia Commons, the Internet Archive (Prelinger
films), Openverse and the Art Institute of Chicago at once (no keys); downloads land in the media bin with their credit,
and a Credits list copies them all. Files go to Phosphor's downloads folder, not yet into the project's own folder.

- [~] O1. **Stock video and pictures** (NASA, Internet Archive, Wikimedia Commons done; Pexels, Pixabay, Coverr need keys): Pexels and Pixabay (free API keys; free to use, no credit needed), Coverr,
  NASA Image and Video Library (public domain, no key), Internet Archive (public-domain films, no key), Wikimedia
  Commons (CC, no key).
- [~] O2. **Art and photos** (Openverse and the Art Institute of Chicago done; Unsplash needs a key; other museums next): Openverse (search across CC collections, no key), Unsplash (key), museum open-access
  collections with APIs and CC0 pictures: the Met, Art Institute of Chicago, Smithsonian, Rijksmuseum, Europeana.
- [~] O3. **Shaders** (Vidvox's ISF-Files already ship with Phosphor; isf.video, glslsandbox, Shadertoy still to do): the ISF online library (isf.video) and Vidvox's ISF-Files on GitHub (straight into the
  effects / generators list), glslsandbox; Shadertoy (API key; most shaders are non-commercial CC BY-NC-SA: label
  them). Overlaps #94.
- [ ] O4. **Lottie animations**: LottieFiles' free animations (once Lottie clips land, #81).
- [~] O5. **Icons and vector shapes** (Iconify done: every open icon set, white SVG; emoji sets come with it): Iconify (200,000+ open-licence SVG icons, one API, no key), Phosphor icons,
  OpenMoji / Twemoji (emoji as SVG): as SVG clips or text-layer symbols.
- [~] O6. **3D models, textures, HDRIs** (Poly Haven textures done; models and HDRIs next): Poly Haven and ambientCG (CC0, public APIs), Kenney (CC0 packs), NASA 3D
  models, Smithsonian 3D: for 3D-model slices and textures.
- [~] O7. **Music and sound** (Online › Music: Internet Archive netlabel releases, Creative Commons; Freesound / Jamendo need keys): Freesound (CC, key), Jamendo (CC music, key), Free Music Archive, Internet Archive
  audio, Pixabay music: clip sound, timeline sound tracks, and something for the beat detection to follow.
- [x] O8. **Fonts** (Online › Fonts: Google Fonts families, regular + bold into Phosphor's fonts folder): Google Fonts (open licences; the GitHub repo or the API) for text layers.
- [x] O9. **DMX fixture definitions** (LED / DMX › A real light…: any Open Fixture Library light and mode): Open Fixture Library (MIT, JSON for thousands of lights): pick a real moving
  head / LED bar by name instead of typing channels.
- [x] O10. **MilkDrop presets** (Online › Visualizers: the whole Cream of the Crop pack, searchable): projectM's preset packs ("cream of the crop") once #93 lands.
- [x] O11. **Live data that moves visuals** (weather, ISS, earthquakes as modulators; news headlines and a city clock on text layers) (as modulators): weather (Open-Meteo, no key), earthquakes (USGS), the ISS
  position, RSS / news headlines into text layers, a clock in any city.
- [ ] O12. **Live pictures**: public webcams and space feeds (NASA / ISS live streams) as live layers; GIFs from
  Giphy / Tenor (keys). The "From a link" yt-dlp route already covers pasted video links.
- [x] O13. **Starter shows** (Online › Start a show: Space, Nature, Retro, Club, Gallery): one click makes a ready show from these sources (a few clip banks, effects, a sound
  track), by mood ("club", "ambient", "corporate").

## Next round (2026-10-21, "keep going on the large stuff"), quickest first

Released in **0.2.97** (Mac, Windows zip + installer, Linux ARM; installed here): the first three items below.
Hap R is in **0.2.98**, lostjared's effects in **0.2.99** (both published). This round is done.

- [x] **Mapped controls stand out** (asked mid-way): gold outline and tint on every mapped control. Checked on a screenshot.
- [x] **Stereo audio pictures**: row 0 left, row 1 right, both sides in each pixel's colours; newest 512 samples in
      a row. Checked with test tones, left only and then 2:3 stereo. Curve Ink Scope (px-stream) ships and draws the
      Lissajous figure; X.1–X.5 need a picture drawn by their Max patch, not sound.
- [x] **Instanced vertex shaders**: a shader can draw its quad many times (`gl_InstanceID`, MadMapper
      `INSTANCE_COUNT`, a fixed number or a setting), and its vertex stage can hand its own values to the pixel stage
      (`out`/`varying`). MadMapper's **Transform Particles** now works (checked: copies of the test card fly out and
      blend over each other). The 406-shader corpus went from 396 to 397. MadMapper's "3D Particles" and
      "instancing" generators still don't: they draw their own 3D meshes (vertex attributes), which Phosphor's quad can't stand in for.
- [x] **Hand-picked lostjared effects**: 53 shipped (0.2.99). 1,330 of the collection compiled; drawn over a photo,
      557 dropped automatically (blank, untouched, hard flashing, near-duplicates), the other 773 looked at on contact
      sheets. Not checked moving in the app with real music.
- [x] **Hap R recording** (BC7: Hap's best quality), and with it a real check of Hap R playback. Record and Convert
      both offer it. Playback didn't work before: FFmpeg doesn't know the Hap R tag, so files opened as an unknown codec
      (fixed). Checked: 1080p recording kept up at 30 fps, and the recording played back with the right picture. Not
      tried in other VJ apps.

## First round (asked 2026-10-21)

- [~] **Crash reports sent automatically** (#5): the app side is done (below); **needs you:** a private endpoint
      on your server. Spec: accept `POST` with JSON `{"version", "os", "arch", "report"}` (all strings), answer
      2xx, keep it private. Then put its address in `ENDPOINT` (crates/phosphor/src/crash_report.rs), or tell me.
- [?] **Render thread** (#6): **measured first, and the reasons for it don't show up.** Holding off unless you
      see a stall in real use (tell me when and what you were doing).
      - Menus: a new bench test (`--quick menutest`) opens a real macOS menu. Rendering went on at ~57 fps the
        whole time it was open, so menus don't freeze the outputs.
      - File dialogs are already non-blocking.
      - The idle cost (26% of a core) is mostly the system's GPU work; Phosphor's own code is a few percent of it.
      - Doing it means moving the engine (346 call sites) and the output windows off egui: the biggest, riskiest
        change in the app, for no measured gain today.

- [~] **Virtual camera** (#2): **built; needs your Apple Developer account to switch on.**
      - Output › Virtual camera turns it on: the composition as a camera called "Phosphor" (1920×1080, 30 fps).
      - The camera is a Core Media I/O camera extension (`macos/PhosphorCamera`, built by
        `scripts/build-camera-extension.sh`, packed into the app by `bundle-mac.sh`). Phosphor installs it and
        feeds it frames.
      - Checked: both chips build; the bundle carries the extension; Phosphor asked macOS to install it, and macOS
        answered "Missing entitlement com.apple.developer.system-extension.install", shown in the panel. That's
        the step only your account can pass.
      - **Needs you:** the steps at the top of `scripts/build-camera-extension.sh` (team ID, two App IDs, two
        Developer ID provisioning profiles, sign, notarize). The frame sending can't be tried until then.

## Done

- [x] **Windows installer** (#7): `Phosphor-windows-setup.exe` (Inno Setup), built on GitHub with every push and
      attached to releases next to the zip. Installs for the current user (no admin prompt), Start menu shortcut,
      optional desktop shortcut, `.phosphor` projects open in Phosphor (and Phosphor now opens a project given
      on its command line), uninstaller; the built-in updater keeps working. Checked on GitHub's Windows machine:
      silent install, files and file association present, the installed copy ran the test project, uninstall
      removed it. **Not tried on a real PC by hand.**

- [x] **Phosphor for Raspberry Pi / Linux ARM** (#9): built on GitHub's ARM Linux machines with every push
      (`.github/workflows/linux.yml`), packaged as `Phosphor-linux-arm64.tar.gz` (program, FFmpeg libraries,
      shaders, a start script and a README), attached to releases when it built. It's the full app; as a player it
      uses Session › Follow another Phosphor › "Every time Phosphor starts (a player)". Checked on GitHub: engine
      tests pass on ARM Linux, and the packaged app opened the test project on a virtual display (software Vulkan,
      8.5 fps on the CPU-only renderer; a Pi 5's GPU is far faster). **Not run on a real Pi** (none here). Media
      must be at the same paths as on the Mac.

- [x] **Crash reports, app side** (#5): Report a Bug › "Send crash reports by themselves" (off unless chosen;
      greyed until the server exists). After a crash, the same diagnostics the bug window shows (version,
      system, project shape as counts, end of crash.log; no project content, no file names) are posted once,
      and the recovery banner says whether it went. Checked: a local server received the JSON POST (test).

- [x] **Older macOS** (#9): Phosphor now runs from **macOS 13 Ventura** (was 14), the same floor as VDMX. One
      setting (`scripts/min-macos`) for FFmpeg, Syphon and the app; FFmpeg and Syphon rebuilt for it; a test
      bundle has every binary marked 13.0, its "too new" check and the panel check pass, and the linker gave
      no "built for newer macOS" warnings. 11–12 aren't possible: the screen-capture library calls macOS 13
      functions (it would mean a build without screen and window capture). **Not run on a real macOS 13 Mac** (none
      here).

- [x] **Masters** (#5): an output can show a whole comp as its program (Outputs list and Studio › output:
      "Shows: Main mix / Comp …"). Make one comp per master, switch its "In the master mix" off, and pick it on the
      output: each output then has its own mix. Checked: two comps (red, green), output 2 showing the green one,
      main output red, output 2 green; with the green comp still in the master mix, the picker says so.

- [x] **Shader packs** (#5), as far as is sensible:
      - daitomanabe's 11 (MIT) were already in; **px-stream**: of darosh's own shaders (MIT), Glitch Wave ships.
        His XY scopes convert but need stereo audio pictures (Phosphor's is mono), so they'd only draw a diagonal.
      - The converter now renames words that newer GLSL reserves (`sample`, `filter`, `input`…), which older ISF
        shaders use as names. That fixed 3 of these, and helps anything you import. No change across the 406-shader
        corpus.
      - **lostjared/shaders** (MIT, 2,700 effects): checked, not shipped. No categories or ratings to pick from, and
        converting 2,700 at every launch would cost tens of seconds of CPU. Worth a hand-picked set if you want one.

- [x] **One Inspector** (#8): the Inspector shows whatever you selected last: a layer (with its clip, and its
      effects as before), a mapping slice or mask, or a Control Surface widget. A Layer / Slice / Widget switch on
      top when more than one is selected. Checked: a click on a slice in the Studio turned the Inspector to that
      slice's settings (screenshot). Widgets use the same path; not seen on screen yet.
