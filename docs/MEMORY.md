# Memory breakdown of the loaded English checker

Measured 2026-10-11 on `m2-tagger` (commit `a70c364`) on an Apple Silicon MacBook. This is a
measurement report only: nothing here changes runtime behavior, and every "idea for reducing it" is
unverified.

## How to reproduce

```sh
mkdir -p data storage
curl -sSL -o data/en.zip https://f000.backblazeb2.com/file/nlprule/en.zip
unzip -q data/en.zip -d data
RUST_LOG=WARN cargo run --release --all-features --bin compile -- \
    --build-dir data/en --tokenizer-out storage/en_tokenizer.bin --rules-out storage/en_rules.bin
cargo test --release --all-features --test memory_breakdown -- --ignored --nocapture
```

The test lives in `nlprule/tests/memory_breakdown.rs`; the `#[doc(hidden)]` probe it drives is
`nlprule::memory_probe`, behind the off-by-default `memory-probe` cargo feature. With the feature
off, neither module is compiled.

## Method

- **Fresh process per measurement.** The test re-executes its own test binary once per
  component and repetition. Each child reads its own RSS with
  `ps -o rss= -p <pid>` before and after deserializing exactly one component, so no earlier
  allocation can mask a later one (macOS RSS does not shrink on free). Three repetitions, median
  reported. Anything the child leaves resident after `ps` is not counted.
- **Serialized bytes** are the length of each component re-serialized with bincode by the probe,
  not hand-counted. For the top-level components these match the bytes in the shipped binaries.
- **Two isolation strategies, both reported:**
  - *isolated* — deserialize that one component from its own file. This is the literal "deserialize
    field X alone" method. It includes transient allocator high-water (e.g. the temporaries the
    chunker allocates while converting its model), so the isolated parts sum to **more** than the
    whole (71.9 MB vs 64.8 MB).
  - *in-context* — clone the full `Tokenizer`/`Rules` with one component blanked, deserialize that,
    and take `full − blanked`. These deltas **partition** the total (they sum to it exactly) and are
    the number to trust for "how much does this component retain".
- **Stability.** In the final runs the spread across the three repetitions is 0–2% for every
  component; nothing is near the 20% instability threshold.

## Totals vs. the 67 MB / 87 MB headline

`wmb-probe` (default features, i.e. the onig regex backend) reports 2 MB baseline, 67 MB after
`Tokenizer::new`, 88 MB after `Rules::new`. Our test binary's baseline is ~2.7 MB; the peaks below
are growth over that baseline.

| binary | serialized | RSS growth (`--all-features`) | RSS growth (regex-onig only) | total RSS (onig) |
|---|---:|---:|---:|---:|
| `en_tokenizer.bin` | 11.59 MB | 64.8 MB | 64.4 MB | ~67 MB |
| `en_rules.bin` | 9.12 MB | 30.1 MB | 22.8 MB | ~90 MB |

The tokenizer total matches the headline. The rules total is ~7 MB heavier under `--all-features`
than the headline; see "Why `--all-features` inflates the rules" below. With the default onig
backend our test says 64.4 + 22.8 ≈ 87–90 MB, matching `wmb-probe`'s 88 MB.

## Tokenizer components

Primary numbers are `--all-features`; the onig-only run is nearly identical except where noted.

| component | serialized | in-context MB | isolated MB | what dominates it | idea for reducing it (unverified) |
|---|---:|---:|---:|---|---|
| disambiguation rules | 0.81 MB | 3.6 | 4.4 | `Engine::Token` compositions (`Vec<Part>` of `Atom`s); all 843 rules are token engines, 0 text | box the parts / share identical compositions |
| chunker | 5.45 MB | 21.9 | 27.2 | `Model.outcomes` (`usize`), `pmap` (`HashMap`), `tagdict` (16.2k `HashMap<String,Vec<String>>`) | see chunker sub-breakdown below |
| multiword tagger | 0.05 MB | 16.1 | 17.0 | the Aho-Corasick dense DFA built from the phrase list | build it lazily, or use a sparse NFA / custom trie |
| sentencizer (`srx`) | 0.008 MB | 12.0 | 13.6 | 85 compiled `regex` regexes built during deserialization (`serde_regex`) | compile the SRX regexes lazily, or replace srx with a builtin segmenter |
| tagger | 4.72 MB | 11.2* | 9.8 | `tag_fst` (3.3 MB) + `word_buf` (3.1 MB) + `word_offsets` (1.3 MB) | mmap/borrow the FST bytes; drop `word_buf`+`word_offsets` by reading words from the FST |
| **total** | 11.05 MB | **64.8** | 71.9 | | |

