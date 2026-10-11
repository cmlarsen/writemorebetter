# Findings: an embeddable LanguageTool-grade checker for English fiction

Research done 2026-10-11 while evaluating grammar checking for Subplot (a writing app for fiction authors).
Every number below was measured on a MacBook (Apple Silicon) unless marked otherwise; the scripts that
produced them live in `bench/tools/`.

## The question

LanguageTool (LT) is the most thorough open rule-based grammar checker, but it is a Java server. We want
its checking quality inside an editor — browser (WASM), Electron, and native iOS/macOS — instantly, offline,
and without a JVM. Can we port it, and should we build on an existing Rust port (nlprule) or on Harper?

## Conclusions

1. **On unedited fiction drafts, LT's rules find errors that nothing else does.** Against 49 hand-verified
   errors, stock LT caught 48, Harper 34. The gap is grammar and wrong-word errors (LT 9/10, Harper 1/10)
   and spelling-suggestion quality. On already-edited prose the tools looked equivalent; drafts are where
   the difference shows.
2. **Porting LT's rules into Harper is impractical.** Harper's Weir rule language can express ~10% of LT's
   5,729 active English rules today. The rules that caught real errors need 5–8 matcher features Weir lacks
   (regex, lemmas, Penn tags, skips, chunks, inflection synthesis). Getting there means rebuilding LT's
   matcher, tagger, and chunker inside Harper, which is what nlprule already is.
3. **Build on nlprule.** Upstream nlprule (Rust, MIT/Apache, last release 2021, LT 5.2) has the right
   architecture but is stale: no speller, none of LT's Java-coded rules, older rules. A recent third-party
   fork shows that current LT 6.6 rules plus a speller reach 38/49 in Rust, but the fork is not usable as a
   base (crashes and broken rules, see below).
4. **Memory is fixable, and has been fixed.** nlprule's English footprint (~145 MB) was mostly load-time
   inflation: the tagger FST into hash maps, the chunker model into wider types, the multiword list into a
   dense DFA, SRX regexes into one-pass DFAs. Removing each (M2, 2026-10-11) took the loaded tokenizer from
   124 MB to 34 MB and peak RSS checking 150 KB with the speller on from 168 MB to 79 MB, with identical
   output. See `docs/MEMORY.md` for the per-component breakdown.
5. **N-gram confusions are out of scope for on-device.** Three of LT's catches (heal/heel, peaked/peeked,
   he/the) come from rules backed by LT's ~8 GB n-gram index. An embeddable checker should leave those to a
   separate layer (an LLM pass, or a small model later).

## Candidates

| Candidate | What it is | License | State (Oct 2026) |
|---|---|---|---|
| LanguageTool 6.6 / 6.9 | Java server; XML pattern rules + Java rules + Morfologik speller + n-gram rules | LGPL-2.1 | Active |
| Harper 2.12 | Rust/WASM checker by Automattic; own rules + Weir DSL | Apache-2.0 | Active, ~16k stars |
| nlprule 0.6.4 | Rust port of LT's XML rules, tagger, disambiguator, chunker | MIT/Apache code, LGPL data binaries | Unreleased since Apr 2021; unmerged spellcheck (#51) and modularize (#72) PRs |
| nlprule fork (`alexvinasdev/nlprule`) | Claims all 35 LT 6.5/6.6 languages, speller, Java-rule ports, LT-compatible HTTP server | Same as nlprule | PRs #91/#92 opened Sep–Oct 2026; unreviewed; ~655k lines in ~110 commits from a new account |

## Method

**Corpus** (`bench/corpus/`, 16,576 words, contemporary first-person fiction by one author): one edited
chapter, three unedited drafts (one partly dictated), and one edited short piece. The corpus is private and
git-ignored; see `bench/README.md`. Every example quoted below is invented to illustrate the same error and
rule — none is taken from the corpus.

**Truth set** (`bench/corpus/truth.json`): 49 errors verified by reading each flagged line, in five kinds —
28 misspellings, 7 accidental repeated words, 10 grammar/wrong-word errors, 3 n-gram confusions, 1 comma
spacing. A flag counts as a catch when a tool flags that line on an overlapping span with a
non-whitespace rule. Recall only; precision was judged by reading the flags (below).

