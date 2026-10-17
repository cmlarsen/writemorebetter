# Pacing: slow stretches

`style::reports::pacing` (#251) is an **opt-in lens** for sustained backstory, exposition and
interior reflection. It diagnoses patterns and never generates replacement prose. A flag is a
place to inspect, not a judgment that the passage should be faster. Reflection and exposition
are often exactly what a scene needs.

The thresholds are **heuristic and uncalibrated against published contemporary fiction**.
Only public texts and invented fixtures were used here. Published-fiction calibration is private
and owner-run; there is no shipped baseline or population comparison.

```rust
use writemorebetter_style::reports::pacing::{report, PacingOptions};
let pacing = report(&document, &PacingOptions::new())?;
// Retained StyleDocument: document.pacing(None)?
```

```javascript
const pacing = JSON.parse(document.pacing());
const custom = JSON.parse(document.pacing('{"minimumPastPerfectPercent":30}'));
```

```swift
let pacing: PacingReport = try document.pacing()
let custom = try document.pacing(optionsJson: "{\"minimumPastPerfectPercent\":30}")
```

## Signals and runs

Each row in `paragraphs` has a self-contained `span`, `sceneBreakBefore`, `hasSpeech`, `signals`
and `triggeredSignals`. Row order is the document's prose-paragraph order after its heading
and scene-marker exclusions. The original spans remain valid for hard-wrapped text; the report
never reparses paragraphs. See [document conventions](STYLE-DOCUMENT.md).

`PacingSignals` includes raw numerators and denominators so a UI can explain every percentage:

| Field | Definition |
|---|---|
| `words` | `Split<usize>` (`all`, `narration`, `dialogue`), counted by `is_word && !is_skipped`; clitics count as words, as in sentence length |
| `sentences`, `meanSentenceLength` | number of analyzed sentences (including zero-word sentences), and counted words ÷ sentences |
| `dialoguePercent` | speech word tokens ÷ all word tokens × 100; uses `Document::mode_at`, not sentence mode |
| `verbPhrases` | approximate verb heads: a counted token with a `VB*` reading inside a verb-phrase chunk (`*-VP`), or with no chunk at all; adjacent *be/have/do* auxiliary chains, with intervening adverbs, collapse into one head |
| `pastPerfect`, `pastPerfectPercent` | heads introduced by *had* (`VBD`, lemma *have*) or *'d* followed by `VBN`; count ÷ verb phrases × 100 |
| `cognition`, `cognitionPercent` | heads with a cognition verb lemma from the authored list; count ÷ verb phrases × 100 |

A token chunked as anything other than a verb phrase is not a verb head, so noun *thought*,
particle *up* (*dug up*), adverb *home* (*'d come home*) and adjective *wrong* stay out of both
numerator and denominator even though each has a lexical `VB` reading. Tokens the chunker left
unchunked fall back to the `VB` reading. The chunker sometimes labels a real verb as a noun phrase
(*was* in *What she had done was wrong*); it is then not counted. Multiple readings never count a
token twice. Auxiliary chains may cross only whitespace and consecutive adverbs, never
punctuation, other words, skipped names, sentences or paragraph boundaries. An adverb here is a
token with an `RB`, `RBR` or `RBS` reading and no `VBN`, `VBG` or `VBD` reading, so *still* and
*back* count as adverbs despite their base-verb readings: *had still not forgotten* and *had back
then known* are past perfect, while *had still water* and *had back pain* are not. *Had never
been*, *had already gone* and *had been remembering* count once. *Had a boat* is ordinary
possession. Inverted counterfactuals (*Had she known, …*) are not counted, because *had* is
followed by the subject rather than the participle. Straight and curly
apostrophes normalize alike. Contracted *'d gone* counts, but *'d go*, *'d put* and other cases
where the following token also has a base `VB` reading do not: *would* is still possible.

Cognition is the head of the collapsed phrase: *had remembered* contributes to both signals.
The 22-lemma original list in `style/data/pacing/cognition.txt` includes *think, wonder, remember,
realize/realise, know, imagine, recall, understand*, and related interior verbs. This is a small
editorial vocabulary, not a psychological taxonomy; unlisted verbs do not count. It contains no
data copied or derived from third-party dictionaries or rule resources.

Null means an empty denominator, never zero or NaN. These local measurements remain available
for short paragraphs so a UI can draw a strip; they are not baseline comparisons and have no
50-sentence display floor. `triggeredSignals` on a row lists which thresholds that paragraph's own
values meet; it is a measurement, not a verdict. **A pacing strip should highlight only
`slowRuns`** and draw paragraph signals as neutral values. A short in-scene sentence with one
relative-clause past perfect (*the coffee I had made too strong*) or immediate-action past perfect
(*I had opened my mouth to reply, when…*) easily meets two thresholds on its own. An independent
review of PR #284 judged a seeded random sample of 40 such two-signal rows on the public texts:
15 useful, 25 noise, 0 wrong. `hasSpeech` checks the document's speech spans even when
the speech contains only punctuation or all its words were skipped; a rounded zero dialogue share
can never make speech qualify. Scare quotes and quoted titles follow the document's non-speech rule.

A paragraph is eligible for a run when it contains no speech, has at least one counted word and
meets at least two of the three thresholds. `slowRuns` starts from maximal consecutive sequences
of eligible rows, bounded by speech, an ineligible paragraph, recognized headings or scene breaks.
Ordinary paragraph breaks do not stop a sequence. A run must have **at least 3 paragraphs OR
strictly more than 400 words**, and its pooled signal values must also meet at least two
thresholds. When a whole sequence fails the pooled check, the report keeps the longest window
inside it that passes (the earliest on ties) and searches the remainder on each side the same
way. Different signal pairs can support adjacent paragraphs, so pooling the whole sequence can
dilute both pairs below their thresholds; the window search means that appending more eligible
paragraphs to a flagged sequence never leaves it without a run. Each `PacingRun` owns its covering `span`,
half-open `paragraphs: {start, end}` indexes into this report's rows, pooled `signals` and
`triggeredSignals` (`sentenceLength`, `pastPerfect`, `cognition`). Pooling sums raw counts and then
recomputes rates; it never averages paragraph percentages. An eligible paragraph is not
necessarily inside a run.

## Options

All Rust options are `#[non_exhaustive]`, with `new()`/`default()` and matching `with_*` builders.
Bindings accept camelCase JSON and snake_case aliases, and reject unknown keys. Native methods
return `Result<_, StyleError>`; compact documents return `needsTokenDetails`.

| Option | Default | Meaning |
|---|---|---|
| `minimumParagraphs` | 3 | inclusive run length; positive |
| `moreThanWords` | 400 | strict alternative word cutoff; 400 alone does not qualify |
| `minimumSentenceLength` | 18 | inclusive average words per sentence; finite, nonnegative |
| `minimumPastPerfectPercent` | 20 | inclusive share of approximate verb phrases; 0–100 |
| `minimumCognitionPercent` | 20 | inclusive share of approximate verb phrases; 0–100 |

No action detector is claimed. Short, rapid clauses often avoid the long-sentence signal, but
long action with cognitive verbs can still flag; past perfect can describe immediate action,
and indirect speech is not quoted dialogue. Tagger/chunker ambiguity, archaic inflection,
unknown verbs and the small cognition list affect both numerator and denominator. A single
verb can produce 100% in a small paragraph; the run floor and two-signal requirement provide
context rather than a statistical confidence claim.

## Public measurements

Public-only measurement and spot judgments are recorded below. Standard Ebooks chapter downloads
are pinned by URL, revision and SHA-256 in `bench/style/sources.json`; their CC0 dedication links
are recorded there. Downloaded text and review excerpts stay in ignored `.cache/` and `out/`.
The five repository stories are invented public fixtures, not published fiction. Moby uses the
full unchanged hard-wrapped `moby150k.txt` timing input, including front matter, not the narrative
cut used by the sentence-start gate. The other inputs use one paragraph per line.

Reproduce with `python3 bench/style/pacing.py`. The output contains per-paragraph signal
distributions and a separate ignored review artifact. The first tier 2 exact-span baseline is
in `bench/style/pacing-baseline.json`; it measures regression agreement on injected fixtures,
not held-out literary precision or recall.

### First baseline (2026-10-16)

51,987 counted word tokens across 1,207 paragraphs; 128 paragraphs meet two thresholds on their
own, but defaults report only one sustained run. All four Standard Ebooks chapters and the full Moby excerpt have zero
runs. The first chapter of each is fetched at benchmark time; no downloaded text is committed.
Full signal means/percentiles, options, sweep counts and judgment metadata are recorded in
[`pacing-baseline.json`](../bench/style/pacing-baseline.json).

| Input | Words | Paragraphs | Two-signal rows | Paragraphs in runs | Runs |
|---|---:|---:|---:|---:|---:|
| story1 | 2,209 | 96 | 25 | 4 | 1 |
| story2 | 2,038 | 96 | 16 | 0 | 0 |
| story3 | 2,008 | 96 | 19 | 0 | 0 |
| story4 | 2,175 | 93 | 21 | 0 | 0 |
| story5 | 1,856 | 86 | 21 | 0 | 0 |
| moby150k (hard-wrapped) | 25,002 | 315 | 9 | 0 | 0 |
| The Mysterious Affair at Styles, ch. 1 | 4,161 | 157 | 4 | 0 | 0 |
| The Great Gatsby, ch. 1 | 6,067 | 152 | 7 | 0 | 0 |
| The Sign of the Four, ch. 1 | 2,949 | 56 | 3 | 0 | 0 |
| Anne of Avonlea, ch. 1 | 3,522 | 60 | 3 | 0 | 0 |

These numbers follow the PR #284 review fixes. Before them, the verb-head rule admitted particles,
adverbs and adjectives with a lexical `VB` reading (7.9% of counted verb tokens in story1–5), which
diluted both verb shares: 116 two-signal rows, the same single default run, and 7 / 5 / 2 runs in
the first three sweep rows below.

Public-only threshold sweep (`python3 bench/style/pacing.py --sweep`):

| Mean length / past-perfect % / cognition % | Runs | Spot judgment: useful / noise / wrong |
|---|---:|---|
| 15 / 10 / 10 | 8 | 4 / 4 / 0 |
| 20 / 10 / 10 | 6 | 2 / 4 / 0 |
| 20 / 15 / 15 | 2 | 0 / 2 / 0 |
| **18 / 20 / 20 (defaults)** | **1** | **1 / 0 / 0** |
| 25 / 30 / 30 | 0 | no flags to judge |

The implementer spot-judged every loose-setting stretch and the default run as a
fiction editor inspecting a pacing strip: useful means sustained backstory/interior reflection
or exposition worth locating; noise means predominantly current scene business or a short
setup; wrong means the asserted signals do not describe the passage. This is a lens-usefulness
judgment, not a recommendation to cut prose. The useful loose stretches were story1's sustained
recollection of a past job offer (paragraphs 79–82, zero-based), the Styles family-history block,
Gatsby's Buchanan background/portrait and, borderline, Moby's introduction of Father Mapple (his
sailor past, then the pulpit's description). Four other story1/story4 stretches were scene/setup
noise. The 20 / 10 / 10 and 20 / 15 / 15 runs are subsets or trimmed variants of those stretches
and inherit their labels. The defaults retain only the job-offer recollection. They miss the
Styles, Gatsby and Moby blocks: deliberate conservatism trades recall for fewer interruptions. There was no blind or
independent adjudication, the sample is small and historically skewed, and these thresholds
remain uncalibrated against modern published fiction. Raising the length threshold alone lost
the clearest backstory block; raising the verb shares removed current-scene noise instead.

### Hand-label set (#256)

Five models labeled 305 paragraphs in 40 public-domain passages independently and blind on
2026-10-16: DeepSeek V4 Pro, Muse Spark 1.3, Codex gpt-6.1-sol, Claude Opus 5.5 and Claude Fable 5.1.
The passages are stratified on the report's default and loose flags (14 around a reported run, 13
near-misses, 13 ordinary). Each labeler saw only the packet: numbered passage text, a blank template
and the [labeler guidelines](../bench/style/labels/PACING-GUIDELINES.md). A 5-way majority per
paragraph is the consensus; no slow ties are possible. It marks 59 paragraphs slow, in 15 stretches
across 13 passages (35 of 104 paragraphs in flagged passages, 20 of 102 in near-misses and 4 of 99 in
ordinary ones). Tooling, label files and provenance are in the
[benchmark README](../bench/style/README.md#pacing-hand-label-set-256).

| Agreement | Value |
|---|---:|
| Fleiss' κ, `slow` | 0.712 |
| Fleiss' κ, `kind` | 0.805 |
| Fleiss' κ, passage has any stretch | 0.740 |
| Pairwise Cohen's κ, `slow` (min / mean / max) | 0.612 / 0.713 / 0.841 |
| Pairwise Cohen's κ, `kind` (min / mean / max) | 0.748 / 0.805 / 0.890 |
| `slow` vote splits 5–0 / 4–1 / 3–2 | 248 / 32 / 25 (8.2% decided 3–2) |
| Mean pairwise Jaccard of a passage's slow paragraphs | 0.831 |
| `kind` plurality ties | 8 |

Pairwise Cohen's κ, `slow` / `kind`:

| | claude-fable | claude-opus | codex-sol | deepseek | muse |
|---|---:|---:|---:|---:|---:|
| claude-fable | — | 0.84 / 0.89 | 0.76 / 0.81 | 0.66 / 0.82 | 0.73 / 0.82 |
| claude-opus | 0.84 / 0.89 | — | 0.69 / 0.83 | 0.61 / 0.79 | 0.62 / 0.81 |
| codex-sol | 0.76 / 0.81 | 0.69 / 0.83 | — | 0.64 / 0.76 | 0.72 / 0.77 |
| deepseek | 0.66 / 0.82 | 0.61 / 0.79 | 0.64 / 0.76 | — | 0.84 / 0.75 |
| muse | 0.73 / 0.82 | 0.62 / 0.81 | 0.72 / 0.77 | 0.84 / 0.75 | — |

No labeler is an outlier. The lowest mean pairwise `slow` κ (DeepSeek, 0.69) is 0.03 below the
median of the others, against a 0.1 flag margin. The labelers split into two pairs by strictness: the
Claude models mark 72–76 paragraphs slow and agree with each other at κ 0.84, while DeepSeek and
Muse mark 48–49 and also agree at κ 0.84; Codex (56) sits between. Agreement is lowest on
p21, p10, p15, p40, p31 and p29 (pairwise stretch Jaccard 0.32–0.40), which are the habitual-summary,
recap and opening-exposition passages the labelers reported as ambiguous.

To put the report's numbers next to human-like agreement, each labeler is scored against the strict
majority of the other four (2–2 ties excluded at paragraph level), and the report against the same
five leave-one-out majorities:

| Labeler vs the other four | Slow ¶ | Paragraph P / R / F1 | Run any-overlap F1 |
|---|---:|---|---:|
| claude-fable | 72 | 0.79 / 1.00 / 0.88 | 0.77 |
| claude-opus | 76 | 0.68 / 0.92 / 0.78 | 0.76 |
| codex-sol | 56 | 0.91 / 0.86 / 0.88 | 0.87 |
| deepseek | 49 | 0.90 / 0.74 / 0.81 | 0.81 |
| muse | 48 | 0.98 / 0.79 / 0.88 | 0.89 |
| **mean of labelers** | | **F1 0.85** | **0.82** |
| report at defaults (mean over the same five majorities) | 11 | F1 0.26 | 0.35 |

#### Report accuracy at defaults

Against the five-labeler consensus the defaults (18 / 20 / 20, three paragraphs) flag 11 paragraphs,
8 of them slow: paragraph precision 0.73, recall 0.14, F1 0.23. Run level, 3 of 4 reported runs
overlap a consensus stretch and 3 of 15 stretches are found (any-overlap P / R / F1 0.75 / 0.20 /
0.32; IoU ≥ 0.5 F1 0.21). The report is precise and misses most of what readers call slow; its F1 is
about a quarter of the labelers' mutual F1. These numbers are the CI baseline
(`bench/style/pacing-labels-baseline.json`).

#### Threshold sweep

`python3 bench/style/pacing_labels.py sweep` scores 350 settings (mean sentence length
0/10/12/15/18/20/25, past perfect and cognition 5/10/15/20/30 %, minimum paragraphs 2/3; the
400-word alternative is unchanged). Full results are in
[`pacing-labels-sweep.json`](../bench/style/pacing-labels-sweep.json). The named settings and the top
of the grid, by paragraph F1 against the consensus:

| Length / past perfect % / cognition % / min ¶ | Paragraph P / R / F1 | Predicted ¶ | Run any-overlap P / R / F1 | Run IoU ≥ 0.5 F1 | Mean ¶ F1 vs leave-one-out majorities |
|---|---|---:|---|---:|---:|
| **10 or 12 / 20 / 5 / 3 (best)** | **0.62 / 0.44 / 0.51** | 42 | 0.67 / 0.53 / 0.59 | 0.44 | 0.57 |
| 0 / 20 / 5 / 3 (length signal off) | 0.58 / 0.44 / 0.50 | 45 | 0.62 / 0.53 / 0.57 | 0.43 | 0.55 |
| 10 or 12 / 15 / 10 / 3 | 0.63 / 0.41 / 0.49 | 38 | 0.64 / 0.47 / 0.54 | 0.46 | 0.55 |
| 10 or 12 / 15 / 5 / 3 | 0.55 / 0.44 / 0.49 | 47 | 0.62 / 0.53 / 0.57 | 0.43 | 0.54 |
| 0, 10 or 12 / 15 / 20 or 30 / 3 | 0.78 / 0.36 / 0.49 | 27 | 0.86 / 0.40 / 0.55 | 0.45 | 0.54 |
| 18 / 20 / 5 / 3 | 0.69 / 0.34 / 0.45 | 29 | 0.75 / 0.40 / 0.52 | 0.43 | 0.51 |
| 15 / 10 / 10 / 3 (loose) | 0.42 / 0.37 / 0.40 | 52 | 0.40 / 0.47 / 0.43 | 0.27 | 0.43 |
| 12 / 20 / 20 / 3 | 0.80 / 0.20 / 0.32 | 15 | 0.80 / 0.27 / 0.40 | 0.30 | 0.36 |
| 15 / 20 / 15 / 3 (PR #291) | 0.65 / 0.19 / 0.29 | 17 | 0.67 / 0.27 / 0.38 | 0.29 | 0.32 |
| 20 / 10 / 10 / 3 | 0.46 / 0.19 / 0.27 | 24 | 0.57 / 0.33 / 0.42 | 0.09 | 0.30 |
| **18 / 20 / 20 / 3 (defaults)** | **0.73 / 0.14 / 0.23** | 11 | 0.75 / 0.20 / 0.32 | 0.21 | 0.26 |
| 20 / 15 / 15 / 3 | 0.50 / 0.10 / 0.17 | 12 | 0.75 / 0.20 / 0.32 | 0.11 | 0.19 |
| 25 / 30 / 30 / 3 | 0.00 / 0.00 / 0.00 | 2 | 0.00 / 0.00 / 0.00 | 0.00 | 0.00 |

Paragraph F1 peaks at 0.51 with mean sentence length 10 or 12 (identical predictions), past perfect
20 % and cognition 5 %, three paragraphs. The best run-level any-overlap F1 is 0.60 at 25 / 5 / 5 /
2, which reaches it with long runs (paragraph F1 0.41). The optimum is flat: 149 of 350 settings reach
paragraph F1 ≥ 0.40, and with 59 positive paragraphs the top dozen are within noise of each other.
Two levers matter. Lowering cognition to 5 % alone lifts F1 from 0.23 to 0.45 (18 / 20 / 5);
lowering mean sentence length below 15 adds a little more, and turning it off costs only 0.01. The
length threshold mostly removes true slow paragraphs here. Allowing two-paragraph runs gains
recall but loses more precision; the best such setting reaches 0.47.

PR #291 proposes 15 / 20 / 15 from published-fiction flag frequency. On these labels it adds 6
predicted paragraphs, 3 of them slow: paragraph F1 0.23 → 0.29 and run any-overlap F1 0.32 → 0.38,
with precision 0.73 → 0.65. A paired bootstrap over passages (4,000 resamples) puts the paragraph F1
gain's 95% interval at −0.03 to +0.23, so the labels are consistent with the change but do not
establish an improvement. Neither half moves anything alone: 15 / 20 / 20 and 18 / 20 / 15 predict
exactly what the defaults do. The best grid setting beats both 18 / 20 / 20 (+0.29, interval +0.07
to +0.48) and 15 / 20 / 15 (+0.23, interval +0.05 to +0.39).

These labels are model judgments on nineteenth- and early twentieth-century prose plus the five
invented stories, the sweep tunes on the same 305 paragraphs it reports, and the stratification
over-represents flagged and near-miss passages. Treat the sweep as direction for the calibration on
published contemporary fiction, not as a held-out estimate. This PR does not change the defaults.

## Owner-run calibration

```sh
python3 bench/style/run.py tier3 --pacing --self-test
# Owner only, later:
python3 bench/style/run.py tier3 --pacing --export /approved/export
```

The script is a calibration entry point without a published-fiction baseline. It pools units by
`author_key` and prints only aggregates: the pooled per-paragraph signal distributions, the
distribution of author signal means, the distribution of per-author flag rates (share of
paragraphs inside runs, runs per thousand words), and the share of authors with any run. It
prints no per-author rows or exact word totals. It never emits text or source
metadata. The synthetic self-test checks pooling and metadata suppression. No private corpus,
export, title or author record was accessed to implement or test this report. See the
[benchmark README](../bench/style/README.md#pacing-251) for export layout, weighting and gates.