\* the tagger row is the residual of the in-context partition; the isolated load is the truer number
for the tagger alone. The 1.4 MB gap is allocator interaction between components loaded together.

The three surprises: a 0.05 MB file costs 16 MB loaded, a 0.008 MB file costs 12 MB loaded, and the
chunker inflates 5.45 MB to ~22 MB.

### Tagger sub-fields (isolated loads)

| field | serialized | loaded MB | type |
|---|---:|---:|---|
| `tag_fst` | 3.32 MB | 5.3 | `fst::Map<Vec<u8>>` |
| `word_buf` | 3.09 MB | 3.1 | `String` of all 331,644 words |
| `word_store_fst` | 1.63 MB | 1.7 | `fst::Map<Vec<u8>>` |
| `word_offsets` | 1.33 MB | 1.4 | `Vec<u32>` (332k entries) |
| `tag_store` | 774 B | 0.2 | `BiMap<String, PosIdInt>`, 59 entries |

The tagger is now close to the binary size — the FST-direct-query fix worked. The only real slack is
`word_buf` + `word_offsets` (4.5 MB) duplicating data already in `word_store_fst`.

## Rules components

| component | serialized | in-context MB (`--all-features`) | in-context MB (onig) | isolated MB | what dominates it | idea for reducing it (unverified) |
|---|---:|---:|---:|---:|---|---|
| grammar rules | 7.20 MB | 27.9 | 20.6 | 28.3 | `Engine` matcher compositions (blanking the engine drops 20.9 → 6.5 MB on onig) | see "why all-features inflates the rules" |
| spell (variant none) | 1.49 MB | 1.9 | 1.8 | 2.1 | the spell FST/`multiwords`/`map` kept raw | nothing large at this size |
| spell, en_US selected | — | +18.7 on top of the load | +18.8 | — | `ingest_options` rebuilds a per-variant FST **and** copies every valid word into a `HashSet<String>` | query the FST subset for membership instead of materializing a `HashSet<String>` |
| **total** | 8.70 MB | **30.1** | **22.8** | 30.4 | | |

### Grammar-rule sub-fields (isolated, onig)

| field | loaded MB | note |
|---|---:|---|
| `engine` (matchers/atoms) | ~14.5 | blanked engine: `grammar_no_engine` 6.5 MB vs 20.9 MB |
| `examples` | ~2.2 | test strings |
| `suggesters` + `message` | ~0.8 | `Synthesizer`s |
| everything else | ~0.4 | ids, names, urls, categories |

### Why `--all-features` inflates the rules by ~7 MB

`nlprule::utils::regex::Regex` is `{ regex_str: String, regex: OnceCell<regex_impl::Regex> }`, and
`once_cell::sync::OnceCell` stores its value **inline**. With only onig the cached type is a pointer
(`size_of::<Regex>() = 48 B`); with `regex-fancy` also enabled the cache must be large enough for a
whole `fancy_regex::Regex` plus an onig regex (`size_of::<Regex>() = 152 B`). Every one of the
~70k pattern `Regex`es in the grammar rules pays that +104 B even though nothing is compiled at load.
Boxing the cache (`OnceCell<Box<regex_impl::Regex>>`) would make the slot one pointer word and
recover the ~7 MB. This does not affect the tokenizer because its 843 disambiguation rules are all
token engines with no `Regex`.

## Chunker sub-breakdown (in-context, `--all-features`)

| sub-model | loaded MB | parameters (disk→loaded) | outcomes (disk→loaded) | pmap | tagdict |
|---|---:|---:|---:|---:|---:|
| token model | 2.9 | 80,798 bf16 → 161,596 f32 | 40,399 u8 → 323,192 usize | 35,505 entries | — |
| pos model | **14.2** | 1,286,938 → 2,573,876 | 643,469 → 5,147,752 | 103,444 entries (931 KB disk) | 16,204 entries |
| chunk model | 6.3 | 547,360 → 1,094,720 | 273,680 → 2,189,440 | 107,838 entries (971 KB disk) | — |

The pos model's fields, measured in-context by blanking them one at a time:

