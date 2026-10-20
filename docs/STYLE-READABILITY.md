# Readability: indices and reading time

`style::reports::readability` (#300, phase 1) is an **opt-in lens** that reports document
statistics: Flesch Reading Ease, Flesch–Kincaid grade, Coleman–Liau, the Automated Readability
Index (ARI), estimated reading time, and the same values for every paragraph. It flags nothing and
never suggests replacement prose. Fiction aimed at adults routinely scores at grade 4–8 on these
formulas, and a low grade is not a fault. The values describe sentence and word length, nothing
more.

```rust
use writemorebetter_style::reports::readability::{report, ReadabilityOptions};
let readability = report(&document, &ReadabilityOptions::new())?;
// Retained StyleDocument: document.readability(None)?
```

```javascript
const readability = JSON.parse(document.readability());
const narrationOnly = JSON.parse(document.readability('{"excludeDialogueParagraphs":true}'));
```

```swift
let readability: ReadabilityReport = try document.readability()
let custom = try document.readability(optionsJson: "{\"wordsPerMinute\":238}")
```

It works on compact documents (`includeTokenDetails: false`), since it needs no readings.

## How this differs from #207

#207 tracks a port of LanguageTool's `READABILITY_RULE_SIMPLE` and `READABILITY_RULE_DIFFICULT`
into `nlprule`. Those are off-by-default grammar-checker rules: each computes Flesch Reading Ease
per paragraph and returns a **match** on a paragraph whose score level is above or below a
threshold, gated on a whole-text score. This report returns **statistics** instead of matches:
four indices plus reading time for the whole text, for narration and dialogue separately, and for
each paragraph, with no threshold and no flag. The two don't duplicate each other. A caller who
wants LanguageTool's flagged paragraphs enables those rules once #207 lands; a caller who wants
numbers for a dashboard uses this report. The syllable counters are different too: LanguageTool
counts vowel groups with a few adjustments; this one adds the English exceptions below.

## Result

The report is `{groups, paragraphs}`.

- `groups: Split<ReadabilityScores>` holds `all`, `narration` and `dialogue`. Each sentence goes
  wholly to the mode of its first word (`Document::sentence_mode`), the convention the sentence
  reports use, so `“Go,” she said.` is a dialogue sentence.
- `paragraphs` has one `ReadabilityParagraph` per document paragraph, in document order:
  `span`, `dialogueOnly` (every counted sentence is dialogue), `excluded` (left out of `groups` by
  `excludeDialogueParagraphs`) and `scores` for the whole paragraph. Excluded paragraphs keep their
  row and scores. A paragraph with no words (a scene-break marker) has zero counts and null indices.

`ReadabilityScores`:

| Field | Definition |
|---|---|
| `sentences` | sentences with at least one word |
| `words` | words as a reader counts them (below) |
| `syllables` | sum of per-word syllables, at least 1 per word |
| `characters` | letters and digits in counted words; apostrophes and hyphens excluded |
| `polysyllables` | words of three or more syllables |
| `lowConfidence` | groups: `sentences < minimumSentences`; paragraphs: `words < minimumParagraphWords` |
| `fleschReadingEase` | 206.835 − 1.015 × (words ÷ sentences) − 84.6 × (syllables ÷ words) |
| `fleschKincaidGrade` | 0.39 × (words ÷ sentences) + 11.8 × (syllables ÷ words) − 15.59 |
| `colemanLiau` | 0.0588 × L − 0.296 × S − 15.8, with L = characters per 100 words and S = sentences per 100 words |
| `automatedReadability` | 4.71 × (characters ÷ words) + 0.5 × (words ÷ sentences) − 21.43 |
| `readingMinutes` | words ÷ `wordsPerMinute` |

The four indices are null when the sample has no words. They are never clamped: a run of very
short words can give a Flesch score above 100 or a negative grade, as the published formulas do.
Below the minimums the values are still returned and `lowConfidence` is set, because these are
means rather than percentages; a UI should grey them out rather than hide them.

The formulas were written from their published definitions: Flesch (1948), Kincaid, Fishburne,
Rogers and Chissom (1975, US Navy RBR 8-75), Coleman and Liau (1975) and Smith and Senter (1967,
AMRL-TR-66-220). No code or data was vendored from `textstat` or any other package, so there is no
NOTICE entry for this report.

### Words and sentences

The report counts words the way the formulas assume, which differs on purpose from the
`is_word && !is_skipped` token count in sentence length and pacing:

- Adjacent word tokens joined by nothing, a hyphen (`-`, U+2010, U+2011) or an apostrophe (`'`,
  `’`) form one word: `can’t`, `She’s`, `well-known` and `mother-in-law` are one word each. An
  em dash, en dash, ellipsis or space separates words (`can't—won’t` is two).
- Skipped names count: a reader reads them.
- Any token with a letter or digit is a word, so `1999` is one word of one syllable.

Sentences come from the document pass; one with no words (`…`, `* * *`) is not counted. U+2028
inside a paragraph is whitespace, so it neither splits words nor ends a sentence by itself.

## Options

| Option | Default | Meaning |
|---|---:|---|
| `minimumSentences` | 50 | groups below this many sentences are `lowConfidence`, the threshold the other reports use |
| `minimumParagraphWords` | 30 | paragraphs below this many words are `lowConfidence` |
| `wordsPerMinute` | 260 | must be finite and positive |
| `excludeDialogueParagraphs` | false | leave dialogue-only paragraphs out of every group |

Unknown keys are errors; camelCase and snake_case names are accepted. The 260 words per minute is
Brysbaert's (2019) meta-analytic estimate for silent reading of English fiction by adults; his
non-fiction estimate is 238. The 30-word paragraph minimum is a judgment: the formulas were fitted
on 100-word samples, and below about 30 words one sentence boundary moves the grade by several
points.

`excludeDialogueParagraphs` drops only paragraphs where every sentence is dialogue. Dialogue
sentences in a mixed paragraph stay, so with the option on `groups.dialogue` holds only those.

## Syllable counter

`syllables(word)` is a heuristic: count vowel groups, then apply English exceptions. All rules are
original to this crate.

- **Vowels.** a e i o u y and accented Latin vowels. `u` after `q` is a consonant (`queen`). `y` is
  a consonant at the start of a word and between vowels, including after a vowel + `w`
  (`player`, `beyond`, `lawyer`). A vowel `y` before another vowel ends its group (`dying`,
  `everyone`), except a final `-ye`, `-yes`, `-yed` (`bye`, `dyed`). A diaeresis starts a new group
  (`naïve`); `é` is never silent (`café`).
- **Silent final e.** A final `-e`, `-es` or `-ed` that is a vowel group of its own after a
  consonant is silent (`make`, `makes`, `walked`), except: `-es` after s, x, z, c, g, ch or sh
  (`horses`, `judges`, `watches`); `-ed` after t or d (`wanted`); and consonant + l/r + e(s/d)
  where the two consonants differ (`little`, `tables`, `settled`, `hundred`; but `killed`).
- **`-gue`/`-que`.** `tongue`, `league`, `unique` lose the `ue` (but `argue` keeps it).
- **Hiatus.** Some vowel pairs are two syllables: `ia` (`giant`, `trivial`; not -cial/-tial/-sian
  or -iage), `io`/`iou` (`lion`, `curious`; not after c, t, s, x, g, n, h or ll: `nation`,
  `onion`, `million`), `eo` (`video`; not after g, nor `people`, `leopard`), `ua`/`uo` (`actual`;
  not after q or g, nor `persuade`), `ui` before d or n (`fluid`, `ruin`), `iu` (`medium`), `ie` in
  `-ier`/`-iest` from y, `-iet`, and `-ient`/`-ience` except after c, t or s (`happier`, `quiet`,
  `client`, `science`; not `patient`), `oe` before m or t (`poem`), and final `-dea`/`-rea` and
  `-eate` (`idea`, `area`, `create`). A vowel before `-ing` adds one (`going`, `being`).
- **Endings.** `-sm` and `-thm` add one (`prism`, `rhythm`, `criticism`).
- **Compounds and suffixes.** A stem's silent e stays silent before -ly, -ment(s), -ful(ly),
  -less and -ness (`lovely`, `statement`, `hopeless`), and after the prefixes some, where, there,
  here, home, fire, life and time (`something`, `therefore`, `timeless`). `every` + a suffix is
  two syllables plus the rest (`everyone`, `everything`).
- **Contractions and possessives.** Curly apostrophes are normalized. `n't`, `'ll`, `'d`, `'ve` and
  `'re` add a syllable after a consonant sound (`didn’t`, `haven’t`, `it’ll`; not `don’t`,
  `you’ll`); `'s` adds one after a sibilant (`James’s`); `'m` never does. Other apostrophes are
  dropped (`o’clock`, `ma’am`). Hyphenated words are the sum of their parts.
- Words with no Latin vowel (`hmm`, `1999`) count as one.

### Accuracy

Counts are compared with the first CMUdict pronunciation of each word.

| List | Words | Correct | Accuracy |
|---|---:|---:|---:|
| Development (`style/tests/fixtures/syllables.tsv`) | 441 | 431 | 97.7% |
| Held out (`style/tests/fixtures/syllables-heldout.tsv`) | 190 | 189 | 99.5% |

The development list is common fiction vocabulary plus the hiatus, silent-e, contraction and
hyphen cases the rules were written for, so it overstates accuracy on hard words and was used to
tune them. Its remaining misses are words a rule can't reach without a dictionary: `maybe`,
`naked`, `wicked`, `rugged`, `crooked`, `business`, `evening`, `Wednesday`, `recipe`,
`apostrophe`. The held-out list is a seeded random sample (seed 300) of 200 distinct words from
the five `bench/public` stories that weren't in the development list, with misspellings, names and
words with two common pronunciations removed. It was labeled after the rules were frozen; its one
miss is `priest` (counted as `-iest`, 2). Running prose is dominated by one-syllable words, so the
per-token accuracy on real text is higher than either figure.

### Seam for a dictionary (phase 2)

`report_with(document, options, &dyn SyllableCounter)` takes any counter. `report` uses
`HeuristicSyllables`. Phase 2 can add a counter backed by vendored CMUdict counts that falls back
to `syllables()` for unknown words, without changing the formulas or the result types. Check the
size impact before bundling it.

## Public measurements (2026-10-20)

```sh
cargo run --release -p writemorebetter-style --example readability_report -- \
  bench/public/story{1,2,3,4,5}.txt bench/public/moby150k.txt
```

`moby150k.txt` is read hard-wrapped. Default options.

| Text | Sentences | Words | Syll./word | FRE | FKGL | CLI | ARI | Minutes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| story1 | 119 | 2190 | 1.29 | 79.4 | 6.8 | 6.1 | 6.6 | 8.4 |
| story1 narration | 82 | 1871 | 1.30 | 73.9 | 8.6 | 6.7 | 9.0 | 7.2 |
| story1 dialogue | 37 | 319 | 1.22 | 94.9 | 2.2 | 2.9 | 0.6 | 1.2 |
| story2 | 131 | 2018 | 1.21 | 88.4 | 4.7 | 5.0 | 4.4 | 7.8 |
| story2 narration | 91 | 1681 | 1.21 | 85.4 | 5.9 | 5.4 | 6.0 | 6.5 |
| story2 dialogue | 40 | 337 | 1.22 | 95.4 | 2.1 | 3.0 | 0.6 | 1.3 |
| story3 | 127 | 1986 | 1.23 | 86.6 | 5.1 | 5.2 | 4.7 | 7.6 |
| story3 narration | 83 | 1654 | 1.23 | 82.2 | 6.7 | 5.6 | 6.9 | 6.4 |
| story3 dialogue | 44 | 332 | 1.23 | 95.5 | 1.8 | 2.9 | 0.5 | 1.3 |
| story4 | 128 | 2156 | 1.24 | 84.4 | 5.7 | 5.4 | 5.4 | 8.3 |
| story4 narration | 100 | 1888 | 1.25 | 81.6 | 6.6 | 5.6 | 6.4 | 7.3 |
| story4 dialogue | 28 | 268 | 1.18 | 97.1 | 2.1 | 3.4 | 1.2 | 1.0 |
| story5 | 117 | 1837 | 1.22 | 87.3 | 5.0 | 4.8 | 4.5 | 7.1 |
| story5 narration | 85 | 1581 | 1.22 | 84.6 | 6.1 | 5.0 | 5.8 | 6.1 |
| story5 dialogue | 32 | 256 | 1.24 | 93.6 | 2.2 | 4.0 | 1.4 | 1.0 |
| moby150k | 1125 | 24760 | 1.39 | 66.5 | 9.5 | 8.6 | 10.2 | 95.2 |
| moby150k narration | 760 | 17564 | 1.41 | 63.8 | 10.1 | 8.9 | 10.9 | 67.6 |
| moby150k dialogue | 365 | 7196 | 1.35 | 72.7 | 8.0 | 7.8 | 8.5 | 27.7 |

With `excludeDialogueParagraphs`, `all` equals `narration` for stories 1–4 (their dialogue is all
in dialogue-only paragraphs). Story 5 becomes 86 sentences, 1586 words, FKGL 6.0; moby150k
becomes 907 sentences, 20645 words, FKGL 9.9.

Per-paragraph Flesch–Kincaid grade, over paragraphs with at least 30 words:

| Text | Paragraphs | Scored | FKGL min | median | max |
|---|---:|---:|---:|---:|---:|
| story1 | 96 | 22 | 3.0 | 12.2 | 18.3 |
| story2 | 96 | 9 | 4.8 | 11.6 | 13.9 |
| story3 | 96 | 4 | 8.3 | 10.9 | 12.1 |
| story4 | 93 | 8 | 3.0 | 9.0 | 11.1 |
| story5 | 86 | 10 | 5.8 | 10.6 | 11.6 |
| moby150k | 315 | 208 | 1.8 | 10.3 | 47.6 |

What the numbers show:

- **Dialogue reads much easier than narration** in every text: 4–6 grades lower in the stories
  and 2 in Moby-Dick. A whole-text score mixes the two in proportion to how much dialogue
  there is, which is why the split matters for fiction.
- **The long paragraphs are the dense ones.** The 30-word minimum selects long narration
  paragraphs, so the per-paragraph median sits well above the whole-text grade. Most paragraphs in
  the stories are short dialogue lines and are `lowConfidence`.
- **One sentence can dominate a paragraph.** Moby-Dick's 47.6 is a paragraph that is a single
  sentence of more than a hundred words. The grade there says "very long sentence", which the sentence-length
  report already shows more directly.
- **The indices broadly agree on order, not on value.** Coleman–Liau and ARI use characters
  rather than syllables, so they don't depend on the syllable heuristic; on the same sample they
  land within about two grades of Flesch–Kincaid.
