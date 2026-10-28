#!/bin/bash
# The Mac installer: a disk image whose window shows Phosphor, an arrow and Applications (drag to install).
# Usage: scripts/make-dmg.sh <Phosphor.app> <out.dmg>
# The picture is macos/dmg/background.png + @2x (from background.html: re-render both after editing it, see there).
# Uses dmgbuild (MIT) from a build-only Python environment in target/ (made on first use; not part of the app).
set -euo pipefail
cd "$(dirname "$0")/.."
APP="$1"
OUT="$2"
VENV="${DMG_VENV:-target/dmg-venv}"
if [[ ! -x "$VENV/bin/dmgbuild" ]]; then
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q "dmgbuild==1.6.7"
fi
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
# One TIFF with both sizes: Finder picks the sharp one on Retina screens.
tiffutil -cathidpicheck macos/dmg/background.png macos/dmg/background@2x.png -out "$WORK/background.tiff" >/dev/null
rm -f "$OUT"
"$VENV/bin/dmgbuild" -s scripts/dmg-settings.py -D app="$APP" -D background="$WORK/background.tiff" "Phosphor" "$OUT" >/dev/null
echo "made $OUT"