| field | loaded MB | why |
|---|---:|---|
| `outcomes` | 5.0 | `Vec<usize>` (8 B) where disk stores `u8` (1 B): 8× |
| `tagdict` | 3.8 | 16,204 entries of `HashMap<String, Vec<String>>` |
| `pmap` | 3.2 | disk is `Vec<(u64,u8)>` (9 B/entry); loaded is `HashMap<u64,(usize,usize)>` (~30 B/entry + control bytes) |
| `parameters` | 2.5 | `Vec<f32>` (4 B) where disk stores `bf16` (2 B): 2× |

The chunk model shows the same pattern (`outcomes` 2.1, `pmap` 3.2, `parameters` 1.0). The model
weights are stored compactly (bf16 / u8 / sorted `Vec`) and then inflated at load into wider types
and hash maps. Ideas, all unverified: keep `parameters` as bf16, store `outcomes` as `u16`/`u8` with
an offset table (or fold them into `parameters`), and replace `pmap` with a sorted `Vec<(u64,(u32,u8))>`
plus binary search, or an open-addressed map of `u64 → (u32,u8)`.

## Disambiguation-rule sub-breakdown (isolated, `--all-features`)

843 rules, 0.81 MB serialized, 4.4 MB isolated / 3.6 MB in-context.

| part (blanked out) | cost MB | note |
|---|---:|---|
| `engine` | ~2.6 | token compositions; text engines cost ~0 |
| `examples` | ~0.7 | embedded test strings |
| `disambiguations` | ~0.1 | actions |
| `filter`, `unification` | ~0 | rarely present |

## Isolated loads — full raw table (`--all-features`, median of 3)

| component | serialized bytes | RSS growth MB | spread |
|---|---:|---:|---:|
| tokenizer_full | 11,585,199 | 64.80 | 0% |
| rules_full | 9,118,565 | 30.06 | 0% |
| tagger | 4,951,115 | 9.78 | 0% |
| tagger_word_store_fst | 1,626,571 | 1.72 | 0% |
| tagger_tag_fst | 3,323,703 | 5.34 | 0% |
| tagger_tag_store | 774 | 0.24 | 0% |
| tagger_word_buf | 3,094,812 | 3.13 | 0% |
| tagger_word_offsets | 1,326,588 | 1.42 | 0% |
| disambiguation_rules | 850,401 | 4.36 | 1% |
| disambig_no_examples | 673,248 | 3.70 | 0% |
| disambig_no_filter | 849,901 | 4.39 | 1% |
| disambig_no_unification | 850,074 | 4.39 | 1% |
| disambig_no_disambiguations | 804,145 | 4.25 | 1% |
| disambig_no_engine | 323,866 | 1.81 | 1% |
| disambig_no_text_engine | 850,401 | 4.39 | 1% |
| disambig_no_token_engine | 323,866 | 1.80 | 1% |
| chunker | 5,720,223 | 27.19 | 0% |
| chunker_token_model | 440,792 | 4.30 | 0% |
| chunker_pos_model | 3,487,528 | 19.41 | 0% |
| chunker_pos_tagdict | 625,620 | 3.08 | 0% |
| chunker_chunk_model | 1,791,902 | 11.48 | 0% |
| chunker_model_parameters | 1,094,728 | 1.19 | 0% |
| chunker_model_outcomes | 2,189,448 | 4.28 | 0% |
| chunker_model_pmap | 2,588,120 | 4.91 | 0% |
| chunker_model_outcome_labels | 296 | 0.17 | 0% |
| chunker_token_model_parameters | 161,604 | 0.30 | 0% |
| chunker_token_model_pmap | 852,128 | 1.75 | 0% |
| chunker_pos_model_parameters | 2,573,884 | 4.62 | 0% |
| chunker_pos_model_pmap | 2,482,664 | 4.91 | 0% |
| multiword_tagger | 54,723 | 17.00 | 0% |
| sentencizer | 8,446 | 13.61 | 1% |
| grammar_rules | 7,555,031 | 28.28 | 0% |
| grammar_no_examples | 6,587,520 | 25.92 | 0% |
| grammar_no_suggesters | 7,370,033 | 27.06 | 0% |
| grammar_no_engine | 2,307,798 | 7.24 | 2% |
| spell | 1,563,534 | 2.13 | 1% |
| spell_en_us | 1,563,534 | 20.83 (raw 2.1 + en_US 18.7) | 0% |

The `*_no_*` rows are the partition variants: they show the loaded cost of the named part as a
difference against the full load.

