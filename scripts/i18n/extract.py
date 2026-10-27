#!/usr/bin/env python3
"""Collects the interface text from the app's source into crates/phosphor/lang/strings.json (the list the language
packs translate). format! placeholders become {0}, {1}… in order. Run after changing UI text; the i18n test reports
strings a language pack doesn't have yet."""
import json, os, re, sys

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
SRCS = [os.path.join(ROOT, "crates", c, "src") for c in ("phosphor", "phosphor-engine")]
OUT = os.path.join(ROOT, "crates", "phosphor", "lang", "strings.json")

LIT = re.compile(r'(?<![A-Za-z0-9_#])"((?:[^"\\\n]|\\.)*)"')
PLACE = re.compile(r"\{\{|\}\}|\{[^{}]*\}")
# Calls whose string arguments are never shown (logs, ids, keys, paths, errors for developers).
SKIP_CALL = re.compile(r"(log::\w+!|info!|warn!|error!|debug!|trace!|eprintln!|println!|panic!|assert\w*!|unreachable!|expect\(|Id::new\(|id_salt\(|\.get_temp|env::var|var_os\(|Command::new\(|\.arg\(|\.args\(|include_str!|concat!|\.join\(|with_extension|ends_with\(|starts_with\(|contains\(|strip_prefix\(|strip_suffix\(|split\(|find\(|replace\(|matches!|ui\.push_id|push_id\()")

def unescape(s):
    return s.replace('\\"', '"').replace("\\n", "\n").replace("\\t", "\t").replace("\\\\", "\\").replace("\\'", "'")

def shown(text):
    t = text.strip()
    if len(t) < 2 or not re.search(r"[A-Za-z]{2}", t):
        return False
    if re.fullmatch(r"[a-z0-9_.:/\-]+", t):  # ids, keys, file names, paths
        return False
    if re.fullmatch(r"[A-Z0-9_]+", t) and len(t) > 4 and "_" in t:  # ENV_NAMES
        return False
    if t.startswith(("/", "http", "#", "--", "fs_", "vs_", "isf:", "ffgl", "wgsl", "@", "{\"", "<", "PHOSPHOR_")):
        return False
    if re.search(r"\.(rs|wgsl|glsl|json|ron|png|mov|mp4|txt|fs|vs)$", t):
        return False
    if any(c in t for c in ("vec4", "vec2", "uniform", "fn ", "let ", "::", "=>")):
        return False
    return True

def template(s):
    n = [0]
    def rep(m):
        g = m.group(0)
        if g in ("{{", "}}"):
            return g[0]
        r = "{%d}" % n[0]
        n[0] += 1
        return r
    return PLACE.sub(rep, s)

def in_tests(text, pos):
    i = text.rfind("#[cfg(test)]", 0, pos)
    return i != -1 and "\nmod tests" in text[i:i + 80]

def main():
    keys = set()
    for dirpath, _, files in (w for src in SRCS for w in os.walk(src)):
        for f in files:
            if not f.endswith(".rs") or f in ("tests.rs", "gpu_tests.rs") or "shaders" in dirpath and f.endswith(".wgsl"):
                continue
            text = open(os.path.join(dirpath, f), encoding="utf-8").read()
            for m in LIT.finditer(text):
                if in_tests(text, m.start()):
                    continue
                line_start = text.rfind("\n", 0, m.start()) + 1
                before = text[line_start:m.start()]
                if before.lstrip().startswith("//") or SKIP_CALL.search(before[-60:]):
                    continue
                raw = unescape(m.group(1))
                fmt = "format!(" in before[-40:] or "{" in raw
                s = template(raw) if fmt else raw
                if shown(re.sub(r"\{\d+\}", "", s)):
                    keys.add(s)
    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    json.dump(sorted(keys), open(OUT, "w", encoding="utf-8"), ensure_ascii=False, indent=0)
    print(f"{len(keys)} strings -> {OUT}")

if __name__ == "__main__":
    main()
