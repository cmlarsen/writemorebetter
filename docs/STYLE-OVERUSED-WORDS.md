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
4. A hyphenated token the tagger doesn't know (no readings) counts as its head, the part after the
   last hyphen, with the tagger's readings for that word: *half-smiled* → *smile*, *yellow-grey* →
   *grey*, *self-containing* → *contain*. Ngram v3 splits hyphenated words, so the reference counts
   the head the same way. Compounds the tagger lists keep their own lemma (*old-fashioned*,
   *cross-legged*, *blue-eyed*, *well-known*). These are lexicalized, and their head is often another
   part of speech (*fashion*, *leg*), so the bundled table, which has no hyphenated keys, never flags
   them. A custom table can still list them.
5. British spellings count under their American lemma: *grey* and *gray*, *realise* and *realize*,
   *colour* and *color* share one count and one reference rate. `SPELLING_VARIANTS` in
   `overused_words.rs` holds the 248 pairs (*-ise/-ize*, *-yse/-yze*, *-isation*, *-our/-or*,
   *-re/-er*, *-ence/-ense*, *-ogue/-og*, *ae/oe*, single and double *l*, and a short hand list such
   as *grey, plough, mould, moustache, pyjamas, sceptical, judgement*). They were generated from the
   lemmatized Ngram forms by those suffix rules and pruned by hand of false pairs (*hour/hor*,
   *prise/prize*, *canoe/cane*, *timbre/timber*). The flag reports the spelling the document uses
   most.
6. Nouns are left out unless `includeNouns` is true: a token whose chosen lemma has only noun readings
   after step 3 is a noun use.

A lemma is flagged when its count is at least `minimumCount`, its reference rate is at least
`minimumReferencePerMillion`, and

```
ratio = (count / words × 1,000,000) / reference per million ≥ minimumRatio
```

`words` is the document's word count, the same denominator every report uses. Above
`countScaleWords` words (10,000 by default) the count threshold grows with the square root of the
length, `minimumCount × √(words / countScaleWords)` rounded to the nearest count, so it moves
smoothly past the boundary: still 5 at 12,000 words, 6 from 12,100, 8 at 25,000, 15 at 90,000. On
the 25,002-word `moby150k` excerpt that gives 7 flags instead of 25, and 13 on the five stories
joined (10,286 words), the same as unscaled. The defaults are calibrated on chapters; run the report per chapter
for chapter-sized results, or set `countScaleWords` to 0 to turn the scaling off. Lemmas missing from
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
| `countScaleWords` | 10,000 | length above which `minimumCount` scales with √length; 0 turns it off |

The ratio uses the POS-collapsed reference (noun and verb *smile* share one rate), so excluding noun
uses from the document count makes the default conservative for noun/verb homographs.

Known gap: a noun used as a modifier that the tagger reads as an adjective still counts as a non-noun
(*express* in *express car*, *express office*, flagged at 26× in *The Call of the Wild*). Ngram's
own tags can't separate these. It tags *express* ADJ 304k and NOUN 238k times, but *tender* NOUN
2.4M and ADJ 183k. A rule that dropped prenominal adjectives whose noun uses rival their adjective
uses would remove *express*, but also *tender*, *tense*, *evil* and *secret* in *a tender smile*,
which are the adjectives this lens exists for. On the 18 texts below it removed only *express*, so it
was not adopted.

The document picks one lemma per token with the chunk restriction above; `echoes` groups words into
families (`echo_families`) and keeps every family. The same token can therefore get a different
lemma in the two reports. #248 asked both to read one lemma index. They don't, because the shared
`lemma_index` lists every reading's lemma and would count an ambiguous token twice here.

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
| *that* | the tokenizer chunks it `B-SBAR` and a word follows it, opening a clause (*knew that he*, *so dark that*) | often omissible; determiners (*that house*) and pronoun *that* before punctuation (*He said that.*) are not counted |

The tagger never gives complementizer *that* an `IN` reading, so the chunk decides. The chunker
also opens some relative clauses with `B-SBAR` (*the advantages that you've had*): in 18 sampled
Gatsby and story spans, 14 were complementizers, 4 relatives and none determiners. The reference rate is
Ngram's `that_ADP` and `That_ADP` (complementizers only), so the *that* ratio reads slightly high.

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
  document's word predicate. Ngram v3 splits contractions as the tokenizer does (*do* + *n't*,
  *she* + *'s*; the one-token *don't* occurs about 5 times per million), so both sides count a
  contraction as two words.
