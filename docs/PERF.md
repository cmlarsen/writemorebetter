# Speed: making the English check faster (2026-10-11)

Milestone PERF-CHECK-TIME. Making the check faster without changing which rules exist or
what they report. Every change is behind a measurement; identities are byte-for-byte.

## Headline

`bench/tools/wmb-probe`, `PROFILE=fiction SPELL=en_US`, first 150,000 bytes of Moby-Dick
(`data/moby150k.txt`), release builds. Check time is the median of 5 runs before and 7 runs
after (and every per-step number below is a median of 5); peak RSS from
`/usr/bin/time -l`.

| | check, before | check, after | peak RSS, before | peak RSS, after |
|---|---|---|---|---|
| LT 5.2 rules | 3.08 s | **2.15 s** | 76 MB | 78 MB |
| LT 6.6 rules | 3.79 s | **2.64 s** | 96 MB | 100 MB |

Target was LT 6.6 at or below 2.7 s with at most +5 MB peak RSS: met (−30% time, +4 MB).
Suggestion lines are byte-identical before and after for `story1..5` and `moby150k` with
both rule sets; conformance `--list-passing` is byte-identical for both rule sets; public
recall is unchanged (6.6 55/60, 5.2 54/60, the CI floors).

> **The machine was not idle.** Every run recorded a load average between 5 and 12 (the
> task's 3.0 s / 3.7 s baselines were taken on an idle machine). Both the before and the
> after numbers in this file were taken on the same machine under comparable load, so the
> ratios are meaningful but the absolute times are inflated. Repeated `median of 7`
> measurements varied by about ±0.05 s.

## Part 1 — profile

Method: `samply record` (sampling, 1 kHz; `cargo install samply`). The default run uses
rayon and is *parallel*, and on the parallel profile the main thread is parked in
`__psynch_cvwait` 59% of the time while the 15 worker threads mostly `sched_yield`/park —
almost all samples are scheduling, not work, so function attribution is useless there. So
the phase tables below come from a **sequential** profile (`NLPRULE_PARALLELISM=off`),
which shows what the work actually is; the parallel story is in Part 2.

### Before: top functions (sequential, LT 6.6, ~6.2 s)

Self time:

| self | function |
|---|---|
| 13.7% | `Atom::is_match` (enum-dispatched first/middle atom check) |
| 12.8% | `Matcher::is_match` (literal/`UniCase` and regex text comparison) |
| 10.2% | `_platform_memmove` (inside the Oniguruma matcher) |
| 9.4% | `onig::match_at` |
| 6.4% | `Composition::apply` |
| 6.2% | `WordDataMatcher::is_match` |
| 4.8% | `chunk::Model::eval` (chunker) |
| 2.7% | `unicase::UniCase<&str>::eq` |
| 1.7% | `Composition::apply_recursive` |
| 1.6% | `EngineMatches::next` |
| 1.4% | `fst::StreamWithState<Levenshtein>::next_with` (speller) |
| 1.2% | `malloc` |
| 1.1% | `Levenshtein::accept` (speller) |
| 0.9% | `fst::raw::node::Node::new` |
| 0.7% | `sip::Hasher::write` |

Inclusive time (`start`/`main` omitted):

| incl | function |
|---|---|
| 98.0% | `Rules::suggest` |
| 77.6% | `EngineMatches::next` |
| 75.9% | `Composition::apply` |
| 70.2% | `Rules::apply_paragraph` |
| 68.6% | `Rules::apply_with` (grammar rules + speller + builtins) |
| 63.7% | `Atom::is_match` |
| 63.3% | `Flatten<…>` in `apply_with` |
| 63.3% | `Vec<(usize, Suggestion)>` collect |
| 61.0% | `Rule::Suggestions::next` |
| 43.8% | `Matcher::is_match` |
| 28.2% | `onig::Regex::is_match` (all regex atoms) |
| 28.1% | `onig::Regex::match_with_options` |
| 24.1% | `onig_match_with_param` |
| 21.4% | `match_at` |
| 18.0% | `Tokenizer::disambiguate_up_to_id` |

Other phase functions for the split: `Tokenizer::tokenize` (tagging + chunking) 9.1%,
`Chunker::apply` 8.1%, `Spell::suggest` 5.2%, `Builtin::check_text` 1.5%.

Phase split (inclusive share of the sequential run):

| phase | before | after |
|---|---|---|
| grammar rule matcher (`Composition::apply`) | 75.9% | 72.6% |
| disambiguation | 18.0% | 14.9% |
| tokenize + tag + chunk | 9.1% | 8.7% |
| speller | 5.2% | 7.0% |
| builtins | 1.5% | 1.7% |

Takeaways: the shallow first-atom check that `Composition::apply` runs at every position
dominates (`Atom::is_match` self 13.7%, and `apply_recursive` — the "real" matching — is
only 1.7%), and regex atoms (`onig`) are together ~28% inclusive. The speller and builtins
are minor. Disambiguation is a real cost because it re-runs the whole disambiguation rule
set per sentence.

## Part 2 — changes

Both changes were kept behind the identity check (`storage/perf/verify.sh` rebuilds the
probe and diffs the `SHOW=1` suggestion lines of `story1..5` and `moby150k`).

### 1. Required-start-literal prefilter (`start_keys`)

`Composition` now stores, computed at compile time, the literal token texts a match of its
first part must start with (`start_keys`). The matcher looks those positions up in a
per-sentence token-text index and jumps straight there; if the sentence contains none of
them the rule is skipped without touching the engine at all. 54% of compositions have a
literal first part (`part0_kind=text`, from a compile-time census of 46,983
compositions); regex-first (20%) and POS-first (18%) are not covered. `Composition` is
serialized, so `RULES_FORMAT_VERSION` 6→7 and `TOKENIZER_FORMAT_VERSION` 4→5.

A per-rule token scan would have cost as much as the first-atom scan it replaces; the
saving comes from the sentence-level index, which is built once per sentence and shared by
all rules.

Check time, LT 6.6: 3.79 s → **3.13 s**; LT 5.2: 3.08 s → 2.87 s. Sequential time 6.12 s →
5.19 s. Peak RSS unchanged (the extra `start_keys` add ~570 KB to the rules binary).

A first version also sized the per-rule overlap mask lazily (it was a `Vec<bool>` the size
of the sentence's character count, allocated once per rule per sentence even when the rule
never matched); it was neutral on time but lowers allocation, so it stayed.

### 2. Batch sentences into one parallel region

`Rules::suggest` used to run one rayon region *per sentence* (a parallel iterate over all
~6,000 rules for one sentence). On a 15-core machine the workers spent more time waking and
parking than working: check 2.64 s of wall but **10.3 s user + 8.4 s sys**. Dropping to 6
threads cut sys from 8.4 s to 1.8 s and was *faster* (2.5 s), which is the tell.

`suggest` now collects tokenized sentences into batches of 128 and runs a whole batch's
rule checks in one rayon region (rules serial within a sentence). Same work, ~150 regions
instead of ~2,500. Check time: LT 6.6 3.13 s → **2.64 s**, LT 5.2 2.87 s → **2.15 s**.
The cost is peak RSS, which rises by ~2–3 MB from holding a batch at once (batch size is a
constant, `PARALLEL_BATCH`).

Thread scaling after (wall, LT 6.6): 8 threads 2.41 s, 15 threads 2.64 s — the remaining
gap is partly this machine's other load.

### Not changed

`RULES`/`TOKENIZER_FORMAT_VERSION` were bumped, nothing else. No rule, builtin, profile or
speller logic changed. The rayon thread count is left to the host (no library-side cap):
on an idle machine 15 threads should be at least as good as 8.

## Identity

`SHOW=1 PROFILE=default SPELL=en_US` suggestion lines, `story1..5` + `moby150k`, both rule
sets: **byte-identical** before/after (12 files).

## Conformance and recall

`test --list-passing` and `test_disambiguation --list-passing` are **byte-identical**
before/after for both rule sets, and match the CI floors:

| | runnable | passing | CI floor |
|---|---|---|---|
| grammar 5.2 | 4,335 | 4,320 | 4,320 |
| grammar 6.6 | 6,032 | 5,977 | 5,977 |
| disambiguation 5.2 | 843 | 843 | 843 |
| disambiguation 6.6 | 1,015 | 1,012 | 1,012 |

Public recall: 6.6 55/60, 5.2 54/60 (floors 55 / 54). Unchanged.

## Gates

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features`: no errors (the
  pre-existing warnings only).
- `cargo test --workspace --exclude nlprule-python --all-features --release`: all pass.
- `cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm`:
  compiles.
- Load/load times (6.6): tokenizer 81 ms / 40 MB, rules 33 ms / 65 MB, speller 2.4 ms.
  (5.2): 72 ms / 36 MB, 17 ms / 52 MB.

## Reproduce

```sh
# both build dirs, like .github/workflows/ci.yml (LT 5.2 = published en.zip, 6.6 = generated)
cargo run --release --all-features --bin compile -- \
    --build-dir data/en      --tokenizer-out storage/52/en_tokenizer.bin --rules-out storage/52/en_rules.bin
cargo run --release --all-features --bin compile -- \
    --build-dir data/en-lt66 --tokenizer-out storage/66/en_tokenizer.bin --rules-out storage/66/en_rules.bin

cargo build --release --manifest-path bench/tools/wmb-probe/Cargo.toml
PROFILE=fiction SPELL=en_US bench/tools/wmb-probe/target/release/wmb-probe storage/66 data/moby150k.txt
```

## M8 reproducible harness

`bench/tools/perf.sh` builds the release probe and checks the six public texts with both
rule sets, `PROFILE=fiction SPELL=en_US`, five independent processes each. It writes raw
samples and medians to `storage/perf.json`, a table to `storage/perf.md`, and prints the
table. Use `--skip-build` after rebuilding explicitly; `--rules 66 --texts moby150k`
selects a subset. Compile version-matched binaries into `storage/52` and `storage/66`
using the commands above after every engine change.

`bench/tools/golden.sh --check` checks 24 committed suggestion files: six texts, both
rule sets, both default and fiction profiles. Only suggestion lines are captured; timing
and memory diagnostics are excluded. Golden regeneration (`--write`) is reserved for an
explicitly approved output change after the Phase 0 review.

`bench/public/moby150k.txt` is the supplied first 150,000 bytes of the Project Gutenberg
Moby-Dick edition with its header and start marker removed (149,091 bytes, original CRLF
line endings retained). There is no footer in this truncated excerpt. This differs from
the header-bearing input in the earlier headline measurements; use the new baseline
for relative comparisons, and repeat the original input locally when judging parity
against the original 2.16/2.63 seconds and 78/100 MB targets. No private texts are used.

The Performance workflow checks goldens and compares the PR with its merge base in one
Ubuntu job. Each engine compiles its own data, with the same generated build inputs and
dependency lockfiles; the current probe code measures both engines. Native check/load
seconds and RSS use five-run medians; peak RSS is also measured by `/usr/bin/time -v`.
Callgrind instruction totals and allocator-call counts cover one **whole process**
(load plus one fixed Moby-Dick check), LT 6.6 fiction, `NLPRULE_PARALLELISM=off` to avoid
thread scheduling noise. Allocator calls sum `malloc`/`calloc`/`realloc` call edges in the
same profile; there is no extra profiling pass and no allocated-byte estimate. A separate
Memcheck pass exceeded 14 minutes and was dropped before Phase 0 merged. Wall time never gates. Golden identity and the existing
conformance/public recall floors remain the automated correctness gates; instruction
and RSS deltas inform the keep/drop decision. Raw profiler files, allocator-call counts,
JSON samples and the summary table are uploaded as the `performance` artifact.

### Hill-climb ledger

Negative deltas mean improvement. Peak RSS columns use MiB (1,048,576 bytes); historical
MB labels above were the probe's rounded MiB. Each optimization is one PR; dropped
attempts remain recorded. Stop after five consecutive dropped/sub-1% attempts or 25
attempts total. Phase 0 establishes measurement and does not count as an optimization.

| Attempt | PR | Change | Instructions Δ (6.6) | Peak RSS Δ (6.6) | Peak RSS Δ (5.2) | Local check time Δ (6.6 / 5.2) | Kept/dropped |
|---|---|---|---:|---:|---:|---|---|
| 0 | pending | Public golden output and reproducible local/CI harness | pending | pending | pending | baseline | review pending |
