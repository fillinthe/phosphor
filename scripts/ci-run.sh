#!/bin/bash
# Prints the id of the GitHub build (a workflow file, e.g. windows.yml or linux.yml) that built the current code: the
# newest run whose commit has the same code as HEAD's newest code commit (a later docs-only commit doesn't change
# what was built).
#   scripts/ci-run.sh <workflow.yml> [--success]   (only successful runs)
set -u
WORKFLOW=$1
CODE_SHA=$(git log -1 --format=%H -- . ':(exclude)docs' ':(exclude)*.md')
STATUS=()
[ "${2:-}" = "--success" ] && STATUS=(--status success)
gh run list -R fillinthe/phosphor --workflow "$WORKFLOW" ${STATUS[@]+"${STATUS[@]}"} --limit 30 --json databaseId,headSha --jq '.[] | "\(.databaseId) \(.headSha)"' 2>/dev/null | while read -r id sha; do
  git cat-file -e "$sha^{commit}" 2>/dev/null || continue
  if [ "$(git log -1 --format=%H "$sha" -- . ':(exclude)docs' ':(exclude)*.md')" = "$CODE_SHA" ]; then
    echo "$id"
    break
  fi
done