**Runs:** LT via the public API (v6.9-SNAPSHOT, en-US; free tier, so no premium rules). Harper via
`harper-cli lint --format json` (v2.12.0, US dialect, all rules). nlprule 0.6.4 via the published English
binaries and a small Rust probe (`bench/tools/probe`). The fork built from source at commit `19f80ac`,
with an English build dir generated from the LanguageTool 6.6 distribution by the fork's own
`make_build_dirs.py` plus the upstream `chunker.json`.

## Results

### Recall on 49 verified errors

| Tool | Spelling (28) | Repeated words (7) | Grammar (10) | N-gram (3) | Comma spacing (1) | **Total** |
|---|---|---|---|---|---|---|
| LanguageTool 6.9 (API) | 28 | 7 | 9 | 3 | 1 | **48** |
| nlprule fork (LT 6.6) | 27 | 2 | 8 | 0 | 1 | **38** |
| Harper 2.12 | 26 | 6 | 1 | 0 | 1 | **34** |
| nlprule 0.6.4 (LT 5.2) | 0 | 0 | 5 | 0 | 0 | **5** |
| writemorebetter M0 (LT 5.2 + speller, en_US) | 28 | 0 | 5 | 0 | 0 | **33** |
| writemorebetter + M5 builtins (2026-10-11) | 28 | 0 | 5 | 6 | 1 | **40** |

Kinds of grammar errors only LT-family tools caught, with the LT rule that fired (illustrative sentences):

| Text | Rule |
|---|---|
| "she leafed trough the ledger" | TROUGH_THROUGH |
| "This is would explain the noise" | THIS_IS_HAVE |
| "On the sill a was a cold cup of tea" | A_WAS |
| "stacked next to a boxes and crates" | A_NNS_AND |
| "she tucked a her hair back" | A_MY / collocation |
| "Digging though the drawer" | THOUGH_THROUGH |
| "The we drove north at dawn" | DT_PRP |
| "I'd like take a walk" | MISSING_TO_BEFORE_A_VERB |
| "Marta is seems tired" | IS_VBZ |

Kinds of errors no tool caught (all need meaning, not patterns), illustrated: "the dog chased it's tail"
(its), "leaned at sharp angle" (missing article), "I treated it was a joke" (as), "he saddled up to the bar"
(sidled), a paragraph ending in `she said,`.

### Noise

Total flags across the corpus, before any rule tuning: LT 279, Harper 569, nlprule 0.6.4 682, fork 1,150.

- **LT**: mostly style rules that misfire on fiction voice — sentence-initial "I" repetition, verbless dialogue
  questions ("You okay?" → "Are you okay?"), "gotta"/"gonna" in dialogue, comma before "and". All are rule toggles.
- **Harper**: 197 of 290 flags on the edited chapter were whitespace (double spaces after periods, line-end
  spaces). Beyond that: a smaller dictionary (flags "distilled", "archaea", "nitrile", character names
  LT accepts) and grammar rules that misfire — a questioning "It's raining?" → "Its", "they were broke" →
  "broken", "She dove" → "doves", "cumin" → "cum in", "hard hat" → "hardhat", "adits" → "ad its". It suggested
  "reg on" for "regon" and "sat ion" for "sation" where LT suggested "region" and "station", and missed
  "patenice" and "figureo" entirely.
- **nlprule 0.6.4**: 280 of 308 flags on the edited chapter were smart-quote suggestions.
- **Fork**: two broken builtins — `COMMA_PARENTHESIS_WHITESPACE` flags every ordinary comma with an identical
  replacement (496 flags), `EN_UNPAIRED_QUOTES` flags every quotation mark (296).

### Speed and memory

| | Load | 5k-word chapter | 150 KB of Moby-Dick | Peak RSS | English data on disk |
|---|---|---|---|---|---|
| nlprule 0.6.4 | 0.1 s | 0.63 s | 2.45 s | 159 MB | 11.6 MB tokenizer + 7.6 MB rules |
| Harper CLI 2.12 | 0.5 s | 1.37 s | 3.8 s | 400 MB | in binary (58 MB executable) |
| nlprule fork | 0.2 s | 4.84 s | — | 218 MB idle, 350 MB peak | 10.3 MB tokenizer + 38 MB rules |
| writemorebetter M0, speller off | 0.1 s | 0.66 s | 2.73 s | 157 MB | 11.6 MB tokenizer + 9.1 MB rules |
| writemorebetter M0, en_US speller | 0.2 s | 0.71 s | 3.19 s | 171 MB | same |
| writemorebetter after M2, en_US speller | 0.1 s | — | 3.07 s | 79 MB (34 MB after tokenizer load) | 11.6 MB tokenizer + 9.2 MB rules |

