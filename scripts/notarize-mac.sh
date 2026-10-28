#!/bin/bash
# The public Mac downloads, both opening on any Mac without warnings (Developer ID signature, Apple notarization,
# stapled):
#   dist/Phosphor-mac.zip  the app (the updater installs this one)
#   dist/Phosphor-mac.dmg  the installer window: drag Phosphor onto Applications (scripts/make-dmg.sh)
# Needs (once, by the owner):
#   1. Apple Developer Program membership and a "Developer ID Application" certificate in the keychain
#      (Xcode › Settings › Accounts › Manage Certificates › + › Developer ID Application).
#   2. A notary login saved in the keychain:
#      xcrun notarytool store-credentials phosphor-notary --apple-id <your Apple ID> --team-id <TEAM ID>
#      (it asks for an app-specific password from appleid.apple.com).
# Then:  scripts/notarize-mac.sh   (scripts/release-github.sh runs it by itself when the certificate is there)
set -euo pipefail
cd "$(dirname "$0")/.."
PROFILE="${NOTARY_PROFILE:-phosphor-notary}"
SIGN_ID="${SIGN_ID:-$(security find-identity -v -p codesigning | grep -m1 'Developer ID Application' | sed -E 's/.*"(.*)"/\1/' || true)}"
if [[ -z "$SIGN_ID" ]]; then
  echo "No 'Developer ID Application' certificate in the keychain (see the top of this script)." >&2
  exit 1
fi
echo "signing as: $SIGN_ID"
# (Never installed: /Applications keeps the local build, signed for this Mac.)
SIGN_ID="$SIGN_ID" scripts/bundle-mac.sh --no-install

# The app: Apple checks a zip of it, then the ticket is stapled to the app itself and it's zipped again.
rm -f dist/Phosphor-mac.zip
(cd dist && ditto -c -k --keepParent Phosphor.app Phosphor-mac.zip)
xcrun notarytool submit dist/Phosphor-mac.zip --keychain-profile "$PROFILE" --wait
xcrun stapler staple dist/Phosphor.app
rm -f dist/Phosphor-mac.zip
(cd dist && ditto -c -k --keepParent Phosphor.app Phosphor-mac.zip)
spctl -a -t exec -vv dist/Phosphor.app

# The installer: the stapled app inside, the disk image signed and notarized too.
DMG=dist/Phosphor-mac.dmg
scripts/make-dmg.sh dist/Phosphor.app "$DMG"
codesign --force --timestamp --sign "$SIGN_ID" "$DMG"
xcrun notarytool submit "$DMG" --keychain-profile "$PROFILE" --wait
xcrun stapler staple "$DMG"
spctl -a -t open --context context:primary-signature -vv "$DMG"
echo "ready: dist/Phosphor-mac.zip, $DMG"
