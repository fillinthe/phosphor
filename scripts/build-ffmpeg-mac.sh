#!/bin/bash
# Builds the FFmpeg that ships inside Phosphor.app: FFmpeg 9.0.2 (the version the Rust bindings use) + dav1d,
# shared libraries and the ffmpeg command, for macOS 14 and later (Homebrew's are built for the macOS they were
# compiled on, so a bundle made from them won't open on older Macs).
# Only what Phosphor uses: FFmpeg's own decoders, VideoToolbox / AudioToolbox, AVFoundation capture, dav1d (AV1),
# libvpx decoders (WebM with alpha), OpenSSL (RTMPS, and WHIP / WebRTC). Output: vendor/ffmpeg (ignored by git); scripts/bundle-mac.sh uses it when present.
#   scripts/build-ffmpeg-mac.sh        (about 5 minutes)
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$PWD
# The oldest macOS Phosphor runs on, for every bundled binary (one value: scripts/min-macos; MIN_MACOS overrides it,
# e.g. the build for older Intel Macs). Built for this Mac's processor (arm64, or x86_64 on an Intel Mac).
MIN_MACOS=${MIN_MACOS:-$(cat scripts/min-macos)}
ARCH=$(uname -m)
if [[ $ARCH == x86_64 ]]; then VPX_TARGET=x86_64-darwin19-gcc; SSL_TARGET=darwin64-x86_64-cc; else VPX_TARGET=arm64-darwin23-gcc; SSL_TARGET=darwin64-arm64-cc; fi
FFMPEG_VER=9.0.2
DAV1D_VER=1.5.1
VPX_VER=1.15.2
OPENSSL_VER=3.5.4
PREFIX=$ROOT/vendor/ffmpeg
WORK=$ROOT/target/ffmpeg-build
export MACOSX_DEPLOYMENT_TARGET=$MIN_MACOS
JOBS=$(sysctl -n hw.ncpu)
mkdir -p "$WORK"
rm -rf "$PREFIX"

# meson (dav1d's build tool) in a throwaway venv, not on the system.
if [[ ! -x "$WORK/venv/bin/meson" ]]; then
  python3 -m venv "$WORK/venv"
  "$WORK/venv/bin/pip" install -q meson
fi

cd "$WORK"
[[ -f dav1d-$DAV1D_VER.tar.xz ]] || curl -sSLO "https://downloads.videolan.org/pub/videolan/dav1d/$DAV1D_VER/dav1d-$DAV1D_VER.tar.xz"
rm -rf dav1d-$DAV1D_VER && tar xf dav1d-$DAV1D_VER.tar.xz
(
  cd dav1d-$DAV1D_VER
  "$WORK/venv/bin/meson" setup build --prefix="$PREFIX" --libdir=lib --buildtype=release --default-library=shared \
    -Denable_tools=false -Denable_tests=false -Dc_args="-mmacosx-version-min=$MIN_MACOS" -Dc_link_args="-mmacosx-version-min=$MIN_MACOS" >/dev/null
  ninja -C build install >/dev/null
)
# Load by @rpath so the bundle can carry it.
install_name_tool -id "@rpath/libdav1d.7.dylib" "$PREFIX/lib/libdav1d.7.dylib"

# libvpx, decoders only: VP8 / VP9 WebM with transparency (FFmpeg's own VP8/VP9 decoders drop the alpha).
[[ -f libvpx-$VPX_VER.tar.gz ]] || curl -sSL -o libvpx-$VPX_VER.tar.gz "https://github.com/webmproject/libvpx/archive/refs/tags/v$VPX_VER.tar.gz"
rm -rf libvpx-$VPX_VER && tar xf libvpx-$VPX_VER.tar.gz
(
  cd libvpx-$VPX_VER
  ./configure --prefix="$PREFIX" --target=$VPX_TARGET $([[ $ARCH == x86_64 ]] && echo --as=nasm) --enable-shared --disable-static --disable-examples --disable-tools \
    --disable-docs --disable-unit-tests --disable-vp8-encoder --disable-vp9-encoder --enable-vp9-highbitdepth \
    --extra-cflags="-mmacosx-version-min=$MIN_MACOS" >/dev/null
  make -j"$JOBS" >/dev/null
  make install >/dev/null
)
vpx=$(basename "$(readlink -f "$PREFIX/lib/libvpx.dylib")")
install_name_tool -id "@rpath/$vpx" "$PREFIX/lib/$vpx"

# OpenSSL, static (it ends up inside libavformat, nothing more to bundle): TLS for RTMPS and DTLS for WHIP
# (WebRTC), which macOS's own TLS can't do.
SSL=$WORK/openssl
[[ -f openssl-$OPENSSL_VER.tar.gz ]] || curl -sSLO "https://github.com/openssl/openssl/releases/download/openssl-$OPENSSL_VER/openssl-$OPENSSL_VER.tar.gz"
if [[ ! -f "$SSL/lib/libssl.a" ]]; then
  rm -rf openssl-$OPENSSL_VER && tar xf openssl-$OPENSSL_VER.tar.gz
  (
    cd openssl-$OPENSSL_VER
    ./Configure $SSL_TARGET no-shared no-tests no-docs no-apps --prefix="$SSL" --libdir=lib -mmacosx-version-min=$MIN_MACOS >/dev/null
    make -j"$JOBS" >/dev/null
    make install_sw >/dev/null
  )
fi

[[ -f ffmpeg-$FFMPEG_VER.tar.xz ]] || curl -sSLO "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VER.tar.xz"
rm -rf ffmpeg-$FFMPEG_VER && tar xf ffmpeg-$FFMPEG_VER.tar.xz
(
  cd ffmpeg-$FFMPEG_VER
  PKG_CONFIG_PATH="$PREFIX/lib/pkgconfig:$SSL/lib/pkgconfig" ./configure --prefix="$PREFIX" \
    --enable-shared --disable-static --enable-gpl --enable-version3 \
    --disable-doc --disable-ffplay --disable-ffprobe --disable-debug \
    --disable-autodetect --enable-videotoolbox --enable-audiotoolbox --enable-openssl --enable-avfoundation \
    --enable-zlib --enable-bzlib --enable-libdav1d --enable-libvpx \
    --extra-cflags="-mmacosx-version-min=$MIN_MACOS" --extra-ldflags="-mmacosx-version-min=$MIN_MACOS -Wl,-rpath,$PREFIX/lib" \
    --install-name-dir='@rpath' >/dev/null
  make -j"$JOBS" >/dev/null
  make install >/dev/null
)

# Check: only system libraries and our own (no Homebrew), and nothing needing a newer macOS than $MIN_MACOS.
if otool -L "$PREFIX"/lib/*.dylib "$PREFIX/bin/ffmpeg" | grep -E "^\s+/(opt|usr/local)/"; then echo "links outside the bundle"; exit 1; fi
bad=0
for f in "$PREFIX"/lib/*.dylib "$PREFIX/bin/ffmpeg"; do
  [[ -L "$f" ]] && continue
  m=$(otool -l "$f" | awk '/LC_BUILD_VERSION/{b=1} b && /minos/{print $2; exit}')
  if [[ "$(printf '%s\n%s\n' "$m" "$MIN_MACOS" | sort -V | tail -1)" != "$MIN_MACOS" ]]; then echo "too new: $f ($m)"; bad=1; fi
done
[[ $bad == 0 ]] || exit 1
echo "built $PREFIX (macOS $MIN_MACOS+): $("$PREFIX/bin/ffmpeg" -hide_banner -version | head -1)"
