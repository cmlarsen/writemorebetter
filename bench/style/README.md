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
python3 bench/style/run.py tier3 --flag-rates --self-test
python3 -m unittest discover -s bench/style -p 'test_*.py'
```

`--tokenizer PATH` and `--analyzer PATH` select the data and an already-built analysis probe for tiers 1/3.
Without `--analyzer`, the entry point builds `bench/tools/wmb-analyze` automatically. The probe accepts
one JSON string per stdin line and emits one JSON array of owned analysis sentences per stdout line,
using Rust's byte, Unicode scalar (`char`) and UTF-16 span fields unchanged. A persistent process reuses
one tokenizer across all inputs. Tier 1 also builds `bench/tools/wmb-document` to score product dialogue
spans; `--document-probe PATH` selects an already-built document probe. Its `--dialogue` mode accepts
JSONL objects with `file` and `text`, and emits one JSONL object per detected span with `file`, `byte`,
`utf16`, and `pattern: "dialogue"`. This mode needs no tokenizer. `--output PATH` saves tier 1's
aggregate JSON. See [document measurements](../../docs/STYLE-DOCUMENT-BENCHMARKS.md) for native timing.

## Usefulness revision (2026-10-16)

The owner decisions on #241 change report defaults and scope. The source pins, gold labels and
labeler provenance remain frozen. Rebaselines record the intended changes rather than claiming
better editorial precision from synthetic injections:

- Echoes: plural and gerund matches are restored while the ambiguity regressions remain blocked.
  Precision is 1.0; individual echo-injection recall is 100% in both splits. Combined recall improves
  from 43/58 to 57/58 dev and from 48/61 to 60/61 test, replacing the over-broad morphology floor.
  The remaining natural-copy misses (25/26 each split) target subwords inside `low-arched` and
  `sun-toasted`, which the product tokenizes as whole compounds. The prior quote-turn explanation
  was incorrect. Gold, the 12–25-word near range, the 30-word cap and all far controls are unchanged.
- Repeated phrases: injections now distinguish three uses, two near uses and separate distant-pair
  negative controls. Phrasal verbs and *Piglet said that* are included. In-scope recall and precision
  are 100%; the former distant-pair recall reduction is removed. Rust fixtures cover punctuation
  boundaries, longest-overlap selection and the optional name fallback.
- Dialogue: all frozen floors remain unchanged, including beat precision 9/11 and recall 9/10.
  Response beats such as *I nodded* and *She smiled* no longer attribute the preceding speaker.
  Interrupt-type standalone beats attach to the following quote and skip negated clauses.
- Sentences: the probe explicitly enables the top-three long-sentence lens and checks
  deliberate anaphora separately. Labeled/injected F1 floors remain unchanged. Heading counting
  preserves signs and ordinary prose, recognizes colon/dash titles and excludes exact numbered chapter headings; a single blank line does not break narration runs.
- Synthetic tier 3 self-tests cover a 13-exchange untagged run. All 36 Python tests pass. The new
  owner-supplied chapter-window bands and full percentiles are preserved with exact provenance;
  no private calibration inputs were accessed. Calibration used #270's segmentation, predating
  this PR's refinements. Synthetic median/p95 chapters verify matching-mode highlights.

The final default request retains sentence-length statistics and monotony, sentence starts,
dialogue tags and clichés. Echoes, repeated phrases and long-sentence flags are opt-in; the owner
made this decision after the re-review measured 97% phrase noise/wrong flags and 84% long-sentence
noise. The long lens needs `sentenceLength.includeLongSentences:true`; direct defaults also keep
that flag list empty. The sentence and flag-rate probes explicitly opt in so their gates still
exercise the lens.

`usefulness-counts.json` records default and opt-in counts alongside per-report before/after counts for all six `bench/public` texts,
against main `42f658d`. Reproduce the after counts with:

```sh
cargo run -p writemorebetter-style --release --example usefulness -- \
  bench/public/story1.txt bench/public/story2.txt bench/public/story3.txt \
  bench/public/story4.txt bench/public/story5.txt bench/public/moby150k.txt
```

Echoes are explicitly opted in by this counter; Subplot’s recommended default methods omit them. The
counter uses line mode for stories, hard-wrapped mode for Moby, and no skipped names. Flat `echoes`, `phrases` and `long` fields are explicit lens counts; `defaultFlags` is the final default total, and the snapshot adds separate `default` and `optIn` maps. These are
workload counts, not editorial judgments.

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
| QuoteLi3, *Pride and Prejudice* only (`pp_full.xml`, `pp_test.xml`) | no upstream tag or commit; `Last-Modified` 2017-03-18 / 2021-04-27, sha256 in `sources.json` | **No license found.** Owner decision 2026-10-16: local/CI benchmarking only; annotations are downloaded at bench time, pinned by sha256, and never committed or redistributed. Muzny, Fang, Chang and Jurafsky, [EACL 2017](https://aclanthology.org/E17-1044/); [evidence](../../docs/STYLE-DATA-LICENSES.md#quoteli3-256) | direct speech |

Standard Ebooks confirms that the underlying works are believed public domain in the US and dedicates
its own contributions under CC0. Users elsewhere must check their jurisdiction. See also its
[public-domain explanation](https://standardebooks.org/about/standard-ebooks-and-the-public-domain).
LitBank's quotation layer is described in the
[quotation paper linked by its original release](https://arxiv.org/abs/2004.13980).

`splits.json` explicitly records every LitBank document: sorted filenames, first 20 dev, remaining 80
test. This is our split, not an upstream standard. It is not author-disjoint and should not be used to
claim generalization to unseen authors. QuoteLi3 is split by chapter, also our own split and not
upstream's: dev is chapters 1–2 (the excerpt LitBank's dev split already annotates) plus every fifth
chapter (5, 10, …, 60), 14 chapters and 323 quotes; test is the other 47 chapters and 1,427 quotes.
Upstream's own P&P test file (chapters 19–26) is pooled with `pp_full.xml` before splitting, so its
chapters fall on both sides. One novel cannot support any claim beyond Austen. EWT uses its official
dev/test split. Tune only on dev; test scores are regression measurements. Splits must stay fixed when adding detectors.

Excluded datasets:

- **QuoteLi3 *Emma* and *The Steppe*:** the EACL paper says they "were taken from the CQSC corpus"
  (Elson and McKeown 2010), which Columbia distributes only under a non-commercial academic license.
  Only QuoteLi3's *Pride and Prejudice* files are downloaded. QuoteLi3 itself has no license; a
  third-party Hugging Face card's CC BY 4.0 tag is unsourced and not accepted. See the
  [license evidence](../../docs/STYLE-DATA-LICENSES.md#quoteli3-256).
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
| Lemma conventions | Unicode case folding, then `could → can`, `would → will`, `these → this`, `those → that` on both gold and candidate lemmas |
| Unsupported gold XPOS | `ADD GW AFX XX FW LS` excluded from lexical scoring and counted |
| Punctuation/symbols | Gold UPOS `PUNCT`/`SYM` excluded from lexical tag/lemma scoring; `is_punctuation` agreement reported separately (expected true for PUNCT, false for SYM), including per-class counts/agreement |
| Lexical conventions (diagnostic only) | gold subordinator (`UPOS=SCONJ`, XPOS IN) vs CC/DT/WDT/WP; WDT↔WP; NNPS↔NNS; RBR↔JJR |

`normalized_tag_any` is gold XPOS membership among distinct M1-normalized readings on aligned lexical
tokens. `normalized_lemma_any` uses the lemma conventions on the same lexical denominator. Unknown
words with empty readings are incorrect; there is no surface-as-lemma fallback. Raw exact tag/lemma
membership and first-reading equality over **all** aligned tokens remain diagnostics for comparison.
Neither reading order nor priority is promised by `analyze()`, so first-reading accuracy is not gated.
Tag/lemma membership is tested independently, not as a paired reading match.

Adding readings can inflate any-reading accuracy. The gate requires `single_normalized_correct`
(exactly one distinct normalized POS string, equal to gold) not to drop, and caps
`mean_distinct_normalized_tags_known`, computed only over lexical tokens with at least one reading.
The latter catches net ambiguity growth on already-ambiguous or wrong tokens even when any-reading
accuracy rises. A previously unknown token gaining one tag lowers this conditional mean, rather than
being penalized for moving from zero to one. The all-token mean remains diagnostic. Both means count
all distinct M1-normalized POS strings, including internal strings without a Penn mapping.
Lexical convention matches are printed as additional counts per pair and an inclusive accuracy, but
**never folded into the gated accuracy**. The no-reading share covers all aligned non-PUNCT/SYM tokens,
including unsupported gold tags, and has an upper bound gate.

`punctuation_flag_accuracy` is a **tokenization/convention diagnostic**, not model accuracy and not a
minimum gate: the flag is a pure function of Unicode punctuation classes. All 61 test disagreements
are Unicode-vs-UD conventions: `/`, `#`, `-` are UD SYM but Unicode punctuation; `<`, `>`, `~` are UD
PUNCT but Unicode symbols. Its value can change when tokenization changes.

### Sentence-start bias check

For each EWT gold sentence, analyze that sentence independently to isolate starts from boundary errors.
Project its first non-quote gold XPOS through the Google universal-12 Penn map (`X` for unsupported gold
tags), and apply tier 3's shared procedure to the prediction. Each candidate Penn reading is normalized
before mapping; duplicate universal candidates count once. Resolved-only shares use singleton starts;
fractional shares allocate `1/N` to each of N candidate universal tags. Unknown starts have no allocation.
Gold shares cover all gold sentences, including starts for which the prediction has no candidates,
so the diagnostic includes selection bias from unknowns as well as ambiguity.

