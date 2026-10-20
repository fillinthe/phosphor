# Free code-made visuals we could add to Phosphor's library

Research date: 2026-10-20. Licences were checked at the source (LICENSE files on GitHub, site terms pages)
wherever possible. Anything I could not confirm is marked **(unverified)**. This is not legal advice.

Phosphor is GPL-3.0-or-later and already ships Vidvox's ISF-Files (MIT, ~370 files in `assets/isf`). That
set already contains **68 of the 125 gl-transitions**, so the counts below are for what is *new*.

## Top 5 (best licence x quality x ease)

1. **gl-transitions** (MIT): about 60 transitions we don't ship yet (cube, page curl, doorway, morph, ripple,
   glitch displace...). Same small adapter Vidvox already used for the other 68. Safe, quick, polished.
2. **Hand-picked Shadertoy shaders with an open licence + a Shadertoy importer**: Shadertoy has the best
   visuals in the world, but the default licence is non-commercial (CC BY-NC-SA 3.0). Bundle only the ones
   whose header says MIT / CC0 / public domain (Star Nest is one). Build the importer anyway: users can then
   paste any Shadertoy shader for their own shows. Small adapter for one-pass shaders, medium for multi-buffer.
3. **TiXL (Tooll 3) operator shaders** (MIT): a serious open-source motion-graphics tool. About 36 image
   generators, 67 image effects, plus particle and point systems. Show-quality. Written in HLSL for DirectX,
   so it's a medium port (each one rewritten to WGSL by hand or with a converter).
4. **lostjared/shaders** (MIT): 2,700+ GLSL video effects (mirrors, kaleidoscopes, warps, strobes, rainbow
   trails). Very easy adapter (rename 3 inputs). Quality is mixed: worth curating the best 50-100.
