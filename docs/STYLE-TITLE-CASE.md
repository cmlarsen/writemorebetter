# Title case for headings

Issue #307 (part of #296) adds a capitalization check for **headings**, such as the chapter
titles Subplot passes alongside the manuscript. It takes heading strings, not prose paragraphs,
and checks each word against one of three styles:

| Style | Value | Capitalized | Lowercase |
|---|---|---|---|
| Chicago-style headline case (default) | `"chicago"` | first and last word, first word after a colon, every major word: nouns, pronouns, verbs (including *Is*, *Be*), adjectives, adverbs, subordinating conjunctions (*If*, *Because*), *Yet* and *So* | articles; *and*, *but*, *or*, *nor*, *for*; *to* and *as*; prepositions of any length (*with*, *from*, *between*, *through the*) |
| AP-style headline case | `"ap"` | first and last word, first word after a colon, every word of four or more letters, including prepositions and conjunctions (*With*, *From*, *Into*, *Than*) | articles; conjunctions and prepositions of three letters or fewer (*and*, *but*, *for*, *nor*, *or*, *so*, *yet*, *as*, *at*, *by*, *in*, *of*, *off*, *on*, *out*, *per*, *to*, *up*, *via*) |
| Sentence case | `"sentence"` | the first word | every other word, unless it is a proper noun |

The styles describe common editorial practice in our own words. Chicago and AP headline case are
modeled on how *The Chicago Manual of Style* and the *AP Stylebook* are usually applied; nothing is
copied from either book, and writemorebetter is not affiliated with or endorsed by their
publishers. The code is our own. We looked at
[chicago-capitalize](https://github.com/timdp/chicago-capitalize) (MIT, Tim De Pauw, 0.1.0), which
capitalizes a title in place; we did not port it, because this check diagnoses rather than
rewrites, needs AP and sentence case too, and has to say when a word could go either way.

| Surface | Call |
|---|---|
| Rust | `title_case::report(&headings, &TitleCaseOptions) -> Result<TitleCaseReport, StyleError>` |
| Rust, from a preset | `preset.title_case(&headings, skip_names)` (the preset's `titleCase` style) |
| WASM | `titleCase(headingsJSON, optionsJSON?)`, a free function; `headingsJSON` is a JSON array of strings |
| Swift | `TitleCase.check(_ headings: [String], optionsJson:) -> TitleCaseReport` |

No `StyleDocument` is involved: a heading is a separate string, not part of the analyzed prose, and
the check needs no tokenizer.

## Options

| Key | Default | Meaning |
|---|---|---|
| `style` | `"chicago"` | `"chicago"`, `"ap"` or `"sentence"` |
| `skipNames` | `[]` | Names (characters, places) kept as written, matched word by word ignoring case; a possessive (`Mara’s`) matches its name |

Unknown keys and an unknown style are `invalidOptions` errors, and so is an empty name.

## Result

`TitleCaseReport` has `style`, `headings` (how many were checked), `flaggedHeadings` (how many had
a match) and `matches`. Each match has `heading` (index in the input), `span` (the word within that
heading; UTF-16 `{start, end}` in the bindings, byte, char and UTF-16 ranges in Rust), `word` as
written, `replacement` (the same word with its first letter raised or the word lowered, never other
wording) and `rule`:

| Rule | Meaning |
|---|---|
| `firstWord` | The heading's first word is capitalized (in every style) |
| `lastWord` | The last word, of the heading or of a title before a colon, is capitalized (headline styles) |
| `afterColon` | The first word after a colon, question mark or exclamation mark is capitalized (headline styles; either case passes in sentence case) |
| `majorWord` | A major word is capitalized |
| `minorWord` | An article, coordinating conjunction, *to*, *as* or preposition is lowercase |
| `sentenceCase` | In sentence case, a word after the first is lowercase unless it is a proper noun |

## Details

- **Words.** Letters and digits, joined by apostrophes and hyphens between them (`Don’t`,
  `Mother-in-Law`, `Up‑to‑Date` with a non-breaking hyphen). Anything else separates words: spaces
  and U+2028, curly and straight quotes, em and en dashes, ellipses, slashes and emoji. Spans are
  offsets into the heading as passed, so curly quotes and emoji before a word shift its UTF-16
  start, not its text.
- **Left as written.** Words with a capital after their first letter (`FBI`, `McCall’s`, `iPhone`),
  words that start with a digit (`3rd`, `12`), single letters other than *a* (`Plan B`, *I*, the *n*
  of *rock ’n’ roll*), contractions of *I* (*I’m*, *I’ve*, *I’d*), and names in `skipNames`. A heading with no lowercase letters at all
  (`CHAPTER ONE`) is a typographic choice and is skipped whole.
- **Hyphenated compounds.** The first part is capitalized. Later parts follow the word rules, so an
  article, conjunction or preposition stays lowercase (`Up-to-Date`, `Mother-in-Law`) and other
  words are capitalized (`Self-Sustaining`, `Twenty-One`). After a prefix that doesn't stand alone
  (*anti*, *co*, *de*, *non*, *pre*, *re*, *un* and the like) either case passes (`Anti-intellectual`,
  `Pre-War`). The last part of the last word is the heading's last word (`The Long Run-In`).
- **Words that are prepositions only sometimes.** *About*, *after*, *before*, *down*, *off*, *out*,
  *over*, *through*, *under*, *up*, *since*, *until*, *like*, *than* and similar words are adverbs,
  particles or conjunctions as often as prepositions (`Looking Up`, `Before You Go`). Without a
  parser the check expects lowercase only when the next word opens a noun phrase (an article,
  possessive or demonstrative: `Journey through the Night`); elsewhere either case passes. The
  cost: a phrasal verb before its object (`Turn Off the Light`) is flagged, and a preposition
  before a bare noun (`Journey Through Time`) is not.
- **In, on, at, by.** These are treated as prepositions everywhere, so a rare adverb use
  mid-heading (`Hold On Tight`) is flagged.
- **Sentence case and proper nouns.** Every capitalized word after the first is flagged with
  `sentenceCase` unless it is in `skipNames`, since the check cannot otherwise tell a name from a
  title-cased word. Pass the same names the document uses.

## Presets

A [style preset](STYLE-PRESETS.md) chooses a style with its `titleCase` key:

| Preset | `titleCase` |
|---|---|
| `chicago-fiction` | `"chicago"` |
| `ap` | `"ap"`: AP-style capitalizes prepositions of four or more letters, which Chicago lowercases |
| `uk` | `"sentence"`: British publishers commonly set headings in sentence case. A user preset can switch it back with `"titleCase": "chicago"` |

A preset without the key (a user preset that sets it to `null`) uses Chicago style. In Rust,
`preset.title_case(&headings, skip_names)` applies it. In the bindings, read the key from the
resolved preset and pass it as an option:

```js
const style = JSON.parse(loadStylePreset(presetName)).titleCase;   // or parseStylePreset(json)
const report = JSON.parse(titleCase(JSON.stringify(chapterTitles), JSON.stringify({ style, skipNames })));
```

## Tests

`style/tests/title_case.rs` covers headings that must pass in Chicago style (25 of them, including
compounds, acronyms, numbers and an all-capitals heading), minor and major words, first and last
words, words after colons and question marks, hyphenated compounds (with Unicode hyphens), AP and
sentence case, `skipNames`, curly quotes and apostrophes, em dashes, ellipses, U+2028 and emoji in
spans, strict options, and the presets' choices. The WASM (`wasm/test.mjs`) and Swift
(`CheckerTests.testAlliterationAndTitleCase`) tests check UTF-16 spans through the bindings.
