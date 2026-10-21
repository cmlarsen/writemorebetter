# Alliteration

Issue #307 (part of #296) adds an **opt-in, informational lens** that marks runs of stressed words
in one sentence that start with the same consonant sound: *toils, trials and tribulations*, *the
watery world of woe*. Fiction uses alliteration on purpose, so the report passes no verdict. It
shows where the runs are, and how often they occur in narration and in dialogue, so a writer can
tell a deliberate effect from an accidental jingle. Like every style report it never suggests other
wording.

| Surface | Call |
|---|---|
| Rust | `alliteration::report(&Document, &AlliterationOptions) -> Result<AlliterationReport, StyleError>` |
| Retained Rust | `document.alliteration(options_json)` on a `StyleDocument` |
| WASM | `document.alliteration(optionsJSON?)` |
| Swift | `document.alliteration(optionsJson:) -> AlliterationReport` |

It works on compact documents (no token readings needed).

## Options

| Key | Default | Meaning |
|---|---|---|
| `minimumWords` | 3 | Fewest *different* words sharing a sound that make a run; at least 2 |
| `maximumGap` | 0 | Most stressed words with another sound allowed between two words of a run |
| `skipDialogue` | false | Leave dialogue out: no runs there, and its words leave the totals |

Options use camelCase with snake_case aliases and reject unknown keys, as for every report.

## Result

`runs` lists each run in text order with `span` (first word to last), `spans` and `words` (each
word as written), `sound`, `sentence` (index in the document) and `mode` (narration or dialogue).
Bindings serialize spans as UTF-16 `{start, end}`. `groups` is a `Split` (all, narration, dialogue)
of `words`, `runs` and `perThousandWords`, using each mode's word count as the denominator.

`sound` is a lowercase ARPAbet consonant label: `b ch d dh f g hh jh k l m n ng p r s sh t th v w y
z zh`. `jh` is the sound of *j* in *jam* and *g* in *gem*, `dh` is *th* in *then*, `hh` is *h*, and
`zh` is the *s* in *measure*.

## What counts

- **Sound, not spelling.** *Cats keep chorus lines* is one `k` run; *Cheap chemical charts* is not a
  run (`ch`, `k`, `ch`); *Gentle girls get gems* is not (`jh`, `g`, `g`, `jh`); *Nine knotted nets* is
  an `n` run. *Once* starts with `w` and *hour* with a vowel. Words that start with a vowel sound
  never form runs, but they still sit between other words and count toward the gap.
- **Stressed words.** Articles, pronouns, prepositions, conjunctions, question words, auxiliaries
  and modals (and their contractions: *she’d*, *don’t*) are unstressed in running speech. They are skipped:
  they neither join a run nor break one, so *Tom took the tram to the town* is one `t` run. Clitics
  the tokenizer splits off (`’s`, `’d`, `n’t`) are skipped the same way. *Up*, *down*, *out*,
  *off*, *over*, *under* and *past* stay in: as particles (*broke down*) they carry stress. Numbers and names in the
  document's `skipNames` are skipped too.
- **The first syllable's stress is not checked.** *Deliberate* (stress on its second syllable)
  counts as a `d` word. Keeping CMUdict's stress for polysyllables would add about 19,600 words
  whose first syllable is unstressed, ten times the onset table, and no spelling rule predicts it
  well (the best prefixes, *de-* and *re-*, are close to even).
- **Distinct words.** A repeated word counts once toward `minimumWords`: *Dark, dark, dark days* is
  repetition, not a run; *Dark, dark, deep days* is a run.
- **Clusters share their first sound.** *Sleeps, swims, seems* is an `s` run.
- **Inside one sentence and one mode.** A run never crosses a sentence boundary (including one at a
  U+2028 line separator) or a change between narration and dialogue: in *“Please put the pail
  down,” Peter pleaded patiently.* the dialogue run and the narration run are separate.
  Punctuation inside a sentence does not break a run: commas, em dashes and ellipses are fine
  (*seen seven swans—so still…*).

## Initial sounds: CMUdict with a spelling fallback

