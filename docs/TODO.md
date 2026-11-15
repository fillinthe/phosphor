# Phosphor: to do

Open work only, by what it's waiting on. Finished items: `docs/TODO-done.md` (moved there 2026-11-14).

## Owner's answers (2026-11-15, after the show)

- **Release 0.5.7:** wait (MacBook only until tested more).
- **Designer:** boxes use real layers: a Layer box picks an existing layer; boxes stop making hidden layers.
- **Clips / Layers / Mixer:** skip for now.
- **Outputs:** full MadMapper-style: slices listed like layers (order, show/hide, opacity, solo, drag), masks on
  slices, per-slice blend modes, feathered masks, mask groups, a separate input / output view.
- **Tidy the project folders:** do it (nothing deleted).
- **Clip grid column buttons:** grayed out while there are no layers.
- **Review leftovers:** fix all six (Windows waits to update until no outputs are open first).
- **Next up, all four:** OSC / linking like VDMX, audience game pad + 3D, code audit + code health, Morpholio import.
- **Game pad:** scripts for both Unreal and Unity; one pad, players take turns; A = boost, B = trigger (fires
  something in their world).

## Needs your decision

- [~] **Designer needs work** (owner, 2026-11-14; **Layer box built 2026-11-15**, not seen on screen yet: Designer › right-click › Layer, pick one of your layers; nothing is made for it): every box quietly makes layers of its own (Picture = a layer,
      Effect = a layer, Mix = two), so layers keep appearing while you work. Owner: "I should be able to make layers
      and then attach them to something". Proposal: hide the Designer's own layers from Layers / Clips / Mixer, and a
      "Layer" box that brings an existing layer into the network. Fixed 2026-11-14 (not released, not seen on screen yet): a box's
      source row no longer runs past the box's edge (the icon buttons were measured short), and a comp is no longer
      offered as a source inside itself (a comp fed into itself drew nothing and its movies stopped).
