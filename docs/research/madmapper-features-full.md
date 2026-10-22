# MadMapper 6: full feature list

Researched 2026-10-22 against **MadMapper 6.x** (release notes go up to 6.1.0). GarageCube, Mac and Windows.
Sources, most trusted first: the official manual at docs.madmapper.com/madmapper/6/ (walked chapter by chapter),
the madmapper.com product and features pages, the release-notes history in the manual, then press coverage (CDM).
Items marked **(unverified)** come from a non-official source or were inferred. Items marked **(5.x)** came in a 5.x
release according to the official release notes and were not seen described in the v6 manual pages.

Not found in any official source, so left out: Kinet (Color Kinetics) output, an iPad "MadMapper Remote" app, a
general 3D scene with cameras and several lights, motor/"kinetic" control other than Arduino servos. Control from a
tablet is done over OSC / OSCQuery (TouchOSC, Chataigne) or a Stream Deck.

**Products:** MadMapper (video + MadLight DMX/LED mapping) plus paid extensions **MadLaser** (lasers), **MadAI**
(text-to-shader, credits) and **MadProjectorControl** (PJLink), and the **MiniMad** hardware player.

---

## 1. Interface and workspace

Source: https://docs.madmapper.com/madmapper/6/2.-the-interface/interface-overview , https://docs.madmapper.com/madmapper/6/2.-the-interface/toolbar-and-workspace , https://docs.madmapper.com/madmapper/6/2.-the-interface/keyboard-shortcuts

- **Three-part window:** left tabs (media, surfaces, master), centre views, right inspector/media; redesigned for v6.
- **Object tabs:** Media (Cmd+1), Video surfaces (Cmd+2), Laser (Cmd+3), DMX (Cmd+4), Outputs (Cmd+5), Modules (Cmd+6), Master (Cmd+7).
- **List + Inspector per tab:** a list of objects on top, the selected object's settings below, with a splitter between them.
- **Separate Media tab:** undock the media pool so it stays visible on the right.
- **Input View:** shows the source media and which part of it (the UVs / input rectangle) each surface uses.
- **Stage (Output) Preview:** shows the surfaces as they land on the output, where you move and warp them.
- **View layout toggle:** show both views, only Input, only Stage, or rotate the view orientation.
- **Handles on both views:** scale, rotate and four corner handles on surfaces in Input and Stage; Tab cycles corners.
- **Snap to objects** (Alt+S): snap points and edges to other surfaces.
- **Lock Surfaces Movement** (Alt+B): stop surfaces from being moved by accident.
- **Object Isolation:** show only the selected objects while editing.
- **Object Isolation Groups:** save named sets of objects and switch between them from a dropdown.
- **Show surface names** in Input View and Output Preview (6.0).
- **Preview Mode:** preview selected surfaces, including ones hidden from the output (6.1).
- **Highlight background / highlight selection** shortcuts on the output for finding things on the wall.
- **Collapse the interface:** a button hides the left side, leaving the project tab.
- **Fullscreen toggle** for video outputs and a **Laser arm toggle** in the top bar.
- **Desktop mode / Full Screen mode** switch (Cmd+T / Cmd+U).
- **Contextual tooltip** area at bottom left, expandable.
- **Advanced Tools panel:** a dropdown picks Timelines, Control List, Library, Code Editor, Fixture Editor, MadLight Recorder, DMX Monitor or Notes.
- **Undock any advanced panel** into a floating, resizable window (multi-screen layouts); show/hide and maximize toggles.
- **Notes editor:** keep written notes inside the project (5.4).
- **Copy/paste settings:** "Copy Settings", "Paste Settings", "Reset to default" and "CUE ALL" on parameter group menus (5.2).
- **Copy/paste geometry:** copy output geometry, input geometry, FX settings and laser settings between surfaces (5.2).
- **Surface context menu:** Enable/Disable Perspective, Match Media Size, Match Media Ratio (5.6).
- **Multi-selection editing** of media, controls and modules (5.6).
- **Unlimited undo/redo.**
- **Application-wide color palette** usable on every color parameter, with custom palette import (5.6).
- **Windows display scaling** support for in-between values like 150% (6.0).
- **Window and splitter layout restored** on project reload (5.2).
- **Beta channel** sign-up from the account page.

## 2. Preferences and project settings

Source: https://docs.madmapper.com/madmapper/6/2.-the-interface/madmapper-preferences , https://docs.madmapper.com/madmapper/6/15.-troubleshooting/updating-from-madmapper-5

- **Two preference levels:** Application Settings (hardware, audio devices) and Project Settings (show-specific I/O and safety), new in v6.
- **Minimize at startup** and **Silent Mode** (no prompts, for unattended installs).
- **Open Project dialog at startup** toggle.
- **AutoSave** on/off and interval; **max number of automatic project backups**.
- **Reset Master settings on new project** / **restore Master settings from project** on load.
- **Expert Mode:** exposes advanced routing, laser and vectorization parameters.
- **Video input options:** auto device check, Blackmagic support, show all system devices, outdated-driver warnings, Canon EDSDK, Kinect ONE, Intel RealSense.
- **Re-enable discarded video inputs / audio devices** (6.0).
- **Control devices:** Stream Deck support, GamePad support, Makey Makey forwarding.
- **DMX display options:** show Master DMX color, always draw fixture pixels, auto-address duplicated/pasted fixtures.
- **Audio device settings:** input, analysis channels, output, sample rate, buffer size; WASAPI option on Windows (6.0).
- **Audio Data window:** live diagnostics of devices, channels and levels.
- **Engine info and options:** active GPU, double-buffered previews (less tearing), Retina high-quality previews.
- **Media defaults:** play movie audio by default on import; default resolution/FPS for image folders.
- **Startup options per project:** force fullscreen video on open, auto-arm lasers on open.
- **Engine FPS lock** (fixed rate or Auto) and **Media anti-aliasing** toggle.
- **Timeline options:** stop active timelines when switching banks; what legacy scenes record (fixtures, output geometry).
- **Workspace path** chosen at startup (5.2).
- **Log files** accessible from a menu (5.2).
- **One instance only** on Windows (6.0).

## 3. Project files, backup and portability

