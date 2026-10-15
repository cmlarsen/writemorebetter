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
fetch it again; checksum mismatches fail rather than accepting changed inputs. GitHub codeload tarball
bytes can change even at an unchanged commit (for example, recompression). If that occurs, migrate to
per-file raw URLs at the same pinned commit with individually reviewed checksums; do not simply accept
a new archive hash without checking its contents.

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

EWT `# newdoc` and `# newpar` delimit paragraphs. Within each paragraph, gold sentence text is joined
with a single space; each paragraph is analyzed separately, matching #243 and tier 3. Boundaries are
sentence **end offsets** in Unicode scalar characters, after trimming whitespace. Paragraph-final
boundaries are forced by the input and excluded from the main P/R/F1; their counts and scores are
reported separately. An additional recall measures internal gold boundaries after `.`, `!`, or `?`
with optional closing quote marks. Email fragments without terminal punctuation remain in the main
score, so this terminal-punctuation recall is useful for fiction. The benchmark does not independently
call the splitter on each gold sentence to score boundaries.

Gold token forms align in order to the source text by exact scalar offsets. Empty dependency nodes
are ignored. For a multiword-token surface whose component forms concatenate exactly to the surface,
components receive cumulative offsets and are scored individually. Only non-concatenating components
are excluded, with both surface/component exclusion counts reported (zero on the pinned test split).
An analysis token must match both endpoints; there is no edit-distance or concatenated-token matching.
Counts include total gold components, aligned count/fraction, and the separate lexical denominator.
Alignment coverage has a floor to prevent hiding difficult tokens through tokenization changes.

### Equivalence definitions and ambiguity

[`equivalence.py`](equivalence.py) is the single shared implementation of the following definitions,
used by the tag benchmark, sentence-start benchmark, and tier 3. These definitions are fixed and must
be reviewed before changing a baseline.

| Convention | Definition |
|---|---|
| M1 POS normalization | `NN:* → NN`, `PRP_* → PRP`, `PRP$_* → PRP$`, `RB_SENT → RB` |
| M1 punctuation | `PCT` maps by surface to Penn punctuation (`.!? → .`, `;: → :`, comma, quote, bracket, hyphen and symbol classes); default `.` |
| Lemma conventions | Unicode case folding, then `could → can`, `would → will`, `these → this` on both gold and candidate lemmas |
| Unsupported gold XPOS | `ADD GW AFX XX FW LS` excluded from lexical scoring and counted |
| Punctuation/symbols | Gold UPOS `PUNCT`/`SYM` excluded from lexical tag/lemma scoring; `is_punctuation` agreement reported separately (expected true for PUNCT, false for SYM), including per-class counts/accuracy |
| Lexical conventions (diagnostic only) | gold subordinator (`UPOS=SCONJ`, XPOS IN) vs CC/DT/WDT/WP; WDT↔WP; NNPS↔NNS; RBR↔JJR |

`normalized_tag_any` is gold XPOS membership among distinct M1-normalized readings on aligned lexical
tokens. `normalized_lemma_any` uses the lemma conventions on the same lexical denominator. Unknown
words with empty readings are incorrect; there is no surface-as-lemma fallback. Raw exact tag/lemma
membership and first-reading equality over **all** aligned tokens remain diagnostics for comparison.
Neither reading order nor priority is promised by `analyze()`, so first-reading accuracy is not gated.
Tag/lemma membership is tested independently, not as a paired reading match.

Adding readings can inflate any-reading accuracy. The gate therefore also requires
`single_normalized_correct` (exactly one distinct normalized POS string, equal to gold) not to drop.
This guards ambiguity without capping the mean number of tags, which could penalize a legitimate
new reading on a previously unknown token. Mean distinct normalized tags is still printed; it includes
all normalized POS strings, even internal strings without a Penn mapping, and gives unknowns zero.
Lexical convention matches are printed as additional counts per pair and an inclusive accuracy, but
**never folded into the gated accuracy**. The no-reading share covers all aligned non-PUNCT/SYM tokens,
including unsupported gold tags, and has an upper bound gate.

