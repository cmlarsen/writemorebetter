# Consistency

Issue #298 (part of #296) adds an opt-in **consistency** lens over an existing `StyleDocument`. It
counts and locates mixed usage inside one document: quotes, apostrophes, ellipses, dashes,
capitalization and hyphenation. It diagnoses only. The author picks the house style; the report
never says which form is right and never generates replacement text. It needs no data files.

| Surface | Call |
|---|---|
| Rust | `consistency::report(&Document, &ConsistencyOptions)` |
| Retained Rust | `document.consistency(options_json)` |
| WASM | `document.consistency(optionsJSON?)` |
| Swift | `document.consistency(optionsJson:) -> ConsistencyReport` |

The report works on compact documents (`includeTokenDetails: false`); there it loses only the
part-of-speech guard for capitalized homographs described below. Options use camelCase, accept
snake_case aliases and reject unknown keys:

| Option | Default | Meaning |
|---|---|---|
| `minimumMinority` | 1 | A check or word is `mixed` only when its second most common form occurs at least this often. Must be at least 1. Raise it to hide a single stray form. |
| `preferred` | none | A house form per punctuation check, as a [style preset](STYLE-PRESETS.md) sets it: `{"doubleQuotes", "singleQuotes", "apostrophes": "curly" \| "straight", "ellipses": "character" \| "periods" \| "spacedPeriods", "dashes": "emClosed" \| "emSpaced" \| "enSpaced" \| "doubleHyphen" \| "spacedHyphen"}`. An unknown check or form is an `invalidOptions` error. |

## Result

```text
ConsistencyReport {
  doubleQuotes, singleQuotes, apostrophes, ellipses, dashes: ConsistencyCheck
  dashCharacters: [ConsistencyCount]          // hyphen, enDash, emDash
  capitalization, hyphenation: [ConsistencyWord]  // mixed words only
}
ConsistencyCheck   { mixed, variants: [ConsistencyVariant], preferred, nonPreferred }
ConsistencyVariant { form, counts: Split<number>, spans: [Span] }
ConsistencyCount   { form, counts: Split<number> }
ConsistencyWord    { word, variants: [ConsistencyVariant] }
```

A check with a `preferred` form echoes it and counts every occurrence of its other forms in
`nonPreferred` (0 without a preference). The preference changes nothing else: `mixed` still compares
the forms with each other, and a document written entirely in the other form is not `mixed` but has
a nonzero `nonPreferred`.

Every punctuation check lists all of its forms in a fixed order, including forms with zero
count, so a UI can show "curly 412 · straight 3". `counts` splits each form into narration and
dialogue using the document's one speech definition. Dialogue and narration share one rule: a
writer who uses straight quotes only inside dialogue still has a mixed document. Each occurrence
has its own span (UTF-16 `{start, end}` in the bindings). Words are listed only when mixed, most
frequent first; their variants are sorted by count.

Text is scanned per paragraph, so headings that the document pass drops (chapter headings, `#`
lines, scene-break ornaments) are never counted.

## Checks

### Quotes and apostrophes

`doubleQuotes`, `singleQuotes` and `apostrophes` each have `curly` and `straight` forms.

- `“` and `”` are curly double quotes and `"` is straight. A `"` directly after a digit is an
  inch mark and is not counted (`5'10"`).
- A `'`, `‘` or `’` between two letters or digits is an apostrophe (`don’t`, `rock’n’roll`).
- Before a letter, a mark is an apostrophe when it starts a known elision: the document pass's list
  (`’em`, `’tis`, `’twas`, `’n’`, `’cause`, `’til`, decades such as `’90s`, …). Otherwise it opens
  a single quotation. An opening-direction `‘` before an elision still opens a quotation when a
  closing `’` (one not followed by a letter) comes later on its line (`‘Tis’ he said`); an
  autocorrected `‘Tis` with no closer stays an apostrophe. A possessive such as `dogs’` later on
  the same line looks like that closer, so `‘Tis … dogs’` is read as a quotation.
