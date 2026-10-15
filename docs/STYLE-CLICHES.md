# Clichés and redundancies

`writemorebetter_style::reports::cliches` flags clichés ("dead as a doornail") and redundancies ("ATM
machine") from vendored phrase lists, with spans, and counts narration and dialogue separately (#246).
It diagnoses only and never suggests replacement wording.

```rust
use writemorebetter_style::reports::{cliches, PhraseList};
let phrases = PhraseList::bundled(&tokenizer);
let report = cliches(&document, &phrases);
```

Build the `PhraseList` once per tokenizer and reuse it. `PhraseList::new(&tokenizer, entries)` takes a
caller-supplied list instead (each `PhraseEntry` has a kind, phrase, source, optional acronym and a
fixed-wording flag); this is the extension point for a later, owner-approved redundancy list.

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
  A list word that is itself inflected ("on the rocks", "in your dreams") matches only that form, and
  comparatives and superlatives never match by lemma ("years young" does not match "years younger").
- **Slots.** Possessive determiners in an entry match any possessive determiner or a possessive noun
  phrase (`her`, `the captain’s`, `Mara’s`): "air your dirty laundry" matches "aired her dirty
  laundry". Personal pronouns match any personal pronoun or a skipped name; reflexives match any
  reflexive. An entry-initial `a`/`an`/`the` is optional, and the span includes whichever article or
  possessive fills it. `style/data/fixed-wording.txt` lists entries whose pronouns are part of the
  idiom ("in your dreams", "mark my words"); those match their own pronouns only.
- **Spacing and punctuation.** Matches run across any whitespace inside a paragraph, including hard
  line wraps in `hard_wrapped` mode and U+2028, but never across paragraphs or sentence punctuation.
  Hyphens are optional ("red handed" matches "red-handed", "bird’s-eye view" matches "bird's eye
  view"). Commas, dashes and semicolons are accepted only where the entry has punctuation, plus a
  serial comma before `and`/`or`. Curly and straight apostrophes are equivalent.
- **Names.** Skipped names never match a list word: with `skip_names: ["Jack"]`, "Jack of all trades"
  is not flagged but "a jack of all trades" is.
- **Acronyms.** Redundancy entries built from acronyms require the acronym in capitals, optionally
  plural (`UPC codes`, not `upc codes`).
- **Overlaps.** A match contained in a longer match is dropped ("a chip off the old block" is reported
  once).

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

`python3 bench/style/run.py cliches --gate` is the tier 2 suite. It injects 32 sentence templates
(inflected forms, slots, hard wraps, U+2028, curly quotes and apostrophes, em dashes, ellipses,
dialogue, acronyms, and 5 negative near-misses) at the start of the 120 pinned Standard Ebooks
passages. It runs the detector on both the clean and the injected text, and drops matches already
present in the clean passage before scoring exact spans with `kind.context` labels. Measured
2026-10-15: precision, recall and F1 1.0 (105/105/105), 1 natural hit in the clean passages; floors
are in `bench/style/tier2-cliches-baseline.json`. The templates were written against this matcher,
so the suite is a regression gate, not an accuracy claim on natural prose.

As an unscored audit, the bundled lists over every chapter paragraph of the two pinned books (4,621
paragraphs) give 31 matches (23 narration, 8 dialogue), all idiomatic uses on one reviewer's reading,
for example "hold her tongue", "high and dry", "adding insult to injury". The three false-positive
kinds that audit first found ("years younger", "in their dreams", "on the rocking") are now fixture
guards.

## Cost

`bench/tools/wmb-cliches --benchmark TOKENIZER TEXT` on the pinned *Moby-Dick* chapters as one
document (209,407 words, compact document): building the 882-pattern `PhraseList` takes 441 ms once
per tokenizer, and the report takes 248 ms (median of 5), against 45.5 s for `analyze_document` on the
same, loaded machine. Besides its match list, the report builds a token-to-lemma map from the lemma
index and one token stream per paragraph.
