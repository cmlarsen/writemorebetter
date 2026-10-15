# Prose-report comparison plan

Candidate tools checked against their primary documentation on 2026-10-14. The candidate list is followed by the first measured comparison below. Different tools serve different layers of the prose reports.

| Comparator | Layer to compare | Primary documentation |
|---|---|---|
| spaCy, starting with `en_core_web_sm` | Sentence boundaries, tokenization, lemma/POS analysis, and optional dependency evidence for sentence starts | [Linguistic features](https://spacy.io/usage/linguistic-features), [English models](https://spacy.io/models/en) |
| BookNLP | Literary quotation extraction; later, speaker attribution as a separate, more ambitious task | [Project and output formats](https://github.com/booknlp/booknlp) |
| proselint | Overlapping cliché, redundancy and repetition rules | [Checks](https://github.com/amperser/proselint) |
| write-good | Overlapping wordiness, cliché, adverb, passive-voice and repetition rules | [Checks and API](https://github.com/btford/write-good) |
| ProWritingAid | Product comparison for sentence length, echoes, dialogue, transitions and pacing reports | [Writing reports](https://prowritingaid.com/features/writing-reports) |

Start with spaCy for the #242/#243 foundation and BookNLP for quoted-dialogue comparisons. BookNLP
uses spaCy for syntax, so its syntactic output is not an independent second opinion. Add proselint
and write-good per report where rules overlap. ProWritingAid is a report-level comparison; do not
assume its reports or results are available through an automatable API.

## Evaluation protocol

1. Pin tool, model and rule-set versions and record enabled components. Compare sentence/tagging
   pipelines separately from full applications with extra grammar or coreference work.
2. Use invented, hand-annotated fiction fixtures as correctness truth. Include curly quotes,
   contractions, dropped-g words, abbreviations, fragments, nested and multi-paragraph dialogue,
   apostrophes/possessives, em dashes, ellipses, U+2028, combining marks and non-BMP characters.
3. Normalize offsets into original-text UTF-8 byte ranges. Do not compare raw token indices from
   different tokenizers. Compare sentence-boundary precision/recall/F1, quotation-span exact and
   overlap precision/recall, and aligned lemma/POS readings with an explicit tag mapping.
4. For each report, score matching rule concepts against human annotations, including deliberate
   fictional voice as negative examples. Distinguish detection quality from subjective advice.
   Tool agreement is a diagnostic, not ground truth. Compare distribution results only after
   documenting differences in word counting, dialogue handling and thresholds.
5. Measure cold load, warm analysis latency, throughput, peak RSS, model/data size and output
   materialization separately. Use the same paragraph boundaries and CPU/thread settings. Include
   both a short chapter and a novel-length public-domain input, and record hardware and load.
6. Keep the EPUB corpus local. Read extracted text only from the separate fiction-baseline
   project's ignored `out/`, preserve reviewed-catalog filtering and author weighting, and publish
   aggregate statistics only. Commercial-service comparisons use invented/public-domain text.
   Keep validation authors separate from authors used to compute comparison baselines.

The existing LanguageTool 6.6 reference probes remain useful for inherited-engine conformance;
spaCy/BookNLP provide additional comparisons rather than redefining that conformance target.
Benchmark adapters and optional local tooling are separate from the shipped engine and style crate.

## First measured comparison

The local harness now runs spaCy 3.8.11 / `en_core_web_sm` 3.8.0 and BookNLP's standalone
`QuoteTagger` at commit `3d900fc2224e55960c3363826ae28539b77b4204` against the Rust document model.
NER is excluded from spaCy; syntax uses its tok2vec, tagger, parser, attribute ruler and lemmatizer.
The BookNLP measurement adds the unmodified quote component over spaCy tokens, with caller-supplied
paragraph IDs. It does **not** run neural entity/coreference/speaker attribution. spaCy alone has no
quote task here and is not assigned a quotation score.

Results are in [bench/style/results.json](../bench/style/results.json), measured on an Apple M5 Pro,
one CPU process, with common thread caps and `NLPRULE_PARALLELISM=off`. Each process loads once;
the timing shown is the median of two warm calls after its first analysis. RSS is the process peak
across those calls. Python import/startup is excluded from model-load timing but included in recorded
process time. Snapshots are released before the next call. Rust includes owned token/chunk output
and a lemma index; spaCy includes dependency parsing. These pipelines do useful overlapping work,
not identical computation.

| Input / tool | Warm analysis | Peak RSS |
|---|---:|---:|
| 147,687 bytes: writemorebetter document | 0.876 s | 62.5 MiB |
| 147,687 bytes: spaCy syntax | 1.837 s | 271.6 MiB |
| 147,687 bytes: spaCy + BookNLP quotes | 1.829 s | 306.2 MiB |
| 1,476,879 bytes: writemorebetter document | 8.329 s | 245.1 MiB |
| 1,476,879 bytes: spaCy syntax | 18.776 s | 586.8 MiB |
| 1,476,879 bytes: spaCy + BookNLP quotes | 18.885 s | 716.1 MiB |

The input is the repository's public-domain Moby-Dick excerpt, then ten copies as a novel-length
stress input, with identical blank-line paragraph handling. This tests throughput, not literary
coverage across a whole novel.

On 16 crafted fiction fixtures (30 expected sentence spans, 11 outer quoted spans, nine selected
lemma/POS probes), exact sentence-span F1 is 0.949 for writemorebetter and 0.900 for spaCy. Exact
quote-span F1 is 1.000 for the style model and 0.750 for the BookNLP quote component; byte-coverage
F1 is 1.000 / 0.840. Selected-reading coverage is 9/9 / 7/9. This last metric asks whether the
expected reading is present among a token's readings; it is **not** overall POS tagging accuracy.
The fixtures cover known project requirements and are not an independent held-out test set, so these
numbers do not establish general superiority.

Both pipelines keep one ellipsis/U+2028 fixture as one sentence where the annotations expect two.
spaCy additionally separates an exclamation-ended quotation from its attribution. Quote-component
differences include possessives, dropped-g words, EOF handling and multi-paragraph range policy.
Some differences are deliberate contract choices, rather than bugs in the comparator. Normalized
differences are retained in the result JSON. No grammar-engine changes were made to improve these scores.

### Reproduce

```sh
bash bench/style/setup.sh
# Pinned Python packages/models and upstream BookNLP source live only in ignored data/.
data/style-bench/venv/bin/python bench/tools/style_compare.py --runs 3
# Add --keep-details for full byte-normalized fixture predictions in the result file.
data/style-bench/venv/bin/python -m unittest discover -s bench/tools -p test_style_compare.py
```

The harness currently uses macOS `/usr/bin/time -l` for RSS. It does not call remote inference
services. Proselint/write-good comparisons remain for the phrase/rule reports they actually overlap;
ProWritingAid is still a product-level comparison on public/invented text.

### Local corpus values

`bench/tools/style_corpus.py` can compute provisional sentence-length and opening distributions
from the cached local fiction corpus. It leaves the catalog unchanged and keeps all extracted text,
requests, counts and aggregate results inside `fiction-baseline/out/provisional-style/`. Selection
uses included, untranslated novels/novellas first published from 1990 with author metadata and body
sections; anthologies/collections are excluded until story attribution is reviewed. Counts are pooled
within an author, normalized per context, then averaged equally across authors with at least 200
sentences in that context. All currently eligible entries are unreviewed, so the output is explicitly
provisional, not a curated published-fiction baseline. No corpus text enters the public benchmark.
