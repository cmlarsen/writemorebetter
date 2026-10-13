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

The engine is compiled with the `onig` backend by default. The WASM build uses
`regex-fancy`, whose backtracking behaviour differs; fuzz it too with

```sh
cargo +nightly fuzz run --no-default-features --features fancy suggest -- \
    -max_len=4096 -timeout=30 -max_total_time=3600
```

The deterministic `stress` binary takes the same features
(`cargo build --release --bin stress --no-default-features --features fancy`).

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

## Crashes found and fixed

1. **Suggestion span starts at zero at a later sentence's first token.** For an input
   like `"jW\u{2029},<d"` the `TYPOS/NON_STANDARD_WORD` rule matched `,<d` in the
   second sentence but reported a span starting at position 0. `Rules::apply_suggestions`
   subtracts earlier replacements' length from the position as an `isize` and casts it
   to `usize`, so the negative start wrapped and the slice index panicked. Root cause:
   `Suggestions::next` extends a span leftwards to the end of the preceding token for a
   replacement that starts with a no-space character; when that preceding token is the
   zero-width `SENT_START`, its default end (0) was used. Fixed in
   `nlprule/src/rule/mod.rs`; regression test
   `suggestions_at_a_second_sentence_start_apply_cleanly`; the minimized input is in
   `nlprule/fuzz/corpus/crashes/`.

## Pathological slowness

The harness is also used to find slow inputs. `stress --timings corpus/suggest`
prints every input under 10 KB that takes over 1 s, plus the slowest inputs overall.

### The rule-matching quadratic (fixed)

A rule that matches at *every* position of a long token run made
`MatchGraph::fill_empty` walk the whole sentence once per group per match. The walk is
O(sentence) and runs once per match, so a run of `n` matching tokens cost O(n²). The
fix reads "empty" from a group's span (already set by the matcher) instead of
re-deriving it with `Group::tokens`, so `fill_empty` no longer touches the sentence.
It has nothing to do with `skip="-1"`: `skip="-1"` is already capped at
`max_matches() = 20` (`compile/parse_structure.rs`), and lowering that cap to 4 left
the timings unchanged.

End-to-end `check` times (`PROFILE=fiction SPELL=en_US`, LT 6.6, release, min of 5;
median in parentheses). `nt` is public prose run together with no sentence terminator;
`oq` is `"“" * n`:

| Input | Before | After |
|---|---|---|
| nt 500 words | 0.082 s (0.092) | 0.088 s (0.101) |
| nt 1 000 words | 0.340 s (0.385) | 0.297 s (0.363) |
| nt 2 000 words | 1.881 s (2.058) | 1.183 s (1.253) |
| nt 5 000 words | 13.30 s (16.60) | 5.68 s (7.04) |
| oq 1 000 quotes | 1.193 s (1.209) | 0.294 s (0.298) |
| oq 2 000 quotes | 15.19 s (15.41) | 1.159 s (1.290) |

A shorter, ruleset-independent reproduction is `"I " * n`, which the regression test
`repeated_token_run_does_not_scan_quadratically` uses: `n = 1000` went 1.5 s → 0.16 s
and `n = 2000` went 21.0 s → 0.6 s (identical on LT 5.2 and 6.6).

### The residual superlinear cost on long sentences (fixed by PR #127)

The `nt` rows above are still slow because a sentence with no terminator costs more in
the **tokenizer** than in the rules. Tagging is linear; the two superlinear stages after
it are rule-based **disambiguation** and the **chunker**. Stage times at
1 000 / 2 000 / 5 000 tokens, one sentence, sequential (`prepared_sentences` =
sentencizing + tagging only):

| stage | 1 000 | 2 000 | 5 000 |
|---|---|---|---|
| tagging (`prepared_sentences`) | 0.002 s | 0.003 s | 0.008 s |
| chunker (`chunk_apply`) | 0.020 s | 0.063 s | 0.380 s |
| disambiguation (`disambiguate`) | 0.047 s | 0.147 s | 0.745 s |

Disambiguation is dominated by a few rules re-applied across iterations, most of all
`DISAMBIGUATION/VBN_VBD/0` (0.0033 s → 0.025 s → 0.13 s), and the time is inside the
rule's match iteration, not group-token lookups. The per-iteration clone/`into_sentence`
(≈0.046 s of the 0.75 s at 5 000 tokens) and making `Group::tokens` span-local (0.745 s
→ 0.697 s) were both measured and ruled out. The full pipeline at 5 000 tokens is
1.63 s, of which ~0.5 s is outside tagging/chunking/disambiguation (multiword tagging
and materialising the `Sentence`). This is why nt 1 000 words cannot reach 100 ms.

It was tracked in
[#101 engine: disambiguation and chunker are superlinear on a single long sentence](https://github.com/cmlarsen/writemorebetter/issues/101)
and fixed in PR #127: antipattern spans are computed once per rule and sentence, a
disambiguation change only scans the tokens inside its match, and the chunker's beam search
shares back-pointers instead of copying each hypothesis. The PR measured a 5 000-word `nt` sentence going from
~6.6 s to ~0.8 s. Disambiguation no longer rebuilds the sentence once per applied rule; it
refreshes only the tokens each rule changed
([#125](https://github.com/cmlarsen/writemorebetter/issues/125)).

The speller blowup (`"a" * 100_000`, 121 s → 0.08 s) is fixed with a length cap in
`VariantChecker::search` (`nlprule/src/spell/mod.rs`).

## Seed corpus

`python3 nlprule/fuzz/make_seed_corpus.py` rebuilds `corpus/suggest`: paragraphs of
the committed public texts (`bench/public`) plus the handcrafted boundary cases
defined in `nlprule/fuzz/src/lib.rs` (empty, whitespace, U+2028/U+2029, lone combining
marks, ZWJ emoji, RTL, CJK, 10k-char words, 100k chars with no sentence break, deep
nesting, straight/curly quote mixing, repeated punctuation and every builtin's trigger
text). The handcrafted strings have a single definition in Rust; the script asks the
`stress` binary to write them.
