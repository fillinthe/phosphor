#!/usr/bin/env python3
"""lostjared/shaders GLSL 330 effects -> ISF effects. Usage: lj2isf.py SRC_DIR OUT_DIR. Prints a tally.
Then render them with the isf_sheet test (crates/phosphor/src/isf_lib.rs) and sort with pick.py."""
import json, os, re, sys, collections

src, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)

KNOWN = {"samp", "time_f", "iTime", "iResolution", "amp_peak", "amp_smooth", "amp_low", "amp_mid", "amp_high",
         "amp_rms", "amp", "iamp", "iMouse", "spectrum", "spectrum0"}
SKIP_IF = ["history", "spectrum_history", "mv_matrix", "proj_matrix"]

HELPERS = """
float lj_spec(float x) { return IMG_NORM_PIXEL(audioFFT, vec2(clamp(x, 0.0, 1.0), 0.5)).r; }
vec4 lj_spec4(float x) { return vec4(lj_spec(x)); }
float lj_band(float a, float b) { return (lj_spec(a) + lj_spec(mix(a, b, 0.33)) + lj_spec(mix(a, b, 0.66)) + lj_spec(b)) * 0.25; }
#define amp_low lj_band(0.0, 0.08)
#define amp_mid lj_band(0.08, 0.35)
#define amp_high lj_band(0.35, 0.9)
#define amp_rms lj_band(0.0, 0.5)
#define amp_smooth lj_band(0.0, 0.5)
#define amp_peak max(lj_band(0.0, 0.1), lj_band(0.1, 0.5))
#define amp lj_band(0.0, 0.5)
#define iamp lj_band(0.0, 0.5)
#define iMouse vec4(RENDERSIZE * 0.5, 0.0, 0.0)
"""
# Renamed in the text (the converter rewrites image reads before the preprocessor runs).
RENAME = {"time_f": "TIME", "iTime": "TIME", "iResolution": "RENDERSIZE", "tc": "isf_FragNormCoord", "samp": "inputImage"}

def replace_calls(code, name, repl):
    """texture(name, X) / texture1D(name, X) -> repl(X), with balanced parentheses."""
    pat = re.compile(r"\btexture(?:1D)?\s*\(\s*" + name + r"\s*,")
    while True:
        m = pat.search(code)
        if not m:
            return code
        i, depth = m.end(), 1
        while i < len(code) and depth:
            depth += {"(": 1, ")": -1}.get(code[i], 0)
            i += 1
        arg = code[m.end():i - 1]
        code = code[:m.start()] + repl + "(" + arg + ")" + code[i:]

tally = collections.Counter()
for root, dirs, files in os.walk(src):
    if "/compute" in root or root.endswith("compute"):
        continue
    for f in sorted(files):
        if not f.endswith(".glsl"):
            continue
        code = open(os.path.join(root, f), errors="replace").read()
        uniforms = re.findall(r"^\s*uniform\s+\w+\s+(\w+)", code, re.M)
        if any(u in SKIP_IF for u in uniforms):
            tally["skip: history/3D"] += 1
            continue
        extra = [u for u in uniforms if u not in KNOWN]
        if extra:
            tally["skip: other uniforms"] += 1
            continue
        if not re.search(r"^\s*in\s+vec2\s+tc\s*;", code, re.M) or not re.search(r"^\s*out\s+vec4\s+(\w+)\s*;", code, re.M):
            tally["skip: no tc/out"] += 1
            continue
        outname = re.search(r"^\s*out\s+vec4\s+(\w+)\s*;", code, re.M).group(1)
        code = re.sub(r"^\s*#version.*$", "", code, flags=re.M)
        code = re.sub(r"^\s*uniform\s+[^;]*;", "", code, flags=re.M)
        code = re.sub(r"^\s*in\s+vec2\s+tc\s*;", "", code, flags=re.M)
        code = re.sub(r"^\s*out\s+vec4\s+\w+\s*;", "", code, flags=re.M)
        code = re.sub(r"^\s*precision\s+[^;]*;", "", code, flags=re.M)
        for s in ("spectrum0", "spectrum"):
            code = replace_calls(code, s, "lj_spec4")
        if re.search(r"\bspectrum0?\b", code):
            tally["skip: spectrum used oddly"] += 1
            continue
        uses_audio = bool(re.search(r"\b(amp\w*|iamp|lj_spec)\b", code))
        stem = os.path.splitext(f)[0]
        desc = ""
        m = re.search(r"^//\s*(.+)$", code, re.M)
        lines = [l[2:].strip() for l in code.splitlines() if l.startswith("//")]
        if len(lines) > 1:
            desc = lines[1]
        header = {"DESCRIPTION": desc, "CREDIT": "Jared Bruni (lostjared/shaders, MIT)", "ISFVSN": "2",
                  "CATEGORIES": ["lostjared"], "INPUTS": [{"NAME": "inputImage", "TYPE": "image"}]}
        if uses_audio:
            header["INPUTS"].append({"NAME": "audioFFT", "TYPE": "audioFFT"})
            helpers = HELPERS
        else:
            helpers = "\n".join(l for l in HELPERS.splitlines() if not ("lj_" in l or "amp" in l))
        for a, b in list(RENAME.items()) + [(outname, "gl_FragColor")]:
            code = re.sub(r"\b" + a + r"\b", b, code)
        body = helpers + "\n" + code
        open(os.path.join(out, stem + ".fs"), "w").write("/*" + json.dumps(header, indent=2) + "*/\n" + body)
        tally["written" + (" (audio)" if uses_audio else "")] += 1
for k, v in sorted(tally.items()):
    print(f"{v:5} {k}")
