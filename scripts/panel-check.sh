#!/bin/sh
# Opens every panel once in a short bench run (temp HOME, silent) and fails if any of them panics when drawn.
# Usage: scripts/panel-check.sh [path/to/phosphor]   (default: the bundle's binary)
BIN=${1:-dist/Phosphor.app/Contents/MacOS/Phosphor}
# (GNU timeout where there is one; macOS has none built in.)
T=$(command -v timeout || command -v gtimeout || true)
run() {
  H=$(mktemp -d)
  OUT=$(HOME=$H ${T:+$T 180} "$BIN" --bench 7 --tab all 2>&1)
  rm -rf "$H"
  TABS=$(printf '%s\n' "$OUT" | grep -c '^TAB ')
  TOTAL=$(printf '%s\n' "$OUT" | sed -n 's/^TAB_TOTAL //p')
}
bad() {
  printf '%s\n' "$OUT" | grep -q 'panicked' || ! printf '%s\n' "$OUT" | grep -q '^SUMMARY' || [ "$TABS" != "$TOTAL" ]
}
run
# A GUI bench can stall while the screen is in use (no panic, just no SUMMARY): try once more before failing.
if bad && ! printf '%s\n' "$OUT" | grep -q 'panicked'; then
  echo "panel check: no result (stalled?), trying again"
  run
fi
if bad; then
  printf '%s\n' "$OUT" | grep -E '^TAB |panicked' | tail -4
  echo "PANEL_CHECK failed (after $TABS panels)"; exit 1
fi
echo "PANEL_CHECK ok ($TABS panels)"
