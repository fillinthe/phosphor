#!/bin/bash
# Builds Phosphor's virtual camera (macos/PhosphorCamera, a Core Media I/O camera extension) into
# vendor/PhosphorCamera.systemextension; scripts/bundle-mac.sh puts it in Phosphor.app/Contents/Library/SystemExtensions.
#   scripts/build-camera-extension.sh [TEAM_ID]
#
# macOS only runs it when it's signed with a Developer ID and a provisioning profile that allows camera extensions,
# so built unsigned here it sits in the app unused. To make it work (the owner, with the Apple Developer account):
#   1. Pass your team ID (e.g. ABCDE12345): it goes into the extension's service name and app group, which must
#      start with it.
#   2. Developer portal: an App ID for co.fillintheblank.phosphor with "System Extension" and one for
#      co.fillintheblank.phosphor.camera with the app group TEAM_ID.co.fillintheblank.phosphor.camera; a Developer ID
#      provisioning profile for each.
#   3. Sign (inside out): the extension with Camera.entitlements and its profile copied in as
#      Contents/embedded.provisionprofile, then the app with com.apple.developer.system-extension.install added to
#      scripts/Phosphor.entitlements and its profile. Then scripts/notarize-mac.sh as usual.
set -euo pipefail
cd "$(dirname "$0")/.."
TEAM=${1:-TEAMID}
MIN_MACOS=$(cat scripts/min-macos)
VERSION=$(grep -m1 '^version' crates/phosphor/Cargo.toml | cut -d'"' -f2)
OUT=vendor/PhosphorCamera.systemextension
WORK=$(mktemp -d)
for arch in arm64 x86_64; do
  xcrun swiftc -O -target "$arch-apple-macos$MIN_MACOS" -emit-executable -o "$WORK/camera-$arch" \
    macos/PhosphorCamera/main.swift macos/PhosphorCamera/Provider.swift \
    -framework CoreMediaIO -framework CoreMedia -framework CoreVideo -framework IOKit
done
rm -rf "$OUT"
mkdir -p "$OUT/Contents/MacOS"
lipo -create -output "$OUT/Contents/MacOS/co.fillintheblank.phosphor.camera" "$WORK/camera-arm64" "$WORK/camera-x86_64"
sed -e "s/TEAMID/$TEAM/" -e "s/VERSION/$VERSION/g" macos/PhosphorCamera/Info.plist > "$OUT/Contents/Info.plist"
sed -e "s/TEAMID/$TEAM/" macos/PhosphorCamera/Camera.entitlements > "$WORK/Camera.entitlements"
cp "$WORK/Camera.entitlements" vendor/PhosphorCamera.entitlements
rm -rf "$WORK"
echo "built $OUT (team $TEAM; unsigned: sign it as described at the top of this script)"