### Sentence-start bias check

For each EWT gold sentence, analyze that sentence independently to isolate starts from boundary errors.
Project its first non-quote gold XPOS through the Google universal-12 Penn map (`X` for unsupported gold
tags), and apply tier 3's shared procedure to the prediction. Each candidate Penn reading is normalized
before mapping; duplicate universal candidates count once. Resolved-only shares use singleton starts;
fractional shares allocate `1/N` to each of N candidate universal tags. Unknown starts have no allocation.
Gold shares cover all gold sentences, including starts for which the prediction has no candidates,
so the diagnostic includes selection bias from unknowns as well as ambiguity.

Output reports gold, resolved-only and fractional shares, both biases in percentage points, and the
resolution rate within each gold tag. The gate caps **each tag's absolute fractional bias**, and floors
the number of starts with candidates. This exposes the resolved-subset selection effect before any
Ngram comparison; it does not make the EWT web distribution a fiction baseline.

### LitBank original prose

Sequentially align quotation-layer TSV tokens to `original/*.txt`, permitting only whitespace in the
gaps, then map inclusive annotation sentence/token endpoints to half-open original scalar spans.
Validate annotation text against its TSV tokens. The originals contain whole public-domain books;
**only the prefix through the last annotated token is scored**, so unannotated later speech cannot
become a false positive. Original spacing, wrapped lines and paragraph breaks are retained.

The bench-only baseline handles curly/straight **double** quotes, carries open quotes across
paragraphs, and treats a repeated opening quote at an actual paragraph start (blank-line separator)
as continuation. A wrapped line is not a paragraph boundary. Unclosed quotes are discarded.
Delimiters are included; a true positive requires the complete span to match exactly. Counts are
micro-aggregated over documents, including those with no quotations. Scare quotes can be false
positives; single-quote and dash speech are missed. There is no speaker attribution. Until #243/#249
replace this baseline, its floor guards the harness rather than product dialogue accuracy.

## Tier 2 framework

```sh
python3 bench/style/run.py tier2 generate --out bench/style/out
# Run a detector against every file in out/documents.jsonl, including untouched controls.
python3 bench/style/run.py tier2 score --gold bench/style/out/answers.jsonl \
    --predictions bench/style/out/predictions.jsonl
```

The smoke run does all three steps. From each pinned book it skips at least the first 2,500 chapter
words, beyond LitBank's annotated opening excerpts, then selects 60 chapter paragraphs with at least
20 words and no adjacent repeated words. It repeats the eighth word in 40 of them and
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
UTF-16 counts code units and bytes count UTF-8. Pass `--manifest PATH` to select the document manifest;
the default is `documents.jsonl` beside the gold file. Gold and prediction files must belong to that
manifest, including untouched controls; unknown file identifiers are rejected before filtering.
`--report NAME` selects records with that report field; `--pattern NAME` selects a single pattern.
The scorer accepts any detector with this format, counts
extra flags on untouched text as false positives, treats duplicate predictions as extra false positives,
and rejects duplicate gold spans. It reports micro P/R/F1 per pattern and an exact-span confusion matrix
with `__none__` for unmatched predictions/misses. A wrong category on the right span appears off-diagonal.
It does not grant overlap credit. A record may instead carry `spans: [{"byte": {...}, "utf16": {...}}, ...]`
for a multi-occurrence group, omitting top-level offsets. For `repeated_word`, a single covering span
includes both copies and their separating whitespace. For a grouped echo/phrase/run pattern, the
identity is the **unordered exact set of occurrence spans**; all occurrences must match, with duplicate
spans rejected. Gold has one label per file/span-set within a selected suite. There is no inferred
`group` identifier or partial-match credit.

