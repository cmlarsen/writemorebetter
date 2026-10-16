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
  Percentages are suppressed below a minimum sample (default 50 sentences), where noise is larger than
  the differences.

## Where the code lives

```
nlprule/   the LanguageTool port. Unchanged in scope.
           + analyze(text): sentences and tokens with offsets, lemmas, POS tags, chunks (#242)
style/     the reports: crate writemorebetter-style, MIT OR Apache-2.0 (#243)
  src/document.rs   paragraphs, sentences, tokens, quoted-dialogue spans and speech flags, lemma index
  src/reports/      one module and report() entry point per report
  data/             phrase lists and word lists (each credited in NOTICE)
    baselines/      default baselines (source recorded inside each file)
swift/, wasm/       one binding each, exposing check() + analyze() + analyzeDocument() + StyleDocument
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
| Sentence starts (subject, adverb, gerund, …) | #244 | POS of first tokens | author-weighted published chapter-window shares and p5–p95 bands |
| [Echoes](STYLE-ECHOES.md) (same lemma nearby) | #245 | lemmas | none |
| [Repeated phrases](STYLE-ECHOES.md#repeated-phrases) | #245 | lemma n-grams | none |
| Clichés and redundancies | #246 | phrase lists | none |
| Transitions, pronoun starts | #247 | #244's classifier, word list | Ngram `_START_` 2- to 4-grams (planned; not yet in #247) |
| [Overused words](STYLE-OVERUSED-WORDS.md), filler words | #248 | lemma counts | Ngram English Fiction 1990–2019 unigrams, lemmatized (bundled) |
| Dialogue tags | #249 | dialogue spans | published-fiction corpus (owner-run tier 3 command; see [STYLE-DIALOGUE-TAGS.md](STYLE-DIALOGUE-TAGS.md)) |
| [Sensory balance, diction (concreteness)](STYLE-NORMS.md) | #250 | lemmas, word-norm tables | none; Lancaster norms bundled (CC BY 4.0), concreteness table caller-supplied |
| Pacing | #251 | dialogue spans, sentence stats | published-fiction corpus, for threshold calibration |

Sentence length and sentence starts are public modules under `style::reports`, with no flattened
crate-root exports. [Sentence report definitions](STYLE-SENTENCES.md) specify word counting, Penn
patterns and priority, ambiguity allocation, dialogue grouping, baseline comparisons, bindings and
public regression measurements. The fine start categories use a documented deterministic priority;
the Ngram universal-12 comparison allocates ambiguous starts fractionally.

## Independent reports, shared analysis

Build the document once in Rust, then call the reports the application needs:

```rust
use writemorebetter_style::reports::{sentence_length, sentence_starts};
let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
let length = sentence_length::report(&document, &Default::default(),
    sentence_length::SentenceLengthBaseline::bundled().as_ref())?;
let starts = sentence_starts::report(&document, &Default::default(),
    sentence_starts::SentenceStartsBaseline::bundled().as_ref())?;
