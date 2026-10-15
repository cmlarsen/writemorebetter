# Clichés and redundancies

`writemorebetter_style::reports::cliches` flags clichés ("dead as a doornail") and redundancies ("ATM
machine") from vendored phrase lists, with spans, and counts narration and dialogue separately (#246).
It diagnoses only and never suggests replacement wording.

```rust
use writemorebetter_style::reports::cliches::{cliches, PhraseList};
let phrases = PhraseList::bundled(&tokenizer);
let report = cliches(&document, &phrases);
```

The report is the public module `reports::cliches`; nothing is re-exported at the crate root or in
`reports`. Build the `PhraseList` once per tokenizer and cache it (compiling the bundled list analyzes
each of its 882 entries; bindings should do this once, which matters most on WASM and iOS).
`PhraseList::new(&tokenizer, entries)` takes a caller-supplied list instead: each `PhraseEntry` has a
kind, phrase, source, optional acronym, a fixed-wording flag and `not_followed_by` words. This is the
extension point for a later, owner-approved redundancy list. `PhraseList::len()` counts compiled
patterns; an entry with fewer than two words after an optional leading article is dropped, so a
caller-supplied list can compile to fewer patterns than `entries().len()`.

## Output

- `matches`: one `PhraseMatch` per occurrence with `kind` (`cliche` / `redundancy`), `context`
  (`narration` / `dialogue`), the list `phrase`, its `source`, the paragraph index and an absolute
  byte/char/UTF-16 `span`.
- `narration` and `dialogue`: `PhraseCounts` with `cliches`, `redundancies` and `words` (word tokens
  outside skipped names, the `Document::word_count` convention, so the two `words` sum to it).

A match is dialogue when its first byte lies inside one of the document's dialogue spans. Clichés
in dialogue are often deliberate characterization, so they are reported separately rather than
suppressed; the caller decides whether to show them.

## Matching

- **Lemmas.** A list word in base form matches any token sharing a lemma with it, so "kick the bucket"
  matches "kicked the bucket" and "cock and bull story" matches "stories". Token lemmas come from the
  document's lemma index, so compact documents (`with_token_details(false)`) give the same matches.
  A list word keeps only lemmas equal to its surface or a regular stem of it (at least three letters):
  "raining" keeps *rain*, but "better" drops *good*/*well*, "worst" drops *bad*, "shot" drops *shoot*,
  "ground" drops *grind*, "being" drops *be* and "done" drops *do*. So "a good half hour" is not
  *better half* and "shooting in the dark" is not *shot in the dark*. A list word that is itself
  inflected ("on the rocks", "in your dreams") matches only that form. On the text side, regular
  comparatives and superlatives (`-er`, `-est`) and the irregular ones (*better, best, worse, worst,
  more, most, less, least, further, farther, elder* and their superlatives) never match a different list
  word by lemma: "years younger" is not *years young* and "worse to the bone" is not *bad to the bone*.
- **Slots.** A possessive determiner in an entry is a slot. It matches any possessive determiner,
  optionally followed by *own*; a possessive noun phrase (`the captain’s`, `Mara’s`, `the Joneses’`); a
  plain determiner (`the`, `a`, `this`, `that`); or nothing, unless the slot is the entry's first word.
  So "air your dirty laundry" matches "aired her own dirty laundry", "aired the dirty laundry" and "aired
  dirty laundry". In entries naming a body part (*hair, foot, feet, tongue, wounds*, …) the slot rejects
  *its*, so "the cat landed on its feet" and "the dog licked its wounds" are not flagged. Personal
  pronouns match any personal pronoun or a skipped name; reflexives match any reflexive. An entry-initial
  `a`/`an`/`the` is optional, and the span includes a preceding article or possessive determiner
  ("his far cry", but not "that far cry"). A list word ending in *s* also matches the singular
  possessive: "at my wits end" matches "at his wit's end" and "at her wits’ end".
- **Guards.** `style/data/fixed-wording.txt` lists entries whose pronouns are part of the idiom ("in your
  dreams", "mark my words"); those match their own pronouns only. `style/data/literal-guards.txt` lists
  following words that mark a literal use: "let her hair down **from**", "put her foot down **on**",
  "kicked the bucket **of** water".
- **Spacing and punctuation.** Matches run across any whitespace inside one sentence, including hard
  line wraps in `hard_wrapped` mode, CRLF, NBSP, tabs and U+2028, but never across sentences or
  paragraphs: "It was all talk… No action came." is not *all talk, no action*. Hyphens are optional
  ("red handed" matches "red-handed", "bird’s-eye view" matches "bird's eye view"). Commas, dashes and
  semicolons are accepted only where the entry has punctuation, plus a serial comma before `and`/`or`.
  Curly and straight apostrophes are equivalent.
- **Names.** Skipped names never match a list word: with `skip_names: ["Jack"]`, "Jack of all trades"
  is not flagged but "a jack of all trades" is.
- **Acronyms.** Redundancy entries built from acronyms require the acronym in capitals, optionally
  plural (`UPC codes`, not `upc codes`).
- **Overlaps.** A match contained in a longer match is dropped ("a chip off the old block" is reported
  once).

Known precision limits: the lists mark no literal readings, so a literal action that fiction describes
often can still be flagged when no guard applies. Examples: "crossing the ford, she got her feet wet",
"the salt sat on the tip of her tongue", "he jumped and landed on his feet". Add guards to
`literal-guards.txt` (an owner-authored list, allowed under the data rules) as they are found.

## Data

| File | Entries | Source |
|---|---:|---|
| `style/data/cliches/no-cliches.txt` | 698 | no-cliches v0.3.6, MIT |
| `style/data/cliches/proselint.txt` | 1 | proselint `cliches/hell.py`, BSD-3-Clause |
| `style/data/redundancy/redundant-acronyms.tsv` | 82 acronyms → 186 phrases | retext-redundant-acronyms v5.0.0, MIT |
| `style/data/redundancy/proselint-ras.tsv` | 20 (4 not already above) | proselint `redundancy/ras_syndrome.py`, BSD-3-Clause |

Each acronym row yields one entry per suffix of its expansion after the first word, following
retext-redundant-acronyms: `ATM automated teller machine` gives "ATM teller machine" and "ATM machine".
After de-duplication there are 889 vendored entries; 882 ship. Commits, copyright lines and license
texts are in `NOTICE` and `style/data/licenses/`. See [STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md)
for the license check.

**Redundancy coverage is thin by design.** Only acronym redundancies come from permissively licensed
sources. proselint's larger redundancy lists (After the Deadline, Garner) and its other cliché lists
(GNU diction, Garner) were dropped because proselint names a GPL or all-rights-reserved origin for
them. No replacement list was authored for this PR; add one through `PhraseList::new` or a new data
file once its origin is approved.

## Overlap with the grammar checker

`style/data/suppressed.txt` drops the seven vendored entries that the English rules shipped in
`en_rules.bin` already flag on the same phrase. The check ran every vendored entry through
`bench/tools/wmb-cliches --covered` in three carrier sentences, with default rules and with every rule
enabled, and kept only hits from style and redundancy rules whose span is the phrase itself:

| Entry | Rule | Default |
|---|---|---|
| ATM machine | `REDUNDANCY/ATM_MACHINE` | on |
| CD disc | `REDUNDANCY/CD_DISC` | on |
| HIV virus | `REDUNDANCY/HIV_VIRUS` | on |
| LCD display | `REDUNDANCY/LCD_DISPLAY` | on |
| PIN number | `REDUNDANCY/PIN_NUMBER` | on |
| ISBN number | `REDUNDANCY/PIN_NUMBER` | on |
| for all intents and purposes | `PLAIN_ENGLISH/FOR_ALL_INTENTS_AND_PURPOSES` | off |

Other hits were kept: grammar-rule hits caused by the carrier sentence, off-by-default E-Prime rules
(which flag forms of *be*, not the phrase), `USELESS_THAT`, `THREE_NN`, the `EN_COMPOUNDS_*` hyphenation
builtins ("dog tired"), `TYPOS/CHOMPING_AT_THE_BIT`, and `REDUNDANCY/OUTSIDE_OF` inside "think outside
of the box". Those flag a different problem in the phrase, so both may show; the UI should expect
overlapping spans there. Rerun `--covered` and update `suppressed.txt` when the rules data changes.
Nothing under `style/data/` is copied from the rule files; the suppression list holds only vendored
phrases.

## Accuracy

`python3 bench/style/run.py cliches --gate` is the tier 2 suite. It injects 50 sentence templates at
the start of the 120 pinned Standard Ebooks passages. The 30 positive templates cover inflected forms,
slots (including an empty slot and "wit's end"), hard wraps, U+2028, curly quotes and apostrophes, em
dashes, ellipses, dialogue and acronyms. The 20 negative templates are near-misses and the PR #267
review's false positives ("a good half hour", "a bad nightmare", "shooting in the dark", "licked its
wounds", "all talk… No action", "years younger", …). It runs the detector on both the clean and the
injected text, and drops matches already present in the clean passage before scoring exact spans with
`kind.context` labels. Measured 2026-10-15: precision, recall and F1 1.0 (80/80/80) with 40 negative
controls and 1 natural hit in the clean passages; floors are in
`bench/style/tier2-cliches-baseline.json`. The templates were written against this matcher, so the
suite is a regression gate, not an accuracy claim on natural prose. It injects only at a passage's
start and runs in hard-wrapped mode.

As an unscored audit, the bundled lists over every chapter paragraph of the two pinned books (4,621
paragraphs) give 31 matches (23 narration, 8 dialogue), all idiomatic uses on one reviewer's reading,
for example "hold her tongue", "high and dry", "adding insult to injury". The three false-positive
kinds that audit first found ("years younger", "in their dreams", "on the rocking") are now fixture
guards; the review fixes left the audit unchanged (the same 31 matches).

## Cost

`bench/tools/wmb-cliches --benchmark TOKENIZER TEXT` on the pinned *Moby-Dick* chapters as one
document (209,407 words, compact document). Before the review fixes: building the 882-pattern
`PhraseList` took 441 ms, the report 248 ms (median of 5) and `analyze_document` 45.5 s. After them, on
the same machine at load average 64: 3.9 s, 349 ms and 34.2 s. The report stays at about 1% of
`analyze_document`; the compile time varies most with load. Besides its match list, the report builds a token-to-lemma map from the lemma
index and one token stream per paragraph.
