# Transitions and pronoun starts

Issue #247 adds two independent **opt-in lenses** over an existing `StyleDocument`. They describe
patterns, never suggest replacement prose, and do not label a particular share as good or bad.
Separate modules keep their options and results independent: transition matching and pronoun-run
measurement have different eligibility rules and neither needs to compute the other report.

| Surface | Transitions | Pronoun starts |
|---|---|---|
| Rust | `transitions::report(&Document, &TransitionsOptions)` | `pronoun_starts::report(&Document, &PronounStartsOptions)` |
| Retained Rust | `document.transitions(options_json)` | `document.pronoun_starts(options_json)` |
| WASM | `document.transitions(optionsJSON?)` | `document.pronounStarts(optionsJSON?)` |
| Swift | `document.transitions(optionsJson:) -> TransitionsReport` | `document.pronounStarts(optionsJson:) -> PronounStartsReport` |

Pronoun starts requires token details; transitions also works on compact documents. Both return
`Result<_, StyleError>` in Rust. Unknown options are errors;
camelCase JSON and snake_case aliases are supported. `minimumSentences` defaults to 50. Each `Split`
group independently suppresses `percent` below this minimum; counts and spans remain available.
`lowConfidence` is true below 200 eligible sentences (or a caller's higher minimum). For pronoun
starts, a sentence whose original quote-skipped opener is a skipped name is excluded from counts and
denominators and breaks runs, as in #244. Transitions measure sentence content rather than its opener,
so name-led sentences stay eligible: the matcher skips only the name tokens, and the repeat window
continues through them (`Mary, however, stayed.` counts a non-opening `however`).
Punctuation-only sentences remain in the denominator. Zero eligible sentences never produce a percentage.

There is no baseline argument yet. All `Share.difference` fields are null and `highlighted` fields
false. The future Ngram `_START_` reference belongs in a separate baseline argument following the
existing report APIs; it is not silently approximated by the sentence-start category baseline.

## Transition measurement

`style/data/transitions.tsv` is an original list, MIT OR Apache-2.0, with contrast, cause, time,
addition, example and condition entries. This is a small, explicit inventory rather than a comprehensive
semantic discourse parser. Common conjunctions are included conservatively. Matching uses the
sentence-start classifier's quote-skipped token view, lowercases text, normalizes curly apostrophes,
and selects the longest matching phrase at a position. Phrase tokens must be adjacent except for
whitespace (including U+2028 inside an unterminated sentence); punctuation cannot be bridged.
`what's more` accepts both straight and curly apostrophes in the tokenizer's contraction tokens.
Original spans preserve case, apostrophes and intervening whitespace.

Cheap ambiguity guards:

- `and` and `but` count only as the first quote-skipped token; noun-phrase conjunctions inside a
  sentence do not count.
- `so` counts only at the opening and directly before I/you/he/she/it/we/they/there. This excludes
  `so tired`, `so much`, and `so that`, at the cost of missing causal `so the ship left`.
- `then` counts only at the opening and followed by a comma. Mid-clause temporal sequencing and
  unpunctuated initial `Then he left` are intentionally excluded; the comma is cheap evidence of
  a discourse marker rather than a complement or modifier.
- `however` needs a following comma, or a preceding comma with no word directly after it
  (`She stayed, however.`). This excludes initial `However hard she tried` and the concessive
  "no matter how" sense after a comma (`an idea, however wild,` and `then, however the captains
  may`). It needs no readings, so compact documents get the same guard.
- `besides` needs a following comma. This excludes the preposition, including after a comma
  (`But, besides the money,`).
- `finally` needs a following or preceding comma.
- `thus` counts at the opening before a comma, or anywhere directly before a subject pronoun or
  `there` (`and thus she left`). This excludes the manner sense (`He stood thus for an hour`,
  `who had thus broken`) and `thus far`, at the cost of missing an unpunctuated causal `Thus the
  plan failed`.
- `instead` does not count before `of`.
- The remaining explicit entries count at any position. `because` includes subordinate clauses and
  `because of`; `also`, `eventually`, and `instead` include clause-internal connectors. Polysemy
  remains possible.

The condition category (#301) includes *if*, *unless*, *provided that*, *as long as* and *in case*. `while` is deliberately omitted: its time, contrast and conditional readings are too ambiguous for the current lexical matcher. Condition shares use the same distinct-sentence denominator as the other categories. This addition can increase total transition counts and repeated pairs in unchanged text; existing category counts do not change.

The report is `{matches, repeats, groups}`. Each match holds its own `span`, global `sentence` index,
canonical `phrase`, `kind`, `opening`, and sentence `mode`. `groups: Split<TransitionsGroup>` contains
`sentences`, `lowConfidence`, `containing`, `opening`, and six `types` rows.
Every share counts **distinct sentences**, not occurrences; one sentence with three causal markers
counts once in the cause share. Type shares can overlap and need not sum to 100%.

`repeatWindow` defaults to **5 sentences**, must be positive, and counts an inclusive window:
two occurrences qualify when their zero-based global sentence index difference is less than 5.
Occurrences inside the same sentence qualify. `repeats` emits consecutive occurrence pairs for each
canonical phrase, with a covering `span`, both exact occurrence `spans`, both `sentences`, and `mode`.
Three nearby uses produce two pairs, not three combinations or one unbounded chain. Ordinary paragraph
breaks preserve the window in narration. Scene/chapter boundaries and changes between dialogue and
narration clear it. So does every paragraph boundary after dialogue: each dialogue paragraph is treated
as one speaker's turn, the same conservative proxy echoes uses, so `“But I can’t,” she said.⏎⏎“But you
must,” he said.` is not a repeat. Repeats inside one dialogue paragraph still count. This keeps
repetition local to one passage and one speaker and prevents stale occurrences from bridging a
different mode. Percentages use sentence mode consistently with #244;
a mixed sentence's transition can lie inside or outside its quoted portion.

## Pronoun measurement and runs

The report now also measures personal and possessive pronouns anywhere among eligible words (#301). Each group has `words`, `pronounWords` (a `Share` whose percent divides by word count), and alphabetical `wordPronouns` rows with the same denominator. These counts include object and possessive forms such as *her*, *my*, *hers* and *theirs*. Word-mode allocation follows the token’s speech span, independent of the sentence opener mode. Empty groups suppress the percent. The existing start and run measurements are unchanged.

The report is `{starts, groups}`. Each start includes its token `span`, global `sentence` index,
lowercase `pronoun`, and `mode`. `groups: Split<PronounStartsGroup>` contains `sentences`,
`excludedSentences`, `lowConfidence`, the overall `pronounStarts` share, alphabetically ordered
`pronouns` share rows, and `runs`. Pronoun rows divide by all eligible sentences, rather than just
pronoun-led sentences. Openers use #244's `opening_tokens` and POS normalization: any surviving PRP
or WP reading qualifies, with explicit personal/reflexive, independent possessive and indefinite
pronouns (including #244's someone/somebody/something/nobody/nothing/everyone/everybody/everything/
anyone/anybody/anything cues). A following nominal/adjectival token excludes ambiguous her/his/whose/
what/which/one determiners (`Her coat`, `What color`, `One evening`); `One cannot know` still counts.
`What’s more` is a transition opener and never counts as a `what` start. Demonstrative this/that/these/those and each/either/
neither qualify before a finite verb (allowing intervening adverbs) or punctuation/end, avoiding
`This book` and complementizer `That she left`. Existential `there` (EX) does not qualify by that
reading alone. These guards are heuristics; ambiguous readings remain a limitation of the tagger.
Leading quotes are skipped; other leading punctuation is retained. Contractions count by their pronoun
head (`She’s` opens with `she`).

A run is **at least three adjacent document sentences** with the same folded pronoun and same mode.
It holds a covering sentence `span`, `pronoun`, half-open global `sentences` range, and `mode`.
Narration and dialogue are separately reported; intervening dialogue breaks a narration run rather
than being removed to manufacture adjacency. Ordinary paragraph breaks **do not reset** a run,
following #241's narration-run decision: visual paragraph layout should not hide a persistent opening
pattern. Scene/chapter boundaries reset runs because they start a new passage. A non-pronoun opener,
skipped opener or different pronoun/mode also resets a run. In dialogue, every paragraph boundary
resets a run, using the same speaker-turn proxy as the transition window: `“She left,” I said.⏎⏎“She
did,” he said.⏎⏎“She will,” I said.` is three speakers' lines, not a run. A run inside one speaker's
paragraph still counts.

**Relationship to `sentence_starts.sameWordRuns` (owner-confirmed 2026-10-16).** The default
same-word signal already flags narration runs of one opener, omitting speech, repeated first-two-word
openings (`She was… She was…`) and all-question/exclamation runs. On public text three of four
narration pronoun runs were exactly those flags, so this lens leaves out any narration run that lies
inside a `sameWordRuns` range; a consumer showing both never sees the same span twice. The lens keeps
what the default signal does not show: per-pronoun shares, dialogue runs within one speaker's turn,
and the repeated-two-word and rhetorical narration runs that `sameWordRuns` suppresses, each with the
pronoun named. Computing this calls `sentence_starts::report` once, only when a narration run exists.
`starts` and the shares are unaffected. It does not
implement or replace LanguageTool's `ENGLISH_WORD_REPEAT_BEGINNING_RULE` or
`PARAGRAPH_REPEAT_BEGINNING_RULE`: those remain grammar/style rule diagnostics. This is descriptive
whole-document measurement with per-mode shares, explicit sample floors and all qualifying pronoun runs.

## Public regression and usefulness

`python3 bench/style/transitions_pronouns.py --gate` scores two sets. `labels/transitions.jsonl` holds
62 hand-labeled invented sentences (36 positives covering the original five types; 26 negatives, including the
manner `thus`, prepositional `besides`, concessive `however` and `instead of` cases). It also records a
known miss: mid-sentence causal `so` is excluded by design. The second set appends 14 fixtures, each
once, after a different pinned public-domain passage with only a paragraph break between them, so the
source prose can interact with the fixture. Five positive runs vary the pattern (same-two-word across a
paragraph, `It’s` contractions, U+2028 separators, one speaker's dialogue, rhetorical `Who…?`). Five
run controls must stay silent (a run `sameWordRuns` already reports, a two-speaker exchange,
alternating pronouns, `One evening…`, `Her coat…`). Four transition-repeat fixtures cover
`What’s more`, narration, one speaker and two speakers. Flags that start in the unchanged source are
counted as diagnostics, not assumed absent. Exact byte and UTF-16 span identity uses the tier 2 scorer.
The gate fails if any measurement differs from `bench/style/transitions-pronouns-baseline.json`, which
includes the scores and the public counts, so a recorded miss is visible rather than hidden by an F1 = 1
requirement. This is a development regression, not held-out editorial accuracy. CI runs it alongside
the existing style gates.

The original labeled baseline: transition precision is 100% for every original type; recall is 100% except cause (10/11, the
mid-sentence `so` miss). Pronoun runs score 5/5 with no false positives on the five controls, and
transition repeats 4/4 with none on the two-speaker control.

Public counts use all of `story1`–`story5`, the full unchanged `moby150k`, and Pride and Prejudice
chapters 1–6 (Standard Ebooks, paragraphs joined by blank lines, chapters by `***`), with line mode for
the stories and hard-wrapped mode for Moby. `transitionMatches` and `pronounStarts` are observations;
`transitionRepeats` and `pronounRuns` are the candidate editorial flags. Parenthesized “was” values refer to the
first #247 PR version (f00baf2).

| Text | Sentences | Transition matches | Repeated pairs | Pronoun starts | Pronoun runs |
|---|---:|---:|---:|---:|---:|
| story1.txt | 119 | 18 | 1 | 61 | 0 |
| story2.txt | 131 | 12 | 0 | 74 | 0 (was 1) |
| story3.txt | 127 | 12 | 0 (was 1) | 69 | 0 (was 1) |
| story4.txt | 128 | 10 | 1 | 59 | 0 |
| story5.txt | 117 | 20 | 4 | 61 (was 63) | 1 (was 2) |
| moby150k.txt | 1126 | 240 | 45 | 211 | 0 (was 1) |
| Pride ch. 1–6 | 411 | 84 | 5 | 137 | 1 (was 3) |

The #301 condition category adds 102 matches and 17 repeated pairs across these public texts. The labeled transition and pronoun-run scores did not change. The public flag review below predates condition; the 17 additional repeat pairs have not been editorially judged. The `(was …)` figures record the first #247 PR version, before the #301 category.

Every pre-#301 candidate flag in these texts was judged by the implementation author, using the #247 reviewer's
rule. Useful means a line editor would plausibly point it out; noise means accurate but not worth
attention; wrong means it rests on a non-transition match. Before the fixes, 55 flags were 5 useful,
49 noise and 1 wrong. After them, 41 flags are **4 useful, 37 noise, 0 wrong**. The fixes removed
10 cross-speaker dialogue flags, 3 narration runs that `sameWordRuns` already reports (including the
one useful Moby I-run, which the default signal still shows) and the prepositional `besides` pair.
Most remaining noise is sentence-opening `But`/`And` two to four sentences apart in Melville's
rhetoric and deliberate same-sentence `because… because` parallels. Rows and reasons are in
`bench/style/transitions-pronouns-review.json`. The judgment is single-judge and not blind. These
results support keeping both reports opt-in, with no share highlights until a calibrated baseline
exists.

## Published-fiction calibration (2026-10-16)

The owner-only `tier3 --flag-rates` run (default options) measured both lenses on the
published-fiction export: `cmlarsen/fiction-baseline` at b464cac (EXTRACTOR_VERSION 1), 210 units,
71 authors, 7.70 million counted words, 1,559 chapter windows. Only aggregate numbers left the run;
the method, window rule and weighting are in the
[benchmark README](../bench/style/README.md#flag-rate-calibration-241). Rates are flags per 1,000
words of that mode's sentences; `share` is the author-weighted share of chapter windows (or, for
manuscripts, of authors) with at least one flag.

| Flag (mode) | Chapter windows flagged | Authors flagged | Per 1,000 words, chapter median (p5–p95) |
|---|---:|---:|---|
| Transition repeats, window 5 (all) | 66.5% | 95.8% | 0.28 (0–1.44) |
| Transition repeats (narration) | 53.1% | 91.5% | 0.20 (0–1.56) |
| Transition repeats (dialogue) | 25.3% | 65.2% | 0 (0–1.37) |
| Pronoun runs (all) | 33.4% | 74.6% | 0 (0–0.48) |
| Pronoun runs (narration) | 10.0% | 46.5% | 0 (0–0.25) |
| Pronoun runs (dialogue) | 27.9% | 71.0% | 0 (0–1.50) |

The repeat window sweep (all modes) shows that no window keeps the flag rare in published prose:

| `repeatWindow` | Chapter windows flagged | Authors flagged | Per 1,000 words, chapter median (p5–p95) |
|---:|---:|---:|---|
| 2 | 42.7% | 87.3% | 0 (0–0.51) |
| 3 | 54.9% | 91.5% | 0.16 (0–0.83) |
| 4 | 63.2% | 95.8% | 0.21 (0–1.11) |
| **5 (default)** | **66.5%** | **95.8%** | **0.28 (0–1.44)** |

**Decision: defaults unchanged.** Two thirds of published chapters contain a repeated transition
within five sentences, and even a two-sentence window flags 43%. Together with the public review
(4 useful of 41 flags), this supports keeping both reports opt-in rather than tuning the window: a
shorter window removes flags without evidence that the remaining ones are the useful ones. Pronoun
runs fire mostly inside one speaker's dialogue; narration runs that `sameWordRuns` does not already
report occur in 10% of published chapters. `pronounStarts` has no threshold to tune, and
`minimumSentences` (50) keeps 99.4% of narration and 79.7% of dialogue windows eligible.

### Published-author bands

[`bench/style/published-bands-transitions-pronouns-sensory.json`](../bench/style/published-bands-transitions-pronouns-sensory.json)
records the author-weighted chapter-window p5/p95 band for every share these reports return, per
mode, with the full percentile sets and manuscript-scale distributions in `calibration`. It follows
the sentence-start band convention (provenance inside the file, aggregate numbers only) and is
built from the tier 3 output by `bench/style/published_bands.py`. Neither report takes a baseline
argument yet, so nothing highlights against these bands; they are the reference for that future
argument. These bands predate the condition category; condition has no published band yet. Selected bands, percent of eligible sentences, p5 / median / p95:

| Share | All | Narration | Dialogue |
|---|---|---|---|
| Containing a transition | 2.38 / 6.59 / 13.23 | 1.87 / 5.93 / 14.35 | 1.40 / 6.53 / 13.67 |
| Opening with a transition | 1.35 / 4.93 / 11.14 | 0.68 / 4.21 / 11.98 | 0.95 / 5.30 / 12.30 |
| Contrast (containing) | 0.68 / 2.37 / 5.40 | 0.31 / 2.27 / 6.45 | 0 / 2.25 / 5.56 |
| Cause (containing) | 0 / 0.93 / 3.44 | 0 / 0.83 / 3.66 | 0 / 0.73 / 3.33 |
| Time (containing) | 0 / 0.25 / 1.51 | 0 / 0.34 / 1.87 | 0 / 0 / 1.02 |
| Addition (containing) | 0.69 / 2.53 / 6.40 | 0.07 / 2.14 / 7.11 | 0 / 2.94 / 7.32 |
| Example (containing) | 0 / 0 / 0.25 | 0 / 0 / 0.33 | 0 / 0 / 0.26 |
| Pronoun starts | 17.91 / 30.94 / 45.27 | 13.01 / 26.21 / 45.98 | 23.80 / 36.53 / 49.28 |
| *I* | 1.03 / 6.67 / 20.12 | 0 / 0.54 / 21.88 | 5.17 / 11.96 / 19.43 |
| *he* | 0.75 / 4.62 / 13.40 | 0.82 / 7.03 / 18.33 | 0 / 1.24 / 5.67 |
| *she* | 0 / 2.39 / 10.04 | 0 / 3.39 / 15.43 | 0 / 0.12 / 4.27 |
| *it* | 1.43 / 3.65 / 6.94 | 0.72 / 3.25 / 8.04 | 0.86 / 3.91 / 7.42 |

Narration and dialogue differ enough (dialogue opens with *I* and *you*, narration with *he* and
*she*) that a blended band would mislead, as #241 found for sentence starts. Per-pronoun narration
bands mostly measure point of view: a first-person chapter sits near the *I* p95 and the *he*/*she*
p5. A future highlight should use only `pronounStarts` per mode and treat per-pronoun bands as
context. The tier 3 probe buckets every pronoun outside *I, you, he, she, it, we, they* into one
`other` count, so no unlisted opener reaches the output.
