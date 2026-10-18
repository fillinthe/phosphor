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
# Don't swap /Applications/Phosphor.app under a Phosphor that's running from it.
if pgrep -f "/Applications/Phosphor.app/Contents/MacOS" >/dev/null 2>&1; then
  scripts/bundle-mac.sh --no-install
else
  scripts/bundle-mac.sh
fi
rm -f dist/Phosphor-mac.zip
(cd dist && ditto -c -k --keepParent Phosphor.app Phosphor-mac.zip)
NOTES=${1:-}
if [ -z "$NOTES" ]; then
  # The newest section of docs/NOTES.md.
  NOTES=$(mktemp)
  {
    echo "**Install:** unzip, drag Phosphor to Applications. First launch: macOS says it can't verify Phosphor (not notarized yet): click Done, then System Settings → Privacy & Security → scroll down → **Open Anyway**, once. Terminal alternative: \`xattr -dr com.apple.quarantine /Applications/Phosphor.app\`. Phosphor updates itself after that (Help › Check for Updates…)."
    echo
    awk '/^## /{n++} n==1' docs/NOTES.md
  } > "$NOTES"
fi
git tag -f "$TAG"
git push -f origin "$TAG"
ASSETS="dist/Phosphor-mac.zip"
# The Windows build of the newest code commit (docs-only commits don't build; see paths-ignore in windows.yml).
CODE_SHA=$(git log -1 --format=%H -- . ':(exclude)docs' ':(exclude)*.md')
WIN_RUN=$(gh run list -R fillinthe/phosphor --workflow windows.yml --commit "$CODE_SHA" --status success --limit 1 --json databaseId --jq '.[0].databaseId' 2>/dev/null || true)
if [ -n "$WIN_RUN" ]; then
  rm -rf dist/win && mkdir -p dist/win
  if gh run download "$WIN_RUN" -R fillinthe/phosphor -n Phosphor-windows -D dist/win; then
    ASSETS="$ASSETS dist/win/Phosphor-windows.zip"
    echo "Adding the Windows build from run $WIN_RUN"
  fi
fi
gh release create "$TAG" $ASSETS -R fillinthe/phosphor --title "Phosphor $VER" --notes-file "$NOTES"
echo "Published $TAG. Running apps see it within a day (Help › Check for Updates… right away)."