- Each `_NOUN`, `_VERB`, `_ADJ` and `_ADV` form with at least 1,000 occurrences is lemmatized by
  `en_tokenizer.bin`'s tagger (`bench/tools/wmb-document` `lemmatize_forms`), keeping readings whose
  Penn tag maps to the form's universal tag. Capitalized forms are dropped when they have a
  proper-noun reading or outnumber their lowercase form (*Daisy*, *Rose*), so names do not inflate
  common nouns. Capitalized nouns are also dropped above a tenth of their lowercase form
  (sentence-initial nouns run at a median 4%), which keeps *Red Sea*, *Main Street* and *Mother* out
  of *sea*, *street* and *mother*. A form with several lemmas goes to the most frequent one, matching
  step 3 above.
- `_ADP`, `_CONJ` and `_PRT` uses are added to a lemma that already has content uses. A preposition
  keeps its adverb and adjective readings in a document (*near the door* reads `IN`, `JJ`, `RB`), so
  the document counts every *near*. Without its `_ADP` count the reference held only a fraction of
  those uses: *above* went from 20.2 to 202 per million, *near* from 72.2 to 254, *inside* from 188 to
  389, *past* from 199 to 315, *outside* from 152 to 254, *opposite* from 54.6 to 67.5 and *aboard*
  from 1.37 to 21.4. `_DET` and `_PRON` uses are not added, because the tagger leaves those tokens
  without a content reading (*half the time* reads `PDT` only), so the document never counts them.
- British spelling variants are counted under the American key (step 5), so *gray* is 124 per
  million instead of *grey* 46.1 and *gray* 78.1, and *realize* is 289 instead of 50.3 and 239.
- Function words, particles and fillers are excluded, matching the report. Filler rates come from
  the `_ADV` forms and `that_ADP`/`That_ADP`.
- The table keeps the 14,382 lemmas at or above 1 per million (a tenth of the default floor), rounded
  to three significant digits: 233,226 bytes of JSON, 78,344 gzipped. Keeping all 45,955 lemmas
  would take 843,773 bytes, and no lemma below the default floor can ever be flagged.
- `--retrieved` defaults to the committed download's date so the rebuild is byte-identical; pass the
  new date when rebuilding from a fresh download.

A caller can pass its own table, for example a genre-specific one, as `{"lemmas": {...},
"fillers": {...}}` with rates per million. Lemma keys must already be folded as the document folds
them (lowercase, straight apostrophes) and use the American spelling of a listed variant pair (*gray*,
not *grey*); anything else is rejected with `invalidBaseline` instead of silently never matching.
Filler keys must be from the fixed list; rates must be finite and non-negative. `"none"` skips the
comparison and returns fillers only.

