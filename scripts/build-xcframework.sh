#!/usr/bin/env bash
#
# Builds dist/WriteMoreBetterFFI.xcframework (universal macOS, iOS device and iOS simulator
# slices) and regenerates the Swift bindings in swift/Sources/WriteMoreBetter/Generated/.
#
# The two English data binaries (en_tokenizer.bin, en_rules.bin) are LGPL-2.1 resources and are
# not part of either artifact: the checker loads them from disk at runtime, see swift/README.md.
#
# Requires a macOS host with Xcode and these Rust targets:
#   aarch64-apple-darwin x86_64-apple-darwin aarch64-apple-ios x86_64-apple-ios aarch64-apple-ios-sim
#
# Usage: scripts/build-xcframework.sh [--debug]
set -euo pipefail

cd "$(dirname "$0")/.."

PROFILE="release"
case "${1:-}" in
    "") ;;
    --debug) PROFILE="debug" ;;
    --help)
        sed -n '2,12p' "$0"
        exit 0
        ;;
    *)
        echo "unknown option: $1" >&2
        exit 2
        ;;
esac

if [ "$PROFILE" = "release" ]; then
    PROFILE_DIR="release"
else
    PROFILE_DIR="debug"
fi

CRATE="writemorebetter-swift"
LIB_NAME="libwritemorebetter_swift.a"
FFI_NAME="WriteMoreBetterFFI"
# The library inside each slice keeps the `lib` prefix: SwiftPM derives a `-l<name>` link flag from
# the file name and rejects anything that does not start with `lib` ("unexpected binary framework").
FFI_LIB="lib${FFI_NAME}.a"
OUT="dist"
GENERATED="${OUT}/generated"
BINDINGS_OUT="swift/Sources/WriteMoreBetter/Generated"

# nlprule's default feature is the native `regex-onig` backend. It builds for every Apple target,
# so no slice falls back to `regex-fancy`.
BACKEND="regex-onig (native)"

build_slice() {
    cargo build --profile "$PROFILE" -p "$CRATE" --target "$1"
}

slice_lib() {
    echo "target/${1}/${PROFILE_DIR}/${LIB_NAME}"
}

rm -rf "$OUT" "$BINDINGS_OUT"
mkdir -p "$OUT" "$GENERATED" "$BINDINGS_OUT"

echo "==> building macOS slices (aarch64-apple-darwin, x86_64-apple-darwin)"
build_slice aarch64-apple-darwin
build_slice x86_64-apple-darwin
mkdir -p "${OUT}/tmp/macos"
lipo -create "$(slice_lib aarch64-apple-darwin)" "$(slice_lib x86_64-apple-darwin)" \
    -output "${OUT}/tmp/macos/${FFI_LIB}"

echo "==> building iOS device slice (aarch64-apple-ios)"
build_slice aarch64-apple-ios
mkdir -p "${OUT}/tmp/ios"
cp "$(slice_lib aarch64-apple-ios)" "${OUT}/tmp/ios/${FFI_LIB}"

echo "==> building iOS simulator slices (aarch64-apple-ios-sim, x86_64-apple-ios)"
build_slice aarch64-apple-ios-sim
build_slice x86_64-apple-ios
mkdir -p "${OUT}/tmp/ios-simulator"
lipo -create "$(slice_lib aarch64-apple-ios-sim)" "$(slice_lib x86_64-apple-ios)" \
    -output "${OUT}/tmp/ios-simulator/${FFI_LIB}"

echo "==> generating Swift bindings and modulemap"
# No --xcframework: that makes uniffi emit `framework module WriteMoreBetterFFI`, which clang
# rejects for a static-library slice (a framework module cannot live outside a .framework), so
# SwiftPM silently compiles without the FFI module. The slices here are static libraries with the
# modulemap in Headers/, where a plain `module` is correct.
cargo run --profile "$PROFILE" -p "$CRATE" --features cli --bin uniffi-bindgen -- \
    "$(slice_lib aarch64-apple-darwin)" "$GENERATED" \
    --swift-sources --headers --modulemap \
    --module-name "$FFI_NAME" --modulemap-filename module.modulemap

echo "==> assembling ${OUT}/${FFI_NAME}.xcframework"
# Bindgen has already read the metadata symbols out of the macOS slice, so the shipped archives can
# lose their debug info: 26 MB per slice becomes 11 MB. `strip` warns once per object that is
# already free of debug info; the noise is not worth reading.
strip -S "${OUT}/tmp/macos/${FFI_LIB}" 2>/dev/null
strip -S "${OUT}/tmp/ios/${FFI_LIB}" 2>/dev/null
strip -S "${OUT}/tmp/ios-simulator/${FFI_LIB}" 2>/dev/null
xcodebuild -create-xcframework \
    -library "${OUT}/tmp/macos/${FFI_LIB}" -headers "$GENERATED" \
    -library "${OUT}/tmp/ios/${FFI_LIB}" -headers "$GENERATED" \
    -library "${OUT}/tmp/ios-simulator/${FFI_LIB}" -headers "$GENERATED" \
    -output "${OUT}/${FFI_NAME}.xcframework"
rm -rf "${OUT}/tmp"

cp "${GENERATED}/WriteMoreBetter.swift" "${BINDINGS_OUT}/WriteMoreBetter.swift"

echo
echo "backend: ${BACKEND}"
echo "slices:"
du -sh "${OUT}/${FFI_NAME}.xcframework/"* | sed 's/^/  /'
echo "total: $(du -sh "${OUT}/${FFI_NAME}.xcframework" | cut -f1)"
echo "bindings: ${BINDINGS_OUT}/WriteMoreBetter.swift"
