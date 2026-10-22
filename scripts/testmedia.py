#!/usr/bin/env python3
"""Synthetic test media for beat tracking and Match the beat (nothing on this Mac is cut to music).

  python3 scripts/testmedia.py OUTDIR

Writes drums-<bpm>.wav (kick on every beat, accent on beat one, a syncopated kick every other bar, hats, a bass
note per bar) for 92, 128, 140 and 170 BPM, and cuts-128.mp4: a 1280x720 clip that hard-cuts to a new colour on
every beat of the 128 BPM track, with that track as its sound (needs ffmpeg on PATH).

Bench checks:
  PHOSPHOR_BENCH_BPM=128 phosphor --bench 30 --sound --movie OUTDIR/drums-128.wav --gen plasma --quick baroff
  phosphor --bench 20 --movie OUTDIR/cuts-128.mp4 --quick beat
"""
import math, os, random, struct, subprocess, sys, wave

SR = 48_000


def drums(path, bpm, secs=36):
    beat = 60 / bpm
    n = int(SR * secs)
    out = bytearray()
    random.seed(2)
    kick = lambda t: math.sin(2 * math.pi * (55 + 40 * math.exp(-t * 30)) * t) * math.exp(-t * 9)
    hat = lambda t: (random.random() * 2 - 1) * math.exp(-t * 60) * 0.35
    bass = lambda t, f: 0.25 * math.sin(2 * math.pi * f * t) * math.exp(-t * 2)
    for i in range(n):
        t = i / SR
        b = t / beat
        bi = int(b)
        frac = (b - bi) * beat
        v = (1.0 if bi % 4 == 0 else 0.55) * kick(frac)
        if bi % 8 == 2:  # syncopated kick on the "and" of two, every other bar
            o = b - 0.5
            oi = int(o)
            if oi == bi:
                v += 0.7 * kick((o - oi) * beat)
        o = b * 2
        v += hat((o - int(o)) * beat / 2)
        bar = b / 4
        v += bass((bar - int(bar)) * beat * 4, [55, 65, 49, 73][int(bar) % 4])
        s = int(max(-1, min(1, v * 0.8)) * 32767)
        out += struct.pack("<hh", s, s)
    w = wave.open(path, "wb")
    w.setnchannels(2)
    w.setsampwidth(2)
    w.setframerate(SR)
    w.writeframes(bytes(out))
    w.close()


def cuts(path, audio, bpm, secs=36, fps=30):
    """A clip that cuts to a new flat colour on every beat (ffmpeg draws the frames from a concat list)."""
    beat = 60 / bpm
    random.seed(3)
    lst = path + ".txt"
    with open(lst, "w") as f:
        t = 0.0
        k = 0
        while t < secs:
            d = min(beat, secs - t)
            col = "0x%02X%02X%02X" % (random.randrange(40, 255), random.randrange(40, 255), random.randrange(40, 255))
            f.write(f"file 'color=c={col}:s=1280x720:r={fps}:d={d:.4f}'\nduration {d:.4f}\n")
            t += d
            k += 1
    # lavfi sources can't go through concat demuxer; build a filter chain instead.
    parts = []
    t = 0.0
    random.seed(3)
    inputs = []
    i = 0
    while t < secs:
        d = min(beat, secs - t)
        col = "0x%02X%02X%02X" % (random.randrange(40, 255), random.randrange(40, 255), random.randrange(40, 255))
        inputs += ["-f", "lavfi", "-t", f"{d:.4f}", "-i", f"color=c={col}:s=1280x720:r={fps}"]
        parts.append(f"[{i}:v]")
        t += d
        i += 1
    os.remove(lst)
    chain = "".join(parts) + f"concat=n={i}:v=1:a=0[v]"
    subprocess.run(["ffmpeg", "-v", "error", "-y", *inputs, "-i", audio, "-filter_complex", chain, "-map", "[v]", "-map", f"{i}:a", "-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", path], check=True)


if __name__ == "__main__":
    out = sys.argv[1] if len(sys.argv) > 1 else "/tmp/phosphor-testmedia"
    os.makedirs(out, exist_ok=True)
    for bpm in (92, 128, 140, 170):
        drums(os.path.join(out, f"drums-{bpm}.wav"), bpm)
    cuts(os.path.join(out, "cuts-128.mp4"), os.path.join(out, "drums-128.wav"), 128)
    print("wrote", sorted(os.listdir(out)))
