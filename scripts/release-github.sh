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
scripts/bundle-mac.sh
rm -f dist/Phosphor-mac.zip
(cd dist && ditto -c -k --keepParent Phosphor.app Phosphor-mac.zip)
NOTES=${1:-}
if [ -z "$NOTES" ]; then
  # The newest section of docs/NOTES.md.
  NOTES=$(mktemp)
  awk '/^## /{n++} n==1' docs/NOTES.md > "$NOTES"
fi
git tag -f "$TAG"
git push -f origin "$TAG"
gh release create "$TAG" dist/Phosphor-mac.zip -R fillinthe/phosphor --title "Phosphor $VER" --notes-file "$NOTES"
echo "Published $TAG. Running apps see it within a day (Help › Check for Updates… right away)."
