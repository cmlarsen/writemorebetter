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
    --debug) PROFILE="dev" ;;
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

# Minimum OS versions for the slices and the framework's Info.plist.
MACOSX_MIN="12.0"
IPHONEOS_MIN="16.0"

CRATE="writemorebetter-swift"
LIB_NAME="libwritemorebetter_swift.dylib"
FFI_NAME="WriteMoreBetterFFI"
BUNDLE_ID="com.github.cmlarsen.${FFI_NAME}"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' swift/Cargo.toml | head -n1)
OUT="dist"
GENERATED="${OUT}/generated"
BINDINGS_OUT="swift/Sources/WriteMoreBetter/Generated"

# nlprule's default feature is the native `regex-onig` backend. It builds for every Apple target,
# so no slice falls back to `regex-fancy`.
BACKEND="regex-onig (native)"

build_slice() {
    local target=$1
    local min_args=() slice_env=()
    case "$target" in
        *-apple-darwin) min_args=(-C "link-arg=-mmacosx-version-min=${MACOSX_MIN}") ;;
        *-apple-ios*) slice_env=("IPHONEOS_DEPLOYMENT_TARGET=${IPHONEOS_MIN}") ;;
    esac
    env ${slice_env[@]+"${slice_env[@]}"} \
        cargo rustc --profile "$PROFILE" -p "$CRATE" --lib --target "$target" -- \
        -C strip=none \
        -C "link-arg=-Wl,-install_name,@rpath/${FFI_NAME}.framework/${FFI_NAME}" \
        -C "link-arg=-Wl,-x" -C "link-arg=-Wl,-S" \
        ${min_args[@]+"${min_args[@]}"}
}

slice_lib() {
    echo "target/${1}/${PROFILE_DIR}/${LIB_NAME}"
}

info_plist() {
    cat <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${FFI_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>${BUNDLE_ID}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${FFI_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>${VERSION}</string>
    <key>CFBundleVersion</key>
    <string>${VERSION}</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>$1</string>
    </array>
    <key>$2</key>
    <string>$3</string>
</dict>
</plist>
EOF
}

make_framework() {
    local dir=$1 platform=$2
    shift 2
    local framework="${OUT}/tmp/${dir}/${FFI_NAME}.framework"
    local contents="$framework"
    if [ "$platform" = "MacOSX" ]; then
        contents="${framework}/Versions/A"
        mkdir -p "${contents}/Resources"
        info_plist "$platform" LSMinimumSystemVersion "$MACOSX_MIN" \
            > "${contents}/Resources/Info.plist"
        ln -s A "${framework}/Versions/Current"
        for entry in "$FFI_NAME" Headers Modules Resources; do
            ln -s "Versions/Current/${entry}" "${framework}/${entry}"
        done
    else
        mkdir -p "$contents"
        info_plist "$platform" MinimumOSVersion "$IPHONEOS_MIN" \
            > "${contents}/Info.plist"
    fi
    mkdir -p "${contents}/Headers" "${contents}/Modules"
    cp "${GENERATED}/${FFI_NAME}.h" "${contents}/Headers/"
    cp "${GENERATED}/module.modulemap" "${contents}/Modules/"
    lipo -create "$@" -output "${contents}/${FFI_NAME}"
    codesign --force --sign - --timestamp=none "$framework" >/dev/null
}

rm -rf "$OUT" "$BINDINGS_OUT"
mkdir -p "$OUT" "$GENERATED" "$BINDINGS_OUT"

echo "==> building macOS slices (aarch64-apple-darwin, x86_64-apple-darwin)"
build_slice aarch64-apple-darwin
build_slice x86_64-apple-darwin

echo "==> building iOS device slice (aarch64-apple-ios)"
build_slice aarch64-apple-ios

echo "==> building iOS simulator slices (aarch64-apple-ios-sim, x86_64-apple-ios)"
build_slice aarch64-apple-ios-sim
build_slice x86_64-apple-ios

echo "==> generating Swift bindings and modulemap"
cargo run --profile "$PROFILE" -p "$CRATE" --features cli --bin uniffi-bindgen -- \
    "$(slice_lib aarch64-apple-darwin)" "$GENERATED" \
    --swift-sources --headers --modulemap --xcframework \
    --module-name "$FFI_NAME" --modulemap-filename module.modulemap

echo "==> assembling ${OUT}/${FFI_NAME}.xcframework"
make_framework macos MacOSX \
    "$(slice_lib aarch64-apple-darwin)" "$(slice_lib x86_64-apple-darwin)"
make_framework ios iPhoneOS \
    "$(slice_lib aarch64-apple-ios)"
make_framework ios-simulator iPhoneSimulator \
    "$(slice_lib aarch64-apple-ios-sim)" "$(slice_lib x86_64-apple-ios)"
xcodebuild -create-xcframework \
    -framework "${OUT}/tmp/macos/${FFI_NAME}.framework" \
    -framework "${OUT}/tmp/ios/${FFI_NAME}.framework" \
    -framework "${OUT}/tmp/ios-simulator/${FFI_NAME}.framework" \
    -output "${OUT}/${FFI_NAME}.xcframework"
rm -rf "${OUT}/tmp"

cp "${GENERATED}/WriteMoreBetter.swift" "${BINDINGS_OUT}/WriteMoreBetter.swift"

echo
echo "backend: ${BACKEND}"
echo "slices:"
du -sh "${OUT}/${FFI_NAME}.xcframework/"* | sed 's/^/  /'
echo "total: $(du -sh "${OUT}/${FFI_NAME}.xcframework" | cut -f1)"
echo "bindings: ${BINDINGS_OUT}/WriteMoreBetter.swift"