`writemorebetter_style::pronunciation::initial_sound(word)` gives a word's first phoneme. It folds
the word to a key (lowercase, straight apostrophes, no leading or trailing punctuation), looks up
the whole key, then the part before the first hyphen, then the stem before an apostrophe, and
otherwise guesses from the spelling with `spelled_onset`: digraphs first (`sch` → `sh`, `chr` →
`k`, `ph` → `f`, `wh` → `w`, `wr` → `r`, `kn` and `gn` → `n`, `qu` → `k`, …), then soft *c* before
*e*, *i*, *y*, soft *g* before *e* and *y* (hard before *i*: *give*, *girl*), `x` → `z`, and *u*
read as `y` in *unit*, *use* but not *under*, *unable*.

The lookup is `style/data/pronunciation/cmudict-onsets.tsv`, a second output of
`scripts/build-cmudict.py` from the same pinned `cmudict.dict` as the syllable table (commit
`74790861`, sha256 pinned). For each headword it takes the first phoneme of the first pronunciation
and keeps only the words the spelling guess gets wrong, filtered by running the guess itself
(`cargo run -p writemorebetter-style --example cmudict_onsets`). Only that one phoneme per word is
bundled, never a whole pronunciation. CI regenerates both tables and fails if
`style/data/pronunciation/` differs, so a change to `spelled_onset` must rebuild the table.

| Table | Rows | Bytes | `gzip -9` |
|---|---:|---:|---:|
| Every key, `word\tsound` | 125,159 | 1,324,850 | 358,104 |
| Spelling misses only (bundled) | 1,943 | 20,888 | 7,018 |

With the table, every CMUdict headword gets CMUdict's first phoneme. The misses are mostly names
(German *sch*, Italian *ce*/*ci*) and the common exceptions: *hour*, *honest*, *who*, *whole*, *sure*,
*sugar*, *chef*, *chorus*, *get*, and *the*, *this*, *they* (`dh`).

The table is CMUdict-derived (BSD-2-Clause-style); NOTICE and `style/data/licenses/cmudict.txt`
carry its notice, as for the syllable table.

### Cost

- **Size.** The table adds 20,888 bytes (7,018 gzipped) to the crate source. The WASM module after
  `wasm-bindgen`, with this report, the title-case check and their bindings, grows from 7,053,229 to
  7,170,422 bytes (+117,193, +1.7%); gzipped with `gzip -9`, from 1,771,694 to 1,795,167 (+23,473,
  +1.3%). Measured against main at `2b90758` on macOS with Rust's wasm32 target, without
  `wasm-opt`.
- **Load.** The 1,943-row table is parsed into a hash map on first lookup and kept for the life of
  the process.
- **Check time.** Native release, median of 21 runs: 0.46–0.58 ms on a 2,000-word story, 7.3 ms on
  `moby150k`.

## Counts on `bench/public`

`cargo test -p writemorebetter-style --release --test alliteration -- --ignored --nocapture
public_counts` prints these (add `SHOW=1` to list the runs). Stories use line mode, `moby150k`
hard-wrapped mode. Runs per 1,000 words in parentheses.

| Text | Words (narration / dialogue) | Default (gap 0) | Narration | Dialogue | Gap 1 | 4+ words |
|---|---|---:|---:|---:|---:|---:|
| story1 | 1,922 / 287 | 1 (0.45) | 1 | 0 | 6 | 0 |
| story2 | 1,780 / 258 | 2 (0.98) | 2 | 0 | 4 | 1 |
| story3 | 1,740 / 268 | 4 (1.99) | 3 | 1 | 9 | 2 |
| story4 | 1,990 / 185 | 1 (0.46) | 1 | 0 | 5 | 0 |
| story5 | 1,638 / 218 | 1 (0.54) | 1 | 0 | 5 | 0 |
| moby150k | 17,939 / 7,063 | 51 (2.04) | 35 (1.95) | 16 (2.27) | 151 | 2 |

**Why gap 0.** Read by hand, the default runs on `moby150k` are mostly Melville's own effects
(*toils, trials, tribulations*, *watery world … woe*, *born, bred … bosom*, *bitter blanks …
black-bordered*) with some incidental ones (*saw such a sight*). Allowing one word between run
members triples the count on `moby150k` and is mostly incidental (*worked … way … work*), so it is an option, not
the default. On the five modern stories the default finds one to four runs each.

## Known limits

- Stress inside polysyllables is not read (above): a content word with an unstressed first
  syllable (*deliberate*, *conclude*) still counts by its first sound.
- The function-word list is English and lexical; a stressed *that* or *here* is skipped anyway.
- Words outside CMUdict (coinages, invented names) use the spelling guess.
- Vowel alliteration (assonance at word starts: *apt alert attention*) is not reported.
