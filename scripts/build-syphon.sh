#!/bin/bash
# Builds Syphon.framework (BSD, github.com/Syphon/Syphon-Framework) into vendor/ for Syphon output.
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=$(mktemp -d)/Syphon-Framework
# A download can fail for a moment (it did on CI, exit 128 with nothing said): try three times, then say why.
for try in 1 2 3; do
  rm -rf "$SRC"
  if git clone -q --depth 1 https://github.com/Syphon/Syphon-Framework.git "$SRC"; then break; fi
  [[ $try == 3 ]] && { echo "couldn't download Syphon-Framework from GitHub"; exit 1; }
  sleep 10
done
xcodebuild -project "$SRC/Syphon.xcodeproj" -scheme Syphon -configuration Release \
  ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO MACOSX_DEPLOYMENT_TARGET="${MIN_MACOS:-$(cat scripts/min-macos)}" \
  SYMROOT="$SRC/build" >/dev/null
mkdir -p vendor
rm -rf vendor/Syphon.framework
cp -R "$SRC/build/Release/Syphon.framework" vendor/
cp "$SRC/License.txt" vendor/Syphon-LICENSE.txt
echo "built vendor/Syphon.framework"
