#!/bin/bash
# Makes a zipped Phosphor.app that someone else built (GitHub's Intel Mac build, signed ad-hoc there) open on any Mac
# without a warning: signs it with the Developer ID certificate, has Apple notarize it, staples the ticket and zips it
# again in place. Without this macOS says it "could not verify" the app and people have to allow it in System Settings.
# Usage: scripts/notarize-app-zip.sh <Phosphor-mac-intel.zip>     (same certificate and notary login as notarize-mac.sh)
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ZIP="$(cd "$(dirname "$1")" && pwd)/$(basename "$1")"
PROFILE="${NOTARY_PROFILE:-phosphor-notary}"
SIGN_ID="${SIGN_ID:-$(security find-identity -v -p codesigning | grep -m1 'Developer ID Application' | sed -E 's/.*"(.*)"/\1/' || true)}"
[[ -n "$SIGN_ID" ]] || { echo "No 'Developer ID Application' certificate in the keychain." >&2; exit 1; }
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
ditto -x -k "$ZIP" "$WORK"
[[ -d "$WORK/Phosphor.app" ]] || { echo "No Phosphor.app in $ZIP" >&2; exit 1; }
"$ROOT/scripts/sign-app.sh" "$WORK/Phosphor.app" "$SIGN_ID"
"$ROOT/scripts/zip-app.sh" "$WORK" Phosphor.app upload.zip
xcrun notarytool submit "$WORK/upload.zip" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$WORK/Phosphor.app"
"$ROOT/scripts/zip-app.sh" "$WORK" Phosphor.app out.zip
spctl -a -t exec -vv "$WORK/Phosphor.app"
mv "$WORK/out.zip" "$ZIP"
echo "notarized: $ZIP"
