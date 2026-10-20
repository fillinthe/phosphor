# Changelog

What changed in each Phosphor release, newest first. Downloads are on the
[releases page](https://github.com/fillinthe/phosphor/releases); Phosphor also finds and installs updates itself
(Help › Check for Updates…).

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
