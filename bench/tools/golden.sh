#!/usr/bin/env bash
set -euo pipefail
mode=check
if [[ ${1:-} == --check || ${1:-} == --write ]]; then
    mode=${1#--}
    shift
fi
exec python3 "$(dirname "$0")/perf.py" --golden "$mode" "$@"
