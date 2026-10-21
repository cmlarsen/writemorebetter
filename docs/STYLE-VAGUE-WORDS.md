# Vague words

Issue #307 (part of #296) adds an **opt-in lens** that counts words that soften or blur a
sentence: fillers (*basically*, *probably*, *began to*), hedges (*sort of*, *a bit*, *seemed to*),
vague quantities (*a lot*, *a number of*), vague nouns (*thing*, *stuff*) and judging adjectives
(*nice*, *amazing*). It diagnoses only. It shows spans, terms and rates, never a replacement, and it
doesn't say a match is wrong: in dialogue these words are often characterization, and published
fiction uses them throughout (about 11 per 1,000 narration words below).

| Surface | Call |
|---|---|
| Rust | `vague_words::report(&Document, &VagueWordsOptions)` |
| Retained Rust | `document.vague_words(options_json)` |
| WASM | `document.vagueWords(optionsJSON?)` → `VagueWordsReport` (`wasm/style-results.d.ts`) |
| Swift | `document.vagueWords(optionsJson:) -> VagueWordsReport` |

The report needs token readings (`StyleError::NeedsTokenDetails` on compact documents). Options are
camelCase JSON. Unknown keys are `invalidOptions`.

| Option | Default | Meaning |
|---|---|---|
| `fillers` | `true` | Include the bundled filler list. `false` keeps only the project-authored hedges, quantities, nouns and adjectives. |

## Result

`{matches, terms, groups}`. Each match has:

- `span`: the word or phrase (`sort of`, `began`, `things`).
- `term`: the list entry. Verbs and nouns are matched by lemma, so *began* is `begin`, *seemed to*
  is `seem to` and *things* is `thing`.
- `category`: `filler`, `hedge`, `quantity`, `noun` or `adjective`.
- `sentence`: the global sentence index, and `mode`.

`terms` has one row per term found (`term`, `category`, `count`, `narration`, `dialogue`), most
frequent first. `groups: Split<VagueWordsGroup>` holds `matches`, `words` and `perThousandWords`
(null when there are no words). **Narration and dialogue are reported separately**: mode is the
document's speech mode (`Document::mode_at`), as in most reports, and `all` is their sum. Skipped
names are neither matched nor counted as words. Spans are UTF-16 `{start, end}` in the bindings.

## What counts

Each sentence is scanned left to right; the longest entry that matches at a position wins, and a
match isn't reused. Every entry has a guard, so a listed word counts only in its vague sense:

| Entries | Counted when | Not counted |
|---|---|---|
| *sort of*, *kind of* | before an adjective, adverb or verb, with no determiner, possessive or adjective before | *what kind of man*, *a strange sort of creak*, *kind of him* |
| *a bit*, *a little* | before an adjective, adverb or participle that is not itself attributive | *a little girl*, *a bit of bread*, *a little withered old man* |
| *in a way*, *in some ways*, *to a degree*, *to some extent* | before a comma or the end of a clause | *in a way we never had*, *to a degree that surprised her* |
| *or so* | at the end of a clause | *Or so he said* |
| *seem to* | before a verb | *seemed to him a stranger* |
| *a lot*, *a great deal* | before *of*, *more*, *less*, a comparative, an adverb, clause punctuation or the end of the sentence | *bought a lot near the river*, *struck a great deal with the dealer* |
| *more or less*, *or something*, *lots of*, *a number of* | always | |
| *various*, *numerous*, *countless*, the adjectives | an adjective reading | |
| *thing*, *stuff* | a common-noun reading (lemma) | *She stuffed the bag* |
| adjectives and nouns | not capitalized inside a sentence, unless after an opening quote or a dash (*He said, “Nice work.”*) | *They flew to Nice* |
| fillers *like*, *well*, *right*, *ok*, *okay*, *whatever*, *whenever*, *wherever*, *whoever* | standalone, between a clause edge and punctuation | *looked like her mother*, *did it well*, *all right* |
| fillers *so*, *too*, *most*, *much*, *real*, *rather* | an adverb before an adjective or adverb (not a gerund), or *So,* alone | *so on*, *so far*, *do so*, *and so landed*, *the most*, *like most*, *as much as*, *or rather*, *would rather*, *rather than*, *me too* |
| fillers *start*, *begin*, *try* | any form, before *to* or a gerund | *He tried the door* |
| filler *might* | a modal before a verb or adverb | *a giant in might* |
| filler *actual* | an adjective reading | |
| other fillers (*basically*, *probably*, *slightly*, …) | an adverb or interjection reading | |

## Data

Two files, both `include_str!`'d into the style crate and parsed once on first use:

