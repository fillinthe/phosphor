#!/bin/bash
# Builds Syphon.framework (BSD, github.com/Syphon/Syphon-Framework) into vendor/ for Syphon output.
set -euo pipefail
cd "$(dirname "$0")/.."
SRC=$(mktemp -d)/Syphon-Framework
git clone --depth 1 https://github.com/Syphon/Syphon-Framework.git "$SRC" >/dev/null 2>&1
xcodebuild -project "$SRC/Syphon.xcodeproj" -scheme Syphon -configuration Release \
  ARCHS="arm64 x86_64" ONLY_ACTIVE_ARCH=NO CODE_SIGNING_ALLOWED=NO MACOSX_DEPLOYMENT_TARGET="$(cat scripts/min-macos)" \
  SYMROOT="$SRC/build" >/dev/null
mkdir -p vendor
rm -rf vendor/Syphon.framework
cp -R "$SRC/build/Release/Syphon.framework" vendor/
cp "$SRC/License.txt" vendor/Syphon-LICENSE.txt
echo "built vendor/Syphon.framework"
