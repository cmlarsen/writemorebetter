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
`lowConfidence` is true below 200 eligible sentences (or a caller's higher minimum). A sentence whose
original quote-skipped opener is a skipped name is excluded from counts and denominators, and breaks
runs/repetition chains. Punctuation-only sentences otherwise remain in the denominator. Zero eligible
sentences never produce a percentage.

There is no baseline argument yet. All `Share.difference` fields are null and `highlighted` fields
false. The future Ngram `_START_` reference belongs in a separate baseline argument following the
existing report APIs; it is not silently approximated by the sentence-start category baseline.

## Transition measurement

`style/data/transitions.tsv` is an original list, MIT OR Apache-2.0, with contrast, cause, time,
addition and example entries. This is a small, explicit inventory rather than a comprehensive
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
- `however`, `besides`, and `finally` need a following comma or a preceding comma. This excludes
  initial `However hard she tried` and common unpunctuated prepositional/verb-modifier uses.
- The remaining explicit entries count at any position. `because` includes subordinate clauses;
  `also`, `eventually`, and `instead` include clause-internal connectors. Polysemy remains possible.

The report is `{matches, repeats, groups}`. Each match holds its own `span`, global `sentence` index,
canonical `phrase`, `kind`, `opening`, and sentence `mode`. `groups: Split<TransitionsGroup>` contains
`sentences`, `excludedSentences`, `lowConfidence`, `containing`, `opening`, and five `types` rows.
Every share counts **distinct sentences**, not occurrences; one sentence with three causal markers
counts once in the cause share. Type shares can overlap and need not sum to 100%.

`repeatWindow` defaults to **5 sentences**, must be positive, and counts an inclusive window:
two occurrences qualify when their zero-based global sentence index difference is less than 5.
Occurrences inside the same sentence qualify. `repeats` emits consecutive occurrence pairs for each
canonical phrase, with a covering `span`, both exact occurrence `spans`, both `sentences`, and `mode`.
Three nearby uses produce two pairs, not three combinations or one unbounded chain. Ordinary paragraph
breaks preserve the window; scene/chapter boundaries, excluded sentences and changes between dialogue
and narration clear it. This keeps repetition local to a continuous passage and prevents stale
occurrences from bridging a different mode. Percentages use sentence mode consistently with #244;
a mixed sentence's transition can lie inside or outside its quoted portion.

## Pronoun measurement and runs

The report is `{starts, groups}`. Each start includes its token `span`, global `sentence` index,
lowercase `pronoun`, and `mode`. `groups: Split<PronounStartsGroup>` contains `sentences`,
`excludedSentences`, `lowConfidence`, the overall `pronounStarts` share, alphabetically ordered
`pronouns` share rows, and `runs`. Pronoun rows divide by all eligible sentences, rather than just
pronoun-led sentences. Openers use #244's `opening_tokens` and POS normalization: any surviving PRP
or WP reading qualifies, with explicit personal/reflexive, independent possessive and indefinite
pronouns (including #244's someone/somebody/something/nobody/nothing/everyone/everybody/everything/
anyone/anybody/anything cues). A following nominal/adjectival token excludes ambiguous her/his/whose/
what/which determiners (`Her coat`, `What color`). Demonstrative this/that/these/those and each/either/
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
skipped opener or different pronoun/mode also resets a run.

This intentionally differs from `sentence_starts.sameWordRuns`: that default editorial signal omits
speech, repeated first-two-word openings and all-question/exclamation runs. This opt-in census includes
those uses and returns dialogue runs, with the repeated pronoun named explicitly. Consumers should
choose the lens or the default same-word signal rather than add their counts together. It does not
implement or replace LanguageTool's `ENGLISH_WORD_REPEAT_BEGINNING_RULE` or
`PARAGRAPH_REPEAT_BEGINNING_RULE`: those remain grammar/style rule diagnostics. This is descriptive
whole-document measurement with per-mode shares, explicit sample floors and all qualifying pronoun runs.

## Public regression and usefulness

`python3 bench/style/transitions_pronouns.py --gate` runs hand-labeled invented positive/negative
transition sentences and pronoun runs injected into pinned public-domain prose. Its first measurement
is `bench/style/transitions-pronouns-baseline.json`. This is a development regression, not held-out
editorial accuracy. Injected-run scoring covers the injected passage only; runs in the unchanged
public source are counted separately as diagnostics, not assumed absent. Exact byte and UTF-16 span
identity uses the tier 2 scorer. CI runs this suite alongside the existing style gates.

Public-text counts use all of `story1`–`story5` and the full unchanged `moby150k`, with line mode for
stories and hard-wrapped mode for Moby. `transitionMatches` and `pronounStarts` are observations;
`transitionRepeats` and `pronounRuns` are the candidate editorial flags. The committed measurements
and PR record the counts and spot judgments. Both lenses stay opt-in regardless of whether particular
sample flags prove useful: no calibrated threshold or bundled baseline exists for either share.

First baseline (31 labeled sentences, 17 transition positives; 6 injected runs, 6 alternating controls):
all five transition types and injected runs have exact-span precision/recall/F1 **100%**.
These examples were used during development and do not estimate unseen-text accuracy.

| Text | Sentences | Transition matches | Repeated pairs | Pronoun starts | Pronoun runs |
|---|---:|---:|---:|---:|---:|
| story1.txt | 119 | 15 | 1 | 61 | 0 |
| story2.txt | 131 | 11 | 0 | 74 | 1 |
| story3.txt | 127 | 10 | 1 | 69 | 1 |
| story4.txt | 128 | 9 | 1 | 59 | 0 |
| story5.txt | 117 | 16 | 4 | 63 | 2 |
| moby150k.txt | 1126 | 201 | 35 | 211 | 1 |

An implementation-author spot review selected the first up to two repeated-transition pairs and
first up to two pronoun runs per text (12 flags total): **2 useful, 10 noise, 0 wrong**.
The useful examples were nearby causal explanations in story4 and Moby's four-sentence I-led
action sequence. Most noise was deliberate spoken repetition, parallel clauses or anaphora.
Offsets and individual reasons are in `bench/style/transitions-pronouns-review.json`; the sample
is deterministic, small, single-judge and not blind or representative. These results support
keeping both reports opt-in and avoiding share highlights without a calibrated baseline.
