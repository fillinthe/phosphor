# Performance

Measured with `scripts/perf-suite.sh` (release build, 15 s per run, `--rate 60`, temp HOME, silent). Four layers of
the same clip, blended (Normal, Add, Screen, Normal at 50%), on a canvas the clip's size. CPU is the whole app
(UI included) where 100% = one core.

## 2026-10-21, Apple M5 (10 cores), macOS 26

| 4 layers of… | fps | CPU | CPU over idle | GPU ms/frame | memory |
|---|---|---|---|---|---|
| nothing (idle UI at 60 fps) | 59.5 | 26% | — | 0.09 | 212 MB |
| 4K Hap (Phosphor's encoder) | 59.9 | 63% | 37% | 0.40 | 605 MB |
| 4K H.264 (40 Mb/s) | 59.9 | 40–46% | ~17% | 1.1–1.4 | 330 MB |
| 4K HEVC (30 Mb/s) | 59.9 | 55–62% | ~33% | 1.2–1.3 | 490 MB |
| 1080p ProRes 422 | 60.0 | 64% | 38% | 1.7 | 242 MB |
| 1080p ProRes 4444 with alpha | 59.9 | 45% | ~19% | 0.43 | 272 MB |

Every run holds 60 fps; the longest gap between two frames was 34–37 ms (one late frame in 15 s, also seen idle).

**Fixed while measuring:** ProRes 422 was decoded in hardware, then copied out and converted on the CPU, because
VideoToolbox hands it over as 4:2:2 and the zero-copy path only took 4:2:0. It now goes straight to the GPU too
(116% → 64% CPU, 392 → 242 MB), and keeps its full colour resolution.

**Fixed while measuring, 2:** ProRes 4444 with alpha was decoded entirely in software (478% CPU, 989 MB for four
layers, frames skipped). VideoToolbox decodes it with alpha, as one 16-bit plane of A, Y, Cb, Cr ('y416'), which
now goes straight to the GPU and is converted in the shader: **45% CPU, 272 MB**, colours closer to the true values
(75% bars at 189–190 of an expected 191; the old CPU conversion gave 176–186) and the alpha unchanged.

Note: a GUI bench stalls while someone is using the screen (frames seconds apart). The suite re-runs a stalled clip
once; numbers from a stalled run (worst gap in seconds) don't count.

## Against the targets (docs/VDMX-REVIEW.md)

- **4 layers of 4K Hap at 60 fps, under 10% CPU on an M1:** 60 fps holds. 63% of one core = 6% of this 10-core
  Mac; most of it is unpacking 4K Hap frames (~1 GB/s at 60 fps × 4) and the UI (26%). Not measured on an M1.
- **Bundle under 30 MB:** the download is 31 MB zipped, the app 67 MB installed. Most of it is FFmpeg and the 327
  Vidvox ISF shaders.
- **VDMX side by side:** VDMX6 Plus is on this Mac, but driving it means taking over the mouse; not run.
