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
  data/             phrase lists and word lists, each source credited in NOTICE
swift/, wasm/       one binding each, exposing check() + analyze() + the reports
```

- **Not in `nlprule`.** `nlprule` measures itself by LanguageTool conformance. These reports are not
  LanguageTool features, and they return statistics as well as spans.
- **Not in a separate repository.** An app should link one Rust library per platform. Two Rust static
  libraries collide (#239), and two WASM modules would load the multi-megabyte tokenizer twice.
- **No duplicates of LanguageTool rules.** Some LanguageTool rules overlap the reports: EN_REPEATEDWORDS,
  TOO_OFTEN_USED_* (#234, #235), PARAGRAPH_REPEAT_BEGINNING_RULE (#206), READABILITY_RULE_* (#207) and
  EN_CONSISTENT_APOS (#228). Their ports stay in `nlprule` for conformance. A report that overlaps one
  says how it differs.
- **One whole-document pass.** #243's document pass is also the "document statistics" call #234 needs.
  Design it once.

## Reports

| Report | Issue | Needs | Baseline |
|---|---|---|---|
| Sentence length (distribution, monotony runs) | #244 | sentences | private corpus (needs running text) |
| Sentence starts (subject, adverb, gerund, …) | #244 | POS of first tokens | Google Books Ngram fiction `_START_` bigrams |
| Echoes (same lemma nearby) | #245 | lemmas | none |
| Repeated phrases | #245 | lemma n-grams | none |
| Clichés and redundancies | #246 | phrase lists | none |
| Transitions, pronoun starts | #247 | #244's classifier, word list | Ngram `_START_` bigrams |
| Overused words | #248 | lemma counts | Ngram fiction unigrams, or HTRC per-author counts |
| Dialogue tags | #249 | dialogue spans | private corpus |
| Diction (concreteness), sensory balance | #250 | word-norm tables | none, and the data license comes first |
| Pacing | #251 | dialogue spans, sentence stats | private corpus, for threshold calibration |

## Baselines

A baseline is the expected value in published fiction: category shares, rates per 1,000 words, or word
frequencies. **Every report takes its baseline as an input**, so a caller can swap in a genre-specific one.
Default baseline files may ship in `style/data/baselines/`, each listing its sources and license. Where
the numbers come from:

1. **Derived, openly licensed datasets (preferred).** These are counts, not text, and they cover modern
   in-copyright fiction:
   - *Google Books Ngram, English Fiction (2020 release, "v3")*, CC BY 3.0. POS-tagged n-grams with a
     `_START_` sentence marker; n-grams don't cross sentence boundaries. Gives sentence-start shares and
     word frequency by year. Filter to 1990 onward.
   - *HathiTrust Research Center Extracted Features 2.0* (CC BY 4.0) joined with *NovelTM*
     English-fiction metadata (Underwood, Kimutis and Witte, 2020; CC0). Per-page counts with Penn Treebank
     POS tags and sentence counts, for fiction up to 2009. Gives per-book and per-author mean sentence
     length, POS rates and word counts, which allows per-author weighting.

   Baselines computed from these may be committed with attribution, under the source's license.
2. **A private corpus**, for what needs running text: per-sentence length distributions, the
   narration/dialogue split, dialogue tags, pacing calibration. It is built from legally obtained,
   DRM-free fiction. The text is never committed anywhere; aggregate numbers derived from it may be
   published.

Build rules for any baseline:
- Use fiction first published in or after 1990. Dates in these datasets are often edition dates.
- Average within each author before averaging across authors, so one prolific author or series counts once.
- Record the source, version and date range next to every number.
- Mark genre-specific baselines as such. A blended "fiction" baseline is the default.

## Data rules

- **No pirated or scraped corpora**, for development or evaluation: Books3, LibGen, Anna's Archive,
  unlicensed Hugging Face scrapes.
- Every file under `style/data/` records its source, upstream version and license in NOTICE. Confirm
  each license from the source itself, not from a third party.
- Word-norm datasets (concreteness, sensorimotor norms) are vendored only if their license allows
  redistribution. Otherwise they are a caller-supplied input, like baselines.

## Decisions

| Date | Decision | Why |
|---|---|---|
| 2026-10-14 | Reports live in a `style/` crate in this repo, consuming `analyze()` | One linked library and one tokenizer per app (#239); keeps `nlprule` a faithful port |
| 2026-10-14 | Every report takes its baseline as an input | Callers can swap in genre-specific baselines |
| 2026-10-14 | No pirated or scraped corpora | Legal exposure, and the reason for *Bartz v. Anthropic* (2025) |
| 2026-10-14 | Audiobook transcription rejected as a text source | Speech-to-text loses the author's punctuation, so sentence and dialogue statistics would measure the STT model |
| 2026-10-15 | Aggregate numbers derived from any source, the private corpus included, may be published; text never | Aggregates aren't the text; attribution follows each dataset's license |
| 2026-10-15 | Prefer derived datasets (Ngram, HTRC EF) over building a corpus | They already cover modern fiction at scale, openly licensed; a private corpus is only needed for running-text statistics |
