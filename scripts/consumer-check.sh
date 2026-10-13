#!/usr/bin/env bash
#
# Resolves and builds the WriteMoreBetter Swift package the way an app does, against an
# already-built XCFramework zip:
#
#   1. a checkout without dist/, turned into a git repository and tagged, so the consumer
#      depends on it the way an app depends on GitHub: as a remote (file://) git dependency.
#      SwiftPM compiles a dependency's manifest from a virtual filesystem, so a git dependency
#      is the only way to catch a manifest that reads a sibling file,
#   2. this run's url and checksum staged into that checkout,
#   3. a tiny consumer package that depends on that repository, imports WriteMoreBetter,
#      loads the English data and calls check.
#
# SwiftPM accepts only `https` for a binary target URL (file:// and http are rejected at
# manifest load), so the zip is served over a throwaway local HTTPS server. That means the
# check needs macOS: the server's CA is installed into the system keychain for the run
# (`sudo`, as the release workflow's macos runner allows).
#
# Usage:
#   scripts/consumer-check.sh --zip FILE --data-dir DIR --version VERSION [--work-dir DIR] [--port PORT]
#
# --zip       WriteMoreBetterFFI.xcframework.zip to resolve as the release asset.
# --data-dir  directory holding en_tokenizer.bin and en_rules.bin.
# --version   the tag the consumer depends on, e.g. 0.7.1 (X.Y.Z).
# --work-dir  scratch directory (default: a fresh mktemp -d).
# --port      port for the local HTTPS server (default: 8899).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

zip_path=""
data_dir=""
version=""
work_dir=""
port=8899
while [ $# -gt 0 ]; do
    case "$1" in
        --zip) zip_path=$2; shift 2 ;;
        --data-dir) data_dir=$2; shift 2 ;;
        --version) version=$2; shift 2 ;;
        --work-dir) work_dir=$2; shift 2 ;;
        --port) port=$2; shift 2 ;;
        -h | --help) sed -n '2,25p' "$0"; exit 0 ;;
        *)
            echo "error: unknown argument '$1'" >&2
            exit 1
            ;;
    esac
done

if [ -z "$zip_path" ] || [ -z "$data_dir" ] || [ -z "$version" ]; then
    echo "error: --zip, --data-dir and --version are required" >&2
    exit 1
fi
if ! printf '%s' "$version" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "error: --version '$version' is not a plain X.Y.Z tag" >&2
    exit 1
fi
[ -f "$zip_path" ] || { echo "error: no such zip: $zip_path" >&2; exit 1; }
[ -d "$data_dir" ] || { echo "error: no such data directory: $data_dir" >&2; exit 1; }
for binary in en_tokenizer.bin en_rules.bin; do
    [ -f "$data_dir/$binary" ] || { echo "error: $data_dir is missing $binary" >&2; exit 1; }
done

# Absolute paths: the consumer package lives somewhere else.
zip_path=$(cd "$(dirname "$zip_path")" && pwd)/$(basename "$zip_path")
data_dir=$(cd "$data_dir" && pwd)
if [ -z "$work_dir" ]; then
    work_dir=$(mktemp -d)
else
    mkdir -p "$work_dir"
fi
work_dir=$(cd "$work_dir" && pwd)

server_pid=""
cleanup() {
    if [ -n "$server_pid" ]; then
        kill "$server_pid" 2>/dev/null || true
    fi
    # On a developer's machine undo the throwaway trust; CI runners are ephemeral and
    # do not need the extra sudo calls.
    if [ -z "${CI:-}" ] && [ -n "${ca_pem:-}" ]; then
        sudo security remove-trusted-cert "$ca_pem" >/dev/null 2>&1 || true
        sudo security delete-certificate -c "writemorebetter consumer check CA" \
            /Library/Keychains/System.keychain >/dev/null 2>&1 || true
    fi
}
trap cleanup EXIT

# --- the checksum the release publishes ---------------------------------------
# Same tool the release job uses, so the check exercises the checksum the app sees.
checksum=$(swift package compute-checksum "$zip_path")
echo "artifact: $(basename "$zip_path")"
echo "checksum: $checksum"
echo "sha256:   $(shasum -a 256 "$zip_path" | awk '{print $1}')"

# --- a checkout without dist/, served as a git dependency ---------------------
# The consumer clone has to be a git repository so the dependency looks like the real
# one; a sibling release.json would be readable on disk and hide the manifest-load bug.
pkg="$work_dir/repo/writemorebetter"
mkdir -p "$pkg"
git -C "$root" archive --format=tar HEAD | tar -x -C "$pkg"
rm -rf "$pkg/dist"
# The check only means something if Package.swift cannot take the local, path-based
# binary target instead of the released URL.
if [ -e "$pkg/dist/WriteMoreBetterFFI.xcframework" ]; then
    echo "error: dist/WriteMoreBetterFFI.xcframework exists in the copy; the released URL would not be used" >&2
    exit 1
fi

url="https://127.0.0.1:$port/WriteMoreBetterFFI.xcframework.zip"
if grep -Eq '^[[:space:]]*url: "' "$pkg/Package.swift"; then
    "$root/scripts/set-binary-target.sh" "$url" "$checksum" "$pkg/Package.swift"
else
    # A pre-0.7.1 manifest reads release.json instead. Stage it anyway so the regression
    # run fails for the real reason — a remote manifest cannot read a sibling file — not
    # because a file is missing.
    printf '{\n  "url": "%s",\n  "checksum": "%s"\n}\n' "$url" "$checksum" > "$pkg/release.json"
