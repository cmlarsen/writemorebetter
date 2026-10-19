# Pacing: slow stretches

`style::reports::pacing` (#251) is an **opt-in lens** for sustained backstory, exposition and
interior reflection. It diagnoses patterns and never generates replacement prose. A flag is a
place to inspect, not a judgment that the passage should be faster. Reflection and exposition
are often exactly what a scene needs.

The thresholds were set on 2026-10-16 from an owner-run, aggregate-only calibration on published
fiction (210 units, 71 authors) plus spot judgments on public texts; see
[published-fiction calibration](#published-fiction-calibration-2026-10-16). There is still no shipped
baseline or population comparison, and no labeled published passages: the published data fix how
often the lens fires, not whether a flagged passage is worth inspecting.

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
| `minimumPastPerfectPercent` | 15 | inclusive share of approximate verb phrases; 0–100 |
| `minimumCognitionPercent` | 15 | inclusive share of approximate verb phrases; 0–100 |

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

### Public baseline (2026-10-16)

51,987 counted word tokens across 1,207 paragraphs; at the calibrated 18 / 15 / 15 defaults 154
paragraphs meet two thresholds on their own (128 at the previous 18 / 20 / 20), but the defaults
report only four sustained runs: story1's job-offer recollection, its opening situation, its
pier walk and the Styles family history. The other Standard Ebooks chapters and the full Moby
excerpt have zero runs. The first chapter of
each is fetched at benchmark time; no downloaded text is committed.
Full signal means/percentiles, options, sweep counts and judgment metadata are recorded in
[`pacing-baseline.json`](../bench/style/pacing-baseline.json).

| Input | Words | Paragraphs | Two-signal rows | Paragraphs in runs | Runs |
|---|---:|---:|---:|---:|---:|
| story1 | 2,209 | 96 | 30 | 12 | 3 |
| story2 | 2,038 | 96 | 17 | 0 | 0 |
| story3 | 2,008 | 96 | 21 | 0 | 0 |
| story4 | 2,175 | 93 | 27 | 0 | 0 |
| story5 | 1,856 | 86 | 23 | 0 | 0 |
| moby150k (hard-wrapped) | 25,002 | 315 | 11 | 0 | 0 |
| The Mysterious Affair at Styles, ch. 1 | 4,161 | 157 | 7 | 4 | 1 |
| The Great Gatsby, ch. 1 | 6,067 | 152 | 10 | 0 | 0 |
| The Sign of the Four, ch. 1 | 2,949 | 56 | 4 | 0 | 0 |
| Anne of Avonlea, ch. 1 | 3,522 | 60 | 4 | 0 | 0 |

The previous defaults gave 25 / 16 / 19 / 21 / 21 / 9 / 4 / 7 / 3 / 3 two-signal rows and only
story1's run; 15 / 20 / 15, this PR's first proposal, gave 28 / 18 / 20 / 23 / 26 / 10 / 8 / 9 / 3 / 3
and two runs. These numbers follow the PR #284 review fixes. Before them, the verb-head rule admitted particles,
adverbs and adjectives with a lexical `VB` reading (7.9% of counted verb tokens in story1–5), which
diluted both verb shares: 116 two-signal rows, the same single default run, and 7 / 5 / 2 runs in
the first three sweep rows below.

Public-only threshold sweep (`python3 bench/style/pacing.py --sweep`):

| Mean length / past-perfect % / cognition % | Runs | Spot judgment: useful / noise / wrong |
|---|---:|---|
| 15 / 10 / 10 | 8 | 4 / 4 / 0 |
| 20 / 10 / 10 | 6 | 2 / 4 / 0 |
| 20 / 15 / 15 | 2 | 0 / 2 / 0 |
| 18 / 15 / 20 | 4 | 2 / 1 / 0, plus 1 borderline |
| 18 / 15 / 10 (rejected) | 6 | 3 / 3 / 0 |
| **18 / 15 / 15 (defaults)** | **4** | **2 / 1 / 0, plus 1 borderline** |
| 18 / 20 / 20 (previous defaults) | 1 | 1 / 0 / 0 |
| 18 / 20 / 15 | 1 | 1 / 0 / 0 (the same run) |
| 15 / 20 / 15 (first proposal) | 2 | 2 / 0 / 0 |
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
and inherit their labels. The previous defaults retained only the job-offer recollection.
15 / 20 / 15 adds the Styles family-history block (paragraphs 8–10, the useful loose run trimmed by
one paragraph) and no noise; it still misses the Gatsby and Moby blocks. At 18 / 15 / 20 and at the
18 / 15 / 15 defaults the Styles block (paragraphs 8–11) appears with story1's current-scene pier walk
(paragraphs 65–69, noise) and its opening situation (22–24, borderline): on this sample, loosening
past perfect admits in-scene business. 18 / 15 / 10 adds Gatsby's Buchanan background (useful) and
story4's opening (noise) and widens story1's opening run to paragraphs 20–24. There was
no blind or independent adjudication and the public sample is small and historically skewed.
Raising the length threshold alone lost the clearest backstory block; raising the verb shares
removed current-scene noise instead.

### Hand-label set (#256)

Five models labeled 305 paragraphs in 40 public-domain passages independently and blind on
2026-10-16: DeepSeek V4 Pro, Muse Spark 1.3, Codex gpt-6.1-sol, Claude Opus 5.5 and Claude Fable 5.1.
The passages are stratified on the report's default and loose flags (14 around a reported run, 13
near-misses, 13 ordinary). Each labeler saw only the packet: numbered passage text, a blank template
and version 1 of the [labeler guidelines](../bench/style/labels/PACING-GUIDELINES-v1.md); the
current [version 2](../bench/style/labels/PACING-GUIDELINES.md) rewrites the rules the labelers found
ambiguous, and the set was not relabeled under it. A 5-way majority per
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

#### Report accuracy at the previous defaults

Against the five-labeler consensus the previous defaults (18 / 20 / 20, three paragraphs) flag 11 paragraphs,
8 of them slow: paragraph precision 0.73, recall 0.14, F1 0.23. Run level, 3 of 4 reported runs
overlap a consensus stretch and 3 of 15 stretches are found (any-overlap P / R / F1 0.75 / 0.20 /
0.32; IoU ≥ 0.5 F1 0.21). The report is precise and misses most of what readers call slow; its F1 is
about a quarter of the labelers' mutual F1. These numbers were the CI baseline until #291 set the
18 / 15 / 15 defaults; `bench/style/pacing-labels-baseline.json` now records that setting (paragraph F1
0.41, run any-overlap F1 0.48).

#### Threshold sweep

`python3 bench/style/pacing_labels.py sweep` scores 350 settings (mean sentence length
0/10/12/15/18/20/25, past perfect and cognition 5/10/15/20/30 %, minimum paragraphs 2/3; the
400-word alternative is unchanged). Full results are in
[`pacing-labels-sweep.json`](../bench/style/pacing-labels-sweep.json). The named settings and the top
of the grid, by paragraph F1 against the consensus. The last two columns run each setting over every
document of the 18 label sources (391 documents, 16,243 report paragraphs) and count the paragraphs
it flags, per 1,000 report paragraphs and outside the labeled windows. Where a row merges settings
with identical in-window predictions, the flag columns list each setting:

| Length / past perfect % / cognition % / min ¶ | Paragraph P / R / F1 | Predicted ¶ | Run any-overlap P / R / F1 | Run IoU ≥ 0.5 F1 | Mean ¶ F1 vs leave-one-out majorities | Source flags per 1,000 ¶ | Source flags outside windows |
|---|---|---:|---|---:|---:|---:|---:|
| **10 or 12 / 20 / 5 / 3 (best)** | **0.62 / 0.44 / 0.51** | 42 | 0.67 / 0.53 / 0.59 | 0.44 | 0.57 | 20.6 / 19.8 | 292 of 334 / 279 of 321 (87 %) |
| 0 / 20 / 5 / 3 (length signal off) | 0.58 / 0.44 / 0.50 | 45 | 0.62 / 0.53 / 0.57 | 0.43 | 0.55 | 22.5 | 320 of 365 (88 %) |
| 10 or 12 / 15 / 10 / 3 | 0.63 / 0.41 / 0.49 | 38 | 0.64 / 0.47 / 0.54 | 0.46 | 0.55 | 11.9 / 11.4 | 155 of 193 / 147 of 185 (80 %) |
| 10 or 12 / 15 / 5 / 3 | 0.55 / 0.44 / 0.49 | 47 | 0.62 / 0.53 / 0.57 | 0.43 | 0.54 | 24.8 / 23.7 | 356 of 403 / 338 of 385 (88 %) |
| 0, 10 or 12 / 15 / 20 or 30 / 3 | 0.78 / 0.36 / 0.49 | 27 | 0.86 / 0.40 / 0.55 | 0.45 | 0.54 | 4.8–7.0 | 51–87 of 78–114 (65–76 %) |
| 18 / 20 / 5 / 3 | 0.69 / 0.34 / 0.45 | 29 | 0.75 / 0.40 / 0.52 | 0.43 | 0.51 | 15.9 | 230 of 259 (89 %) |
| 18 / 15 / 15 / 3 | 0.74 / 0.29 / 0.41 | 23 | 0.83 / 0.33 / 0.48 | 0.38 | 0.46 | 5.0 | 58 of 81 (72 %) |
| 15 / 10 / 10 / 3 (loose) | 0.42 / 0.37 / 0.40 | 52 | 0.40 / 0.47 / 0.43 | 0.27 | 0.43 | 17.7 | 235 of 287 (82 %) |
| 12 / 20 / 20 / 3 | 0.80 / 0.20 / 0.32 | 15 | 0.80 / 0.27 / 0.40 | 0.30 | 0.36 | 2.1 | 19 of 34 (56 %) |
| 15 / 20 / 15 / 3 | 0.65 / 0.19 / 0.29 | 17 | 0.67 / 0.27 / 0.38 | 0.29 | 0.32 | 3.0 | 31 of 48 (65 %) |
| 20 / 10 / 10 / 3 | 0.46 / 0.19 / 0.27 | 24 | 0.57 / 0.33 / 0.42 | 0.09 | 0.30 | 12.6 | 181 of 205 (88 %) |
| **18 / 20 / 20 / 3 (defaults)** | **0.73 / 0.14 / 0.23** | 11 | 0.75 / 0.20 / 0.32 | 0.21 | 0.26 | 1.8 | 19 of 30 (63 %) |
| 20 / 15 / 15 / 3 | 0.50 / 0.10 / 0.17 | 12 | 0.75 / 0.20 / 0.32 | 0.11 | 0.19 | 4.1 | 54 of 66 (82 %) |
| 25 / 30 / 30 / 3 | 0.00 / 0.00 / 0.00 | 2 | 0.00 / 0.00 / 0.00 | 0.00 | 0.00 | 0.1 | 0 of 2 |

Paragraph F1 peaks at 0.51 with mean sentence length 10 or 12 (identical predictions), past perfect
20 % and cognition 5 %, three paragraphs. The best run-level any-overlap F1 is 0.60 at 25 / 5 / 5 /
2, which reaches it with long runs (paragraph F1 0.41). The optimum is flat: 149 of 350 settings reach
paragraph F1 ≥ 0.40, and with 59 positive paragraphs the top dozen are within noise of each other.
Two levers matter. Lowering cognition to 5 % alone lifts F1 from 0.23 to 0.45 (18 / 20 / 5);
lowering mean sentence length below 15 adds a little more, and turning it off costs only 0.01. The
length threshold mostly removes true slow paragraphs here. Allowing two-paragraph runs gains
recall but loses more precision; the best such setting reaches 0.47.

**The label-optimal settings are not deployable on label F1 alone.** Precision is measured only
inside the 40 labeled windows, and the loosened settings put most of their new flags outside them.
At 12 / 20 / 5 the report flags 321 paragraphs across the source documents, against 30 at defaults
(19.8 against 1.8 per 1,000). Only 42 of the 321 are inside a labeled window, 38 of those in
`flagged`-stratum windows chosen around the default and loose runs; the `ordinary` stratum gets no
predictions at either setting. Its precision of 0.62 therefore describes the paragraphs next to the
report's existing runs. Nothing measures the other 279 flags (87 %), which are most of what a writer
would see. A candidate setting needs its out-of-window flags sampled and labeled, or a published
flag-rate check, before it ships.

PR #291 set the defaults to 18 / 15 / 15 from both these labels and published-fiction flag rates
(see [published-fiction calibration](#published-fiction-calibration-2026-10-16)). On these labels 18 / 15 / 15 flags 23 paragraphs, 17 of them slow
(paragraph F1 0.41, run any-overlap F1 0.48), and 81 across the source documents, 58 of them outside
the windows. Its first proposal, 15 / 20 / 15 from published-fiction flag frequency, adds 6
predicted paragraphs, 3 of them slow: paragraph F1 0.23 → 0.29 and run any-overlap F1 0.32 → 0.38,
with precision 0.73 → 0.65. A paired bootstrap over passages (4,000 resamples) puts the paragraph F1
gain's 95% interval at −0.03 to +0.23, so the labels are consistent with the change but do not
establish an improvement. Neither half moves anything alone: 15 / 20 / 20 and 18 / 20 / 15 predict
exactly what the defaults do. The best grid setting beats both 18 / 20 / 20 (+0.29, interval +0.07
to +0.48) and 15 / 20 / 15 (+0.23, interval +0.05 to +0.39).

These labels are model judgments on nineteenth- and early twentieth-century prose plus the five
invented stories, the sweep tunes on the same 305 paragraphs it reports, and the stratification
over-represents flagged and near-miss passages. Treat the sweep as direction for the calibration on
published contemporary fiction, not as a held-out estimate, and read each setting's precision
together with its flag volume outside the windows.

## Published-fiction calibration (2026-10-16)

```sh
python3 bench/style/run.py tier3 --pacing --self-test
python3 bench/style/run.py tier3 --pacing --export /approved/export   # owner only
```

The owner-only run pools units by `author_key` and prints only aggregates: the pooled
per-paragraph signal distributions, the distribution of author signal means, per-author flag rates
and the share of authors with any run, as before; plus author-weighted percentiles of each signal
over **eligible paragraphs** (no speech, at least one counted word), the same chapter windows as
`--flag-rates` (scene-break split, target 2,000–6,000 words: closed after reaching 6,000 or at a
scene break past 3,000, short tails merged; each window weighted 1 / its author's
windows), and a 125-setting sweep (length 15/18/20/22/25 × past perfect 10/15/20/25/30 × cognition
10/15/20/25/30, with `minimumParagraphs` 3 and `moreThanWords` 400 fixed). The probe builds each
document once and reruns the report per setting. It prints no text, titles, identifiers, author
keys or per-author rows. The aggregate, with provenance, is the `published` block of
[`pacing-baseline.json`](../bench/style/pacing-baseline.json).

Input: `cmlarsen/fiction-baseline` at b464cac (EXTRACTOR_VERSION 1), 210 units, 71 authors, 1,559
chapter windows, 91,391 eligible paragraphs (85,747 with a verb phrase); 14.5 minutes.

| Eligible-paragraph signal | p25 | Median | p75 | p90 | p95 |
|---|---:|---:|---:|---:|---:|
| Mean sentence length (words) | 9.50 | 14.50 | 20.00 | 26.00 | 31.00 |
| Past perfect % | 0 | 0 | 7.69 | 20.00 | 28.57 |
| Cognition % | 0 | 0 | 5.88 | 14.29 | 22.22 |

The previous 18 / 20 / 20 sat at uneven points of these distributions: length near the 66th
percentile, past perfect at the 90th, cognition near the 93rd. At those thresholds 6.6% of
eligible published paragraphs met two signals, but runs appeared in only **3.4%** of chapter
windows and 32.4% of authors' books; per-author runs per 1,000 words had a median of 0 and a p95 of
0.037.

| Length / past perfect / cognition | Chapter windows with a run | Authors with a run | Chapter runs per 1,000 words, p95 |
|---|---:|---:|---:|
| 15 / 10 / 10 | 34.7% | 78.9% | 0.49 |
| 15 / 15 / 15 | 14.4% | 62.0% | 0.28 |
| 18 / 15 / 20 | 5.3% | 42.3% | 0.14 |
| 18 / 20 / 15 | 7.1% | 38.0% | 0.16 |
| 18 / 20 / 20 (previous) | 3.4% | 32.4% | 0 |
| 15 / 20 / 20 | 5.6% | 42.3% | 0.16 |
| 15 / 20 / 15 (first proposal) | 9.6% | 47.9% | 0.16 |
| 18 / 15 / 10 (rejected) | 14.1% | 59.2% | 0.20 |
| **18 / 15 / 15 (defaults)** | **9.0%** | **46.5%** | **0.16** |
| 20 / 20 / 20 | 2.7% | 25.4% | 0 |
| 25 / 30 / 30 | 0.1% | 2.8% | 0 |

Every setting's chapter median is 0 runs; the full grid, with author p5/p50/p95 rates and run
paragraph shares, is in the baseline file.

### Label accuracy against published rates (2026-10-16)

PR #293 scored the report against a five-model blind consensus on 305 public paragraphs in 40
passages (Fleiss' kappa 0.71; 59 paragraphs labeled slow) over a 350-setting grid, in
`bench/style/pacing-labels-sweep.json` on that branch. Its grid and the published sweep share 64
settings: length 15 / 18 / 20 / 25 × past perfect 10 / 15 / 20 / 30 × cognition 10 / 15 / 20 / 30, at
`minimumParagraphs` 3. The best label setting overall, 10–12 / 20 / 5 (F1 0.51), has no published
rate because cognition 5 was not swept. The table joins the two committed aggregates; no new
private run was made. Label scores are paragraph-level against the consensus; published shares are
the author-weighted chapter windows and authors' books with at least one run.

| Length / past perfect / cognition | Label P | Label R | Label F1 | Predicted paragraphs | Published chapters with a run | Published authors with a run |
|---|---:|---:|---:|---:|---:|---:|
| 18 / 10 / 10 | 0.63 | 0.37 | 0.468 | 35 | 23.6% | 70.4% |
| 18 / 10 / 15 | 0.69 | 0.34 | 0.455 | 29 | 17.0% | 60.6% |
| 18 / 15 / 10 (rejected) | 0.67 | 0.34 | 0.449 | 30 | 14.1% | 59.2% |
| 15 / 15 / 10 | 0.59 | 0.34 | 0.430 | 34 | 24.8% | 73.2% |
| 15 / 15 / 30 | 0.77 | 0.29 | 0.420 | 22 | 8.5% | 53.5% |
| **18 / 15 / 15 (defaults)** | **0.74** | **0.29** | **0.415** | **23** | **9.0%** | **46.5%** |
| 15 / 15 / 20 | 0.74 | 0.29 | 0.415 | 23 | 10.5% | 57.7% |
| 18 / 15 / 30 | 0.76 | 0.27 | 0.400 | 21 | 4.5% | 39.4% |
| 15 / 15 / 15 | 0.65 | 0.29 | 0.400 | 26 | 14.4% | 62.0% |
| 15 / 10 / 10 | 0.42 | 0.37 | 0.396 | 52 | 34.7% | 78.9% |
| 15 / 10 / 15 | 0.48 | 0.34 | 0.396 | 42 | 26.0% | 73.2% |
| 18 / 15 / 20 | 0.73 | 0.27 | 0.395 | 22 | 5.3% | 42.3% |
| 18 / 10 / 30 | 0.67 | 0.27 | 0.386 | 24 | 10.3% | 52.1% |
| 18 / 10 / 20 | 0.64 | 0.27 | 0.381 | 25 | 12.8% | 56.3% |
| 15 / 10 / 30 | 0.50 | 0.29 | 0.366 | 34 | 17.7% | 66.2% |
| 15 / 10 / 20 | 0.49 | 0.29 | 0.362 | 35 | 21.3% | 69.0% |
| 15 / 20 / 10 | 0.58 | 0.24 | 0.337 | 24 | 18.0% | 63.4% |
| 15 / 20 / 15 (first proposal) | 0.65 | 0.19 | 0.289 | 17 | 9.6% | 47.9% |
| 18 / 20 / 10 | 0.61 | 0.19 | 0.286 | 18 | 10.0% | 49.3% |
| 20 / 10 / 10 | 0.46 | 0.19 | 0.265 | 24 | 18.9% | 63.4% |
| 20 / 10 / 15 | 0.50 | 0.15 | 0.234 | 18 | 11.0% | 47.9% |
| 25 / 10 / 10 | 0.47 | 0.15 | 0.231 | 19 | 3.9% | 38.0% |
| 20 / 15 / 10 | 0.47 | 0.15 | 0.231 | 19 | 10.7% | 49.3% |
| 18 / 20 / 30 | 0.73 | 0.14 | 0.229 | 11 | 2.8% | 29.6% |
| 18 / 20 / 20 (previous) | 0.73 | 0.14 | 0.229 | 11 | 3.4% | 32.4% |
| 15 / 20 / 30 | 0.73 | 0.14 | 0.229 | 11 | 4.0% | 35.2% |
| 15 / 20 / 20 | 0.73 | 0.14 | 0.229 | 11 | 5.6% | 42.3% |
| 18 / 20 / 15 | 0.73 | 0.14 | 0.229 | 11 | 7.1% | 38.0% |
| 25 / 15 / 10 | 0.53 | 0.14 | 0.216 | 15 | 2.1% | 21.1% |
| 15 / 30 / 10 | 0.58 | 0.12 | 0.197 | 12 | 11.1% | 49.3% |
| 20 / 20 / 10 | 0.50 | 0.12 | 0.192 | 14 | 7.8% | 43.7% |
| 25 / 20 / 10 | 0.60 | 0.10 | 0.174 | 10 | 1.7% | 18.3% |
| 25 / 15 / 30 | 0.55 | 0.10 | 0.171 | 11 | 0.6% | 8.5% |
| 20 / 15 / 30 | 0.55 | 0.10 | 0.171 | 11 | 2.4% | 29.6% |
| 25 / 15 / 20 | 0.50 | 0.10 | 0.169 | 12 | 1.4% | 14.1% |
| 25 / 15 / 15 | 0.50 | 0.10 | 0.169 | 12 | 1.5% | 15.5% |
| 20 / 15 / 20 | 0.50 | 0.10 | 0.169 | 12 | 3.5% | 33.8% |
| 20 / 15 / 15 | 0.50 | 0.10 | 0.169 | 12 | 5.2% | 36.6% |
| 25 / 10 / 30 | 0.43 | 0.10 | 0.164 | 14 | 1.5% | 19.7% |
| 20 / 10 / 30 | 0.43 | 0.10 | 0.164 | 14 | 6.2% | 40.8% |
| 25 / 10 / 20 | 0.40 | 0.10 | 0.162 | 15 | 2.5% | 25.4% |
| 25 / 10 / 15 | 0.40 | 0.10 | 0.162 | 15 | 2.6% | 26.8% |
| 20 / 10 / 20 | 0.40 | 0.10 | 0.162 | 15 | 8.9% | 46.5% |
| 18 / 30 / 30 | 0.67 | 0.07 | 0.123 | 6 | 0.7% | 14.1% |
| 15 / 30 / 30 | 0.67 | 0.07 | 0.123 | 6 | 1.0% | 15.5% |
| 18 / 30 / 20 | 0.67 | 0.07 | 0.123 | 6 | 1.5% | 16.9% |
| 15 / 30 / 20 | 0.67 | 0.07 | 0.123 | 6 | 1.8% | 19.7% |
| 18 / 30 / 15 | 0.67 | 0.07 | 0.123 | 6 | 3.7% | 19.7% |
| 18 / 30 / 10 | 0.67 | 0.07 | 0.123 | 6 | 6.4% | 33.8% |
| 25 / 20 / 30 | 0.57 | 0.07 | 0.121 | 7 | 0.4% | 5.6% |
| 25 / 20 / 20 | 0.57 | 0.07 | 0.121 | 7 | 1.2% | 9.9% |
| 25 / 20 / 15 | 0.57 | 0.07 | 0.121 | 7 | 1.3% | 11.3% |
| 20 / 20 / 30 | 0.57 | 0.07 | 0.121 | 7 | 1.6% | 22.5% |
| 20 / 20 / 20 | 0.57 | 0.07 | 0.121 | 7 | 2.7% | 25.4% |
| 20 / 20 / 15 | 0.57 | 0.07 | 0.121 | 7 | 4.5% | 33.8% |
| 15 / 30 / 15 | 0.44 | 0.07 | 0.118 | 9 | 4.2% | 25.4% |
| 25 / 30 / 30 | 0.00 | 0.00 | 0.000 | 2 | 0.1% | 2.8% |
| 20 / 30 / 30 | 0.00 | 0.00 | 0.000 | 2 | 0.6% | 11.3% |
| 25 / 30 / 20 | 0.00 | 0.00 | 0.000 | 2 | 0.9% | 5.6% |
| 25 / 30 / 15 | 0.00 | 0.00 | 0.000 | 2 | 0.9% | 5.6% |
| 25 / 30 / 10 | 0.00 | 0.00 | 0.000 | 2 | 1.1% | 9.9% |
| 20 / 30 / 20 | 0.00 | 0.00 | 0.000 | 2 | 1.4% | 14.1% |
| 20 / 30 / 15 | 0.00 | 0.00 | 0.000 | 2 | 2.8% | 15.5% |
| 20 / 30 / 10 | 0.00 | 0.00 | 0.000 | 2 | 3.4% | 21.1% |

The label set is not held out: its passages were sampled partly around runs at the defaults and
at 15 / 10 / 10, and the sweep tunes on it. One paragraph moves F1 by 0.005–0.02.

**Decision (2026-10-16): defaults 18 / 15 / 15** (was 18 / 20 / 20). The rule was the highest label
F1 with published chapter windows with a run at or below about 15%, since pacing is an opt-in lens.
18 / 15 / 15 beats this PR's first proposal, 15 / 20 / 15, on every measure, and the previous
defaults on every label score at a higher published rate: label precision 0.74 (previous 0.73,
first proposal 0.65), recall 0.29 (0.14, 0.19), F1 0.415 (0.229, 0.289); published chapters with a
run 9.0% (3.4%, 9.6%) and authors 46.5% (32.4%, 47.9%).
Past perfect moves to 15, between its published p75 (7.7) and p90 (20); cognition to 15, its
published p90 (14.3, rounded); length stays at 18. Tier 2 stays at 45 cases with 20 / 20 exact
spans.

- **18 / 15 / 10 rejected.** It has the highest F1 under the rule (0.449 at 14.1% of chapters) but
  flags story4's untouched opening (paragraphs 0–3) in every story4 tier 2 case, all five negative
  controls included, so tier 2 precision falls to 0.59 with nine failing cases. The consensus labels
  those paragraphs not slow (at most two of five votes), and its F1 gain over 18 / 15 / 15 is three
  true positives for four false positives on a set used for tuning.
- **15 / 15 / 30 ties.** It has the same 17 true-positive paragraphs with one fewer prediction
  (F1 0.420), the same run-level F1, the same public runs and tier 2 result, and 8.5% of chapters,
  but flags more authors (53.5%). Its cognition threshold lies above the published p95 (22.2), so
  the cognition signal would almost never fire.
- **Public cost.** On the ten public texts, the defaults add story1's pier walk (noise) and opening
  situation (borderline) to the two useful runs. The single-judge public sample favored 15 / 20 / 15;
  the larger blind label set does not.

No published passage was read or labeled, so the published side is a flag rate, not a precision.
`minimumParagraphs` and `moreThanWords` were not swept against the published set.