```

Rust’s `StyleDocument` convenience methods return typed reports with byte, char and UTF-16 spans;
bindings apply `reports::to_utf16_json` when returning strings.

Swift and WASM expose `checker.styleDocument(text, options)` to retain this analysis in a
`StyleDocument`. Several methods cost one analysis plus each report pass. The object owns its
analysis and retains the tokenizer/resources, so it can outlive the checker. Build a new object
when text or document options change. WASM callers must call `free()` when finished; Swift uses
ARC. Construction respects `includeTokenDetails`; methods requiring readings reject compact
documents. Construct documents off the UI thread.

A retained moby150k document (149,091 input bytes) holds about **11–12 MB of WASM linear memory**
in the reviewed measurement. Retain the chapter on screen and `free()` its document on navigation;
Swift callers should drop their retained reference. WASM frees space for reuse by later documents,
while its linear memory high-water mark does not shrink back to the OS. Avoid caching every chapter.

```javascript
const document = checker.styleDocument(text, JSON.stringify({skipNames: ["Mara"]}));
const length = JSON.parse(document.sentenceLength());
const starts = JSON.parse(document.sentenceStarts('{"minimumSentences":50}'));
const cliches = JSON.parse(document.cliches());
const tags = JSON.parse(document.dialogueTags());
const stats = JSON.parse(document.stats());
const echoes = JSON.parse(document.echoes());
const phrases = JSON.parse(document.repeatedPhrases());
const senses = JSON.parse(document.sensoryBalance());
const diction = JSON.parse(document.diction(undefined, concretenessTableText));
const overused = JSON.parse(document.overusedWords());
const long = JSON.parse(document.sentenceLength('{"includeLongSentences":true}'));
const descriptive = JSON.parse(document.sentenceLength(undefined, "none"));
document.free();
```

Each method accepts that report's options JSON (omitted means defaults) and returns only its
result as JSON. `stats()` returns `DocumentStats`. Baseline-aware methods (`sentenceLength`,
`sentenceStarts`, `dialogueTags`, `overusedWords`) take a separate optional `baselineJSON`: omit it for bundled data,
pass a baseline object encoded as JSON to override it, use `"bundled"` explicitly, or use
`"none"` to disable comparisons.
There is no bundled dialogue-tag baseline yet. `diction` takes no baseline; its second argument is
the caller's word-to-concreteness table (JSON object or TSV text), which is required: omitting it
throws `missingTable` rather than returning an empty report. Options use camelCase, accept snake_case aliases,
and reject unknown keys. Overrides never change later calls. Native Rust `report` functions take
`Option<&Baseline>` explicitly; `XBaseline::bundled()` loads the matching bundled file.
`OverusedWordsBaseline::bundled()` returns `Option<&'static OverusedWordsBaseline>`, parsed once,
because its table holds 14,382 lemmas; pass it directly instead of through `.as_ref()`.

Swift has typed `Codable`, `Equatable`, `Sendable` results decoded from the same UTF-16 JSON:

```swift
let document = checker.styleDocument(text: text, options: DocumentOptions(skipNames: ["Mara"]))
let length: SentenceLengthReport = try document.sentenceLength()
let starts: SentenceStartsReport = try document.sentenceStarts(optionsJson: "{\"minimumSentences\":50}")
let tags: DialogueTagReport = try document.dialogueTags()
let cliches: ClicheReport = try document.cliches()
let stats: DocumentStats = try document.stats()
let echoes: EchoReport = try document.echoes()
let phrases: RepeatedPhraseReport = try document.repeatedPhrases()
let senses: SensoryBalanceReport = try document.sensoryBalance()
let diction: DictionReport = try document.diction(table: concretenessTableText)
let overused: OverusedWordsReport = try document.overusedWords()
let custom = try document.sentenceLength(baseline: .custom("{\"mean\":13}"))
let descriptive = try document.sentenceLength(baseline: .none)
if let match = cliches.matches.first {
    let range: NSRange = match.span.nsRange
}
```

Swift’s typed methods take `baseline: StyleBaseline = .bundled`; choose `.none` to disable
comparisons or `.custom(json)` for an override. Raw `*Json` methods expose the underlying strings
and accept the same named string selectors as WASM. Real JS `null`, Swift `nil`, and omitted
arguments select bundled data; the JSON text `"null"` is invalid baseline input. Malformed baseline
JSON reports `invalidBaseline` with the report name. TypeScript result declarations ship in the generated WASM package and are also in
[`wasm/style-results.d.ts`](../wasm/style-results.d.ts); import a result with `import type` and
annotate the value decoded with `JSON.parse`.

Subplot chooses which methods to call. Its recommended default set is **clichés, dialogue tags,
sentence length statistics and monotony, and sentence starts**. **Echoes, repeated phrases,
long-sentence flags, overused and filler words, sensory balance and diction** are opt-in lenses;
sensory balance and diction stay opt-in until the owner has seen usefulness numbers on real manuscripts. Length statistics and monotony remain available without
`includeLongSentences`. The library has no combined request or default report selection.

