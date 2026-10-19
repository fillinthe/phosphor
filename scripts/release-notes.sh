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
echo "**Mac (Apple silicon):** download Phosphor-mac.zip, unzip, drag Phosphor to Applications. First launch: macOS says it can't verify Phosphor (not notarized yet): click Done, then System Settings › Privacy & Security › scroll down › **Open Anyway**, once."
case "$VER" in
  0.2.[0-9]|0.2.[1-4][0-9]) ;;
  *) echo
     echo "**Windows 10/11 (preview):** download Phosphor-windows.zip, unzip anywhere, run Phosphor.exe. Windows warns once because the app isn't signed: **More info**, **Run anyway**." ;;
esac
echo
echo "After that, Phosphor updates itself (Help › Check for Updates…)."
