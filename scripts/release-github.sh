#!/bin/sh
# Publishes a GitHub release the app's updater can find: builds the bundle, zips it, creates the release.
# Usage: scripts/release-github.sh [notes.md]   (version = crates/phosphor/Cargo.toml; needs `gh auth login`)
set -e
cd "$(dirname "$0")/.."
VER=$(grep -m1 '^version' crates/phosphor/Cargo.toml | sed 's/.*"\(.*\)"/\1/')
TAG="v$VER"
if gh release view "$TAG" -R fillinthe/phosphor >/dev/null 2>&1; then
  echo "Release $TAG already exists. Bump the version in crates/phosphor/Cargo.toml first."; exit 1
fi
# The downloads must be exactly the tagged source: no uncommitted code (the Mac app is built from this folder).
if ! git diff --quiet HEAD -- crates scripts Cargo.toml Cargo.lock assets || [ -n "$(git ls-files --others --exclude-standard crates scripts)" ]; then
  echo "Uncommitted code changes: commit them (and let the Windows build run) before releasing."; exit 1
fi
scripts/release-notes.sh "$VER" >/dev/null || { echo "Add a '## $VER (date)' section to CHANGELOG.md first."; exit 1; }
# Don't swap /Applications/Phosphor.app under a Phosphor that's running from it.
if pgrep -f "/Applications/Phosphor.app/Contents/MacOS" >/dev/null 2>&1; then
  scripts/bundle-mac.sh --no-install
else
  scripts/bundle-mac.sh
fi
rm -f dist/Phosphor-mac.zip
(cd dist && ditto -c -k --keepParent Phosphor.app Phosphor-mac.zip)
# Only /Applications/Phosphor.app should be openable: the build copy goes (Spotlight / Launchpad could open it).
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -u "$PWD/dist/Phosphor.app" 2>/dev/null || true
rm -rf dist/Phosphor.app
NOTES=${1:-}
if [ -z "$NOTES" ]; then
  # This version's CHANGELOG.md section plus install steps (add the section before releasing).
  NOTES=$(mktemp)
  scripts/release-notes.sh "$VER" > "$NOTES"
fi
git tag -f "$TAG"
git push -f origin "$TAG"
ASSETS="dist/Phosphor-mac.zip"
# The Windows build of the newest code commit (docs-only commits don't build; see paths-ignore in windows.yml).
CODE_SHA=$(git log -1 --format=%H -- . ':(exclude)docs' ':(exclude)*.md')
WIN_RUN=$(bash scripts/windows-run.sh --success || true)
if [ -z "$WIN_RUN" ] && [ "${ALLOW_NO_WINDOWS:-}" != 1 ]; then
  ANY=$(bash scripts/windows-run.sh || true)
  if [ -n "$ANY" ]; then
    echo "The Windows build of this code (run $ANY) hasn't finished successfully yet: wait for it, then release again (ALLOW_NO_WINDOWS=1 to publish Mac only)." >&2
  else
    echo "No Windows build for this code: push and let CI run, then release again (ALLOW_NO_WINDOWS=1 to publish Mac only)." >&2
  fi
  exit 1
fi
if [ -n "$WIN_RUN" ]; then
  rm -rf dist/win && mkdir -p dist/win
  if gh run download "$WIN_RUN" -R fillinthe/phosphor -n Phosphor-windows -D dist/win; then
    ASSETS="$ASSETS dist/win/Phosphor-windows.zip"
    # The installer, when this run made one (runs from before it existed only have the zip).
    if gh run download "$WIN_RUN" -R fillinthe/phosphor -n Phosphor-windows-setup -D dist/win 2>/dev/null; then
      ASSETS="$ASSETS dist/win/Phosphor-windows-setup.exe"
    fi
    echo "Adding the Windows build from run $WIN_RUN"
  fi
fi
# The Linux ARM (Raspberry Pi) package, when its build of this code succeeded (optional: never holds a release up).
LINUX_RUN=$(bash scripts/ci-run.sh linux.yml --success || true)
rm -rf dist/linux
if [ -n "$LINUX_RUN" ] && gh run download "$LINUX_RUN" -R fillinthe/phosphor -n Phosphor-linux-arm64 -D dist/linux 2>/dev/null; then
  ASSETS="$ASSETS dist/linux/Phosphor-linux-arm64.tar.gz"
  echo "Adding the Linux ARM build from run $LINUX_RUN"
fi
# The build for older Intel Macs (macOS 10.15+), when its build of this code succeeded (optional, like Linux).
INTEL_RUN=$(bash scripts/ci-run.sh mac-intel.yml --success || true)
rm -rf dist/intel
if [ -n "$INTEL_RUN" ] && gh run download "$INTEL_RUN" -R fillinthe/phosphor -n Phosphor-mac-intel -D dist/intel 2>/dev/null; then
  ASSETS="$ASSETS dist/intel/Phosphor-mac-intel.zip"
  echo "Adding the Intel Mac build from run $INTEL_RUN"
fi
gh release create "$TAG" $ASSETS -R fillinthe/phosphor --title "Phosphor $VER" --notes-file "$NOTES"
echo "Published $TAG. Running apps install it within the hour, and when they next open."
