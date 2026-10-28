#!/bin/bash
# Makes a Phosphor DMG that opens on any Mac without warnings: Developer ID signature, Apple notarization, stapled.
# Needs (once, by the owner):
#   1. Apple Developer Program membership and a "Developer ID Application" certificate in the keychain
#      (Xcode › Settings › Accounts › Manage Certificates › + › Developer ID Application).
#   2. A notary login saved in the keychain:
#      xcrun notarytool store-credentials phosphor-notary --apple-id <your Apple ID> --team-id <TEAM ID>
#      (it asks for an app-specific password from appleid.apple.com).
# Then:  scripts/notarize-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."
PROFILE="${NOTARY_PROFILE:-phosphor-notary}"
SIGN_ID="${SIGN_ID:-$(security find-identity -v -p codesigning | grep -m1 'Developer ID Application' | sed -E 's/.*"(.*)"/\1/' || true)}"
if [[ -z "$SIGN_ID" ]]; then
  echo "No 'Developer ID Application' certificate in the keychain (see the top of this script)." >&2
  exit 1
fi
echo "signing as: $SIGN_ID"
SIGN_ID="$SIGN_ID" scripts/bundle-mac.sh --no-install

VERSION=$(grep -m1 '^version' crates/phosphor/Cargo.toml | cut -d'"' -f2)
DMG="dist/Phosphor-${VERSION}.dmg"
# The installer window: Phosphor, an arrow and Applications on the Phosphor picture.
scripts/make-dmg.sh dist/Phosphor.app "$DMG"
codesign --force --timestamp --sign "$SIGN_ID" "$DMG"

xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
spctl -a -t open --context context:primary-signature -vv "$DMG"
echo "ready: $DMG"