Output reports gold, resolved-only and fractional shares, both biases in percentage points, and the
resolution rate within each gold tag. Per-tag biases remain diagnostics. The gate caps **total absolute bias** (sum of absolute per-tag
fractional biases), with an explicit 1 percentage point of slack above the measured 21.55954035 pp:
**22.55954035 pp**. It also floors the number of starts with candidates. This is a selection-effect
tripwire, not a per-tag ratchet: small redistributions or newly known starts can pass, and the bound
is not automatically tightened when results improve. It does not make EWT a fiction baseline.

### LitBank original prose

Sequentially align quotation-layer TSV tokens to `original/*.txt`, permitting only whitespace in the
gaps, then map inclusive annotation sentence/token endpoints to half-open original scalar spans.
Validate annotation text against its TSV tokens. The originals contain whole public-domain books;
**only the prefix through the last annotated token is scored**, so unannotated later speech cannot
become a false positive. Original spacing, wrapped lines and paragraph breaks are retained.

The Rust document probe calls `writemorebetter_style::dialogue_spans_with_options` on these original prefixes
with hard-wrapped paragraph mode.
Delimiters are included; a true positive requires all byte and UTF-16 endpoints to match exactly.
The tier 2 scorer validates file identifiers and counts duplicate predictions as extra false positives.
Counts are micro-aggregated over documents, including those with no quotations. These procedural
paired-quotation spans have no speaker attribution; scare quotes can be false positives and dash speech
is missed. The historical Python `quote_spans` helper remains for baseline fixture tests only. Its
measured baseline and the Rust comparison are recorded in the document measurements. The final
corrected Rust test floors are precision **85.2799%**, recall **91.8838%**, and F1 **88.4588%**
(1,234 exact matches, 1,447 predictions, 1,343 gold), above the historical Python floors.
These supersede the inflated pre-recovery floors; the review correction is explained below. EWT floors are unchanged.

### QuoteLi3 *Pride and Prejudice*

`pp_full.xml` (53 chapters) and `pp_test.xml` (chapters 19–26) are parsed as data with the standard
library XML parser. The document text is the concatenated character data of `<text>`, so every
`<quote>` element's start and end are exact scalar offsets into it; no tokenized text is aligned.
Chapters start at each `CHAPTER <roman> (<n>)` heading and run to the next, named `pp-01` … `pp-61`
by the parenthesized absolute number. Inline `__` separators and volume lines stay in the text. A
quote before the first heading or crossing a chapter, or a chapter in both files, fails the run.
Spans include their delimiters. The text uses straight quotation marks throughout. The product probe
and scorer are LitBank's (hard-wrapped mode, exact byte and UTF-16 endpoints, duplicates as extra false
positives, micro-aggregated over chapters, including chapter 12 with no quotes).

| Split | Precision | Recall | F1 | TP / predicted / gold |
|---|---:|---:|---:|---:|
| dev (14 chapters) | 97.5309% | 97.8328% | 97.6816% | 316 / 324 / 323 |
| test (47 chapters) | **98.5975%** | **98.5284%** | **98.5629%** | 1,406 / 1,426 / 1,427 |

The test values are the first measurement and are the floors in `tier1-baseline.json`; nothing was
tuned on either split. All misses trace to two causes. Quoted letters (Darcy's, Collins's,
the Gardiners', Lydia's and others) are annotated as one quote across paragraphs whose inner
paragraphs open without a quotation mark, so the product drops the span at the first paragraph
break (3 dev, 10 test). On test, 6 more misses follow from those letters: their closing mark opens a
span that swallows the next quote. A quotation nested in the same straight double mark as its
outer speech (`"You wanted me, I know, to say "Yes," that you might …"`) splits the outer span
in two (4 dev, 5 test). The remaining false positives are two `" -- "` fragments inside a letter
and one scare-quoted phrase in narration (`"very glad;"`) on test, and one empty `""` on dev where
a nested close meets the outer close.

### Multi-paragraph speech continuation

QuoteLi3's P&P gold has no conventionally continued speech: its 13 quotes that cross a paragraph
break are letters whose inner paragraphs open without a mark. Disabling continuation (an open
quote carried into a paragraph that starts with the same mark) therefore *raises* QuoteLi3 test
F1 from 98.56% to 98.98%, and LitBank's from 88.62% to 88.76%. Changes must not trade continuation
for QuoteLi3 points. `labels/continuation.jsonl` is a small original fixture scored by the same
probe and scorer in every tier 1 run: three spans of straight, curly double and curly single
speech carried across two or three paragraphs (with curly apostrophes, an em dash, an ellipsis,
U+2028 and an emoji), plus closed and unclosed controls that must not continue. Its floors in
`tier1-baseline.json` are 100% precision, recall and F1; with continuation disabled only 6 of 9
spans match and the gate fails. The Rust tests in `style/tests/document.rs` guard the rule as well.

## Tier 2 framework

```sh
python3 bench/style/run.py tier2 generate --out bench/style/out
# Run a detector against every file in out/documents.jsonl, including untouched controls.
python3 bench/style/run.py tier2 score --gold bench/style/out/answers.jsonl \
    --predictions bench/style/out/predictions.jsonl
```

The smoke run does all three steps. From each pinned book it skips at least the first 2,500 chapter
words in numeric chapter order, beyond LitBank's annotated opening excerpts, then selects 60 chapter paragraphs with at least
20 words and no adjacent repeated words. It repeats the eighth word in 40 of them and
leaves 20 untouched. These 120 passages are clean only with respect to the toy predicate, not adjudicated
for every future report. Output includes the answer key, source manifest, modified texts and predictions
under ignored `out/`. The toy detector flags adjacent case-insensitive exact repeated words separated
by whitespace. The smoke gate requires P/R/F1 = 1 across 80 injections and 40 untouched controls. All 120 clean source
passages were checked against the two LitBank annotated excerpts after numeric chapter ordering:
zero passages share a case-folded, letter-only six-gram with those excerpts.

Every answer or prediction is one JSONL object:

```json
{"file":"pride-001.txt","byte":{"start":38,"end":47},"utf16":{"start":38,"end":47},"pattern":"repeated_word"}
```

Offsets are zero-based, half-open and refer to the **final injected text**, not the clean source.
`file` is the exact manifest-relative identifier. All four offsets must match to receive credit;
UTF-16 counts code units and bytes count UTF-8. Pass `--manifest PATH` to select the document manifest;
the default is `documents.jsonl` beside the gold file. Gold and prediction files must belong to that
manifest, including untouched controls; unknown file identifiers are rejected before filtering.
`--report NAME` filters both gold and predictions by that report field, and **rejects every prediction
missing a nonempty string `report`** when the filter is supplied; predictions are never silently lost
because they omit the field. `--pattern NAME` selects a single pattern on both sides.
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

### Clichés and redundancies (#246)

`python3 bench/style/run.py cliches --gate` builds `bench/tools/wmb-cliches` (or takes `--probe PATH`) and
scores the cliché report's own tier 2 suite: 60 templates (35 positive, 25 negative) injected into the 120 passages above,
each template once at a passage's start and once at a narration sentence boundary near its middle, in both
hard-wrapped and default mode (240 documents). Rows are labelled `cliche|redundancy.narration|dialogue`, with
clean/injected pairs so matches already in the clean passage are excluded rather than counted. Floors are in
`tier2-cliches-baseline.json` (P/R/F1 1.0 on 140 injections and 100 negative controls). See [the report notes](../../docs/STYLE-CLICHES.md).

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
| EWT normalized tags per lexical token with readings | not measured | **1.58103** (upper-bound gate) |
| EWT lexical-convention inclusive accuracy | not measured | 92.1439% (diagnostic; not gated) |
| EWT gold lemma among readings | 91.6326% raw over all aligned | **95.4520%** normalized over lexical tokens |
| EWT no readings / aligned non-punctuation | not separated | 748 / 21,214 = **3.5260%** |
| EWT punctuation/symbol flag agreement | not separated | 97.8878% (2,888 tokens; Unicode-vs-UD conventions, diagnostic only) |
| EWT raw tag any / first (current cohort) | 80.9639 / 69.4530% on old cohort | 81.2879 / 70.0357% (diagnostic) |
| EWT raw lemma any / first (current cohort) | 91.6326 / 89.2973% on old cohort | 91.6438 / 89.3785% (diagnostic) |
| LitBank quote-span precision / recall / F1 | 84.6154 / 82.7252 / 83.6596% (tokenized lines) | **84.9962 / 83.0975 / 84.0361%** (original annotated prefixes) |
| LitBank exact matches / predicted / gold | 1,111 / 1,313 / 1,343 | **1,116 / 1,313 / 1,343**, 80 test documents |
| Tier 2 smoke P/R/F1 | 100 / 100 / 100% | **100 / 100 / 100%**, after the 2,500-word exclusion |

Current EWT start-share comparison (all 2,077 gold sentences; 1,833 starts have candidates, 1,304 are
resolved). Values are percentages; biases are percentage points. The per-tag values below are
diagnostic; the aggregate total absolute bias is 21.55954035 pp and the fixed gate is 22.55954035 pp.

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
alignment count/fraction, normalized tag/lemma accuracy, single-tag correctness and candidate-start
coverage; upper bounds cover no-reading share, conditional known-token mean tag count, and total
absolute fractional start bias with explicit 1 pp slack. Per-tag biases and punctuation agreement
are diagnostic only. Raw and lexical-convention metrics are diagnostic only. The tolerance is 1e-12 for floating
arithmetic; thresholds are never rounded or automatically rewritten. A rebaseline needs review of the
metric definitions, exclusions, alignment coverage, ambiguity and per-category differences, not only
headline scores. The README retains the initial numbers above to make this review change explicit.

## Current document detector after #261

