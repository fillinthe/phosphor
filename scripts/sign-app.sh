#!/bin/bash
# Signs a Phosphor.app inside-out (libraries, framework, helper programs, then the app); --deep is not reliable.
# Usage: scripts/sign-app.sh <Phosphor.app> [identity]      identity "-" (default) = ad-hoc, for local builds;
# a certificate = hardened runtime + timestamp, what notarization needs (scripts/Phosphor.entitlements).
# Used by bundle-mac.sh for every build and by notarize-app-zip.sh to re-sign the Intel build GitHub made.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP="$1"
SIGN_ID="${2:--}"
if [[ "$SIGN_ID" == "-" ]]; then
  SIGN_ARGS=(--force --sign -)
else
  SIGN_ARGS=(--force --timestamp --options runtime --sign "$SIGN_ID")
fi
# Finder marks and other attributes on the bundle are "detritus" to codesign --strict.
xattr -cr "$APP"
for lib in "$APP"/Contents/Frameworks/*.dylib; do codesign "${SIGN_ARGS[@]}" "$lib" >/dev/null; done
codesign "${SIGN_ARGS[@]}" "$APP/Contents/Frameworks/Syphon.framework" >/dev/null
[[ -f "$APP/Contents/MacOS/ffmpeg" ]] && codesign "${SIGN_ARGS[@]}" "$APP/Contents/MacOS/ffmpeg" >/dev/null
# The camera extension, before the app around it (its own entitlements; a real one also needs its provisioning profile,
# see scripts/build-camera-extension.sh).
CAM="$APP/Contents/Library/SystemExtensions/PhosphorCamera.systemextension"
[[ -d "$CAM" ]] && codesign "${SIGN_ARGS[@]}" --entitlements "$ROOT/vendor/PhosphorCamera.entitlements" "$CAM" >/dev/null
codesign "${SIGN_ARGS[@]}" --entitlements "$ROOT/scripts/Phosphor.entitlements" "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
