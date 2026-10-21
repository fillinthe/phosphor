#!/bin/sh
# Opens every panel once in a short bench run (temp HOME, silent) and fails if any of them panics when drawn.
# Usage: scripts/panel-check.sh [path/to/phosphor]   (default: the bundle's binary)
BIN=${1:-dist/Phosphor.app/Contents/MacOS/Phosphor}
H=$(mktemp -d)
OUT=$(HOME=$H timeout 180 "$BIN" --bench 7 --tab all 2>&1)
rm -rf "$H"
TABS=$(printf '%s\n' "$OUT" | grep -c '^TAB ')
if printf '%s\n' "$OUT" | grep -q 'panicked' || ! printf '%s\n' "$OUT" | grep -q '^SUMMARY'; then
  printf '%s\n' "$OUT" | grep -E '^TAB |panicked' | tail -4
  echo "PANEL_CHECK failed (after $TABS panels)"; exit 1
fi
echo "PANEL_CHECK ok ($TABS panels)"