The historical table above records #259's Python baseline. Current product-detector test floors are
precision **85.3205%**, recall **92.1817%**, F1 **88.6185%** (1,238 / 1,451 / 1,343 TP/predicted/gold).
The probe uses the crate's hard-wrapped paragraph mode for original LitBank text. Open quotes are
dropped at paragraph boundaries unless the next paragraph repeats the same opener; known leading
elisions after a curly left quote and all straight single openers before a word are tentative: they
require a punctuated single closer in the same paragraph. A double opener takes precedence; a
same-mark single opener before a word replaces them unless it begins a listed elision. A single
outer span is recovered if its punctuated closer follows the nested double's closer in that paragraph.
Tentative singles never continue into the next paragraph. The earlier 86.8887% precision claim was
partly an artifact of lost predictions after unclosed single quotes. The initial detector revision was triggered by a test-split
precision failure, then evaluated on dev; the subsequent recovery fix was prompted by review that
included test-split diagnostics. The #261 dev measurement was unchanged (411 / 475 / 422;
86.5263 / 97.3934 / 91.6388% P/R/F1), including after review. Test moved from 1,234 / 1,447 / 1,343
(85.2799 / 91.8838 / 88.4588%) through initial #261's 1,237 / 1,451 / 1,343
(85.2516 / 92.1072 / 88.5469%) to the current floors after the elision/nesting review fixes.
Full before/after results and revised floors are documented in
[STYLE-DOCUMENT-BENCHMARKS.md](../../docs/STYLE-DOCUMENT-BENCHMARKS.md).

## Echoes and repeated phrases (#245)

```sh
cargo build --release --manifest-path bench/tools/wmb-document/Cargo.toml
python3 bench/style/echoes.py --gate --probe bench/tools/wmb-document/target/release/wmb-echoes
```

The report's probe is `wmb-echoes`, a second binary of `bench/tools/wmb-document`. It reads JSONL
`{"file", "text"}` lines (`--hard-wrapped`, `--min-content-words N` optional) and writes one record per
echo (`report: "echoes"`, `pattern: "echo"`, `key`, `label`, two spans) and per repeated phrase
(`report: "repeated_phrases"`, `pattern: "repeated_phrase"`, `keys`, `labels`, every occurrence span), in the
grouped-span format above. `--benchmark` times the document pass and both reports.

`echoes.py` takes 40 passages from each pinned Standard Ebooks book: paragraphs of at least 110
words, after the first 2,500 chapter words, with at least four sentence boundaries (`Mr.` and similar
abbreviations excluded). The first 20 passages of each book are **dev**, the rest **test**; the two
splits use disjoint injected vocabularies. Each clean passage gets up to three injected copies,
written to the ignored `out/echoes/`:

- **echo:** two inserted sentences carry one invented word each, in different forms (`It was the
  lantern.` … `There were lanterns.`, `He knelt.` … `They were kneeling.`), in plain,
  curly-quote/em-dash and ellipsis variants. Every fourth passage uses a **homograph** instead: a verb
  use, a noun use and a plural (`He wound it. It was the wound.` … `There were wounds.`), giving two
  gold echoes. The invented words are absent from the passage (checked by stem, including related
  forms such as `wind` for `wound`). Their companions are function words, so these copies test
  recall, lemma families, window bounds and typography.
- **natural:** one lowercase word of five or more letters from the passage, not a listed function
  word, is copied as a one-word sentence (`Attention.`, `“Attention…”`, `Attention!`) 12–35 words
  after its natural occurrence. No word sharing its first four letters may appear from 55 words before
  that occurrence to 60 words after the copy. Gold is the natural occurrence and the copy. The copy is
  tagged in a new context and measured against real text, so a wrong tag, function-word decision or
  window count produces a spurious or missed flag.
- **phrase:** the same four- or five-word phrase with three content words is inserted twice as its
  own sentence, varied by plural noun, an inserted comma, or curly quotes (`The miller’s silver
  thimble.` … `“The miller’s silver thimble!”`).

In one passage in three, the echo and natural injections become **far controls**: the pair is at
least 70 words apart (natural: no prefix match within 75 words before or 60 after the copy) and has
no gold record. Clean flags are shifted by the inserted lengths and matched by exact span set; the
remaining injected flags go to the tier 2 scorer with `--report echoes` or `--report
repeated_phrases`. Natural flags in the clean passages and clean flags lost after injection (an
insertion can push a natural pair beyond the window) are printed as diagnostics, not scored.

Results (English 6.6 tokenizer, 2026-10-15, after the PR #265 review), 319 documents:

| Split | Echoes (all) | Invented-word echoes | Natural-word echoes | Phrases | Spurious flags on far controls |
|---|---:|---:|---:|---:|---:|
| dev | 60/60, 60 new flags | 34/34 | 26/26 | 40/40, 40 new flags | 0 of 28 |
| test | 62/62, 62 new flags | 36/36 | 26/26 | 40/40, 40 new flags | 0 of 28 |

The clean passages contain 514 natural echoes and 61 natural repeated phrases; 15 natural echoes are
lost after injection. `echoes-baseline.json` sets precision, recall and true-positive floors per split
for all echoes, natural-word echoes and phrases. The gate also fails on any spurious flag on a far
control.

Before the review the report found 51/60 dev and 53/62 test echoes on this suite. It missed the
homograph plurals and five natural copies; the fixes are described in
[STYLE-ECHOES.md](../../docs/STYLE-ECHOES.md#keys-and-ambiguous-tokens). The original suite (now the
dev vocabulary) drove the per-spelling key change and found a suite bug, sentence splitting after
`Mr.`, so dev is not held out. The test vocabulary and the natural-word injections were written after
the review fixes and run once, with no tuning since. Keep test untouched when tuning further.

## Dialogue tags (#249)

```sh
python3 bench/style/run.py dialogue-tags tier2
python3 bench/style/run.py dialogue-tags tier3 --self-test
python3 bench/style/run.py dialogue-tags tier3 --export /path/to/export/out   # owner only
```

`--probe PATH` selects an already-built `dialogue_tags` binary (built from `bench/tools/wmb-document`,
`src/bin/dialogue_tags.rs`); `--tokenizer PATH` selects the data. The probe reads `file`/`text` JSONL
and calls `analyze_document` with default (line) paragraphs, then the report. It emits one record per
dialogue span (`report: dialogue_span`) and, per tag, records for `dialogue_tag_position`
(`before`/`after`/`split`) and `dialogue_tag_verb` (`speech`/`non_speech`/`unlisted`) on the verb
token, `dialogue_tag_adverb` on the adverb token, and `dialogue_beat` (`tag`/`beat`) on the first word
of the narration clause. Each category is scored separately with tier 2's exact-span scorer.

**Model-labeled set (gate: two model labelers, coordinator-adjudicated).** Four pinned chapter files from Standard Ebooks (CC0 dedication; underlying works
public domain in the US; none is in LitBank) are added to `sources.json`. `dialogue_tags.py` joins the
listed paragraph windows one paragraph per line:

| File | Source | Paragraphs | Split | Dialogue spans |
|---|---|---|---:|---:|
| `styles-1.txt` | Christie, *The Mysterious Affair at Styles*, ch. 1 | 12–69 | dev | 46 |
| `gatsby-1.txt` | Fitzgerald, *The Great Gatsby*, ch. 1 | 34–71 | dev | 46 |
| `sign-1.txt` | Doyle, *The Sign of the Four*, ch. 1 | 3–33 | test | 47 |
| `avonlea-1.txt` | Montgomery, *Anne of Avonlea*, ch. 1 | 0–49 | test | 47 |

`labels/dialogue-tags.jsonl` holds 238 records (70 tags, 10 beats) over those 186 spans, with
verbatim `text` that the suite checks against the offsets. These are **model labels**, not hand labels:
two sessions of the same model family as the detector's author (`claude-opus-5-5/a` and `/b`), each
given only the guidelines and a worksheet with the detector's quote spans marked. They labeled every
quote before the detector was run on the passages. They agreed on 80 of 81 clause records and on
every position, verb class and adverb. The one disagreement, on `avonlea-1.txt` paragraph 6, was
whether `His farm had been bought…` is a beat for `“a crank,”` in `… “a crank,” Mrs. Rachel Lynde
said.` The coordinator adjudicated it as **no beat**: the farm sentence does not touch the quotation,
and the quote has its own tag. The CI gate on this suite therefore rests on **two model labelers,
coordinator-adjudicated**; the labels are still model labels, not hand labels.

The test passages are not untouched held-out data. The author read all four passages while building
the worksheets, before the rules and docs were final; both labelers' notes on scare quotes, covering
all passages, motivated the embedded-quote rule; the docs' examples echo test text; and four
speech-list verbs (`splutter`, `remonstrate`, `implore`, `snap`) occur in the gold only on test. The
dev passages were inspected after the first detector run and their two misses were not tuned away.
Review of #266 then cited the test adverb miss (`told Marilla Cuthbert indignantly`); the generic fix
(an adverb after a pronoun or short name object) moved test adverb recall from 90.00% to 100%. Read
the test column as regression measurements, as with #261's LitBank numbers, not as a generalization
claim.

| Category / pattern | Dev P / R | Test P / R | All P / R (TP / predicted / gold) |
|---|---:|---:|---:|
| position `after` | 100 / 100 | 100 / 100 | 100 / 100 (50 / 50 / 50) |
| position `before` | 100 / 80.00 | 100 / 100 | 100 / 85.71 (6 / 6 / 7) |
| position `split` | 100 / 100 | 100 / 100 | 100 / 100 (13 / 13 / 13) |
| verb `speech` | 100 / 96.30 | 100 / 97.62 | 100 / 97.10 (67 / 67 / 69) |
| verb `non_speech` | 100 / 100 | none | 100 / 100 (1 / 1 / 1) |
| adverb | 100 / 100 | 100 / 100 | 100 / 100 (18 / 18 / 18) |
| beat vs tag: `tag` | 100 / 96.43 | 100 / 100 | 100 / 98.57 (69 / 69 / 70) |
| beat vs tag: `beat` | 100 / 83.33 | 66.67 / 100 | 81.82 / 90.00 (9 / 11 / 10) |

`dialogue-tags-baseline.json` stores the full-precision P/R/F1 for every gold pattern on all four
passages as floors; the suite fails when any falls. They are regression tripwires, not accuracy
estimates: counts are tiny (non-speech 1, before 7, beats 10, adverbs 18), so one new false positive
trips a 100% precision floor. Unlisted predictions are not gated directly (one occurs, on `flashed`). The Rust fixtures in `style/tests/fixtures/dialogue-tags.json` cover the
issue's acceptance list (tags before and after, split dialogue, beats, non-speech tags, an untagged
run) with curly and straight quotes, curly apostrophes, em dashes, ellipses and U+2028.