| Report | Module and entry point | Options | Baseline | Needs readings |
|---|---|---|---|---|
| echoes | `echoes::report(&Document, &EchoOptions)` | `window`, `matchForms` | none | yes |
| repeated phrases | `repeated_phrases::report(&Document, &RepeatedPhraseOptions)` | `minimumLength`, `maximumLength`, `minimumContentWords`, `minimumOccurrences`, `nearbyWords` | none | yes |
| clichés | `cliches::report(&Document, &ClicheList, &ClicheOptions)` | none yet | none | no |
| dialogue tags | `dialogue_tags::report(&Document, &DialogueTagOptions, Option<&DialogueTagBaseline>)` | `maxUntaggedExchanges`, `minimumTags` | caller only | yes |
| sentence length | `sentence_length::report(&Document, &SentenceLengthOptions, Option<&SentenceLengthBaseline>)` | `minimumSentences`, `includeLongSentences`, `longSentenceWords`, `longestPerChapter` | bundled | no |
| sentence starts | `sentence_starts::report(&Document, &SentenceStartsOptions, Option<&SentenceStartsBaseline>)` | `minimumSentences` | bundled | yes |
| sensory balance | `sensory_balance::report(&Document, &PerceptualNorms, &SensoryBalanceOptions)` | `minimumStrength`, `dominanceMargin`, `sightMargin`, `minimumSenseWords`, `singleSensePercent` | none (norms bundled: `PerceptualNorms::bundled()`) | yes |
| diction | `diction::report(&Document, &ConcretenessTable, &DictionOptions)` | `abstractBelow`, `minimumRatedWords`, `minimumRunSentences` | none (table caller-supplied: `ConcretenessTable::parse`) | yes |
| overused words | `overused_words::report(&Document, &OverusedWordsOptions, Option<&OverusedWordsBaseline>)` | `minimumRatio`, `minimumCount`, `minimumReferencePerMillion`, `includeNouns`, `countScaleWords` | bundled; fillers run without one | yes |

Every entry point returns `Result<_, StyleError>`. Type names are prefixed with their report
(`SentenceLengthReport`, `ClicheMatch`, `DialogueTagVerbClass`, …) so they stay unique in Swift's
and TypeScript's flat namespaces; shared vocabulary is `Span`, `Mode`, `Split<T>`, `Share`,
`SentenceRow`, `SentenceRun`, `DocumentStats` and `StyleError`. Options are `#[non_exhaustive]` and
built with `new()` plus `with_*` builders.

### Errors

Each method validates its inputs independently. Baseline parse errors identify the report and
use `invalidBaseline`; options errors use `invalidOptions`. `StyleError` has five kinds: `invalidOptions`
(unparseable options or unknown keys), `invalidBaseline` (with `report`: invalid values, percentile
or universal tags), `needsTokenDetails` (with `report`: a compact document supplied to a report
requiring readings), `missingTable` (with `report`: diction called without a word table) and
`invalidTable` (with `report`: an unparseable, empty or non-finite word table). WASM throws an `Error` named `StyleError` with `kind` and `report`; Swift throws
`CheckError.ReportInput` with the same message.

### Result

Each report stands alone. Sentence length and starts each include their own `sentences` rows and
`words: {all, narration, dialogue}`. Echoes and phrases include `words`; clichés have word counts
per group; dialogue tags have split word counts and their own dialogue/speech/exchange counts.
Bindings serialize `Span` as UTF-16 `{start, end}`; native Rust spans keep byte, char and UTF-16
ranges. The examples below come from `“Fine,” Mara said softly. She grinned and grinned.⏎The old
lamp lit the old lamp room. He kicked the bucket.` with `Mara` skipped, sample minimums set to 1,
and phrases set to two occurrences and one content word. Arrays are truncated to their first
entries and narration/dialogue groups abbreviated as `{}`; all top-level fields are shown.

`stats` (`DocumentStats`) holds the shared counts every report agrees with:

```json
{"paragraphs": 2, "hardWrapped": false, "tokenDetails": true, "words": {"all": 19, "narration": 18, "dialogue": 1}, "sentences": {"all": 4, "narration": 3, "dialogue": 1}, "dialogueSpans": 1, "speechSpans": 1}
```

`sentences` belongs to each sentence report. All its indexes, including opener references, long
sentences and runs, refer only to that result’s own table:

```json
[{"span": {"start": 0, "end": 26}, "mode": "dialogue", "words": 3}, {"span": {"start": 26, "end": 50}, "mode": "narration", "words": 4}, {"span": {"start": 51, "end": 87}, "mode": "narration", "words": 8}, {"span": {"start": 87, "end": 108}, "mode": "narration", "words": 4}]
```

`echoes`:

```json
{"echoes": [], "words": 19, "perThousandWords": 0}
```