## Structural facts (not measured, printed by `memory_probe::describe`)

| fact | value |
|---|---:|
| tagger words | 331,644 |
| tagger `word_buf` bytes | 3,094,804 |
| tagger `tag_store` entries | 59 |
| disambiguation rules (text-engine rules) | 843 (0) |
| chunker pos `tagdict` entries | 16,204 |
| grammar rules | 3,749 |
| spell FST bytes / variants / multiwords / map | 1,425,695 / 3 / 796 / 177 |
| sentencizer rules | 85 |
| `size_of::<nlprule Regex>()` onig / onig+fancy | 48 / 152 |

## Where the memory is, in one paragraph

The English tokenizer's 65 MB is spread more evenly than expected: the chunker (~22 MB), the
multiword tagger (~16 MB), the sentencizer (~12 MB), and the tagger (~11 MB), with 3–4 MB of
disambiguation rules. The chunker is large because its maxent weights are stored compactly
(bf16 parameters, u8 outcomes, a sorted `Vec` for `pmap`) and then inflated at load into `f32`,
`usize`, and `HashMap` — the outcomes alone become 8× larger. The multiword tagger and the
sentencizer are small data files that build large in-memory structures at load (an Aho-Corasick
dense DFA and 85 compiled regexes, respectively). The rules' 23–30 MB is almost entirely the
grammar-rule engines; the ~7 MB swing between `--all-features` and the default onig build is the
inline `OnceCell` slot in every lazily-cached `Regex` growing to hold a `fancy_regex::Regex`.

---

# M2H: cutting the grammar-rule memory

Measured 2026-10-11 on the same Apple Silicon MacBook, `--all-features` (both regex backends) and
`PROFILE=fiction SPELL=en_US` for the peaks. This section supersedes the rule numbers above for the
current tree; the tokenizer numbers above still hold.

## What changed

The loaded rules were dominated by the per-rule matcher engine, not the metadata. The changes
(`perf(rules)` commits) shrink the engine without changing matching:

- `Matcher.matcher` was `Either<Either<String, GraphId>, Regex>` (48 B); it is now a compact
  `MatcherKind { Text(Box<str>), Graph(GraphId), Regex(Box<Regex>) }` (32 B). `Atom` 128 → 80 B and
  `Part` 152 → 96 B, so the ~139k pattern atoms in the LT 6.6 rules each cost 56 B less inline.
- `TextMatcher.set` was `HashSet<WordIdInt>` (48 B inline + a table per set); it is a sorted
  `Box<[WordIdInt]>` with binary-search membership (16 B + one slice). 276k ids in 30k sets.
- `Composition.id_to_idx` was a `HashMap<GraphId, usize>` (~30 B/entry, one table per composition);
  it is a `Vec<u32>` (`u32::MAX` = absent). 179k entries in 44k compositions.
- `Quantifier` is `u32` rather than `usize`.
- `Regex` pattern strings are interned in a process-wide pool: 76% of the 5.56 MB of pattern bytes
  are duplicates, so interning cuts the pattern heap to the 1.33 MB of distinct patterns.
- `Rule::examples` (22k examples) is kept as a lazily parsed bincode blob; the running engine never
  materializes the `Example` structs, and the rule-test binaries parse it on demand.