**Tier 3.** `dialogue-tags tier3 --export DIR` reads the same export layout as tier 3, sends each unit
to the probe's `--counts` mode and pools counts per `author_key`. It prints totals, per-author means
and percentiles (authors weighted equally) of `said_asked_percent`, `adverb_percent`,
`non_speech_percent`, `unlisted_percent`, `tagged_span_percent` and `beats_per_100_spans` (both over
speech spans, which exclude embedded scare quotes),
`untagged_run_exchange_percent` and `tags_per_1000_words`, tag position shares, and the share of tags
for each verb on the crate's lists. Unlisted verbs are counted, never named, so no corpus text
reaches the output. Errors exit with a generic message and probe stderr is discarded; `run.py`
also suppresses argument errors for any command line that contains `tier3`, in any position. `--self-test`
builds a synthetic export in a temporary directory and checks counts, author weighting and that no
metadata leaks. CI runs only the self-test. No private corpus or fiction-baseline export was read
for #249; the published-fiction baseline is for the owner to produce with this command.

## Sentence reports (#244)

`python3 bench/style/run.py tier2 sentences --gate` runs the actual Rust reports through the persistent
`wmb-document --reports TOKENIZER [--hard-wrapped]` probe. Its default is **one paragraph per line**;
wrapped Moby imports explicitly select `--hard-wrapped`. `--document-probe`, `--tokenizer`, `--out` and
`--output` select executable, engine data, generated cases and the aggregate score artifact. Reports
share one full document per probe request; no grammar rules are loaded. The probe runs
the two per-report Rust functions with default options, bundled baselines and the long-sentence lens
enabled for evaluation. It prints `{"reports": {"sentenceLength": ..., "sentenceStarts": ...},
"reports_ms", "paragraphs"}` with native spans; each result owns its `sentences` table, and long
sentences and run members index that report’s own table. `wmb-document --defaults` prints
the default options and run thresholds (`longSentenceWords`, `monotonyMinSentences`,
`monotonyMaxSpread`, `sameWordMinSentences`), and the independent Python reference reads them from
there rather than hardcoding them, so a change to a default fails as a deliberate decision. The gate
also checks that the narrative Moby cut, with default options and bundled baselines, yields a length
mean difference and all 11 universal differences (`bundledBaselineComparison`).

### Labels and adjudication

The original 332 Standard Ebooks rows in `labels/sentence-starts.jsonl` are now **two-labeler with
coordinator adjudication**. Before adjudication at head `1ace3bc`, the implementing Codex labeler and
an independent Claude labeler agreed on **305/332 (91.87%), Cohen's kappa 0.873**. The second labeler
flagged 76 rows ambiguous; all 27 disagreements were among those rows. It wrote labels before opening
existing labels, Rust code or predictions, but read the definitions document, which doubles as the
classifier rule table: it was **not fully blind to the heuristic**.

The coordinator adopted nine semantic corrections (After all x2, As a professional man, Much as,
Now that, Oppressively competent, This time, Ten to one, There you go), and decided that the 15
verbless NP fragments plus two Nonsense utterances belong to **other**, not subject. Existing five
subjectless verb/imperative examples also use other. Each record preserves the independent label and
ambiguity flag and records adjudication. The main sentence-start gate is **no longer provisional**:
`sentence-floors.json` is rebaselined against these corrected labels. These are development/regression
agreement figures, not held-out accuracy; rules were tuned against these development rows. Row-specific
`ten to one`, `this time` and fronted RB+JJ rules have been removed; any resulting gold-score losses are
accepted in the new floors. The set and fixtures were used during classifier development.

The initial sample consists of 150 analyzer sentence spans from each book, minus two chapter-heading
fragments, plus 34 implementer-selected rare gerund/infinitive/participle openings. It remains skewed
and rare categories have small denominators. Full P/R/F1 and confusion counts are committed in
`sentence-measurements.json`. Definitions and the classifier's limits are in
[STYLE-SENTENCES.md](../../docs/STYLE-SENTENCES.md).

A **separate** 34-sentence dialogue-heavy diagnostic sample in `sentence-starts-modern.jsonl` comes
from this repository's invented public `story1.txt` and `story2.txt`; records carry exact source blob
IDs and source/local offsets. It is still single-labeler and is not pooled into the adjudicated floors.
`style/tests/fixtures/sentence-start-review.json` covers the review's imperative/fragment cases and
positive counterparts; these are strict product fixtures, not inferred labels. The fixtures are regression examples used during the fixes, not held-out accuracy; no unit test
requires matching every adjudicated development-label row.

### Public sources and injected suites

Standard Ebooks sources are *The Blue Castle* (1926), pinned at
`08942ee29ed0abd918f7c47268806bdeb5f6188b`, and *The Murder of Roger Ackroyd* (1926), pinned at
`e3f77eb999de92aa16afcaf6f145c40677927bed`. Neither book appears in the pinned LitBank set.
`sources.json` records archive SHA-256 values, URLs and CC0 dedication links; the underlying works
are public domain in the US. Numeric chapter order and `<p>` text joined with two line feeds define
the initial four-chapter source offsets; curated records instead address a named chapter/paragraph.
`verify_labels()` checks every excerpt and endpoint. Public-domain excerpts are an explicit exception
to the older harness policy against committing downloaded text. No private corpus was accessed.

Forty simple source sentences generate three separate structural injections: four repetitions for
monotony, three for same-word starts, and twelve concatenated copies with internal terminal punctuation
removed for long sentences. Controls are now **40 untouched natural passages with at least four
sentences**, not trivially short single sentences. An independent Python reference enumerates maximal
pairwise-within-two-word windows, long spans and same-word runs from analysis tokens. Naturally
occurring control patterns are included in gold and printed as `naturalDiagnostics`; they are not
assumed absent or called writing errors. Exact byte/UTF-16 scoring counts duplicate predictions as
false positives. Each modified suite scores its own target plus all natural controls.

The universal map agreement check includes all Penn mappings, normalization variants, ORD,
`_apostrophe_contraction_`, PCT and unknowns. Unsupported tags remain unmapped in both implementations.

### Natural distribution sanity gate

The distribution gate uses each public input's actual paragraph convention. `story1.txt`–`story5.txt`
are LF, one paragraph per line: `hardWrapped: false` (96/96/96/93/86 paragraphs). Moby is hard-wrapped
CRLF: `hardWrapped: true`, with CRLF preserved. The probe returns its actual paragraph count; the
harness independently checks the count against the selected source layout, and a regression test
fails if the two modes are swapped.

The Moby gate measures narrative **from the second exact `CHAPTER 1. Loomings.` line onward**. The
first occurrence is a contents entry; the second starts at **UTF-8 byte 28,824**. Matching is by text,
not a hard-coded offset; the marker, occurrence and measured offset are recorded in the artifact.
This excludes contents, etymology and extracts, but retains the narrative chapter headings. The cut
contains 120,267 bytes, 202 blank-line paragraphs and 878 sentences. It is separate from the timing
workload, which remains the complete unchanged 149,091-byte file.

Stories have **16/615 other (2.60%)**, ceiling **5%**. Narrative-only Moby has **130/878 other
(14.81%)**, ceiling **16%**, a 1.19 percentage-point margin. Top mutually exclusive narrative other
surface shapes are 26 remaining verb-tagged openings (including literary/archaic constructions),
26 vocative/interjection shapes, 16 command-like openers, 14 short fragment shapes, 12 chapter-number
headings, 10 article-led starts, 2 punctuation/markup-led starts, and 24 remaining openings. These
are structural diagnostics, not independently adjudicated grammatical labels. Full counts/shares,
source modes, paragraph counts and cut provenance are in `sentence-measurements.json`; ceilings are
in `sentence-floors.json`. The sanity check complements the development F1 floors and makes no
held-out accuracy claim.

### Owner-only length calibration

```sh
python3 bench/style/run.py tier3 --sentence-length --self-test
python3 bench/style/run.py tier3 --sentence-length --export /approved/export
```

The export has **one paragraph per line**, with blank scene breaks. Calibration uses the report's
own `is_word && !is_skipped` counting in default line mode. The self-test includes consecutive
unpunctuated heading/narration lines and interrupted dialogue, and fails if hard-wrapped mode merges
them. Authors pool units first; baseline means and within-author sentence percentiles average authors
equally. `authorMeanPercentiles` is a separate author-distribution diagnostic. Output is numeric
aggregate JSON only, with all error/argparse details suppressed whenever tier3 appears anywhere in
argv. Sentinel tests cover invalid options before and after the subcommand.

The owner supplied the actual calibrated provenance: 2026-10-15,
fiction-baseline `b464cac1193a3555957fa2784a1ddfd553de505b`, EXTRACTOR_VERSION 1, 210 units,
71 author units, 663,457 sentences. `style/data/baselines/sentence-length.json` now records the
12.900118525896554-word mean and separate sentence/author-mean percentiles. The earlier hard-wrapped
calibration is invalid. No worker read private exports, titles, authors or unit IDs.

## Flag-rate calibration (#241)