- [~] **Designer** (the TouchDesigner-style section, owner's name). Plan in `docs/design/nodes.md`. **Phase 1 done
      2026-10-29** (not installed): Designer area (⌘4), Picture / Effect / Mix / Output boxes on the node canvas, each
      backed by hidden layers; `--quick designer`. Checked on a screenshot + engine test; not yet with a real mouse.
      **Phase 2 done 2026-10-29** (not installed): Number boxes (LFO / sound / beat / random / steps / knob), a blue
      number input per setting on every box, chaining as the maths. Checked on a screenshot + engine test.
      **Phase 3 done 2026-10-29** (not installed): Box of boxes (double-click to build inside, breadcrumb back, Input
      boxes), saved by name (boxes preset folder) and added again from the canvas menu; Duplicate copies the inside.
      **Phase 4 done 2026-10-29** (not installed): points. Points: Shape (grid / sphere / cube / ring / helix /
      random), Relief (a picture as points), Model (an OBJ file's vertices), Move, Particles, and Dots / Lines / Mesh
      renders with a camera (distance, lens, turn) and a light (shade, fog), colored from a second picture input;
      a green Points wire kind; `--quick points` (+ `PHOSPHOR_POINTS_MODEL=file.obj`). Checked on screenshots + a
      GPU test. **Phase 5 done 2026-10-29** (not installed): data. Number boxes take their value from a live
      reading (weather / ISS / earthquakes), a web address answering JSON (a dotted path to the number, 0-at / 1-at),
      a Timer, the Clock or a table file (CSV, a column, a row every so many seconds); a Text box's words take the
      number ({value}, {percent}, {raw}); an Event (a number crossing a line sets any control, optionally both
      ways); canvas menu › Data; `--quick data`. Checked on a screenshot + tests (timer, clock, table, event; JSON
      paths; the Text input). Data left: a web box isn't checked against a real site (the parser is), a Number box
      drives one Text template only, no "every N rows" / loops for timers (a saw LFO does that), no OSC / MIDI *out*
      from an Event (an Event sets a control; mapping to OSC out is the Control Surface's job). All five phases of
      `docs/design/nodes.md` are built. Points: done (2026-10-30: Mesh has a depth buffer, Lines have a real width, a render fed by a
      Model reads in order by itself; particles step by the real frame time; effects after a render run at the canvas
      size, also on a Model layer; **Light boxes**: Points › Light, its settings = where from (left / right, below /
      above), color, color amount, strength, ambient, wired into a render's Light input (a yellow wire; the render's
      Shade goes all the way while one is wired); one light a render, no shadows). **Small items done 2026-10-30:** Number input boxes on a
      box of boxes, Wrap in a box (one box at a time: no multi-select on the canvas yet), Undo (toolbar button and
      Cmd-Z over the canvas, 50 steps per comp, boxes and wires only: a slider change isn't undone), Duplicate copies
      a box's settings, a half-visible box shows the settings that fit, a Number box's Knob is mappable (right-click ›
      Learn). Multi-select done 2026-10-30: Shift-click titles, drag moves them together, right-click › Wrap N boxes.
      **2026-10-30 (later):** boxes size to what their settings drew (`ui::nodes` measures each body), Event boxes
      send OSC / MIDI / Arduino out (`Trigger.send`).
- [ ] **Designer**: one Number box driving several Text templates; web data box reading a list over time (owner to say).
- [ ] **Clips / Layers / Mixer are confusing** (owner, 2026-11-14: "not sure what is best to use"): three views of the
      same layers. Idea put to the owner, not loved yet: Clips as the clear home (blend, solo, reorder, right-click
      add / group / delete on each row), Layers and Mixer out of the main tab row, a one-line purpose per panel.
- [~] **Outputs: layering and masking like MadMapper** (owner, 2026-11-14; "full MadMapper-style" 2026-11-15). Built
      (tests pass, not seen on screen yet): a mask on a slice cuts that slice only (right-click a slice › Add a mask
      on it; it moves and warps with the slice); slice blends Lighten and Subtract. Already there: slices listed like
      layers (eye, opacity, solo, lock, drag to reorder / group / another layout), soft mask edges, mask groups, the
      Studio's input view, undo, rename pop-up. Left: a mask shared by several slices, the blend shown on the row.
- [ ] **Real-hardware checks** (2026-11-13): Windows fullscreen output on a real second screen / mixed scaling (0.5.4,
      reporter #41); the Windows #28 freeze on a PC with a real graphics card (the VMs have none); 0.5.5's Inspector
      and ISF-folder fixes (reporter #42). Owner will use a real PC later.
- [ ] **Mouse checks** of the Designer items and the focused tab's X; **a real laser** for everything laser.
- [ ] **Phone test** (2026-11-04): MIDI / OSC › Phone or tablet › Get it on the phone: scan the QR code with the iPad,
      open the downloaded Phosphor.tosc in TouchOSC, point TouchOSC at the Mac (Browse list), move a control: step 3
      should say "Hearing from…". Owner to try with the real device.
- [ ] 96. **Blackmagic** capture and SDI / HDMI output (V, M). Hardware + SDK.
- [ ] 98. **Depth cameras** (Kinect v2, RealSense) (M). Hardware.

## Needs an account, a key or a server

Online media (Library › Online) already searches NASA, Wikimedia Commons, Internet Archive, Openverse, the Art
Institute of Chicago, Iconify, Poly Haven, projectM, Google Fonts, netlabels and isf.video with no keys. Left: the
sources that need an API key (Pexels, Pixabay, Unsplash, Freesound, Jamendo, Shadertoy, Giphy): a key field per
source, or a proxy on your server.

- [~] O1. **Stock video and pictures** (NASA, Internet Archive, Wikimedia Commons done; Pexels, Pixabay, Coverr need keys): Pexels and Pixabay (free API keys; free to use, no credit needed), Coverr,
  NASA Image and Video Library (public domain, no key), Internet Archive (public-domain films, no key), Wikimedia
  Commons (CC, no key).
- [~] O2. **Art and photos** (Openverse, the Art Institute of Chicago and the Cleveland Museum of Art (2026-10-27) done; Unsplash needs a key; other museums next): Openverse (search across CC collections, no key), Unsplash (key), museum open-access
  collections with APIs and CC0 pictures: the Met, Art Institute of Chicago, Smithsonian, Rijksmuseum, Europeana.
- [~] O3. **Shaders** (Online › Shaders (ISF): isf.video's shared shaders, checked and loaded; Vidvox's ISF-Files ship with Phosphor; Shadertoy needs a key): the ISF online library (isf.video) and Vidvox's ISF-Files on GitHub (straight into the
  effects / generators list), glslsandbox; Shadertoy (API key; most shaders are non-commercial CC BY-NC-SA: label
  them). Overlaps #94.
- [~] O5. **Icons and vector shapes** (Iconify done: every open icon set, white SVG; emoji sets come with it): Iconify (200,000+ open-license SVG icons, one API, no key), Phosphor icons,
  OpenMoji / Twemoji (emoji as SVG): as SVG clips or text-layer symbols.
- [~] O6. **3D models, textures, HDRIs** (Poly Haven textures and 360 panoramas (+ the 360 View effect) done; models next; ambientCG skipped 2026-10-27: its previews are spheres and its textures come only as zips): Poly Haven and ambientCG (CC0, public APIs), Kenney (CC0 packs), NASA 3D
  models, Smithsonian 3D: for 3D-model slices and textures.
- [~] O7. **Music and sound** (Online › Music: Internet Archive netlabel releases, Creative Commons; Freesound / Jamendo need keys): Freesound (CC, key), Jamendo (CC music, key), Free Music Archive, Internet Archive
  audio, Pixabay music: clip sound, timeline sound tracks, and something for the beat detection to follow.
- [~] O12. **Live pictures** (2026-10-27: Inputs › Live from the internet: NASA ISS, Sen 4K ISS, Florida rocket pads, a volcano; any YouTube / Twitch live page pasted in Network stream; the page is saved and its stream looked up each start. Left: Giphy / Tenor (keys)): public webcams and space feeds (NASA / ISS live streams) as live layers; GIFs from
  Giphy / Tenor (keys). The "From a link" yt-dlp route already covers pasted video links.
- [~] **Crash reports sent automatically** (#5): the app side is done (below); **needs you:** a private endpoint
      on your server. Spec: accept `POST` with JSON `{"version", "os", "arch", "report"}` (all strings), answer
      2xx, keep it private. Then put its address in `ENDPOINT` (crates/phosphor/src/crash_report.rs), or tell me.
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
- [ ] 94. **Online shader library**: browse, install, publish (M). **Needs you**: a server.

## Planned

- [~] **3D fly-through with the audience playing** (asked 2026-11-14; plan `docs/design/game-pad.md`). **Built
      2026-11-15** (tests + a fake phone and a UDP listener; not yet on a real phone, and the Unity / Unreal scripts
      are not compiled here: no Unity or Unreal on this Mac): Session › Audience play › Game pad: a D-pad, A and B on
      the play page, held like a real pad; each press is a control (Learn, then press on the phone: `/pad/1/up`…)
      and OSC out to "Send pad to" (`/pad/<player>/<button> 1|0`, `/pad/<player>/stick x y`); a silent held button
      lets go by itself; Kit for 3D artists… writes Documents/Phosphor Pad Kit (README, Unity `PhosphorPad.cs`,
      Unreal `PhosphorPadComponent`; A = boost, B = trigger). Bench `--quick pad` (+ `PHOSPHOR_PAD_PORT`).
      Left: glTF models inside Phosphor (step 3 of the plan), several players each with their own camera.
- [ ] **Import Morpholio Trace files** (.morpholiotrace, owner 2026-11-14; sample in `_local/samples/`, not in git): a
      zip of `data-books.plist` (the layers in order: title, hidden, opacity, blend, a 4×4 transform, `drawingToken`),
      one 4096×4096 transparent PNG per layer (`<token>_original.png`), and a SQLite file with each layer's pen
      strokes (`stroke_<token>`: NSKeyedArchiver blobs, readable with `import::keyed::unarchive`) and tiles. Plan: each
      drawing layer → a Phosphor layer (name, on / off, opacity, placed by its transform), PNGs copied beside the
      project; later the strokes as lines that draw themselves on (and for lasers).
- [~] *(Stack is no longer in the default layout)* **Stack panel shifts around** (find what moves: hover, values changing width, rows wrapping).
- [ ] **Code audit** (owner, 2026-11-15, after the show): correctness, leaks, threads and locks, dead code, duplication,
      the design-system rules, and the graphify map's loosely knit areas below. Report first, then fix in small steps.
- [ ] **OSC and linking like VDMX / Resolume** (owner, 2026-11-15): one control driving many things with their own
      ranges, user-built control windows, a proper LFO window (the LFOs & Modulators panel exists; make it the
      place to build them, not right-click menus), "programmable". The researched list (26 items, VDMX / Resolume
      docs vs the code): `docs/design/osc-linking.md`.
- [ ] **Code health** (graphify map, 2026-11-14: 14,953 nodes, 501 communities): `ui/kit.rs` (cohesion 0.05) and
      Mapping + Studio (0.11) are loosely knit: split them by job. British spellings left only in internal names
      (`colour_button`, `theme::FAVOURITE`, `colour_budget.txt`; serde field names stay for old saves).
- [ ] **Windows installs updates by itself** (2026-11-14, built, not released): check on the owner's Win11 PC.

## Ideas

- [ ] **Play to another Phosphor over the internet** (owner, 2026-11-15, later): Follow already works on the same
      network (`follow::{Link, Leader}`, the `/follow` socket); reach a Phosphor somewhere else (relay or invite
      link through the G Workshop account, NAT traversal, latency and media that isn't on the other machine).
- [ ] **Hand a show over to another computer** (owner, 2026-11-15, later): move the project and its media to another
      Phosphor so someone can take over when you leave, ideally live (state sync, then the other one takes the
      outputs). Today: File › Export show (a folder with the media) and Follow on the same network.

- [ ] **Lights on drones** (owner, 2026-10-30). Drone shows are flown by their own show software; Phosphor could send
      colors to it (DMX / Art-Net out, as for LED fixtures, or that software's own protocol). **Needs you**: which drone
      system, to see what it accepts.
