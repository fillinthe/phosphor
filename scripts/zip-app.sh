#!/usr/bin/env bash
# Zips an app bundle for download: zip-app.sh <dir> <App.app> <out.zip> (out is written inside <dir>).
# No extended attributes, resource forks or ACLs go in: ditto would otherwise store them as "._name" files, and an
# unzip tool other than Finder's writes those into the bundle, which breaks its seal ("unsealed contents present in the
# root directory of an embedded framework"; bug report 2026-11-09). The signature lives in files, so nothing is lost.
set -euo pipefail
dir="$1"; app="$2"; out="$3"
rm -f "$dir/$out"
(cd "$dir" && ditto -c -k --norsrc --noextattr --noqtn --noacl --keepParent "$app" "$out")
if unzip -l "$dir/$out" | grep -qE '(/|^ *[0-9].* )\._|__MACOSX'; then
  echo "zip-app: $out still holds ._ files" >&2; exit 1
fi