5. **Godot Shaders library, CC0 and MIT ones only**: about 2,300 shaders, roughly three quarters CC0/MIT.
   Game-oriented (transitions, dissolves, CRT, portals, skies), so show-readiness is mixed. Small adapter
   (Godot's `uniform float x : hint_range(0, 1)` maps neatly onto our `// @param`). Needs curation and a check
   that "CC0" uploads aren't secretly Shadertoy ports.

Honourable mention: **Milkdrop presets** (9,795 in projectM's "Cream of the Crop"). The most famous
audio-reactive visuals ever, but they have no licence at all (projectM just "assumes public domain") and need a
whole Milkdrop engine. Big job, grey licence. See notes.

**Plain warning about "NC" (non-commercial) licences.** The owner said non-commercial is fine because Phosphor
is given away. That works, with care: GPL lets *anyone* sell copies of Phosphor, and NC files can't go into a
sold copy. So NC shaders must ship as a separate folder with their own licence file and credits, never mixed
into GPL code, and our converted versions stay under the same NC licence (share-alike).

## All sources at a glance

| Source | What | Count (approx) | Licence | Can we bundle? | Format / effort | Quality |
|---|---|---|---|---|---|---|
| [gl-transitions](https://github.com/gl-transitions/gl-transitions) | Transitions | 125 (≈60 new to us) | MIT; 2 files BSD | **Yes** | GLSL `transition()` fn; small adapter | High, polished |
| [Shadertoy](https://www.shadertoy.com) | Everything; best scenes anywhere | tens of thousands (unverified) | Default CC BY-NC-SA 3.0; author may set another | **Only per-shader**: yes if MIT/CC0; NC pack with care; no if "all rights reserved" | `mainImage`; small adapter (1 pass), medium (buffers) | The best |
| [TiXL / Tooll 3](https://github.com/tixl3d/tixl) | Generators, effects, particles, raymarch | 414 HLSL files (~100 image ones) | MIT | **Yes** (check a few for Shadertoy credits) | HLSL + C# params; medium port | High |
| [lostjared/shaders](https://github.com/lostjared/shaders) | Video effects | 2,700+ (+182 compute) | MIT | **Yes** | GLSL 330, 3 renamed inputs; easy | Mixed, psychedelic |
| [Godot Shaders](https://godotshaders.com) | Game shaders, 2D effects, transitions | ~2,300 (≈1,320 CC0, ≈430 MIT in a 2025 snapshot) | Per shader: CC0, MIT, GPL-3, or "Shadertoy port" | **CC0/MIT/GPL yes**; ports no | Godot shading language; small adapter | Mixed |
| [Milkdrop via projectM](https://github.com/projectM-visualizer/presets-cream-of-the-crop) | Audio-reactive presets | 9,795 | **None**; projectM "assumes public domain" | Grey: widely shipped, never licensed | .milk; big job (engine) | Iconic |
| [Hydra](https://github.com/hydra-synth/hydra-synth) | Live-coding building blocks (osc, noise, voronoi, kaleid, modulate...) | ~55 GLSL functions | AGPL-3.0 | Yes legally with GPL-3 (AGPL rules then cover the web build) | JS-wrapped GLSL snippets; medium | Building blocks, not finished looks |
| [cables.gl](https://cables.gl) | Patcher with GLSL ops | 100+ image ops (unverified) | MIT (since 2024, per cables docs) | Yes (core ops) | GLSL inside JS ops; medium | Good |
| [daitomanabe/isf-shaders-for-vj](https://github.com/daitomanabe/isf-shaders-for-vj) | Datamosh, flow smear, pixel sort | 11 | MIT | **Yes** | ISF; drop-in | High (Daito Manabe) |
| [px-stream](https://github.com/darosh/px-stream) | Audio/MIDI-reactive ISF | 50 | MIT or CC BY 4.0 (repo) | **Some**: author's own (X.Darosh.*) yes; LYGIA- and Shadertoy-based ones no | ISF (extended); near drop-in | Good |
| [ProjectileObjects/MiscISFShaders](https://github.com/ProjectileObjects/MiscISFShaders) | Mixed ISF | 33 | MIT (repo) | **Some**: several are Shadertoy ports (IDs in names) | ISF; drop-in | Mixed |
| [three.js shaders](https://github.com/mrdoob/three.js/tree/dev/examples/jsm/shaders) | Post effects | ~50 | MIT | Yes | GLSL in JS; small | Basic; mostly duplicates what we have |
| [pmndrs/postprocessing](https://github.com/pmndrs/postprocessing) | Post effects (bloom, glitch, god rays...) | ~40 | Zlib | Yes | GLSL in TS; small-medium | Good, mostly duplicates |
| [hg_sdf (Mercury)](https://mercury.sexy/hg_sdf/) | 3D shape toolkit for raymarching | 1 library | MIT (unverified; seen in a port's copy) | Yes | GLSL library; building block | Demoscene-grade toolkit |
| [stegu/webgl-noise](https://github.com/stegu/webgl-noise) | Noise functions | library | MIT | Yes | GLSL; building block | Standard |
| [LYGIA](https://github.com/patriciogonzalezvivo/lygia) | Huge shader function library (has WGSL!) | hundreds of files | Prosperity 3.0 (non-commercial; 30-day commercial trial) / paid Patron licence | **No** for the GPL app (extra restrictions clash with GPL) | GLSL/WGSL; easy, but licence blocks | Excellent |
| [The Book of Shaders](https://github.com/patriciogonzalezvivo/thebookofshaders) | Tutorial + examples | ~100 examples | "All rights reserved", no reuse in any project | **No** | — | Teaching |
| [Inigo Quilez](https://iquilezles.org) | Articles + Shadertoy | 500+ shaders | Algorithm code MIT; "artwork" shaders copyrighted, many say "you cannot distribute" | **Only** shaders whose header says MIT | `mainImage` | The best |
| [GLSL Sandbox](https://glslsandbox.com) | Gallery | tens of thousands (unverified) | Site code MIT; **no licence for gallery shaders** | **No** | GLSL; small adapter | Mixed |
| [ISF community editor](https://editor.isf.video) | ISF gallery | thousands (a 2020 scrape has 1,523) | No site licence; per-shader headers; many Shadertoy ports | **Only per-shader** | ISF; drop-in | Mixed to great |
| [grigM/ISF-shaders-collection](https://github.com/grigM/ISF-shaders-collection) | Scrape of the above | 1,523 | None | **No** | ISF | Mixed |
| [bareimage/ISF](https://github.com/bareimage/ISF) | Fractals/3D, persistent buffers | 62 | Mixed CC (NC) and MIT per file; no repo licence | NC pack only, file by file | ISF; drop-in | High |
| [Ethereios/ISF-shaders](https://github.com/Ethereios/ISF-shaders) (sleeplessmonk) | Fractal/3D ISF | 41 (zipped) | Repo says CC0, README says CC non-commercial; mostly Shadertoy remixes | **No** (contradictory, upstream authors' rights) | ISF in zips | High |
| [nwhetsell/isf-shaders](https://github.com/nwhetsell/isf-shaders) | Simulations (physarum, SmoothLife, fluids) | 27 | No repo licence; uses LYGIA | **No** (ask author) | ISF, float buffers | High |
| [livecode.demozoo.org](https://github.com/psenough/livecode.demozoo.org) | Shader Showdown / Jam archive | hundreds | None | **No** | Bonzomatic GLSL | Great, raw |
| [keijiro/ShaderSketches](https://github.com/keijiro/ShaderSketches) | KodeLife sketches | dozens | None | **No** (ask; he's usually generous) | GLSL | High |
| [Butterchurn presets](https://github.com/jberg/butterchurn-presets) | Milkdrop converted to WebGL | thousands | MIT on repo, but presets underneath unlicensed | Same grey as Milkdrop | JS+GLSL; big | Iconic |
| [Shader Park](https://github.com/shader-park/shader-park-core) | JS-to-shader 3D sculptures | gallery | Engine MIT; gallery licence unknown | Engine only | JS DSL; big | Good |
| [shaders.com](https://shaders.com/license) | Design platform | presets | Engine MIT; presets proprietary, no redistribution | **No** (presets) | — | Good |
| Synesthesia, TouchDesigner, Resolume, KodeLife, VEDA, Bonzomatic, MadMapper library, Magic | Tools, not open collections | — | Bundled content is the vendor's / per-author; MadMapper's online library states no licence | **No** bulk source found | — | — |
| [awesome-glsl](https://github.com/vanrez-nez/awesome-glsl) | List of learning links | — | MIT (the list) | n/a | Pointers only | — |

## Notes per source

### gl-transitions (top pick)
- What: the standard open collection of video transitions, used by FFmpeg filters, video editors, etc.
  125 files in `transitions/`. Phosphor already has 68 through Vidvox; about 57-62 are new (name match, approximate).
- Licence: repo LICENSE is MIT; "individual transitions may have their own licence in the header". Searched
  headers: no GPL / NC / CC found; two are BSD (StereoViewer, InvertedPageCurl), which is fine. `burn0` and
  `perlin` credit a Shadertoy noise snippet but are marked MIT. **Bundle: yes**, keep the author lines.
- Format: GLSL function `vec4 transition(vec2 uv)` with `getFromColor`, `getToColor`, `progress`, and
  uniforms with `// = default` comments. Vidvox's ports show the exact pattern (startImage, endImage, progress).
  A tiny converter script could do all of them at once.
- Quality: clean, broadcast-style. Missing examples worth having: **cube**, **doorway**, **morph**,
  **crosswarp**, **GlitchDisplace**, **DreamyZoom**, **pinwheel**, **squeeze**, **TilesWave**, **hexagonalize**.

### Shadertoy (best quality, licence per shader)
- Licence: Shadertoy's terms say authors own their shaders and choose a licence; if none is stated, the
  default is **CC BY-NC-SA 3.0 Unported** (quoted widely, e.g. by the Magic Music Visuals and Godot Shaders
  communities; the terms page refused my fetch, so the exact wording is **(unverified)** today).
  Inigo Quilez's newer shaders carry "I am the sole copyright owner... You cannot host, display, distribute or
  share this Work" (seen quoted, e.g. on shader Xds3zN), which means **no**.
- What to do: (1) build a Shadertoy importer (`mainImage` -> our main, `iTime` -> TIME, `iResolution` ->
  RENDERSIZE, `iChannel0..3` -> inputs, `iMouse`, `iFrame`; Buffer A-D -> ISF PASSES with persistent buffers,
  which Phosphor already supports). Users can use any shader privately. (2) Bundle only shaders whose header
  says MIT, CC0, Unlicense or public domain. (3) Optionally, a separate "Community (non-commercial)" pack of
  CC BY-NC-SA ones with full credits, kept apart from the GPL code.
- Standout examples (licences from memory of the shader headers, **verify each on the page**):
  - **Star Nest** by Kali (XlfGRj): header "License: MIT". Flying through a starfield fractal. Bundle-able.
  - **Raymarching - Primitives** by iq: MIT. A technique demo more than a show visual.
  - **Seascape** by TDM: CC BY-NC-SA 3.0. Ocean at sunset. NC pack only.
  - **Protean Clouds** by nimitz: CC BY-NC-SA 3.0. Volumetric clouds fly-through. NC pack only.
  - **The Universe Within** by BigWings: CC BY-NC-SA 3.0. Glowing neural net. NC pack only (px-stream has a port).

### TiXL (Tooll 3)
- What: open-source realtime motion-graphics app (5k GitHub stars, active today). In
  `Operators/Lib/Assets/shaders/`: 36 image generators, 67 image effects, 10 blurs, fluid fx, particles,
  point systems, mesh fx, raymarch templates. 414 HLSL files in total.
- Licence: **MIT** (repo LICENSE). A few shaders may credit Shadertoy authors; grep for "shadertoy" before porting.
- Format: DirectX 11 HLSL with a `cbuffer` of params; the param names/ranges live in the C# operator and `.t3` files.
  Medium port: one by one into our WGSL effect format (`// @param`). Particles/points need compute work: big.
- Standouts by name: **RaymarchFractal**, **KochKaleidoscope**, **image-fx-reaction-diffusion**,
  **color-physarum** (slime-mold simulation), **StarGlowStreaks**, **MandelbrotFractal**, **RyojiPattern1/2**
  (Ryoji Ikeda-style data patterns), **PixelSortGlitch**.

### lostjared/shaders
- What: 2,700+ fragment shaders and 182 compute shaders by Jared Bruni (Acid Cam). Mostly live-video effects:
  mirrors, kaleidoscopes, fisheye, twists, strobes, rainbow trails, fractal zooms. Very active (pushed 2026-10-18).
- Licence: **MIT** (repo). Bundle: yes.
- Format: GLSL 330, inputs `samp` (image), `time_f`, `iResolution`, `tc`. A tiny adapter renames them to ISF.
  Many also use mouse; map to a param.
- Quality: lots of near-duplicates (kale, kale2, kale3...) and some flashing content (the README warns about
  photosensitivity). Worth curating, not bulk-importing. Examples: **kale** family, **mirror-twist**,
  **warp_tunnel**, **rainbow_fractal**, **brot-zoom-mouse**.

### Godot Shaders (godotshaders.com)
- What: community library, ~60 pages of 39 = about 2,300 shaders (2D "canvas item", 3D, sky).
- Licence: uploader picks **CC0, MIT, GNU GPL v3**, or "Shadertoy port". A 2025 snapshot showed ~1,320 CC0,
  ~430 MIT, ~100 Shadertoy ports, ~18 GPL. CC0/MIT/GPL-3 are all fine for us. Caution: some "CC0" uploads are
  really ports of other people's work; check the credits on each.
- Format: Godot shading language (GLSL-like): `fragment()`, `UV`, `TIME`, `COLOR`, `TEXTURE`,
  `uniform float x : hint_range(0, 1)`. Small adapter for 2D ones; 3D ones don't fit.
- Quality: aimed at games, but a fair number of good 2D looks. Seen on the front page: **Wormhole / Blackhole**,
  **swirl portal**, **Broken glass hole**, **Circle wipe scene transition**, **Retro Postprocessor**, **CRT Lite**.

### Milkdrop / projectM (honourable mention)
- What: Winamp-era audio-reactive presets; projectM's default pack "Cream of the Crop" is 9,795 presets.
- Licence: **none**. projectM's LICENSE file says authors hold copyright but "it is safe to assume them to be in
  the public domain" and removes presets on request. Debian and others ship them, but it is not a real licence.
- Format: `.milk` files: per-frame equations + warp/composite shaders (HLSL-ish). Needs a full Milkdrop engine.
  libprojectM is LGPL-2.1 (GPL-compatible) but OpenGL, not wgpu. **Big job.** Butterchurn (MIT, WebGL) is a
  cleaner reference implementation.

### Hydra
- Engine and the GLSL snippets (`src/glsl/glsl-functions.js`, ~55 functions: osc, noise, voronoi, shape,
  kaleid, modulate*, colorama...) are **AGPL-3.0**. GPL-3 code may be combined with AGPL-3 code, but then the
  AGPL network rule applies to that part (relevant to Phosphor's web build). Simpler: copy the *idea*
  (chainable generators -> modulators) and write our own; the maths is tiny.
- Community sketches: licence **(unverified)**; don't bundle.

### cables.gl
- Went fully open source (MIT) in 2024; its FAQ says everything needed to run a patch is MIT. Image-compose ops
  contain GLSL, but parameters are wired in JS. Medium port per effect. Count of ops **(unverified)**.

### Small ISF repos
- **daitomanabe/isf-shaders-for-vj** (MIT, 2026): 11 "computer-vision" glitch effects (CV Flow Smear, CV Motion
  Heat Datamosh, CV Optical Feedback Tunnel, Motion Data Mosh). Drop-in. Recommended.
- **px-stream** (MIT or CC BY 4.0): 50 ISF; the author's own `X.Darosh.*` (10) are safe; `X.Lygia.*` use LYGIA
  (non-commercial); "The Universe Within" is a BigWings port (NC). Pick file by file.
- **ProjectileObjects/MiscISFShaders** (MIT): 33; names with Shadertoy IDs (e.g. `_WttBRl`, `_MsXGR2`) are ports,
  so their real licence is the Shadertoy author's. The VDMX/feedback/laser utility ones are fine.
- **bareimage/ISF**: 62, says each file is either CC (non-commercial) or MIT. NC pack only for the CC ones.
- **Ethereios/ISF-shaders**: GitHub shows CC0, README says CC non-commercial, and most are remixes of other
  people's Shadertoy work (iq, evvvvil, Jeyko). Don't bundle. (The Vidvox forum has a thread about people
  re-uploading others' shaders under MIT: a reminder that a repo label isn't proof.)

### Building blocks (not looks, but useful for writing our own)
- **stegu/webgl-noise** (MIT): simplex/Perlin/cellular noise.
- **hg_sdf** (MIT, **unverified** at source): 3D shapes and operators for raymarched scenes.
- **LYGIA**: the best library of all and it even has WGSL, but **Prosperity 3.0** = non-commercial only, with a
  30-day commercial trial. That's an extra restriction GPL doesn't allow, so it can't go into Phosphor's code.
  Fine for the owner's personal experiments; a paid licence exists.
- **glsl-tone-map** (MIT, has WGSL), **glsl-blend** (MIT), **glsl-film-grain** (MIT).

### Not usable for bundling
- **The Book of Shaders**: LICENSE says all rights reserved; can't be used in any product, even free ones.
- **GLSL Sandbox**: no licence on gallery shaders (site is also read-only now).
- **Inigo Quilez art shaders**: copyrighted unless the header says MIT.
- **Shader Showdown archive**, **keijiro/ShaderSketches**, **nwhetsell/isf-shaders**: no licence. Could ask the
  authors directly; several are known to say yes.
- **Synesthesia, TouchDesigner, Resolume, KodeLife, MadMapper library, Magic**: no open bulk collections found;
  their bundled content belongs to the vendor or each author.

## Suggested next steps
1. Convert the ~60 missing gl-transitions (one script, MIT, done in a day).
2. Add a Shadertoy importer (paste or drop a shader); ship a small curated MIT/CC0 Shadertoy pack with credits.
3. Port a first batch of TiXL generators (RaymarchFractal, KochKaleidoscope, reaction-diffusion, physarum).
4. Curate 50-100 lostjared and Godot CC0/MIT effects; drop in daitomanabe's 11.
5. Keep every bundled pack in its own folder with a LICENSE/CREDITS file; NC material in a clearly separate pack.

Sources: the GitHub repos linked above (LICENSE files read via the GitHub API); godotshaders.com/license;
cables.gl docs and CDM (2024, MIT announcement); shaders.com/license; Magic Music Visuals forum and CDM on
Shadertoy's non-commercial default; iquilezles.org legal notice as quoted in search results.