- A straight `'` after a digit is a foot mark and is not counted, even inside an open single
  quotation (`‘He is 5' tall’`).
- After a letter, a mark closes the open single quotation in the paragraph if there is one, and is
  otherwise a trailing apostrophe (`dogs’`, `goin’`).
- A mark between two non-letters (`,’`) is a single quote.

So `rock ’n’ roll`, `’em` and `’90s` count as apostrophes, not as unbalanced quotes.

### Ellipses

`ellipses` has three forms: `character` (`…`), `periods` (three or more periods in a row, so
`....` counts once) and `spacedPeriods` (`. . .`, separated by a space, no-break space or narrow
no-break space). Two periods are not an ellipsis, and abbreviations (`Mr. Smith`) never match.

### Dashes

`dashes` counts dashes used as sentence punctuation:

| Form | Matches |
|---|---|
| `emClosed` | `—` touching a letter or digit and no space (`paused—then`, `“I—”`) |
| `emSpaced` | `—` with a space and no touching letter (`stopped — then`) |
| `enSpaced` | `–` with a space and no touching letter (`London – the city`) |
| `doubleHyphen` | two or more hyphens in a row (`--`), spaced or not |
| `spacedHyphen` | a single hyphen with whitespace on both sides (`wait - what`) |

A run of em dashes (`——`) is one dash. A quote mark, other punctuation, a paragraph edge
(including its indentation) or a line separator (U+2028, newline) decides nothing, so a dash
beside one is classified by its other side. Emphasis marks (`_`, `*`) are looked through
(`—_Genesis_`). A dash is not classified when nothing decides on either side, when it has a space
on one side and a letter on the other (`word —word`), or when it follows a closing quote and a
space, the epigraph attribution form (`whale.” —_Hamlet_`). Closed en dashes are ranges or compounds (`10–12`,
`Paris–London`), not dashes, and are left out of `dashes`.

`dashCharacters` reports raw character counts for `hyphen` (`-` and U+2010), `enDash` and
`emDash`, including hyphens inside words. These have no `mixed` flag and no spans.

### Capitalization

A word counts when it appears both lowercase (`mining`) and capitalized (`Mining`) mid-sentence.
Words are runs of letters with internal hyphens and apostrophes, keyed by their lowercase form
with curly apostrophes folded. A word is not counted when it is:

- the first word of a sentence, or the first after an opening quote, a colon, a line separator, or
  a `.`, `!`, `?` or `…` inside the sentence (`“Stop!” Mara said`, `Mr. Bird`);
- one letter long, *I* or an *I* contraction, all capitals (`CAPTAIN`), or mixed case
  (`McDonald`, `iPhone`);
- a skipped name (`skipNames`);
- a function word (`the`, `and`, `with`, …), which is capitalized mid-sentence only in verse
  and titles;
- capitalized and next to another mid-sentence capitalized word, as in multiword names and titles
  (`Captain Ahab`, `White Whale`), which are capitalized by convention;
- capitalized directly after a comma or semicolon: reflowed verse lines (`storms, Like as`) and
  vocatives (`Yes, Captain`), both capitalized by convention;
- inside `_…_` or `*…*` emphasis, where titles of works are set;
- in a title-case sentence, where at least two and at least half of the mid-sentence words longer
  than three letters are capitalized (a contents line: `The Whiteness of the Whale.`);
- in a title-like paragraph: ten words or fewer, no closing punctuation, every word longer than
  three letters capitalized.

When token readings are available, a word is reported only when its capitalized and lowercase
occurrences share a noun or adjective reading, proper-noun readings aside. This drops name
homographs (`May`/`may`, `Will`/`will`) and capitalized verbs and adverbs, which mid-sentence are
verse or titles rather than a house style. Compact documents have no readings and keep such
words.

### Hyphenation

