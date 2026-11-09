#!/bin/bash
# Checks the release downloads the way people's computers unpack them, before (or after) they're published.
# Usage: scripts/check-downloads.sh DIR      DIR holds the assets (any of them; missing ones are skipped):
#   Phosphor-mac.zip  Phosphor-mac.dmg  Phosphor-mac-intel.zip  Phosphor-windows.zip  Phosphor-windows-setup.exe
#   Phosphor-linux-arm64.tar.gz
# or:    scripts/check-downloads.sh --release v0.5.0     downloads that release's assets first.
# Exits 1 on any FAIL. Nothing from the downloads is run except the Apple Silicon app's --version (CHECK_RUN=0 skips).
#
# Why (bug report 2026-11-09): a zip that unpacked fine with Finder left "._" files in the app with other unzip tools,
# which broke its signature, and macOS refused to open it. Every Mac zip is now unpacked with each tool there is and
# its signature and Gatekeeper verdict checked after each.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ "${1:-}" == "--release" ]]; then
  DIR=$(mktemp -d)
  gh release download "$2" -R fillinthe/phosphor -D "$DIR" || exit 1
else
  DIR="$(cd "${1:?usage: check-downloads.sh DIR | --release TAG}" && pwd)"
fi
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
fails=0
ok() { echo "  ok    $*"; }
bad() { echo "  FAIL  $*"; fails=$((fails + 1)); }
note() { echo "  note  $*"; }

# A browser download carries the quarantine mark; Gatekeeper only looks hard at marked files.
quarantine() { xattr -w com.apple.quarantine "0081;$(printf '%x' "$(date +%s)");Safari;" "$1" 2>/dev/null || true; }

# Libraries a Mac binary loads from outside the bundle and the system (a Homebrew path = works on this Mac only).
outside_libs() { otool -L "$1" 2>/dev/null | grep -v ':$' | awk '{print $1}' | grep -vE '^(@rpath|@executable_path|@loader_path|/usr/lib/|/System/Library/)' || true; }

check_app() { # app path, label, wants notarization (1/0), wanted arch
  local app="$1" label="$2" notarized="$3" arch="$4"
  if find "$app" -name '._*' | grep -q .; then bad "$label: ._ files inside the app ($(find "$app" -name '._*' | wc -l | tr -d ' '))"; else ok "$label: no ._ files"; fi
  if codesign --verify --deep --strict "$app" 2>"$WORK/cs.txt"; then ok "$label: signature valid (deep, strict)"; else bad "$label: signature: $(tr '\n' ' ' < "$WORK/cs.txt")"; fi
  if [[ "$notarized" == 1 ]]; then
    if spctl -a -t exec -vv "$app" >"$WORK/sp.txt" 2>&1 && grep -q 'Notarized Developer ID' "$WORK/sp.txt"; then ok "$label: Gatekeeper accepts (Notarized Developer ID)"; else bad "$label: Gatekeeper: $(tr '\n' ' ' < "$WORK/sp.txt")"; fi
    if xcrun stapler validate "$app" >/dev/null 2>&1; then ok "$label: notarization ticket stapled (opens offline)"; else bad "$label: no stapled ticket"; fi
  fi
  local exe="$app/Contents/MacOS/Phosphor"
  [[ -x "$exe" ]] || { bad "$label: no Contents/MacOS/Phosphor"; return; }
  if lipo -archs "$exe" 2>/dev/null | grep -qw "$arch"; then ok "$label: built for $arch"; else bad "$label: built for $(lipo -archs "$exe" 2>&1), wanted $arch"; fi
  local stray=""
  while IFS= read -r f; do
    file -b "$f" | grep -q 'Mach-O' || continue
    local o; o=$(outside_libs "$f")
    [[ -n "$o" ]] && stray="$stray $(basename "$f"):$o"
    # Every @rpath library it names is in the bundle.
    for lib in $(otool -L "$f" 2>/dev/null | grep -v ':$' | awk '{print $1}' | grep '^@rpath/' | sed 's|@rpath/||'); do
      [[ -e "$app/Contents/Frameworks/$lib" ]] || stray="$stray $(basename "$f"):missing:$lib"
    done
  done < <(find "$app/Contents/MacOS" "$app/Contents/Frameworks" -type f 2>/dev/null)
  if [[ -z "$stray" ]]; then ok "$label: every library it loads is in the app or macOS"; else bad "$label: libraries from outside:$stray"; fi
  local min; min=$(vtool -show-build "$exe" 2>/dev/null | awk '/minos/{print $2; exit}')
  [[ -n "$min" ]] && note "$label: needs macOS $min or newer"
}

