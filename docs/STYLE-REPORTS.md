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
  src/document.rs   paragraphs, sentences, tokens, quoted-dialogue spans, lemma index
  src/reports/      one module per report
  data/             phrase lists and word lists (each credited in NOTICE)
    baselines/      default baselines (source recorded inside each file)
swift/, wasm/       one binding each, exposing check() + analyze() + the reports
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
- **Inputs.** Every report takes the document, its baseline, and an optional skip-list of names
  (characters, places) that the caller supplies. Skipped names are left out of counts and denominators,
  not filtered from the output afterwards.

## Reports

| Report | Issue | Needs | Baseline |
|---|---|---|---|
| Sentence length (distribution, monotony runs) | #244 | sentences | published-fiction corpus (needs running text); HTRC for the mean |
| Sentence starts (subject, adverb, gerund, …) | #244 | POS of first tokens | Google Books Ngram fiction `_START_` bigrams |
| Echoes (same lemma nearby) | #245 | lemmas | none |
| Repeated phrases | #245 | lemma n-grams | none |
| Clichés and redundancies | #246 | phrase lists | none |
| Transitions, pronoun starts | #247 | #244's classifier, word list | Ngram `_START_` 2- to 4-grams (planned; not yet in #247) |
| Overused words | #248 | lemma counts | Ngram fiction unigrams, or HTRC per-author counts |
| Dialogue tags | #249 | dialogue spans | published-fiction corpus (planned; not yet in #249) |
| Diction (concreteness), sensory balance | #250 | word-norm tables | none, and the data license comes first |
| Pacing | #251 | dialogue spans, sentence stats | published-fiction corpus, for threshold calibration |

## Baselines

A baseline is the expected value in published fiction: category shares, rates per 1,000 words, or word
frequencies. **Every report takes its baseline as an input**, so a caller can swap in a genre-specific one.
Default baseline files may ship in `style/data/baselines/`, each recording its source and version. They
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
