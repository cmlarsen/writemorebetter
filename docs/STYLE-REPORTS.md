# Prose style reports

This is the design reference for writemorebetter's prose style reports: sentence structure, sentence
length, echoes, overused words, clichés, dialogue tags, and the rest. Read it before working on any of
them. Task-level specs live in the issues. Tracking issue: #241.

## What they are

Reports that help a fiction writer see patterns in their own prose. Two examples: "72% of your sentences
start with a subject, against 61% in published fiction", and "*glanced* appears 48 times". Subplot is the
first consumer.

- **Diagnose, never rewrite.** A report shows spans, counts and comparisons. It never generates
  replacement prose. That is a Subplot product rule, so there is no "rephrase" or thesaurus feature.
- **Procedural.** No LLM and no network at runtime. Everything runs on device: WASM, native, and Swift.
- **Whole-document.** Reports run over a document or chapter on idle or save, never on each keystroke.
  Percentages are suppressed below a minimum sample (default 200 sentences), where noise is larger than
  the differences.

## Where the code lives

```
nlprule/   the LanguageTool port. Unchanged in scope.
           + analyze(text): sentences and tokens with offsets, lemmas, POS tags, chunks (#242)
style/     the reports: crate writemorebetter-style, MIT OR Apache-2.0 (#243)
  src/document.rs   paragraphs, sentences, tokens, quoted-dialogue spans and speech flags, lemma index
  src/reports/      one module per report, plus the combined run() / run_json() in mod.rs
  data/             phrase lists and word lists (each credited in NOTICE)
    baselines/      default baselines (source recorded inside each file)
swift/, wasm/       one binding each, exposing check() + analyze() + analyzeDocument() + styleReports()
```

- **Not in `nlprule`.** `nlprule` measures itself by LanguageTool conformance. These reports are not
  LanguageTool features, and they return statistics as well as spans.
- **Not in a separate repository.** An app should link one Rust library per platform. A second one
  carries its own copy of Rust std and of the tokenizer data, and a second WASM module would load the
  multi-megabyte tokenizer twice. (#239 hit duplicate symbols between two static libraries; 0.8.0 fixed
  that with a dynamic framework, so the reason now is size and memory, not link failure.)
