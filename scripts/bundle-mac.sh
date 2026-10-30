#!/bin/bash
# Builds Phosphor.app (release) with its icon, signs it, and installs it to /Applications.
#   scripts/bundle-mac.sh            build + install (ad-hoc signed: runs on this Mac)
#   scripts/bundle-mac.sh --no-install
#   SIGN_ID="Developer ID Application: …" scripts/bundle-mac.sh --no-install
#       signs for other Macs (hardened runtime + entitlements); then scripts/notarize-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."
VERSION=$(grep -m1 '^version' crates/phosphor/Cargo.toml | cut -d'"' -f2)

# Oldest macOS the app runs on. Everything bundled must be built for it (checked at the end).
# The oldest macOS Phosphor runs on (one value for FFmpeg, Syphon and the app: scripts/min-macos). 13 is the floor:
# the ScreenCaptureKit bridge uses macOS 13 calls.
# MIN_MACOS overrides it: the build for older Intel Macs (CI, .github/workflows/mac-intel.yml) uses 10.15 with
# CARGO_FEATURES=--no-default-features (no screen capture, which needs macOS 12.3).
export MIN_MACOS=${MIN_MACOS:-$(cat scripts/min-macos)}
# FFmpeg built for MIN_MACOS (scripts/build-ffmpeg-mac.sh); Homebrew's only runs on the macOS it was built on.
# The bundle build gets its own target folder so day-to-day builds keep using Homebrew's FFmpeg.
VENDOR_FFMPEG=$PWD/vendor/ffmpeg
[[ -x "$VENDOR_FFMPEG/bin/ffmpeg" ]] || scripts/build-ffmpeg-mac.sh
export FFMPEG_DIR=$VENDOR_FFMPEG PKG_CONFIG_PATH=$VENDOR_FFMPEG/lib/pkgconfig MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS
CARGO_TARGET_DIR=target/bundle cargo build --release -p phosphor ${CARGO_FEATURES:-}

APP=dist/Phosphor.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/bundle/release/phosphor "$APP/Contents/MacOS/Phosphor"

# Syphon output (BSD). Built once by scripts/build-syphon.sh; the app runs without it.
[[ -d vendor/Syphon.framework ]] || scripts/build-syphon.sh
mkdir -p "$APP/Contents/Frameworks"
cp -R vendor/Syphon.framework "$APP/Contents/Frameworks/"
# The virtual camera (scripts/build-camera-extension.sh), when it's been built. Inert until signed (see that script).
if [[ -d vendor/PhosphorCamera.systemextension && -z "${CARGO_FEATURES:-}" ]]; then
  mkdir -p "$APP/Contents/Library/SystemExtensions"
  cp -R vendor/PhosphorCamera.systemextension "$APP/Contents/Library/SystemExtensions/"
fi
cp vendor/Syphon-LICENSE.txt "$APP/Contents/Resources/"
# The free ISF collection (MIT), so every Mac has those effects without VDMX.
cp -R assets/isf "$APP/Contents/Resources/isf"
cp -R assets/gl-transitions "$APP/Contents/Resources/gl-transitions"
cp -R assets/luts "$APP/Contents/Resources/luts"

# The ffmpeg command (streaming, ProRes / H.264 recording) rides along; its libraries are relinked below.
cp "$VENDOR_FFMPEG/bin/ffmpeg" "$APP/Contents/MacOS/ffmpeg"
chmod 755 "$APP/Contents/MacOS/ffmpeg"
# The build-time rpath into vendor/ is no use on other Macs.
install_name_tool -delete_rpath "$VENDOR_FFMPEG/lib" "$APP/Contents/MacOS/ffmpeg" 2>/dev/null || true

# FFmpeg and its libraries, so the app (and exported shows) run on Macs without Homebrew.
python3 -I scripts/bundle-dylibs.py "$APP" "$VENDOR_FFMPEG/lib"

# Icon: every size macOS asks for, from the 1024 px master.
ICONSET=$(mktemp -d)/Phosphor.iconset
mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s*2)) $((s*2)) assets/icon-1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Phosphor.icns"

# A test build (made from a branch other than main) never updates by itself: a public release would replace work
# that isn't released yet. Release builds come from main (or a tag in CI: "HEAD").
BRANCH=$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo main)
TEST_BUILD=""
[[ "$BRANCH" != "main" && "$BRANCH" != "HEAD" ]] && TEST_BUILD="  <key>PhosphorTestBuild</key><string>${BRANCH}</string>"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Phosphor VJ</string>
  <key>CFBundleDisplayName</key><string>Phosphor VJ</string>
  <key>CFBundleIdentifier</key><string>co.fillintheblank.phosphor</string>
  <key>CFBundleExecutable</key><string>Phosphor</string>
  <key>CFBundleIconFile</key><string>Phosphor</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleVersion</key><string>$(git rev-list --count HEAD 2>/dev/null || echo 1)</string>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleLocalizations</key><array><string>en</string><string>es</string><string>de</string><string>fr</string><string>it</string><string>pt-BR</string><string>ja</string><string>zh-Hans</string></array>
  <key>LSMinimumSystemVersion</key><string>${MIN_MACOS}</string>
${TEST_BUILD}
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

# Nothing in the bundle may need a newer macOS than the app says it runs on (a library that does stops the
# app opening there at all).
too_new=0
while IFS= read -r f; do
  m=$(otool -l "$f" 2>/dev/null | awk '/LC_BUILD_VERSION/{b=1} b && /minos/{print $2; exit}')
  [[ -z "$m" ]] && continue
  if [[ "$(printf '%s\n%s\n' "$m" "$MIN_MACOS" | sort -V | tail -1)" != "$MIN_MACOS" ]]; then echo "needs macOS $m: $f"; too_new=1; fi
done < <(find "$APP/Contents" -type f \( -name "*.dylib" -o -perm -u+x \) ! -name "*.icns" ! -name "*.plist")
[[ $too_new == 0 ]] || { echo "Some bundled files need a newer macOS than $MIN_MACOS"; exit 1; }

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
# The camera extension, before the app around it (its own entitlements; a real one also needs its provisioning profile,
# see scripts/build-camera-extension.sh).
CAM="$APP/Contents/Library/SystemExtensions/PhosphorCamera.systemextension"
[[ -d "$CAM" ]] && codesign "${SIGN_ARGS[@]}" --entitlements vendor/PhosphorCamera.entitlements "$CAM" >/dev/null
codesign "${SIGN_ARGS[@]}" --entitlements scripts/Phosphor.entitlements "$APP" >/dev/null
codesign --verify --deep --strict "$APP"
echo "built $APP ($(du -sh "$APP" | cut -f1))"

# Every panel must draw without panicking before this build goes anywhere.
# (SKIP_PANEL_CHECK=1: a build machine with no screen to draw on.)
[[ -n "${SKIP_PANEL_CHECK:-}" ]] || scripts/panel-check.sh "$APP/Contents/MacOS/Phosphor"

if [[ "${1:-}" != "--no-install" ]]; then
  rm -rf /Applications/Phosphor.app
  cp -R "$APP" /Applications/
  touch /Applications/Phosphor.app
  # Tell Finder/Dock about the new copy so its icon shows straight away.
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Phosphor.app || true
  echo "installed /Applications/Phosphor.app"
fi
