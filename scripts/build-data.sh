#!/usr/bin/env bash
#
# Build the release data binaries (`en_tokenizer.bin`, `en_rules.bin`) from pinned
# inputs, on a clean macOS or Linux runner.
#
# Every input is pinned by sha256 and the LanguageTool build directory is generated
# from scratch, so two runs from a clean tree produce byte-identical binaries; the
# checksums are written to `<out-dir>/SHA256SUMS`. CI and the release workflow call
# this same script so they cannot drift.
#
# Usage:
#   scripts/build-data.sh [--out-dir DIR] [--cache-dir DIR] [--build-dir DIR]
#
# Defaults:
#   --out-dir    dist/data    binaries, SHA256SUMS, LICENSE-LGPL-2.1.txt, README.md
#   --cache-dir  data/cache   downloads, portable JRE, Python venv (safe to cache)
#   --build-dir  data/en-lt66 generated LanguageTool build directory (wiped first)
set -euo pipefail

# LanguageTool desktop release and nlprule's en.zip (source of the OpenNLP chunker
# model, which LT 6.6 reuses). Both are pinned; a mismatch aborts the build.
LT_VERSION=6.6
LT_ZIP_SHA256=53600506b399bb5ffe1e4c8dec794fd378212f14aaf38ccef9b6f89314d11631
EN_ZIP_URL=https://f000.backblazeb2.com/file/nlprule/en.zip
EN_ZIP_SHA256=57a0ded6f3becbb669835c0ef9a8841c642cd809c005357ff48bf5c6319adf01

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"

out_dir=dist/data
cache_dir=data/cache
build_dir=data/en-lt66
while [ $# -gt 0 ]; do
    case "$1" in
        --out-dir) out_dir=$2; shift 2 ;;
        --cache-dir) cache_dir=$2; shift 2 ;;
        --build-dir) build_dir=$2; shift 2 ;;
        -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
        *)
            echo "error: unknown argument '$1'" >&2
            exit 1
            ;;
    esac
done

mkdir -p "$cache_dir"

sha256_of() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

verify() { # FILE EXPECTED
    local actual
    actual=$(sha256_of "$1")
    if [ "$actual" != "$2" ]; then
        echo "error: sha256 mismatch for $1" >&2
        echo "  expected $2" >&2
        echo "  actual   $actual" >&2
        exit 1
    fi
}

# --- pinned LanguageTool desktop release -------------------------------------
# Download it here (rather than letting fetch_lt.sh do it) so the checksum is
# verified before anything is unzipped or used.
lt_zip="$cache_dir/LanguageTool-$LT_VERSION.zip"
if [ ! -f "$lt_zip" ]; then
    echo "Downloading LanguageTool $LT_VERSION ..." >&2
    curl -fSL --retry 3 -o "$lt_zip.part" \
        "https://languagetool.org/download/LanguageTool-$LT_VERSION.zip"
    mv "$lt_zip.part" "$lt_zip"
fi
verify "$lt_zip" "$LT_ZIP_SHA256"

# fetch_lt.sh reuses the verified zip, fetches a portable Temurin 21 JRE and
# unzips both; it prints LT_VERSION, LT_DIR and JAVA_BIN.
eval "$(build/fetch_lt.sh "$cache_dir" "$LT_VERSION")"

# --- pinned nlprule en.zip (OpenNLP chunker model) ---------------------------
en_zip="$cache_dir/en.zip"
if [ ! -f "$en_zip" ]; then
    echo "Downloading nlprule en.zip ..." >&2
    curl -fSL --retry 3 -o "$en_zip.part" "$EN_ZIP_URL"
    mv "$en_zip.part" "$en_zip"
fi
verify "$en_zip" "$EN_ZIP_SHA256"

# --- Python build-dir generator ----------------------------------------------
venv="$cache_dir/venv"
if [ ! -x "$venv/bin/python" ]; then
    python3 -m venv "$venv"
fi
"$venv/bin/pip" install --quiet --disable-pip-version-check -r build/requirements.txt

# --- generate the build directory from scratch --------------------------------
rm -rf "$build_dir"

# LT 6.x keeps the tagger dictionary and the Morfologik speller dictionaries
# inside a jar; extract them next to the other English resources.
unzip -o "$LT_DIR/libs/english-pos-dict.jar" \
    'org/languagetool/resource/en/english.dict' \
    'org/languagetool/resource/en/english.info' \
    'org/languagetool/resource/en/hunspell/*.dict' \
    'org/languagetool/resource/en/hunspell/*.info' \
    -d "$LT_DIR" >/dev/null

"$venv/bin/python" build/make_build_dir.py \
    --lt_dir "$LT_DIR" \
    --lang_code en \
    --tag_dict_path "$LT_DIR/org/languagetool/resource/en/english.dict" \
    --tag_info_path "$LT_DIR/org/languagetool/resource/en/english.info" \
    --spell_map_path "$LT_DIR/org/languagetool/rules/en/contractions.txt" \
    --grammar_path build/lt-rules/en/grammar.xml \
    --segment_path build/lt-rules/core/segment.srx \
    --java "$JAVA_BIN" \
    --out_dir "$build_dir"

# The chunker model and the builtin word lists come from nlprule's en.zip and the
# committed `build/builtin-data` (LanguageTool-derived, LGPL-2.1).
unzip -oq "$en_zip" -d "$cache_dir"
cp "$cache_dir/en/chunker.json" "$build_dir/chunker.json"
mkdir -p "$build_dir/builtin"
cp -r build/builtin-data/en/. "$build_dir/builtin/"

# --- compile the two binaries -------------------------------------------------
mkdir -p "$out_dir"
cargo run --release --all-features --bin compile -- \
    --build-dir "$build_dir" \
    --tokenizer-out "$out_dir/en_tokenizer.bin" \
    --rules-out "$out_dir/en_rules.bin"

# --- package the data with its license, readme and checksums ------------------
cp binaries/README.md "$out_dir/README.md"
cp binaries/LICENSE-LGPL-2.1.txt "$out_dir/LICENSE-LGPL-2.1.txt"
(
    cd "$out_dir"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum en_tokenizer.bin en_rules.bin
    else
        shasum -a 256 en_tokenizer.bin en_rules.bin
    fi
) > "$out_dir/SHA256SUMS"

echo "Wrote release data to $out_dir:" >&2
ls -l "$out_dir" >&2
