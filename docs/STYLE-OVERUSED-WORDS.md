# Overused and filler words

`style::reports::overused_words` (#248) flags words an author uses far more often than published
fiction does, and reports a short fixed list of filler words by rate. It is an **opt-in lens**, not
part of Subplot's recommended default set. It diagnoses only: it returns counts, rates, ratios and
spans, never replacement words.

```rust
use writemorebetter_style::reports::overused_words::{report, OverusedWordsBaseline, OverusedWordsOptions};
let result = report(&document, &OverusedWordsOptions::new(), OverusedWordsBaseline::bundled())?;
```

```javascript
const overused = JSON.parse(document.overusedWords());               // bundled table
const fillersOnly = JSON.parse(document.overusedWords(undefined, "none"));
const custom = JSON.parse(document.overusedWords(undefined, '{"lemmas":{"glance":294}}'));
```

```swift
let overused = try document.overusedWords()
let custom = try document.overusedWords(baseline: .custom("{\"lemmas\":{\"glance\":294}}"))
```

## Method

For each counted word token (skipped names excluded, as in every report), the report picks one
content lemma:

1. Tokens with a number reading, function words (the echoes list), the filler words and the
   particles *around, away, back, down, like, off, out, round, up* are not candidates. *Like* is
   excluded because its preposition and verb readings are not separable.
2. Proper nouns are excluded: a capitalized token that is not sentence-initial (after any opening
   quotes or a colon), a capitalized spelling that appears mid-sentence anywhere in the document,
   or a capitalized token with an `NNP`/`NNPS` reading.
3. Readings are restricted by chunk: a verb-phrase token keeps its verb readings and a noun-phrase
   head keeps its noun readings. The remaining `NN*`, `VB*`, `JJ*` and `RB*` lemmas are the
   candidates; ambiguous tokens take the candidate with the highest reference rate (*saw* → *see*,
   *found* → *find*).
4. Nouns are left out unless `includeNouns` is true: a token whose chosen lemma has only noun readings
   after step 3 is a noun use.

A lemma is flagged when its count is at least `minimumCount`, its reference rate is at least
`minimumReferencePerMillion`, and

```
ratio = (count / words × 1,000,000) / reference per million ≥ minimumRatio
```

`words` is the document's word count, the same denominator every report uses. Lemmas missing from
the table are never flagged. Flags are sorted by ratio, then count. Each carries the lemma, count,
`perTenThousandWords`, `referencePerTenThousandWords` (the table rate divided by 100, so both rates
share a unit), the ratio and every span. A UI can say "you use *glance* 48 times; published fiction
averages 2.9 per 10,000 words".

| Option | Default | |
|---|---|---|
| `minimumRatio` | 4 | document rate ÷ reference rate |
| `minimumCount` | 5 | occurrences in the document |
| `minimumReferencePerMillion` | 10 | drops rare, archaic and topical words (*bake*, *whale*, *heed*) |
| `includeNouns` | false | nouns are mostly setting and topic (*island*, *ferry*, *ship*) |

The ratio uses the POS-collapsed reference (noun and verb *smile* share one rate), so excluding noun
uses from the document count makes the default conservative for noun/verb homographs.

This differs from LanguageTool's TOO_OFTEN_USED_* rules (#234, #235), which compare a word's share
within the text alone and stay in `nlprule` for conformance. It does not read `lemma_index`, which
lists every reading's lemma and would double-count ambiguous tokens; it chooses one lemma per token.

## Filler words

The fixed list runs with or without a table and is never thresholded: every filler present is
returned with its count, rate per 10,000 words, spans and, when the table has it, the reference rate
and ratio. Sorted by count.

| Word | Counted when | Why |
|---|---|---|
| *just*, *really*, *very*, *quite* | an `RB` reading | intensifiers and hedges editors cut first |
| *actually*, *simply* | an `RB` reading | assertion padding |
| *suddenly*, *somehow* | an `RB` reading | announce an event or skip its cause instead of showing it |
| *that* | the tokenizer chunks it `B-SBAR`, opening a clause (*knew that he*, *so dark that*) | often omissible; determiners (*that house*) and pronoun *that* are not counted |

The tagger never gives complementizer *that* an `IN` reading, so the chunk decides. The chunker
also opens some relative clauses with `B-SBAR` (*the advantages that you've had*): in 18 sampled
Gatsby and story spans, 14 were complementizers, 4 relatives and none determiners. The reference rate is
Ngram's `that_ADP` (complementizers only), so the *that* ratio reads slightly high.

## Reference table

`style/data/baselines/overused-words.json` maps lemma → rate per million words. It is built from
Google Books Ngram English Fiction 1-grams, version 20200217, CC BY 3.0 (see
[STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#word-frequencies-248)), by
`bench/style/overused_baseline.py`:

```sh
curl -o SCRATCH/1-00000-of-00001.gz \
  https://storage.googleapis.com/books/ngrams/books/20200217/eng-fiction/1-00000-of-00001.gz
python3 bench/style/overused_baseline.py extract SCRATCH/1-00000-of-00001.gz SCRATCH/forms.tsv
python3 bench/style/overused_baseline.py build SCRATCH/forms.tsv
```

- Match counts are summed over edition years 1990–2019. Ngram has no book or author dimension, so
  the table is volume-weighted.
- The denominator is every untagged 1-gram containing a letter or digit (97,330,709,341), the
  document's word predicate. Ngram keeps *don't* as one token where the tokenizer counts two, so
  document rates are slightly low relative to the reference for contraction-heavy prose.
- Each `_NOUN`, `_VERB`, `_ADJ` and `_ADV` form with at least 1,000 occurrences is lemmatized by
  `en_tokenizer.bin`'s tagger (`bench/tools/wmb-document` `lemmatize_forms`), keeping readings whose
  Penn tag maps to the form's universal tag. Capitalized forms are dropped when they have a
  proper-noun reading or outnumber their lowercase form (*Daisy*, *Rose*), so names do not inflate
  common nouns. A form with several lemmas goes to the most frequent one, matching step 3 above.
- Function words, particles and fillers are excluded, matching the report. Filler rates come from
  the `_ADV` forms and `that_ADP`.
- The table keeps the 14,720 lemmas at or above 1 per million (a tenth of the default floor), rounded
  to three significant digits: 238,416 bytes of JSON, 85,608 gzipped. The 50,000 most frequent
  lemmas would be 1.17 MB, and every lemma below the default floor can never be flagged.

A caller can pass its own table, for example a genre-specific one, as `{"lemmas": {...},
"fillers": {...}}` with lowercase lemma keys and rates per million. Filler keys must be from the
fixed list; rates must be finite and non-negative. `"none"` skips the comparison and returns
fillers only.

Bundled into WASM, the table and report grow the module from 4,482,886 to 4,839,941 bytes after
`wasm-bindgen` (+357,055, +8.0%; gzipped 1,120,483 → 1,219,768, +99,285) and from 2,537,378 to
2,817,409 bytes after `wasm-opt -Oz` (+280,031, +11.0%). The table's JSON is 238,416 of those bytes.
It is parsed once, on the first call.

## Calibration and usefulness

Thresholds were calibrated on chapter-sized public-domain texts: the five invented stories in
`bench/public/` (1,856–2,209 words), the `moby150k` excerpt split at paragraph breaks into five
windows of about 5,000 words, and the first chapters of four Standard Ebooks texts fetched from their
GitHub repositories (*The Great Gatsby*, *The Sun Also Rises*, *The Mysterious Affair at Styles*,
*The Age of Innocence*). `style/examples/overused_words.rs` prints every flag with options taken from
`WMB_OVERUSED_OPTIONS`.

| Setting (ratio, count, floor) | Nouns | Flags per text: median (min–max) | Total over 14 texts |
|---|---|---:|---:|
| 4, 5, 0 | included | 17 (6–29) | 244 |
| 4, 5, 0 | excluded | 5 (2–9) | 76 |
| 4, 5, 10 | included | 16 (6–28) | 223 |
| 3, 5, 10 | excluded | 7 (5–10) | 105 |
| 4, 4, 10 | excluded | 9 (3–13) | 124 |
| **4, 5, 10** | **excluded** | **5 (2–9)** | **75** |
| 4, 6, 10 | excluded | 3 (0–5) | 41 |
| 5, 5, 10 | excluded | 4 (2–7) | 60 |

Every one of the 75 default flags was judged as a fiction editor would: *useful* if a line edit
would plausibly raise the repetition, *noise* if it is topical, a deliberate refrain, rhetoric or
quoted material, *wrong* if the lemma or part of speech is wrong.

| Texts | Flags | Useful | Noise | Wrong |
|---|---:|---:|---:|---:|
| Stories 1–5 | 25 | 16 | 9 | 0 |
| Standard Ebooks chapters | 16 | 8 | 7 | 1 |
| Moby-Dick windows | 34 | 7 | 26 | 1 |
| **All** | **75** | **31 (41%)** | **42 (56%)** | **2 (3%)** |

- Useful: the stories' *learn* (*I learned the schedule the way you learn a scar*, in three of five
  stories), *small*, *hold*, *keep*, *laugh*, *sit*, *mean* (*which meant I was awake at three, which
  meant…*); Christie's *certainly*; Gatsby's *bright* (*bright things… bright eyes and a bright
  passionate mouth*), *forward* (three characters *leaned forward*) and *surprise*; Melville's
  *strange*, *indeed* and *however*.
- Noise: setting verbs (*sell* in a bakery, *box* for a boxer, *sail*, *sleep* in the bed-sharing
  chapter); refrains (*run* for the ferry timetable, *pay attention*, *understand*); the Moby-Dick
  *Extracts* window, all nine flags of which come from quoted sources; sermon rhetoric (*seek*, *true*).
- Wrong: *form* in *The Age of Innocence* counts the noun *“form”* and *sea* in *going to sea* is
  counted as a non-noun; both are noun uses the chunk restriction missed.

Including nouns adds 148 flags, mostly setting and topic: *ferry*, *island*, *clock*, *bread*,
*whale*-chapter *ship*, *sea*, *bed*, *landlord*. About ten are useful (*way* in the *the way you…*
similes, *thing*, Gatsby's *murmur*, *sort of*), so nouns stay opt-in.

The filler list needs no threshold. In the same texts *that* (complementizer) runs at 15–66 per
10,000 words, near the reference 51; Hemingway's *very* stands out at 51 per 10,000 words, 5.7 times
the reference, and Christie's *suddenly* and *quite* at 3.5 and 3.2 times.