The M2 row was measured with four cargo builds running alongside, so its time is an upper bound; memory is
unaffected by load. The writemorebetter M0 rows (measured 2026-10-11) are upstream nlprule `main` plus the merged spellcheck PR,
compiled from upstream's LT 5.2 build dir. Turning the speller on costs 47 ms and ~14 MB, because selecting a
variant copies the speller FST's words into a hash set; the M2 fix for the tagger applies here too. The speller
added 126 flags across the corpus: the 28 verified misspellings, the rest mostly names and coined words.

The fork's English compile took 4.5 minutes and 815 MB. Its 5× larger rules binary comes from expanding
`<phrases>` by cloning rules. The Harper number is the CLI; its WASM build was not measured.

## nlprule's memory, explained

The English tokenizer binary stores the tagger as two FSTs (compact because they share prefixes and
suffixes). `Tagger::from(TaggerFields)` (`nlprule/src/tokenizer/tag.rs`) streams every entry out at load and
rebuilds them as a `BiMap<String, WordId>` (two hash maps, an owned `String` per word form) plus a
`Vec<(lemma, tag)>` per word. Several hundred thousand entries of String/Vec/hash-bucket overhead turn
11.6 MB into ~110 MB. Rules add ~19 MB (compiled Oniguruma regexes); checking adds ~17 MB.

Fix: query the FSTs directly (`fst::Map::get` for word → id, a prefix range scan for its tags), optionally
behind a small cache of frequent words. Lookups cost microseconds instead of a hash probe; memory drops to
about the file size, which can also be memory-mapped or held as one WASM buffer.

## The fork, audited

The fork's XML compiler work is valuable: full `<phrases>`/`<includephrases>` expansion, Java→Oniguruma regex
translation fixes (inline flags, surrogates, possessives, lookbehind hoisting), and a build pipeline
(`build/make_build_dirs.py`) that generates build dirs from an LT desktop distribution. With LT 6.6 rules it
caught 8/10 grammar errors (upstream: 5) and 27/28 misspellings (upstream: 0).

It is not usable as a base:

- **Panics on ordinary fiction text.** `builtins.rs:2234` slices at byte offsets inside multi-byte
  characters; a curly `”` and a U+2028 each killed the process (4 of ~650 paragraphs).
- **Two broken builtins** producing ~800 false flags (above).
- **Word-repeat rule misses 5 of 7** ("into into", "little little", "the the", "here here", "once once").
- **Speller tokenization** breaks curly-apostrophe contractions ("weren", "didn", "doesn") and em-dash joins
  ("door—then", "quiet—too") into misspellings; flags "Dr".
- **Build hygiene:** compiles only with the `zh` and `ja` features enabled; the compiled binaries and the
  "companion repo" of build inputs are not published.
- **Performance:** 8× slower than upstream on the same chapter; 2× the memory.

Its claimed "100% rule-ID parity" was likely measured on ASCII test sentences; it does not hold on prose
with curly quotes and em dashes.

## Why not port LT's rules into Harper

`bench/tools/weir_coverage.py` classifies every active rule in LT's English `grammar.xml` + `style.xml` by
the matcher features it uses:

| Weir supports… | LT English rules expressible |
|---|---|
| as of Harper 2.12 (words, sequences, alternatives, UPOS, exceptions, 1-token wildcards, fixed replacements) | 554 (10%) |
| + case-sensitivity, token regexes | 916 (16%) |
| + backreferences, lemmas | 1,521 (27%) |
| + fine-grained Penn POS tags | 2,016 (35%) |
| + token skips | 3,208 (56%) |
| + inflection synthesis in suggestions | 3,995 (70%) |
| + LT chunk tags | 5,052 (88%) |

Of the rules that fired on the drafts, only A_WAS fits Weir (after adding case-sensitivity). Two further
problems: LT's exceptions and antipatterns are tuned to LT's own tagger and disambiguator, so the rules would
misfire on Harper's tagger; and converted rules stay LGPL, which Harper (Apache-2.0) cannot take upstream.

## Licensing

nlprule's model works: engine code MIT/Apache-2.0, compiled rule and dictionary binaries LGPL-2.1 (derived
from LT's resources), with the engine linking them per LGPL §6(a). Per-language dictionaries carry their own
licenses and need checking before bundling. "LanguageTool" is a trademark of LanguageTooler GmbH; the project
must not use the name. LT's premium rules and AI features are closed and out of reach.

## Plan

