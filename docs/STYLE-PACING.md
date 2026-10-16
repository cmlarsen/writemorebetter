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