Source: https://docs.madmapper.com/madmapper/6/2.-the-interface/madmapper-project-file-structure , https://docs.madmapper.com/madmapper/6/12.-advanced/project-backup-and-recovery , https://docs.madmapper.com/madmapper/6/2.-the-interface/merging-projects

- **Project workspace folder (`.madproject`):** one self-contained folder per project, default in Documents/MadMapper (6.0).
- **`.mad` project files** inside hold surfaces, routing and show data, plus past saved versions.
- **Resources folder:** project-specific materials, generators, 3D objects and other assets live with the project.
- **Collect External Resources:** copies every referenced external file into the project folder.
- **Export Project To:** writes the whole project folder, optionally with all resources, somewhere else.
- **AutoSave snapshots** that don't overwrite the real project, with a 10-second cancellable countdown and a toolbar "pause AutoSave" button.
- **Crash recovery:** offers to restore the AutoSave state after a crash.
- **Automatic backups** of the previous version on every manual save, with a max count.
- **Merge Project:** imports another project's surfaces, fixtures, media, cues/timelines and MIDI/OSC/DMX mappings into this one, without overwriting (6.1).
- **Import custom resources from another project** via the Library (6.0).
- **MadMapper 5 projects convert** to a workspace folder; legacy Cues become Timelines; the original stays intact.
- **Project templates:** File > New Project > From Template (e.g. Video Wall, SoftEdge Blending 4 Outputs).
- **Export to SVG, PNG and PDF** (full version only).

## 4. Media Bin

Source: https://docs.madmapper.com/madmapper/6/3.-media/media-bin , https://docs.madmapper.com/madmapper/6/3.-media

- **Media categories:** Generators, Materials, Laser Materials, Laser Generators, ISFs, Quartz Composer, Images, Movies, Image Folders, Syphon/Spout/NDI, Live Input, Loopbacks, Montages.
- **Add media** with "+", drag and drop into the bin, or drop straight into a Timeline.
- **List or grid view** with several thumbnail sizes; categories fold/unfold.
- **Usage count on thumbnails:** shows how many surfaces use the media; click to select those surfaces.
- **Live preview in the inspector,** switchable off to save GPU; refresh/reset thumbnail.
- **Info pop-up:** type, ID, path, resolution, duration, FPS, codec, decoder, pixel format, bit rate, color space, embedded audio.
- **Replace a material or generator** on a visual from the Media Bin (6.0).
- **Convert a Material to a Generator** by dragging it into Generators (renders once to a texture instead of per surface).
- **Reload file** menu entry for file-based media (5.4).
- **Multi-select media** to edit, duplicate or delete together (5.6).
- **Force Media option** (Master) to apply the same aspect ratio to media.

## 5. Movies, images and audio files

Source: https://docs.madmapper.com/madmapper/6/3.-media/media-formats-and-video-codecs , https://docs.madmapper.com/madmapper/6/3.-media/media-bin , https://madmapper.com/madmapper/features

- **Movie formats via FFmpeg:** MOV, MP4, M4V, MKV, AVI, WMV, MPEG, MTS/TS, M2TS, FLV, VOB, DV, WebM, GIF and more.
- **Codecs:** H.264, HEVC/H.265, AV1, H.266, ProRes, DNxHD, PhotoJPEG; NotchLC and DXV not supported.
- **Native GPU HAP decoding:** Hap, Hap Alpha, Hap Q; **Hap R** on Windows only (5.5).
- **ProRes hardware decoding** on macOS (VideoToolbox), on by default.
- **Optional hardware decoding** of H.264/HEVC (AVFoundation on Mac, DXVA2 on Windows), with fallback to FFmpeg CPU decoding.
- **Movie transport:** step back/forward, play, pause, jump to start/end.
- **Playback modes:** loop, play once and hold last frame, ping-pong, random frames.
- **Playhead, speed (%) and loop in/out** sliders per movie.
- **Movie audio:** on/off, output device, volume, and "link audio level to surface opacity".
- **Per-movie audio routing matrix** (6.0).
- **Per-movie color profile;** YUV (YCrCb) color-profile control for FFmpeg, NDI and video inputs (5.2).
- **Image formats:** JPG, JPEG 2000, JPEG XL, PNG, PSD, TIFF, TGA, BMP, EXR, HDR, HEIC, RAW, SGI, ICO, DDS, WebP, PICT, PDF, SVG (alpha supported).
- **32-bit float EXR** for precise data (displacement maps, shaders, laser control).
- **SVG image player:** SVG renders at any chosen pixel size without blur (6.0); on laser surfaces the paths are traced directly.
- **Lottie JSON animations** (After Effects/Bodymovin export) import.
- **Image Folder player:** plays a numbered image sequence like a movie, with all movie transport modes and scrubbing.
- **Image Folder options:** set FPS, width/height, scale mode (e.g. Scale to Fit), frame cache size.
- **Image Folder Auto Scan:** picks up images added to the folder while running.
- **Audio files:** WAV, MP3, AIF, AAC, M4A, AC3, WMA, FLAC, OGG.
- **Maximum texture size 16K** (32K on an RTX 2080); video mapping up to 16K.
- **Quartz Composer files** as media (Mac).

## 6. Generators and built-in content

Source: https://docs.madmapper.com/madmapper/6/3.-media/media-bin , https://docs.madmapper.com/madmapper/6/15.-troubleshooting/madmapper-release-notes-history , https://madmapper.com/madmapper/features

- **Generators:** built-in patterns rendered once at a set pixel size and shared by all surfaces.
- **Built-in colors, patterns, grids, text and shader content.**
- **Grid Generator:** test grid with adjustable columns/rows for lining up with a building.
- **Test Pattern generator** (6.1).
- **Calibration and soft-edge calibration cards** (downloadable from the account).
- **Text Generator** (new version in 5.2).
- **Countdown generator** (5.2) and **Chrono generator** (5.6).
- **Video Sampler generator** (5.2).
- **GPU Particles generator** (5.1) and **3D Particles generator** (5.5).
- **Solid color** generator for blocking out edges (unverified: from a university tutorial, not the manual).

## 7. Materials (GLSL / ISF) and the Code Editor