A word counts when the document writes it in two or more of: hyphenated (`e-mail`), open
(`half way`) and closed (`halfway`). Forms are keyed by their letters (`email`, `halfway`).

- Any hyphenated word seeds its open and closed forms. Its open form counts only before another
  word, where a hyphen would join it (`a well known painter`), or when the closed form also
  appears. When readings are available that next word must have a noun or adjective reading.
  Predicative open compounds (`her work was well known`) follow the common hyphenate-before-a-noun
  convention and are not a second form.
- Without a hyphenated form, an open pair counts against a closed word (`news paper`/`newspaper`)
  only when both parts have three or more letters and neither is a function word, so `every one`,
  `may be`, `any way` and `some time` stay separate words.

## Relationship to LanguageTool rules

LanguageTool's `EN_QUOTES` and `ELLIPSIS` rules prescribe one typographic form and flag the
other wherever it appears. This report prescribes nothing: it counts every form and says whether
the document mixes them, so a manuscript written entirely with straight quotes is consistent.
`EN_CONSISTENT_APOS` (#228, a builtin that is off by default and on in the `fiction` profile, which flags only the rarer style) is the closest rule: it flags every apostrophe token
once a text mixes the two kinds and suggests the other one. The `apostrophes` check gives the
same mix as counts and spans, keeps apostrophes apart from single quotes, and suggests nothing.
`EN_WORD_COHERENCY` flags a second spelling from a fixed list of variant pairs (LGPL data) within
one paragraph; this report uses no list and compares forms found anywhere in the document. No
LanguageTool data is used.

## Public measurements

`cargo test -p writemorebetter-style --test consistency -- --ignored --nocapture public_counts`
prints every count below. The stories use line mode and `moby150k` uses hard-wrapped mode,
matching the other style measurements. A flag is a mixed punctuation check or a mixed word.

| Text | Mixed checks | Capitalization words | Hyphenation words | Flags |
|---|---|---:|---:|---:|
| story1.txt | none | 0 | 0 | 0 |
| story2.txt | none | 0 | 0 | 0 |
| story3.txt | none | 0 | 0 | 0 |
| story4.txt | none | 0 | 0 | 0 |
| story5.txt | none | 0 | 0 | 0 |
| moby150k.txt | `singleQuotes` (curly 55, straight 1) | 18 | 5 | 24 |

The five stories were written with one typographic convention (curly quotes and apostrophes, `…`,
closed em dashes), so silence on them is the false-positive check. On them the report counts
326 curly double quotes, 100 curly apostrophes, 3 ellipsis characters and 3 closed em dashes, all
consistent.

On `moby150k`, single-judge and not blind:

- `singleQuotes`: one straight `'` closes a quotation in otherwise curly text (`What people?'`),
  a real mix in the source edition. Useful.
- Hyphenation, 5 of 5 real mixes: `sperma ceti`/`sperma-ceti`/`spermaceti`, `fire-board`/
  `fireboard`, `fore-castle`/`forecastle`, `old fashioned`/`old-fashioned`, `a whaling`/
  `a-whaling`.
- Capitalization, 13 of 18 real mixes, mostly Melville's personification and emphasis (`Time`,
  `Life`, `Heaven`, `Book`, `Pagan`, `Tomahawk`, `Whales`) and `Captain`/`captain` (9/4). The
  other five misread the text: an italic title broken across lines (`Whale`), a reflowed verse
  line (`Around`), the inn name `Sword-Fish Inn`, `Give` opening a clause after a dash, and an
  exclamation `Oh`.

Development removed the noise in stages: before the function-word, part-of-speech, emphasis,
title-case and comma guards, capitalization listed 75 Moby words, led by `the`/`The` and
`and`/`And` from reflowed verse in the extracts. Before the emphasis, attribution and indentation
rules, `dashes` was mixed on Moby with 64 "spaced" em dashes, all epigraph attributions
(`” —_Genesis_`). Real manuscripts were not measured; the report is an opt-in lens until they
are.
