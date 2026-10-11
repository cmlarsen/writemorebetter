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
