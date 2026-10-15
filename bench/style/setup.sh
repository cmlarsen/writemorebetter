#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
cache=data/style-bench
revision=3d900fc2224e55960c3363826ae28539b77b4204
if [ ! -x "$cache/venv/bin/python" ]; then
    uv venv --python 3.11 "$cache/venv"
fi
uv pip install --python "$cache/venv/bin/python" -r bench/style/requirements.lock
if [ ! -d "$cache/booknlp/.git" ]; then
    git clone https://github.com/booknlp/booknlp.git "$cache/booknlp"
fi
git -C "$cache/booknlp" checkout --detach "$revision"
cargo build --release --manifest-path bench/tools/wmb-style/Cargo.toml
