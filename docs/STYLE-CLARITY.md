# Clarity: sticky sentences and long subordinate clauses

`style::reports::clarity` (#302, part of #296) is an **opt-in lens** with two signals:

- **Sticky sentences.** Sentences where glue words (closed-class words such as determiners,
  prepositions, conjunctions, pronouns, *to*, modals, and forms of *be*/*have*/*do*) make up most of the
  words.
- **Long subordinate clauses.** Clauses opened by a subordinating conjunction or a relative or
  WH-word that run on for many words before any punctuation.

Like every style report it diagnoses and never generates replacement prose. A flag marks a place
to inspect, not a judgment that the sentence is wrong. Glue-heavy dialogue is often exactly how
people talk, and a long clause can be deliberate rhythm. The report needs no new data: both signals
come from the tagger readings and chunker output that `analyze()` already produces.

The thresholds were set on 2026-10-20 from flag rates on public texts; see
[public calibration](#public-calibration-2026-10-20). There is no baseline, no population
comparison, and no labeled set. The published data fix how often the lens fires, not whether a
flagged sentence is worth inspecting.

```rust
use writemorebetter_style::reports::clarity::{report, ClarityOptions};
let clarity = report(&document, &ClarityOptions::new())?;
// Retained StyleDocument: document.clarity(None)?
```

```javascript
const clarity = JSON.parse(document.clarity());
const strict = JSON.parse(document.clarity('{"stickyGluePercent":70,"longClauseWords":25}'));
```

```swift
let clarity: ClarityReport = try document.clarity()
let strict = try document.clarity(optionsJson: "{\"stickyGluePercent\":70}")
```

## Sticky sentences

A word is a **glue word** when any of these holds:

1. It has a verb reading (`VB*`) whose lemma is *be*, *have* or *do*: *was*, *’s*, *had*, *did*,
   *being*.
2. It has a closed-class reading (`DT`, `PDT`, `IN`, `CC`, `PRP`, `PRP$`, `WP`, `WP$`, `WDT`, `TO`,
   `MD`, `EX`, `RP`, `POS`), and the chunk does not show that an open-class reading is the one
   in use. The tagger leaves many words ambiguous, so the chunk decides:
   - In a verb phrase, a word with a verb reading is content unless it is also a modal: *They
     like the rain* (content) against *She can open it* (glue).
   - At the end of a noun phrase, a word with a noun, adjective or number reading is content:
     *She kicked the can*.
   - A word with an adverb reading is content unless it sits in a prepositional, subordinate or
     particle chunk, so *just*, *now* and *more* are content while *up* in *gave up* is glue.
     Existential *there* (`EX`) stays glue.
   - An adjective reading inside an adjective phrase is content.

*Not* and *n’t* are adverbs and count as content. Words are counted the way every style report
counts them (`Sentence::words`): contractions such as *’s* and *’d* are their own words, and
skipped names drop out of both the glue count and the sentence's word count.

A sentence is **sticky** when it has at least `minimumSentenceWords` words and its glue share is at
least `stickyGluePercent`. The length floor exists because short sentences reach high glue shares
trivially (*It was all of it that she had.* is 88% glue). Each `stickySentences` row carries the
sentence `span`, its document `sentence` index, `mode`, `words`, `glueWords`, `gluePercent`, and the
`glue` spans themselves, so a UI can highlight the glue inside the sentence.

This glue set is broader than some commercial checkers' lists, because pronouns and all forms of
*be*/*have*/*do* count. Glue share is around 50% of all words in the published texts below, and the
median eligible sentence is at 50–53%. The default threshold therefore sits near the 95th
percentile of eligible sentences rather than at a "half the words" rule of thumb.

## Long subordinate clauses

A clause opens at a word that is any of:

- **Chunked `B-SBAR`** by the chunker (*because*, *although*, *whether*, *if*, *as*, *than*,
  *before*, *so* in *so that*). Bare *so* is a discourse marker and does not open a clause. The
  chunker also gives a demonstrative pronoun *that* `B-SBAR`, so a `B-SBAR` *that* needs a noun
  phrase and verb phrase after it (*He said that the ship had sailed*, not *She knew that and
  kept it*).
- **A WH-word** with a `WP`, `WP$`, `WDT` or `WRB` reading (*who*, *whom*, *whose*, *which*, *what*,
  *when*, *where*, *how*, *why*). As the first word of a sentence or of a quote, a WH pronoun never
  opens a clause, and a WH adverb opens one only if the next sentence-ending mark after it is not
  a question mark (*Who knows?*, *When will you come?* and *“Why did it sink?” she asked.* do not;
  *When she came, …* does). *That* opens a clause only as a one-word noun phrase (*the house that
  stood*), never as a determiner (*that apple*). The `kind` is `relative`, except that a WH adverb
  that does not follow a noun (*When the rain stopped*, *she left when it rained*) is a
  `subordinator`; *the night when* stays `relative`.
- **A subordinating conjunction the chunker labels as a preposition**, such as *after*, *since*,
  *until*, *till*, *once*, *unless*, *while* (a fixed list in `clarity.rs`), with an `IN` or `CC`
  reading, and only when a noun phrase and then a verb phrase follow. That separates
  *after the rain stopped* (a clause) from *after dinner* (a phrase).

The clause runs until the first of:

- a punctuation boundary: `, ; : ( ) [ ] { } — – … " “ ” « » . ! ?` or `--`;
- a change between narration and dialogue, so a quote edge ends a clause even without punctuation;
- an **unpunctuated main clause**: once the clause has a verb phrase, a new noun phrase followed
  (adverbs aside) by a verb phrase, directly after
  - a content word that is not a verb: *When I came back from the East last autumn* | *I felt …*;
  - a verb, when the new subject is *I*, *he*, *she*, *we* or *they*, which cannot be its object:
    *When the rain finally stopped* | *we pulled on …*;
  - *and*, *but*, *or* or *yet*, which then stays outside the clause: *when the storm had torn the
    roof from the barn* | *and her father stood …*. Coordinated verbs without a new subject
    (*who sailed the sea and carried letters home*) stay inside.

  A glue word before the new subject keeps it inside, so nested clauses (*if we called*) are not
  cut off.

Nested introducers inside an open clause extend it instead of starting a second, overlapping
one, so a chain such as *which is why the island felt smaller …* is measured as one clause.
U+2028 and hard-wrap line breaks are whitespace and do not end a clause. A closing single quote
`’` is not a boundary (it doubles as an apostrophe), so an untagged-comma single-quoted line
(*‘…on the hill’ she said.*) runs into its tag, and single-quoted speech reads as narration. A clause of at least
`longClauseWords` words is flagged. Each `longClauses` row carries the clause `span`, the
`introducer` (folded) and its `introducerSpan`, `kind` (`subordinator` or `relative`), `words`, the
whole sentence's `sentenceWords` for context, `sentence` and `mode`.

## Groups

`groups` is a `Split<ClarityGroup>` (`all`, `narration`, `dialogue`), with sentences assigned by
the shared sentence mode and clauses by the mode at their introducer. `longClauses` therefore
counts by the clause's mode and `longClauseSentences` by the sentence's mode: in *She turned to him
and asked, “Why …”* with a long clause inside the quote, the clause counts under `dialogue` and the
sentence under `narration`. Within `all` the two always agree.

| Field | Meaning |
|---|---|
| `sentences` | sentences in the group |
| `eligibleSentences` | sentences with at least `minimumSentenceWords` words |
| `lowConfidence` | fewer than 200 (or `minimumSentences`, if higher) sentences |
| `glue` | `Share` of glue words over all counted words |
| `sticky` | `Share` of sticky sentences over eligible sentences |
| `subordinateClauses` | clauses detected, of any length |
| `longClauses` | clauses at or above `longClauseWords` |
| `longClauseSentences` | `Share` of sentences containing a long clause |

As in every report, percentages are null below `minimumSentences`, and `difference` is null because
there is no baseline yet. Counts are always present.

## Options

All Rust options are `#[non_exhaustive]`, with `new()`/`default()` and matching `with_*` builders.
Bindings accept camelCase JSON and snake_case aliases, and reject unknown keys. Native methods
return `Result<_, StyleError>`; compact documents return `needsTokenDetails`.

| Option | Default | Meaning |
|---|---|---|
| `minimumSentences` | 50 | sample size for non-null percentages |
| `minimumSentenceWords` | 15 | inclusive length floor for a sticky flag; positive |
| `stickyGluePercent` | 65 | inclusive glue share for a sticky flag; 0–100 |
| `longClauseWords` | 20 | inclusive clause length for a long-clause flag; positive |

## Not sentence length, not TOO_LONG_SENTENCE

Neither signal measures sentence length. `sentenceLength`'s `includeLongSentences` lens and the
`nlprule` port of LanguageTool's `TOO_LONG_SENTENCE` flag sentences by word count. Clarity's length
floor only decides which sentences are long enough for a glue share to mean anything, and a long
clause is measured from its introducer to its boundary, not over the sentence. At the defaults on
the texts below, 3 of the 72 sticky flags and 11 of the 21 long-clause flags fall in sentences of
40 or more words (sentence length's default `longSentenceWords`). The rest would not be flagged by
either length check.

## Public calibration (2026-10-20)

Reproduce with `python3 bench/style/clarity.py` (add `--review` to write the default flags' text
to ignored `bench/style/out/clarity-review.jsonl`). It runs
`style/examples/clarity_calibration.rs` over the six `bench/public` texts and the four Standard
Ebooks chapters pinned in `bench/style/sources.json` (the pacing inputs; CC0, fetched into ignored
`.cache/`). Story1–5 are invented public fixtures, not published fiction, so they are reported
separately. Moby is the full hard-wrapped `moby150k.txt`, front matter included. No private corpus
text was used.

At the defaults (15 words, 65% glue, 20-word clauses):

| Input | Words | Sentences | Eligible | Glue % | Eligible glue p50 / p95 | Sticky | Clauses | Long clauses |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| story1 | 2,209 | 119 | 74 | 53.7 | 53 / 65 | 5 | 68 | 1 |
| story2 | 2,038 | 131 | 69 | 55.3 | 56 / 64 | 2 | 72 | 1 |
| story3 | 2,008 | 127 | 75 | 53.4 | 52 / 65 | 5 | 64 | 0 |
| story4 | 2,175 | 128 | 78 | 53.6 | 54 / 65 | 5 | 77 | 0 |
| story5 | 1,856 | 117 | 70 | 56.1 | 56 / 67 | 11 | 67 | 0 |
| moby150k (hard-wrapped) | 25,002 | 1,126 | 693 | 49.7 | 50 / 62 | 20 | 689 | 8 |
| The Mysterious Affair at Styles, ch. 1 | 4,161 | 354 | 99 | 49.1 | 50 / 62 | 4 | 101 | 0 |
| The Great Gatsby, ch. 1 | 6,067 | 348 | 166 | 50.5 | 51 / 64 | 7 | 163 | 2 |
| The Sign of the Four, ch. 1 | 2,949 | 198 | 83 | 53.3 | 53 / 67 | 6 | 91 | 3 |
| Anne of Avonlea, ch. 1 | 3,522 | 196 | 98 | 50.4 | 50 / 66 | 7 | 122 | 6 |

**Published flag rates** (Moby plus the four chapters: 41,701 words, 2,222 sentences, 1,139
eligible): 44 sticky sentences, which is 3.9% of eligible sentences, 2.0% of all sentences and
1.1 per 1,000 words. 19 long clauses in 19 sentences, which is 0.9% of sentences and 0.5 per
1,000 words. Per text, sticky flags range from 1.1% (Styles) to 3.6% (Avonlea) of sentences,
and long clauses from none (Styles) to 3.1% (Avonlea, whose first chapter is mostly long
speeches). The invented stories flag more sticky sentences (28 of 622, 4.5%) and fewer long
clauses (2).

Sweep, sticky sentences (flags summed; published / stories):

| Minimum words | 55% | 60% | **65%** | 70% | 75% |
|---|---:|---:|---:|---:|---:|
| 10 | 445 / 189 | 215 / 110 | 69 / 38 | 30 / 14 | 8 / 3 |
| 12 | 400 / 181 | 170 / 102 | 58 / 35 | 19 / 11 | 8 / 3 |
| **15** | 330 / 166 | 138 / 93 | **44 / 28** | 12 / 9 | 2 / 1 |

Sweep, long clauses (published / stories):

| Clause words | 15 | 18 | **20** | 25 | 30 |
|---|---:|---:|---:|---:|---:|
| Flags | 90 / 9 | 34 / 4 | **19 / 2** | 4 / 1 | 1 / 0 |

The defaults were chosen to keep each signal near or below one flag per 1,000 published words, so
that a chapter shows a handful rather than a highlight on every page. At 60% glue the lens flagged
one published sentence in eight eligible ones, and at 15 words a long-clause flag fell on 4.1% of
published sentences. Neither rate supports a lens a writer would leave on.

**Spot judgment (implementer, not blind).** All 72 default sticky flags were read with their glue
words marked. Every one is genuinely function-word-dense: chains of pronoun + auxiliary +
preposition (*I had thanked her, and she had looked embarrassed, and we had both pretended it was
about the measurements*), period dialogue and sermon, and *as if it were* constructions. One
misclassification was seen: the noun *will* in *the will of God* counts as glue (a chunker miss),
and that sentence stays above 65% without it. Plain conversational lines make up a visible share,
which is the expected way this lens over-reads fiction. All 21 default long-clause
flags were read: 19 are correctly bounded long subordinate or relative clauses. One is a garden
path where the relative clause absorbs the main verb (*which formed its back was adorned …*), and
in one a participial phrase of the main clause stays inside a fronted clause (*finally when it
reached the house drifting up the side …*). Before the unpunctuated-main-clause rule and the
bare-*so* exclusion, the defaults gave 38 long-clause flags, among them fronted clauses run into
their main clauses (Gatsby's *When I came back from the East last autumn I felt …* and *When I
reached my estate at West Egg I ran …*). The first review's version, with that rule but without
the coordination and pronoun-subject closes, gave 24: three ran through *and*/*but* plus a new
subject (*… our pasture and I noticed the other day …*, *… but I do place him …*). Those three
were the only change when the coordination rule went in; sticky flags were unchanged.

## Limitations

- The tagger is not disambiguated for every word and the chunker makes errors. *I like rain*
  chunks *like* as a preposition, so it counts as glue, while *They like the rain* chunks it as a
  verb.
- Clause ends are heuristic. A participial phrase or a main verb after an unpunctuated relative
  clause (a garden path) stays inside it, a main clause after a verb and a noun subject (*When it
  stopped the rain …*) is read as the clause's object, and clauses set off only by line breaks
  or by no punctuation at all can run long.
- Free relatives at a sentence start (*What he said was true*) are not detected, by design: the
  same words open questions.
- The thresholds rest on public-domain and invented texts. A modern published-fiction calibration
  (owner-run, aggregate-only, like [pacing's](STYLE-PACING.md#published-fiction-calibration-2026-10-16))
  is still owed, along with a labeled usefulness set.