- **No duplicates of LanguageTool rules.** Some LanguageTool rules overlap the reports: EN_REPEATEDWORDS,
  TOO_OFTEN_USED_* (#234, #235), PARAGRAPH_REPEAT_BEGINNING_RULE (#206), READABILITY_RULE_* (#207),
  EN_CONSISTENT_APOS (#228), TOO_LONG_SENTENCE and ENGLISH_WORD_REPEAT_BEGINNING_RULE (#244), and
  LanguageTool's redundancy and wordiness style rules (#246). Their ports stay in `nlprule` for conformance. A report that overlaps one
  says how it differs.
- **One whole-document pass.** #243's document pass is also the "document statistics" call #234 needs.
  Design it once.
- **Inputs.** Every report takes the document (built once with an optional skip-list of names,
  characters and places, that the caller supplies), its options and, where it has one, its baseline
  as a separate argument. Skipped names are left out of counts and denominators, not filtered from the
  output afterwards.

## Reports

| Report | Issue | Needs | Baseline |
|---|---|---|---|
| Sentence length (distribution, monotony runs) | #244 | sentences | author-weighted published fiction with this report’s exact word/paragraph convention |
| Sentence starts (subject, adverb, gerund, …) | #244 | POS of first tokens | Google Books Ngram fiction `_START_` bigrams |
| [Echoes](STYLE-ECHOES.md) (same lemma nearby) | #245 | lemmas | none |
| [Repeated phrases](STYLE-ECHOES.md#repeated-phrases) | #245 | lemma n-grams | none |
| Clichés and redundancies | #246 | phrase lists | none |
| Transitions, pronoun starts | #247 | #244's classifier, word list | Ngram `_START_` 2- to 4-grams (planned; not yet in #247) |
| Overused words | #248 | lemma counts | Ngram fiction unigrams, or HTRC per-author counts |
| Dialogue tags | #249 | dialogue spans | published-fiction corpus (owner-run tier 3 command; see [STYLE-DIALOGUE-TAGS.md](STYLE-DIALOGUE-TAGS.md)) |
| Diction (concreteness), sensory balance | #250 | word-norm tables | none, and the data license comes first |
| Pacing | #251 | dialogue spans, sentence stats | published-fiction corpus, for threshold calibration |

Sentence length and sentence starts are public modules under `style::reports`, with no flattened
crate-root exports. [Sentence report definitions](STYLE-SENTENCES.md) specify word counting, Penn
patterns and priority, ambiguity allocation, dialogue grouping, baseline comparisons, bindings and
public regression measurements. The fine start categories use a documented deterministic priority;
the Ngram universal-12 comparison allocates ambiguous starts fractionally.

## One call for every report

`reports::run` builds nothing itself: it takes one `Document` and runs the reports a `StyleRequest`
selects. `reports::run_json` is the same call from text to JSON, and both bindings' `styleReports`
are thin wrappers over it, so the document pass (the dominant cost) runs once however many reports
are selected.

```rust
use writemorebetter_style::reports::{run, run_json, StyleRequest, StyleResources};

let resources = StyleResources::new(); // keep one per tokenizer; it caches the compiled cliché list
let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
let reports = run(&document, &StyleRequest::new(), &resources, &tokenizer)?; // StyleReports
let json = run_json(&tokenizer, &resources, text, Some(r#"{"echoes":{}}"#), &DocumentOptions::new())?;
```

| Report | Module and entry point | Options | Baseline | Needs readings |
|---|---|---|---|---|
| echoes | `echoes::report(&Document, &EchoOptions)` | `window`, `matchForms` | none | yes |
| repeated phrases | `repeated_phrases::report(&Document, &RepeatedPhraseOptions)` | `minimumLength`, `maximumLength`, `minimumContentWords` | none | yes |
| clichés | `cliches::report(&Document, &ClicheList, &ClicheOptions)` | none yet | none | no |
| dialogue tags | `dialogue_tags::report(&Document, &DialogueTagOptions, Option<&DialogueTagBaseline>)` | `maxUntaggedExchanges`, `minimumTags` | caller only | yes |
| sentence length | `sentence_length::report(&Document, &SentenceLengthOptions, Option<&SentenceLengthBaseline>)` | `minimumSentences`, `longSentenceWords` | bundled | no |
| sentence starts | `sentence_starts::report(&Document, &SentenceStartsOptions, Option<&SentenceStartsBaseline>)` | `minimumSentences` | bundled | yes |

Every entry point returns `Result<_, StyleError>`. Type names are prefixed with their report
(`SentenceLengthReport`, `ClicheMatch`, `DialogueTagVerbClass`, …) so they stay unique in Swift's
and TypeScript's flat namespaces; shared vocabulary is `Span`, `Mode`, `Split<T>`, `Share`,
`SentenceRow`, `SentenceRun`, `DocumentStats` and `StyleError`. Options are `#[non_exhaustive]` and
built with `new()` plus `with_*` builders.

### Request

`StyleRequest::new()` (and an absent `requestJson`) selects every report with default options and
bundled baselines. A JSON request selects exactly the non-null report members it names:

```json
{"echoes": {}, "repeatedPhrases": {"minimumContentWords": 1}, "cliches": {}, "dialogueTags": {"minimumTags": 50},
 "sentenceLength": {"longSentenceWords": 40}, "sentenceStarts": {}, "baselines": "bundled"}
```

`baselines` is `"bundled"`, `null`/absent (no baselines), or an object with any of `sentenceLength`,
`sentenceStarts` and `dialogueTags`, each in the bundled file's `baseline` member format.
`StyleBaselines::bundled()` and each `XBaseline::bundled()` load the files under
`style/data/baselines/` (dialogue tags has none yet, so its `bundled()` is `None`). Option and request
keys are camelCase, also accepting their snake_case names; unknown keys are errors.

### Errors

`StyleError` has three kinds: `invalidOptions` (unparseable request or document options, unknown
keys), `invalidBaseline` (with `report`: a percentage outside 0..=100, a non-finite or negative
length, an unknown percentile or universal tag) and `needsTokenDetails` (with `report`: a compact
document given to a report that needs readings). `run` validates the whole request before running
any report. WASM throws a JS `Error` named `StyleError` with `kind` and `report` properties; Swift
throws `CheckError.ReportInput` with the same message.

### Result

`StyleReports` has `stats`, `sentences` and one member per selected report. Unselected reports are
omitted from JSON. `run_json` and the bindings serialize every `Span` as its UTF-16 `{start, end}`
(native Rust spans keep `byte`, `char` and `utf16`). The examples below come from running the call on
`“Fine,” Mara said softly. She grinned and grinned.⏎The old lamp lit the old lamp room. He kicked
the bucket.` with `Mara` skipped, every report selected, low sample minimums and bundled baselines.
Arrays are cut to their first entries.

`stats` (`DocumentStats`) holds the shared counts every report agrees with:

```json
{"paragraphs": 2, "hardWrapped": false, "tokenDetails": true, "words": {"all": 19, "narration": 18, "dialogue": 1},
 "sentences": {"all": 4, "narration": 3, "dialogue": 1}, "dialogueSpans": 1, "speechSpans": 1}
```

`sentences` is the one sentence table (present when a sentence report is selected); reports refer to
sentences by index into it:

```json
[{"span": {"start": 0, "end": 26}, "mode": "dialogue", "words": 3}, {"span": {"start": 26, "end": 50}, "mode": "narration", "words": 4}]
```

`echoes`:

```json
{"echoes": [{"key": "grin", "label": "grin", "earlier": {"start": 30, "end": 37}, "later": {"start": 42, "end": 49}, "distance": 2}],
 "words": 19, "perThousandWords": 157.89473684210526}
```

`repeatedPhrases`:

```json
{"phrases": [{"keys": ["the", "old", "lamp"], "labels": ["the", "old", "lamp"], "count": 2, "spans": [{"start": 51, "end": 63}, {"start": 68, "end": 80}]}]}
```

`cliches` (`narration` and `dialogue` groups have the same shape as `all`):

```json
{"matches": [{"kind": "cliche", "mode": "narration", "phrase": "kick the bucket", "source": "no-cliches", "span": {"start": 90, "end": 107}}],
 "groups": {"all": {"cliches": 1, "redundancies": 0, "words": 19}, "narration": {}, "dialogue": {}}}
```

`dialogueTags`:

```json
{"tags": [{"span": {"start": 8, "end": 24}, "clause": {"start": 8, "end": 25}, "verb": {"start": 13, "end": 17}, "verbText": "said",
           "lemma": "say", "class": "speech", "position": "after", "adverb": {"start": 18, "end": 24}, "quotes": [{"start": 0, "end": 7}]}],
 "beats": [], "untaggedRuns": [], "otherVerbs": [], "dialogueSpans": 1, "speechSpans": 1, "taggedSpans": 1, "exchanges": 1,
 "untaggedExchanges": 0, "saidAsked": {"count": 1, "percent": 100.0, "difference": null},
 "withAdverb": {"count": 1, "percent": 100.0, "difference": null}, "nonSpeech": {"count": 0, "percent": 0.0, "difference": null}}
```

Beats are `{"span", "quotes"}`, untagged runs `{"span", "quotes", "exchanges"}` and other verbs
`{"lemma", "class", "count"}`; `class` is `speech`, `nonSpeech` or `unlisted` and `position` is
`before`, `after` or `split`.

`sentenceLength` (every group has the same shape as `all`):

```json
{"groups": {"all": {"sentences": 4, "zeroWordSentences": 0, "histogram": [{"words": 3, "sentences": 1}, {"words": 4, "sentences": 2}],
  "mean": 4.75, "meanDifference": -8.150118525896554, "percentiles": [{"percentile": 5, "value": 3.15, "difference": 1.4415492957746479}],
  "longSentences": [], "monotonyRuns": []}, "narration": {}, "dialogue": {}}}
```

`longSentences` are sentence indexes; `monotonyRuns` are `{"span", "sentences": {"start", "end"}}`.

`sentenceStarts` (every group has the same shape as `all`; `sameWordRuns` are runs like length's):

```json
{"starts": [{"sentence": 0, "opener": {"start": 1, "end": 5}, "word": "fine", "category": "adverb", "ambiguous": true,
             "universalCandidates": ["ADJ", "ADV", "NOUN", "VERB"]}],
 "groups": {"all": {"sentences": 4, "excludedSentences": 0, "ambiguousStarts": 1, "unknownStarts": 0,
  "categories": [{"category": "subject", "count": 3, "percent": 75.0, "difference": null}],
  "universal": [{"tag": "DET", "weight": 1.0, "percent": 25.0, "difference": 12.9}], "sameWordRuns": []}, "narration": {}, "dialogue": {}}}
```

### Conventions

- Keys and enum values are camelCase on the wire. The `analyzeDocument` JSON and the Rust
  `Document` serialization keep their shipped snake_case.
- Every item a UI highlights carries a self-contained `span`; nothing refers to `Document` internals
  by index. The only indexes are into the result's own `sentences` table.
- Aggregates split by mode use `Split<T>` (`all`, `narration`, `dialogue`), with one mode definition
  for every report ([STYLE-DOCUMENT.md](STYLE-DOCUMENT.md#narration-and-dialogue)).
- Shares are percentages, null below the report's `minimum*` sample, with `difference` in
  percentage points against the baseline (null without one). Rates are `perThousandWords`.
- Maps are arrays of records (`histogram`, `percentiles`, `categories`, `universal`), so Swift
  `Codable` and TypeScript types need no string-keyed dictionaries; counts are integers except the
  fractional universal `weight`.
- Results from different `Document`s are not mergeable (echo and phrase keys depend on the whole
  document); analyze the unit the UI shows.

## Baselines

A baseline is the expected value in published fiction: category shares, rates per 1,000 words, or word
frequencies. **Every report with a baseline takes it as a separate input**, so a caller can swap in a
genre-specific one. Default baseline files may ship in `style/data/baselines/`, each recording its
source and version; `StyleBaselines::bundled()` loads them, so callers never strip the source wrapper
by hand. They
are built in the private `cmlarsen/fiction-baseline` repo; each committed baseline file names the build
commit there. Where the numbers come from:

1. **Derived, openly licensed datasets (preferred).** These are counts, not text, and they cover modern
   in-copyright fiction:
   - *Google Books Ngram, English Fiction (2020 release, "v3")*, CC BY 3.0. POS-tagged n-grams with a
     `_START_` sentence marker; n-grams don't cross sentence boundaries, and quotation marks are separate
     tokens, so `_START_ " X` 3-grams give dialogue's first word. Gives sentence-start shares and word
     frequency by year, filtered to edition years 1990–2019. Its tags are Google's 12-tag universal set,
     so VBG, VBN and VB all collapse to VERB, and subordinators such as "because" are ADP. The
     sentence-start baseline therefore covers the categories that survive a Penn Treebank → universal
     mapping, plus word-form cues such as -ing. It blends narration and dialogue.
   - *HathiTrust Research Center Extracted Features 2.0* (CC BY 4.0) joined with *NovelTM*
     English-fiction metadata (Underwood, Kimutis and Witte, 2020; CC0 on Harvard Dataverse, MIT on
     GitHub). Per-page counts with Penn Treebank POS tags and sentence counts; usable window 1990–2009
     after the date filter. Gives per-book and per-author mean sentence length, POS rates and word
     counts, which allows per-author weighting. Sentence counts are per page, so sentences crossing a
     page break are counted twice, and token counts include punctuation: mean sentence length is
     approximate.

   Baselines computed from these may be committed.
2. **The published-fiction baseline corpus**, for what needs running text: per-sentence length
   distributions, the narration/dialogue split, dialogue tags, pacing calibration, and finer
   sentence-start categories. It is legally obtained, DRM-free published fiction, kept in the
   fiction-baseline repo's library. The text is never committed anywhere; aggregate numbers derived from
   it may be published. **It is not `bench/corpus/`**, which is one author's unpublished fiction used only
   for recall and never for baselines.

Build rules for any baseline:
- Where the source has book-level metadata (HTRC + NovelTM, the published-fiction corpus), use fiction
  first published in or after 1990 and average within each author before averaging across authors, so
  one prolific author or series counts once.
- Ngram has no book or author dimension: its baselines are filtered by edition year (1990–2019) and are
  volume-weighted, and the file says so.
- Lemma-keyed baselines (overused words) are lemmatized with the same tagger `analyze()` uses, with the
  word form's POS tag as the disambiguator, and record that tagger's version.
- Record the source, version and date range next to every number.
- Mark genre-specific baselines as such. A blended "fiction" baseline is the default.

## Data rules

- **No pirated or scraped corpora**, for development or evaluation: Books3, LibGen, Anna's Archive,
  unlicensed Hugging Face scrapes.
- **`style/` is MIT OR Apache-2.0 and holds no LGPL data.** Nothing under `style/data/` is copied or
  derived from LanguageTool resources: rules XML, dictionaries, or builtin word lists (the wordiness,
  redundancy and profanity lists included). The crate reads LanguageTool-derived analysis only at
  runtime, through `analyze()`.
- Every vendored third-party file under `style/data/` (phrase lists, word lists, word norms) records its
  source, upstream version and license in NOTICE. Confirm each license from the source itself, not from
  a third party. Baseline files under `style/data/baselines/` carry their source record inside the file
  and need no NOTICE entry.
- Word-norm datasets (concreteness, sensorimotor norms) are vendored only if their license allows
  redistribution. Otherwise they are a caller-supplied input, like baselines.

## Decisions

| Date | Decision | Why |
|---|---|---|
| 2026-10-14 | Reports live in a `style/` crate in this repo, consuming `analyze()` | One Rust library and one tokenizer per app; keeps `nlprule` a faithful port |
| 2026-10-14 | Every report takes its baseline as an input | Callers can swap in genre-specific baselines |
| 2026-10-14 | No pirated or scraped corpora | Legal exposure: pirated library copies were the basis of the settled claims in *Bartz v. Anthropic* (N.D. Cal., 2025) |
| 2026-10-14 | Audiobook transcription rejected as a text source | Speech-to-text loses the author's punctuation, so sentence and dialogue statistics would measure the STT model |
| 2026-10-15 | Aggregate numbers derived from any source, the private corpus included, may be published; text never | Statistics are facts, not creative output, so they need no attribution |
| 2026-10-15 | Prefer derived datasets (Ngram, HTRC EF) over building a corpus | They already cover modern fiction at scale, openly licensed; a private corpus is only needed for running-text statistics |
| 2026-10-15 | Vendor phrase lists only when the list's own origin is permissive (#246): no-cliches, retext-redundant-acronyms, and proselint's `hell.py`/`ras_syndrome.py`; drop proselint's GNU diction, After the Deadline, Garner, Wallace and Nordquist lists | proselint's BSD relicense does not clear the original authors' rights; see [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md) |
| 2026-10-15 | Report clichés in dialogue separately, not suppressed (#246) | Clichés in dialogue are often deliberate characterization; the caller decides; see [STYLE-CLICHES.md](STYLE-CLICHES.md) |
| 2026-10-15 | Brysbaert concreteness ratings are a caller-supplied input; the Lancaster Sensorimotor Norms may be vendored (#250) | CC BY-NC-ND 3.0 vs CC BY 4.0; see [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md) |
| 2026-10-16 | One public API for all reports before the bindings freeze it: report-prefixed type names, camelCase JSON, baselines as a separate argument, spans instead of document indexes, one sentence table, and `reports::run`/`run_json` as the single combined entry point | Swift and TypeScript types share one flat namespace, and separate per-report binding calls rebuilt the document each time |
| 2026-10-16 | One narration/dialogue definition, computed in the document pass: a quote is speech unless it is embedded or the subject of a copula | The sentence reports, clichés and dialogue tags disagreed on the same passages |
| 2026-10-16 | Reports that need readings reject compact documents (`StyleError::NeedsTokenDetails`) | Echoes, repeated phrases, sentence starts and dialogue tags silently degraded on them |

## Accuracy benchmarks

[`bench/style/README.md`](../bench/style/README.md) documents the three-tier harness, pinned public
sources, scoring conventions and first test-split regression floors for #256. CI gates tier 1 analysis
and quote-span accuracy, tier 2's injected-error smoke suite, and tier 3's synthetic-only self-test.
After measurement review, EWT test paragraph-internal boundary F1 is 87.1127% (99.5881% recall after
terminal punctuation), normalized lexical XPOS accuracy is 90.6768%, and normalized lemma accuracy is
95.4520% on 24,102 aligned tokens. Tag gates also require single-normalized-tag correctness not to drop
and conditional mean tag count not to rise. Sentence-start total absolute fractional bias has a
22.5595 pp ceiling, including 1 pp of explicit slack; per-tag biases remain diagnostic. LitBank original-prose test
quote-span precision/recall/F1 is 85.3205% / 92.1817% / 88.6185% after #261 tentative-opener recovery. The benchmark README preserves initial vs reviewed numbers and conventions.
Tier 3 real-corpus calibration remains owner-only and emits aggregate numbers exclusively.