fi

git -C "$pkg" init -q
git -C "$pkg" add -A
git -C "$pkg" -c user.name="writemorebetter consumer check" \
    -c user.email="consumer-check@users.noreply.github.com" \
    commit -q -m "release $version"
git -C "$pkg" tag "$version"

# --- serve the zip over https ------------------------------------------------
www="$work_dir/www"
tls="$work_dir/tls"
mkdir -p "$www" "$tls"
cp "$zip_path" "$www/WriteMoreBetterFFI.xcframework.zip"

openssl req -x509 -newkey rsa:2048 -nodes -keyout "$tls/ca.key" -out "$tls/ca.pem" \
    -days 2 -subj "/CN=writemorebetter consumer check CA" >/dev/null 2>&1
openssl req -newkey rsa:2048 -nodes -keyout "$tls/server.key" -out "$tls/server.csr" \
    -subj "/CN=localhost" >/dev/null 2>&1
printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\n' > "$tls/san.cnf"
openssl x509 -req -in "$tls/server.csr" -CA "$tls/ca.pem" -CAkey "$tls/ca.key" \
    -CAcreateserial -out "$tls/server.pem" -days 2 -extfile "$tls/san.cnf" >/dev/null 2>&1
ca_pem="$tls/ca.pem"

echo "==> trusting the local CA (sudo)"
sudo security add-trusted-cert -d -r trustRoot -k /Library/Keychains/System.keychain "$ca_pem"

echo "==> serving $www on https://127.0.0.1:$port"
python3 - "$www" "$tls/server.pem" "$tls/server.key" "$port" >"$work_dir/server.log" 2>&1 <<'PY' &
import functools
import http.server
import ssl
import sys

root, cert, key, port = sys.argv[1], sys.argv[2], sys.argv[3], int(sys.argv[4])
handler = functools.partial(http.server.SimpleHTTPRequestHandler, directory=root)
server = http.server.HTTPServer(("127.0.0.1", port), handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(cert, key)
server.socket = context.wrap_socket(server.socket, server_side=True)
server.serve_forever()
PY
server_pid=$!

for attempt in $(seq 1 30); do
    if curl -fsS -o /dev/null "https://127.0.0.1:$port/WriteMoreBetterFFI.xcframework.zip"; then
        break
    fi
    if [ "$attempt" -eq 30 ]; then
        echo "error: the local HTTPS server never became reachable and trusted" >&2
        cat "$work_dir/server.log" >&2
        exit 1
    fi
    sleep 1
done

# --- a tiny consumer package -------------------------------------------------
consumer="$work_dir/consumer"
mkdir -p "$consumer/Sources/wmb-consumer"
cat > "$consumer/Package.swift" <<EOF
// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "wmb-consumer",
    platforms: [.macOS(.v13)],
    dependencies: [.package(url: "file://$pkg", exact: "$version")],
    targets: [
        .executableTarget(
            name: "wmb-consumer",
            // The dependency's identity is its URL's last path component, not the name in
            // its manifest.
            dependencies: [.product(name: "WriteMoreBetter", package: "writemorebetter")]
        ),
    ]
)
EOF
cat > "$consumer/Sources/wmb-consumer/main.swift" <<'SWIFT'
import Foundation
import WriteMoreBetter

let dataDirectory = URL(fileURLWithPath: CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : ".")
let checker = try Checker(
    tokenizerPath: dataDirectory.appendingPathComponent("en_tokenizer.bin").path,
    rulesPath: dataDirectory.appendingPathComponent("en_rules.bin").path,
    spellVariant: "en_US"
)

let text = "She was not been here since Monday."
let suggestions = checker.check(text: text)
guard let hit = suggestions.first(where: { $0.replacements.contains("has not been") }) else {
    FileHandle.standardError.write(
        Data("consumer check failed: BEEN_PART_AGREEMENT did not fire: \(suggestions.map(\.source))\n".utf8)
    )
    exit(1)
}
print("consumer check OK: \(hit.source) at \(hit.nsRange), \(suggestions.count) suggestions")
SWIFT

echo "==> resolving and building the consumer package"
swift build --package-path "$consumer"
binary="$(swift build --package-path "$consumer" --show-bin-path)/wmb-consumer"
if [ ! -x "$binary" ]; then
    echo "error: the consumer executable was not built at $binary" >&2
    exit 1
fi

# Build and run the product directly rather than through `swift run`: on the macOS
# runner the `swift run` process was SIGKILLed right after it finished linking (exit
# 137), while the linked product runs. The one-shot re-sign and retry stays as a guard
# for a runner killing the first launch of a just-linked executable.
run_consumer() {
    "$binary" "$data_dir"
}

echo "==> running the consumer app"
attempt=0
until run_consumer; do
    status=$?
    attempt=$((attempt + 1))
    echo "consumer app exited with status $status (attempt $attempt)" >&2
    if [ "$attempt" -ge 2 ]; then
        echo "==> signature" >&2
        codesign -dvvv "$binary" >&2 2>&1 || true
        echo "==> recent system log for the launch" >&2
        sudo log show --last 3m --style compact \
            --predicate 'process == "wmb-consumer" OR eventMessage CONTAINS "wmb-consumer"' \
            2>/dev/null | tail -30 >&2 || true
        exit "$status"
    fi
    codesign --force --sign - "$binary" || true
    sleep 2
done