- `style/data/vague/vague-words.tsv` (33 entries, 759 bytes): `category<TAB>phrase`, written for
  this project by its contributors, MIT OR Apache-2.0 like the engine. 13 hedges, 7 quantities,
  2 nouns, 11 adjectives.
- `style/data/vague/fillers.txt` (80 entries, 1,137 bytes): the words/fillers 2.0.1 entries
  (Titus Wormer, MIT) that mikpanko/expresso (Mikhail Panko, MIT) also lists, as the
  [wave 3 check](STYLE-DATA-LICENSES.md#wordsfillers-wordsweasels-wordshedges-307) requires.
  `scripts/build-filler-words.py FILLERS_DATA EXPRESSO_FILLERS` rebuilds it byte for byte from
  `data.txt` at words/fillers commit `d1924b589e6b76d7cd8c4ee3c6d4859aadae8fc7` and
  `corpora/fillers` at expresso commit `2d58bbbcda716ba1e3acc4615c522ad538effe06` (the last commit
  that has it), checking both sha256s. The three words/fillers entries expresso doesn't back
  (*amazing*, *stuff*, *things*) are not vendored. *amazing* and *stuff* are in the project list,
  chosen independently as two of the commonest vague words, and *thing* is matched by lemma.
  Both MIT notices are in `style/data/licenses/fillers.txt` (2,629 bytes) and NOTICE.

words/hedges and words/weasels were rejected in the same check (their sources are unlicensed or
copyrighted web pages) and nothing from them is used.

### Not duplicated

- **LanguageTool `TIRED_INTENSIFIERS`** (in `PLAIN_ENGLISH`, off by default, which presets can turn
  on) flags every *really*, *very*, *quite*, *extremely* and *severely*. The first four are in the
  filler list. The report leaves them out (`vague_words::COVERED`), so turning on both never
  reports one word twice. *severely* is not in either list.
- **The overused-words report's filler list** counts *actually*, *just* and *simply* (and *quite*,
  *really*, *very*), so those are left out here too. *somehow* and *suddenly*, its other fillers,
  are not in this report's lists.
- `PLAIN_ENGLISH`'s other rules (`IN_MY_OPINION`, `AS_A_MATTER_OF_FACT`, …) suggest a shorter
  phrase for a wordy one; none of their phrases is in these lists. The report differs from those
  rules by counting per mode and per term and never suggesting a replacement.

## Measurements (2026-10-21)

`python3 bench/style/vague_hidden_verbs.py` on bench/public (CC0 stories and the moby150k
excerpt):

| Text | Narration matches / words | per 1,000 | Dialogue matches / words | per 1,000 |
|---|---|---|---|---|
| story1 | 14 / 1,922 | 7.28 | 5 / 287 | 17.42 |
| story2 | 17 / 1,780 | 9.55 | 4 / 258 | 15.50 |
| story3 | 16 / 1,740 | 9.20 | 4 / 268 | 14.93 |
| story4 | 21 / 1,990 | 10.55 | 3 / 185 | 16.22 |
| story5 | 11 / 1,638 | 6.72 | 4 / 218 | 18.35 |
| moby150k | 236 / 24,602 | 9.59 | 3 / 400 | 7.50 |
| **all** | 315 / 33,672 | 9.35 | 23 / 1,616 | 14.23 |

With `--books` (the 13 public-domain books and chapters in `bench/style/sources.json` plus
bench/public), 6,269 matches in 581,826 narration words (10.77 per 1,000) and 3,945 in 289,952
dialogue words (13.61 per 1,000). The commonest terms are *so* 1,673, *thing* 1,287, *might* 947,
*well* 520, *too* 451, *perhaps* 408, *seem to* 377 and *begin* 374.

**Precision, hand-judged** (is the span the listed word in its vague sense?): 48 of 50 on a
development sample of bench/public matches before the last guards; its two misses (*would rather*,
conjunction *and so*) are guarded now. 48 of 50 on a held-out sample from the books afterwards
(`--sample 50 --report vagueWords --seed 3072`); the misses are *that much blunt* (slang noun read
as an adjective) and the formula *be so kind as to*. No recall measure. The PR review's fixes (the
*a lot* / *a great deal* guard, capitals after an opening quote) came after these samples, which
were not re-judged; bench/public is unchanged.

## Size

The two data files are 759 and 1,137 bytes (403 and 669 with `gzip -9`). Both reports together grow
the WASM module by 20,776 gzipped bytes; see [STYLE-HIDDEN-VERBS.md](STYLE-HIDDEN-VERBS.md#size).
