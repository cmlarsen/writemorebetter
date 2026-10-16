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

Each row in `paragraphs` has a self-contained `span`, `sceneBreakBefore`, `hasSpeech`, `signals`,
`triggeredSignals` and `slow`. Row order is the document's prose-paragraph order after its heading
and scene-marker exclusions. The original spans remain valid for hard-wrapped text; the report
never reparses paragraphs. See [document conventions](STYLE-DOCUMENT.md).

`PacingSignals` includes raw numerators and denominators so a UI can explain every percentage:

| Field | Definition |
|---|---|
| `words` | `Split<usize>` (`all`, `narration`, `dialogue`), counted by `is_word && !is_skipped`; clitics count as words, as in sentence length |
| `sentences`, `meanSentenceLength` | number of analyzed sentences (including zero-word sentences), and counted words ÷ sentences |
| `dialoguePercent` | speech word tokens ÷ all word tokens × 100; uses `Document::mode_at`, not sentence mode |
| `verbPhrases` | approximate verb heads: a counted token with a `VB*` reading outside a noun phrase; adjacent *be/have/do* auxiliary chains, with intervening adverbs, collapse into one head |
| `pastPerfect`, `pastPerfectPercent` | heads introduced by *had* (`VBD`, lemma *have*) or *'d* followed by `VBN`; count ÷ verb phrases × 100 |
| `cognition`, `cognitionPercent` | heads with a cognition verb lemma from the authored list; count ÷ verb phrases × 100 |

Noun-phrase chunks (`*-NP*`) suppress even mis-tagged noun *thought*. Verb-phrase chunks take
precedence if both chunk families occur. Multiple readings never count a token twice. Auxiliary
chains may cross only whitespace and consecutive adverbs (`RB`, `RBR`, `RBS`), never punctuation,
other words, skipped names, sentences or paragraph boundaries. *Had never been*, *had already gone*
and *had been remembering* count once. *Had a boat* is ordinary possession. Straight and curly
apostrophes normalize alike. Contracted *'d gone* counts, but *'d go*, *'d put* and other cases
where the following token also has a base `VB` reading do not: *would* is still possible.

Cognition is the head of the collapsed phrase: *had remembered* contributes to both signals.
The 22-lemma original list in `style/data/pacing/cognition.txt` includes *think, wonder, remember,
realize/realise, know, imagine, recall, understand*, and related interior verbs. This is a small
editorial vocabulary, not a psychological taxonomy; unlisted verbs do not count. It contains no
data copied or derived from third-party dictionaries or rule resources.

Null means an empty denominator, never zero or NaN. These local measurements remain available
for short paragraphs so a UI can draw a strip; they are not baseline comparisons and have no
50-sentence display floor. A paragraph's `slow` means it contains no speech and meets at least two
of the other three signal thresholds. `hasSpeech` checks the document's speech spans even when
the speech contains only punctuation or all its words were skipped; a rounded zero dialogue share
can never make speech qualify. Scare quotes and quoted titles follow the document's non-speech rule.

`slowRuns` contains maximal consecutive sequences of slow rows, bounded by speech, a nonqualifying
paragraph, recognized headings or scene breaks. A sequence is reported when it has **at least 3
paragraphs OR strictly more than 400 words**, and its pooled signal values also meet at least two
thresholds. Ordinary paragraph breaks do not stop it. Each `PacingRun` owns its covering `span`,
half-open `paragraphs: {start, end}` indexes into this report's rows, pooled `signals` and
`triggeredSignals` (`sentenceLength`, `pastPerfect`, `cognition`). Pooling sums raw counts and then
recomputes rates; it never averages paragraph percentages. Paragraph `slow` is a local indicator;
it does not imply membership in a flagged run. Different combinations of signals can support
adjacent paragraphs, so the pooled check is independent.

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

## Owner-run calibration

```sh
python3 bench/style/run.py tier3 --pacing --self-test
# Owner only, later:
python3 bench/style/run.py tier3 --pacing --export /approved/export
```

The script is a calibration entry point without a published-fiction baseline. It pools units by
`author_key`, prints only anonymous numeric signal distributions and flag rates per author,
and equally weighted distributions across author means/rates. It never emits text or source
metadata. The synthetic self-test checks pooling and metadata suppression. No private corpus,
export, title or author record was accessed to implement or test this report. See the
[benchmark README](../bench/style/README.md#pacing-251) for export layout, weighting and gates.
