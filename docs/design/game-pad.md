# Audience game pad + 3D worlds (plan, 2026-11-14; build week of 2026-11-16)

**Goal:** people in the audience scan a QR code or tap an NFC tag, get a game pad on their phone (up / down / left /
right, A, B), and fly a camera through a 3D world shown on the projectors. The world can be a model loaded into
Phosphor, or a whole Unreal / Unity project running on its own PC with its lighting, materials and effects untouched.
Phosphor makes a ready-to-hand-over kit for whoever built the world.

```
 phones (Play page, game pad) ──WebSocket /guest──▶ Phosphor ──OSC /pad/…──▶ Unreal / Unity project (camera script)
                                                       ▲                              │
                                                       └──────── NDI / Spout ◀────────┘ (its picture, as a live layer)
```

## 1. Game pad on the Play page (needed for everything else)
- **Engine:** `AudiencePlay.pad: PadSettings` (on/off; layout: D-pad + A / B; hold = repeat or held-down state;
  players share one pad or get one each). New fields get defaults (old projects open). Turns, rest and places
  reuse what the Play page already does (`State::guest_may`, `slot_page`).
- **Phone (`player.html` / play page):** a full-screen pad: big D-pad left, A / B right, landscape and portrait,
  vibration on press, the player's number and time left in their turn. Sends `pad {player, button, down}` over the
  existing `/guest` socket; `remote::apply` checks the guest's turn (`guest_may`).
- **App:** `ControlSource::Pad { player, button }`: map a button to anything like a MIDI note (`ui::learn` Learn
  works by pressing on the phone). Held buttons are 1 while down, 0 when let go (a camera moves while held).
- **OSC out:** every press also goes out as `/pad/<player>/<button> 1|0` (and `/pad/<player>/stick x y` for the
  D-pad as a direction) to the OSC out host; a "Send pad to" host:port field in the Play settings. Uses the
  surface's existing OSC out path (`ControlHub::surface_osc`).
- **Session panel:** the Play section already shows the link, QR and Copy; add **Copy for NFC** (the link alone)
  and a note: the link holds this computer's Wi-Fi address, so give it a fixed address on the venue's router or
  the tags need writing again. Optional: show the QR on the output (exists).
- **Tests:** bench `--net` + a WebSocket client playing a guest (`--quick pad`: presses, turns, OSC out to a local
  UDP listener, PAD summary line); a real phone check.

## 2. The 3D project kit (export from Phosphor)
**Session › Play › Export kit for 3D artists…** writes one folder / zip, ready to email:
- `README.md` (plain words): what this is, the two steps below, this show's OSC port and address names, how to test.
- `Unity/PhosphorPad.cs`: a MonoBehaviour for the camera (or any object): listens for OSC on a UDP port (no
  plugin: a tiny built-in OSC reader), flies with the pad. Settings in the Inspector: speed, turn rate, rise
  speed, boost, smoothing, start point (B = back to it), which player(s), keep inside a box.
- `Unreal/PhosphorPad/`: a small plugin with an Actor Component (C++, UDP + OSC reader) doing the same, its
  settings shown in the editor; plus a Blueprint-only route using Unreal's built-in OSC plugin for people who
  don't compile C++.
- **Sending the picture back:** links and steps for NDI (Unreal: the official NDI plugin; Unity: KlakNDI) and Spout
  (Windows); Phosphor picks it up under Inputs (NDI / Spout already supported).
- `test-pad.html`: opens in a browser and sends pad presses as if from Phosphor (via Phosphor), so they can test
  without a crowd. Or simpler: Phosphor's own "Test pad" buttons in the Play section.
Their project keeps all its settings (lighting, post-processing, sky, materials): the script only moves the camera.

## 3. 3D models inside Phosphor (later, if wanted)
- glTF / GLB loader (`gltf` crate) → a mesh layer with textures and simple lighting; a camera (position, yaw,
  pitch, speed) whose controls the pad drives (same mapping as the scripts). Today models load as points only.
- For simple models; worlds needing Unreal's lighting use step 2.

## Order and size
1. Pad + OSC out + Copy for NFC + tests: about a day.
2. Unity script, Unreal plugin + Blueprint route, README, export command: about a day; testing Unity / Unreal
   needs those installed (on the PC with a graphics card).
3. glTF layer: 2–3 days, only if asked.

## Open questions for the owner
- Which engine did the 3D person use (Unreal or Unity)? That script first.
- One pad shared by turns, or several players at once (e.g. 4 cameras / 4 players)?
- What should A and B do by default (rise / boost / reset / trigger something in their world)?