- `RULES_FORMAT_VERSION` → 4, `TOKENIZER_FORMAT_VERSION` → 2 (the engine types are shared by the
  tokenizer's disambiguation rules).

## Before / after

"Before" is `origin/main` at the merge base (`11b63ae`, after the M4I merge); the pre-merge
`352799c` numbers are shown too because the field breakdown below was first measured there.

| binary | rules_full before (11b63ae) | rules_full after | peak before | peak after |
|---|---:|---:|---:|---:|
| LT 6.6 | 81.5 MB | **58.2 MB** | 160.7 MB | **135.1 MB** |
| LT 5.2 | 47.4 MB | **37.1 MB** | 109.5 MB | **96.3 MB** |

| binary | rules_full (352799c) | rules_full after | peak (352799c) | peak after |
|---|---:|---:|---:|---:|
| LT 6.6 | 68.2 MB | **58.2 MB** | 144.1 MB | **135.1 MB** |
| LT 5.2 | 34.7 MB | **37.1 MB*** | 97.1 MB | **96.3 MB** |

\* the 5.2 rules_full grew slightly because the merged tree compiles ~48 more rules than `352799c`
(4,334 vs 4,286); against its own base (`11b63ae`, 47.4 MB) it drops 10.3 MB.

Load time for the rules grew a little (6.6 47.5 → 63.9 ms, 5.2 26.2 → 35.9 ms) from interning the
pattern strings; check time is unchanged within noise (6.6 3.95 → 4.57 s, 5.2 3.27 → 3.41 s on a
loaded machine). Memory is the point; the timings are reported for completeness.

## Part 1: the per-field breakdown

The memory probe now blanks one serialized part of `Rule`/`Engine` at a time inside the full
`Rules` (`rules_no_gr_*`) and reports `in-context = rules_full − blanked`, plus the serialized size
of the blanked part. LT 6.6, in-context MB:

| field | serialized (before) | in-context (before) | serialized (after) | in-context (after) |
|---|---:|---:|---:|---:|
| engine (whole) | 20.39 | 49.77 | 16.89 | 30.23 |
| examples | 2.71 | 6.59 | 2.78 | 2.70 |
| message synthesizer | 0.56 | 1.67 | 0.57 | 1.44 |
| suggesters | 0.32 | 1.80 | 0.33 | 1.42 |
| strings (name/url/cat) | 0.42 | 0.45 | 0.43 | 0.34 |
| id (rule index) | 0.13 | 0.16 | 0.13 | 0.17 |
| unification / filter | 0.02 | ~0 | 0.02 | ~0 |
| — antipatterns | 15.58 | 39.36 | 12.60 | 23.14 |
| — composition parts | 16.45 | 39.72 | 15.09 | 25.17 |
| — composition id_to_idx | 2.86 | 5.30 | 0.72 | 1.02 |
| — composition can_stop_mask | 0.14 | 0.66 | 0.14 | 0.52 |
| — part atoms | 13.25 | 17.69 | 12.99 | 11.98 |
| — all regex strings | 5.58 | 6.20 | 5.59 | 2.17 |
| — matcher strings | 0.33 | 1.14 | 0.33 | 1.25 |
| — text-matcher sets | 1.35 | 2.62 | 1.35 | 1.30 |

The "before" column is `352799c` (the same probe); the nested rows overlap, and the ~±1 MB
residual is RSS noise, so treat them as attribution, not a partition. The two facts the table
establishes: (1) the antipattern compositions are ~80% of the engine, and (2) most of their cost is
the inline `Vec<Part>` and its atom heap, which is why the struct shrinks above pay off.

Structural counts for the LT 6.6 rules (printed by `memory_probe::describe`): 6,031 rules, 44,196
token compositions, 139,133 parts (117,194 of them antipattern), 179,020 `id_to_idx` entries,
40,419 regexes, 30,116 text-matcher sets holding 276,524 ids, 22,474 examples. `size_of::<Part>()`
152 → 96 B, `Atom` 128 → 80, `TextMatcher` 96 → 48, `Matcher` 48 → 32.

## Identity and conformance

`SHOW=1 PROFILE=default SPELL=en_US` output over `bench/public/story1..5` and Moby-Dick's first
150 KB is byte-identical before/after (all 12 files, 5.2 and 6.6). `test --list-passing` and
`test_disambiguation` are unchanged and match the CI floors exactly: 6.6 5,959/6,031 grammar and
1,011 disambiguation, 5.2 4,319/4,334 and 843. Public recall meets the floors (6.6 55/60, 5.2
54/60). `cargo fmt --check`, clippy, and `cargo test --workspace --exclude nlprule-python
--all-features --release` pass.

## Not reached, and what is left

The target was 6.6 `rules_full` < 35 MB and peak < 100 MB; the result is 58.2 MB / 135.1 MB. The
remaining 6.6 engine (~30 MB in-context) is now mostly the antipattern `Vec<Part>` (139k parts at
96 B ≈ 13 MB inline) and the atom heap (~12 MB: the interned regex strings 2.2, matcher strings
1.3, text-matcher sets 1.3, and the rest the boxed `Regex` structs and `PosMatcher` masks). Getting
under 35 MB needs a different representation, not more field shrinking: either boxing every large
`Atom` payload so `Part` is one word (worth ~4–5 MB after allocator overhead), or flattening the
matchers into a per-`Rules` interned table addressed by `u32` so the inline `Matcher`/`TextMatcher`
and the 5.6 MB of duplicated pattern bytes disappear (worth ~10 MB). Both are larger, riskier
changes than the ones above and were left out of this pass.
