"""Sorts an isf_sheet sheet.tsv: drops errors, blank, untouched, hard-flashing and near-duplicate shaders.
Usage: pick.py sheet.tsv left.tsv (left.tsv: one line per group, the best first, then its look-alikes)."""
import sys, collections
rows = [l.rstrip("\n").split("\t") for l in open(sys.argv[1])]
err = [r for r in rows if r[2]]
ok = [r for r in rows if not r[2]]
tally = collections.Counter()
keep = []
for r in ok:
    slug, name, _, spread, change, motion, flicker, colour, thumb = r
    spread, change, motion, flicker, colour = map(float, (spread, change, motion, flicker, colour))
    if spread < 8: tally["flat/blank"] += 1; continue
    if change < 6: tally["picture untouched"] += 1; continue
    if flicker > 25: tally["flickers hard"] += 1; continue
    t = bytes.fromhex(thumb)
    keep.append(dict(slug=slug, spread=spread, change=change, motion=motion, flicker=flicker, colour=colour, t=t))
# near-duplicates: greedy clusters by thumbnail distance
keep.sort(key=lambda k: -(k["spread"] + k["colour"] * 0.5 + min(k["motion"], 40)))
groups = []
for k in keep:
    for g in groups:
        d = sum(abs(a - b) for a, b in zip(k["t"], g[0]["t"])) / len(k["t"])
        if d < 10:
            g.append(k); break
    else:
        groups.append([k])
tally["errors"] = len(err)
tally["near-duplicates dropped"] = sum(len(g) - 1 for g in groups)
for k, v in sorted(tally.items()): print(f"{v:5} {k}")
print(f"{len(groups):5} left")
open(sys.argv[2], "w").write("\n".join(g[0]["slug"] + "\t" + ",".join(x["slug"] for x in g[1:]) for g in groups) + "\n")
