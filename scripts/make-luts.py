#!/usr/bin/env python3
"""Writes the colour-grading looks Phosphor ships (assets/luts/*.cube, 17-point 3D LUTs) from simple formulas, so they
are ours to give away. Usage: scripts/make-luts.py [out_dir]"""
import math, os, sys

out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(os.path.dirname(__file__), "..", "assets", "luts")
os.makedirs(out, exist_ok=True)
N = 17

def clamp(x): return max(0.0, min(1.0, x))
def luma(r, g, b): return 0.2126 * r + 0.7152 * g + 0.0722 * b
def sat(r, g, b, k):
    y = luma(r, g, b)
    return (y + (r - y) * k, y + (g - y) * k, y + (b - y) * k)
def contrast(v, k): return 0.5 + (v - 0.5) * k
def smooth(v): return v * v * (3 - 2 * v)

LOOKS = {
    "Warm": lambda r, g, b: (r * 1.06 + 0.02, g * 1.01, b * 0.88),
    "Cool": lambda r, g, b: (r * 0.9, g * 0.99, b * 1.08 + 0.02),
    "Teal and Orange": lambda r, g, b: (lambda y: (
        r + (0.10 if y > 0.5 else -0.04) * (abs(y - 0.5) * 2),
        g + (0.02 if y > 0.5 else 0.03) * (abs(y - 0.5) * 2),
        b + (-0.10 if y > 0.5 else 0.10) * (abs(y - 0.5) * 2)))(luma(r, g, b)),
    "Bleach Bypass": lambda r, g, b: tuple(contrast(v, 1.25) for v in sat(r, g, b, 0.45)),
    "Mono Contrast": lambda r, g, b: (lambda y: (y, y, y))(smooth(clamp(contrast(luma(r, g, b), 1.3)))),
    "Cross Process": lambda r, g, b: (smooth(r), g ** 0.9, b * 0.8 + 0.12),
    "Faded Film": lambda r, g, b: tuple(0.08 + v * 0.84 for v in sat(r, g, b, 0.75)),
    "Night": lambda r, g, b: (lambda y: (y * 0.55, y * 0.75, y * 1.0 + 0.04))(luma(r, g, b) ** 1.3),
}

for name, f in LOOKS.items():
    lines = [f'TITLE "{name}"', f"LUT_3D_SIZE {N}", ""]
    for bi in range(N):
        for gi in range(N):
            for ri in range(N):
                r, g, b = ri / (N - 1), gi / (N - 1), bi / (N - 1)
                o = f(r, g, b)
                lines.append(" ".join(f"{clamp(v):.6f}" for v in o))
    with open(os.path.join(out, f"{name}.cube"), "w") as fh:
        fh.write("\n".join(lines) + "\n")
print(f"{len(LOOKS)} looks in {out}")
