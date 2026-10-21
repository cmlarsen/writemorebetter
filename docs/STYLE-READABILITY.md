# Readability: indices and reading time

`style::reports::readability` (#300) is an **opt-in lens** that reports document
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
| `readingMinutes` | words ÷ `wordsPerMinute`; for `groups.all`, every word in the text, excluded paragraphs included |

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
- A double hyphen is a typed em dash, so `It was--he thought--late.` is five words. The tokenizer
  keeps `was--he` as one token, so the shared token count in sentence length and pacing still
  counts it as one.
- Numbers stay whole across a comma, period or slash between digits (`1,000`, `3.5`, `9/11`), and
  initialisms across periods between single letters (`p.m.`, `U.S.`).
- Skipped names count: a reader reads them.
- Any token with a letter or digit is a word, so `1999` and `1,000` are one word of one syllable
  each. CMUdict has no numerals, so the dictionary doesn't spell them out either. Initialisms it
  lists are read letter by letter (`U.S.` 2, `a.m.` 2); others fall back to the heuristic
  (`p.m.` 1).

Sentences come from the document pass; one with no words (`…`, `* * *`) is not counted. U+2028
inside a paragraph is whitespace, so it neither splits words nor ends a sentence by itself.

## Options

| Option | Default | Meaning |
|---|---:|---|
| `minimumSentences` | 50 | groups below this many sentences are `lowConfidence`, the threshold the other reports use |
| `minimumParagraphWords` | 30 | paragraphs below this many words are `lowConfidence` |
| `wordsPerMinute` | 260 | must be finite and positive |
| `excludeDialogueParagraphs` | false | leave dialogue-only paragraphs out of every group's counts and indices; `groups.all.readingMinutes` still counts them |

Unknown keys are errors; camelCase and snake_case names are accepted. The 260 words per minute is
Brysbaert's (2019) meta-analytic estimate for silent reading of English fiction by adults; his
non-fiction estimate is 238. The 30-word paragraph minimum is a judgment: the formulas were fitted
on 100-word samples, and below about 30 words one sentence boundary moves the grade by several
points.

`excludeDialogueParagraphs` drops only paragraphs where every sentence is dialogue. Dialogue
sentences in a mixed paragraph stay, so with the option on `groups.dialogue` holds only those.
The option is for scoring the narration's difficulty; the reader still reads the dialogue, so the
whole-text reading time is computed from every word either way. `narration` and `dialogue` reading
times cover the words in their own group.

## Syllable counter

`report` counts syllables with `DictionarySyllables` (phase 2, [below](#dictionary-phase-2)):
CMUdict's count for the words the heuristic gets wrong, the heuristic for everything else.
`report_with(document, options, &HeuristicSyllables)` gives the phase 1 counts.

`syllables(word)` is the heuristic: count vowel groups, then apply English exceptions. All rules are
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
  `client`, `science`; not `patient`), `oe` before m or t (`poem`), final `-uel(s)`, `-uet` and
  `-uent` except after q, g, m or n (`cruel`, `fuels`, `duet`, `fluent`; not `Samuel`), `-eum`
  (`museum`), `ao` before s or t (`chaos`, `chaotic`), final `-dea(s)`/`-rea(s)` and `-eate`
  (`idea`, `areas`; not `whereas`), and the stems `cre-at-` (not before u), word-initial `re-act-`
  and `the-at-` with any suffix (`creates`, `creation`, `reacted`, `theatrical`; not `creature`).
  A vowel before `-ing` adds one (`going`, `being`).
- **`-ire`.** The final e of `-ire`, `-ires` and `-ired` after a lone `i` is the second syllable
  (`fire`, `tired`, `desires`), except `-shire` (`Berkshire`). CMUdict itself is split here
  (`fire` 2 but `admire`, `retire` 1 per `-ire`); the rule follows the more frequent words.
- **Endings.** `-sm` and `-thm` add one (`prism`, `rhythm`, `criticism`).
- **Compounds and suffixes.** A stem's silent e stays silent before -ly, -ment(s), -ful(ly),
  -less and -ness (`lovely`, `statement`, `hopeless`), and after the prefixes some, where, there,
  here, home, fire, life and time, which count one syllable each (`something`, `therefore`,
  `fireman`). `every` + a suffix is two syllables plus the rest (`everyone`, `everything`). The
  co- prefix before a vowel-initial root is its own syllable (`coexist`, `coincide`, `cooperate`,
  `coordinate`, `coed`).
- **Titles.** `Mr`, `Mrs` and `Dr` are read in full: two syllables each.
- **Contractions and possessives.** Curly apostrophes are normalized. `n't`, `'ll`, `'d`, `'ve` and
  `'re` add a syllable after a consonant sound (`didn’t`, `haven’t`, `it’ll`; not `don’t`,
  `you’ll`); `'s` adds one after a sibilant (`James’s`); `'m` never does. Other apostrophes are
  dropped (`o’clock`, `ma’am`). Hyphenated words are the sum of their parts.
- Words with no Latin vowel (`hmm`, `1999`) count as one.

### Accuracy

Counts are compared with the first CMUdict pronunciation of each word. This section measures the
heuristic alone; see [the dictionary's accuracy](#accuracy-with-the-dictionary) for the default
counter.

| List | Words | Correct | Accuracy |
|---|---:|---:|---:|
| Development (`style/tests/fixtures/syllables.tsv`) | 484 | 467 | 96.5% |
| Held out (`style/tests/fixtures/syllables-heldout.tsv`) | 190 | 189 | 99.5% |

The development list grew from 441 words to 484 after review of the first version: inflections
of `create` and `idea`, `-ire` and `-our` words, `cruel`/`fuel`/`chaos`, `museum`,
`cooperate`, unaccented `naive`/`cafe`, and controls such as `treated`, `great`, `real`, `four`
and `your`. The first version scored 431 of 441 on the original words and 439 of the 484. This
one still scores 431 of the original 441 and gets 36 of the 43 additions right, up from 8. Over every alphabetic
CMUdict headword (117,493, mostly names and rare words), correct counts rose from 109,096 to
109,208 (92.9% to 93.0%). Weighted by word frequency in `bench/public`, they rose from 99.01% to
99.19% of tokens.

The development list is common fiction vocabulary plus the hiatus, silent-e, contraction and
hyphen cases the rules were written for, so it overstates accuracy on hard words and was used to
tune them. Its remaining misses are words a rule can't reach without a dictionary: `maybe`,
`naked`, `wicked`, `rugged`, `crooked`, `business`, `evening`, `Wednesday`, `recipe`,
`apostrophe`, `cooperate` and `coordinate` (CMUdict gives 4: the `-ate` is counted with the
`e`), `naive` and `cafe` without accents (`naïve` and `café` are right), and `hour`, `hours`,
`flour`, `sour` and `our`. The `-our` words are 2 syllables in CMUdict's first entry but `four`,
`pour`, `your` and `tour` are 1, and spelling doesn't predict which, so they are left at 1.
CMUdict's second entry for `our` is also 1. The held-out list is a seeded random sample (seed 300) of 200 distinct words from
the five `bench/public` stories that weren't in the development list, with misspellings, names and
words with two common pronunciations removed. It was labeled after the rules were frozen; its one
miss is `priest` (counted as `-iest`, 2). Running prose is dominated by one-syllable words, so the
per-token accuracy on real text is higher than either figure.

## Dictionary (phase 2)

`DictionarySyllables` asks a table derived from the CMU Pronouncing Dictionary first and the
heuristic second, through `syllables_with(word, lookup)`. The lookup is tried on the whole word,
then on each hyphen-separated part, then on the stem before a contraction or possessive, so
`Wednesday’s` gets CMUdict's `wednesday` plus the heuristic's possessive rule. Keys are lowercase
with straight apostrophes and no outer punctuation; curly quotes, curly apostrophes, em dashes,
ellipses and U+2028 never reach a key.

**Source.** `cmudict.dict` from [cmusphinx/cmudict](https://github.com/cmusphinx/cmudict) at
commit `74790861f652b15e4ac49015a90074ad62a27690` (sha256
`81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22`, 3,618,488 bytes),
BSD-2-Clause-style; the license is in `style/data/licenses/cmudict.txt` and NOTICE. See
[STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#cmudict-300-307-alliteration).

**Table.** `scripts/build-cmudict.py cmudict.dict` refuses any other sha256, takes each headword's
first pronunciation, and counts its vowel phones (the phones with a stress digit; at least 1).
Keys are the headwords, with initialisms (`u.s.`) stored without their final period, since the
word joiner leaves the sentence's period off; other trailing-period abbreviations (`corp.`, `dr.`)
and headwords with a leading apostrophe (`'em`) are dropped, and `dogs'` defers to `dogs`. That
gives 125,159 keys. The script then runs `cargo run --example cmudict_syllables`, which keeps a
key only if `syllables_with` misses CMUdict's count without it, and adds keys back until every one
of the 125,159 gets CMUdict's count from the kept table. **8,437 keys are kept** (6.7%); the heuristic alone gets 8,830 keys wrong, and the other 393 come
right once their stem or parts are looked up (`wednesday's`, `able-bodied`). Only counts are
written, never phonemes. Rebuild the table whenever the heuristic changes, or words it used to get
right may drift; the `bundled_syllable_table_is_well_formed` test checks every kept key.

| Table | Rows | Bytes | `gzip -9` |
|---|---:|---:|---:|
| Every key, `word\tcount` | 125,159 | 1,312,434 | 394,477 |
| Heuristic misses only (bundled) | 8,437 | 92,749 | 32,885 |

Keeping only the misses is 14× smaller raw and 12× smaller gzipped than the full table, with the
same counts for every CMUdict headword. The trade is the coupling to the heuristic above.

**First pronunciation.** The table follows CMUdict's first entry, the same reference the accuracy
lists use, so `our`, `hour` and `fire` are 2 and `every` is 3.

**Reuse.** `writemorebetter_style::pronunciation` holds the lookups derived from CMUdict:
`syllable_exception(key)` for this table. Its `None` means "use the heuristic", not "not in
CMUdict". The alliteration lens (#307) needs initial sounds, not counts; it should add its own
derived table as a second output of `scripts/build-cmudict.py` from the same pinned file, with its
own lookup in that module, so phonemes still aren't exposed.

### Accuracy with the dictionary

| List | Words | Heuristic | Dictionary |
|---|---:|---:|---:|
| Development (`style/tests/fixtures/syllables.tsv`) | 484 | 467 (96.5%) | 482 (99.6%) |
| Held out (`style/tests/fixtures/syllables-heldout.tsv`) | 190 | 189 (99.5%) | 190 (100%) |
| Every CMUdict key (first pronunciation) | 125,159 | 116,329 (92.9%) | 125,159 (100%) |

The two development misses are `beautifully` and `convenient`, labeled 4 in the list; CMUdict's
first (and only) entry gives 3 for both (`B Y UW1 T AH0 F L IY0`, `K AH0 N V IY1 N Y AH0 N T`), and
both counters say 3. Words outside CMUdict (names, coinages, misspellings) keep the heuristic's
accuracy.

### Cost

- **Size.** The table adds 92,749 bytes to the crate source. The WASM module after `wasm-bindgen`
  grows from 5,909,360 to 6,009,357 bytes (+99,997, +1.7%); gzipped with `gzip -9`, from
  1,502,780 to 1,537,709 (+34,929, +2.3%). Measured against main at `10c81ac` on macOS with Rust's
  wasm32 target, without `wasm-opt`.
- **Load.** The table is parsed into a hash map on the first lookup: 0.8 ms native (release), held
  for the life of the process (estimated at a few hundred KB for 8,437 entries; not measured).
- **Check time.** A readability report on a 2,000-word story takes 1.0–1.2 ms with either counter
  (+0.04 ms with the dictionary); on `moby150k`, 13.8 ms → 14.3 ms (+3%). Native release, median
  of 21 runs:

  ```sh
  cargo run --release -p writemorebetter-style --example readability_report -- --timing \
    bench/public/story{1,2,3,4,5}.txt bench/public/moby150k.txt
  ```

## Public measurements (2026-10-20)

```sh
cargo run --release -p writemorebetter-style --example readability_report -- \
  bench/public/story{1,2,3,4,5}.txt bench/public/moby150k.txt
```

`moby150k.txt` is read hard-wrapped. Default options, with the dictionary counter; pass
`--heuristic` first for the phase 1 values.

| Text | Sentences | Words | Syllables | Syll./word | FRE | FKGL | CLI | ARI | Minutes |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| story1 | 119 | 2190 | 2818 | 1.29 | 79.3 | 6.8 | 6.1 | 6.6 | 8.4 |
| story1 narration | 82 | 1871 | 2429 | 1.30 | 73.8 | 8.6 | 6.7 | 9.0 | 7.2 |
| story1 dialogue | 37 | 319 | 389 | 1.22 | 94.9 | 2.2 | 2.9 | 0.6 | 1.2 |
| story2 | 131 | 2018 | 2466 | 1.22 | 87.8 | 4.8 | 5.0 | 4.4 | 7.8 |
| story2 narration | 91 | 1681 | 2054 | 1.22 | 84.7 | 6.0 | 5.4 | 6.0 | 6.5 |
| story2 dialogue | 40 | 337 | 412 | 1.22 | 94.9 | 2.1 | 3.0 | 0.6 | 1.3 |
| story3 | 127 | 1986 | 2456 | 1.24 | 86.3 | 5.1 | 5.2 | 4.7 | 7.6 |
| story3 narration | 83 | 1654 | 2049 | 1.24 | 81.8 | 6.8 | 5.6 | 6.9 | 6.4 |
| story3 dialogue | 44 | 332 | 407 | 1.23 | 95.5 | 1.8 | 2.9 | 0.5 | 1.3 |
| story4 | 128 | 2156 | 2700 | 1.25 | 83.8 | 5.8 | 5.4 | 5.4 | 8.3 |
| story4 narration | 100 | 1888 | 2383 | 1.26 | 80.9 | 6.7 | 5.6 | 6.4 | 7.3 |
| story4 dialogue | 28 | 268 | 317 | 1.18 | 97.1 | 2.1 | 3.4 | 1.2 | 1.0 |
| story5 | 117 | 1837 | 2252 | 1.23 | 87.2 | 5.0 | 4.8 | 4.5 | 7.1 |
| story5 narration | 85 | 1581 | 1935 | 1.22 | 84.4 | 6.1 | 5.0 | 5.8 | 6.1 |
| story5 dialogue | 32 | 256 | 317 | 1.24 | 94.0 | 2.1 | 4.0 | 1.4 | 1.0 |
| moby150k | 1125 | 24753 | 34578 | 1.40 | 66.3 | 9.5 | 8.6 | 10.2 | 95.2 |
| moby150k narration | 760 | 17560 | 24864 | 1.42 | 63.6 | 10.1 | 8.9 | 10.9 | 67.5 |
| moby150k dialogue | 365 | 7193 | 9714 | 1.35 | 72.6 | 8.0 | 7.8 | 8.6 | 27.7 |

With `excludeDialogueParagraphs`, `all` has the same counts and indices as `narration` for stories 1–4 (their dialogue is all
in dialogue-only paragraphs). Story 5 becomes 86 sentences, 1586 words, FKGL 6.0; moby150k
becomes 907 sentences, 20640 words, FKGL 9.9. Reading time stays at the whole text's.

**Phase 1 → phase 2.** The dictionary changes the count of 9–16 words per story and 226 in
`moby150k`, in both directions, so totals move little:

| Text | Syllables (heuristic → dictionary) | FRE | FKGL |
|---|---|---|---|
| story1 | 2818 → 2818 | 79.3 → 79.3 | 6.8 → 6.8 |
| story2 | 2454 → 2466 | 88.3 → 87.8 | 4.8 → 4.8 |
| story3 | 2452 → 2456 | 86.5 → 86.3 | 5.1 → 5.1 |
| story4 | 2694 → 2700 | 84.0 → 83.8 | 5.7 → 5.8 |
| story5 | 2257 → 2252 | 87.0 → 87.2 | 5.0 → 5.0 |
| moby150k | 34578 → 34578 | 66.3 → 66.3 | 9.5 → 9.5 |

No index moves by more than 0.5 in any group (story2 FRE 88.3 → 87.8, story2 dialogue
95.4 → 94.9). Since the heuristic was already right on about 99% of tokens in running fiction,
the dictionary matters for single paragraphs and unusual vocabulary rather than whole-text scores.
Coleman–Liau and ARI don't use syllables and are unchanged.

Per-paragraph Flesch–Kincaid grade, over paragraphs with at least 30 words:

| Text | Paragraphs | Scored | FKGL min | median | max |
|---|---:|---:|---:|---:|---:|
| story1 | 96 | 22 | 3.0 | 12.2 | 18.3 |
| story2 | 96 | 9 | 4.8 | 11.6 | 13.9 |
| story3 | 96 | 4 | 8.3 | 10.9 | 12.5 |
| story4 | 93 | 8 | 3.0 | 9.3 | 11.1 |
| story5 | 86 | 10 | 5.8 | 10.6 | 11.6 |
| moby150k | 315 | 208 | 2.4 | 10.4 | 47.6 |

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
