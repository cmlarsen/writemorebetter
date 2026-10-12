# Fuzzing the checker (0.7.0, lane "fuzz")

`check` must never panic on text a user can type. The `nlprule/fuzz` crate holds the
harness; it is excluded from the workspace and has its own `[workspace]` table.

## What runs in CI

`.github/workflows/fuzz.yml` runs on every change to `nlprule/src`, `nlprule/tests`
or `nlprule/fuzz`. It:

1. generates the LT 6.6 build dir (same steps as `ci.yml`, cached),
2. compiles `storage/66`,
3. builds the `stress` binary, and
4. runs `stress --iterations 3000`, which replays the committed seed and crash
   corpora and then a seeded generator over boundary-heavy inputs.

The generator is deterministic (xorshift64\*), so the inputs are identical on every
run; no nightly toolchain, libFuzzer or sanitizer is needed. The whole job is a
couple of minutes on a warm cache.

## Layout

| Path | What it is |
|---|---|
| `nlprule/fuzz/fuzz_targets/suggest.rs` | libFuzzer target: `suggest` + `apply_suggestions` on arbitrary UTF-8 |
| `nlprule/fuzz/src/lib.rs` | engine loader and the deterministic generator |
| `nlprule/fuzz/src/bin/stress.rs` | the CI stress/replay binary |
| `nlprule/fuzz/corpus/suggest/` | committed seed corpus |
| `nlprule/fuzz/corpus/crashes/` | committed minimized crashes, replayed by `stress` |
| `nlprule/fuzz/make_seed_corpus.py` | rebuilds the seed corpus |

The engine loads the LT 6.6 binaries once, applies the `fiction` profile and enables
the `en_US` speller — the same configuration the writing app runs. Set
`WMB_FUZZ_DIR` to point at another binary directory.

## Running coverage fuzzing locally

Long-running coverage fuzzing is **not** part of CI (it needs nightly and a sanitizer
and is not deterministic). Reproduce it with:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz

# once: compile the LT 6.6 binaries (docs/PERF.md, "Reproduce")
mkdir -p storage/66
cargo run --release --all-features --bin compile -- \
    --build-dir data/en-lt66 --tokenizer-out storage/66/en_tokenizer.bin --rules-out storage/66/en_rules.bin

cd nlprule/fuzz
cargo +nightly fuzz run suggest -- \
    -max_len=256 -timeout=30 -max_total_time=3600
```

Run at least two campaigns: `-max_len=256` for token-level inputs and `-max_len=4096`
for paragraph-sized inputs. `-fork=N` runs N workers; `-ignore_timeouts=1` (fork mode)
keeps slow-but-not-crashing inputs from stopping the run. `WMB_FUZZ_DIR` defaults to
`../../storage/66`, i.e. the repo root's `storage/66` when cargo-fuzz runs from
`nlprule/fuzz`.

## Triaging a crash

```sh
cd nlprule/fuzz
cargo +nightly fuzz tmin suggest artifacts/suggest/crash-<hash>      # minimize
cp artifacts/suggest/crash-<hash> corpus/crashes/<short-name>         # commit for replay
```

Then add a regression test in `nlprule/tests/tests.rs` that feeds the input to
`RULES.suggest` (and `apply_suggestions`) and fix the root cause. Do **not** wrap the
call in `catch_unwind`: a panic is a bug in a UTF-8 boundary, an index or an
invariant, and the fix belongs at the source.

## Pathological slowness

The harness is also used to find slow inputs. `stress --timings corpus/suggest`
prints every input under 10 KB that takes over 1 s, plus the slowest inputs overall.

Findings from the 0.7.0 lane (`PROFILE=fiction SPELL=en_US`, LT 6.6, release):

| Input | Size | Before | Cause |
|---|---|---|---|
| `"a" * 100_000` | 100 KB | 121 s | speller Levenshtein search on one enormous token |
| `"a" * 100_000` (after fix) | 100 KB | 0.08 s | — |
| `"no sentence break here " * 5000` | 115 KB | 828 s | one sentence with ~23k tokens; rule matching scales ~O(tokens²) |
| `"“" * 2000` | 6 KB | 22 s | `TYPOGRAPHY/GERMAN_QUOTES` starts with `[„“]` and has an unbounded `skip="-1"` atom |
| `"“" * 1000` | 3 KB | 1.7 s | same |
| `"(" * 2000` | 4 KB | 0.8 s | long token run through the rule engine |

The speller blowup is fixed with a length cap in `VariantChecker::search`
(`nlprule/src/spell/mod.rs`). The rule-matching blowup on very long *sentences* is a
property of LT-style matching: a sentence with no terminator (or a long run of tokens
that every rule's first atom matches) makes each rule's unbounded skip scan the whole
sentence, so any fix has to be a sentence-length policy (splitting over-long
sentences before rule application), which changes which spans rules can match. It is
reported here rather than fixed in this lane.

## Seed corpus

`python3 nlprule/fuzz/make_seed_corpus.py` rebuilds `corpus/suggest`: paragraphs of
the committed public texts (`bench/public`) plus the handcrafted boundary cases
defined in `nlprule/fuzz/src/lib.rs` (empty, whitespace, U+2028/U+2029, lone combining
marks, ZWJ emoji, RTL, CJK, 10k-char words, 100k chars with no sentence break, deep
nesting, straight/curly quote mixing, repeated punctuation and every builtin's trigger
text). The handcrafted strings have a single definition in Rust; the script asks the
`stress` binary to write them.
