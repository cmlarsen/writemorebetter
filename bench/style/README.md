# Style benchmarks

Accuracy and calibration infrastructure for [#256](https://github.com/cmlarsen/writemorebetter/issues/256).
The harness uses Python's standard library and the existing tokenizer through a small Rust JSONL probe.
It changes no engine behavior and loads no grammar rules. Run from the repository root with English
6.6 data built by `scripts/build-data.sh` and staged at `storage/en_tokenizer.bin`:

```sh
python3 bench/style/run.py tier1 --split dev
python3 bench/style/run.py tier1 --gate
python3 bench/style/run.py tier2 smoke
python3 bench/style/run.py tier3 --self-test
python3 -m unittest discover -s bench/style -p 'test_*.py'
```

`--tokenizer PATH` and `--analyzer PATH` select the data and an already-built probe for tiers 1/3.
Without `--analyzer`, the entry point builds `bench/tools/wmb-analyze` automatically. The probe accepts
one JSON string per stdin line and emits one JSON array of owned analysis sentences per stdout line,
using Rust's byte, Unicode scalar (`char`) and UTF-16 span fields unchanged. A persistent process reuses
one tokenizer across all inputs. `--output PATH` saves tier 1's aggregate JSON.

## Download provenance and splits

`sources.json` is the lockfile: every download has a tag or commit, URL, SHA-256 and original license
source. Downloads are verified before parsing, including on cache hits, and stored in the git-ignored
`.cache/`. Archives are read as data without extracting or executing their members. No downloaded text
or annotations are committed. CI caches by the complete lockfile hash. Delete a damaged cache file to
fetch it again; checksum mismatches fail rather than accepting changed inputs.

| Dataset | Pin | License and attribution | Use |
|---|---|---|---|
| UD English-EWT | `r2.15` (`4dc8e10cf32352e11ab2c46e024b19853b91546e`) | [CC BY-SA 4.0; EWT contributors](https://github.com/UniversalDependencies/UD_English-EWT/blob/r2.15/README.md) | official dev and test files |
| LitBank quotation layer | `3e50db0ffc033d7ccbb94f4d88f6b99210328ed8` | [CC BY 4.0; David Bamman and collaborators](https://github.com/dbamman/litbank/blob/3e50db0ffc033d7ccbb94f4d88f6b99210328ed8/README.md) | direct speech |
| Standard Ebooks, Austen's *Pride and Prejudice* | `ed94a32be3e875c0814568050c7544e03aad3ef5` | [CC0 dedication](https://standardebooks.org/about) | tier 2 source paragraphs |
| Standard Ebooks, Melville's *Moby-Dick* | `013b51e2d9d6bb28b1d054bfc30beeeff4b85480` | [CC0 dedication](https://standardebooks.org/about) | tier 2 source paragraphs |

Standard Ebooks confirms that the underlying works are believed public domain in the US and dedicates
its own contributions under CC0. Users elsewhere must check their jurisdiction. See also its
[public-domain explanation](https://standardebooks.org/about/standard-ebooks-and-the-public-domain).
LitBank's quotation layer is described in the
[quotation paper linked by its original release](https://arxiv.org/abs/2004.13980).

`splits.json` explicitly records every LitBank document: sorted filenames, first 20 dev, remaining 80
test. This is our split, not an upstream standard. It is not author-disjoint and should not be used to
claim generalization to unseen authors. EWT uses its official dev/test split. Tune only on dev; test
scores are regression measurements. Splits must stay fixed when adding detectors.

Excluded datasets:

- **QuoteLi3:** the [original Stanford release page](https://nlp.stanford.edu/~muzny/quoteli.html),
  checked 2026-10-15, links XML and archives but gives no data license. A third-party card is insufficient
  license evidence; neither those files nor QuoteLi3 are downloaded by this harness.
- **UD English-GUM:** CC BY-NC-SA 4.0; excluded for its noncommercial restriction.
- **PDNC:** no confirmed data license; excluded.

## Tier 1 scoring conventions

EWT sentences are reconstructed from `# text` and joined with a single newline within each upstream
`newdoc` document. Sentence boundaries are **end offsets** in Unicode scalar characters, after trimming
whitespace. Document-final boundaries count. We analyze complete reconstructed documents, not each gold
sentence independently. Joining and upstream punctuation are kept fixed across runs. These web texts
are a useful foundation test, not an estimate of fiction accuracy.

Gold token forms are aligned to the reconstructed text by exact character offsets in order. We ignore
empty dependency nodes. Multiword-token surface ranges have no single gold XPOS/lemma; their component
words have no uniquely defined surface offsets in this conservative alignment and are excluded. Tokens
outside multiword ranges are scored only when both their start and end equal an analysis token's scalar
offsets. No edit-distance matching, case folding, or token concatenation is used. The denominator is
aligned gold tokens, including punctuation and unknown tokens (empty readings are incorrect). Output
includes total gold component tokens, aligned count/fraction and excluded multiword surface count.
Clitics and hyphens can therefore lower coverage without silently becoming POS errors. Coverage has a
regression floor so removing difficult alignments cannot silently improve accuracy.

Tag and lemma matches are exact. `tag_any` and `lemma_any` test independent membership among all
post-disambiguation readings; this is not a claim that the gold tag/lemma occur as the same pair.
**The gate uses these any-reading accuracies.** First-reading equality is also printed for diagnosis,
but `analyze()` does not promise reading priority or stable ordering, so it is not a best-tag estimate
or a gate. Repeated readings count once per token. No Penn-to-universal mapping is used in tier 1.

LitBank is scored on its supplied whitespace-tokenized quotation-layer `.txt`, preserving sentence
newlines. Annotation start/end sentence-token indices are converted to half-open scalar spans (end
index inclusive upstream). Each span's text is checked against the annotation before scoring. The
baseline handles curly/straight **double** quotes, carries open quotes across paragraphs, and treats a
repeated opening quote at a line start as continuation. Unclosed quotes are discarded. Delimiters are
included, matching LitBank; a true positive requires the complete span to match exactly. Counts are
micro-aggregated over documents, including documents without any quotations. Source paragraph layout
is not preserved in LitBank's tokenized files; paragraph continuation is also tested synthetically.

This baseline detects typography rather than speech: scare quotes can be false positives; single-quote
speech and dash speech are missed. It has no speaker attribution. It lives only in the bench until the
style document detector replaces it. A replacement must use the same split and scoring conventions.

## Tier 2 framework

```sh
python3 bench/style/run.py tier2 generate --out bench/style/out
# Run a detector against every file in out/documents.jsonl, including untouched controls.
python3 bench/style/run.py tier2 score --gold bench/style/out/answers.jsonl \
    --predictions bench/style/out/predictions.jsonl
```

The smoke run does all three steps. From each pinned book it selects the first 60 chapter paragraphs
with at least 20 words and no adjacent repeated words. It repeats the eighth word in 40 of them and
leaves 20 untouched. These 120 passages are clean only with respect to the toy predicate, not adjudicated
for every future report. Output includes the answer key, source manifest, modified texts and predictions
under ignored `out/`. The toy detector flags adjacent case-insensitive exact repeated words separated
by whitespace. The smoke gate requires P/R/F1 = 1 across 80 injections and 40 untouched controls.

Every answer or prediction is one JSONL object:

```json
{"file":"pride-001.txt","byte":{"start":38,"end":47},"utf16":{"start":38,"end":47},"pattern":"repeated_word"}
```

Offsets are zero-based, half-open and refer to the **final injected text**, not the clean source.
`file` is the exact manifest-relative identifier. All four offsets must match to receive credit;
UTF-16 counts code units and bytes count UTF-8. The scorer accepts any detector with this format, counts
extra flags on untouched text as false positives, treats duplicate predictions as extra false positives,
and rejects duplicate gold spans. It reports micro P/R/F1 per pattern and an exact-span confusion matrix
with `__none__` for unmatched predictions/misses. A wrong category on the right span appears off-diagonal.
It does not grant overlap credit. Future report suites define their own pattern labels and injection
strategies (echoes, phrases, clichés, starts, tags); this PR claims accuracy only for the toy detector.

`labels/example.jsonl` defines the hand-label format for judgment suites: the same `file`, `byte`,
`utf16`, and `pattern` fields, plus `report` (`sentence_start`, `transition`, `pacing`), `labelers`
(independent labeler identifiers) and `adjudication`. Offsets refer to `labels/excerpt.txt`, the first
sentence of [Melville's public-domain opening](https://standardebooks.org/ebooks/herman-melville/moby-dick/text/chapter-1).
Its end excludes the final newline. These three records demonstrate the schema only; they are not an
adjudicated evaluation set. Filter by report before scoring so one task has one label per span. Each
real suite needs its own label vocabulary, about 300 sentences, two independent labels and owner
adjudication before its accuracy is gated.

## Tier 3: owner-only calibration

Only the owner runs the following against an actual fiction-baseline export:

```sh
python3 bench/style/run.py tier3 --export /path/to/export/out \
    --baseline bench/style/published-baseline.json
```

The export has `units.jsonl` (including `author_key`, `authors`, `book_id`, `form`, `genre`,
`publisher_type`, `source`, `title`, `unit_id`, `words`, `year`) and `units/<unit_id>.txt`, one paragraph
per line with blank scene breaks. This skeleton consumes only `author_key` and `unit_id`; the owner must
supply the desired fiction/year/genre selection. It does not claim to validate that metadata or select
books automatically. Duplicate unit IDs and path traversal fail.

The script analyzes each nonempty paragraph separately, as required by the engine notes on #243.
It pools sentence counts within each `author_key`, computes each author's mean length and start shares,
then averages authors equally. Thus a prolific author counts once. Length percentiles (linear
interpolation at 0/5/25/50/75/95/100) describe the **distribution of author means**, not individual
sentence lengths. Authors without sentences/resolved starts are excluded from the respective mean and
reported as aggregate counts. Units and author-key counts are printed; names, titles, identifiers and
text never are. Probe stderr and exception details are suppressed in tier 3, including errors.

A word token contains a Unicode letter or digit. Length without punctuation counts those tokens;
length with punctuation adds tokens flagged `is_punctuation`. Symbols such as emoji are excluded from
both. Contractions follow `analyze()` tokenization. Ngram and HTRC count punctuation tokens; matching
that convention helps comparison but tokenization, source selection and weighting still differ.
The JSON baseline records source/version/year ranges and the supplied published values. It is a
reference snapshot from #256, not a rebuild of those datasets. Ngram is volume-weighted; HTRC has
per-page sentence-count artifacts. Export units need not be whole volumes, so HTRC's volume median is
printed as a reference rather than treated as a matched unit-level estimator.

Sentence starts skip leading quote tokens, following Ngram's convention. Penn tags map to Google's
12-tag universal set (`IN → ADP`, `TO/RP/POS → PRT`, all verb forms and MD → VERB). A start is resolved
only when its recognized Penn readings agree on one universal tag; unknown and ambiguous starts are
counted and excluded from the start-share denominator. Reading order does not decide ambiguity.
All 12 shares are printed with baseline/gap where a published value exists; missing categories are
`null`, never invented zeros or renormalized to the seven supplied categories. Length output compares
the punctuation-inclusive author mean with Ngram/HTRC means and the author median with HTRC's median.

`--self-test` creates only synthetic generated text in a temporary directory, verifies unequal author
weights, quote handling, counts and numeric aggregates, and emits the same aggregate-only JSON. CI runs
this mode only. No private corpus or real fiction-baseline export was accessed to implement or test
this PR. Tier 3 is a foundation for future report distributions, not a calibration claim about modern
fiction or a report-quality gate.

## First test-split baseline (2026-10-15)

English 6.6 tokenizer, engine main `a0db66a` (merged analyze API), pinned datasets above:

| Metric | Baseline |
|---|---:|
| EWT sentence-boundary precision / recall / F1 | 97.8495% / 74.4824% / **84.5817%** |
| EWT aligned gold tokens / total | **23,508 / 25,094 (93.6798%)** |
| EWT multiword surface tokens excluded | 354 |
| EWT gold XPOS among readings | **80.9639%** |
| EWT first-reading XPOS (diagnostic) | 69.4530% |
| EWT gold lemma among readings | **91.6326%** |
| EWT first-reading lemma (diagnostic) | 89.2973% |
| LitBank direct-speech span precision / recall / F1 | 84.6154% / 82.7252% / **83.6596%** |
| LitBank exact matches / predicted / gold, 80 test documents | 1,111 / 1,313 / 1,343 |
| Tier 2 repeated-word smoke P/R/F1 | **100% / 100% / 100%** |

`tier1-baseline.json` stores the full-precision floors used by `--gate`: boundary and quote P/R/F1,
aligned count/fraction, any-reading XPOS and lemma accuracy. CI fails on any drop (tolerance 1e-12 for
floating arithmetic), without rounding the comparison. Baselines are not automatically rewritten;
update only after review of changed metrics, coverage and conventions. First-reading values remain
informational. Data manifests and splits must be reviewed with any baseline change.
