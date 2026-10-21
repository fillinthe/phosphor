# Phosphor on Linux (Raspberry Pi 5 and other 64-bit ARM boards)

**Preview.** Built for 64-bit Raspberry Pi OS (Bookworm) / Ubuntu 24.04 on ARM. The main use: a **player** that
follows a Phosphor on a Mac over the network and shows its outputs in sync (Follow another Phosphor).

## Run it

1. Unpack: `tar xzf Phosphor-linux-arm64.tar.gz`
2. Make sure these are installed (Raspberry Pi OS with a desktop has most of them):
   `sudo apt install libxkbcommon-x11-0 libasound2 libvulkan1 mesa-vulkan-drivers`
3. Start: `Phosphor/Phosphor.sh`. To follow a Mac at start: in Phosphor, Session › Follow another Phosphor, pick
   the Mac, and tick "Follow at launch".

The Raspberry Pi 5's GPU runs Phosphor through Vulkan. FFmpeg comes in the `lib` folder next to the program.
Not available on Linux: Syphon, NDI, Spout, screen capture, Apple-only video tracking.
