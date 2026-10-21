#!/bin/sh
# Performance suite: makes test clips (4K Hap with Phosphor's own encoder, 4K H.264 / HEVC and 1080p ProRes with
# FFmpeg), then plays 4 layers of each at 60 fps and prints one line per run (fps, CPU, GPU, memory, worst frame gap).
# Silent (no sound in the clips), temp HOME. Compare with the targets in docs/VDMX-REVIEW.md.
# Usage: scripts/perf-suite.sh [path/to/phosphor] [outdir] [seconds]
BIN=${1:-target/release/phosphor}
OUT=${2:-/tmp/phosphor-perf}
SECS=${3:-15}
mkdir -p "$OUT"
H=$(mktemp -d)
# A stalled run is cut off (GNU timeout where there is one).
T=$(command -v timeout || command -v gtimeout || true)
bench() { HOME=$H ${T:+$T $((SECS + 90))} "$BIN" --rate 60 "$@" 2>&1; }

# Test clips (made once; delete the folder to make them again).
if [ ! -f "$OUT/hap4k.mov" ]; then
  cat > "$OUT/make4k.phosphor" <<EOF
{"width": 3840, "height": 2160, "next_id": 9, "layers": [{"id": 1, "name": "Plasma", "source": {"Generator": "plasma"}}]}
EOF
  bench --bench 12 --project "$OUT/make4k.phosphor" --record "$OUT/hap4k.mov" --hap | grep -E "RECORDED|error" >&2
fi
[ -f "$OUT/h264_4k.mp4" ] || ffmpeg -v error -y -f lavfi -i "testsrc2=size=3840x2160:rate=60:duration=10" -c:v h264_videotoolbox -b:v 40M -pix_fmt yuv420p "$OUT/h264_4k.mp4"
[ -f "$OUT/hevc_4k.mp4" ] || ffmpeg -v error -y -f lavfi -i "testsrc2=size=3840x2160:rate=60:duration=10" -c:v hevc_videotoolbox -b:v 30M -tag:v hvc1 -pix_fmt yuv420p "$OUT/hevc_4k.mp4"
[ -f "$OUT/p4444_1080.mov" ] || ffmpeg -v error -y -f lavfi -i "testsrc2=size=1920x1080:rate=60:duration=10,format=yuva444p10le" -c:v prores_ks -profile:v 4 -pix_fmt yuva444p10le "$OUT/p4444_1080.mov"
[ -f "$OUT/prores_1080.mov" ] || ffmpeg -v error -y -f lavfi -i "testsrc2=size=1920x1080:rate=60:duration=10" -c:v prores_videotoolbox -profile:v 2 "$OUT/prores_1080.mov"

# 4 layers of one clip, on a canvas of the clip's size.
project() { # file w h
  cat > "$OUT/p.phosphor" <<EOF
{"width": $2, "height": $3, "next_id": 20, "layers": [
 {"id": 1, "name": "A", "source": {"Movie": {"name": "a", "path": "$1"}}, "opacity": 0.5},
 {"id": 2, "name": "B", "source": {"Movie": {"name": "b", "path": "$1"}}, "opacity": 0.5, "blend": "Add"},
 {"id": 3, "name": "C", "source": {"Movie": {"name": "c", "path": "$1"}}, "opacity": 0.5, "blend": "Screen"},
 {"id": 4, "name": "D", "source": {"Movie": {"name": "d", "path": "$1"}}, "opacity": 0.5}]}
EOF
}

echo "PERF clip | fps | cpu % (one core = 100) | cpu ms/frame | gpu ms | render ms | worst gap ms | memory MB"
for spec in "hap4k.mov 3840 2160" "h264_4k.mp4 3840 2160" "hevc_4k.mp4 3840 2160" "prores_1080.mov 1920 1080" "p4444_1080.mov 1920 1080"; do
  set -- $spec
  [ -f "$OUT/$1" ] || { echo "PERF $1 | missing"; continue; }
  project "$OUT/$1" "$2" "$3"
  # A GUI bench stalls while the screen is in use (a gap of seconds between frames): run it again, once.
  S=$(bench --bench "$SECS" --project "$OUT/p.phosphor" | grep '^SUMMARY' | head -1)
  if printf '%s' "$S" | python3 -c 'import json,sys; l=sys.stdin.read(); b=l[8:]; sys.exit(0 if l and json.loads(b[:b.rindex("}")+1]).get("worst_ms",0) < 5000 else 1)'; then :; else
    echo "PERF $1: stalled, running again" >&2
    S=$(bench --bench "$SECS" --project "$OUT/p.phosphor" | grep '^SUMMARY' | head -1)
  fi
  # SUMMARY {json} cpu_ms_per_frame=x
  printf '%s\n' "$S" | python3 -c '
import json, sys
line = sys.stdin.read().strip()
if not line:
    print("PERF " + sys.argv[1] + " | no SUMMARY (stalled?)"); sys.exit()
body = line[len("SUMMARY "):]
j = json.loads(body[: body.rindex("}") + 1])
per = body.rsplit("cpu_ms_per_frame=", 1)[-1]
print("PERF %s x4 | %.1f | %.0f | %s | %.2f | %.2f | %.0f | %.0f" % (sys.argv[1], j["fps"], j["cpu_percent"], per, j.get("gpu_ms", 0), j.get("render_ms", 0), j.get("worst_ms", 0), j["memory_mb"]))
' "$1"
done
rm -rf "$H"
du -sh "$OUT"/*.mov "$OUT"/*.mp4 2>/dev/null | sed 's/^/SIZE /'
