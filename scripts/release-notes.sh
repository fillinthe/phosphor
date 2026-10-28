#!/bin/sh
# Prints a release's notes: its CHANGELOG.md section plus how to install.
# Usage: scripts/release-notes.sh 0.2.51
set -e
cd "$(dirname "$0")/.."
VER="$1"
SECTION=$(awk -v v="$VER" '/^## /{on = ($2 == v)} on && !/^## /' CHANGELOG.md)
if [ -z "$SECTION" ]; then
  echo "CHANGELOG.md has no section for $VER" >&2; exit 1
fi
echo "## What's new in $VER"
# One line per bullet (GitHub shows the changelog's wrapped lines as line breaks).
echo "$SECTION" | sed '/./,$!d' | awk 'NR > 1 && /^  [^ ]/ { sub(/^ +/, ""); line = line " " $0; next } { if (NR > 1) print line; line = $0 } END { print line }'
echo
echo "Full history: [CHANGELOG.md](https://github.com/fillinthe/phosphor/blob/main/CHANGELOG.md)"
echo
echo "## Install"
case "$VER" in
  0.[0-3].*|0.4.0) echo "**Mac (Apple silicon):** download Phosphor-mac.zip, unzip, drag Phosphor to Applications and open it. It's signed and notarized by Apple, so it opens without warnings." ;;
  *) echo "**Mac (Apple silicon):** download Phosphor-mac.dmg, open it and drag Phosphor onto Applications (or unzip Phosphor-mac.zip). It's signed and notarized by Apple, so it opens without warnings." ;;
esac
case "$VER" in
  0.2.[0-9]|0.2.[1-4][0-9]) ;;
  *) echo
     echo "**Windows 10/11 (preview):** run Phosphor-windows-setup.exe (or unzip Phosphor-windows.zip anywhere and run Phosphor.exe). Windows warns once because the app isn't signed: **More info**, **Run anyway**." ;;
esac
case "$VER" in
  0.[0-2].*|0.3.[01]) ;;
  *) echo
     echo "**Older Intel Macs (macOS 10.15 Catalina to 12):** download Phosphor-mac-intel.zip, unzip, drag Phosphor to Applications. The first time, right-click Phosphor › **Open** › Open (it isn't notarized yet). Same app without screen / window capture and hearing the Mac's sound." ;;
esac
echo
echo "After that, Phosphor updates itself (Help › Check for Updates…)."
