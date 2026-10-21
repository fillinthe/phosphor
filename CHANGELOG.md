# Changelog

What changed in each Phosphor release, newest first. Downloads are on the
[releases page](https://github.com/fillinthe/phosphor/releases); Phosphor also finds and installs updates itself
(Help › Check for Updates…).

## 0.2.95 (2026-10-21)

- **ProRes plays far more lightly:** ProRes 422 now goes straight from the Mac's video decoder to the graphics card
  (about half the CPU), and **ProRes 4444 with transparency** is decoded by the Mac's hardware too: four 1080p layers
  went from nearly five CPU cores to under half of one, with truer colours.
- **Record and convert to Hap Q and Hap Q Alpha:** much better colour than Hap (smooth gradients, skin, logos), still
  decoded by the graphics card.
- **Fixed:** Hap Q Alpha clips had dark fringes on their soft edges.

## 0.2.94 (2026-10-21)

- **Library › Inputs:** every camera, capture card, screen, window, Syphon and NDI source in one list, plus a stream
  address. Drag one onto a layer or a clip cell, or put it on the selected layer or a new one.
- **Drag onto a slice:** in the Mapping Studio, drop a media file, a live input or a layer on a slice to show it there.
- **The Inspector shows the clip you click:** name, how a press plays it, MIDI mapping, sound, its own settings.
- **Moving heads: extra channels** (gobo, prism, zoom, strobe…), each a control you can map and modulate.
- **Fixed:** dragging media onto a layer card did nothing; some sliders were hidden off the edge (a moving head's
  tilt and dimmer, the 3D-model mapping sliders, LED gamma).

## 0.2.93 (2026-10-21)

- **Curved slice edges:** turn on Curved edges on a slice and drag the two handles on each edge to bend it onto
  round tables, arches and curved screens; the picture follows. Works with Mesh warp too.
- **Movie sound in streams and recordings:** Stream & Record › Movie sound puts the sound of the movies playing into
  your stream or recording, in sync with the picture, mixed with your mic or interface if you pick one too.
- **Choose where movie sound plays:** Audio › Plays on (headphones, an interface, a loopback…). Remembered.
- **Step tracks fire clips:** a gate step track can play clips, fire columns or switch any control on each lit step,
  a clip sequencer on the beat.
- **Lights are controls:** a moving head's pan, tilt, dimmer and movement size, and any light's brightness, can be
  learned to MIDI, modulated, locked, put on the Control Surface, and set over OSC.

## 0.2.92 (2026-10-21)

- **Describe a shader (AI):** in the Shader Editor, Describe it… — say what it should look like and the AI writes it
  (a new effect or source, or a change to the open shader), with sliders for its settings. Shaders that don't compile
  are fixed automatically. Needs your own Anthropic API key, kept in the macOS Keychain.

- **Phosphor keeps itself up to date.** It checks for a new version as soon as it opens (and every hour), downloads
  and installs it in the background, and then asks to relaunch. Choose Later and the new version opens next time.
  Turn it off in Help › Check for Updates (Install updates by themselves).
- **From a link:** Media › From a link… brings in the video or GIF from a post on X, Instagram, TikTok, YouTube,
  Vimeo and more: download it into the media bin (a clip that loops and works offline) or play it live from the
  link. Instagram may need "Sign-in from <your browser>". Needs yt-dlp (free; the window says how to get it).
  GIFs stay GIFs: a direct GIF link is saved as is, a site's GIF is preferred over its MP4, and GIF/picture posts
  (Tumblr, Imgur…) come in through gallery-dl when it's installed. (X and Instagram only have MP4s.)
- **FFGL mixer plugins (Mac).** Mixer plugins (the kind Resolume uses to blend layers) now work: Inspector ›
  **Mixer plugin** picks one for a layer, and it blends that layer onto the layers below instead of its blend
  mode. The layer's opacity goes to the mixer's Opacity setting, like in Resolume, and its other settings are
  right there.
- **Where a layer's picture comes from.** The Inspector now says it at the top: generated (and by what), a movie
  or picture file (its full path, a Show in Finder button, and a warning if the file is missing), a camera,
  screen, Syphon, NDI or stream, another layer, or text; and which clip slot triggered it.
- **New example patch:** Shader Editor › Patch › "Example: kaleidoscope glow with trails".
- **The Session picture is live (WebRTC).** People watching your output in a browser on the network now get
  it over WebRTC (H.264): sub-second instead of the old JPEG stream, and much less network traffic. Browsers that
  can't do it still get the old picture.
- **A file straight on a slice.** A mapping slice's "Shows" list has **A file…**: pick a picture or movie and
  that slice plays it on its own, without it appearing in the mix.

- **Sharp sliders.** Every slider, meter, progress bar, video scrub bar and step-sequencer bar now has square corners, and the last few plain sliders (crossfader, interface
  brightness, stream bitrate, media tile size, layer window width) are the same slider as everywhere else: name
  and value inside, double-click to reset, right-click for an exact value. The crossfader's right-click also
  maps it.
- **Player mode.** While following another Phosphor, "Every time Phosphor starts (a player)" makes this Mac
  follow that leader at every launch: with start-at-login and outputs-on-their-screens it comes back by itself.
- The patch canvas no longer draws under the Shader Editor's preview, and a comp's picture shows in the Inspector.

## 0.2.90 (2026-10-21)

- **Follow another Phosphor.** Network › Follow another Phosphor: type the address of a Phosphor running a
  Session, and this one plays its show in sync (project, beat, clips, fades, movie positions) on its own outputs
  and mapping. Use it for several machines or projectors in one show. The media must be at the same place on
  each Mac.
- **Audience play: limits per control and per player.** In the Control Surface editor each control has
  **Guests can use it** (off hides it from guests' phones) and a guest range, so guests can push a strobe only up
  to 30%, say. With several players at once, each place can have its own page (Network › Audience play). The
  limits are checked on the Mac, not just on the phone.

## 0.2.89 (2026-10-21)

- **Clips start the moment you trigger them.** Movies now open in the background instead of holding up the app,
  and the next few movie clips in the bank on screen are kept open, paused on their first frame. A triggered clip
  shows on the very next frame instead of about a tenth of a second later, which matters on beat-quantised cuts.
  **Preload** in the launch bar turns it off.
- **Patches and your own shaders keep their settings when you edit them.** Layers already using one keep each
  setting, its modulation and its MIDI/OSC mapping by name, even when blocks are added or removed.
- **Scan the stage with a camera.** In the Mapping Studio, **Stage photo › Scan the stage with a camera…**: the
  projector shows black, white and striped pictures while a camera watches, and Phosphor draws the stage as the
  projector sees it. That becomes the output's stage photo, so you can draw slices straight onto the real objects
  (MadMapper's spatial scanner). Any layer showing a camera works; it can stay hidden. "The output itself" runs a
  test without a camera.
- **Comps: canvases of their own.** **+ Comp** (above Clips, Layers and the Mixer) makes another comp. A comp has
  its own layers, its own size, its own effects, opacity and blend, and the master mixes the comps in order.
  - The tabs pick which comp the Clips, Layers and Mixer panels show. Right-click a tab to rename it, open its
    settings, preview it in a window, change the order or delete it (its layers move to the first comp).
  - In the Inspector a comp can have its own size (fit, fill or stretch into the master), its own effects, and
    can stay out of the master mix to go only to outputs of its own (the screen button, as for layers).
  - A layer can show a comp (source: Feed › Comp), a mapping slice can show a comp, and a layer's right-click
    menu has **Move to comp**.
  - Comps share one tempo. Surprise acts on the comp you have open; column triggers (keys 1–9, mapped buttons)
    fire every comp, so a controller does the same whichever comp is on screen. Solo works inside a comp.
  - Projects from earlier versions open as one comp, "Main", and look the same.
- **FFGL plugins (Mac).** Phosphor now loads FFGL effect and source plugins, the kind Resolume and VDMX use.
  It looks in ~/Library/Graphics/FreeFrame Plug-Ins, /Library/Graphics/FreeFrame Plug-Ins, Resolume's Extra
  Effects folders and Phosphor's own FFGL folder (right-click "FFGL: …" under the Effects list to open it).
  Plugins show in Effects and Sources under **FFGL**, and their settings work like any other: MIDI, modulation,
  cues. Each new plugin is tried out in a separate process first, so a broken one can't take Phosphor down when
  it loads. A slow plugin updates less often instead of slowing the show. Not yet: plugins that mix two pictures,
  text and file settings, and Windows.
- Fixed a crash in development builds when showing hardware-decoded video.

## 0.2.88 (2026-10-21)

- **Patches: build an effect from blocks, no code.** In the Shader Editor, **+ Patch** opens a canvas: right-click
  to add blocks (pictures like noise, rings and stripes; colour; warps like kaleidoscope, ripple and swirl; blur,
  glow and edges; ways to mix two pictures; and **Last frame** for trails), then drag from one block's dot to the
  next to wire them. The result is a normal effect or generator: it shows in the Effects list, and every block
  slider becomes one of its settings, so you can map it to MIDI and modulate it. **Code** shows the shader the
  blocks make, and **Turn into code** carries on from there by hand.
- Your own shaders can now use extra passes and trails (`@pass` and `@feedback`) together.

## 0.2.87 (2026-10-21)

- **Fix:** 0.2.86 froze when the Mapping Studio opened. Fixed, and every release now opens each panel once
  before it ships.

## 0.2.86 (2026-10-21)

- **Fixes:**
  - a mapping with Delay dropped most fader moves while soft takeover was on;
  - "Only selected" in the Mapping Studio now ends when you leave the Studio, and selecting a mask no longer
    blacks the output;
  - image sequences no longer count as missing media in imports;
  - the hidden "._" files macOS leaves on external drives no longer turn into a broken extra clip;
  - a daily cue doesn't fire again when the computer's clock is set back a little;
  - Remove Background does no work on Windows (where it can't cut yet) or in effect previews.
- **Several "show only inside" masks work together:** two inverted masks now show two windows of the picture.
  Before, each one blacked out the other's window and the output went black.
- **Scopes** (as in VDMX): a new Scopes panel (View › Panels, or the Outputs area) shows a waveform, RGB parade,
  vectorscope or histogram of the mix or any layer, for matching colours between cameras, clips and projectors.
  It only measures while it's on screen.

## 0.2.85 (2026-10-21)

- **Effects on one slice** (MadMapper's surface FX): select a slice in the Mapping Studio; under its settings,
  Effects › Add effect works as on a layer. The effects run on that slice's part of the picture only: pixelate one
  window of a building, invert one panel. Their settings can be mapped and modulated like any effect.
- **One look for small switches:** solo "S", the "3 fx" bypass badges, the show/hide eyes on masks and slices and the
  invert arrow in mappings now share the same two small switch styles.
- **Daily cues** (for installations): a cue can fire every day at a set time on the computer's clock ("daily"), on
  the days you pick: lights up at 18:00, off at 02:00, weekdays only.
- **Corner pin a layer** (VDMX's quad mode): Inspector › Transform › Corner pin, then drag the layer's four
  corners right on the preview (or type them). The picture stretches into them with perspective, inside the mix,
  before any output mapping. Each corner can be mapped to a knob or OSC (right-click its field › Learn).
- **Your own shaders with several passes:** in the Shader Editor, `// @pass name 0.5` lines add steps drawn before
  the main one (at a size you choose), each read by the next with `read_name(uv)`, for blurs, glows and bloom.
  New template: "Passes: soft glow".
- **Image sequences** (as in VDMX): drop a folder of numbered pictures (frame_0001.png, frame_0002.png…, as rendered
  from After Effects, Blender or TouchDesigner) and it comes in as one clip that plays at 30 frames a second, loops,
  scrubs and syncs like a movie. Watched folders and Export Show handle them too.
- **Only selected** (MadMapper's "show only selected"): a Mapping Studio switch that makes the output (and the
  projector) show only the selected slice(s) while you line them up.
- **Mask soft edge inside or outside:** with a soft edge on a mask, Fade picks where it sits: centred on the
  outline, all inside the shape, or all outside it (as in MadMapper).
- **Flip a slice's picture** (as in MadMapper): a slice's Input region has Flip left-right and Flip upside down
  (both = turned half round), for rear projection or a projector hung from the ceiling.

## 0.2.84 (2026-10-21)

- **Video tracking** (as in VDMX, Mac): Modulators › Video tracking. Pick what to watch (usually a camera layer),
  switch on Faces, Hands and/or Body, and Phosphor follows them with Apple Vision: the face's position and size, the
  index fingertip and a thumb-to-finger pinch, the hips and both wrists. Each one is a modulator ("Track Face X",
  "Track Pinch"…) that any slider can follow: right-click it › Modulate. A small picture shows what it sees, with a
  dot on each find; Smoothing takes out the jitter. Nothing runs while it's off.
- **Remove Background** (Mac): a new effect (Key) that keeps the people in a picture and drops everything else,
  using Apple Vision. Put it on a camera layer and the person stands on whatever is underneath. Edge and Softness
  tune the outline; Invert keeps the background and cuts the people out instead.
- **Projector control (PJLink)** (as in MadMapper): Mapping Studio › select an output › Projector › Add its
  projector, type its network address (and its PJLink password, if it has one). Then Switch on, Switch off, close or
  open the Shutter, or Check how it is (on, warming up, cooling down, its name). Outputs › Projectors has All on /
  All off / Shutter for every projector at once, and each of those can be mapped to a pad or key. Works with most
  Epson, Panasonic, NEC, Sony, Optoma, Christie and Barco projectors.
- **Crossfader Auto-fade** (as in VDMX): Mixer › Auto glides the crossfader to the other side over the time beside
  it (2 s to start with); press again on the way to turn back. Auto can be mapped to a pad or key.
- **DMX in from a lighting desk** (as in VDMX): Controllers › DMX in. Phosphor listens for Art-Net and sACN, and
  any channel can drive any control like a MIDI fader: right-click a slider › Learn, then move the desk's fader.
  Phosphor's own LED output is never mistaken for a desk.
- **Delay on controller mappings:** Controllers › a mapping › On the way in › + Delay: each move of that knob or
  fader lands a set time later (with Smooth, it glides there).
- **Bezier handles on mask points** (as in MadMapper): right-click a mask point › Bezier handles, then drag either
  handle to bend the outline there (the other side mirrors it).
- Masks now redraw on the output straight away when you switch Smooth or a sharp corner.

## 0.2.83 (2026-10-20)

- **Slice groups** (as in MadMapper): in the Mapping Studio, right-click slices › Group these slices (or right-click
  the canvas with several selected). Groups are saved with the project, fold open and shut, and can sit inside other
  groups. Click a group to select all its slices; its eye shows or hides them all. Right-click a group to rename it,
  move it into another group, ungroup it, or delete it with its slices. Right-click a slice › Move to group.
- **Groups are controls:** a group's on/off and opacity can be mapped to MIDI, OSC, keys or a Control Surface
  button like a single slice (right-click the group › Learn, or pick "(group)" in any target list).

## 0.2.82 (2026-10-20)

- **LUT Mix** (as in VDMX): with two or more LUTs in your LUT folder, a LUT Mix effect blends between any two of
  them: pick LUT A and LUT B by name, slide Mix between them (or put an LFO on it), Amount over the picture.
- **Effect menus show their names:** ISF effects with a menu setting (a mode, a shape, a direction) now show a
  list of named choices instead of a number slider.
- **One look for switches and choices everywhere:** the remaining tick boxes for on/off settings are now the same
  lit switches as elsewhere, and the remaining rows of pick-one buttons (view, order, timing, alignment, loop
  length, "add to slider / sweep a range") are joined segments.
- **Mapping Studio in a narrow tab:** instead of squeezing three columns over each other, it shows Slices,
  Canvas or Properties one at a time (pick at the top).

## 0.2.81 (2026-10-20)

- **Preview windows as an XY pad** (as in VDMX): switch on XY in a preview window, then click or drag on the
  picture. Two modulators, "Preview 1 X" and "Preview 1 Y", follow the pointer; any slider can follow them
  (right-click › Modulate › pick one). Handy for moving a layer or an effect's centre by pointing at the picture.
- **Select several slices** (as in MadMapper): Shift-click slices on the canvas or in the list. Drag one to move
  them all, arrows nudge them all, and Properties sets on/off, lock, opacity and blend for all of them together.
- **Draw a mask by hand:** Mapping Studio › Draw mask, then drag around what to hide. It becomes a smooth mask you
  can edit point by point, invert or soften like any other.
- **Attach a mask to a slice:** a mask's Attached to setting makes it move and warp with that slice (it stays put
  if the slice is deleted).
- **Zoom to slice:** a Mapping Studio button that fills the view with the selected slice(s) for close-up work.

## 0.2.80 (2026-10-20)

Projection mapping, more like MadMapper:
- **Control slices from MIDI, OSC or a phone:** every slice's opacity and ON can be learned (right-click › Listen)
  or sent to `/phosphor/slice/<id>/opacity` and `/phosphor/slice/<id>/on`, and they can be locked like any control.
- **Cues remember the mapping:** a recorded cue now keeps every slice's on/off, opacity, corners and mesh points;
  GO glides the slices into place over the cue's fade (switch stage layouts between songs). Older cues leave the
  mapping as it is.
- **Edit a mesh line by line:** select a mesh point, then right-click the canvas to add a column or row beside it,
  or remove its column or row. The picture keeps its shape.
- **Copy a slice's look:** right-click a slice › Copy its look to another slice or every slice on the output
  (opacity, soft edges, colour, blend, shape; not its place or picture).
- **Fine-tune one point:** click a corner or mesh point (or press Tab) in the Mapping Studio. Tab and Shift-Tab
  go to the next and previous point, the arrows nudge it one pixel (Shift = 10), and Properties shows its X and Y
  in pixels to type. Esc goes back to moving the whole slice.
- **Snap to other slices:** with Snap on, a dragged point lands on another slice's point when it comes within a
  few pixels (otherwise on the grid), so neighbouring slices meet exactly.
- **Test patterns:** right-click an output's GRID: Grid, Test card (colour bars, grey ramp and a different colour in
  each corner, so a flipped projector is obvious), Colour bars, White, Checkerboard, Grey ramp.
- **Stage photo:** Mapping Studio › Stage photo: a photo of the real stage taken from the projector shows behind
  the slices while you line them up (never on the output).
- **Blend per slice:** Normal, Add, Screen or Multiply: overlapping slices can add up instead of covering.
- Fixed: preview windows could stay empty or show another window's picture after opening several.

## 0.2.78 (2026-10-20)

- **Preview windows** (as in VDMX): open as many as you like (+ on the Preview bar, View › New Preview Window,
  or right-click a layer or group › Preview in a window, or the screen button on an output). Each one shows the
  mix, the output window, one layer, one group, or one output after its mapping; pick it at the top of the window.
  Drag a preview anywhere, or detach it into its own window for a second screen.

## 0.2.77 (2026-10-20)

- **Capture a screen or another app's window as a layer** (as in VDMX): a layer's Live menu › Screens and
  windows. Full Retina size at up to 60 fps, straight to the GPU. macOS asks for the Screen Recording permission
  the first time (System Settings › Privacy & Security › Screen & System Audio Recording).
- **Step sequencer** (as in VDMX): Modulators › Step track adds a track of 4 to 64 steps that plays on the beat
  (a step lasts a bar, a beat, or 1/8, 1/16, 1/32). Draw levels, or switch on Gates and click steps on and off;
  Glide slides between steps; shift the pattern, Random, Clear. Add several tracks for a multi-track sequencer.
  Any slider can follow a track: right-click › Modulate › pick it. An old 8-step modulator turns into a track
  with More steps….
- **Crop a layer:** Inspector › Transform has Crop left / right / top / bottom; the cut stays with the picture
  when you move, scale or rotate it.
- **Play once, then clear:** a layer's autopilot has a new rule, Clear the layer: when the clip ends (or after
  the beats you set) the layer empties, with the launch fade if Fade is on.

## 0.2.76 (2026-10-20)

- **Buttons look like what they do.** Play, triggers, Surprise and Rec are solid, bold buttons. On/off switches
  (ON, Loop, Syphon, Link, Mute…) are pills with a light that glows when on. Pick-one choices (Cut / Fade,
  Now / Beat / Bar, Forward / Reverse, the media filters) are joined segments. Delete is red.
- **Type a slider's value:** right-click a slider and the value is already selected; type a number and press Enter.
- **Lock any control:** right-click › Lock. A locked slider, button, clip or column can't be changed by the
  mouse, a controller, OSC or a phone until you unlock it. Locked sliders turn grey with a lock.
- **Snap points:** hold Shift while dragging a slider to land on the ends, the default or 100%. Right-click ›
  Snap points to add your own (they show as small ticks).
- **Soft edge per side** (as in MadMapper): a slice can fade only the side that overlaps the next projector
  (Soft edge per side: left, right, top, bottom).
- **Lock a slice:** once it's lined up, Lock stops its points being dragged or nudged by accident.

## 0.2.75 (2026-10-20)

- **Game controllers** (as in VDMX): buttons, sticks and triggers on a connected gamepad (Xbox, PlayStation,
  Switch Pro and most USB / Bluetooth pads) are control sources like MIDI: press Listen in MIDI / OSC (or right-click
  a slider › Listen) and move a stick. Buttons act like pads; sticks and triggers like faders.

- **LUT colour grades as effects** (as in VDMX): drop `.cube` files (from grading apps, cameras or LUT packs) into
  *Help › Open LUT Folder*, and each one is an effect (category LUT) with an Amount slider at the next launch.

- **MIDI / OSC monitor:** the MIDI / OSC panel lists every message as it comes in (which knob, pad or OSC address,
  its value, and what it's mapped to), with pause and clear. No more guessing what a controller sends.
- **Value effects on mappings** (like VDMX's): the ~ button on a mapping adds *Smooth* (glide to each new value),
  *Steps* (snap to a few values), *Curve* (ease in or out) or a *Formula* such as `1 - $VAL`.
- **Data looper** (as in VDMX): a Hand modulator (the one a knob drives) can record what you do with the knob for 1
  to 16 beats, starting on the beat, then play it back looped in time with the music. Map anything to that
  modulator and your gesture repeats by itself.

- **Soft takeover for MIDI and OSC faders** (as in VDMX): a fader or knob only changes a value once it reaches it
  (or moves past it), so nothing jumps when the hardware was left somewhere else, or after you moved the value with
  the mouse, a phone or a Surprise. On by default; the switch is in the MIDI / OSC panel.

- **Stopping a collection is easy to find:** a stop button on the layer's strip in Clips (next to the collection's
  name) and in the collection's header ("Playing on: … Stop"). Putting any clip or file on that layer yourself
  also stops it following the collection.
- Fixed: Phosphor's own test runs could save their test project over yours when they finished (that's where
  "Show A" and a stray test clip came from). They now never touch your project or settings.

## 0.2.70 (2026-10-20)

- **Files you import land where you're looking:** importing while a collection is showing puts the files in that
  collection (before, they went to the bin only, so you didn't see them).
- **Watch folder… makes a collection** of that folder (named after it; new files join as they arrive), ready to
  *Play on* a layer, like a VDMX media bin page.
- **What's playing is marked** in the Media panel: an outlined tile with a ▶ (hover: which layers).
- **A layer playing a collection keeps the collection's name** instead of taking each file's name.
- Tidier Media panel: one row of actions, and inside a collection only *Random* (its *Play on* does the rest);
  folders show by name (full path on hover).

- **Stage:** a tab in the Outputs area that shows every output on one screen, live and as big as fits, with each
  output's name, size, GRID button and where it goes. Click one to pick it in the Mapping Studio.
- **Comps to outputs:** the "Shows on" screen button is on comps (groups) too, next to their name in the Inspector.
- Fixed: the alignment grid blacked out the output's picture underneath it.

## 0.2.68 (2026-10-20)

- **Alignment grid on any output:** the GRID button on an output (Outputs panel, or the strip in the Outputs
  area) draws cells, a centre crosshair, a circle, the diagonals and a border over that output, to line up a
  projector. Right-click it for the number of cells, line width and colour. The old per-slice pattern is now
  called "Slice test pattern".
- **Corners by number:** a slice's four corners can be typed or nudged as percentages in its settings, for
  fine alignment (arrow keys on the canvas still nudge the selected slice; Shift = 10 px).

- **Collections that work:** *New collection* in the Media panel makes one straight away (rename it in its
  header). Put files in it (right-click › Collections) or **Add folder…** so every clip in a folder, and new ones
  as they arrive, belong to it. **Play on ▸** a layer: the layer steps through the collection by itself, next /
  previous / random, when a clip ends or after so many beats (Inspector).
- **Send a layer or comp to an output:** the screen button next to a layer's name in the Inspector lists your
  outputs; tick one and the layer shows there as a slice of its own (shape it in the Outputs area), besides the mix.
- **Surprise says where:** the top button reads "Surprise · Layer 2" (the selected layer) or "Surprise · all
  layers", and its arrow lets you tick several layers and surprise just those.
- **Media open from the start:** the Library column (Media, Effects, Presets) is open in the Show area by default.

## 0.2.66 (2026-10-20)

- **Shader transition settings:** a shader transition's own controls (shadow, bounces, angle, colour…) are in the
  Inspector under the transition you picked, and each layer remembers them.
- **Readable effect names:** imported effects and transitions show proper names ("Book Flip", "Multi Pass
  Gaussian Blur") instead of file ids.
- **130 shader transitions for clip changes:** set a layer's (or every layer's) *Fade style* to *Shader
  transition* and pick one: cube, doorway, page curl, book flip, star wipe, ripple, glitch, burn, kaleidoscope,
  mosaic and more. They come from two free collections that now ship with Phosphor (Vidvox's ISF transitions,
  which didn't work before, and gl-transitions).
- **11 new VJ effects:** datamosh, pixel sort, flow smear, motion heat and other glitch effects from
  Daito Manabe's free isf-shaders-for-vj collection.
- **Import from Shadertoy:** in the Shader Editor, *Import from Shadertoy…*: paste a shader's code from
  shadertoy.com, name it, and it becomes a source (or an effect, if it reads the layer's picture). Check a shader's
  licence before sharing a show that uses it.

## 0.2.64 (2026-10-20)

- **Reverse and ping-pong are smooth:** switching a clip to reverse starts at once (no pause), looping backwards
  round to the end doesn't stop, and switching to ping-pong no longer restarts the clip. Checked on 42 clips and
  every kind of file on a 4,600-clip footage drive: no pause longer than one frame. Big ProRes 4444 and short MPEG
  clips play backwards properly now too.
- **Files that are slow backwards say so:** a file with almost no keyframes can't play backwards smoothly in any
  app; the playback line now tells you, and points to *Convert…*, which fixes it.
- **Footage drives from Windows PCs:** the hidden "._" files macOS makes on those drives are ignored (watch
  folders, drops, ISF folders), and old ISF 1 shaders convert.
- **AV1 on Windows** plays (it showed nothing before).

## 0.2.63 (2026-10-20)

- **Control Surface → other apps:** give any button, fader, XY pad or colour on your Control Surface a *Send OSC*
  address, and set *OSC out* (the other computer's address and port) at the top of the panel. Whenever it moves,
  from your Mac, a guest's phone, a MIDI knob or a modulator, its value goes to TouchDesigner, a lighting desk,
  Resolume or anything that speaks OSC. XY pads send x and y, colours red, green and blue.
- **327 more effects for everyone:** Phosphor now ships Vidvox's free ISF collection (blurs, glitches, colour,
  distortions, generators and transitions), so Windows and Macs without VDMX get all of them too.
- **Feedback in your own effects:** add `// @feedback` to a shader in the Shader Editor and `last(uv)` gives
  its picture from the frame before, for trails, echoes and tunnels. New template: *Feedback: trails*.
- **See who changed what from the web:** the Devices panel lists every change made from a phone or browser, with
  the time and who (a fader dragged from a phone is one line).

## 0.2.62 (2026-10-20)

- **Make your own effects from a template:** Shader Editor › *New from template…* gives eight starters that each do
  one thing, with every line explained: colour tint and hue spin, wave warp, pixelate, kaleidoscope, soft circle
  mask, glow, pulsing rings and noise clouds. Change a number and watch it update live; right-click any of its
  sliders › Modulate to make it move with the music.

## 0.2.61 (2026-10-20)

- **Take over from the web in one click:** when phones or browsers control the visuals (the remote page or
  audience play), you always see it: the status line shows how many can control and who is changing things right
  now, and the preview shows "… is controlling from the web · Take over". One click (or *Output › Web Control
  On / Off*, or a pad you map) and phones can only watch; their screens say the visuals are run from the booth.
  Click again to hand control back.
- **Mute all sound** is also in the Output menu.
- **Simpler sliders in every panel:** mapping, output colour, LEDs, modulators, text layers and the
  modulation editor use the new bars too.
- **Plays every common format:** checked 27 formats, from H.264, HEVC, AV1, VP9 with alpha and ProRes 4444 to DNxHR,
  Cineform, Hap and Resolume DXV.

## 0.2.60 (2026-10-19)

- **Mute all sound:** one speaker button on the MASTER bar (and in the Audio panel) silences everything Phosphor
  plays. Streams and recordings started while it's on have no audio. A red "Sound muted" in the status line
  reminds you; click it to turn sound back on. It can be mapped to a pad or key.
- **Even out volume levels:** turn it on in the Audio panel and every movie is brought towards the same
  loudness as it plays, so a loud clip and a quiet one come out about the same. Silence isn't boosted, and no clip
  is turned up or down more than 4×. Each layer's own volume still works on top.
- The Audio panel's input controls use the new simple bars too.

## 0.2.59 (2026-10-19)

- **Simpler sliders everywhere:** every level and effect control is now a plain bar with its name and value
  inside. Click or drag anywhere on it, double-click to reset, right-click to type an exact value, map it to a
  controller, or make it move. When something moves a bar on its own, a white line shows where it really is.
- **Hide a layer in one click:** an eye next to each layer's opacity shows or hides it.
- **Volume on each layer:** layers playing a movie have a volume bar (and mute) right under their opacity.
- **Surprise is easier to undo:** Surprise now says what it did; ⌘Z undoes it, and the new *Quick › Stop the
  Motion* keeps the look but stops everything moving with the sound. *Reset to default* on a control now also
  stops it moving (before, it kept jumping with the music).
- **Watch Folder works again:** a watched folder now fills the Media panel straight away and picks up new files
  every few seconds (it only loaded when another file window was used).
- **Show folders on disk:** each subfolder of a watched folder is its own group in the Media panel (for example
  one folder per show). Right-click a folder's title for **Random**, **Shuffle on a new layer** or **Fill clips of
  the selected layer** from just that folder.

## 0.2.58 (2026-10-19)
- **Smooth reverse and ping-pong:** movies play backwards at full frame rate and turn around without a hitch
  (before, phone and web videos stuttered backwards: each step re-decoded seconds of video).
- **Mute:** a speaker button on each layer (in its clip row and the Inspector) and *Mute sound* on any clip
  (right-click); muted clips show it on their cell. The volume is kept for when you unmute.
- **Media remembers how it played:** speed, direction, loop, effects and mute stick to the file, so the next
  time you drop it into a clip or onto a layer it comes back the same.
- **Play on ▸** (right-click a media file) picks the layer; hovering a file says where a click plays it.
- **Convert movies for the show:** right-click a movie › *Convert…* and pick Hap (smoothest on any computer,
  Mac or Windows), ProRes, or H.264 with every frame a keyframe (small, keeps sound), how much to compress, and
  an optional smaller size. You see the size it will take and the free disk space first; the converted file
  goes next to the original and this project switches to it. Converted files play backwards perfectly.
- **Show folders:** put files in a Collection, pick it in the Media panel, then *Random* plays one of them on
  the selected layer, or *Shuffle on a new layer* makes a layer that plays them in random order by itself.

## 0.2.57 (2026-10-19)
- **Three areas, each arranged your way:** Show, Outputs and Control. Drag any panel anywhere (split, stack, pop
  it out into its own window) and each area remembers its arrangement, also after a restart. View › Reset
  Layout brings the current area back to its default. (No separate "Custom" mode any more.)
- **Simpler Show:** a big preview, the clip grid under it, and each clip row starts with its layer: live
  picture, ON and opacity. Click a layer's picture to edit it in the Inspector.
- **Library button** (bottom right) opens media, effects and presets as a column on the left, and closes it.
- Asking for a panel (search, a hint) opens the area it belongs to.
- New installs start tidy (before, every panel landed in the first area).
- The Library's sort, group-by-folder and tile size are in one *View* menu (nothing cropped).

## 0.2.56 (2026-10-19)
- **Interface brightness:** the sun button in the top bar sets how dark or light Phosphor looks, from *Darkest*
  (for dark rooms and shows) through *Dark* (the usual) and *Dim* to *Soft light* and *Light* (daylight), or
  anywhere in between with the slider. Text and the accent colour adjust so everything stays readable.
- **Nothing hidden in narrow columns:** the Library's search box no longer sits under its buttons, the Effects
  list's controls wrap instead of running off the edge, layer cards always have room, and the side columns
  shrink with the window.
- **The Library no longer creeps wider by itself:** the side columns keep their width until you drag their edge.
- **Donate is GCarve orange** (same button as GCarve and the website).
- **A calmer top bar:** the areas come first; frame rate and CPU moved to the status line (they turn orange below
  50 fps); + Layer, the launch settings and the screen chooser are in the areas (Layers, the Clips toolbar, the
  Outputs strip), so they're not repeated up top.
- **Hide the side columns:** *Library* and *Inspector* toggles at the bottom right give the stage the whole
  width (handy on a laptop).
- **Session log:** Phosphor now keeps a plain log of each session (what was clicked, messages, warnings,
  errors) in its settings folder, only on your computer, so problems can be traced afterwards.
- The Syphon / Spout and NDI menu items work on Windows too.

## 0.2.55 (2026-10-19)
- **A new, unified layout.** Three areas instead of a wall of panels, each with the same shape: your
  **Library** on the left (media, effects and generators, presets), the area's stage in the middle, one
  **Inspector** on the right, and a status line along the bottom.
  - **Show** (⌘1): preview, layers, clip grid (with Mixer, Cues and the Shader Editor beside the clips).
  - **Outputs** (⌘2): every output live side by side, the selected one's mapping, and screens, streaming,
    session, LEDs, audio in and devices underneath.
  - **Control** (⌘3): MIDI / OSC and TouchOSC, your Control Surface, modulators.
- **See all your outputs at once while mapping:** a strip of every output, live, with its name and where it's
  going. Click to edit; double-click opens or closes its window; right-click to send it to a screen, identify,
  rename, switch it off or remove it.
- The status line shows, wherever you are, which outputs are on screen, LIVE / REC, the phone session,
  Syphon / Spout / NDI, and MIDI / OSC activity (click the outputs to jump to the Outputs area).
- Prefer arranging panels yourself? **Custom** in the top bar brings back the free layout, with your saved layouts.

## 0.2.54 (2026-10-19)
- **WebRTC streaming (WHIP):** a new destination in Stream & Record for under-a-second delay to WebRTC services
  (Cloudflare Stream, Dolby / Millicast, OBS, MediaMTX…): paste the WHIP address and token.
- **Save Control Surfaces and MIDI/OSC mappings by name** (the *Saved* menu in each panel), e.g. one mapping set
  per controller, and reuse them in any project.
- **Your named saves follow your account:** looks, layers, effect chains, venues, surfaces and mapping sets
  saved on one computer appear on the others you sign in on (nothing is overwritten or deleted by syncing).
- **Other Phosphors on the network** are listed in the Devices panel, each by its computer's name, with the
  address to send it OSC. Several Phosphors on one network no longer share the same name.
- After an unexpected quit, the recovery banner has a **Report it** button that fills in a bug report for you.
- Projects saved by a newer Phosphor open with a warning to update before saving over them.
- (The Mac download of 0.2.53 already had some of these.)

## 0.2.53 (2026-10-19)
- **The Mac app opens on macOS 14 and 15 again.** The video libraries inside recent downloads only ran on
  macOS 26, so on older Macs Phosphor wouldn't start. They're now built for macOS 14 and later, and the
  download is smaller (52 MB instead of 66).
- **Spout on Windows:** turn on Spout in Output & Stats and pick "Phosphor" in Resolume, TouchDesigner, OBS
  (Spout2 plugin), MadMapper or any Spout app. Off until you turn it on (it copies every frame while on).
- **Export Show on Windows:** packs the project, its media, shaders and mappings into a folder with its own .exe
  that runs on another PC without installing Phosphor.

## 0.2.52 (2026-10-19)
- **Stream health at a glance:** while you stream, the LIVE row shows a small graph of the last two minutes
  (bitrate, with red where frames were dropped or the stream fell behind) and a verdict: *Healthy · 4.1 Mbit/s*,
  or a warning to lower the bitrate or size when the connection can't keep up.
- **NDI on Windows:** send the composition as NDI and receive NDI sources as layers, with the free NDI Tools or
  NDI Runtime installed.

## 0.2.51 (2026-10-18)
- **Windows updates itself**, like the Mac: Phosphor downloads the new version, swaps its files and restarts.
  If anything goes wrong halfway, the old version is put back.
- **Masks with sharp corners:** on a Smooth mask, right-click a point and tick *Sharp corner*. The outline curves
  through the other points and makes a corner there, so arches, doorways and rounded booths are one mask.
- Every change is now built, tested and opened on Windows automatically before it ships.

## 0.2.50 (2026-10-18)
- **Windows (preview):** a Windows build is now on every release (Phosphor-windows.zip): unzip anywhere and run
  Phosphor.exe. Displays, projector outputs, start at login, CPU and memory stats and the crash watchdog work
  there. Not yet on Windows: Spout (the Windows Syphon), NDI, Export Show. Windows warns once because the app
  isn't signed: *More info*, *Run anyway*.
- Mapping Studio right-click on 3D surfaces: Calibrate, Clear pins, Choose another model.
- Show in Finder / Open folder now work the platform's way (Explorer on Windows).

## 0.2.49 (2026-10-17)
- **3D mapping calibration:** pin points of the model to where they land on the real object and Phosphor works
  out the projector's position and lens (from 6 pins).

## 0.2.48 (2026-10-17)
- Modulators can follow your hand: a MIDI knob, OSC or the phone remote, with a learnable fader.

## 0.2.47 (2026-10-17)
- Shared LFOs keep their phase when you change their speed (no jumps); new *Speed* chain mode.

## 0.2.46 (2026-10-17)
- **Modulators:** shared LFOs and audio followers in one panel. Any control can follow one (Modulate menu), and
  modulators can be chained (multiply, add, min, max), VDMX-style. The separate VDMX workspace is gone.

## 0.2.45 (2026-10-17)
- **Mapping Studio right-click menu** on the canvas: mesh warp sizes, slice shape, fill, line, 3D model,
  duplicate, delete, add a slice here, view options.

## 0.2.44 (2026-10-17)
- **3D projection mapping:** load an OBJ model as a surface and project onto it through a virtual projector,
  with a wireframe on the canvas.

## 0.2.43 (2026-10-17)
- Update button in the top bar beside Donate when a new version is out; if installing fails, it offers the
  download instead.

## 0.2.42 (2026-10-17)
- **Moving heads (DMX):** colour taken from the picture, smooth 16-bit pan/tilt, beat-timed circle, figure-8 and
  sweep motions; 8, 9 and 10 channel layouts.

## 0.2.41 (2026-10-17)
- ISF shaders with curve inputs get five editable heights.

## 0.2.40 (2026-10-17)
- ISF shaders with their own vertex stage now work: all 327 shaders in the standard ISF collection import.

## 0.2.39 (2026-10-17)
- Hap Q Alpha movies play on the GPU.

## 0.2.38 (2026-10-17)
- The Session (phone remote and audience play) is reachable at **phosphor.local**; an easy-to-type address in the panel.

## 0.2.37 (2026-10-17)
- New effect: **Motion Flow** (displace by movement, show the flow, or keep only what moves).

## 0.2.36 (2026-10-17)
- New generators: **Spectrum Bars** and **Waveform**, drawn from the live audio.

## 0.2.35 (2026-10-17)
- ISF shaders that need high-precision buffers render correctly.

## 0.2.34 (2026-10-17)
- Output & Stats shows how long the GPU takes per frame.

## 0.2.33 (2026-10-17)
- **Line slices** for mapping: a path with a width, for LED strips, edges and outlines.

## 0.2.32 (2026-10-17)
- Streaming: Secure (RTMPS) option for YouTube and Twitch, on by default.

## 0.2.31 (2026-10-17)
- Streaming: send to a second destination and record a copy at the same time.

## 0.2.30 (2026-10-17)
- Recording: HEVC (H.265) files, smaller at the same quality.

## 0.2.29 (2026-10-17)
- Devices panel: a level meter per audio input.

## 0.2.28 (2026-10-17)
- **Start at login** for installations that should run by themselves.

## 0.2.27 (2026-10-17)
- Phone remote: Auto show toggle and Surprise button.

## 0.2.26 (2026-10-17)
- A welcome card on first launch.

## 0.2.25 (2026-10-17)
- Auto show makes every layer react to the music.
- Help › Open ISF Shader Folder.

## 0.2.24 (2026-10-17)
- Streaming and recording work without installing FFmpeg (it ships inside the app).

## 0.2.23 (2026-10-17)
- Beat detection: fast tempos (170 BPM) no longer read as half speed; off-beat kicks no longer move the downbeat.

## 0.2.22 (2026-10-17)
- Beat detection is steadier.

## 0.2.21 (2026-10-17)
- Better downbeat (bar start) detection.

## 0.2.20 (2026-10-17)
- WAV and AIFF clips play their sound (they were silent).

## 0.2.19 (2026-10-17)
- **Automatic beat detection** follows the music's tempo and finds the downbeat, on by default.

## 0.2.18 (2026-10-17)
- **Auto show:** moves to the next column every few bars and throws a Surprise on drops.

## 0.2.17 (2026-10-17)
- Visuals react to the music Phosphor itself plays, automatically when no other audio source is chosen.
- Match the beat is on by default for new projects.

## 0.2.16 (2026-10-17)
- Imported shaders compile in the background: no hitch the first time one is used.

## 0.2.15 (2026-10-17)
- ISF shaders that read audio (waveform and spectrum) react to the live sound.
- On the Mac, the File / Edit / View menus live only in the macOS menu bar.

## 0.2.14 (2026-10-17)
- Sound files play as clips.

## 0.2.13 (2026-10-17)
- Audio source *Clip sound*: visuals react to the music in your clips.
- Fixed a crash with some imported shaders (Radial Spectrogram).

## 0.2.12 (2026-10-17)
- New fade style: switch clips **through any effect**.

## 0.2.11 (2026-10-17)
- Render rate: display rate, 60 fps (default) or 30 fps; Phosphor uses less power.

## 0.2.10 (2026-10-17)
- More ISF shaders import (324 of 327).

## 0.2.9 (2026-10-17)
- Match the beat leaves clips alone when their rhythm is too weak to trust.

## 0.2.8 (2026-10-17)
- Audience play: a rest between turns. More MadMapper materials import.

## 0.2.7 (2026-10-17)
- Most MadMapper materials import (39 of 42).

## 0.2.6 (2026-10-16)
- Session over HTTPS too, so phones allow the microphone.
- Match the beat picks a natural speed from the music's tempo.

## 0.2.5 (2026-10-16)
- Match the beat shows the tempo it found (with ÷2 / ×2), can match every movie in a row, and can match new
  movies automatically. First MadMapper materials import.

## 0.2.4 (2026-10-16)
- **Match the beat:** Phosphor finds a movie's visual rhythm and lines its hits up with the music's beats.

## 0.2.3 (2026-10-16)
- **Lock Controls** (player mode, optional password), audience play with several players and names, more ISF
  shaders, 60 shader parameters.

## 0.2.2 (2026-10-16)
- All clip columns in one scrolling grid; QR code colour styles; audience guests keep their place if they reconnect.

## 0.2.1 (2026-10-16)
- QR card with a title and address; a thanks moment on the guest page.

## 0.2.0 (2026-10-16)
First public release of Phosphor, a free VJ and projection-mapping app.
- Layers, groups, clips, effects, generators, text, transitions, presets and cues.
- Projection mapping: slices, mesh warp, masks, soft-edge blending, several outputs, venues, Mapping Studio.
- Movies with sound, Hap on the GPU, zero-copy hardware video, Hap recording.
- ISF shaders, MadMapper materials, Resolume and VDMX project import.
- Control: MIDI, OSC (with OSC Query), TouchOSC layouts, Control Surface, phone remote, audience play by QR code.
- Sync: Ableton Link, MIDI clock, MIDI and audio (LTC) timecode.
- Outputs: Syphon, NDI in, streaming and recording, LED / DMX pixel mapping (Art-Net, sACN).
- Quick visuals (Surprise, Shuffle, React, Instant Set), donate, bug reports and in-app updates.
