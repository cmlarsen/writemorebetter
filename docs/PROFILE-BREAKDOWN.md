# Profile breakdown of the English checker

Measured 2026-10-11 on main at `5b6845c` (after the M8 hill-climb, PR #66), `PROFILE=fiction`
`SPELL=en_US`, both English rule sets (LT 5.2 and LT 6.6), first 150,000 bytes of Moby-Dick
(`bench/public/moby150k.txt`, 149,091 bytes, 963 sentences / 30,112 tokens with 6.6 and
1,468 sentences / 30,370 tokens with 5.2). This is a measurement report: no engine code changed.
Every "expected gain" in the last section is an estimate from these numbers, not a measured result.

## Where the hill-climb left things

Final main on the CI `perf` job (Ubuntu, LT 6.6 whole-process Callgrind, 5-run medians for RSS)
against the Phase 0 baseline (#48):

| | Phase 0 baseline | final main | change |
|---|---:|---:|---:|
| instructions, LT 6.6 | 80.792 B | 51.068 B | −36.8% |
| peak RSS, LT 6.6 | 91.441 MiB | 73.371 MiB | −18.07 MiB (−19.8%) |
| peak RSS, LT 5.2 | 70.492 MiB | 65.457 MiB | −5.04 MiB (−7.1%) |
| allocator calls, LT 6.6 | 56.549 M | 25.819 M | −54.3% |

CI runners vary by about ±2 MiB in absolute peak between runs (see the ledger in `PERF.md`), so
the RSS rows are indicative; the per-PR deltas in the ledger were measured against their own merge
base. Nineteen attempts: 12 kept (#49, #51, #52, #55, #57, #59, #60, #61, #63, #65, #66, #68) and
7 dropped (#50, #53, #54, #56, #58, #62, #67).

## Method

| What | How | Noise |
|---|---|---|
| Instructions per function | Callgrind output from the CI `perf` job for the push of `5b6845c` (artifact `performance`, `head.callgrind`): whole process, load plus one check, LT 6.6, `NLPRULE_PARALLELISM=off`, `--cache-sim=no`. Summed by `bench/tools/profile_summary.py`. Inclusive cost of a group counts each call edge once, so nested matches are not double counted. | Deterministic per binary; the CI total moved +0.05% on the push whose code change was a no-op for native builds (#66), and 0.13% on the Phase 0 no-change PR. |
| Time per function, both rule sets | `samply record --rate 4000` on `wmb-probe`, `NLPRULE_PARALLELISM=off`, symbolicated from the `--unstable-presymbolicate` sidecar by the same script. About 12,000 samples per run. | Stage shares varied by at most 1.0 percentage point over three runs per rule set (typically ≤0.3); see "Noise" below. |
| Load phases and pipeline stages | `bench/tools/wmb-profile` (`phases`, `stages`) driven by `bench/tools/profile.py`: 5–7 fresh processes, medians. | Ranges are in the tables. Machine load average was 4–8 on 15 cores, so absolute times are inflated; ratios are stable. |
| Per-component RSS and load time | `nlprule/tests/memory_breakdown.rs` (now also records load time), fresh process per component and repetition, median of 3. | 0–2% spread for most components. |
| Heap bytes by structure, transient peak, allocation volume | dhat (`wmb-profile load\|heap` built with `--features dhat-heap`), classified by `bench/tools/dhat_summary.py`. | dhat counts requested bytes exactly; run-to-run identical for sequential runs. |

Platform notes: Callgrind and all CI numbers are Linux/x86-64 (glibc malloc, onig regex backend).
Local timings, RSS and dhat runs are macOS/arm64 (system allocator, which does not return freed
pages, so RSS growth includes allocator high-water). LT 5.2 has no Callgrind run: its function
shares come from the sampling profile, which agrees with Callgrind on LT 6.6 to within the
differences listed in "Instructions versus time" below.

Reproduce:

```sh
# binaries (data/en = LT 5.2 build dir, data/en-lt66 = LT 6.6, as in .github/workflows/perf.yml)
cargo run --release --all-features --bin compile -- --build-dir data/en \
    --tokenizer-out storage/52/en_tokenizer.bin --rules-out storage/52/en_rules.bin
cargo run --release --all-features --bin compile -- --build-dir data/en-lt66 \
    --tokenizer-out storage/66/en_tokenizer.bin --rules-out storage/66/en_rules.bin

# 1. load phases and pipeline stages (build once, 7 and 5 fresh processes)
python3 bench/tools/profile.py phases --rules 66 --runs 7
python3 bench/tools/profile.py stages --rules 66 --runs 5

# 2. per-component RSS and load time (needs en_*.bin directly in the storage directory)
mkdir -p storage/mem66 && cp storage/66/*.bin storage/mem66/
M2B_STORAGE=storage/mem66 cargo test --release -p nlprule --features memory-probe \
    --test memory_breakdown -- --ignored --nocapture

# 3. heap by structure after load, at the peak, and allocation volume
(cd bench/tools/wmb-profile && CARGO_TARGET_DIR=target/heap cargo build --release --features dhat-heap)
H=bench/tools/wmb-profile/target/heap/release/wmb-profile
DHAT_OUT=/tmp/load66.json $H load storage/66
NLPRULE_PARALLELISM=off DHAT_OUT=/tmp/seq66.json $H heap storage/66 bench/public/moby150k.txt
DHAT_OUT=/tmp/par66.json $H heap storage/66 bench/public/moby150k.txt
python3 bench/tools/dhat_summary.py end /tmp/load66.json
python3 bench/tools/dhat_summary.py delta /tmp/load66.json /tmp/par66.json
python3 bench/tools/dhat_summary.py total /tmp/seq66.json            # add --by-blocks for counts

# 4. execution profile: sampling (both rule sets) and Callgrind (from the CI artifact)
NLPRULE_PARALLELISM=off PROFILE=fiction SPELL=en_US samply record --save-only \
    --unstable-presymbolicate --rate 4000 -o /tmp/prof66.json.gz -- \
    bench/tools/wmb-probe/target/release/wmb-probe storage/66 bench/public/moby150k.txt
python3 bench/tools/profile_summary.py stages /tmp/prof66.json.gz bench/tools/profile_stages.json
gh run download <perf run id of a push to main> -n performance -D /tmp/ci-main
python3 bench/tools/profile_summary.py stages /tmp/ci-main/head.callgrind bench/tools/profile_stages.json
python3 bench/tools/profile_summary.py callers /tmp/ci-main/head.callgrind '^(malloc|calloc|realloc)$'
python3 bench/tools/profile_summary.py groups /tmp/prof66.json.gz 'builtin::en::(\w+)::' --root 'Rules>::suggest'
```

## 1. Loading

### Phases of a full load

`wmb-profile phases`, one process, in the order `wmb-probe` loads them. Median of 7 fresh
processes (range in parentheses); RSS growth is measured around each step in the same process, so
a later step can reuse memory an earlier step freed.

LT 6.6:

| phase | time ms | RSS growth MiB |
|---|---:|---:|
| tokenizer deserialize | 63.0 (62.2–93.5) | 31.95 (31.83–35.95) |
| rules deserialize (includes the speller data and the regex pattern pool) | 22.8 (20.5–33.5) | 21.16 (20.70–21.36) |
| profile apply (`fiction`, 161 rules disabled) | 0.4 (0.3–0.9) | 0.00 |
| speller variant select (`en_US`) | 0.0 (0.0–0.1) | 0.08 |
| **total** | **≈ 86** | **≈ 53.2** |

LT 5.2:

| phase | time ms | RSS growth MiB |
|---|---:|---:|
| tokenizer deserialize | 73.2 (73.0–86.8) | 32.69 (32.50–32.78) |
| rules deserialize | 20.2 (14.5–34.2) | 13.45 (13.20–13.66) |
| profile apply (157 rules disabled) | 0.4 (0.3–0.5) | 0.00 |
| speller variant select (`en_US`) | 0.1 (0.1–0.3) | 0.23 |
| **total** | **≈ 94** | **≈ 46.4** |

Two phases from the requested list do not show up as separate load costs:

- **Speller.** Selecting the variant is lazy; the speller cost is its data inside
  `rules deserialize` (2.2 ms, 1.5–1.7 MiB on 6.6; 2.9 ms, 2.2–2.5 MiB on 5.2; see below).
- **Regex construction.** Loading only interns the pattern strings (`intern_pattern`, 2.6 MiB heap
  on 6.6, 1.4 MiB on 5.2, inside `rules deserialize`). Compilation happens on first use during the
  check: Callgrind attributes 1.15% of all instructions (LT 6.6, whole run) to the first-use
  compile under `OnceCell::initialize`, about 0.6 B instructions, and the sampling profile 1.0%
  (6.6) / 0.7% (5.2). The sentencizer's own regexes are built eagerly at load (`srx::build_regex`,
  0.34% of instructions).

### Per component

`memory_breakdown`, fresh process per component, median of 3. "RSS" is growth after
deserializing that one component alone; load time is the deserialization alone. The
"together" rows are the whole `Tokenizer`/`Rules` loaded through the same probe, which is why they
differ slightly from the phase table above (a different process history).

| component | 6.6 serialized | 6.6 load ms | 6.6 RSS MiB | 5.2 serialized | 5.2 load ms | 5.2 RSS MiB |
|---|---:|---:|---:|---:|---:|---:|
| tokenizer (all components together) | 11.95 MB | 58.1 | 32.0 | 14.66 MB | 75.4 | 32.6 |
| tagger (FSTs + word store) | 5.21 MB | 28.1 | 8.3 | 8.29 MB | 43.9 | 13.0 |
|   tag FST | 2.08 MB | 3.8 | 2.2 | 3.32 MB | 6.2 | 5.3 |
|   synth FST | 2.11 MB | 3.9 | 4.2 | 3.34 MB | 6.2 | 5.3 |
|   word-store FST | 1.01 MB | 2.0 | 1.1 | 1.63 MB | 3.0 | 1.7 |
|   word_buf | 2.01 MB | 0.3 | 2.1 | 3.09 MB | 0.4 | 3.1 |
|   word_offsets | 0.86 MB | 0.5 | 1.0 | 1.33 MB | 0.8 | 1.4 |
| chunker | 5.72 MB | 12.8 | 11.0 | 5.72 MB | 12.7 | 11.0 |
|   POS model | 3.49 MB | 8.7 | 8.5 | 3.49 MB | 9.0 | 8.5 |
|   chunk model | 1.79 MB | 3.2 | 4.5 | 1.79 MB | 3.3 | 4.5 |
|   token model | 0.44 MB | 0.9 | 1.6 | 0.44 MB | 0.9 | 1.6 |
| multiword tagger | 0.28 MB | 5.6 | 8.1 | 0.05 MB | 1.1 | 2.2 |
| sentencizer (`srx`) | 0.01 MB | 10.1 | 8.3 | 0.01 MB | 9.9 | 8.4 |
| disambiguation rules | 0.73 MB | 1.3 | 3.0 | 0.58 MB | 1.0 | 2.6 |
| rules (all components together) | 10.26 MB | 16.5 | 23.5 | 6.89 MB | 16.2 | 17.7 |
| grammar rules | 8.54 MB | 13.2 | 20.5 | 4.82 MB | 8.2 | 13.5 |
| speller data (variant none) | 1.22 MB | 2.2 | 1.5 | 1.56 MB | 2.9 | 2.2 |
| speller, `en_US` selected | 1.22 MB | 2.2 | 1.7 | 1.56 MB | 2.9 | 2.5 |

Isolated loads sum to more than the whole (6.6 tokenizer: 8.3 + 3.0 + 11.0 + 8.1 + 8.3 = 38.7 MiB
against 32.0 MiB together) because each includes its own transient allocator high-water.

The load-time split follows the RSS split. As a share of the tokenizer's load time (isolated
loads, 6.6 / 5.2): tagger 48% / 58%, chunker 22% / 17%, sentencizer 17% / 13%, multiword tagger
10% / 1.5%, disambiguation rules (1,015 / 843 of them) 2% / 1.3%. The sentencizer is the outlier per byte: a 9 KB file costs 10 ms and
8.3 MiB because it compiles 85 regexes during deserialization.

## 2. Memory

### Resident after load

Three views of the same loaded state (LT 6.6 unless a column says otherwise).

**Heap bytes by structure after load** (dhat, requested bytes, leaked at exit so they count as
live; 45.67 MiB for 6.6, 43.20 MiB for 5.2):

| structure | 6.6 MiB | share | 5.2 MiB | share |
|---|---:|---:|---:|---:|
| grammar rules (parts, atoms, examples, suggesters, `Vec<Rule>`) | 15.27 | 33.4% | 10.77 | 24.9% |
| tagger (FSTs, word store) | 10.03 | 22.0% | 14.42 | 33.4% |
| chunker | 7.93 | 17.4% | 7.93 | 18.4% |
| sentencizer (`srx` regexes) | 3.30 | 7.2% | 3.32 | 7.7% |
| regex pattern pool (interned pattern strings) | 2.60 | 5.7% | 1.36 | 3.2% |
| speller | 2.06 | 4.5% | 2.50 | 5.8% |
| multiword tagger (contiguous NFA) | 1.90 | 4.2% | 0.45 | 1.0% |
| builtin rules | 1.54 | 3.4% | 1.54 | 3.6% |
| tokenizer disambiguation rules | 1.04 | 2.3% | 0.91 | 2.1% |

Largest single allocation sites after load (6.6): tagger deserialization 6.96 MiB (FST and word
buffers) plus 1.91 MiB in `Tagger::from`, `Vec<Rule>` storage 4.00 MiB, chunker `PmapEntry`
vectors 3.77 MiB and `ModelFields` 3.51 MiB, `srx::build_regex` 3.08 MiB, grammar matcher `Part`
vectors 2.20 MiB, speller deserialize 2.01 MiB, `Box<Match>` 1.88 MiB, example blobs 1.60 MiB,
interned patterns 1.50 MiB.

**RSS by structure, in-context** (`memory_breakdown`, full minus the component, macOS, MiB). These
partition the total exactly:

| | 6.6 | 5.2 |
|---|---:|---:|
| tokenizer total | 32.0 | 32.6 |
| tagger (residual) | 9.9 | 14.4 |
| chunker | 8.4 | 8.5 |
| sentencizer | 5.9 | 7.2 |
| multiword tagger | 5.7 | 1.7 |
| disambiguation rules | 2.1 | 0.8 |
| rules total | 23.5 | 17.7 |
| matcher part pool (unique parts; atoms 4.8 / 2.9, regex strings 2.6 / 2.0) | 7.9 | 4.6 |
| grammar rules, per-rule data | 11.3 | 8.5 |
| speller data | 1.0 | 2.6 |
| other rule-set fields (residual) | 3.3 | 2.0 |

In-context fields are not additive. Inside the chunker (6.6): the POS model is 4.7 MiB in-context
(blanking its parameters saves 1.3, outcomes 0.7, pmap 2.0, tag dictionary 1.9); the isolated
sub-model loads are in the component table. The in-context saving reported for blanking the whole
chunk model (0.1 MiB) disagrees with its isolated 4.5 MiB and with its sub-fields (0.8 + 0.7 +
0.8), so it is not used here. Inside the grammar rules (6.6, in-context): examples 1.8,
suggesters 1.9, antipatterns 1.3, per-rule engine 1.95, names/urls/categories 0.4.

**Whole-process RSS, CI (Linux, `wmb-probe`, median of 5):** loaded 53.85 MiB (6.6) and 50.07 MiB
(5.2), peak 73.37 and 65.46 MiB. The heap is 45.7 / 43.2 MiB of that; the remainder is the
binary, stacks and allocator overhead.

### Transient peak during a check

dhat heap at the global peak (`t-gmax`) against the loaded heap, on moby150k:

| run | loaded heap | heap at peak | transient | total allocated | blocks |
|---|---:|---:|---:|---:|---:|
| 6.6, sequential | 45.67 MiB | 51.50 MiB | +5.83 | 2,170 MiB | 17.4 M |
| 6.6, parallel (rayon default) | 45.67 MiB | 52.55 MiB | +6.88 | 2,369 MiB | 18.3 M |
| 5.2, sequential | 43.20 MiB | 48.22 MiB | +5.02 | 1,780 MiB | 18.0 M |
| 5.2, parallel | 43.20 MiB | 49.32 MiB | +6.12 | 1,808 MiB | 18.3 M |

Where the +5.8 MiB (6.6, sequential run) sits at the peak:

| structure | loaded MiB | at peak MiB | transient MiB |
|---|---:|---:|---:|
| sentencizer (`srx` match caches) | 3.30 | 5.60 | +2.30 |
| grammar rules (lazily compiled regexes, match graphs) | 15.27 | 17.10 | +1.83 |
| chunker (beam-search sequences) | 7.93 | 8.95 | +1.02 |
| speller | 2.06 | 2.43 | +0.37 |
| everything else | 17.1 | 17.4 | +0.3 |

The parallel run's extra 1.05 MiB is batch state for up to 128 sentences held across worker
threads. The peak RSS on Linux (73.4 / 65.5 MiB) is 20 MiB
above the peak heap. The cause was not measured; the candidates are the binary, thread stacks
and allocator arenas (a per-thread glibc arena effect is a hypothesis, not a finding).

Allocation volume over one check (6.6, sequential, 2,170 MiB in 17.4 M blocks), by structure and
by the largest sites:

| structure | MiB allocated | blocks |
|---|---:|---:|
| grammar rules | 778.9 | 3.60 M |
| chunker | 628.7 | 3.64 M |
| speller | 596.2 | 8.90 M |
| everything else | 166.3 | 1.24 M |

| site | MiB | blocks |
|---|---:|---:|
| `Levenshtein::accept` row `Vec` (`levenshtein.rs:73`) | 327.1 | 4.89 M |
| `Vec<WordData>` allocation (tag lists) | 249.3 | 1.35 M |
| `Vec<Group>` (match graphs) | 249.2 | 0.69 M |
| `SmallVec<[Group; 8]>` spill | 160.8 | 0.33 M |
| `chunk::Sequence::clone` | 151.3 + 37.8 | 0.26 M + 0.26 M |
| chunker `beam_search` (`(&str, &str)` context) | 104.0 | |
| `LevenshteinState::clone` (two sites) and `accept` closure | 87.6 each | 1.33 M each |
| `MaxentChunker::context` | 73.0 | |
| `IncompleteToken` clone | 59.8 | 0.91 M |

The Callgrind allocator-call count (25.8 M) splits as 63.8% Rust (`__rdl_alloc`, 16.5 M), 3.6%
`realloc`, and **31.2% inside Oniguruma** (4.02 M `onig_new_match_param` plus 4.02 M `match_at`
mallocs), i.e. one malloc per regex test that reaches the backend.

## 3. Execution on moby150k

### Pipeline stages, wall time

`wmb-profile stages`, `NLPRULE_PARALLELISM=off`, median of 5 fresh processes (min–max). The three
tokenizer stages are timed through the public API on a separate pass; the suggest total is a
fresh full check, which repeats those stages internally and adds the rules, builtins and speller.

| stage | 6.6 ms | share of suggest | 5.2 ms | share of suggest |
|---|---:|---:|---:|---:|
| sentence split (`Tokenizer::sentencize` call) | 38 (38–39) | 1.3% | 36 (36–38) | 1.3% |
| tokenize + tag + chunk + multiword | 360 (356–368) | 11.9% | 344 (330–354) | 12.7% |
| disambiguation | 456 (455–469) | 15.1% | 299 (298–304) | 11.0% |
| suggest total | 3,021 (2,976–3,098) | 100% | 2,713 (2,689–2,726) | 100% |

Parallel check on this machine (`perf.sh`, 5 processes, median): 2.098 s (6.6), 1.817 s (5.2),
against the Phase 0 local baselines 2.709 s and 2.319 s (−22.6% and −21.6%; those were taken
under different machine load, so the comparison is indicative).

### Stage shares of the whole process

Inclusive share of the profile. Callgrind column: instructions, LT 6.6 (51.068 B total, load plus
one check, including process start). Sampling columns: samples, 12,970 (6.6) and 12,276 (5.2).
The sub-rows are exclusive of each other only where marked; stages under `apply_with_impl` overlap
(the regex atoms are inside `Composition::apply`).

| stage | 6.6 instructions | 6.6 time | 5.2 time |
|---|---:|---:|---:|
| load: `Tokenizer::new` | 2.58% | 2.42% | 2.53% |
| load: `Rules::new` | 0.86% | 0.66% | 0.43% |
| check: `Rules::suggest` | 96.36% | 96.47% | 96.56% |
| sentence split (`srx::split_ranges`) | 1.94% | 1.27% | 1.27% |
| tokenize (tag lookups, chunk, multiword) | 8.55% | 13.70% | 14.43% |
|   chunker | 8.14% | 12.47% | 12.88% |
|   multiword tagger | 0.08% | 0.12% | 0.13% |
|   tagger lookups | 0.29% | 0.36% | 0.51% |
| disambiguation, all rules | 15.77% | 14.29% | 10.65% |
|   matching (`DisambiguationRule::apply`) | 12.62% | 12.24% | 9.13% |
|   applying changes | 0.95% | 0.48% | 0.31% |
| builtin rules (`Builtin::check_text`) | 2.20% | 2.13% | 2.36% |
| per-sentence checks (`apply_with_impl`) | 67.81% | 64.83% | 67.46% |
|   speller (`Spell::suggest`) | 12.24% | 10.61% | 14.58% |
|   grammar rules (the rest) | 55.57% | 54.22% | 52.88% |

Splitting the grammar rules (sampling only, because Callgrind keeps no stacks and
`Composition::apply` is shared with disambiguation; inclusive share of all samples):

| grammar rule work | 6.6 | 5.2 |
|---|---:|---:|
| `Composition::apply`, all matching | 46.89% | 38.22% |
|   regex atoms (`Regex::is_match` and friends, including the Oniguruma calls) | 16.75% | 14.80% |
|   word-data (POS/lemma) matching | 12.27% | 9.19% |
|   regex first-use compile | 0.99% | 0.73% |
| `StartPositions` prefilter (index build, `may_contain`, `get`) | 2.06% | 1.96% |
| suggestion building (synthesizers, apostrophe matching) | 0.02% | 0.01% |
| engine iteration, filters, rule loop (remainder of grammar rules) | ≈ 5.2% | ≈ 12.7% |

Suggestion building is negligible because the check produces only 882 (6.6) and 978 (5.2)
suggestions; the cost is deciding that rules do **not** match. In Callgrind terms (6.6, self cost): `Atom::is_match` 16.0%,
`Composition::apply` 8.5%, `WordDataMatcher::is_match` 7.0%, `Matcher::is_match` 5.6%,
`match_at` (Oniguruma) 4.2%, `apply_recursive` 3.8%, `EngineMatches::next` 3.4%, and the speller's
`Levenshtein::accept` 3.9% plus FST stream 3.1%. `Composition::apply` is entered 84.5 M times
(about 88,000 per sentence; the count includes antipattern compositions and every disambiguation
pass, which the profile does not separate).

Regex atoms: `Regex::is_match` is called 9.18 M times (8.52 M from `Matcher::is_match`, 0.66 M
from `wrong_word_in_context`); 4.02 M of those reach Oniguruma, so the short-input cache and the
trivially short inputs resolve about 56% without the backend.

### Builtins

Sampling, share of all samples, one run per rule set (builtins are 2.13% in total on 6.6, 2.36% on 5.2):

| builtin | 6.6 | 5.2 |
|---|---:|---:|
| `wrong_word_in_context` | 1.77% | 1.88% |
| `simple_replace` | 0.28% | 0.37% |
| `word_coherency` | 0.09% | 0.08% |
| `word_repeat` | 0.06% | 0.09% |
| `unpaired` (brackets, quotes) | 0.05% | 0.07% |
| `uppercase_sentence_start` | not sampled | 0.05% |
| `a_vs_an` | 0.02% | 0.02% |
| `comma_parenthesis_whitespace` | not sampled | 0.02% |

(`wrong_word_in_context` is the only builtin above 0.3%; it calls the regex backend 0.66 M
times.)

### Speller

`Spell::suggest` runs once per sentence (963 calls on 6.6) and is 12.2% of instructions (6.6) and
14.6% of time (5.2). Within it, the Levenshtein automaton accounts for the cost: 3.9% self for
`accept`, 3.1% for the FST stream walk, and allocator traffic for the per-byte row (327 MiB in
4.89 M blocks, 596 MiB and 8.9 M blocks including the state clones). Attempts to remove the row
copies changed allocator calls (−12.8% in #67) but not instructions (+0.62%).

### Instructions versus time

The two profiles disagree on how heavy tokenization is: the chunker is 8.1% of instructions and
12.5–12.9% of samples. It does float math and hash lookups with poor locality, so it costs more
time per instruction than the rule matcher. For deciding what to optimize for wall time, the
sampling column is the better guide; for instruction-gated PRs (the keep rule), the Callgrind
column is. Disambiguation is 15.8% of instructions on 6.6 but 10.7% of time on 5.2 because the
5.2 disambiguation set is smaller (843 vs 1,015 rules).

### Noise

Three sampling runs per rule set, key shares of all samples (range across runs, percentage points):
chunker 12.47–12.59 (6.6), 12.88–13.09 (5.2); disambiguation 14.29–14.48 (6.6), 10.52–10.73
(5.2); speller 10.61–10.82 (6.6), 14.58–14.74 (5.2); `Composition::apply` 46.89–47.00 (6.6),
37.77–38.22 (5.2); regex atoms 16.41–16.75 (6.6), 13.80–14.80 (5.2). The widest spread is 1.0
points (5.2 regex atoms), the rest are within 0.3. Wall-time stage medians in the stage table
are within 1–3%, except one slow 6.6 `tokenizer deserialize` run (93.5 ms against a 62–63 ms
cluster).

## Top remaining opportunities, ranked by expected gain

Estimates only; none of these was implemented or measured. "Gate" is the keep rule from
`PERF.md` (≥1% instructions at ≤1 MiB RSS, or >1 MiB RSS at ≤+0.5% instructions).

| # | opportunity | evidence | expected gain | notes |
|---|---|---|---|---|
| 1 | Index POS-first rules (18% of compositions have a POS first atom and no literal key, from the census in `PERF.md`) the way `StartPositions` indexes literal-first ones, so a rule is skipped when its first POS tag is absent from the sentence | `Atom::is_match` 16.0% self, `Composition::apply` 8.5% self and `WordDataMatcher::is_match` 7.0% self are the shallow first-atom scan; word-data matching is 12.3% of time | 3–6% instructions | needs a per-sentence tag index (built once per sentence, like the text index); must keep exact passing-ID lists |
| 2 | Memoize speller suggestions per distinct misspelled word within a check | `Spell::suggest` 12.2% instructions / 14.6% time; 4.9 M row allocations; fiction repeats names and coinages | up to ~3–8% instructions, depending on repeats in the text (unmeasured) | #56 and #67 only moved allocator calls; this removes the work instead of shrinking it |
| 3 | Compile the 85 SRX sentence-split regexes lazily or share one automaton set | sentencizer: 8.3 MiB RSS and 10 ms for a 9 KB file; +2.3 MiB heap during the check | 3–6 MiB peak RSS, 5–10 ms load | passes the memory gate by itself; sentence boundaries must stay byte-identical (goldens) |
| 4 | Remove the per-call Oniguruma allocations (reuse a match param/region) | 31% of allocator calls (8.0 M); malloc/free ≈ 10% self instructions | 1–3% instructions | small and local to `utils/regex.rs` |
| 5 | Compact chunker model storage (pmap, tag dictionary, outcome `HashMap`s into sorted arrays or an FST) | chunker 7.9 MiB heap, 8.4 MiB RSS in-context; POS pmap 2.0 + tagdict 1.9 MiB | 3–4 MiB RSS | the chunker is 12.5% of time, so lookup speed must not regress |
| 6 | Drop `word_buf` + `word_offsets` by reading words from the word-store FST | 3.1 MiB (6.6) / 4.5 MiB (5.2) duplicated data in the tagger | 3–4 MiB RSS | affects tagger lookups in the hot tokenize path |
| 7 | Shrink the grammar matcher part pool and `Vec<Rule>` (4.0 MiB for the rule structs, 7.9 MiB pool with 4.8 MiB atoms) | dhat and in-context partition | 2–4 MiB RSS | format change; rules format version bump |
| 8 | Reduce chunker beam allocations (`Sequence` clones 151 + 38 MiB, `context` strings 73 MiB, `get_top_n` vectors) | chunker is 29% of allocation volume and 12.5% of time | 1–3% instructions, 3–5% time | #58 swapped the heaps for only −0.4%; the remaining cost is the per-token feature strings |
| 9 | Raise the `Group` inline capacity or pool `Vec<Group>` buffers | 1.0 M allocations (410 MiB) from `Vec<Group>` and `SmallVec` spills | 1–2% instructions | #55 already inlined eight groups; spills remain |
| 10 | Build the multiword automaton without the builder high-water | 6.6 multiword is 1.9 MiB heap but 8.1 MiB isolated RSS (5.7 in-context) | 2–4 MiB RSS (macOS-measured, Linux effect unknown) | the 6.6 phrase list is about 5× the size of the 5.2 one |
| 11 | Incremental disambiguation passes (re-check only rules that can match near a change) | disambiguation 15.8% of instructions (6.6); the loop restarts at the rule after every change | 2–5% instructions | complex; #68 already reuses the text index |

Items 3 and 4 are the cheapest to try. Item 1 has the largest instruction upside. Items 5–7 and 10
are the memory candidates that would clear the >1 MiB gate on their own; items 3 and 10 are also
the two structures whose RSS is far above their heap bytes (8.3 and 8.1 MiB RSS against 3.3 and
1.9 MiB of heap), which suggests transient build memory that the allocator keeps; that is an inference from the
numbers, not something these runs tested.
