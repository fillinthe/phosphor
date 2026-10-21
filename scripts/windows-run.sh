#!/bin/bash
# The Windows build (windows.yml) of the current code: see ci-run.sh.  scripts/windows-run.sh [--success]
exec bash "$(dirname "$0")/ci-run.sh" windows.yml "$@"
