#!/usr/bin/env python3
"""Merges translated chunks (DIR/NN.json, each {english: translation}) into crates/phosphor/lang/CODE.json, keeping
only strings the app has (strings.json) and checking that each translation keeps the same {0} {1}… placeholders.
Usage: merge.py CODE DIR"""
import json, os, re, sys

ROOT = os.path.join(os.path.dirname(__file__), "..", "..")
code, folder = sys.argv[1], sys.argv[2]
keys = set(json.load(open(os.path.join(ROOT, "crates", "phosphor", "lang", "strings.json"), encoding="utf-8")))
out_path = os.path.join(ROOT, "crates", "phosphor", "lang", f"{code}.json")
merged = json.load(open(out_path, encoding="utf-8")) if os.path.exists(out_path) else {}
bad = []
for f in sorted(os.listdir(folder)):
    if f.endswith(".json"):
        for en, tr in json.load(open(os.path.join(folder, f), encoding="utf-8")).items():
            if en not in keys or not isinstance(tr, str):
                continue
            if sorted(re.findall(r"\{\d+\}", en)) != sorted(re.findall(r"\{\d+\}", tr)):
                bad.append(en)
                continue
            merged[en] = tr
merged = {k: v for k, v in sorted(merged.items()) if k in keys}
json.dump(merged, open(out_path, "w", encoding="utf-8"), ensure_ascii=False, indent=0, sort_keys=True)
missing = len(keys) - len(merged)
print(f"{code}: {len(merged)} strings, {missing} missing, {len(bad)} dropped (placeholders changed)")
for b in bad[:20]:
    print("  placeholders:", repr(b))