`repeatedPhrases`:

```json
{"words": 19, "phrases": [{"keys": ["the", "old", "lamp"], "labels": ["the", "old", "lamp"], "count": 2, "spans": [{"start": 51, "end": 63}, {"start": 68, "end": 80}]}]}
```

`cliches` (`narration` and `dialogue` groups have the same shape as `all`):

```json
{"matches": [{"kind": "cliche", "mode": "narration", "phrase": "kick the bucket", "source": "no-cliches", "span": {"start": 90, "end": 107}}], "groups": {"all": {"cliches": 1, "redundancies": 0, "words": 19}, "narration": {}, "dialogue": {}}}
```

`overusedWords` (here `minimumCount` and `minimumRatio` are 1; `fillers` holds records of the same
shape with `word` instead of `lemma` and nullable reference fields):

```json
{"words": 19, "overused": [{"lemma": "grin", "count": 2, "perTenThousandWords": 1052.6315789473683, "referencePerTenThousandWords": 1.64, "ratio": 641.8485237483953, "spans": [{"start": 30, "end": 37}, {"start": 42, "end": 49}]}], "fillers": []}
```

`dialogueTags`:

```json
{"words": {"all": 19, "narration": 18, "dialogue": 1}, "tags": [{"span": {"start": 8, "end": 24}, "clause": {"start": 8, "end": 25}, "verb": {"start": 13, "end": 17}, "verbText": "said", "lemma": "say", "class": "speech", "showy": false, "position": "after", "adverb": {"start": 18, "end": 24}, "quotes": [{"start": 0, "end": 7}]}], "beats": [], "untaggedRuns": [], "otherVerbs": [], "neutralVerbs": [{"lemma": "say", "class": "speech", "count": 1}], "showyVerbs": [], "lowConfidence": true, "dialogueSpans": 1, "speechSpans": 1, "taggedSpans": 1, "exchanges": 1, "untaggedExchanges": 0, "saidAsked": {"count": 1, "percent": 100, "difference": null, "highlighted": false}, "withAdverb": {"count": 1, "percent": 100, "difference": null, "highlighted": false}, "nonSpeech": {"count": 0, "percent": 0, "difference": null, "highlighted": false}}
```

`sensoryBalance` (with `minimumSenseWords: 1`; `chapters` and `scenes` are `{"span", "chapter",
"groups", "singleSense"}` with `groups` shaped like the top-level one; every group lists all six
modalities in a fixed order):

```json
{"words": {"all": 19, "narration": 18, "dialogue": 1}, "senseWords": [{"span": {"start": 18, "end": 24}, "lemma": "softly", "modality": "touch", "strength": 4.0, "mode": "narration"}, {"span": {"start": 64, "end": 67}, "lemma": "light", "modality": "sight", "strength": 4.6, "mode": "narration"}], "groups": {"all": {"words": 19, "senseWords": 2, "modalities": [{"modality": "sight", "count": 1, "percent": 50.0}]}, "narration": {}, "dialogue": {}}, "chapters": [{"span": {"start": 0, "end": 108}, "chapter": 0, "groups": {}, "singleSense": null}], "scenes": [{"span": {"start": 0, "end": 108}, "chapter": 0, "groups": {}, "singleSense": null}]}
```

`diction` (with `minimumRatedWords: 1` and the invented table `{"lamp": 4.8, "room": 4.5,
"bucket": 4.9, "fine": 2.1, "grin": 3.9}`; groups have the same shape as `all`):

```json
{"words": {"all": 19, "narration": 18, "dialogue": 1}, "tableWords": 5, "sentences": [{"span": {"start": 0, "end": 26}, "mode": "dialogue", "words": 3, "ratedWords": 1, "mean": 2.1, "abstract": false}], "paragraphs": [{"span": {"start": 0, "end": 50}, "sentences": {"start": 0, "end": 2}, "ratedWords": 3, "mean": 3.3}], "groups": {"all": {"sentences": 4, "ratedWords": 7, "mean": 4.128571428571428, "abstractSentences": 0}, "narration": {}, "dialogue": {}}, "abstractRuns": []}
```

