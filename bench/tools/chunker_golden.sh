#!/usr/bin/env bash
# Dump or check the chunker's output for the public texts, byte-for-byte.
#
#   bench/tools/chunker_golden.sh --write   # regenerate (reserved for an approved output change)
#   bench/tools/chunker_golden.sh --check   # compare against bench/golden/chunker
#
# Unlike golden.sh (which captures whole-check suggestion lines), this captures only the
# chunker: every token's chunk tags plus the chunk model's internal tokens/POS/chunk labels,
# for the LT 6.6 rules. It runs the pipeline up to chunking once and the chunker on its own.
set -euo pipefail

mode=check
if [[ ${1:-} == --check || ${1:-} == --write ]]; then
    mode=${1#--}
    shift
fi

root=$(cd "$(dirname "$0")/../.." && pwd)
bin="$root/bench/tools/wmb-chunkbench/target/release/wmb-chunkbench"
texts=()
for name in moby150k story1 story2 story3 story4 story5; do
    texts+=("$root/bench/public/$name.txt")
done

cargo build --release --manifest-path "$root/bench/tools/wmb-chunkbench/Cargo.toml" >/dev/null

for rules in 66; do
    "$bin" "$root/storage/$rules" --golden="$mode" --label="$rules" \
        --out="$root/bench/golden/chunker" "${texts[@]}"
done