Each report issue (#244 onward) owns its detector probe/adapter emitting these JSONL records, plus its
pattern vocabulary and span convention. Clean/injected-pair precision (to separate natural hits from
new flags) and overlap scoring are deferred to the first real report issue; exact-match precision here
is valid only for the toy predicate with validated controls. This PR claims no real-report accuracy.

`labels/example.jsonl` defines the hand-label format for judgment suites: the same `file`, `byte`,
`utf16`, and `pattern` fields, plus `report` (`sentence_start`, `transition`, `pacing`), `labelers`
(independent labeler identifiers) and `adjudication`. Offsets refer to `labels/excerpt.txt`, the first
sentence of [Melville's public-domain opening](https://standardebooks.org/ebooks/herman-melville/moby-dick/text/chapter-1).
Its end excludes the final newline. These three records demonstrate the schema only; they are not an
adjudicated evaluation set. Use `--report sentence_start` (or `transition` / `pacing`) when scoring it, so one task has one label
per span; `labels/documents.jsonl` supplies its manifest. Each
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
text never are. Probe stderr and exception details are suppressed in tier 3, including BaseException / interrupts and argparse errors.

A word token contains a Unicode letter or digit. Length without punctuation counts those tokens;
length with punctuation adds tokens flagged `is_punctuation`. Symbols such as emoji are excluded from
both. Contractions follow `analyze()` tokenization. Ngram and HTRC count punctuation tokens; matching
that convention helps comparison but tokenization, source selection and weighting still differ.
The JSON baseline records source/version/year ranges and the supplied published values. It is a
reference snapshot from #256, not a rebuild of those datasets. Ngram is volume-weighted; HTRC has
per-page sentence-count artifacts. Export units need not be whole volumes, so HTRC's volume median is
printed as a reference rather than treated as a matched unit-level estimator.

Sentence starts skip leading quote tokens, following Ngram's convention. M1 normalization precedes
the shared Penn → universal-12 lookup (`IN → ADP`, `TO/RP/POS → PRT`, verb forms and MD → VERB).
Both **resolved-only** and **fractional** shares are printed next to the baseline and its gap.
For a noun/verb-ambiguous start, resolved-only gets no count and fractional gets 0.5 NOUN + 0.5 VERB;
it is never counted as a resolved VERB merely because the noun tag carried an engine suffix.
Unknown and ambiguous starts are counted separately; unknowns cannot be fractionally allocated.
Within each method, shares are computed per author and authors averaged equally; authors with no
eligible starts for that method are counted and excluded from its average. Fractional shares retain
ambiguous starts, but unknown-word selection and tagger choices still bias them. Tier 1's EWT per-tag
bias table measures this effect on public gold data; treat Ngram gaps as calibration diagnostics.

All 12 shares are printed, with `null` for missing reference categories rather than invented zeros or
renormalizing the seven supplied shares. Length output compares punctuation-inclusive author means
with Ngram/HTRC means and the author median with HTRC's median.

`--self-test` creates only synthetic generated text in a temporary directory, verifies unequal author
weights, quote handling, noun/verb ambiguity, counts and numeric aggregates, and emits the same aggregate-only JSON. CI runs
this mode only. No private corpus or real fiction-baseline export was accessed to implement or test
this PR. Tier 3 is a foundation for future report distributions, not a calibration claim about modern
fiction or a report-quality gate.

## Test-split baseline after measurement review (2026-10-15)

English 6.6 tokenizer, unchanged engine (`a0db66a` merged analyze API), pinned sources above. The
initial metric definitions at `07023f3` were superseded following review. This is a redefinition of the
measurement, **not an engine improvement**:

| Test metric | Initial definition | Reviewed definition |
|---|---:|---:|
| EWT boundary precision / recall / F1 | 97.8495 / 74.4824 / 84.5817% (document newline join, forced finals included) | **96.6135 / 79.3132 / 87.1127%** (paragraphs, forced finals excluded) |
| EWT recall after terminal punctuation | not separated | **99.5881%** (971 internal gold boundaries) |
| EWT paragraph-final matches / gold | not separated | 854 / 854 (reported, excluded from main F1) |
| EWT aligned / total tokens | 23,508 / 25,094 (93.6798%) | **24,102 / 25,094 (96.0469%)** |
| EWT MWT components excluded | 708 (all) | **0** (concatenating contractions recovered) |
| EWT lexical denominator / unsupported gold tags | not separated | 21,130 / 84 excluded |
| EWT gold XPOS among readings | 80.9639% raw over all aligned | **90.6768%** M1-normalized over lexical tokens |
| EWT single normalized tag and correct | not measured | **62.2338%** (second tag gate) |
| EWT normalized tags per lexical token | not measured | 1.52925 (diagnostic) |
| EWT lexical-convention inclusive accuracy | not measured | 92.1439% (diagnostic; not gated) |
| EWT gold lemma among readings | 91.6326% raw over all aligned | **95.4520%** normalized over lexical tokens |
| EWT no readings / aligned non-punctuation | not separated | 748 / 21,214 = **3.5260%** |
| EWT punctuation/symbol flag agreement | not separated | 97.8878% (2,888 tokens; per-class detail in JSON) |
| EWT raw tag any / first (current cohort) | 80.9639 / 69.4530% on old cohort | 81.2879 / 70.0357% (diagnostic) |
| EWT raw lemma any / first (current cohort) | 91.6326 / 89.2973% on old cohort | 91.6438 / 89.3785% (diagnostic) |
| LitBank quote-span precision / recall / F1 | 84.6154 / 82.7252 / 83.6596% (tokenized lines) | **84.9962 / 83.0975 / 84.0361%** (original annotated prefixes) |
| LitBank exact matches / predicted / gold | 1,111 / 1,313 / 1,343 | **1,116 / 1,313 / 1,343**, 80 test documents |
| Tier 2 smoke P/R/F1 | 100 / 100 / 100% | **100 / 100 / 100%**, after the 2,500-word exclusion |

Current EWT start-share comparison (all 2,077 gold sentences; 1,833 starts have candidates, 1,304 are
resolved). Values are percentages; biases are percentage points. Every absolute fractional bias below
has its own upper-bound gate.

| Tag | Gold | Resolved-only | Fractional | Fractional bias |
|---|---:|---:|---:|---:|
| ADJ | 7.3182 | 4.1411 | 7.3213 | +0.0031 |
| ADP | 5.3442 | 2.2239 | 2.6878 | -2.6565 |
| ADV | 7.4145 | 8.5890 | 8.8171 | +1.4025 |
| CONJ | 1.7814 | 4.4479 | 4.4326 | +2.6512 |
| DET | 10.6403 | 15.3374 | 11.4339 | +0.7936 |
| NOUN | 22.4362 | 19.2485 | 22.8742 | +0.4380 |
| NUM | 1.6851 | 1.8405 | 1.4139 | -0.2712 |
| PRON | 22.9177 | 34.3558 | 24.7545 | +1.8368 |
| PRT | 0.0963 | 0.0000 | 0.7610 | +0.6648 |
| VERB | 10.1589 | 8.6656 | 13.1488 | +2.9899 |
| . | 4.3332 | 0.6135 | 0.4364 | -3.8967 |
| X | 5.8739 | 0.5368 | 1.9185 | -3.9553 |

`tier1-baseline.json` stores full-precision lower bounds for boundary/quote P/R/F1, terminal recall,
alignment count/fraction, normalized tag/lemma accuracy, single-tag correctness, punctuation agreement
and candidate-start coverage; upper bounds cover no-reading share and every tag's absolute fractional
start bias. Raw and lexical-convention metrics are diagnostic only. The tolerance is 1e-12 for floating
arithmetic; thresholds are never rounded or automatically rewritten. A rebaseline needs review of the
metric definitions, exclusions, alignment coverage, ambiguity and per-category differences, not only
headline scores. The README retains the initial numbers above to make this review change explicit.
