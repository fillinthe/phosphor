#!/bin/bash
# Prints the id of the Windows build (windows.yml) that built the current code: the newest run whose commit has the
# same code as HEAD's newest code commit (a later docs-only commit doesn't change what was built).
#   scripts/windows-run.sh [--success]   (only successful runs)
set -u
CODE_SHA=$(git log -1 --format=%H -- . ':(exclude)docs' ':(exclude)*.md')
STATUS=()
[ "${1:-}" = "--success" ] && STATUS=(--status success)
gh run list -R fillinthe/phosphor --workflow windows.yml ${STATUS[@]+"${STATUS[@]}"} --limit 30 --json databaseId,headSha --jq '.[] | "\(.databaseId) \(.headSha)"' 2>/dev/null | while read -r id sha; do
  git cat-file -e "$sha^{commit}" 2>/dev/null || continue
  if [ "$(git log -1 --format=%H "$sha" -- . ':(exclude)docs' ':(exclude)*.md')" = "$CODE_SHA" ]; then
    echo "$id"
    break
  fi
done