check_mac_zip() { # zip, label, notarized, arch
  local zip="$DIR/$1" label="$2"
  [[ -f "$zip" ]] || { note "$1: not here, skipped"; return; }
  echo "$1"
  if unzip -l "$zip" | grep -qE '/\._|__MACOSX'; then bad "$label: the zip holds ._ / __MACOSX entries"; else ok "$label: no ._ entries in the zip"; fi
  local tools=("unzip:unzip -q ZIP -d OUT" "ditto (Finder's way):ditto -x -k ZIP OUT" "bsdtar:bsdtar -xf ZIP -C OUT")
  command -v 7z >/dev/null && tools+=("7-Zip:7z x -y -oOUT ZIP")
  command -v python3 >/dev/null && tools+=("Python zipfile (no permissions kept):python3 -I -c \"import zipfile,sys;zipfile.ZipFile(sys.argv[1]).extractall(sys.argv[2])\" ZIP OUT")
  local n=0
  for t in "${tools[@]}"; do
    local name="${t%%:*}" cmd="${t#*:}" out="$WORK/x$((n++))-$RANDOM"
    mkdir -p "$out"
    cp "$zip" "$WORK/dl.zip"; quarantine "$WORK/dl.zip"
    cmd="${cmd//ZIP/$WORK/dl.zip}"; cmd="${cmd//OUT/$out}"
    if ! eval "$cmd" >/dev/null 2>&1; then bad "$label, $name: couldn't unpack"; continue; fi
    local app="$out/Phosphor.app"
    [[ -d "$app" ]] || { bad "$label, $name: no Phosphor.app at the top of the zip"; continue; }
    if [[ "$name" == Python* ]]; then
      # Tools that drop permissions and symlinks can't give a working Mac app from any zip; only say so.
      [[ -x "$app/Contents/MacOS/Phosphor" ]] || { note "$label, $name: loses the run permission (true of every Mac app zip)"; continue; }
    fi
    quarantine "$app"
    check_app "$app" "$label, $name" "$3" "$4"
    rm -rf "$out"
  done
}

check_dmg() {
  local dmg="$DIR/Phosphor-mac.dmg"
  [[ -f "$dmg" ]] || { note "Phosphor-mac.dmg: not here, skipped"; return; }
  echo "Phosphor-mac.dmg"
  if hdiutil verify -quiet "$dmg" 2>/dev/null; then ok "disk image: checksum good"; else bad "disk image: hdiutil verify failed"; fi
  if spctl -a -t open --context context:primary-signature -vv "$dmg" >"$WORK/sp.txt" 2>&1 && grep -q 'Notarized Developer ID' "$WORK/sp.txt"; then ok "disk image: Gatekeeper accepts (Notarized Developer ID)"; else bad "disk image: Gatekeeper: $(tr '\n' ' ' < "$WORK/sp.txt")"; fi
  if xcrun stapler validate "$dmg" >/dev/null 2>&1; then ok "disk image: ticket stapled"; else bad "disk image: no stapled ticket"; fi
  local mnt="$WORK/mnt"; mkdir -p "$mnt"
  if hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mnt" "$dmg" 2>/dev/null; then
    [[ -L "$mnt/Applications" ]] && ok "disk image: has the Applications shortcut" || bad "disk image: no Applications shortcut"
    if [[ -d "$mnt/Phosphor.app" ]]; then
      # What dragging it to Applications does: a plain copy.
      cp -R "$mnt/Phosphor.app" "$WORK/dmgapp.app" && quarantine "$WORK/dmgapp.app"
      check_app "$WORK/dmgapp.app" "disk image, dragged copy" 1 arm64
      DMG_APP="$WORK/dmgapp.app"
    else bad "disk image: no Phosphor.app inside"; fi
    hdiutil detach -quiet "$mnt" 2>/dev/null || hdiutil detach -force -quiet "$mnt" 2>/dev/null
  else bad "disk image: couldn't mount"; fi
}