```sh
python3 bench/style/run.py tier3 --flag-rates --self-test
python3 bench/style/run.py tier3 --flag-rates --export /path/to/fiction-baseline/out --diction-table /approved/ratings.json   # owner only
```

This measures how often every style report flags published fiction, at the scale Subplot runs the
reports: per chapter and per manuscript. It reads the tier 3 export layout (`units.jsonl` plus
`units/<unit_id>.txt`, one paragraph per line, blank lines at scene breaks) and uses only
`author_key` (a nonempty string) and `unit_id`. The probe `flag_rates` in `bench/tools/wmb-document`
(built automatically; `--probe PATH` selects a prebuilt one) builds each document with default
`DocumentOptions` (line mode, token details, no skipped names). It runs the six original reports with their
default options: sentence length and sentence starts with the shipped
`style/data/baselines/sentence-length.json` and `sentence-starts.json`, echoes, repeated phrases, the
bundled cliché/redundancy list, and dialogue tags (no dialogue-tag baseline ships). The probe returns
counts only.

Output is one JSON object of numbers. Its keys come from a fixed allowlist: structure names, report
categories, the percentile, sweep and phrase-length digits in use, and the reports' **public list
vocabulary** (bundled cliché entries and sources, listed dialogue speech/non-speech verb lemmas),
which the probe prints with `--vocabulary`. Any other key fails the run, so echo keys,
repeated-phrase keys, unlisted verbs, matched text, titles, author names, unit ids and paths cannot
reach stdout. Probe stderr is discarded. Every argparse or runtime error, including interrupts,
prints only `calibration failed; details suppressed to protect source metadata` whenever argv
contains `tier3`, an argument starting with `--fl` or `--ex`, or any abbreviation of `--export` or
`--flag-rates` such as `--e` (with or without `=value`). The `tier3` subparser also rejects
abbreviated options (`allow_abbrev=False`).

### Two scales

