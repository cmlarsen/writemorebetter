# Sentence reports

Part of #244. Rust entry points are `reports::sentence_length::report` and
`reports::sentence_starts::report`: `&Document`, options and an optional baseline, returning
`Result<_, StyleError>`. Types are report-prefixed (`SentenceLengthOptions`, `SentenceLengthBaseline`,
`SentenceLengthReport`, `SentenceStartsOptions`, `SentenceStartCategory`, …). Public records are
non-exhaustive; construct options with `new()` and the `with_*` builders. Native spans retain
absolute, half-open UTF-8 byte, Unicode scalar and UTF-16 endpoints. Both reports return
`groups: Split<…>` with `all`, `narration` and `dialogue`. Each report runs independently on one retained document; see
[the per-report API](STYLE-REPORTS.md#independent-reports-shared-analysis).

**A sentence table per report.** Each result owns `sentences: Vec<SentenceRow>` with the original
spans, mode and word counts in document order. Long-sentence indexes, start indexes and run ranges
refer only to that result's own table. `SentenceRun { span, sentences }` carries a covering span and
half-open index range. `Document::sentences()` and `reports::sentence_table(&document)` use the same
order. Each result also owns split `words` counts.

A sentence's mode is the mode of its first `is_word` token (`Document::sentence_mode`): dialogue when
that token starts inside a **speech** span, the document-level rule every report shares (see
[STYLE-DOCUMENT.md](STYLE-DOCUMENT.md#narration-and-dialogue)). Scare quotes, titles and unpunctuated quoted
subjects such as `“Yes” was the answer.` are narration. Punctuated passive speech such as
`“Go,” was the answer.` is dialogue. Mixed narration/speech sentences belong
wholly to that opening mode. Monotony and same-opener runs are narration-only. They cross ordinary paragraph breaks and stop at
dialogue, recognized headings and scene breaks (explicit markers or two-plus blank lines in either mode). Counts and spans remain available at every sample size.
Each group's own `minimumSentences` (default 50) suppresses shares, means, percentiles and
differences below that sample. `lowConfidence` is true below 200 eligible sentences, or the configurable display floor when higher. Empty groups never yield NaN.

**Both reports count the same sentences.** Every group's `sentences` is the number of sentences in
that mode, identical in the two reports and equal to `StyleDocument.stats().sentences`.
Length adds `zeroWordSentences`; starts adds `excludedSentences`, the sentences whose first token is
a skipped name. Start shares and the starts sample threshold use `sentences - excludedSentences`.

## Length and calibration

Words are exactly `is_word && !is_skipped` document tokens (`Sentence::word_count`). Unicode
letters/numbers count; clitics count separately, while punctuation and emoji do not. Zero-word
sentences remain in the histogram and are counted in `zeroWordSentences`. Length statistics and
monotony remain selected by default. The long-sentence lens is opt-in via
`includeLongSentences:true` or `with_long_sentences(true)`; `longSentences` is empty otherwise.
Once opted in, long-sentence candidates have **at least** `longSentenceWords` words (default 40). The lens selection is the top N per chapter (`longestPerChapter: 3`). Set `includeAbovePercentile:true`
(or `with_above_percentile(true)`) to also include candidates above the writer's own chapter p95.
Sentences with more than half their words inside quotes, Markdown block quotes, consistently
indented multiline blocks and inscriptions with at least 80% uppercase letters are excluded
from candidates; their sentences remain in descriptive statistics. A narrator’s own sentence containing a short embedded or scare quote remains eligible. Four-space first-line indents alone do not mark a block. The optional p95 is calculated
over the eligible sentence pool, including sentences below the 40-word floor. Ties in the top N use document
order; equality at p95 alone does not qualify. Chapter headings start a new selection unit; without
headings the input is one chapter. A chapter with no 40-word candidates produces no long flags.
Lower the candidate floor explicitly when a UI wants a pure longest-sentences lens. Each group returns `sentences`, a `histogram` of
`{words, sentences}` buckets in ascending order, `mean` and `meanDifference`, `percentiles` as
`{percentile, value, difference}` rows for 5/25/50/75/95, `longSentences` (sentence indexes) and
`monotonyRuns`.

A monotony run is an inclusion-maximal contiguous interval of at least four sentences
(`MONOTONY_MIN_SENTENCES`) whose **maximum minus minimum length is at most two words**
(`MONOTONY_MAX_SPREAD`). This is the owner's pairwise reading of
"within ±2 words of each other": 8/10/8/10 qualifies; 8/12/8/12 does not. Drift is not permitted.
Overlapping maximal intervals can be returned; nested suffixes are not. A sliding multiset keeps the
scan linear in sentence count apart from logarithmic updates. Percentiles 5/25/50/75/95 interpolate
at `(n−1)*p/100`. Differences subtract the matching baseline statistic. Per-mode differences require a separately supplied
`narration` or `dialogue` reference; a blended reference is used only for `all`.
`meanHighlighted` and each percentile's `highlighted` are true only strictly outside the matching
mode's author p5–p95 band. Missing bands never highlight, and the bundled bands use the owner-supplied 2026-10-16 chapter-window calibration. These author bands describe variation across published authors,
not the within-chapter sentence percentiles used to select long sentences. Baseline `percentiles` keys
must be 5, 25, 50, 75 or 95 and values finite and non-negative; anything else is
`StyleError::InvalidBaseline` instead of being ignored. `SentenceLengthBaseline::bundled()` returns
the bundled file's `baseline` member.

The bundled files preserve the exact owner-supplied `source` block and all seven calibration
percentiles under `calibration`. Runtime bands use p5 and p95 separately for all/narration/dialogue;
reference deltas subtract each measure's author-weighted mean. The calibration uses #270 at `273e7b5`,
1,559 chapter windows (target 2,000–6,000 words: closed after reaching 6,000 or at a scene break
past 3,000, short tails merged) from 210 units and 71 author units, with equal author
weights. Eligible length/start samples are 1,515 all, 980 narration and 614 dialogue. Calibration
predates this PR's segmentation refinements; provenance records that version rather than claiming
a fresh corpus run. No private corpus was accessed for this integration.

The export has one paragraph per line, with blank lines at scene breaks. `tier3 --sentence-length`
uses **default line mode**, not hard-wrapped mode. Unpunctuated headings, interrupted dialogue and
following paragraphs remain separate. `wmb-document --reports TOKENIZER [--hard-wrapped]` defaults
to line mode; select hard-wrapped only for wrapped public imports such as Moby. The probe explicitly opts in to the long-sentence lens with otherwise default options and bundled baselines and prints both independent results under
`reports`; `wmb-document --defaults` prints those selected options and run thresholds that the Python
reference reads instead of hardcoding them. The synthetic test
includes consecutive unpunctuated lines and fails under the old hard-wrapped convention.

The optional owner-run sentence-length calibration command pools units within each author, then averages each author's sentence mean and each
within-author sentence percentile equally into its output `baseline` (distinct from the shipped chapter-window calibration). Its `authorMeanPercentiles` separately
measures the distribution of author means and must not be copied into `baseline.percentiles`.
The owner supplies the export/build provenance; the command does not infer or enforce publication
years. It prints numeric aggregate JSON only and
suppresses argument/error details whenever `tier3` appears anywhere in argv, including before invalid
options. Sentinel tests cover both argument orders. No source text or metadata is emitted.

This is a distribution report, with configurable statistics and no correction suggestions. It does
not call or duplicate the engine's TOO_LONG_SENTENCE matcher.

## Start definitions and ambiguity

Same-opener runs omit deliberate anaphora when every sentence ends in `?`/`!` or every sentence
shares its first two words.

Fine classification skips whitespace and stacked leading quotation tokens composed of characters
from `" ' “ ” ‘ ’ « » ‹ › „ ‟ ‚ ‛` and backtick. Other leading punctuation is retained. A skipped
name at that original first token excludes the sentence from fine counts/denominators (it is
counted in `excludedSentences`) and breaks runs; the next token is not promoted. Length still
measures that sentence's remaining words.

The adjudicated targets are grammatical opening functions, rather than the classifier's predictions:

| Category | Target and Penn/token evidence used by the heuristic |
|---|---|
| subject | A nominal/pronominal opening with a main predicate. Candidate heads/modifiers: NN/NNS/NNP/NNPS, PRP/PRP$, WP/WP$, WDT/DT/PDT, EX, JJ/JJR/JJS, CD. Wh-pronoun questions with a finite predicate use this category even when the wh-pronoun is an object. **Verbless NP fragments are other**, including quoted `Nonsense`, `Remorse` and reported noun fragments. |
| adverb | Clause-initial RB/RBR/RBS/WRB, including locative `there`; excludes fixed subordinators and fronted adjective phrases. |
| gerund | VBG opening, including participial adjuncts and form-defined fragments. An NN/NNS alternative followed by finite-predicate evidence instead supports subject (`Evening fell`). |
| infinitive | TO immediately followed by VB, including form-defined fragments. |
| subordinating conjunction | A word/phrase introducing a dependent clause. Includes `much as` and `now that`; temporal `after before since until as once` require a nominal/pronominal subject with predicate evidence before the first comma/semicolon. A temporal preposition plus a verbless NP is introductory, including comma-less adjuncts. |
| coordinating conjunction | CC with `and but or nor yet so`; `for` requires a following subject-case pronoun or existential there. `For` + NP is introductory. |
| past participle | VBN, with **no VB/VBP reading**, and either no VBD or a following comma/IN. Base-form commands `Come here` and `Run away` are other. |
| inversion | An auxiliary/modal preceding an eligible subject. Subject-case pronouns (raw PRP_S* or I/you/he/she/it/we/they) and existential there qualify; object-only me/him/her/us/them, JJ and CD do not. A following DT/PDT/NN/NNS/NNP/NNPS requires interrogative `?` evidence. Bare do/have also require `?`, so `Do it now` and `Have a seat` are other. Bare VBD/VBZ never establishes inversion. |
| introductory | Discourse markers, prepositional/temporal adjuncts, fronted adjective phrases and interjections. The heuristic uses IN/TO and fixed discourse words; it can miss nominal time/odds adjuncts or fronted modified adjectives. Gold examples such as `This time` and `Ten to one` have no special-case rules. |
| other | Subjectless imperatives/verb fragments, **verbless noun phrases**, and unclassified punctuation/unknown openings. Form-defined gerund/infinitive/participle fragments retain their form category. |

Classification is a deterministic heuristic, not a parse. Tags normalize `NN:* → NN`, `PRP_* → PRP`,
`PRP$_* → PRP$`, `RB_SENT → RB`; tests use any surviving normalized reading, independent of reading
order. Priority is fixed discourse words; TO+VB; compound subordinators;
conjunctions; VBG; VBN; auxiliary inversion; there/indefinite-pronoun cues; prepositions; adverbs;
imperative/fragment guard; nominal main-predicate evidence; remaining UH/unknown-capital cues; other.

Unambiguous subordinator surfaces are `although because if unless though whereas while when whenever
whether`, with IN/WRB/CC/DT/WDT/WP/RB evidence. Fixed discourse words are `however
therefore meanwhile finally indeed instead moreover nevertheless furthermore well yes oh ah ay
aye alas hey okay ok yeah yep nope hmm um uh hurrah nay`;
`no` is introductory before punctuation/end. No phrase rules target labeled nominal time/odds rows. Auxiliary surfaces are `am is are was were do does did
have has had can could will would shall should may might must`. One `n't`/`n’t` may intervene before
the inverted subject. Indefinite-pronoun cues are `someone somebody something nobody nothing everyone
everybody everything anyone anybody anything`.

The temporal-clause scan stops before a new subject after an NP head: previous NN/NNS/NNP/NNPS/DT
plus a current subject-case pronoun, DT/PRP$/PDT/NNP not preceded by DT, or unknown capitalized word.
This keeps comma-less `After dinner we left` and `Before dawn the ship sailed` introductory, while
`As the door opened` and `After the war ended` remain subordinating.

Predicate evidence accepts VBD/VBN/VBP/VBZ/MD in a verb chunk, including surviving VBD+VBN
ambiguity, or finite VBD/VBZ/MD and unambiguous VBP readings without nominal/adjectival alternatives.
Surface particles/adverbs `still up down out away here there on off back` cannot supply it. A VBP
immediately after NNS also supports a plural-subject predicate when chunking misses it. Subject-case
pronouns additionally accept verb chunks despite tagger ambiguity. Relative/dependent clauses close
after their first predicate; auxiliary continuations and coordinated relative verbs remain dependent,
so the next main predicate can establish a subject. Inside a dependent clause, determiner `that` before a
singular noun does not reopen the dependent state (`The man who wrote that book died`); outside one it still
opens a complement clause, so `The idea that people lie.` stays other. Comma/semicolon resets that state; post-comma
subject-pronoun parentheticals and TO-scoped nonfinite verbs are excluded. Nominal/VB-ambiguous openings
followed by noun/adjective/preposition evidence use the same scan, so `People on the mainland liked…`
can be subject while `Look at me` remains other. This imperative-branch scan stops at `and`, `or`,
`but` or `nor` followed (after any pure adverbs) by a base-form verb, so a coordinated command (`Look at me and tell me`)
cannot supply subject evidence while a coordinated noun phrase (`Pictures of John and Mary hung…`,
`Father Mapple and his wife rose`) still reaches its predicate. An unknown capitalized
second token also enters the scan, allowing nominal titles (`Father Mapple rose`, `Nurse Ratched came in`).
POS-only `'s`/`’s` supplies predicate evidence when a word follows it and the following tokens do
not continue a noun phrase (optionally modified by adjectives/adverbs); `Mom’s asleep` is subject,
`Mom’s red car` and elliptical possessives (`Dinner at Mom’s.`) are other.
This evidence respects relative-clause and quote scope.

Listed interjections consistently open introductory, including bare utterances. They are retained
as the opener rather than skipped before a comma; see the #269 decision in STYLE-REPORTS.md.
This preserves the adjudicated interjection labels and applies the same convention to the added
dialogue forms. U+2028 after terminal punctuation (including a closing quote) ends a sentence within
its paragraph; attribution reattachment cannot cross that explicit boundary, with or without
whitespace before the separator. A tag after the boundary (`“Where?”` U+2028 `she asked.`) therefore
counts as its own short subject-led sentence in the start and length reports; the dialogue-tags
report still attributes it. Without terminal punctuation U+2028 remains internal whitespace.

Quote state matches the opening quote family and tracks nested families. An inner `‘ghost’` does not
end an outer `“…”` scan; unquoted text can continue after quoted material. A closing outer quote still
stops the scan so an attribution cannot turn `“Nonsense,” she said` into subject. These are heuristic
approximations and can miss complex clauses or inherit bad tags. Unknown capitalized nominal openings
also require predicate evidence, and remain unknown in the universal comparison. Ambiguous NN+VBG
nominal subjects followed by a finite verb (including `Running was hard`) use subject by convention;
other VBG openings use gerund. Gold is not generated from predictions, but these rules were tuned
against the development examples; neither floors nor reviewed fixtures are held-out accuracy.

Fine categories assign one integer count per eligible sentence, including other. `categories` is
an array of `{category, count, percent, difference}` in the fixed category order. `ambiguousStarts`
counts first tokens with multiple distinct normalized Penn tags, even if they map to one universal
tag. Ambiguous readings are retained. The report's `starts` list has one entry per eligible
sentence: its `sentence` index, `opener` (the span of the classified first token after leading
quotes, not the whole sentence), lowercase `word`, `category`, `ambiguous` and
`universalCandidates`. The ambiguity flag describes the fine quote-skipped token, even when universal
allocation skips one additional punctuation token and uses the following word. Per-category span
lists are gone; filter `starts` by category instead.

## Universal baseline and repeated words

Universal-12 allocation is independent of fine categories. **After stripping leading quote tokens,
skip exactly one additional leading punctuation token**, matching the owner-provided Ngram convention.
This includes ellipses, dashes, brackets and asterisks. A punctuation-only tokenizer sentence is unknown;
it cannot borrow a word from another sentence. Fine categories and same-word runs keep their own
quote-only opening convention. EWT keeps its frozen quote-only gold/prediction convention; the Ngram comparison in tier 3 explicitly selects the punctuation-skipping convention. Rust/Python opening parity is tested.

Distinct mapped candidate tags receive `1/N` weight; duplicate readings mapping to one tag count once.
`universal` is an array of `{tag, weight, percent, difference}` in the fixed universal-12 order.
Unknowns get no allocation and are counted. Percentages divide by known starts; the sample threshold
still uses all eligible sentences, leaving unknown-selection bias visible. The Rust/Python Penn map
agreement check covers every mapped tag plus internal normalizations, ORD, `_apostrophe_contraction_`
and unknowns. Baseline universal keys must be one of the 12 tags, and every baseline percentage must
be finite and in 0..=100; violations are `StyleError::InvalidBaseline`.
`SentenceStartsBaseline::bundled()` returns the owner-calibrated chapter-window reference and
all/narration/dialogue bands described above. Fine-category and universal-12 references both use
the same calibration; they no longer mix a blended Ngram reference into per-mode comparisons.
The full distribution and exact source provenance remain in the shipped JSON files.

## Bindings and one document pass

Create `checker.styleDocument(text, optionsJSON)` in WASM or Swift's
`checker.styleDocument(text:options:)`, then call `sentenceLength` and `sentenceStarts`. Both results
include their own sentence rows and split word counts. Each method accepts report options JSON and
an optional baseline JSON override. Bundled baselines are used when omitted; the explicit selector `"none"`
disables comparisons. Swift returns typed Codable results. Spans count UTF-16 units.

Starts requires token details and rejects compact documents. Length works on compact documents.
`node wasm/sentence-reports-bench.mjs` measures analysis separately from all six report calls on
`bench/public/moby150k.txt` with hard-wrapped mode (one warmup, three measured iterations).

## Adjudicated benchmark status

The original 332 public-domain Standard Ebooks rows now have **two labelers and coordinator
adjudication**. Before adjudication, agreement was 305/332 (91.87%), Cohen's kappa 0.873; all 27
disagreements were among rows the second labeler marked ambiguous. The second labeler did not see
existing labels or predictions before writing its labels, but it read this definitions document,
which doubles as the implementation rule table, so it was **not fully blind to the heuristic**.
The owner selected nine semantic corrections and assigned all 17 disputed NP-fragment/Nonsense rows
to other. The gate uses corrected-label development floors, with full confusion counts in
`bench/style/sentence-measurements.json`. This is development/regression agreement, not held-out accuracy.

A separate 34-sentence dialogue-heavy set from this repo's invented public stories remains
single-labeler diagnostic material, separately scored. Reviewed imperative/fragment cases are strict
fixtures. Injection tests also include 40 natural multi-sentence narration controls from the two
pinned Standard Ebooks books. An independent Python reference enumerates expected intervals from
analysis tokens; naturally occurring patterns are included in gold and reported as diagnostics,
rather than assumed absent or called errors.

## Measured agreement and distribution sanity

| Category | Precision | Recall | F1 | Previous F1 (e701503) |
|---|---:|---:|---:|---:|
| adverb | 82.35% | 100.00% | 90.32% | 89.66% |
| coordinatingConjunction | 100.00% | 100.00% | 100.00% | 100.00% |
| gerund | 100.00% | 100.00% | 100.00% | 100.00% |
| infinitive | 100.00% | 100.00% | 100.00% | 100.00% |
| introductory | 100.00% | 89.66% | 94.55% | 98.31% |
| inversion | 100.00% | 100.00% | 100.00% | 100.00% |
| other | 76.00% | 86.36% | 80.85% | 69.84% |
| pastParticiple | 100.00% | 75.00% | 85.71% | 85.71% |
| subject | 97.24% | 96.70% | 96.97% | 94.49% |
| subordinatingConjunction | 100.00% | 100.00% | 100.00% | 100.00% |

All three injected suites have 100% precision/recall/F1: 43 long spans, 41 monotony runs and 44
same-word runs, including the 3/1/4 natural-control occurrences respectively. All 149 review regression
fixtures pass, including nominal past-tense predicates, restrictive relatives, nested quotes,
NP-subject temporal clauses and preposition-followed imperatives. They were used during fixes and
are reported separately from the development-label floors; they are not held out.

The distribution gate uses each public input's actual paragraph convention. `story1.txt`–`story5.txt`
are LF, one paragraph per line: `hardWrapped: false` (96/96/96/93/86 paragraphs). Moby is hard-wrapped
CRLF: `hardWrapped: true`, with CRLF preserved. The probe returns its actual paragraph count; the
harness independently checks the count against the selected source layout, and a regression test
fails if the two modes are swapped.

The Moby gate measures narrative **from the second exact `CHAPTER 1. Loomings.` line onward**. The
first occurrence is a contents entry; the second starts at **UTF-8 byte 28,824**. Matching is by text,
not a hard-coded offset; the marker, occurrence and measured offset are recorded in the artifact.
This excludes contents, etymology and extracts, but retains the narrative chapter headings. The cut
contains 120,267 bytes; after exact numbered-heading exclusion it has 190 prose paragraphs and 854 sentences. It is separate from the timing
workload, which remains the complete unchanged 149,091-byte file.

Stories have **15/622 other (2.41%)**, ceiling **5%**. Narrative-only Moby has **106/854 other
(12.41%)**, ceiling **16%**, a 3.59 percentage-point margin. Top mutually exclusive narrative other
surface shapes are 28 remaining verb-tagged openings (including literary/archaic constructions),
24 vocative/interjection shapes, 16 command-like openers, 10 short fragment shapes, no chapter-number
headings, 2 article-led starts, 2 punctuation/markup-led starts, and 24 remaining openings. These
are structural diagnostics, not independently adjudicated grammatical labels. Full counts/shares,
source modes, paragraph counts and cut provenance are in `sentence-measurements.json`; ceilings are
in `sentence-floors.json`. The sanity check complements the development F1 floors and makes no
held-out accuracy claim.

## Sentence-start follow-up (#269)

Public distribution before (origin/main `397a88b`) and after the classifier fixes, using the same
data and paragraph modes. Each cell is count (percent); totals remain 622 stories / 854 narrative Moby.

| Category | Stories before | Stories after | Moby before | Moby after |
|---|---:|---:|---:|---:|
| adverb | 24 (3.86%) | 24 (3.86%) | 89 (10.42%) | 89 (10.42%) |
| coordinatingConjunction | 26 (4.18%) | 26 (4.18%) | 135 (15.81%) | 135 (15.81%) |
| gerund | 2 (0.32%) | 2 (0.32%) | 23 (2.69%) | 23 (2.69%) |
| infinitive | 0 (0.00%) | 0 (0.00%) | 4 (0.47%) | 4 (0.47%) |
| introductory | 46 (7.40%) | 46 (7.40%) | 125 (14.64%) | 129 (15.11%) |
| inversion | 0 (0.00%) | 0 (0.00%) | 9 (1.05%) | 9 (1.05%) |
| other | 16 (2.57%) | 15 (2.41%) | 106 (12.41%) | 103 (12.06%) |
| pastParticiple | 0 (0.00%) | 0 (0.00%) | 7 (0.82%) | 7 (0.82%) |
| subject | 500 (80.39%) | 501 (80.55%) | 329 (38.52%) | 328 (38.41%) |
| subordinatingConjunction | 8 (1.29%) | 8 (1.29%) | 27 (3.16%) | 27 (3.16%) |

Against origin/main, Moby's `Father Mapple rose, and …` moves from other to subject; `Aye,
shipmates, …` and `Aye, well mightest thou …` move from subject to introductory, and `Hurrah, boys;
…` and `Nay, to this very hour, …` from other to introductory, under the interjection convention.
`Wave after wave thus leaps …`, `Savage though he was, …` and `Fill to _this_ mark, and your charge
is …` stay subject as on origin/main; the last is a known imperative miss. In the stories,
`“Everyone’s a stray out here,” I said.` moves from other to subject.

All adjudicated per-category F1 values above are unchanged; no floor or ceiling changed. The
review fixture set now contains 174 cases, each also tested inside curly quotes followed by a U+2028
boundary and a second sentence with a curly apostrophe, em dash and ellipsis; the test asserts the
split and classifies both sentences. Additional separator tests cover terminal/unterminated
sentences, quoted attributions, repeated separators, both paragraph modes and byte/char/UTF-16 spans.

Issue item 6 is closed by #276: each independent method accepts only its own baseline, so there
is no unselected report baseline to ignore. Item 7’s Swift defaults are also covered by #276:
typed methods default `optionsJson` to `nil` and `baseline` to `.bundled`; generated raw methods
default both optional JSON arguments to `nil`. The old combined input records no longer exist.
Generated bindings need no changes.

## Report cost

**Historical measurements, before #275.** These used the combined API and pre-#275 benchmark
script; the current script measures analysis and each method separately on moby150k with token
details enabled. The old 596,376-byte workload and its combined-output sizes below are retained
for history and cannot be reproduced with the current script.

Native report-only median was **3.741 ms per 149,091 bytes** (3.603–4.513 ms, five calls after one
warmup); document construction, reports, JSON and IPC have median **1,217.2 ms**. Combined WASM
median is **5,679.6 ms per 596,376 bytes** (three calls after one warmup), with a 2,820,893-byte JSON
result and 304,824,320-byte process peak RSS. Both reports share one document pass. Apple M5 Pro,
Rust 1.97.1 release, original hard-wrapped CRLF Moby import; shared-machine observational timing.
Raw samples and measurement boundaries are committed in `bench/style/sentence-timing.json`.

The sentence table (2026-10-16) shrank the combined WASM output on that workload from 2,820,893 to
1,216,687 bytes and process peak RSS from 273 to 187 MB, at the same median time (12.57 s before,
12.53 s after, measured back to back at load average 29–31 on the same machine; the absolute times
are load-bound and not comparable with the numbers above). All six reports together return
5,806,036 bytes in 13.06 s on the same run.