# DLLs a Windows program names that Windows itself doesn't have must be in the folder beside it.
WINDOWS_OWN='^(kernel32|user32|gdi32|advapi32|shell32|ole32|oleaut32|ws2_32|bcrypt|bcryptprimitives|ntdll|msvcrt|comdlg32|comctl32|shlwapi|version|winmm|imm32|setupapi|cfgmgr32|dwmapi|uxtheme|opengl32|dxgi|d3d11|d3d12|d3dcompiler_47|userenv|crypt32|secur32|iphlpapi|psapi|powrprof|propsys|avrt|mmdevapi|dbghelp|netapi32|wtsapi32|winspool|hid|dinput8|xinput.*|mfplat|mf|mfreadwrite|mfuuid|evr|avicap32|msvfw32|dnsapi|ncrypt|rpcrt4|shcore|wldap32|normaliz|dwrite|d2d1|d3d9|dxva2|winhttp|wininet|urlmon|pdh|kernelbase|combase|oleacc|uiautomationcore|usp10|msimg32|ntoskrnl|api-ms-win-.*|ext-ms-.*)\.dll$'
check_windows() {
  local zip="$DIR/Phosphor-windows.zip"
  [[ -f "$zip" ]] || { note "Phosphor-windows.zip: not here, skipped"; return; }
  echo "Phosphor-windows.zip"
  local out="$WORK/win"; mkdir -p "$out"
  unzip -q "$zip" -d "$out" || { bad "Windows zip: couldn't unpack"; return; }
  local exe; exe=$(find "$out" -iname 'phosphor.exe' | head -1)
  [[ -n "$exe" ]] || { bad "Windows zip: no Phosphor.exe"; return; }
  ok "Windows zip: Phosphor.exe is there ($(find "$out" -iname '*.dll' | wc -l | tr -d ' ') DLLs beside it)"
  file -b "$exe" | grep -q 'x86-64' && ok "Windows zip: 64-bit Intel / AMD program" || bad "Windows zip: $(file -b "$exe")"
  local folder; folder=$(dirname "$exe")
  local have; have=$(find "$folder" -maxdepth 1 -iname '*.dll' -exec basename {} \; | tr 'A-Z' 'a-z')
  local missing="" runtime=""
  for f in "$exe" "$folder"/*.dll; do
    [[ -f "$f" ]] || continue
    for dll in $(objdump -p "$f" 2>/dev/null | awk '/DLL Name:/{print tolower($3)}' | sort -u); do
      echo "$dll" | grep -qE "$WINDOWS_OWN" && continue
      echo "$have" | grep -qx "$dll" && continue
      if echo "$dll" | grep -qE '^(vcruntime|msvcp|vcomp|concrt|ucrtbase)'; then runtime="$runtime $dll"; else missing="$missing $(basename "$f")>$dll"; fi
    done
  done
  [[ -z "$missing" ]] && ok "Windows zip: every DLL it names is beside it or part of Windows" || bad "Windows zip: DLLs not in the folder:$(echo "$missing" | tr ' ' '\n' | sort -u | tr '\n' ' ')"
  [[ -z "$runtime" ]] && ok "Windows zip: needs no Visual C++ runtime install" || bad "Windows zip: needs the Visual C++ runtime, which a fresh Windows lacks:$(echo "$runtime" | tr ' ' '\n' | sort -u | tr '\n' ' ')"
  local setup="$DIR/Phosphor-windows-setup.exe"
  if [[ -f "$setup" ]]; then
    echo "Phosphor-windows-setup.exe"
    file -b "$setup" | grep -q 'PE32' && ok "Windows installer: a Windows program ($(du -h "$setup" | cut -f1 | tr -d ' '))" || bad "Windows installer: $(file -b "$setup")"
    if command -v 7z >/dev/null; then
      7z l "$setup" 2>/dev/null | grep -qi 'phosphor.exe' && ok "Windows installer: Phosphor.exe is packed in it"
    fi
    if [[ "$(objdump -p "$setup" 2>/dev/null | awk '/Security Directory/{print $4; exit}')" =~ ^0*$ ]]; then note "Windows installer: not code-signed (Windows shows 'unknown publisher' / SmartScreen the first time)"; else ok "Windows installer: code-signed"; fi
  fi
}

check_linux() {
  local tar="$DIR/Phosphor-linux-arm64.tar.gz"
  [[ -f "$tar" ]] || { note "Phosphor-linux-arm64.tar.gz: not here, skipped"; return; }
  echo "Phosphor-linux-arm64.tar.gz"
  local out="$WORK/linux"; mkdir -p "$out"
  tar -xzf "$tar" -C "$out" 2>/dev/null || { bad "Linux package: couldn't unpack"; return; }
  local exe; exe=$(find "$out" -type f -name 'phosphor*' ! -name '*.*' | head -1)
  [[ -n "$exe" ]] || exe=$(find "$out" -type f -perm +111 | head -1)
  [[ -n "$exe" ]] || { bad "Linux package: no program inside"; return; }
  file -b "$exe" | grep -q 'ARM aarch64' && ok "Linux package: 64-bit ARM program ($(basename "$exe"))" || bad "Linux package: $(file -b "$exe")"
  [[ -x "$exe" ]] && ok "Linux package: the program keeps its run permission" || bad "Linux package: the program isn't marked runnable"
  local needed; needed=$(objdump -p "$exe" 2>/dev/null | awk '/NEEDED/{print $2}')
  local packed=""; for lib in $needed; do find "$out" -name "$lib*" | grep -q . && packed="$packed $lib"; done
  local outside; outside=$(for lib in $needed; do find "$out" -name "$lib*" | grep -q . || echo "$lib"; done | grep -vE '^(libc|libm|libdl|libpthread|librt|libgcc_s|libstdc\+\+|ld-linux-aarch64|libutil)\.so' | tr '\n' ' ')
  note "Linux package: libraries the system must have: ${outside:-none}"
  if find "$out" -maxdepth 2 -iname 'README*' -o -maxdepth 2 -iname 'install*' | grep -q .; then ok "Linux package: has install notes"; else note "Linux package: no README / install notes inside"; fi
}

DMG_APP=""
check_mac_zip Phosphor-mac.zip "Mac zip" 1 arm64
check_dmg
# The Intel build is notarized at release time (scripts/notarize-app-zip.sh); INTEL_NOTARIZED=0 for a raw CI zip.
check_mac_zip Phosphor-mac-intel.zip "Intel Mac zip" "${INTEL_NOTARIZED:-1}" x86_64
check_windows
check_linux

# The Apple Silicon app starts from where a download lands (marked as downloaded, outside /Applications).
if [[ "${CHECK_RUN:-1}" == 1 && -n "$DMG_APP" && "$(uname -m)" == arm64 ]]; then
  echo "Starting it"
  if out=$(HOME="$WORK/home" "$DMG_APP/Contents/MacOS/Phosphor" --version 2>&1); then ok "the app starts: $out"; else bad "the app didn't start: $(echo "$out" | head -3 | tr '\n' ' ')"; fi
fi

echo
if [[ $fails == 0 ]]; then echo "DOWNLOADS ok"; else echo "DOWNLOADS: $fails problem(s)"; exit 1; fi
