#!/bin/bash
# Makes a 3-second clip in each common show/edit format (needs an ffmpeg with x264/x265/SVT-AV1/libvpx, e.g.
# Homebrew's) and plays each through Phosphor's bench, printing what it decoded with.
#   scripts/codec-test.sh [phosphor binary]   (default: target/release/phosphor)
set -u
BIN="${1:-target/release/phosphor}"
D=$(mktemp -d /tmp/phosphor-codecs.XXXX)
make() { local n=$1; shift; ffmpeg -hide_banner -loglevel error -y -f lavfi -i testsrc2=size=640x360:rate=30 -t 3 "$@" "$D/$n" 2>/dev/null || echo "could not make $n"; }
make h264.mp4 -c:v libx264 -pix_fmt yuv420p
make h264_422_10bit.mov -c:v libx264 -pix_fmt yuv422p10le
make hevc.mp4 -c:v libx265 -tag:v hvc1 -pix_fmt yuv420p
make hevc_10bit.mov -c:v libx265 -pix_fmt yuv420p10le -tag:v hvc1
make av1.mp4 -c:v libsvtav1 -pix_fmt yuv420p
make vp8.webm -c:v libvpx
make vp9.webm -c:v libvpx-vp9
make vp9_alpha.webm -c:v libvpx-vp9 -pix_fmt yuva420p
make prores422.mov -c:v prores_ks -profile:v 2
make prores4444_alpha.mov -c:v prores_ks -profile:v 4 -pix_fmt yuva444p10le
make dnxhd.mov -c:v dnxhd -s 1280x720 -b:v 90M -pix_fmt yuv422p
make dnxhr.mxf -c:v dnxhd -profile:v dnxhr_sq -pix_fmt yuv422p
make mjpeg.avi -c:v mjpeg -q:v 3 -pix_fmt yuvj420p
make mpeg2.mpg -c:v mpeg2video
make mpeg4.avi -c:v mpeg4
make cineform.mov -c:v cfhd
make qtrle_animation.mov -c:v qtrle
make png.mov -c:v png
make animation.gif -c:v gif
make ffv1.mkv -c:v ffv1
make utvideo.avi -c:v utvideo
make huffyuv.avi -c:v huffyuv
make vc2.mov -c:v vc2
make wmv2.wmv -c:v wmv2
make flv_h264.flv -c:v libx264 -pix_fmt yuv420p
# Resolume's DXV (width/height multiples of 16).
ffmpeg -hide_banner -loglevel error -y -f lavfi -i testsrc2=size=640x352:rate=30 -t 3 -c:v dxv -pix_fmt rgba "$D/dxv.mov" 2>/dev/null || echo "could not make dxv.mov"
"$BIN" --bench 2 --window 480x300 --record "$D/hap.mov" --hap >/dev/null 2>&1; sleep 1
ok=0; bad=0
for f in "$D"/*; do
  out=$(timeout 40 "$BIN" --bench 3 --window 480x300 --movie "$f" 2>&1 | grep -E "^VIDEO|^LAYER_ERROR" | head -1)
  sleep 1  # let the last window close before the next opens
  if [[ "$out" == VIDEO* ]]; then ok=$((ok + 1)); else bad=$((bad + 1)); fi
  printf "%-24s %s\n" "$(basename "$f")" "${out#VIDEO Layer 1 (Forward): }"
done
echo "CODECS $ok played, $bad failed ($D)"
