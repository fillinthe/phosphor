# Phosphor on a Raspberry Pi: a backup show player (research, 2026-11-18)

Not built yet; decisions pending. The plan in `docs/TODO.md` (Future) says what it should do. This is what we found
about doing it, especially video.

## Recommendation: bake the show on the computer, play it on the Pi

**Stage 1 (do first):** Phosphor renders each output, with its mapping baked in, to one video file per Pi, plus the
LED / DMX show as a `.phdmx` recording. The Pi runs `mpv` full screen with no desktop (DRM/KMS), started and kept
alive by systemd, looping the file, and plays the DMX recording over sACN / Art-Net. This is how MadMapper's MiniMad
works (a Pi-based player: export from MadMapper with the mapping baked in, one unit per projector). No GPU code of
ours runs on the Pi.

**Stage 2 (research spike, maybe never):** a reduced Phosphor (its own renderer, effects, live mapping) on the Pi.
Risky today: wgpu on the Pi's Vulkan driver (Mesa V3DV) is unproven (swapchain bugs, no winit DRM backend, Wayland
trouble on the Pi 5), stock wgpu offers no compressed BC textures there (so Hap would be decoded on the CPU; Hap R
not at all), and zero-copy HEVC into wgpu needs our own V4L2-request + dmabuf path.

## Video on the Pi

| | Pi 4 | Pi 5 |
|---|---|---|
| Hardware decode | HEVC up to 4Kp60, H.264 up to 1080p60 | HEVC up to 4Kp60 only |
| H.264 | hardware, 1080p60 | software: 1080p60 uses about half the CPU, occasional dropped frame |
| Render to | H.264 High, 1080p60, 15–20 Mbit/s (most reliable); HEVC for 4K | HEVC Main 8-bit, 1080p60 15–25 Mbit/s, 4K 40–60 Mbit/s |

- Files: `.mp4` / `.mkv`, yuv420p, one video stream, closed GOP with a keyframe every 1–2 s, loop made seamless in
  the render. Never H.264 at 4K. Hap is a poor Pi format (CPU decode, 4–8× the size of HEVC).
- Playback: `mpv --vo=gpu --gpu-context=drm --drm-connector=HDMI-A-1 --hwdec=drm,drm-copy,auto --loop-file=inf`.
  The fastest path (zero-copy `--hwdec=v4l2request` + overlay planes) needs a newer kernel and a patched mpv.
  Results depend on the exact mpv build: **test on a real Pi 4 and Pi 5** (1080p60 and 4K HEVC, `drm` against
  `drm-copy`, both HDMI ports) before promising anything. One report has 1080p60 HEVC dropping frames on a Pi 5 with
  Trixie's mpv 0.40.
- Two screens: one mpv per HDMI port is simplest, but the two aren't frame-locked (info-beamer found a random offset
  between the ports). Rotation by 90° / 270° isn't free on the display planes.
- Raspberry Pi docs: https://www.raspberrypi.com/documentation/computers/processors.html

## Sync between several Pis

mpv has no network sync of its own. A small agent on each Pi drives mpv over its JSON IPC: start at an agreed
wall-clock time (chrony / NTP on the LAN, or PTP), then nudge speed (0.99 / 1.01) or seek at each loop point (as the
old omxplayer-sync tools did). Expect about a frame of agreement, not genlock. While the computer runs, the Art-Net
timecode Phosphor already sends can be the shared clock; on takeover the Pis use their own clocks. MiniMad units on
one network sync automatically ("Sync Group"); info-beamer needs every device on the same OS release for synced
starts.

## Takeover when the computer stops

None of the existing Pi players does this; they just play from power-on. It's ours: the Pi listens for the
computer's sACN / Art-Net (and later a heartbeat); while it hears it, it stays dark or plays muted; after a few
seconds of silence it plays. Video takeover needs the HDMI to be the projector's input (or a switcher), so in
practice the Pi is either the main player for its projector or wired as a backup input.

## Making the SD card from Phosphor

- Image list: `https://downloads.raspberrypi.com/os_list_imagingutility_v4.json` (the one the Raspberry Pi Imager
  uses). Each OS entry has `url`, `extract_size`, `extract_sha256` (of the unpacked image), `release_date`,
  `init_format` (`cloudinit-rpi` on Trixie, `systemd` on older Bookworm images) and `devices`. A stable link to the
  newest Lite image: `https://downloads.raspberrypi.com/raspios_lite_arm64_latest`. Keep the download; fetch again
  only when a newer one is listed.
- First-boot settings (Trixie, from the November 2025 release on): cloud-init files on the card's FAT boot partition:
  `user-data`, `network-config` (Netplan v2, Wi-Fi under `wifis:`), `meta-data`; the `rpi:` module for Pi settings.
  That's where the Wi-Fi name and password, the device's name, the player and its key go.
  https://www.raspberrypi.com/news/cloud-init-on-raspberry-pi-os/
- Writing the card without a terminal: macOS `/usr/libexec/authopen` (the system's own admin prompt, then a
  handle to `/dev/rdiskN`; unmount first); Windows: admin rights (a helper with `requireAdministrator`), open
  `\\.\PhysicalDriveN`, lock and dismount its volumes, write whole sectors. rpi-imager (Apache-2.0) is the
  reference. No maintained permissive Rust crate does this: a small module per OS (`fatfs` crate, MIT, for writing
  the cloud-init files).

## Already done toward it

- Recording DMX that comes in (another program or a desk) as well as what goes out (0.5.11).
- DMX in listens on every network adapter; LED / DMX output goes out of a chosen adapter.
- Linux ARM builds of Phosphor in CI; mDNS discovery of other Phosphors (`control::discovery`).

## Next steps when we pick it up

1. Try mpv on a real Pi 5 and Pi 4 with Pi OS Lite Trixie: which options play our rendered files smoothly.
2. Render each output to the right file (`Render…` per output, Pi presets: "Pi 5 (HEVC)", "Pi 4 (H.264)").
3. The Pi agent: plays the video and the DMX recording, announces itself, accepts a show, takes over on silence.
4. "Set up a Pi…": download the image, write the card, add the cloud-init files.
5. Several Pis in sync.
