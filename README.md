# Phosphor

Website: https://phosphorfx.com

A real-time VJ / live-visuals app: VDMX-style modular interface, MadMapper-style projection mapping and LED pixel mapping,
one engine for macOS, iPadOS/iOS and the browser. Rust + wgpu (Metal natively, WebGPU on the web) + egui.

- `crates/phosphor-engine`: platform-independent render graph, WGSL shaders, project model
- `crates/phosphor`: the app (dockable UI, output window, benchmark mode)
- `docs/VDMX-REVIEW.md`: teardown of VDMX6 Plus · `ROADMAP.md`: evaluation + TODO list

## Install (macOS)

Download `Phosphor-mac.zip` from the [latest release](https://github.com/fillinthe/phosphor/releases/latest),
unzip, drag Phosphor to Applications. **First launch:** macOS says it can't verify Phosphor (the app isn't
notarized). Click Done, open System Settings → Privacy & Security, scroll down and click **Open Anyway**; that's
once. (Terminal alternative: `xattr -dr com.apple.quarantine /Applications/Phosphor.app`.) After that Phosphor
updates itself (Help › Check for Updates…).

From source:

```bash
scripts/bundle-mac.sh   # builds, signs, installs /Applications/Phosphor.app
```

## Run from source (macOS)

Needs Homebrew FFmpeg 9 for video playback: `brew install ffmpeg pkg-config`.

```bash
cargo run --release -p phosphor
```

Plays images and any common video (H.264/HEVC/ProRes/VP9/AV1 hardware-decoded, GIF, Hap, alpha
formats). The **~** button next to any slider drives it from audio (Audio panel) or an LFO. The **MASTER**
card at the top of Layers holds effects that run over the whole mix. **Mapping** adds corner-pinned
surfaces you drag into place (press **E** in the output window to edit them on the projector). **MIDI / OSC** panel: pick a
target, press Learn, move a knob (or send from TouchOSC: UDP to this Mac, port 8000). **Stream & Record**: YouTube/Twitch/RTMP/SRT
or a file, hardware-encoded, no OBS needed. **Clips**: drop media into the grid, click cells or press 1–9
to trigger columns with per-layer transitions; Tap sets the tempo, Quantize snaps triggers to the beat.
**Session**: start it, share the link — viewers watch in a browser and can send their music back
(Audio panel → "Session viewers").

Drag images onto the window or use **Open…** on a layer. Panels can be dragged by their tabs to
rearrange; **Reset Layout** restores the default. **Open Output** opens the output window; drag it
to a projector and press **F** for fullscreen.

## Benchmark

```bash
target/release/phosphor --bench 20 --gen plasma --gen rings --image media/test/testcard.png \
  --fx kaleidoscope --size 3840x2160 --snapshot /tmp/out.png
```

Prints one JSON stats line per second and a `SUMMARY` line.

## Test

```bash
cargo test   # includes WGSL validation of every shader, no GPU needed
```

## License

GPL-3.0-or-later (see `LICENSE`). Phosphor links GPL libraries (FFmpeg's GPL build, Ableton Link), so the app is GPL too:
use it, change it, share it, and share your changes under the same terms. `vendor/eframe` keeps its MIT/Apache-2.0
licence; Syphon's licence is in `vendor/Syphon-LICENSE.txt`.