Beats are `{"span", "quotes"}`, untagged runs `{"span", "quotes", "exchanges"}` and other verbs
`{"lemma", "class", "count"}`; `class` is `speech`, `nonSpeech` or `unlisted` and `position` is
`before`, `after` or `split`.

`sentenceLength` (every group has the same shape as `all`):

```json
{"sentences": [{"span": {"start": 0, "end": 26}, "mode": "dialogue", "words": 3}, {"span": {"start": 26, "end": 50}, "mode": "narration", "words": 4}, {"span": {"start": 51, "end": 87}, "mode": "narration", "words": 8}, {"span": {"start": 87, "end": 108}, "mode": "narration", "words": 4}], "words": {"all": 19, "narration": 18, "dialogue": 1}, "groups": {"all": {"sentences": 4, "zeroWordSentences": 0, "histogram": [{"words": 3, "sentences": 1}, {"words": 4, "sentences": 2}], "mean": 4.75, "meanDifference": -8.030488207960593, "meanHighlighted": true, "lowConfidence": true, "percentiles": [{"percentile": 5, "value": 3.15, "difference": 1.3636581644704968, "highlighted": true}], "longSentences": [], "monotonyRuns": []}, "narration": {}, "dialogue": {}}}
```

`longSentences` are sentence indexes; `monotonyRuns` are `{"span", "sentences": {"start", "end"}}`.

