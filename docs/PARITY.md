# Phosphor vs VDMX, Resolume Arena and MadMapper

What each app does that a VJ / mapper reaches for, and where Phosphor stands. ✅ done · 🔨 in progress · ⬜ to do ·
➖ not planned (and why). Newest work is also in NOTES.md.

## Playback & clips (Resolume's core)
| Feature | Who has it | Phosphor |
|---|---|---|
| Clip grid (layers × columns), column triggers, keyboard / MIDI / OSC triggers | Resolume, VDMX | ✅ |
| Decks / banks of columns | Resolume | ✅ banks A–D |
| Transitions: crossfade, wipe, dip, blend modes (Add, Screen, Subtract…) | Resolume | ✅ per layer + launch bar |
| Quantized launch (beat / bar), BPM, tap, resync, MIDI clock in | all | ✅ |
| **Movie sound** (clip audio, layer + master volume, follows opacity, crossfades in transitions) | Resolume, VDMX | ✅ |
| Speed, reverse, ping-pong, in/out points, beat loops, scrubbing | all | ✅ |
| Hap / Hap Q / Hap Q Alpha / Hap R on the GPU, hardware H.264/HEVC/ProRes | all | ✅ |
| Autopilot / follow actions (next / previous / random clip after the clip ends or N beats) | Resolume | ✅ |
| Clip trigger modes (normal, toggle, flash, piano) | Resolume | ✅ |
| Per-clip settings (speed, direction, in/out, BPM sync, effects stored on the clip) | Resolume | ✅ |
| BPM sync (clip length = N beats, follows tempo) | Resolume | ✅ |
| **Match the beat**: the movie's own visual rhythm detected and locked to the tempo (loop in + beats set) | — | ✅ (beyond the others) |
| Ableton Link | Resolume, VDMX | ✅ |
| Cue list (fades, follow-on, GO/back mappable) + timecode (show clock, MIDI timecode, LTC audio) | Resolume Arena, MadMapper | ✅ |
| Audio playback of an audio-only file as a layer (sound + visualiser) | Resolume | ✅ sound files are clips; visualiser = any reactive generator with Audio → Clip sound |
| Visuals react to the clips' own sound (no mic, cable or loopback) | Resolume (audio FFT from clips) | ✅ Audio panel → "Clip sound" |
| Automatic tempo, beat and downbeat from the music (no tapping) | Resolume (BPM from audio) | ✅ on by default; checked 92–170 BPM |
| Auto show: next column every N bars, Surprise on drops | — | ✅ launch bar |

## Speed (getting visuals up fast)
| Feature | Who has it | Phosphor |
|---|---|---|
| Randomize a parameter / effect | Resolume (per parameter) | ✅ Shuffle a whole layer (⌘J, mappable) |
| One-click random look for the whole show | — | ✅ Surprise (⌘R, mappable pad) |
| Instant audio/beat motion on a layer | — | ✅ React (⇧⌘J) |
| Fill a clip row with variations / ready-made set | — | ✅ Fill · Instant Set |
| Command palette for everything | — | ✅ ⌘K |

## Mixing & layers
| Feature | Who has it | Phosphor |
|---|---|---|
| Layers with opacity, 10 blend modes + 3 masks, solo, bypass | all | ✅ |
| Layer transform (position, scale, rotation, stretch, flip; modulatable) | all | ✅ |
| Layer corner pin (quad), corners mappable | VDMX | ✅ |
| Crop per layer | VDMX | ✅ |
| Layer groups (mixed first, own effects/opacity) | Resolume | ✅ |
| Comps: several canvases, own size/layers/effects, mixed by the master, sent to their own outputs, fed into other comps | VDMX, Resolume | ✅ (0.2.89) |
| A/B crossfader | Resolume, VDMX | ✅ |
| Crossfader auto-fade | VDMX | ✅ |
| Layer as mask / track matte (brightness, inverted, alpha) | Resolume, VDMX | ✅ |
| Layer source = another layer / group / the program (feedback, re-routing) | VDMX | ✅ |
| Text layers | all | ✅ |
| Generators (shaders), ISF import (341/406 shaders) | all | ✅ |
| Effects: 34 built in + ISF, multi-pass, modulation on every parameter | all | ✅ |
| LUTs (.cube) and LUT Mix | VDMX | ✅ |
| Remove background (people cut out), video tracking (face / hands / body as data) | VDMX | ✅ Mac (Apple Vision) |
| Your own shaders: editor, templates, feedback, several passes | VDMX, Resolume (Wire) | ✅ |
| Image sequences as clips | VDMX, Resolume | ✅ |
| Scopes (waveform, parade, vectorscope, histogram) | VDMX | ✅ |
| FFGL plugins | Resolume, VDMX | ✅ Mac: effects, sources (0.2.89) and mixers as layer blends (0.2.92) |

