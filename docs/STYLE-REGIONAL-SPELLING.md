# Regional spelling

Issue #299 adds an **opt-in lens** that finds mixed American and British spelling in one document
(*color*/*colour*, *realize*/*realise*), reports which spelling system dominates, and flags the
minority instances. It diagnoses only: a flag names the form the dominant system uses for the same
word, never replacement prose.

| Surface | Regional spelling |
|---|---|
| Rust | `regional_spelling::report(&Document, &RegionalSpellingOptions)` |
| Retained Rust | `document.regional_spelling(options_json)` |
| WASM | `document.regionalSpelling(optionsJSON?)` |
| Swift | `document.regionalSpelling(optionsJson:) -> RegionalSpellingReport` |

It is a separate report rather than part of the consistency report (#298, PR #309), which had not
merged when this was written. The `words` records follow #309's per-word variant shape
(`word`, `mixed`, forms with `counts: Split<usize>` and `spans`), so the two can be folded together
later without changing the matching. It needs no readings and runs on compact documents.

## Data

`style/data/spelling/regional.txt` is derived from VarCon 2020.12.07 by
`scripts/build-regional-spelling.py` (see [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#regional-spelling-299)
and NOTICE; it is a modified version under the Ispell notice). Each row is one variant set; each
spelling lists the systems that accept it:

- `a` **American**, `b` **British** with -ise, `z` **Oxford**: British with -ize, which VarCon
  tags `Z` (OED-preferred). A spelling accepts a system when VarCon tags it preferred, equal or a
  common variant (`v`) there. Seldom-used, possible and improper variants (`V`, `-`, `x`) do not.
- A spelling takes every system any of its usages gives it. *practice* the noun, *check* the verb,
  *tire* (exhausted), *meter* (device), *program* (software), *story* (tale) and *license* the verb
  are British too, so they are never markers; *practise*, *cheque*, *tyre*, *metre*, *programme*,
  *storey* and *licence* still mark British. Rare or archaic usages (`(-)`, such as *gray* the
  physical unit) are ignored, so *gray* still marks American.
- `~` marks a spelling whose usages differ (`check/abz~ cheque/bz`).
- Words valid in every system (*judgment*/*judgement*, *among*) are not markers. Rows where every
  spelling suits the same systems are dropped.

5,308 rows, 10,689 spellings: lowercase single words from clusters at SCOWL level 70 or below.
Canadian and Australian spellings are not modeled.

## Matching

Each word token, and each part of a hyphenated compound (*grey-green*), is lowercased and looked up.
Skipped names (`skipNames`) are ignored. **Proper nouns:** a capitalized word counts only first in
its sentence or after an opening quote or parenthesis, and not when the next word is also
capitalized; all-caps words count. So *Pearl Harbor*, *the Labour Party*, *Lincoln Center* and a
sentence-initial *Labour Party members* are skipped, while *Colour drained* and *“Colour me
surprised”* count.

A **marker** is a token whose spelling some system rejects. `systems` counts, for American,
British and Oxford, the markers each system accepts. `dominant` holds the systems with the highest
count once there are at least `minimumMarkers` markers (default 3); it holds every system that
ties. An Oxford document (*colour*, *centre*, *realize*) is Oxford, not mixed, because Oxford
accepts all three. A document with no -ise or -ize evidence ties British and Oxford.
With no dominant system (fewer markers than the minimum and no `system` option) nothing is
flagged; `systems` and `words` are still reported.

## Flags

- `reason: "system"`: a token no dominant system accepts. With British and Oxford tied, *color* is
  flagged, while *realise* and *realize* are not (each suits one of the tied systems).
  `dominantForm` is the row's spelling that every dominant system accepts, if any.
- `reason: "form"`: a word whose forms suit the dominant system but are mixed (*focussed* ×2,
  *focused* ×1 in a British document flags *focused*). The most frequent form leads; a tie flags
  nothing. A word whose leading form the dominant system rejects gets only system flags. Spellings
  marked `~` never make a word mixed: *check* beside *cheque* is not a variant pair.

Each flag has its `span`, global `sentence` index, `form`, the row's `word` (the American spelling
where there is one), the form's `systems`, `reason`, `dominantForm` and sentence `mode`. `words`
lists every row seen in the document with a marker form, the forms seen, their narration/dialogue
`counts` and `spans`.

## Options

`minimumMarkers` (default 3, at least 1) and `system` (`"american"`, `"british"` or `"oxford"`):
a caller that knows the manuscript's intended spelling sets `system`, and every token that system
rejects is flagged regardless of the counts. Unknown keys are errors; snake_case aliases are
accepted.

## Measurements

`cargo run --release -p writemorebetter-style --example regional_spelling -- bench/public/*.txt`:

| File | Words | Markers | American / British / Oxford | Dominant | Flags | Mixed words |
|---|---:|---:|---|---|---:|---:|
| moby150k.txt | 25,002 | 62 | 23 / 39 / 45 | Oxford | 17 | 0 |
| story1.txt | 2,209 | 12 | 12 / 0 / 1 | American | 0 | 0 |
| story2.txt | 2,038 | 3 | 3 / 0 / 1 | American | 0 | 0 |
| story3.txt | 2,008 | 3 | 3 / 0 / 1 | American | 0 | 0 |
| story4.txt | 2,175 | 5 | 5 / 0 / 2 | American | 0 | 0 |
| story5.txt | 1,856 | 0 | 0 / 0 / 0 | none | 0 | 0 |

The five modern stories are consistently American: no flags. The Moby-Dick excerpt (the Project
Gutenberg text) really mixes systems: mostly British *-our* with Oxford *-ize*, plus 16 American tokens:
*honor*, *honorable*, *dishonor*, *endeavor*, *neighbors*, *parlor*, *rumor*, *behavior*, *checkered*, *peddler* and *naught*
and one British-only *monopolising*. All 17 flags are system flags, and all are real
inconsistencies in that text.

Size: the table adds 153,303 bytes to the style crate (32,377 gzipped), compiled in with
`include_str!`. The release WASM module (`cargo build --release --target wasm32-unknown-unknown -p writemorebetter-wasm`, before wasm-bindgen) grows from 5,811,582 to 6,028,521 bytes (+216,939, +3.7%); gzipped (`gzip -9`) from 1,551,591 to 1,597,939 bytes (+46,348, +3.0%).

## Overused-words British fold

`overused_words` folds 248 British lemmas into their American spelling. 244 of the pairs match a
VarCon row; the other four (*ageing*, *analogue*, *chimaera*, *doughnut*) are pairs VarCon accepts in
both systems, so they are not in this table. The fold is kept as is: it works on lemmas, folds
valid-in-both variants on purpose, and the bundled overused-words reference was built with it, so
swapping in the VarCon table would change that report's counts without a re-run of its reference.
