# Changelog

What changed in each Phosphor release, newest first. Downloads are on the
[releases page](https://github.com/fillinthe/phosphor/releases); Phosphor also finds and installs updates itself
(Help › Check for Updates…).

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
