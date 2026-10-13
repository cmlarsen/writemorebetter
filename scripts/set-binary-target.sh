#!/usr/bin/env bash
#
# Rewrites the released binary-target `url:` and `checksum:` literals in Package.swift. The
# release workflow stages a version's values there before it tags, and the consumer check points
# a checkout at the zip built in this run the same way. Exactly one literal of each has to be
# present, so a manifest that lost its release fallback fails here rather than at resolution.
#
# Usage: scripts/set-binary-target.sh URL CHECKSUM [PACKAGE_SWIFT]
set -euo pipefail

if [ $# -lt 2 ] || [ $# -gt 3 ]; then
    echo "usage: $0 URL CHECKSUM [PACKAGE_SWIFT]" >&2
    exit 2
fi
url=$1
checksum=$2
manifest=${3:-Package.swift}
[ -f "$manifest" ] || { echo "error: no such manifest: $manifest" >&2; exit 1; }

tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT

awk -v url="$url" -v checksum="$checksum" '
    /^[[:space:]]*url: "/ { urls++; sub(/url: "[^"]*"/, "url: \"" url "\"") }
    /^[[:space:]]*checksum: "/ { checksums++; sub(/checksum: "[^"]*"/, "checksum: \"" checksum "\"") }
    { print }
    END {
        if (urls != 1) {
            printf("error: expected exactly one url literal in %s, found %d\n", FILENAME, urls) > "/dev/stderr"
            exit 1
        }
        if (checksums != 1) {
            printf("error: expected exactly one checksum literal in %s, found %d\n", FILENAME, checksums) > "/dev/stderr"
            exit 1
        }
    }
' "$manifest" > "$tmp"

mv "$tmp" "$manifest"
