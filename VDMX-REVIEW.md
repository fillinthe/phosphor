# VDMX6 Plus: teardown and a plan for a leaner clone

Reviewed: `/Applications/VDMX6 Plus.app` v1.2.13 (built with Xcode 26.3 / macOS 26.2 SDK), 2026-10-14.
Method: read-only inspection of the bundle (Info.plist, linked libraries, embedded frameworks,
resources, the bundled `Demo.vdmx6` project). VDMX was not launched or disassembled.

---

## 1. What VDMX is made of

| Fact | Value |
|---|---|
| Binary | Universal (x86_64 + arm64), Objective-C/Cocoa (`NSDocument` subclass `MyDocument`) |
| Minimum macOS | **13.2** |
| Bundle size | **628 MB** |
| Embedded frameworks | 31 (most of them Vidvox's own `VV*` kits) |
| Project format | XML plist (`.vdmx6`) with **binary plists nested inside** as `attribs` blobs |
| Bundled ISF shaders | 28 in the app; 365 in `/Library/Graphics/ISF` |

### Where the 628 MB goes

| Component | Size | What it is |
|---|---|---|
| `VVVuoKit` | **482 MB (77%)** | Embedded Vuo runtime + compiler, used for Vuo compositions as sources/FX |
| `ISFMSLKit` | 40 MB | Converts ISF (GLSL) shaders to Metal Shading Language at runtime (bundles a SPIR-V cross-compiler) |
| `VVNDIKit` | 28 MB | NDI network video in/out |
| `Sentry` | 26 MB | Crash reporting |
| Main binary | 19 MB | |
| Everything else | ~33 MB | MIDI, OSC, OSC Query, DMX/lighting, Blackmagic, Syphon, Hap, Wiimote, TouchDesigner, AU, DSP, UI toolkit |

### Rendering stack: half OpenGL, half Metal, both running

The app links **all of these at the same time**:

- `OpenGL.framework` + `VVGLKit`: legacy GL path (Apple deprecated OpenGL in 2018)
- `QuartzComposer.framework`: deprecated since macOS 10.15, still loaded for `.qtz` sources/FX
  (`qcSources`, `qcFX`, `qcCompModes`, plus `BuiltInPlayer.qtz` and `black.qtz` in Resources)
- `Metal` + `MetalPerformanceShaders` + `VVMetalKit` + `MSLCompModes`: the newer path
- `ISFMSLKit`: GLSL to MSL translation, cached under `~/Library/Application Support/VDMX/ISFBinCache`
- `Carbon.framework`: legacy APIs, still linked
- `WebKit`, `Vision`, `ScreenCaptureKit`, `CoreImage`, `QuickLook`, `Accelerate`

The shader resources show it too: there are GLSL `.vert`/`.frag` pairs (`rgb2422`, `bgra2yuva`,
`passthru`) and a `default.metallib`, and the composition-mode code generates GLSL at runtime
(`GLSLCompModeFragShader.fs`, `GLSLCompModeFragCompStep`, …) alongside Metal compute "scenes"
(`VVLUTMTLComputeScene`, `VVMTLHapDecoderComputeScene`).

### Project/document model (from `Demo.vdmx6`)

```
root
├── _projPages   → pages of clips (media bin). Each clip: name, mediaPath, type, size, icon PNG,
│                  uniqueID, and an opaque nested binary-plist "attribs" blob
├── _prsts       → presets
├── _vidins      → video inputs / displays (Display 0, Display 1, capture devices by UID)
├── projVers / pvn
```

---

## 2. Why it is heavier than it needs to be

These are inferences from the bundle structure, not profiler measurements. I haven't profiled
VDMX running yet; that's the next step if you want hard numbers.

1. **Two GPU APIs running at once.** OpenGL on Apple Silicon is a translation layer over Metal.
   Mixing GL and Metal means IOSurface handoffs, synchronization fences, and duplicated texture
   pools (`VVBufferPool` serves both).
2. **Runtime shader translation.** Every ISF shader goes GLSL → SPIR-V → MSL → compiled pipeline.
   That costs time on first load, the cache needs invalidating, and translated code is rarely as
   fast as hand-written MSL.
3. **Three or four plugin runtimes in one process.** Quartz Composer, Vuo, TouchDesigner, ISF and
   Core Image each have their own render model and texture-conversion hops.
4. **A huge resident footprint.** The Vuo runtime alone is close to half a GB, and anything linked
   that early loads at launch.
5. **Codebase age.** Objective-C, Carbon, and an NSDocument + binary-plist-in-plist format. It
   works, but every feature passes through layers of compatibility code.

---

## 3. Proposed clone: architecture

**Goal:** a VJ/live-video app for Apple Silicon and Intel Macs that renders everything through one
Metal graph, then later shares most of its code with iPadOS.

### Stack

| Layer | Choice | Why |
|---|---|---|
| Language | Swift 6 (engine hot paths in Swift + MSL; no ObjC++ unless needed) | One codebase for macOS and iPadOS |
| UI | SwiftUI shell + AppKit/UIKit where SwiftUI is slow (the clip grid, waveform/LFO editors) | Ports to iPad |
| GPU | **Metal only.** No OpenGL, no QC, no CoreImage in the render path | Removes the API mixing and conversions |
| Frame pacing | `CAMetalDisplayLink` (macOS 14+) / `CVDisplayLink` fallback, triple-buffered | Output stays locked to the display's refresh |
| Decode | AVFoundation + VideoToolbox hardware decode → `CVMetalTextureCache` (zero-copy) | No CPU copies of frames |
| Hap | Open-source Hap decoder: DXT/BC textures uploaded straight to Metal (BC is native on Apple GPUs) | Fastest VJ codec, near-zero CPU |
| Effects | **ISF** (open spec, MIT). Translate GLSL → MSL **once, at import**, store the `.metallib` | No translation in the render loop |
| Composition | One render graph: layers → blend → FX chain → outputs, all encoded into **one command buffer per frame** | Fewest GPU round-trips |
| Control | CoreMIDI, OSC (UDP via Network.framework), audio FFT via `AVAudioEngine` + Accelerate | Native and cheap |
| Interop | Syphon (BSD) on macOS; NDI as an **optional** plugin bundle | Keeps the base app small |
| Project file | Versioned JSON (`Codable`) in a document package | Diffable, readable, easy to migrate |

### Targets (not promises)

- App bundle **under 30 MB** (vs 628 MB)
- 4 layers of 4K Hap at 60 fps on an M1 with **under 10% CPU**
- Cold launch under 1 s; switching an effect never drops a frame

### macOS version support: a realistic answer to "all versions"

A modern Metal-only Swift app can't run on *every* macOS version. Realistic floors:

- **macOS 13 Ventura**, the same floor VDMX uses. Works on Intel and Apple Silicon.
- **macOS 14 Sonoma** if we want `CAMetalDisplayLink` and the newer SwiftUI/Observation APIs. This
  makes the code simpler.

Recommendation: **macOS 14+** for the first build. iPadOS 17+ uses the same SDK generation, so the
later port stays cheap.

---

## 4. Build phases

| Phase | Deliverable |
|---|---|
| **0. Spike** (do first) | Swift/Metal app: 2 layers of video (H.264/HEVC + Hap) blended with one ISF effect into a fullscreen output. Measure CPU/GPU/memory **side by side with VDMX** on the same clips |
| 1. Engine | Render graph, layer stack, blend modes, opacity/crossfader, multiple outputs/displays |
| 2. Media | Clip bin, thumbnails, AVFoundation + Hap, live camera input, image/text sources |
| 3. FX | ISF import pipeline (precompiled MSL), FX chains per layer and master, built-in effects |
| 4. Control | Sliders/buttons UI, LFOs/waveforms, audio FFT analysis, MIDI learn, OSC |
| 5. Interop | Syphon in/out, NDI plugin, recording to ProRes/Hap |
| 6. Projects | Save/load, presets, templates; optional **read-only importer for `.vdmx6`** (clip lists + media paths) |
| 7. iPadOS | Shared engine package; touch-first UI |

---

## 5. Legal note

Build it as a **clean-room** clone: copy the workflow and features, never Vidvox's code, binaries,
shaders, `.qtz` files, icons or theme. Open-source parts are fine to use under their licenses:
ISF spec and shaders (MIT), Hap (BSD), Syphon (BSD). NDI requires accepting the NDI SDK license.
Reading `.vdmx6` files for import is ordinary interoperability.