English first; other languages later as data plus per-language builtins behind a per-language interface.

1. **Base:** fork upstream nlprule `main`; merge the spellcheck PR (#51). Skip the unfinished modularize
   rewrite (#72) for now.
2. **Current rules:** cherry-pick the fork's compiler and regex-translation fixes and its build pipeline,
   re-derived with tests, not merged wholesale. Target the latest LT English release.
3. **Builtins:** port LT's Java-coded English rules (word repeat, comma/parenthesis whitespace, unpaired
   quotes, a/an, uppercase sentence start, the SimpleReplace/coherency/wrong-word-in-context tables),
   UTF-8-safe and tested on prose with curly quotes, em dashes, and U+2028.
4. **Memory:** query the tagger and speller FSTs directly; measure against the 145 MB baseline.
5. **Targets:** WASM (`fancy-regex` backend; measure the pass-rate cost against Oniguruma), native, and Swift
   via uniffi.
6. **Fiction profile:** a default rule set that turns off sentence-start repetition, smart-quote and dash
   typography, dialogue informality ("gonna", "You okay?"), and comma-before-"and" style rules.
7. **Gate every step** on LT's own embedded rule examples (conformance) and on this corpus's recall table
   (`bench/tools/recall.py`).

## Status (2026-10-11, end of the first overnight run)

Measured on `main` with both build dirs CI uses (LT 5.2 from upstream's `en.zip`; LT 6.6 generated from
the desktop release), on an idle machine. Conformance pass rate = rules passing their embedded examples /
`<rule>` elements.

| | nlprule 0.6.4 | writemorebetter, LT 5.2 rules | writemorebetter, LT 6.6 rules |
|---|---|---|---|
| Grammar pass rate | 3,749 / 4,366 = 85.9% | 4,320 / 4,366 = 98.9% | 5,977 / 6,098 = 98.0% |
| Disambiguation pass rate | 843 / 855 = 98.6% | 843 / 855 = 98.6% | 1,012 / 1,060 = 95.5% |
| Private corpus recall (49) | 5 | 42 | **45** (LT 6.9 API: 48) |
| Private corpus flags, fiction profile | 682 (no profile) | 186 | **180** (LT 6.9 API: 279) |
| Public corpus recall (60) | 9 | 54 | 55 (LT 6.9 API: 56) |
| Peak RSS, 150 KB of Moby-Dick, speller on | 159 MB | 78 MB | 100 MB |
| Check time, same text (same run, machine under load) | 2.21 s | 2.16 s | 2.63 s |
| Data on disk (tokenizer + rules) | 11.6 + 7.6 MB | 14.6 + 7.9 MB | 11.9 + 12.6 MB |

On the private drafts, LT 6.6 rules miss only the three n-gram confusions (out of scope on-device) and one
grammar error. What closed the gap: the speller (M0); inflection synthesis for `<match postag_replace>`
(M4C); LT 6.6 rule structure (M4D); no-suggestion rules and typed examples (M4F); LT 6.6's contraction
tokenization (M4H); rule filters (M4G); determiners, `include_skipped`, two-digit backreferences (M4I);
chunker and suggestion fixes (M4J); the twelve Java builtins (M5).

Memory: M2 removed load-time inflation from the tagger, chunker, multiword tagger, sentencizer and
speller, and later interned rule-engine parts (139,841 instances, 15,195 distinct). A compile-time
required-literal prefilter and per-text sentence batching brought check time back to 0.6.4's despite the
larger rule set and the speller (`docs/PERF.md`).

## Builtins on fiction (M5)

Ported so far: word repeat, comma/parenthesis whitespace, unpaired brackets and quotes, a/an, uppercase
sentence start. On the private drafts they add 6 of 7 repeated words and the comma-spacing error. Their other
flags were: deliberate repetition in fiction voice (intensifiers, a knock-knock joke, a dialogue stutter: 4
flags, an M7 profile question), quotes opened straight and closed curly (4, real inconsistencies), and a
space before a comma the truth set does not list (fitz3:95; candidate addition). Uppercase sentence start
raised nothing on the drafts.

## Open questions

- Whether `fancy-regex` handles enough of LT's translated regexes for the WASM build.
- Whether the corpus can be published (it is unpublished fiction), or whether a public stand-in corpus with
  planted errors is needed for CI.
- What replaces the n-gram confusion rules on-device, if anything.
- Project name: writemorebetter (not LanguageTool-derived; free on crates.io, npm, and GitHub as of 2026-10-11).
