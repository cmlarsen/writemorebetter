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

The report is `{matches, repeats, groups}`. Each match holds its own `span`, global `sentence` index,
canonical `phrase`, `kind`, `opening`, and sentence `mode`. `groups: Split<TransitionsGroup>` contains
`sentences`, `lowConfidence`, `containing`, `opening`, and five `types` rows.
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
62 hand-labeled invented sentences (36 positives covering all five types; 26 negatives, including the
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

Current baseline: transition precision is 100% for every type; recall is 100% except cause (10/11, the
mid-sentence `so` miss). Pronoun runs score 5/5 with no false positives on the five controls, and
transition repeats 4/4 with none on the two-speaker control.

Public counts use all of `story1`–`story5`, the full unchanged `moby150k`, and Pride and Prejudice
chapters 1–6 (Standard Ebooks, paragraphs joined by blank lines, chapters by `***`), with line mode for
the stories and hard-wrapped mode for Moby. `transitionMatches` and `pronounStarts` are observations;
`transitionRepeats` and `pronounRuns` are the candidate editorial flags. The before column is the
first PR version (f00baf2).

| Text | Sentences | Transition matches | Repeated pairs | Pronoun starts | Pronoun runs |
|---|---:|---:|---:|---:|---:|
| story1.txt | 119 | 15 | 1 | 61 | 0 |
| story2.txt | 131 | 11 | 0 | 74 | 0 (was 1) |
| story3.txt | 127 | 10 | 0 (was 1) | 69 | 0 (was 1) |
| story4.txt | 128 | 9 | 1 | 59 | 0 |
| story5.txt | 117 | 16 | 4 | 61 (was 63) | 1 (was 2) |
| moby150k.txt | 1126 | 183 (was 201) | 33 (was 35) | 211 | 0 (was 1) |
| Pride ch. 1–6 | 411 | 50 (was 53) | 0 (was 5) | 137 | 1 (was 3) |

Every candidate flag in these texts was judged by the implementation author, using the #247 reviewer's
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
