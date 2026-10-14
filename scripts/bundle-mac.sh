#!/bin/bash
# Builds Phosphor.app (release) with its icon, ad-hoc signs it, and installs it to /Applications.
#   scripts/bundle-mac.sh            build + install
#   scripts/bundle-mac.sh --no-install
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

codesign --force --deep --sign - "$APP" >/dev/null
echo "built $APP ($(du -sh "$APP" | cut -f1))"

if [[ "${1:-}" != "--no-install" ]]; then
  rm -rf /Applications/Phosphor.app
  cp -R "$APP" /Applications/
  touch /Applications/Phosphor.app
  echo "installed /Applications/Phosphor.app"
fi
