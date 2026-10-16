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
| Hap / Hap Q / Hap R on the GPU, hardware H.264/HEVC/ProRes | all | ✅ (Hap Q Alpha on CPU) |
| Autopilot / follow actions (next / previous / random clip after the clip ends or N beats) | Resolume | ✅ |
| Clip trigger modes (normal, toggle, flash, piano) | Resolume | ✅ |
| Per-clip settings (speed, effects, in/out stored on the clip) | Resolume | ⬜ |
| Beat-snapped clip speed (clip length = N beats, "BPM sync") | Resolume | ⬜ |
| Ableton Link | Resolume, VDMX | ⬜ (licence check: Link is GPL or a commercial licence from Ableton) |
| SMPTE / MIDI timecode, timeline / cue list | Resolume Arena, MadMapper | ⬜ |
| Audio playback of an audio-only file as a layer (sound + visualiser) | Resolume | ⬜ |

## Mixing & layers
| Feature | Who has it | Phosphor |
|---|---|---|
| Layers with opacity, 10 blend modes, solo, bypass | all | ✅ |
| Layer groups (mixed first, own effects/opacity) | Resolume | ✅ |
| A/B crossfader | Resolume, VDMX | ✅ |
| Layer as mask / track matte (brightness, inverted, alpha) | Resolume, VDMX | ✅ |
| Layer source = another layer / group / the program (feedback, re-routing) | VDMX | ✅ |
| Text layers | all | ✅ |
| Generators (shaders), ISF import (341/406 shaders) | all | ✅ |
| Effects: 34 built in + ISF, multi-pass, modulation on every parameter | all | ✅ |

## Control (VDMX's core)
| Feature | Who has it | Phosphor |
|---|---|---|
| Dockable panels, layouts, detach to windows | VDMX | ✅ |
| Control surfaces you build (buttons, faders, XY, pages) | VDMX | ✅ (colour picker ⬜) |
| LFO / audio / step sequencer / random / envelope → any parameter | VDMX | ✅ |
| MIDI + OSC learn, feedback, MIDI out, keyboard | all | ✅ |
| Phone / tablet remote with no setup | — | ✅ (beyond the others) |
| OSC Query, TouchOSC layout export | VDMX | ⬜ |
| Presets: looks, layers, chains, venues | all | ✅ |

## Mapping (MadMapper's core)
| Feature | Who has it | Phosphor |
|---|---|---|
| Quads with perspective, mesh warp, masks (polygon/circle/triangle, feather, invert) | MadMapper, Resolume | ✅ |
| Many outputs, many slices per output, saved screens, auto-open | all | ✅ |
| Slice shows the whole composition, a layer or a group | Resolume, MadMapper | ✅ |
| Soft-edge blending, blend curve, per-output and per-slice colour | all | ✅ |
| Edit on the projector output, alignment grid, identify | all | ✅ |
| Venue presets | — | ✅ |
| DMX / Art-Net / sACN pixel mapping (LED strips and matrices) | MadMapper, Resolume Arena | ✅ (fixture profiles for moving heads ⬜) |
| Bezier mask edges | MadMapper, Resolume | ⬜ |
| Line / circle / triangle *surfaces* (not just masks) | MadMapper | ⬜ |
| Slice input from another output | Resolume | ⬜ |
| 3D projection mapping onto models | MadMapper | ➖ (large; later) |
| Lasers | MadMapper | ➖ |

## I/O
| Feature | Who has it | Phosphor |
|---|---|---|
| Syphon in/out, NDI out, cameras, capture cards, network streams | all | ✅ |
| NDI in | all | ✅ |
| Record the output (ProRes; Hap) | Resolume, VDMX | 🔨 ProRes/H.264 ✅, Hap ⬜ |
| Stream to Twitch/YouTube, network viewers | — | ✅ |
| Spout | Windows apps | ➖ (Mac first) |

## Projects
| Feature | Who has it | Phosphor |
|---|---|---|
| Save/open, undo, crash recovery, export a standalone show | — | ✅ |
| Import Resolume (.avc) and VDMX projects | — | ⬜ later |
