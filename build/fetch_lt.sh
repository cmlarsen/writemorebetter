#!/usr/bin/env bash
#
# Fetch everything needed to generate an nlprule build directory from the latest
# LanguageTool desktop release, without installing anything system-wide and
# without sudo:
#
#   * LanguageTool-stable.zip (unzipped), and
#   * a portable Temurin 21 JRE used to run the LanguageTool DictionaryExporter.
#
# Usage:
#   build/fetch_lt.sh [CACHE_DIR]
#
# CACHE_DIR defaults to data/cache. Anything already present in the cache is
# reused, so re-running the script is cheap.
#
# On success it prints, on stdout:
#   LT_VERSION=<LanguageTool-6.6>
#   LT_DIR=<absolute path to the unzipped LanguageTool directory>
#   JAVA_BIN=<absolute path to the portable java binary>
set -euo pipefail

CACHE_DIR="${1:-data/cache}"
mkdir -p "$CACHE_DIR"
CACHE_DIR="$(cd "$CACHE_DIR" && pwd)"

# --- detect platform ---------------------------------------------------------
uname_s="$(uname -s)"
uname_m="$(uname -m)"
case "$uname_s" in
    Darwin) os="mac" ;;
    Linux) os="linux" ;;
    *)
        echo "error: unsupported OS '$uname_s' (expected Darwin or Linux)" >&2
        exit 1
        ;;
esac
case "$uname_m" in
    arm64 | aarch64) arch="aarch64" ;;
    x86_64 | amd64) arch="x64" ;;
    *)
        echo "error: unsupported architecture '$uname_m'" >&2
        exit 1
        ;;
esac

lt_url="https://languagetool.org/download/LanguageTool-stable.zip"
jre_url="https://api.adoptium.net/v3/binary/latest/21/ga/${os}/${arch}/jre/hotspot/normal/eclipse"

# --- LanguageTool desktop ----------------------------------------------------
lt_zip="$CACHE_DIR/LanguageTool-stable.zip"
if [ ! -f "$lt_zip" ]; then
    echo "Downloading LanguageTool-stable.zip ..." >&2
    curl -fSL --retry 3 -o "$lt_zip.part" "$lt_url"
    mv "$lt_zip.part" "$lt_zip"
else
    echo "Reusing $lt_zip" >&2
fi

lt_dir="$(find "$CACHE_DIR" -maxdepth 1 -type d -name 'LanguageTool-*' | sort | head -n1)"
if [ -z "$lt_dir" ]; then
    echo "Unzipping $lt_zip ..." >&2
    unzip -q "$lt_zip" -d "$CACHE_DIR"
    lt_dir="$(find "$CACHE_DIR" -maxdepth 1 -type d -name 'LanguageTool-*' | sort | head -n1)"
fi
if [ -z "$lt_dir" ]; then
    echo "error: no LanguageTool-* directory found after unzipping" >&2
    exit 1
fi
lt_dir="$(cd "$lt_dir" && pwd)"
lt_version="$(basename "$lt_dir")"

# --- portable Temurin 21 JRE -------------------------------------------------
jre_archive="$CACHE_DIR/temurin21-jre.tar.gz"
jre_dir="$CACHE_DIR/temurin21-jre"
java_bin="$(find "$jre_dir" -type f -path '*/bin/java' 2>/dev/null | sort | head -n1 || true)"

if [ -z "$java_bin" ]; then
    if [ ! -f "$jre_archive" ]; then
        echo "Downloading portable Temurin 21 JRE ($os/$arch) ..." >&2
        curl -fSL --retry 3 -o "$jre_archive.part" "$jre_url"
        mv "$jre_archive.part" "$jre_archive"
    else
        echo "Reusing $jre_archive" >&2
    fi
    echo "Unpacking $jre_archive ..." >&2
    mkdir -p "$jre_dir"
    tar -xzf "$jre_archive" -C "$jre_dir"
    java_bin="$(find "$jre_dir" -type f -path '*/bin/java' | sort | head -n1)"
fi
if [ -z "$java_bin" ]; then
    echo "error: no java binary found under $jre_dir" >&2
    exit 1
fi
java_bin="$(cd "$(dirname "$java_bin")" && pwd)/java"

echo "LT_VERSION=$lt_version"
echo "LT_DIR=$lt_dir"
echo "JAVA_BIN=$java_bin"