## Control (VDMX's core)
| Feature | Who has it | Phosphor |
|---|---|---|
| Dockable panels, layouts, detach to windows | VDMX | ✅ |
| Control surfaces you build (buttons, faders, XY, colour, pages) | VDMX | ✅ |
| LFO / audio / step sequencer / random / envelope → any parameter | VDMX | ✅ |
| MIDI + OSC learn, feedback, MIDI out, keyboard | all | ✅ |
| Gamepads, DMX in (lighting desk), soft takeover, value effects (smooth, steps, curve, formula, delay) | VDMX | ✅ |
| Data looper, step sequencer tracks, preview windows as XY pads | VDMX | ✅ |
| Lock a control, snap points on sliders | VDMX | ✅ |
| Phone / tablet remote with no setup | — | ✅ (beyond the others) |
| OSC Query, TouchOSC layout export | VDMX | ✅ |
| Presets: looks, layers, chains, venues | all | ✅ |
| Cues with fades, follow-ons, timecode (MTC / LTC / show clock), daily schedules | MadMapper, VDMX | ✅ |

## Mapping (MadMapper's core)
| Feature | Who has it | Phosphor |
|---|---|---|
| Quads with perspective, mesh warp, masks (polygon/circle/triangle, feather, invert) | MadMapper, Resolume | ✅ |
| Many outputs, many slices per output, saved screens, auto-open | all | ✅ |
| Slice shows the whole composition, a comp, a layer, a group or another output | Resolume, MadMapper | ✅ |
| Soft-edge blending, blend curve, per-output and per-slice colour | all | ✅ |
| Edit on the projector output, alignment grid, identify | all | ✅ |
| Venue presets | — | ✅ |
| DMX / Art-Net / sACN pixel mapping (LED strips and matrices) | MadMapper, Resolume Arena | ✅ + moving heads (8/9/10 ch, beat motions) |
| Curved mask edges (smooth curve through the points) | MadMapper, Resolume | ✅ |
| Ellipse / triangle / line slices (follow perspective and mesh) | MadMapper | ✅ |
| Slice input from another output | Resolume | ✅ |
| 3D projection mapping onto models | MadMapper | ✅ OBJ + virtual projector, point calibration |
| Slice groups (nested), multi-select, typed points, snap to other slices, lock, zoom to slice, only selected | MadMapper | ✅ |
| Effects on one slice (surface FX), per-slice blend modes, flip, copy look | MadMapper | ✅ |
| Masks: freehand, bezier handles, attached to a slice, soft edge centred / inside / outside | MadMapper | ✅ |
| Test patterns, stage photo, per-side soft edges | MadMapper | ✅ |
| Projector control (PJLink) | MadMapper | ✅ |
| Spatial scanner (camera sees the stage) | MadMapper | ✅ (0.2.89): stage photo from projector patterns + camera |
| Lasers | MadMapper | ➖ |

## I/O
| Feature | Who has it | Phosphor |
|---|---|---|
| Syphon in/out, NDI out, cameras, capture cards, network streams | all | ✅ |
| Screen / window capture as a layer | VDMX, Resolume | ✅ Mac |
| NDI in | all | ✅ |
| Record the output (H.264, ProRes, Hap, Hap Alpha) | Resolume, VDMX | ✅ |
| Stream to Twitch/YouTube, network viewers | — | ✅ |
| Spout | Windows apps | ✅ Windows |

## Projects
| Feature | Who has it | Phosphor |
|---|---|---|
| Save/open, undo, crash recovery, export a standalone show | — | ✅ |
| Import Resolume compositions (.avc) and VDMX projects (.vdmx5 media bin) | — | ✅ |
| Find missing media (relink by file name) | all | ✅ |
