#!/usr/bin/env bash
set -uo pipefail

usage() {
    cat <<'EOF'
Run the repo's quality gates locally and print one summary.

Usage: bench/tools/gates.sh [--bins DIR] [--corpus DIR] [--skip-golden]

  conformance   rules passing their LanguageTool examples, against the CI floors
  public        bench/tools/public_recall.py against the CI floor
  private       recall on the private corpus (fiction profile) and the flag diff
                against the previous gates run
  golden        bench/tools/golden.sh --check and chunker_golden.sh --check

--bins defaults to storage. --corpus defaults to bench/corpus, or the main
checkout's bench/corpus when this worktree has none; the private gate is skipped
when no truth.json is found. Private flags and runs are written under
storage/gates (git-ignored).
EOF
}

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 1
bins=storage
corpus=
golden=1
while [ $# -gt 0 ]; do
    case "$1" in
        --bins) bins=$2; shift 2 ;;
        --corpus) corpus=$2; shift 2 ;;
        --skip-golden) golden=0; shift ;;
        -h | --help) usage; exit 0 ;;
        *) echo "error: unknown argument '$1'" >&2; exit 2 ;;
    esac
done

if [ -z "$corpus" ]; then
    corpus=bench/corpus
    if [ ! -f "$corpus/truth.json" ]; then
        main=$(git worktree list --porcelain | awk '/^worktree /{print $2; exit}')
        [ -f "$main/bench/corpus/truth.json" ] && corpus=$main/bench/corpus
    fi
fi

floor() { grep -m1 "$1:" .github/workflows/ci.yml | awk '{print $2}'; }
recall_min=$(floor RECALL_MIN)
grammar_passing=$(floor GRAMMAR_PASSING)
grammar_runnable=$(floor GRAMMAR_RUNNABLE)

work=storage/gates
mkdir -p "$work"
summary=()
status=0
record() { summary+=("$(printf '%-12s %-4s %s' "$1" "$2" "$3")"); [ "$2" = ok ] || [ "$2" = skip ] || status=1; }

echo "== building" >&2
cargo build --release --all-features --bin test >/dev/null 2>"$work/build.log" || { cat "$work/build.log" >&2; exit 1; }
cargo build --release --manifest-path bench/tools/wmb-probe/Cargo.toml >/dev/null 2>>"$work/build.log" || { cat "$work/build.log" >&2; exit 1; }
probe=bench/tools/wmb-probe/target/release/wmb-probe

echo "== conformance" >&2
target/release/test --tokenizer "$bins/en_tokenizer.bin" --rules "$bins/en_rules.bin" --list-failing \
    >"$work/conformance.log" 2>/dev/null
runnable=$(awk '/^Runnable rules: /{print $3}' "$work/conformance.log")
passing=$(awk '/^Rules passing tests: /{print $4}' "$work/conformance.log")
failing=$(awk '/^FAIL /{print $2}' "$work/conformance.log")
if [ -n "$failing" ]; then
    ids=()
    for id in $failing; do ids+=(--ids "$id"); done
    RUST_LOG=warn target/release/test --tokenizer "$bins/en_tokenizer.bin" --rules "$bins/en_rules.bin" "${ids[@]}" 2>&1 \
        | grep -E 'WARN .*failed' | sed -E 's/^\[[^]]*\] *//' | cut -c1-400
fi
if [ "${passing:-0}" -ge "$grammar_passing" ] && [ "${runnable:-0}" -ge "$grammar_runnable" ] && [ -z "$failing" ]; then
    record conformance ok "$passing/$runnable passing (floor $grammar_passing/$grammar_runnable)"
else
    record conformance FAIL "$passing/$runnable passing (floor $grammar_passing passing of $grammar_runnable runnable)${failing:+; failing: $(echo $failing)}"
fi

echo "== public recall" >&2
public=$(python3 bench/tools/public_recall.py --min "$recall_min" --bins "$bins" 2>&1)
public_status=$?
public_total=$(echo "$public" | grep -m1 -E '^ *total' | sed -E 's/^ *total *//')
if [ $public_status -eq 0 ]; then
    record public ok "$public_total"
else
    echo "$public" | tail -5
    record public FAIL "${public_total:-see output above}"
fi

echo "== private recall" >&2
if [ -f "$corpus/truth.json" ]; then
    runs=$work/private-runs
    mkdir -p "$runs"
    rm -f "$runs"/wmb-*.log
    for f in "$(dirname "$corpus")"/runs/*; do
        [ -e "$f" ] || continue
        case "$(basename "$f")" in wmb-*) ;; *) ln -sf "$f" "$runs/" ;; esac
    done
    for f in "$corpus"/*.txt; do
        n=$(basename "$f" .txt)
        PROFILE=fiction SHOW=1 "$probe" "$bins" "$f" >"$runs/wmb-$n.log" 2>/dev/null
    done
    table=$(python3 bench/tools/recall.py --runs "$runs" --truth "$corpus/truth.json" 2>&1)
    totals=$(echo "$table" | awk '/^\(n\)/{n=$NF} /^writemorebetter +[0-9]/{w=$NF} END{print w "/" n}')
    missed=$(echo "$table" | grep -m1 '^writemorebetter missed' | sed -E 's/^writemorebetter missed //')
    for f in "$runs"/wmb-*.log; do
        n=$(basename "$f" .log); n=${n#wmb-}
        grep ' | ' "$f" | sed "s/^/$n:/"
    done | sort >"$work/private-flags.txt"
    flags=$(wc -l <"$work/private-flags.txt" | tr -d ' ')
    change=""
    if [ -f "$work/private-flags.prev.txt" ]; then
        added=$(comm -13 "$work/private-flags.prev.txt" "$work/private-flags.txt")
        removed=$(comm -23 "$work/private-flags.prev.txt" "$work/private-flags.txt")
        na=$(printf '%s' "$added" | grep -c . || true)
        nr=$(printf '%s' "$removed" | grep -c . || true)
        change=", vs previous run +$na -$nr"
        { printf '%s\n' "$added" | grep . | sed 's/^/  + /'; printf '%s\n' "$removed" | grep . | sed 's/^/  - /'; } \
            | cut -d'|' -f1-3 | head -40
    fi
    cp "$work/private-flags.txt" "$work/private-flags.prev.txt"
    if [ "${totals%/*}" = "${totals#*/}" ]; then
        record private ok "$totals, $flags flags$change"
    else
        record private FAIL "$totals, $flags flags$change; missed $missed"
    fi
else
    record private skip "no truth.json under $corpus"
fi

if [ $golden -eq 1 ]; then
    echo "== golden" >&2
    if out=$(bench/tools/golden.sh --check 2>&1); then
        record golden ok "$(echo "$out" | tail -1)"
    else
        echo "$out" | grep -E '^(---|\+\+\+|[-+][0-9])' | cut -c1-160 | head -40
        record golden FAIL "see diff above; bench/tools/golden.sh --write after review"
    fi
    if out=$(bench/tools/chunker_golden.sh --check 2>&1); then
        record chunker ok "$(echo "$out" | grep -c '^ok ') files"
    else
        echo "$out" | grep -A1 '^MISMATCH' | head -20
        record chunker FAIL "see mismatch above; bench/tools/chunker_golden.sh --write after review"
    fi
fi

echo
printf '%s\n' "${summary[@]}"
exit $status