`sentenceStarts` (every group has the same shape as `all`; `sameWordRuns` are runs like length's):

```json
{"sentences": [{"span": {"start": 0, "end": 26}, "mode": "dialogue", "words": 3}, {"span": {"start": 26, "end": 50}, "mode": "narration", "words": 4}, {"span": {"start": 51, "end": 87}, "mode": "narration", "words": 8}, {"span": {"start": 87, "end": 108}, "mode": "narration", "words": 4}], "words": {"all": 19, "narration": 18, "dialogue": 1}, "starts": [{"sentence": 0, "opener": {"start": 1, "end": 5}, "word": "fine", "category": "adverb", "ambiguous": true, "universalCandidates": ["ADJ", "ADV", "NOUN", "VERB"]}], "groups": {"all": {"sentences": 4, "lowConfidence": true, "excludedSentences": 0, "ambiguousStarts": 1, "unknownStarts": 0, "categories": [{"category": "subject", "count": 3, "percent": 75, "difference": 17.72164537024203, "highlighted": true}], "universal": [{"tag": "ADJ", "weight": 0.25, "percent": 6.25, "difference": 1.7130070188631956, "highlighted": false}], "sameWordRuns": []}, "narration": {}, "dialogue": {}}}
```

### Conventions

- Keys and enum values are camelCase on the wire. The `analyzeDocument` JSON and the Rust
  `Document` serialization keep their shipped snake_case.
- Every item a UI highlights carries a self-contained `span`; nothing refers to `Document` internals
  by index. The only indexes are into the result's own `sentences` table.
- Aggregates split by mode use `Split<T>` (`all`, `narration`, `dialogue`), with one mode definition
  for every report ([STYLE-DOCUMENT.md](STYLE-DOCUMENT.md#narration-and-dialogue)).
- Shares are percentages, null below the report's `minimum*` sample, with `difference` in
  percentage points against the baseline (null without one). A difference is descriptive;
  only `highlighted` (or length's `meanHighlighted`) warrants a baseline highlight, strictly outside
  the author's p5–p95 band for that mode. Missing bands never highlight. `lowConfidence` is true below
  200 eligible sentences or 50 tags; statistics display from 50 sentences or 15 tags. Rates are `perThousandWords`.
- Maps are arrays of records (`histogram`, `percentiles`, `categories`, `universal`), so Swift
  `Codable` and TypeScript types need no string-keyed dictionaries; counts are integers except the
  fractional universal `weight`.
- Results from different `Document`s are not mergeable (echo and phrase keys depend on the whole
  document); analyze the unit the UI shows.

## Baselines

### Author percentile bands

Sentence baselines retain their existing top-level blended reference values for `all`. Optional
`narration` and `dialogue` reference objects use the same measure fields (length: `mean`,
`percentiles`; starts: `categoryPercent`, `universalPercent`). They are never filled from the blended
reference. The root `bands` is split into `all`, `narration` and `dialogue`; each measure maps to
`PercentileBand { p5, p95 }`. For example:

```json
{"mean":13,"narration":{"mean":17},"dialogue":{"mean":9},
 "bands":{"all":{"mean":{"p5":9,"p95":19}},
          "narration":{"mean":{"p5":12,"p95":24}},
          "dialogue":{"mean":{"p5":5,"p95":14}}}}
```

These illustrative values are synthetic, not published calibration. Length's `bands.*.percentiles`
maps sentence percentile keys (`5`, `25`, `50`, `75`, `95`) to author bands. Starts' band maps use the
same categories and universal tags as its reference values. Missing modes/measures are allowed;
missing bands never highlight. Bounds must be finite, nonnegative and ordered, and share bounds
cannot exceed 100. Equality at either boundary does not highlight.

Bundled files contain the owner-supplied 2026-10-16 author-weighted chapter-window p5/p95 bands,
with exact source provenance and the full percentile sets preserved in `calibration`. Baseline means
and category shares are the same calibration's matching-mode reference means. Per-mode references
accept only measures, not nested modes or bands. Missing measures still never highlight. Synthetic
chapter tests check published medians and values beyond p95 in all three modes, alongside boundary
and invalid-band tests. Confidence labels are independent of population comparisons.

A baseline is the expected value in published fiction: category shares, rates per 1,000 words, or word
frequencies. **Every report with a baseline takes it as a separate input**, so a caller can swap in a
genre-specific one. Default baseline files may ship in `style/data/baselines/`, each recording its
source and version; each report’s `XBaseline::bundled()` loads its file, so callers never strip the source wrapper
by hand. They
are built in the private `cmlarsen/fiction-baseline` repo; each committed baseline file names the build
commit there. The overused-words table is the exception: it comes from public Ngram counts and is
rebuilt in this repository by `bench/style/overused_baseline.py` (see
[STYLE-OVERUSED-WORDS.md](STYLE-OVERUSED-WORDS.md#reference-table)). Where the numbers come from:

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
  redistribution. Otherwise they are a caller-supplied input, like baselines. The Lancaster
  Sensorimotor Norms (CC BY 4.0) ship as a derived table in `style/data/norms/`; the Brysbaert
  concreteness ratings (CC BY-NC-ND) do not ship and are never used in tests.

## Decisions

| Date | Decision | Why |
|---|---|---|
| 2026-10-16 | #281 review: norm lookups prefer a verb reading's lemma over a base-form homograph (*felt* → *feel*), figurative and hearsay rules look past whitespace only, the bundled table keeps the 3.5/0.5 floor with haptic leads measured past interoception, defaults stay 4.0 / 1.0 / sight 3.0, and diction marks only narration sentences `abstract` | Past *felt* counted as the fabric and skipped the *feel* rule; *cold* (touch 4.11, interoception 3.53) could never count. Lowering default strength to 3.7 added *turn*, *wake* and *forget* as noise, so *warm*/*crimson* stay opt-in. Blind re-judging: 14/26/20 → 23/30/7 useful/noise/wrong ([STYLE-NORMS.md](STYLE-NORMS.md#re-review-281)) |
| 2026-10-16 | #250: sensory balance is an opt-in lens over a derived Lancaster table: a sense word's strongest perceptual modality rates ≥ 4.0 and leads the next by ≥ 1.0 (sight by ≥ 3.0); `singleSense` needs ≥ 20 narration sense words and ≥ 85% in one modality | Calibrated on bench/public plus three Standard Ebooks chapters ([STYLE-NORMS.md](STYLE-NORMS.md)): nearly every concrete noun rates visual-dominant, so a uniform margin made sight 75–92% of every scene; the sight margin lifted useful sense words from 13 to 33 of 60 sampled. Published scenes peak at 77% one modality (Gatsby’s multi-sensory dawn), so 85% leaves no false flags on them |
| 2026-10-16 | #250: diction takes a required caller-supplied concreteness table (JSON or TSV), flags runs of ≥ 3 consecutive abstract narration sentences (mean < 2.5 over ≥ 3 rated words), and errors without a table | Brysbaert is CC BY-NC-ND and stays out of the repository and tests; defaults assume its 1–5 scale. Real-world usefulness depends on the table Subplot supplies, an owner decision |
| 2026-10-16 | #248: overused words is an opt-in lens with a bundled Ngram English Fiction 1990–2019 lemma table (14,382 lemmas ≥ 1 per million, 233 KB); flag at ratio ≥ 4, count ≥ 5, reference ≥ 10 per million, nouns opt-in | Calibrated to a median of 5 flags (2–9) on 14 public chapter-sized texts; an editor review judged 32/76 useful, 42 noise, 2 wrong. Nouns added 152 mostly topical flags; the floor drops rare, archaic and topical words. See [STYLE-OVERUSED-WORDS.md](STYLE-OVERUSED-WORDS.md) |
| 2026-10-16 | #248 review: the overused-words reference adds Ngram `_ADP`, `_CONJ` and `_PRT` uses to lemmas with content uses, and folds 248 British spellings into their American lemma on both sides | A document counts prepositional *near*, *above* and *past* because they keep their adverb and adjective readings, so a reference without them inflated ratios up to 10× (*above* 20.2 → 202 per million). *grey* and *realise* had ratios 1.7–4.8× too high for British spelling. See [STYLE-OVERUSED-WORDS.md](STYLE-OVERUSED-WORDS.md#reference-table) |
| 2026-10-16 | #248 review: above 10,000 words the overused-words `minimumCount` grows with √(words / 10,000), rounded to the nearest count (`countScaleWords`) | The defaults were calibrated on chapters; unscaled, the 25,002-word Moby-Dick excerpt gave 25 flags, and a novel would give dozens. Scaled it gives 7. Chapter-sized input is unchanged |
| 2026-10-16 | #248 review: noun modifiers the tagger reads as adjectives (*express car*) stay counted; overused words keeps its own one-lemma-per-token choice instead of `lemma_index` or `echo_families` | Ngram's POS tags can't separate them: *tender* is NOUN 2.4M vs ADJ 183k, so a rule that dropped *express* would drop *a tender smile* too. `lemma_index` lists every reading and would double-count ambiguous tokens, so the same token can get a different lemma in echoes |
| 2026-10-16 | #248: fixed filler list *just, really, very, quite, actually, simply, suddenly, somehow* and clause-opening *that*, reported by rate with or without a table | Short list of the words line editors cut first; rates rather than flags because every writer uses them |
| 2026-10-16 | #269: listed dialogue interjections consistently open `introductory`, including bare utterances; retain their opener spans | Skipping comma-led interjections would contradict adjudicated `Ay`, `Oh` and `Yes` labels and lower the required floors. Extend the existing category convention to aye, alas, hey, okay, ok, yeah, yep, nope, hmm, um, uh, hurrah and nay |
| 2026-10-16 | Owner direction (#275): independent methods on retained Swift/WASM `StyleDocument` objects; remove the combined API; each result owns its counts and sentence rows | Share the expensive analysis while letting Subplot choose its default set and opt-in lenses; typed per-report Swift and TypeScript results |
| 2026-10-16 | #275: retained documents honor `includeTokenDetails: false`; only clichés and sentence length run on compact documents | Reverses the removed combined binding’s forced full analysis. Keep the default full options for style documents; compact `analyzeDocument` options should not be reused for the recommended report set |
| 2026-10-14 | Reports live in a `style/` crate in this repo, consuming `analyze()` | One Rust library and one tokenizer per app; keeps `nlprule` a faithful port |
| 2026-10-14 | Every report takes its baseline as an input | Callers can swap in genre-specific baselines |
| 2026-10-14 | No pirated or scraped corpora | Legal exposure: pirated library copies were the basis of the settled claims in *Bartz v. Anthropic* (N.D. Cal., 2025) |
| 2026-10-14 | Audiobook transcription rejected as a text source | Speech-to-text loses the author's punctuation, so sentence and dialogue statistics would measure the STT model |
| 2026-10-15 | Aggregate numbers derived from any source, the private corpus included, may be published; text never | Statistics are facts, not creative output, so they need no attribution |
| 2026-10-15 | Prefer derived datasets (Ngram, HTRC EF) over building a corpus | They already cover modern fiction at scale, openly licensed; a private corpus is only needed for running-text statistics |
| 2026-10-15 | Vendor phrase lists only when the list's own origin is permissive (#246): no-cliches, retext-redundant-acronyms, and proselint's `hell.py`/`ras_syndrome.py`; drop proselint's GNU diction, After the Deadline, Garner, Wallace and Nordquist lists | proselint's BSD relicense does not clear the original authors' rights; see [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md) |
| 2026-10-15 | Report clichés in dialogue separately, not suppressed (#246) | Clichés in dialogue are often deliberate characterization; the caller decides; see [STYLE-CLICHES.md](STYLE-CLICHES.md) |
| 2026-10-15 | Brysbaert concreteness ratings are a caller-supplied input; the Lancaster Sensorimotor Norms may be vendored (#250) | CC BY-NC-ND 3.0 vs CC BY 4.0; see [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md) |
| 2026-10-15 | Echoes are an opt-in lens with say/ask, name/honorific, particle and heading suppressions; no transitive ambiguous-reading chains | The writer review found 97% noise; deliberate repetition is not an issue list |
| 2026-10-16 | Repeated phrases and long-sentence flags join echoes as opt-in lenses; keep sentence-length statistics and monotony in defaults | Re-review of b113df7 measured phrases at 97% noise/wrong (146/150 reviewed) and long sentences at 84% noise (43/51); default output should prioritize useful signals |
| 2026-10-16 | Use the owner-supplied chapter-window bands, preserving full aggregate provenance | Integrates #270 calibration without accessing private data; strict p5/p95 highlighting per mode |
| 2026-10-16 | Narrow neutral tags; cried remains showy; top-three unquoted long sentences; punctuation-bounded longest phrases; conservative headings and two-blank-line scenes | Owner follow-up to the #273 usefulness review; optional name fallback and p95 lens preserve caller control |
| 2026-10-15 | Baseline highlights require a value outside the published-author p5–p95 band for its measure and narration/dialogue mode | Blended averages and small deltas mislead novelists; ship non-highlighting placeholder bands until owner chapter-window calibration |
| 2026-10-15 | Show sentence statistics from 50 sentences and tag percentages from 15 tags, with low-confidence flags below 200 sentences / 50 tags | Short chapters need statistics while exposing their smaller samples |
| 2026-10-15 | Flag only showy dialogue verbs; report neutral verbs as counts | Reply, answer, whisper, shout and similar ordinary attributions are not prose problems |
| 2026-10-15 | Narration runs cross paragraphs; repeated phrases need frequency/proximity and merged overlaps; long sentences use top 3 per chapter plus candidates above the writer's p95; untagged runs need more than 12 exchanges | Apply the writer review's engineering fixes while leaving the licensed cliché lists unchanged |
| 2026-10-16 | Superseded by #275: one public API for all reports before the bindings freeze it: report-prefixed type names, camelCase JSON, baselines as a separate argument, spans instead of document indexes, one sentence table, and `reports::run`/`run_json` as the single combined entry point | Swift and TypeScript types share one flat namespace, and separate per-report binding calls rebuilt the document each time |
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

The #256 completion audit is in [`bench/style/AUDIT-256.md`](../bench/style/AUDIT-256.md).
All nine merged reports now have tier 2 coverage: the existing sentence, echo/phrase, cliché and
dialogue suites plus `lexical.py` for overused words and fillers, sensory balance and single-sense
scenes, and caller-table diction sentences and abstract runs. The test split scores 100% P/R/F1 per
category on 1,714 cases (295–300 occurrences per overused lemma, 60 per filler word, 60 per sensory
modality, 60 single-sense scenes, 420 abstract sentences, 60 abstract runs), with targeted negative
controls for each filter and threshold and zero changes on paired untouched prefixes; development
has 1,725 cases. The 217 test-split predictions on the untouched public paragraphs are frozen and
gated for equality, so a natural-text precision regression fails CI; a deliberate report change
rebaselines them. These are injection regression scores, not editorial accuracy. All 21 report mutations tried
(the reviewer's 12 plus 9 boundary changes, listed in the README) fail the gate.
Tier 3 flag-rate calibration now covers all nine reports, including sensory modalities, overused/filler
counts and rated-word-weighted diction means, and requires an owner-supplied diction table for real
runs. Only synthetic self-tests ran here. Approximately 300 independent, owner-adjudicated judgment
labels per report and actual owner-only calibration for new reports remain owed; the README lists
existing model-label provenance explicitly, so #256 is referenced and remains open.