- **`chapter`**: each unit is cut into chapter-sized **windows**, and every report runs on each
  window as its own document. The export has no chapter markers, so the rule uses its scene
  structure. Walk the unit's paragraphs, counting words as whitespace-separated tokens containing a
  letter or digit. Close the current window **at a scene break once it holds at least 3,000 words**,
  or **at any paragraph boundary once it holds at least 6,000 words**. A final remainder under 2,000
  words merges into the unit's previous window, so windows normally hold 3,000–6,000 words (a few up
  to about 8,000). A unit shorter than 2,000 words is a single window. Scene breaks inside a window
  are kept as blank lines. Samples are windows, weighted **1 / (that author's window count)** so
  every author counts equally. `window_rule` echoes the three thresholds.
- **`manuscript`**: each whole unit runs as one document, and its counts are pooled within its
  author, as in the first version of this command. Samples are authors, one each.

Each section reports `samples`, `sample_words` (distribution of document words), and the same
`reports` block. Repeated phrases (no window; a phrase counts once it occurs twice anywhere) and, to
a small degree, echoes (function words are decided per document) **depend on document length**, so
use `chapter` rates for chapter-level thresholds. The other flags count local patterns and barely
change between scales.

### Definitions

- **Distribution** (`mean`, `percentiles` 5/10/25/50/75/90/95, `samples`, `authors`): a weighted
  mean, and weighted percentiles by linear interpolation between the samples' cumulative-weight
  midpoints, rescaled so the smallest sample sits at 0 and the largest at 100. With equal weights (the `manuscript` section) this
  is exactly the `(n−1)·p/100` rule used elsewhere. Samples whose denominator is zero (for example,
  no dialogue) are left out of that distribution only, and the remaining samples are reweighted so
  each contributing author still counts equally.
- **`per_1000_words`**: flags × 1000 / words for each sample.
- **`flagged_share`**: the weighted share of samples with at least one flag in that category, among
  samples with a nonzero denominator. In `chapter` this is the share of windows the report would
  flag; in `manuscript` it is the share of authors.
- **Narration and dialogue.** Only sentence length, sentence starts and clichés distinguish them.
  Sentence reports assign a whole sentence by its first word, and their denominator is the words in
  that group's sentences (`is_word && !is_skipped`). Clichés assign each match and word by its own
  position (the report's `narration`/`dialogue` word counts). `all` is both together.
- **Top categories / top patterns**: `{name: rank}` for the 10 highest mean per-1,000-word rates,
  rank 1 highest, rate above zero. Names are categories or list vocabulary, never matched text.
- **`defaults`** echoes the report option defaults in force. **`run`** gives wall-clock `seconds`
  from probe start (after any `cargo build`) through aggregation, `stage_seconds` summed per report
  inside the probe over both scales, and peak RSS in bytes of the probe process (`os.wait4`) and of
  the Python harness.

What counts as one flag:

| Report | Flags | Categories / patterns |
|---|---|---|
| Sentence length | each long sentence (≥ `longSentenceWords`, default 40) and each monotony run | `long_sentence`, `monotony_run`, `total` per group |
| Sentence starts | each same-word start run (≥ 3 sentences) | `same_word_run` per group |
| Echoes | each opt-in echo (default and maximum window 30) | `distance` band in words (`1_5`, `6_10`, `11_25`, `26_50`); `form`: `same_form` if the two words have the same case-folded spelling, else `different_form` |
| Repeated phrases | each distinct repeated phrase; `occurrence` counts every occurrence separately | `length` in words (merged phrases can exceed 5; numeric length keys are allowed) |
| Clichés | each match | `kind` (cliché/redundancy) × group, `source` list, `patterns` per bundled entry with at least one flag (`patterns_with_flags` counts them) |
| Dialogue tags | each tag with a listed speech verb other than *say*/*ask* (`other_speech_verb`, a legacy calibration category; UI flags use showy verbs), each non-speech tag, each tag adverb, each untagged run (> 12 exchanges) | those four categories; `patterns` per listed verb lemma except *say*/*ask*. One tag can carry two flags (verb and adverb). Unflagged context (`tags`, `said_asked`, `unlisted_verb`, `beats`) is under `context_per_1000_words`, in the same shape |

### Baseline measures and comparisons

Sentence length and sentence starts compare a document with their shipped baselines. The report
returns a measure only when the group has at least `minimumSentences` (50) sentences; that sample
(a window, or an author's pooled manuscripts) is then **eligible**.

- **`measures`** gives, for `all`/`narration`/`dialogue`, the distribution over eligible samples of
  the measures themselves, at the 5/10/25/50/75/90/95th percentiles:
  - sentence length: the sample's `mean` and its 5/25/50/75/95th sentence-length percentiles, in
    words, with the report's interpolation at `(n−1)·p/100` over its sentence histogram;
  - sentence starts: each universal-12 tag's share (fractional count ÷ (sentences − unknown starts)
    × 100) under `universal`, and each fine category's share (count ÷ sentences × 100) under `fine`.

  In `chapter` these are per-window distributions; in `manuscript`, per-author distributions. A
  band such as "outside the published 5th–95th range" can be read straight from these percentiles.
- **`baseline_comparison`**: per statistic, or per universal tag that the baseline has (the `.` tag
  has none), `difference` = measure − baseline over eligible samples (words or percentage points);
  `eligible_share`, the weighted share of all samples that are eligible; and
  `share_abs_difference_at_least`, the weighted share of all samples whose |difference| is at least
  0.5/1/2/3/5 words (length) or 1/2/5/10 percentage points (starts). Fine categories have no shipped
  baseline, so their comparisons appear only if a `categoryPercent` baseline is shipped.

Reports return differences on eligible documents and highlight only outside the matching p5–p95 band. The calibration adapter retains all comparison values for measurement. So
`eligible_share` in `chapter` is the share of windows on which the report would show a comparison,
and the threshold sweep is there to choose one. In `manuscript`, `eligible_share` tests each
author's pooled sample against the 50-sentence bar, which is nearly always met. The self-test checks
the harness's histogram mean and percentile differences and universal percentages against the
report's own `meanDifference`, percentile differences and `percent` on one document.

### Cost and self-test

Each unit is analyzed twice, once whole and once as windows. A single pass costs about 45 µs per
export word on an idle Apple M5 Pro (two synthetic 501,054-byte units, 180,780 words, took 8.3 s
before windows were added), almost all of it in document construction, and window passes cost the
same per word. The full run therefore takes about 90 µs per word: about 13 minutes for an export of
8.6 million words (663,457 sentences at 12.9 words), and longer on a loaded machine. Peak probe RSS
depends on the longest unit: 170 MB at 90,000 words.

`--self-test` generates seeded synthetic text in a temporary directory (four units, three authors,
dialogue, clichés, long sentences, monotony and start runs, scene breaks, a made-up sentinel word
repeated in the text) with sentinel strings in every metadata field. It asserts the window rule,
the cross-check above, several expected shares at both scales, numeric-only output, and that no
sentinel string or temporary path appears in the output. `test_flag_rates.py` covers author pooling
and equal author weighting at both scales, weighted percentiles, the window rule, histogram
percentiles, rejection of unlisted probe keys and of non-string author keys, and suppression of
argparse and runtime errors in thirteen argument shapes, including `--e PATH` with no subcommand.
CI runs only the self-test; no private corpus or export was read to build this.

## Remaining #256 scope: lexical suites and owner obligations

```sh
python3 bench/style/lexical.py --split dev
python3 bench/style/lexical.py --gate
```

`lexical.py` uses the shared tier 2 exact-byte/UTF-16 scorer and pinned Austen/Melville
paragraph generator. Each book's paragraph indices 0–29 are development and 30–59 are test;
these splits are fixed before measurement. Generated texts, manifests, answer keys, predictions
and natural-paragraph predictions are written under ignored `out/lexical/`. The persistent
`lexical_reports` Rust probe loads only the tokenizer, bundled overused-word baseline, bundled
sensory norms, and a caller-supplied diction table. It changes no report logic.
`diction-synthetic.json` contains twelve invented ratings (project MIT/Apache-2.0); they are
mechanical test values, not human concreteness norms. No Brysbaert data is used. All report
options are shipped defaults.

Every public paragraph gets the same injected cases, each appended to the untouched paragraph:

| Report / category | Positive injections (gold) | Controls that must stay silent |
|---|---|---|
| Overused words | five occurrences each of *glanced*, *shrugged*, *blinked* (the default minimum count) | four occurrences of each; five *said* in a 240-word padded text, whose ratio to the Ngram rate is below 4 |
| Fillers | *very*, *just* and clause-opening *that* in `It was very cold. She just left. He knew that she had gone.` | adjective *just* and determiner *That* |
| Sensory balance | one rated word per modality: *red*, *music*, *cold*, *stench*, *salty*, *nausea* | figurative *looked like* / *felt that*, flat adverb *pretty late*, the excluded lemma *speech*, and sight word *beautiful* whose margin (2.3) is below the sight margin |
| Diction sentences and runs | three abstract sentences (mean 1.5) forming one run; one near-threshold sentence (mean 2.4); abstract, abstract, concrete, abstract (three sentences, no run) | three concrete sentences; the abstract sentence as dialogue; a sentence with mean 2.6; a sentence with two rated words |

Overused cases skip a paragraph that already contains that lemma, and the ratio control skips
paragraphs that contain *say*. Curly apostrophe, em dash, ellipsis and U+2028 precede injections
to exercise both offset units; the Standard Ebooks paragraphs themselves supply curly double
quotes. Diction gold sentence spans include trailing inter-sentence spaces, matching the
documented analysis span convention.

Natural text is not presumed flag-free, and the gate checks it two ways. First, **every**
prediction of every category on the 60 untouched paragraphs of the split is frozen in
`lexical-baseline.json` as per-category counts plus a SHA-256 of the sorted
`(paragraph, report, category, byte span)` list; any change fails the gate. Second, each injected
case is paired with its untouched prefix: predictions before the insertion boundary must match the
prefix-only result as a multiset, and **all** predictions after the boundary are scored against the
answer key, so unexpected suffix flags and duplicate spans count as false positives and wrong
modalities populate the confusion matrix. The frozen natural predictions are a regression lock, not
a judgment that they are right; a deliberate report change rebaselines with
`python3 bench/style/lexical.py --split test --write-baseline` (and `--split dev`), and the PR
records the count change. A tokenizer or document-analysis change that moves these flags also
needs a rebaseline. Test paragraphs are disjoint from dev paragraphs, but vocabulary and templates
are shared; these are regression scores, not held-out linguistic generalization claims.

Measurements (`lexical-baseline.json`; test unless noted). Every category scores 100% precision,
recall and F1 on both splits:

| Report / category | Test gold (dev) |
|---|---:|
| Overused words: glance; shrug; blink | 295; 300; 300 (300 each) |
| Fillers: very; just; that | 60 each (60 each) |
| Sensory balance: each of six modalities | 60 each (60 each) |
| Diction: abstract sentences; abstract runs | 420; 60 (420; 60) |
| Cases / paired untouched-prefix comparisons changed | 1,534 / 0 (1,545 / 0) |
| Frozen natural-paragraph predictions | 217 (223) |

The test natural predictions are 140 sense words (93 of them sight), 70 fillers and
7 *come* overused flags; development has 164 sense words, 46 fillers and 13 overused flags.

#282 removed the single-sense scene flag, its three injected cases per paragraph (a 90% positive,
80% and below-minimum controls) and its category, and rebaselined both splits. Its sensory fixes
also moved the frozen sense words: test drops the speech tags *exclaimed* and *growled* and adds
participles *singing* ×2 and *marked*; development drops *complained* (below the exact 4.0 strength) and adds *withered*
and *flew*.

Mutation check (2026-10-16; throwaway edits to `style/src/reports/`, reverted and never committed;
each rebuilt `lexical_reports` and `flag_rates`, then ran this gate on the test split and the
flag-rates self-test):

| Mutation | Review run at `1f2c0dd` | This gate: what fails |
|---|---|---|
| Overused `minimumCount` 5→4 | caught | four-occurrence controls flag |
| Overused `minimumCount` 5→6 | missed | five-occurrence positives drop |
| Overused ratio test removed | missed | *said* below-ratio control flags |
| Filler detection disabled | missed | natural freeze; filler recall (controls alone); tier 3 self-test |
| Lancaster `h`/`o` letters swapped | caught | natural freeze; smell/touch confusion (controls alone); tier 3 self-test |
| Sensory excluded-lemma, figurative and flat-adverb filters removed | missed | natural freeze; sight controls (controls alone) |
| Sensory `sightMargin` 3.0→2.0 | missed | natural freeze; *beautiful* control (controls alone) |
| Diction `minimumRatedWords` 3→4 | caught | abstract positives drop; tier 3 self-test |
| Diction `abstractBelow` 2.5→4.0 | missed | mean-2.6 control flags |
| Diction narration-only rule removed | missed | dialogue control flags |
| Diction `minimumRunSentences` 3→4 (reviewer: tier 3) | caught (tier 3) | run positives drop; tier 3 self-test |
| Sensory `singleSensePercent` 85→99 (reviewer: tier 3) | missed | natural freeze; 90% scene drops (controls alone); tier 3 self-test (option removed in #282) |
| Sensory excluded-lemma filter alone removed | — | natural freeze; *speech* control (controls alone) |
| Sensory figurative filter alone removed | — | natural freeze; *looked like* / *felt that* (controls alone) |
| Sensory flat-adverb filter alone removed | — | natural freeze; *pretty late* (controls alone) |
| Diction `minimumRunSentences` 3→2 | — | broken-run case flags a two-sentence run |
| Sensory `singleSensePercent` 85→75 | — | 80% scene control flags (option removed in #282) |
| Diction `minimumRatedWords` 3→2 | — | two-rated-word control flags |
| Diction `abstractBelow` 2.5→2.0 | — | mean-2.4 positive drops |
| Sensory `minimumSenseWords` 20→21 | — | 20-word scene positive drops |
| Overused `minimumRatio` 4→3 | — | *said* below-ratio control flags |

All 21 fail the tier 2 gate (the review run caught 3 of the first 10 in tier 2 and 1 of 2 in
tier 3); the restored build passes. The natural freeze fails first for 8 of them, so those 8 were
rerun with the freeze bypassed: the targeted controls and positives fail each of them on their own.
Of the 12 reviewer mutations, the flag-rates self-test now fails 5. The suite does not catch a
change that leaves every injected boundary case and every natural flag on the 60 gated test paragraphs
unchanged, for example a filter edit that only matters for constructions absent from both.
The two `singleSensePercent` rows record the measurement at the time; #282 removed the option, the
flag and the cases that caught them.

CI's `English` job runs **Style lexical report accuracy gates (256)**; it gates the test-split
P/R/F1 per category, gold coverage, case count, the paired-prefix count and the frozen natural
predictions. Full confusion matrices are recorded in the baseline. The existing **Style benchmark
accuracy gates** step continues to gate the other merged reports, tier 1, and synthetic tier 3. No
transitions, pronoun-start or pacing suite is added here; #247 and #251 own those lanes.

### Every merged report in owner-only tier 3

The `--flag-rates` probe now includes overused words, sensory balance and diction alongside the
six earlier reports. Run the self-test as before. A real owner run must supply the actual diction
table chosen by the caller:

```sh
python3 bench/style/run.py tier3 --flag-rates --self-test
# Owner only; use an approved export and a legally usable caller-supplied table:
python3 bench/style/run.py tier3 --flag-rates --export /approved/export --diction-table /approved/ratings.json
```

The self-test alone uses the committed invented table. There is no synthetic fallback for a real
export. Overused output counts flagged lemmas, their occurrences and filler tokens; sensory output
counts sense words and each of six modalities; diction counts abstract
sentences/runs and pools rated-word sums/counts into a mean rating. Each count has an author-weighted
per-1,000-word distribution and flagged share at both chapter-window and manuscript/author scales.
For sense-word and filler counts, `flagged_share` means a nonempty diagnostic, not an editorial
warning. For diction, the chosen table determines coverage and interpretation. Manuscript samples
pool units by author; ratings are weighted by rated words within an author and equally across authors.
No matched lemmas, table vocabulary, paths or source text enter the output. The existing allowlist and
numeric-only validator apply. Synthetic coverage checks all three added reports (including four
filler words and 26 sight words of 29 sense words), author pooling, rating weighting and leak rejection. No private inputs were accessed for this work.

### Transitions, pronoun starts and per-mode sensory shares

The probe also runs transitions and pronoun starts with default options, keeps sensory balance's
`all`/`narration`/`dialogue` groups, and re-runs two cheap sweeps on the same document: overused
words at `minimumRatio` 3/4/5/6/8 and transition repeats at `repeatWindow` 2/3/4/5. The added
output, at both scales:

- `transitions`: `flags_per_1000_words.repeats` and `repeat_window.<n>` per mode (a repeat's mode
  is its sentences' mode, rated per 1,000 words of that mode's sentences), and `measures` per mode:
  the distribution of `containing` and `opening` shares and of each type's `containing`/`opening`
  share, as percentages of eligible sentences, over samples with at least `minimumSentences` (50)
  in that mode; `eligible_share` is the weighted share of samples meeting it.
- `pronoun_starts`: `flags_per_1000_words.runs` per mode (after the `sameWordRuns` dedupe) and
  `measures` per mode for `pronoun_starts`, each of *i, you, he, she, it, we, they*, and `other`.
  The probe sums every other opener into `other`, so no unlisted word can reach the output.
- `sensory_balance.measures` per mode: `sense_word_density` (sense words per 1,000 counted words of
  that mode) and `modality_percent` for each modality over samples with at least
  `minimumSenseWords` (20) sense words in that mode.
- `overused_words.minimum_ratio.<ratio>`: flagged lemmas per 1,000 words at each ratio.

`--skip-diction` runs a real export without a concreteness table and omits `diction` from the
output; the owner has not chosen a table, so the 2026-10-16 run used it. `--diction-table` and
`--skip-diction` are mutually exclusive. `--self-test` uses `diction-synthetic.json` unless
`--skip-diction` is passed, and CI runs the self-test both ways. `published_bands.py AGGREGATE --dataset … --tool …
--run-date … --output …` turns a saved aggregate into the band file below; it reads only the
aggregate JSON. The unit tests cover the mode floors, author weighting, the bucketed pronoun
allowlist and the band builder on invented counts.

#### 2026-10-16 owner-run results

Run on the published-fiction export (`cmlarsen/fiction-baseline` b464cac, EXTRACTOR_VERSION 1):
210 units, 71 authors, 7,702,268 counted words, 1,559 chapter windows; 802 s wall clock, 664 s of it
document construction; probe peak RSS 275 MB. No text, titles, author keys or unit identifiers were
printed or kept; the aggregate stayed in the session scratchpad and was deleted after the numbers
below and the band file were recorded. Shares are author-weighted. "Chapter" is the share of
chapter windows with a flag and "authors" the share of authors whose pooled manuscripts have one;
rates are flags per 1,000 words over chapter windows, median (p5–p95). Every report ran with its
shipped defaults; no default changed except pacing (see [STYLE-PACING.md](../../docs/STYLE-PACING.md)).

| Report and flag | Chapter | Authors | Per 1,000 words |
|---|---:|---:|---|
| Sentence length: long sentences (≥ 40 words, every one) | 84.1% | 97.2% | 0.48 (0–1.07) |
| Sentence length: monotony runs | 53.7% | 84.5% | 0.16 (0–0.97) |
| Sentence starts: same-word runs | 64.3% | 93.0% | 0.19 (0–1.44) |
| Echoes (opt-in) | 100% | 100% | 20.36 (11.59–33.95) |
| Repeated phrases (opt-in) | 100% | 100% | 4.69 (1.57–8.83) |
| Clichés and redundancies | 59.8% | 85.9% | 0.16 (0–0.64) |
| Dialogue tags: all flag categories | 92.3% | 97.2% | 1.62 (0–7.48) |
| Dialogue tags: non-speech verbs | 15.7% | 53.5% | 0 (0–0.30) |
| Dialogue tags: manner adverbs | 62.6% | 84.5% | 0.28 (0–2.29) |
| Dialogue tags: untagged runs | 1.7% | 7.0% | 0 (0–0) |
| Transitions: repeats (opt-in) | 66.5% | 95.8% | 0.28 (0–1.44) |
| Pronoun starts: runs (opt-in) | 33.4% | 74.6% | 0 (0–0.48) |
| Overused words: flagged lemmas (opt-in) | 99.4% | 100% | 1.44 (0.42–2.88) |
| Sensory balance: sense words (no flag) | 100% | 100% | 33.80 (19.70–58.46) |

The long-sentence row counts every sentence of at least 40 words, not the report's top-three UI
selection, which caps a manuscript's flags (hence a 0.04 author median at manuscript scale). Dialogue
tags' "all flag categories" includes the legacy `other_speech_verb` count (every speech verb except
*say*/*ask*, 88.7% of chapters), not just the showy verbs the UI flags. Details and sweeps:
[transitions and pronoun starts](../../docs/STYLE-TRANSITIONS.md#published-fiction-calibration-2026-10-16),
[sensory bands](../../docs/STYLE-NORMS.md#published-fiction-bands-2026-10-16),
[overused words](../../docs/STYLE-OVERUSED-WORDS.md#published-fiction-flag-rates-2026-10-16).
[`published-bands-transitions-pronouns-sensory.json`](published-bands-transitions-pronouns-sensory.json)
holds the transitions, pronoun-start and sensory p5/p95 bands per mode.

### Label inventory and what the owner still owes

| Set | Size | Actual labeler provenance | Still owed |
|---|---:|---|---|
| `labels/sentence-starts.jsonl` | 332 sentences | Codex + independent Claude; coordinator adjudication; second model saw the category definitions, not existing labels/predictions | Owner adjudication/acceptance; independent human labels if “hand-labeled” is literal; untouched evaluation set (current rows were used in development) |
| `labels/sentence-starts-modern.jsonl` | 34 sentences | One implementing model; invented public story excerpts | Second independent labels and owner adjudication; diagnostic only, not part of the 332-row floor |
| `labels/dialogue-tags.jsonl` | 238 records over 186 quote spans (70 tags, 10 beats) | Two Claude sessions, coordinator adjudication; detector-marked quote worksheets | Owner acceptance; independent human labels if required; not 238 independent sentences and not held out |
| Transitions | No set on audited main | None | #247 lane plus approximately 300 public-domain sentences, two independent labelers and owner adjudication |
| `labels/pacing/*.jsonl`, `labels/pacing-consensus.jsonl` | 305 paragraphs in 40 passages | Five independent, blind model labelers (2026-10-16, owner's choice): DeepSeek V4 Pro and Muse Spark 1.3 via OpenCode Go, Codex gpt-6.1-sol, Claude Opus 5.5 (`claude-opus-5-5`) and Claude Fable 5.1 (`claude-fable-5-1`). Each saw only the packet built from the committed item file (`itemsSha256` in the consensus meta row) and guidelines version 1. Where each labeler ran was not recorded. Merged by 5-way majority with no coordinator adjudication. Guidelines version 1 is [`labels/PACING-GUIDELINES-v1.md`](labels/PACING-GUIDELINES-v1.md), sha256 `d7eaa315…db6`, the wording first committed as `bf832a2`; the consensus meta row records it as `guidelines` | Owner acceptance; independent human labels optional; not held out (the threshold sweep tunes on it) |
| Overused words, sensory balance, diction | No hand-labeled natural-text set | Deterministic generated answer keys only | Owner usefulness/calibration review; injections do not substitute for editorial judgments |

The small `labels/example.jsonl` (3 records), `documents.jsonl` and `excerpt.txt` demonstrate the
scorer format and are not an accuracy set. No second labeler or owner judgment was invented here.
The original #256 request for about 300 sentences **per judgment report**, independently labeled
and owner-adjudicated, is therefore still open. Private tier 3 measurements for newly merged reports,
the chosen diction table and its calibration, and owner publication/acceptance of aggregate numbers
also remain owner work. This PR references #256 rather than closing it. The requirement-by-requirement
audit is in [`AUDIT-256.md`](AUDIT-256.md) and the PR body.

## Pacing (#251)

```sh
python3 bench/style/run.py tier2 pacing --gate
python3 bench/style/run.py tier3 --pacing --self-test
python3 bench/style/pacing.py # public texts only; downloads pinned Standard Ebooks chapters to .cache
python3 bench/style/run.py tier3 --pacing --export /approved/export # owner only; 2026-10-16 run chose the defaults
```

The dedicated `bench/tools/wmb-document` `pacing` JSONL probe runs the actual Rust report;
`--document-probe` (tier 2) or `--probe` (tier 3) selects an existing executable. CI gates
`pacing-baseline.json`: 45 cases with 20 exact byte/UTF-16 gold run spans, using injected
backstory (three-paragraph, contracted Unicode, >400-word single paragraph, and hard-wrapped)
among untouched paragraphs from each of `bench/public/story1.txt`–`story5.txt`. Negative
controls cover untouched opening passages, action, dialogue, short exposition and scene breaks.
These are development/regression fixtures, not held-out literary accuracy. The injections use
one deliberately strong authored backstory passage, so perfect scores do not establish recall on
natural exposition, and they saturate every signal, so they cannot tell nearby thresholds apart.
The gate therefore also reruns the public texts (`story1`–`story5`, `moby150k` and the four pinned
Standard Ebooks chapters) at the defaults and requires each text's `runs` and `twoSignalParagraphs` to
match the recorded `public` block. Default thresholds and public flag distributions are recorded in
[STYLE-PACING.md](../../docs/STYLE-PACING.md).

Tier 3 is the owner-run calibration entry point; it ships no published-fiction baseline, but its
2026-10-16 run chose the defaults and is recorded in the `published` block of
`pacing-baseline.json`. The export layout is `units.jsonl` plus `units/<unit_id>.txt`, one paragraph
per line; `author_key` pools all of an author's units. Output is aggregate-only: pooled
per-paragraph signal distributions (mean and p0/5/25/50/75/95/100, each paragraph weighted
equally), equally weighted distributions of author signal means and of per-author flag rates (share
of paragraphs inside runs, runs per thousand words), and the share of authors with any run.
`eligibleParagraphSignals` gives author-weighted percentiles (each paragraph weighted 1 / its author's
eligible paragraphs) over paragraphs with no speech and at least one counted word, and
`twoSignalParagraphShare` the share of them meeting two thresholds. `chapter` reruns every unit in
the `--flag-rates` chapter windows (each window weighted 1 / its author's windows) and gives the
share of windows with a run and the distributions of runs per thousand words and run-paragraph
percent. `sweep.<length>.<pastPerfect>.<cognition>` repeats those chapter rates, the share of
authors with any run and the author runs-per-thousand-words distribution for 125 threshold settings
(the probe's `sweep` request field reruns the report on one built document). Null denominators are
omitted for that measure. There are no per-author rows or exact word totals. It emits no text,
titles, identifiers or author keys; a fixed output-key allowlist (decimal keys only for the recorded percentiles and, under `sweep`,
the swept threshold values) and
numeric-value check guard serialization, probe stderr is discarded, and run.py suppresses all tier 3
argument/runtime errors. CI runs only synthetic `--self-test`, which checks author pooling, window
weighting, a sweep entry and sentinel metadata suppression; the unit tests add a fake probe for the
sweep and eligible-paragraph aggregates. The 2026-10-16 owner run read the export only through this
command; see [STYLE-PACING.md](../../docs/STYLE-PACING.md#published-fiction-calibration-2026-10-16).

### Pacing hand-label set (#256)

```sh
python3 bench/style/pacing_labels.py packet /path/to/packet   # labeler packet: passages + blank template + guidelines
python3 bench/style/pacing_labels.py check                    # every item materializes from the pinned downloads
python3 bench/style/pacing_labels.py validate labeler.jsonl …
python3 bench/style/pacing_labels.py merge a.jsonl b.jsonl … --out consensus.jsonl --stats agreement.json
python3 bench/style/pacing_labels.py score --consensus consensus.jsonl [--gate | --write-baseline]
python3 bench/style/pacing_labels.py select                   # rebuilds items and strata; changes the frozen set
```

`labels/pacing-items.jsonl` holds 305 paragraphs in 40 passages of 6–10 consecutive paragraphs, one
record per paragraph: `id` (`p07.3`), `passage`, 1-based `paragraph`, `source` (a `sources.json` key or
`story1`–`story5`), `document` (the chapter file inside the pinned Standard Ebooks download, or the
story file), half-open `char`/`byte`/`utf16` offsets and the paragraph's UTF-8 `sha256`. No book text is
committed. A document's text is its body `<p>` elements outside headers, hgroups, footers, tables,
figures and asides, whitespace-collapsed, without noteref markers, joined by `\n`, with `\n\n\n` at
each `<hr/>`; the stories are used byte for byte. The packet materializer checks every offset and hash.

Passages come from nine full Standard Ebooks novels (five added for this set: *The Thirty-Nine Steps*,
*Treasure Island*, *The Hound of the Baskervilles*, *The Age of Innocence*, *Silas Marner*), the four
pinned first chapters, and story1–5: 13 books and 5 stories. Windows never cross a scene break, include
no blockquote paragraph (letters, verse) and hold at most 1,800 words. Selection is stratified on the
current report (defaults and the loose 15 / 10 / 10 setting): 14 passages around a reported run, 13
near-misses (two-threshold paragraphs but no run) and 13 ordinary passages, round-robin across
sources and shuffled with seed 251256. The stratum key is `labels/pacing-strata.json`. **Labelers must
not see it**, the item file or any report output; the packet contains only numbered passage text, the
blank `labels-template.jsonl` and [`PACING-GUIDELINES.md`](labels/PACING-GUIDELINES.md), which does not name
the repository or its tools. Give a labeler that can read files an empty directory holding only the
packet.

Labelers write one JSONL line per passage: per-paragraph `slow` (boolean) and `kind` (backstory,
exposition, interior, description, action, dialogue, mixed), and inclusive slow-stretch ranges with
severity 1–5; `validate` enforces the schema and that stretches cover exactly the slow paragraphs.
`merge` takes a strict majority per paragraph (ties stay `null` and are excluded from scoring), records
vote counts and labeler names, and reports Fleiss' kappa (`slow`, `kind`, passage has-stretch), pairwise
Cohen's kappa, the pairwise Jaccard of each passage's slow paragraphs and its mean, the vote splits
(`5-0`, `4-1`, `3-2`) and the share decided by one vote. Per labeler it records the number of slow
paragraphs, mean pairwise Cohen's kappa for `slow` and `kind`, Cohen's kappa against the other
labelers' strict majority (their ties excluded) and Fleiss' kappa without that labeler. A labeler is
flagged as the `outlier` when its mean pairwise slow kappa is more than 0.1 below the median of the
others'. `kind` takes the plurality; ties stay `null`. Consensus stretches are maximal runs of
majority-slow paragraphs with the median of overlapping labelers' severities. With no file arguments,
`merge` reads every `labels/pacing/*.jsonl` and writes `labels/pacing-consensus.jsonl` and
`labels/pacing-agreement.json`.

`score` runs the report over each whole document at defaults and the four named settings and scores
paragraph-level P/R/F1 for `slow`, plus run-level any-overlap and IoU ≥ 0.5 matching of runs clipped
to the passages. To give the report's F1 a human-like reference, it also scores each labeler against
the strict majority of the other four (`labelersAgainstOthersMajority`; 2–2 ties are excluded at
paragraph level and count as not slow when forming stretches) and the report against the same five
leave-one-out majorities (`againstOthersMajority`). `--gate` re-merges the committed labeler files,
checks the result equals the consensus file it scores (relative or absolute `--consensus`), and fails if any default paragraph or any-overlap
precision, recall or F1 drops below `pacing-labels-baseline.json`. CI runs it. A change to the
defaults that trades precision for recall needs a deliberate `--write-baseline`.

```sh
python3 bench/style/pacing_labels.py sweep --probe bench/tools/wmb-document/target/release/pacing
```

`sweep` scores the 350-setting grid in `GRID` (mean sentence length 0/10/12/15/18/20/25, where 0
turns the length signal off; past perfect and cognition 5/10/15/20/30 %; minimum paragraphs 2/3; the
400-word alternative is unchanged) against the consensus and the leave-one-out majorities, and writes
`pacing-labels-sweep.json` sorted by paragraph F1. Each row's `sourceFlags` runs the same setting over
every document of the 18 label sources (391 documents, 16,243 report paragraphs) and counts the runs,
the flagged paragraphs, flags per 1,000 paragraphs and the flagged paragraphs outside the labeled
windows (`outsideWindows`, `outsideWindowsShare`). The labels say nothing about those paragraphs, so a
setting's precision only covers the in-window share. It takes about fifty minutes with five probes.

The 2026-10-16 labels were made under guidelines version 1. The labelers reported ambiguities about
habitual or iterative summary, backstory or recap that is the scene's point, and opening exposition
before any scene. Version 2 (`PACING-GUIDELINES.md`) gives rules for all three, judges passage edges
and story endings from the text rather than from where the excerpt was cut, and drops the script
path; the set was not relabeled. `LABELED_GUIDELINES` in `pacing_labels.py` names the version the
committed labels were made under, and `merge` records it in the consensus meta row. A labeler added
to this set must work from version 1; a relabel under version 2 replaces all five files and updates
`LABELED_GUIDELINES`, never mixes the two.

Known contested items. The #293 review read six passages against the consensus and disagreed with
three; the labels stand as merged, and these verdicts carry the most reference noise:

- **p21** (story5, consensus stretch 7–9, severity 2, vote 3–2): the reviewer reads 7–9 as the
  story's closing coda, a deliberate ending rather than a stall, and would mark it not slow.
- **p17** (*The Great Gatsby* ch. 1, consensus stretch 7–9, severity 1.5): the reviewer reads 7–9 as
  Tom's portrait at the moment Nick arrives, description inside the scene, and would mark it not
  slow. The reviewer agrees with the 4–5 part of the 1–5 stretch.
- **p40** (*Pride and Prejudice* ch. 36, consensus none slow, votes 1/1/2/2/2/0): the reviewer reads
  ¶4–5 of Elizabeth rereading the letter as rehearsing what the reader has seen, slow at severity
  2. The report's default run 3–4 counts as a false positive against the consensus.

Results are in [STYLE-PACING.md](../../docs/STYLE-PACING.md#hand-label-set-256).

## Sensory norms follow-ups (#282)

`cargo build -p writemorebetter-style --release --example norm_reports` followed by
`python3 bench/style/norms.py --probe target/release/examples/norm_reports` runs five tier 2
synthetic cases: participles, literal versus predicative *feel*, transparent versus lexicalized
compounds, adverb-interrupted figurative predicates, and speech tags. The fixtures include curly
quotes/apostrophes, an em dash, an ellipsis and U+2028; byte and UTF-16 spans must agree. The
`compounds` and `tags` cases assert exact surfaces; `tags` uses *shouted* and *murmured*, which the
bundled table rates ≥ 4.0, so only the speech-tag detector can exclude them. The Rust suite
additionally forces strongly rated speech tags through an invented table.

`test_lancaster_norms.py` checks the derivation using invented means around strength/margin
boundaries, including an exact decimal boundary that binary floating-point subtraction can lose.
`WMB_NORM_JSON=bench/style/out/norms` makes the `norm_reports` example save one JSON result per
input basename for public-text calibration; output belongs in ignored benchmark directories.
The source text is never included in that JSON. The public calibration, the #286 review fixes and
the owner decision to remove `singleSense` are recorded in [STYLE-NORMS.md](../../docs/STYLE-NORMS.md).

## Transitions and pronoun starts (#247)

`python3 bench/style/transitions_pronouns.py --gate` builds the native example probe and scores
`labels/transitions.jsonl` (original, hand-labeled development examples, including known misses) with
exact byte/UTF-16 spans for all five types. It also appends 14 varied fixtures, each once, after a
different pinned public-domain passage with only a paragraph break, and scores pronoun runs and
transition repeats in them: positives (same-two-word runs across a paragraph, contractions, U+2028,
one speaker's dialogue, rhetorical questions, repeated connectors) and controls (a run `sameWordRuns`
already reports, two-speaker exchanges, alternating pronouns, nominal `One`, determiner `Her`). Flags
that start in the unchanged source are diagnostic counts. It also records candidate flag counts on all
six `bench/public` texts and Pride and Prejudice chapters 1–6. The gate fails when any measurement
differs from `transitions-pronouns-baseline.json`, which includes the scores, so improvements and
regressions both require an inspected rebaseline. `--probe PATH` reuses an existing
`transition_reports` example binary. The output under ignored `out/transitions-pronouns/` holds exact
gold/prediction rows and aggregate measurements. See
[definitions and limitations](../../docs/STYLE-TRANSITIONS.md).
