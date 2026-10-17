#!/bin/bash
# Builds Phosphor.app (release) with its icon, signs it, and installs it to /Applications.
#   scripts/bundle-mac.sh            build + install (ad-hoc signed: runs on this Mac)
#   scripts/bundle-mac.sh --no-install
#   SIGN_ID="Developer ID Application: …" scripts/bundle-mac.sh --no-install
#       signs for other Macs (hardened runtime + entitlements); then scripts/notarize-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' crates/phosphor/Cargo.toml | cut -d'"' -f2)

cargo build --release -p phosphor

APP=dist/Phosphor.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/phosphor "$APP/Contents/MacOS/Phosphor"

# Syphon output (BSD). Built once by scripts/build-syphon.sh; the app runs without it.
[[ -d vendor/Syphon.framework ]] || scripts/build-syphon.sh
mkdir -p "$APP/Contents/Frameworks"
cp -R vendor/Syphon.framework "$APP/Contents/Frameworks/"
cp vendor/Syphon-LICENSE.txt "$APP/Contents/Resources/"

# The ffmpeg command (streaming, ProRes / H.264 recording) rides along; its libraries are relinked below.
FFMPEG_BIN="$(command -v ffmpeg || true)"
if [[ -n "$FFMPEG_BIN" ]]; then
  cp "$(readlink -f "$FFMPEG_BIN")" "$APP/Contents/MacOS/ffmpeg"
  chmod 755 "$APP/Contents/MacOS/ffmpeg"
fi

# FFmpeg and its libraries, so the app (and exported shows) run on Macs without Homebrew.
python3 -I scripts/bundle-dylibs.py "$APP"

# Icon: every size macOS asks for, from the 1024 px master.
ICONSET=$(mktemp -d)/Phosphor.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Phosphor.icns"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Phosphor</string>
  <key>CFBundleDisplayName</key><string>Phosphor</string>
  <key>CFBundleIdentifier</key><string>co.fillintheblank.phosphor</string>
  <key>CFBundleExecutable</key><string>Phosphor</string>
  <key>CFBundleIconFile</key><string>Phosphor</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>$(git rev-list --count HEAD 2>/dev/null || echo 1)</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.video</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSCameraUsageDescription</key><string>Phosphor uses cameras and capture cards as live video layers.</string>
  <key>NSMicrophoneUsageDescription</key><string>Phosphor listens to audio input so visuals can react to music.</string>
  <key>NSLocalNetworkUsageDescription</key><string>Phosphor receives OSC from controllers and streams to viewers on your network.</string>
  <key>NSBonjourServices</key><array><string>_osc._udp</string></array>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>Phosphor Project</string>
    <key>CFBundleTypeExtensions</key><array><string>phosphor</string></array>
    <key>CFBundleTypeRole</key><string>Editor</string>
  </dict></array>
</dict>
</plist>
PLIST

# Sign inside-out (libraries, framework, then the app); --deep is not reliable for this.
SIGN_ID="${SIGN_ID:--}"
if [[ "$SIGN_ID" == "-" ]]; then
  SIGN_ARGS=(--force --sign -)
else
  SIGN_ARGS=(--force --timestamp --options runtime --sign "$SIGN_ID")
fi
for lib in "$APP"/Contents/Frameworks/*.dylib; do codesign "${SIGN_ARGS[@]}" "$lib" >/dev/null; done
codesign "${SIGN_ARGS[@]}" "$APP/Contents/Frameworks/Syphon.framework" >/dev/null
[[ -f "$APP/Contents/MacOS/ffmpeg" ]] && codesign "${SIGN_ARGS[@]}" "$APP/Contents/MacOS/ffmpeg" >/dev/null
codesign "${SIGN_ARGS[@]}" --entitlements scripts/Phosphor.entitlements "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
echo "built $APP ($(du -sh "$APP" | cut -f1))"

if [[ "${1:-}" != "--no-install" ]]; then
  rm -rf /Applications/Phosphor.app
  cp -R "$APP" /Applications/
  touch /Applications/Phosphor.app
  # Tell Finder/Dock about the new copy so its icon shows straight away.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Phosphor.app || true
  echo "installed /Applications/Phosphor.app"
fi