Source: https://docs.madmapper.com/madmapper/6/3.-media/writing-custom-glsl-materials , https://docs.madmapper.com/madmapper/6/14.-technical-notes/glsl-shader-functions-reference , https://docs.madmapper.com/madmapper/6/2.-the-interface/toolbar-and-workspace

- **Materials:** generative GPU shaders rendered per surface at that surface's resolution (sharper, costlier than generators).
- **Materials work on** Quads, 3D surfaces, Lines and DMX fixtures.
- **ISF media:** Interactive Shader Format files as media; render passes in ISF supported since 6.0 (manual's Media Bin page still says multi-pass ISF is not supported).
- **MadMapper material format:** ISF-style JSON header (CREDIT, TAGS, DESCRIPTION, VSN, INPUTS, GENERATORS, IMPORTED) plus `materialColorForPixel()`.
- **Parameter types:** float, int, bool, color, floatRange; bool as push-button; color without alpha.
- **Parameter folders:** a slash in the label groups controls in the inspector (e.g. "Noise/Speed").
- **Built-in variables:** TIME, TIMEDELTA, RENDERSIZE (with "Render to Texture").
- **Audio inputs in shaders:** FFT spectrum (configurable band count) and waveform textures, with attack/decay/release smoothing.
- **Generators inside shaders:** `time_base` (speed, reverse, speed curve, BPM sync), animator, damper, ADR envelopes.
- **Imported textures** in materials.
- **Optional vertex shader** next to the fragment shader.
- **Shader libraries:** MadCommon (constants, color conversion), MadNoise (value, simplex, Worley, flow, curl, fBm, ridged, billowed noise), MadSDF (2D shapes, fill/stroke/glow, boolean ops, repeat).
- **Surface FX shaders** have their own macros (FX_NORM_PIXEL, FX_IMG_SIZE) and per-surface-type variables.
- **Laser material shaders** with `laserMaterialFunc`, point count, scan speed, angle optimisation and last-frame access.
- **Built-in Code Editor:** live GLSL coding, syntax colouring, multi-line comments; GLSL up to 4.1 (Mac) / 4.5 (Windows).
- **Start from an empty template, or duplicate** a read-only built-in material to edit it.
- **Drag a material from the Media tab into the Code Editor** to open it.
- **Build / Restart** shortcuts in the editor (Cmd+B / Cmd+R; manual doesn't explain them further).
- **Exposed shader variables become inspector controls** automatically.

## 8. Online Library and MadAI

Source: https://docs.madmapper.com/madmapper/6/3.-media/online-material-library , https://docs.madmapper.com/madmapper/6/12.-advanced/mad-ai-extension , https://docs.madmapper.com/madmapper/6/1.-introduction/extensions-and-companions

- **Online Library (Cmd+Opt+L):** public Materials, Laser Materials, 2D FX, 3D FX, Line FX and Laser Line FX.
- **Featured curation** section in the Library.
- **Favorites,** synchronized across machines (6.0).
- **Install + Add to Project** in one click.
- **Offline library:** library resources available without internet (6.0).
- **Publish materials publicly** (community) or **privately** (backup to your account).
- **Project > Custom tab** lists your own materials; reveal file location.
- **MadAI:** generate a material, 2D Surface FX or laser material from a text prompt (5.7).
- **MadAI builds sliders** for the parameters you ask for.
- **MadAI history tab** of prompts; switch to the Code tab to edit the result by hand.
- **MadAI remixes** existing library content.
- **Credits:** each prompt costs one credit, bought on the MadAI page.

## 9. Live inputs and texture sharing

Source: https://docs.madmapper.com/madmapper/6/3.-media , https://docs.madmapper.com/madmapper/6/2.-the-interface/madmapper-preferences , https://madmapper.com/madmapper/features

- **Cameras:** USB/webcams via AVFoundation (Mac), DirectShow and Windows Media Foundation (5.3).
- **Blackmagic capture devices** natively.
- **Canon DSLR (EDSDK) and Sony cameras** as inputs (Sony 5.3).
- **Depth sensors:** Kinect ONE (v2) and Intel RealSense (5.2).
- **Syphon in/out** (Mac), **Spout in/out** (Windows).
- **NDI in/out** on both platforms.
- **Loopbacks:** a video output or buffer published back into the Media Bin as a live source (5.0; buffers 6.1).
- **"Check for devices"** menu item and automatic device checks (5.3).

## 10. Surfaces (video)

Source: https://docs.madmapper.com/madmapper/6/4.-surfaces , https://docs.madmapper.com/madmapper/6/4.-surfaces/surface-types , https://docs.madmapper.com/madmapper/6/4.-surfaces/assigning-content-to-surfaces

- **Quad:** four-corner surface with scale, rotate and perspective.
- **Triangle:** for slicing odd shapes into triangles.
- **Circle:** four handles plus a centre handle for perspective.
- **Quad as circle:** Mesh Warping > Sphereize turns a quad into a warpable circle.
- **Line (madLine):** a path surface with thickness, drawn point by point or freehand.
- **Mask surface:** an alpha mask with no texture that cuts what's under it.
- **3D Surface:** an imported OBJ model with calibration, lighting, wireframe and UV editing.
- **Surface list as a layer stack:** order sets what's in front; drag to reorder.
- **Show/hide, lock, rename** per surface.
- **Duplicate (Cmd+D) and Clone** (Opt+Shift+Cmd+D).
- **Assign media** by clicking a media item, a hover "load" icon, or the inspector's media pop-up.
- **Drag a file from Finder onto the stage** to create a quad at the media's aspect ratio.
- **Input rectangle:** move/scale/corner the area of the source each surface shows (crop/region of interest).
- **"Switch to media"** button in the surface inspector (6.0).
- **Position quads in exact pixels** from output settings (5.3).
- **Surface multi-select** shows only shared parameters.
- **Group values multiply:** a surface's percentage is multiplied by its group's.

## 11. Surface Inspector: appearance, FX and output

Source: https://docs.madmapper.com/madmapper/6/4.-surfaces/surface-inspector , https://madmapper.com/madmapper/features

- **Opacity, blend mode, flip, color** per surface.
- **RGB/HSV color wheel.**
- **Surface FX slot:** on/off toggle and a dropdown to pick the effect; its controls appear below.
- **Color Controls** (default FX).
- **Effects listed on the features page:** chroma key, luma key, color controls, deformers, color remapping, transform matrix, particles, directional blur, edge detection.
- **"Curve RGBA" and "Curve Luma"** 2D FX (5.1).
- **Downloadable 2D FX** from the Library; **3D FX** for 3D surfaces and **Line FX** for lines (named as Library categories; individual effects not documented).
- **Custom shader-based user FX.**
- **MadAI-generated 2D FX.**
- **Surface FX timebase** can be driven by timeline playback (6.0).
- **Perspective on/off** (corner pin) in Output settings.
- **Feathering** of surface edges.
- **Soft-edge** settings on quads (see section 14).
- **3D-specific parameters:** wireframe, lighting, Edit UVs.

## 12. Warping and geometry

Source: https://docs.madmapper.com/madmapper/6/4.-surfaces/warping-and-geometry

- **Corner pinning** of quads with Perspective on.
- **Mesh grid** generated from horizontal/vertical subdivision counts.
- **Add mesh points** with Alt+click or a button / right-click.
- **Delete a point** (Delete) or **remove a whole row/column** from its right-click menu.
- **Move a segment** by clicking it; click again to **select the whole line.**
- **Box select, Shift-click add, move many points at once.**
- **Nudge** points by 1 px (arrows) or 10 px (Shift+arrows).
- **Bezier mesh warping:** smooth curved mesh with tangent handles; **unlink tangents** for sharp corners.
- **Precision slider** adds polygons for smoother curves.
- **Unlimited warp points.**
- **Link Input and Output Mesh** (default) or edit input and output meshes separately (Show Input Mesh).
- **Grid generator overlay** with low opacity and additive blend to see the building while warping.

## 13. Masks and groups

Source: https://docs.madmapper.com/madmapper/6/4.-surfaces/surface-groups-and-masks , https://docs.madmapper.com/madmapper/6/4.-surfaces/warping-and-geometry

- **Per-surface masks** (Alt+A): cut only that surface.
- **Multiple bezier masks per surface.**
- **Global mask surfaces:** a mask above the stack cuts every layer beneath.
- **Freehand mask drawing:** click-drag; points and beziers are created automatically; Enter closes.
- **Preset mask shapes:** square, triangle, circle.
- **Bezier handles** on mask points.
- **Mask feathering:** a variable gradient along the mask line.
- **Import/Export SVG** on masks and lines (5.5).
- **Groups** (Cmd+G): move, scale, rotate and fade many surfaces together.
- **Nested sub-groups.**
- **Shadow groups:** masks and blend modes inside the group only affect the group.
- **Group in place:** Shift-click the group icon to group a selection where it is.

## 14. Edge blending and multi-projector

Source: https://docs.madmapper.com/madmapper/6/6.-outputs/soft-edge-blending , https://docs.madmapper.com/madmapper/6/6.-outputs/loopback-and-advanced-output-techniques , https://docs.madmapper.com/madmapper/6/12.-advanced/performance-optimization

- **Soft Edge on quads:** checkbox in the surface inspector.
- **Auto Setup:** select overlapping quads and it computes the overlaps and fades.
- **Per-edge Width and Power (gamma)** for top/bottom/left/right.
- **Custom blends with feathered masks** for overlaps that aren't straight.
- **Projector stacking:** duplicate the mapping for 100% overlap and double brightness.
- **Calibration card** media for lining up overlaps.
- **Template "SoftEdge Blending 4 Outputs":** one source split across four projectors via a loopback.
- **Loopback pipelines:** keep projector mapping fixed while cues change content upstream.
- **Hardware splitters** (Datapath Fx4, Matrox QuadHead2Go/TripleHead2Go, DeckLink 8K Pro quad) supported as large outputs.
- **Quadro Sync / genlock** recommended for tear-free multi-output on Windows.
- **Lab-tested 16 Full HD outputs** on one PC; outputs limited only by the GPU.

## 15. Outputs

Source: https://docs.madmapper.com/madmapper/6/6.-outputs

- **Video outputs** for displays, projectors, LED processors.
- **Destination picker** with GPU, resolution and refresh info.
- **Output size** with aspect lock; **up to 8K per output.**
- **Fullscreen** toggle per output; fullscreen covers the display.
- **Flip** horizontal/vertical (rear projection).
- **Frame delay** per output to sync mixed hardware (5.3).
- **MSAA anti-aliasing** per output.
- **Send an output to** a desktop window, Syphon/Spout, NDI or an internal loopback.
- **Test pattern** per output (Opt+Shift+T).
- **Background image and global mask** per output, each with opacity.
- **Snapshot** of an output frame to file (5.5).
- **Matrix split:** divide one canvas over several displays (e.g. 2x2 of 1080p).
- **Video Buffer:** off-screen canvas with size, MSAA, 8-bit / 16-bit float / 32-bit float, alpha, background and snapshot (6.1).
- **DMX outputs** as canvases with their own size/definition and background image (fixtures sample the output).
- **Laser outputs and Laser Buffers** (section 20).
- **Enable/disable and lock** each output; disabling closes its fullscreen at once.
- **Blackmagic video output** support (5.3).
- **Spatial/Laser Scanner results as backgrounds,** also offline.
- **Styleable video cursor** (cross or circle) on the output (6.0).
- **Freeze video and DMX output** without stopping playback (Master).

## 16. Calibration tools (scanners and 3D)

Source: https://docs.madmapper.com/madmapper/6/12.-advanced/special-tools-and-auto-calibration , https://docs.madmapper.com/madmapper/6/4.-surfaces/tools-space-scanner , https://docs.madmapper.com/madmapper/6/4.-surfaces/advanced-3d-and-scanning

- **Space Scanner** (was Spatial Scanner): projects black/white stripe patterns, a camera records them, and MadMapper builds a pixel-accurate picture of the scene from the projector's point of view.
- **Space Scanner sources:** webcam, USB camera, Canon DSLR (Sony added 5.3).
- **Capture timing** setting to sync the camera shutter to the patterns; one-button capture.
- **Scan result as background** layer while mapping, not sent to the output.
- **3D calibration:** pair points on the real object with points on the OBJ model; live RMS error; Fix Position; Reset Calibration.
- **LED Scanner:** camera finds each LED as it flashes and places DMX fixtures where they physically are (section 18).
- **Laser Scanner:** camera reads a 16x16 laser dot grid and builds a background for laser mapping (5.5).
- **Demo limits** the Space Scanner to 1/4 resolution.

## 17. 3D surfaces

Source: https://docs.madmapper.com/madmapper/6/4.-surfaces/advanced-3d-and-scanning , https://docs.madmapper.com/madmapper/6/4.-surfaces/surface-inspector

- **OBJ import** by drag and drop or the "3D Model" dropdown.
- **Calibrate** the model to the real object (see 16).
- **Virtual light:** Lighting toggle; orbit with distance, longitude, latitude.
- **Light color and ambient color.**
- **Light in Preview** toggle.
- **Attenuation** on/off with falloff curve (e.g. quadratic).
- **Spot mode** with exposure and cutoff angle.
- **Shadows** with a quality/resolution setting.
- **Wireframe** display.
- **Edit UVs** (texture coordinates).
- **3D FX** from the Library (individual effects not documented).
- **3D Surfaces on lasers** (section 21).
- No full 3D scene/camera system found in docs.

## 18. DMX and LED mapping (MadLight)

Source: https://docs.madmapper.com/madmapper/6/5.-dmx-and-led-mapping/dmx-fundamentals , https://docs.madmapper.com/madmapper/6/5.-dmx-and-led-mapping/dmx-fixtures , https://madmapper.com/madmapper/features , https://docs.madmapper.com/madmapper/6/13.-setup-and-installation/additional-hardware

- **MadLight:** fixtures sample the video composition so video drives lights.
- **Art-Net output** (Art-Net 4) and **sACN output** (multicast or unicast).
- **Broadcast and unicast with polling;** built-in Art-Net node scanner.
- **Universe synchronization** (Art-Net and sACN).
- **Jumbo frames;** up to 4096 universes; DMX output up to 1000 fps; max-FPS cap.
- **USB DMX devices:** garageCube USB-DMX, ShowJockey, Enttec DMX USB Pro / Pro MK2 / Open DMX, DMX King eDMX2 Pro, Enttec-compatible.
- **Teensy running FastLED** as an output device.
- **DMX Router module:** route Art-Net/sACN universes to one or more USB-DMX devices.
- **DMX tab (Cmd+4):** fixture list working like surface layers (show/hide, lock, rename, reorder, groups).
- **Fixture types:** grid fixture, DMX Line, DMX Circle, DMX Bezier (6.1).
- **DMX fixtures inside video outputs** and sampling surfaces (6.0).
- **Fixture opacity and color.**
- **DMX "dip switch"** address field per fixture.
- **Auto-addressing** of new, duplicated and pasted fixtures (can be turned off, 6.0.2).
- **Sampling filters:** None (centre pixel), Box (average square), Anamorphic (separate X/Y).
- **Response curve** per fixture (brightness vs DMX value).
- **Cross-universe fixtures,** or `avoidCrossUniversePixels` to keep a pixel in one universe.
- **Fixture library** of built-in profiles (generic RGB/RGBW, garageCube LED products).
- **Unlimited fixture definitions;** `.mmfl` import/export.
- **CSV import** of fixtures with names, groups (`/` hierarchy) and addresses.
- **SVG import of fixture positions** (with bezier, device IDs, thickness) and **export fixtures to SVG** (6.1).
- **Video Wall template:** builds a grid of quads or DMX fixtures from screen count, resolution and bezel size.
- **DMX Monitor** panel (Opt+Cmd+M) to see output channels.
- **LED Scanner:** pick camera, fixture profile and universe/channel range; brightness/threshold; maps each LED from its flash; option to place undetected LEDs at 0,0.
- **garageCube hardware:** LED bars (RGB, pure white), DMX bulbs, USB-DMX (512 ch), Art-Net-to-DMX (8 universes).

## 19. Fixture Editor and moving lights

Source: https://docs.madmapper.com/madmapper/6/5.-dmx-and-led-mapping/dmx-fixtures , https://docs.madmapper.com/madmapper/6/5.-dmx-and-led-mapping/controlling-moving-lights , https://docs.madmapper.com/madmapper/6/14.-technical-notes/dmx-fixture-definitions

- **Fixture Editor (Opt+Cmd+F):** create, group, import and export fixture definitions.
- **Size modes:** fixed size, LED Strip mode (pixel count set per fixture), Matrix mode (divisions per fixture) (6.1).
- **Pixel grid editor:** click pixels to patch/unpatch.
- **Pixel types:** RGB, RGBW, Custom; channels per pixel.
- **16 wiring/scan patterns** for strips and matrices.
- **Skip channels 511-512** at universe ends.
- **Channel types:** R, G, B, W, luminance, C, M, Y, Slider, Expression, Unused.
- **8-bit or 16-bit** (coarse/fine) channels.
- **Expression channels:** GLSL formula using color, INDEX, position, time and slider values.
- **Slider channels** become sliders in the fixture inspector (pan, tilt, gobo, strobe, amber, UV, fog...).
- **Moving heads and effects devices** (hazers, fog) driven from sliders, oscillators or timelines while color follows the video.
- **One definition per DMX mode** of a fixture.

## 20. DMX recording, playback and input

Source: https://docs.madmapper.com/madmapper/6/5.-dmx-and-led-mapping/madlight-recorder , https://docs.madmapper.com/madmapper/6/8.-modules , https://docs.madmapper.com/madmapper/6/11.-live-performance-and-control

- **MadLight Recorder (Opt+Cmd+R):** records the outgoing Art-Net/sACN frame by frame into a MadLight Sequence.
- **Attach an audio file** to a sequence, synced or looping on its own.
- **Export sequences to MiniMad** with their audio; trim in/out frames.
- **MadLight Player module:** plays recorded sequences back (5.5).
- **DMX input** via Art-Net, sACN or USB DMX, with a chosen universe and interface.
- **DMX Learn:** map incoming channels to any parameter (purple highlight).
- **DMX to OSC module:** forwards a DMX universe as floats over OSC (6.1).
- **ArtNet timecode** as a timeline sync source (6.0).

## 21. Lasers (MadLaser extension)

Source: https://docs.madmapper.com/madmapper/6/7.-laser-madlaser , https://docs.madmapper.com/madmapper/6/7.-laser-madlaser/getting-started-with-madlaser , https://docs.madmapper.com/madmapper/6/6.-outputs , https://docs.madmapper.com/madmapper/6/15.-troubleshooting/madmapper-release-notes-history

- **Laser surfaces (Cmd+3):** Laser Quad, Laser Lines (straight/bezier, split or linked tangents), Laser Circle (6.0), Laser Text, Laser 3D surfaces (5.2).
- **Laser Text** with single-stroke "stick" fonts.
- **Laser materials:** GLSL generative vector content; 40 atmospheric laser materials (5.6); free ones in the Library.
- **Laser generators:** Polyline Mixer, Countdown, Text (5.2); "Laser FX" generator applies line FX to vector sources (6.0).
- **Laser Line FX** library.
- **Live GLSL coding** of laser materials.
- **Real-time video vectorization:** GPU/OpenCV tracing of movies, cameras or the composition into laser paths.
- **Vectorization settings:** Find Paths / Find Contours, threshold, use color, thickness, denoise, max resolution.
- **Path filtering:** min/max length, max count, keep longest/shortest.
- **Diagnostic views** (processed image, output polylines) and stats in the Monitor.
- **ILDA files** (`.ild`) play like movies on laser surfaces; option to use source points or re-rasterize (5.1).
- **ILDA export** of laser outputs from the timeline; single-frame or ILDA movie recording.
- **16K vectorization in Expert Mode** for offline ILDA conversion.
- **Lottie animations** on lasers (5.4); **SVG Folder Player** (5.3).
- **Laser render settings:** scan & beam mode (auto/manual), max speed, angle optimisation (angle min, delay, end repeat), in/out fade, soft close; fixed scan speed option.
- **3D objects as laser wireframes:** OBJ or built-in primitives; face culling; texturing; calibrate; process mode (e.g. depth); sensibility; definition.
- **Laser mesh warping** (grid and bezier) on quads, lines and text.
- **Laser soft edge** on Laser Quads (5.1).
- **Laser mask feathering** on Laser Quad and Laser Output (6.0).
- **Merge option** for Video and Laser Lines paths (5.1).

## 22. Laser outputs, safety and previsualization

Source: https://docs.madmapper.com/madmapper/6/6.-outputs , https://docs.madmapper.com/madmapper/6/7.-laser-madlaser/laser-visualization-and-safety , https://docs.madmapper.com/madmapper/6/13.-setup-and-installation/additional-hardware

- **Laser DACs/protocols:** Ether Dream, ShowNET, Helios USB, Pangolin FB3/FB4 (via Beyond, Windows), IDN (5.4), AVB/Dante (5.1), Moncha (5.1), LaserCube (Wi-Fi/Ethernet/USB), LaserDock USB (unofficial, Windows).
- **Unlimited laser outputs;** scanners up to 100 kpps.
- **Laser Dispatch:** spread heavy content over several projectors.
- **Laser Buffer:** an off-screen laser canvas dispatched to several lasers, with its own masks (6.1).
- **Points per second,** desired frame rate, point count / ILDA FPS readout, output delay.
- **Preserve FPS mode** (Expert, 5.3).
- **Scan area limits** to keep the beam off the galvo edges.
- **Blank delay** to remove connecting lines.
- **Color calibration:** master levels, minimum voltage, response curves, color delay.
- **Distortion correction** (Level X / Level Y) and distortion transform on laser output (6.0).
- **Laser test patterns:** scan area, geometry, name, color, masks.
- **Laser masks / safety zones:** opacity, invert, feathering, "Mask Beams Only" (6.0.6), per output.
- **Max non-moving beam intensity** cap; unlock security features (with warning).
- **Arm/disarm lasers** with a safety acknowledgement; auto-arm on project open; Freeze laser outputs (5.3).
- **Publish laser to a loopback** (to project the laser picture with a video projector) and **PONK in/out** over the network (5.2).
- **AVB/Dante laser settings:** device, sample rate, buffer, devices count, channels per unit, scene markers.
- **Beam preview:** level, beam origin, end beam on path, simulated fog; can be sent to real projectors/LED screens.
- **External visualization:** send laser data via Art-Net to Capture, WYSIWYG, Depence or LA.preview.
- **Show ILDA points in the preview** (5.0.3).

## 23. Master settings

Source: https://docs.madmapper.com/madmapper/6/9.-master-settings

- **Master tab (Cmd+7).**
- **Master output level** multiplies every surface, fixture and audio level; separate video, audio, DMX and laser masters.
- **4-band audio equalizer** (6.0).
- **Audio input level.**
- **Master Speed** (also affects audio playback).
- **Freeze Engine** (except during Link/BPM sync).
- **Freeze Video and DMX Output** without stopping playback.
- **Global BPM.**
- **Video cursor and laser cursor** controls.
- **Color palette** with custom import.

## 24. Audio: playback, analysis and routing

Source: https://docs.madmapper.com/madmapper/6/9.-master-settings/audio-input-and-beat-detection , https://docs.madmapper.com/madmapper/6/14.-technical-notes/audio-routing

- **Audio Player module:** plays audio files and can create cues.
- **Unlimited multichannel audio** in and out, to one audio interface at a time.
- **Audio routing matrix:** send any source channel to one or more outputs (movies and timeline audio, 6.0).
- **ASIO (Windows), WASAPI, CoreAudio;** works with BlackHole and JACK for sending audio to other apps.
- **Audio input analysis:** choose device and analysis channels.
- **Bands:** amplitude, bass, mid, treble (3 bands plus response).
- **Beat detection** on live input, sets the tempo.
- **Tempo sources:** beat detection, Ableton Link, MIDI clock, tap tempo, typed BPM.
- **Audio drivers for controls:** amplitude, bass, medium, treble, bpm, beatCount, beat divisions (1/4/8 beats), ltc_time.
- **Signal filters:** average, weighted average, pulse detection, attack/release, damper.
- **Response curves:** none, linear, Hermite, Catmull-Rom; editable curve graph.
- **BPM detection** when an audio file is dropped into the Conductor.
- **Audio-reactive shaders** via FFT and waveform textures (section 7).

## 25. Timelines and the Conductor (new in v6)

Source: https://docs.madmapper.com/madmapper/6/10.-timelines/timelines-overview-and-the-conductor , https://docs.madmapper.com/madmapper/6/10.-timelines/creating-and-editing-timelines , https://docs.madmapper.com/madmapper/6/10.-timelines/track-types-and-inspectors

- **Conductor:** the master show timeline.
- **Timeline banks** with several timelines each.
- **Every parameter animatable** with keyframes.
- **Record mode:** touching a control while recording writes a keyframe; armed tracks keyframe automatically.
- **Keyframe interpolation:** none (hold), linear, bezier (handles), auto curve (Catmull-Rom).
- **Simplify** (thin keyframes) and **Quantize** (snap to grid).
- **Add keyframes at playhead** on all or modified tracks.
- **Insert/Delete time.**
- **Loop start/end brackets** (6.1).
- **Timeline settings:** FPS, master BPM, time signature.
- **Playback modes:** default clock, controlled speed, global BPM, external sync.
- **External sync:** MTC, LTC (audio), ArtNet timecode, Ableton Live/Link.
- **Ruler formats:** seconds, frames, timecode, bars/beats (1/32 to 4 bars).
- **Track pinning, keyframed-tracks filter, search by name.**
- **Zoom (Opt+scroll) and track height (Cmd+scroll); Tab jumps between clips.**
- **"Add to Timeline" / "Delete Track"** from any widget's context menu (6.0.1).
- **Timeline Player track:** Scenes and Cues arranged in time, with split, quantize, copy/cut.
- **Montage track:** non-linear editing of video, images, materials and generators (cut, snap, reverse).
- **Montage crossfades** with adjustable length.
- **Montages as media:** a montage appears in the Media Bin for any surface, laser or DMX canvas.
- **Montage options:** no-media behaviour (skip or black), force media resolution, live preview.
- **Audio track:** waveform or spectrum view, FFT, beat tracking, clip gain/speed/offset, detected BPM, track gain, routing matrix, "use as background".
- **Audio waveform in movie clips and montage tracks** (6.1).
- **OSC track:** send float, int, string, color, bool and event values to an OSC device/address with min/max.
- **MIDI track:** send CC, 14-bit CC, pitch bend and notes on a chosen device and channel.
- **Timeline grid BPM sync** with quantized launch and Ableton Link (6.0).

## 26. Markers, clips, scenes and cues

Source: https://docs.madmapper.com/madmapper/6/10.-timelines/time-markers-and-follow-actions , https://docs.madmapper.com/madmapper/6/10.-timelines/clips , https://docs.madmapper.com/madmapper/6/10.-timelines/scenes-and-cues

- **Markers:** Pause (P/), Loop (L/), Go to marker (G/), with colors.
- **Marker actions:** pause, continue, loop, go to marker.
- **Follow actions:** go to marker, go to marker and pause, start a timeline.
- **Go / Go and Play** buttons to jump to a marker.
- **Wait for GO:** hold at a pause marker until the operator presses GO.
- **MAD Clips:** named in/out segments of a montage, loopable, with own speed.
- **Scenes:** full snapshots of every surface, fixture and media.
- **Cues:** store only chosen parameters; media keeps playing.
- **Cue grid** of buttons with rows/columns, insert/delete rows and columns.
- **Fade time** per scene/cue; **transition type** (e.g. linear).
- **Launch quantize** to the beat.
- **Button color, picture and comment** per cue.
- **Record / Update / Update Startup Values** to rewrite a cue from the live state; batch update several.
- **Drag media or surfaces onto a grid slot** to make or overwrite a cue.
- **Edit view highlights** which parameters a cue stores and which differ.
- **Cues become timelines:** each cue can have its own timeline (Animate) for detailed transitions.
- **Per-track transition:** none, timeline setting, or custom duration.
- **"Add To Timeline"** from a cue, filtered to surfaces, lasers, fixtures or media.
- **Trigger columns** and start cues in a grid with fade/position over OSC.
- **Feedback on Scenes and Cues** to controllers (5.0).
- **Unlimited cues; animatable cue transitions.**

## 27. Scheduling and automation

Source: https://docs.madmapper.com/madmapper/6/9.-master-settings/scheduling-and-automation , https://docs.madmapper.com/madmapper/6/8.-modules

- **Timeline Scheduler:** start a timeline on given days/times, stop when leaving the range, finish loop first; 24-h clock.
- **Day/date rules:** year, month ("Each"), weekdays.
- **Calendar Scheduler:** ICS calendar events trigger scenes/cues of the same name; recurring events; offline import; drag-drop ICS.
- **Calendar Scheduler via OSC:** send an ICS file path as an OSC string (5.6).
- **Cue Scheduler:** schedule cues or columns; auto-advance after N beats.
- **Startup Timeline:** run a timeline at startup after a delay, with countdown and Test button.
- **Idle Timeline** (was Idle Cue, 5.5) and **Cue Starter** (5.5) modules.
- **Timeline automation "Start Columns in range"** (6.1).
- **Logging** of scheduler matches (verbose option).

## 28. Modules

Source: https://docs.madmapper.com/madmapper/6/8.-modules

- **Modules tab (Cmd+6):** add instances with "+", turn each on/off.
- **Audio Player.**
- **Calendar Scheduler, Cue Scheduler, Timeline Scheduler, Startup Timeline, Idle Timeline.**
- **Control Surface:** cue grid on APC mini Mk2, APC40 Mk2, Launchpad MK2 / Mini Mk3 / X with colored pads (5.4).
- **Controls Combiner:** mix several controls with different modes.
- **Device Activity** (5.5).
- **DMX to OSC** and **DMX Router.**
- **Macro module:** one widget drives many controls (6.1).
- **MadLight Player.**
- **MiniMad Controller:** remote-control MiniMad units on the network.
- **MIDI Out** and **OSC Out.**
- **Oscillator, Oscillator 2D, Oscillator Bank** (unlimited).
- **RandomNoise.**
- **Pollution / Weather:** live temperature, cloud, humidity, gas data as controls.
- **Firmata** (Arduino) and **PJLink** (MadProjectorControl) modules.

## 29. Control: MIDI, OSC, keyboard, gamepad and more

Source: https://docs.madmapper.com/madmapper/6/11.-live-performance-and-control , https://docs.madmapper.com/madmapper/6/11.-live-performance-and-control/osc-commands-and-channels-list , https://docs.madmapper.com/madmapper/6/2.-the-interface/madmapper-preferences

- **Control List (Cmd+L):** every mapping by source (Keyboard, MIDI, DMX, OSC, Audio, Gamepad, MadMapper, Other).
- **Right-click "Add Control"** on any parameter.
- **Learn mode** with per-protocol colors; "Clear all controls".
- **Mapped values shown** on the widget.
- **Per-control options:** source range, target range, invert, filter, response curve, toggle; enable/disable checkbox (5.6); custom curve (5.6).
- **Keyboard control:** momentary or toggle.
- **MIDI input:** notes, CC, 14-bit pitch bend, optional velocity.
- **MIDI feedback** to motorized faders and LED buttons.
- **MIDI output** devices defined per project; MIDI Out module.
- **MIDI clock** in (tempo).
- **MTC** in (timeline sync).
- **OSC input/feedback ports** (prefs default 8010/9000), Bonjour discovery, custom feedback IP.
- **OSC outputs** list per project (name, IP, port).
- **OSC API:** human-readable addresses (e.g. /surfaces/Quad-1/opacity), get values, wildcards.
- **OSC command groups:** application (quit, restart computer, views, record light), master, media select/assign, modules, outputs, timelines/conductor transport, cues.
- **OSCQuery** discovery with websocket LISTEN/PATH (5.4); "Copy OSC Address".
- **Ableton Link** tempo sync.
- **Gamepads:** PS3, PS4, PS5 DualSense, Xbox, generic HID.
- **Stream Deck plugin** for scenes, cues and parameters (5.4).
- **Makey Makey** and **Leap Motion** as inputs.
- **"Media Playback" input channels** for controls (5.0).
- **Control from other machines** over the network (another MadMapper, Chataigne, TouchOSC).
- **TouchOSC two-way sync** via OSC feedback (named in docs as an example).

## 30. Hardware integration (Arduino, projectors)

Source: https://docs.madmapper.com/madmapper/6/12.-advanced/arduino-and-firmata-integration , https://docs.madmapper.com/madmapper/6/12.-advanced/madprojectorcontrol

- **Firmata module:** two-way link to Arduino Uno/Mega, Teensy or any Firmata board on a serial port.
- **Pin modes:** analog in, digital in, digital out, PWM, servo.
- **Sensors/buttons as controls;** **drive LEDs, relays, servos** from any parameter.
- **MadProjectorControl:** PJLink power on/off and video/audio mute for unlimited projectors.
- **PJLink auto-discovery** or manual IP, password support.
- **Projector feedback:** state follows the projector's own remote; name, model, serial, format, lamp hours.
- **Schedule projector power** with the schedulers.

## 31. Export and offline rendering

Source: https://docs.madmapper.com/madmapper/6/6.-outputs/timeline-export

- **Export from the Conductor:** whole show to last keyframe/marker, or only the loop section.
- **Pick which outputs** to render.
- **Video codecs:** H.264, HEVC, ProRes, Hap, Hap Alpha, Hap Q, image sequence; quality slider.
- **Embed the audio mix** in a video export.
- **ILDA movie per laser output.**
- **MadLight Sequence for DMX:** delta-compressed, zipped, split into FAT32-safe chunks with time indexes; with timeline audio (6.0.8).
- **Audio mixdown:** WAV, MP3 or M4A; channels; sample rate.
- **Export folder** chooser with reset to workspace default.

## 32. MiniMad hardware player

Source: https://docs.madmapper.com/madmapper/6/6.-outputs/minimad , https://docs.madmapper.com/madmapper/6/1.-introduction/extensions-and-companions

- **Export video mapping to MiniMad** (Shift+Cmd+E) to SD card or over the network.
- **Video mode:** quad, triangle, circle and mask surfaces with mesh warp and perspective; one movie at a time; 1080p.
- **Transcoding on export;** adjust movie framerate; blanking time on media change.
- **Fullscreen max quality mode** (1080p30, no mapping).
- **Multi-unit sync** and zero-config clusters; one-click video wall.
- **Light mode:** plays MadLight sequences to Art-Net/DMX (up to 64 universes) with multichannel audio.
- **Playback modes:** loop, random, list.
- **7-button panel:** mode, pause/restart, next/previous, brightness, blackout, shutdown, edit mapping on device.
- **USB keyboard control** and **OSC on port 1111,** Bonjour.
- **Auto-restart after power loss;** mapping edits saved to SD.
- Raspberry Pi 2 based, HDMI out up to 1080p60.

## 33. Performance, platform and licensing

Source: https://docs.madmapper.com/madmapper/6/12.-advanced/performance-optimization , https://madmapper.com/madmapper/features , https://madmapper.com/madmapper/software , https://docs.madmapper.com/madmapper/6/faqs

- **64-bit, native Apple Silicon** (5.0) and Windows 10+; macOS 11+ listed on the product page.
- **Same features on Mac and Windows.**
- **Built-in geometric optimization engine.**
- **Hardware-accelerated video engine;** Hap recommended for many layers and scrubbing.
- **Licenses:** monthly rental, perpetual Artist license (2 seats), educational.
- **Demo:** all features with watermark, DMX blackout every 30 s, 1/4 Space Scanner, no saving/export/library install.
- **License manager:** rename computer and license (6.0.8); command-line authorisation (5.1); mass-deploy installation.
- **Account system** for licences, library and MadAI credits.

---

## Areas the official docs cover thinly

- **Surface FX list:** the manual only names "Color Controls" plus Library categories (2D/3D/Line FX); the effect names in section 11 come from the features page.
- **Built-in generators/materials:** no catalogue in the manual; names come from release notes.
- **3D:** only OBJ surfaces with one light, shadows, wireframe, UVs and calibration. No 3D scene, cameras or multiple lights found.
- **Timecode setup:** LTC/MTC/ArtNet timecode are named as sync sources but the setup isn't documented.
- **Modules chapter** gives one line per module; many modules have no own page.
- **Setup/licensing subpages** and the v6 "What's New" / release-notes-v6 pages returned 404; release history was read from the troubleshooting chapter.