Bundled into WASM, the table and report grow the module from 4,808,929 to 5,212,762 bytes after
`wasm-bindgen` (+403,833, +8.4%; `gzip -9` 1,228,589 → 1,336,327, +107,738) and from 2,798,977 to
3,098,525 bytes after `wasm-opt -Oz` (+299,548, +10.7%), measured against main with sensory balance
and diction (#281), on Linux with Rust's wasm32 target and binaryen 116. The table's JSON is
233,226 of those bytes; the spelling-variant list and the hyphenated-head lookup add about 9 KB and
28 KB. The table is parsed and validated once, on the first call: 20–25 ms in Node. Later calls
skip validating the bundled table and take 1.4 ms on a 2,209-word story and 4.0 ms on the
6,067-word Gatsby chapter (median of 200; 1.3 and 3.6 ms before the review fixes). A caller-supplied
table is validated on every call.

## Calibration and usefulness

Thresholds were calibrated on chapter-sized public-domain texts: the five invented stories in
`bench/public/` (1,856–2,209 words), the `moby150k` excerpt split at paragraph breaks into five
windows of about 5,000 words, and the first chapters of four Standard Ebooks texts fetched from their
GitHub repositories (*The Great Gatsby*, *The Sun Also Rises*, *The Mysterious Affair at Styles*,
*The Age of Innocence*). `style/examples/overused_words.rs` prints every flag with options taken from
`WMB_OVERUSED_OPTIONS`.

| Setting (ratio, count, floor) | Nouns | Flags per text: median (min–max) | Total over 14 texts |
|---|---|---:|---:|
| 4, 5, 0 | included | 18.5 (6–29) | 248 |
| 4, 5, 0 | excluded | 5 (2–9) | 77 |
| 4, 5, 10 | included | 16.5 (6–28) | 228 |
| 3, 5, 10 | excluded | 7.5 (5–10) | 106 |
| 4, 4, 10 | excluded | 9 (3–13) | 124 |
| **4, 5, 10** | **excluded** | **5 (2–9)** | **76** |
| 4, 6, 10 | excluded | 3 (0–5) | 41 |
| 5, 5, 10 | excluded | 4 (2–7) | 61 |

Every one of the 76 default flags was judged as a fiction editor would: *useful* if a line edit
would plausibly raise the repetition, *noise* if it is topical, a deliberate refrain, rhetoric or
quoted material, *wrong* if the lemma or part of speech is wrong. The first table (75 flags) used
the table before the preposition, spelling-variant, hyphen and capitalized-noun fixes:

| Texts | Flags | Useful | Noise | Wrong | Before the fixes |
|---|---:|---:|---:|---:|---|
| Stories 1–5 | 26 | 16 | 10 | 0 | 25: 16 / 9 / 0 |
| Standard Ebooks chapters | 15 | 8 | 6 | 1 | 16: 8 / 7 / 1 |
| Moby-Dick windows | 35 | 8 | 26 | 1 | 34: 7 / 26 / 1 |
| **All** | **76** | **32 (42%)** | **42 (55%)** | **2 (3%)** | **75: 31 (41%) / 42 (56%) / 2 (3%)** |

The changes: Gatsby's *inside* (noise) drops from 4.4× to under 4 once its preposition uses are in
the reference; story 3 gains *stop* (noise, the motel's *nobody stops anymore* refrain); the sermon
window gains *contain* (useful: *containing* three times, *contains*, *self-containing*, the last
counted through its head). Moby-Dick's *near* stays at 5.0× (17.6× before), still noise from quoted
sources. A second set of 15 texts judged in review (the same stories and windows, Gatsby ch. 1,
*The Maltese Falcon* ch. 1–2, *The House of Mirth* 1.1, *The Call of the Wild* ch. 1) went from 88
flags (36 useful, 49 noise, 3 wrong) to 90 (38, 50, 2). *Falcon*'s *grey* is now compared with
*grey* plus *gray*: 22.9× in ch. 1 (43.9× before; seven uses with *blue-grey* and *yellow-grey*)
and a new, useful 10.1× in ch. 2 (*grey socks*, *the grey suit*, *a blank grey sidewall*,
*yellow-grey eyes*). *near* there moves from wrong to noise. *express* (the known gap above) is the
other wrong flag, with *sea* in *going to sea*.

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

Including nouns adds 152 flags, mostly setting and topic: *ferry*, *island*, *clock*, *bread*,
*whale*-chapter *ship*, *sea*, *bed*, *landlord*. About ten are useful (*way* in the *the way you…*
similes, *thing*, Gatsby's *murmur*, *sort of*), so nouns stay opt-in.

The filler list needs no threshold. In the same texts *that* (complementizer) runs at 15–66 per
10,000 words, near the reference 52; Hemingway's *very* stands out at 51 per 10,000 words, 5.7 times
the reference, and Christie's *suddenly* and *quite* at 3.5 and 3.2 times.

## Published-fiction flag rates (2026-10-16)

The owner-only `tier3 --flag-rates` run (published-fiction export at `cmlarsen/fiction-baseline`
b464cac, 210 units, 71 authors, 1,559 chapter windows with a median of 5,966 words; aggregate
numbers only) swept `minimumRatio` with the other defaults fixed. Flags are flagged lemmas; shares
are author-weighted.

| `minimumRatio` | Chapter windows flagged | Authors flagged | Per 1,000 words, chapter median (p5–p95) |
|---:|---:|---:|---|
| 3 | 99.9% | 100% | 2.10 (0.80–4.08) |
| **4 (default)** | **99.4%** | **100%** | **1.44 (0.42–2.88)** |
| 5 | 98.0% | 100% | 1.04 (0.28–2.41) |
| 6 | 94.4% | 98.6% | 0.81 (0.01–1.96) |
| 8 | 89.4% | 98.6% | 0.55 (0–1.58) |

Filler words run at a chapter median of 7.32 per 1,000 words (p5–p95 3.04–15.25); flagged lemmas'
occurrences at 10.11 (2.42–22.10).

**Decision: defaults unchanged.** At the default a typical 5,000–6,000-word published chapter
window gets seven to nine flags (median 1.44 per 1,000 words). The judged public set above runs at
a similar rate: 1.40 per 1,000 words over the Moby-Dick windows (35 flags in 25,002 words) and 2.53
over the five stories (26 in 10,286). Published prose therefore does not show the default as looser
than intended. The comparison is approximate, because the `minimumCount` floor of 5 makes per-word
rates depend on text length: a 6,000-word window clears five uses more easily than a 2,000-word
story. Every published chapter has some lemma four times its Ngram rate, so the flag count
cannot separate drafts from published prose; the lens stays opt-in and its usefulness still rests
on the editor review above. No judged sample of published flags exists to justify a different ratio.
