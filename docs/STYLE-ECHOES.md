# Echoes and repeated phrases

Issue [#245](https://github.com/cmlarsen/writemorebetter/issues/245). Both reports read the shared
document pass from [#243](STYLE-DOCUMENT.md) and return spans; neither suggests replacement words.

```rust
use writemorebetter_style::reports::echoes::{echoes, EchoOptions};
use writemorebetter_style::reports::phrases::{repeated_phrases, PhraseOptions};
use writemorebetter_style::{analyze_document, DocumentOptions};

let options = DocumentOptions::new().with_skip_names(vec!["Mara".into()]);
let document = analyze_document(&tokenizer, text, &options);
let echo_report = echoes(&document, &EchoOptions::new().with_window(50));
let phrase_report = repeated_phrases(&document, &PhraseOptions::new());
```

`EchoOptions` has `window` (default 50 words) and `match_forms` (default true; JSON alias
`matchForms`). `PhraseOptions` has `min_length` and `max_length` (default 2 and 5; set both with
`with_length`) and `min_content_words` (default 2), with camelCase JSON aliases. A `max_length`
beyond the document's longest sentence is clamped to it, so any value, `usize::MAX` included, is safe. Both reports need the document's token readings, which the default
`include_token_details: true` keeps. With compact documents, function words are recognized only by
the built-in list and words match by spelling. Swift and WASM bindings land in a follow-up.

## Words

A word is a token the document counts in `word_count`: it contains a letter or digit and is not a
skipped name. Clitics count separately (`can’t` is `ca` + `n’t`), so densities use the same
denominator as every other report.

A word is a **function word** or a **content word**, decided once per lowercased spelling for the
whole document, so the same spelling never changes class between sentences. A spelling is a function
word if:

- it is on the crate's list of 198 closed-class words, auxiliaries and light adverbs (pronouns,
  determiners, prepositions such as `through` and `over`, conjunctions, `be`, `have`, `do`, `not`,
  `very`, `then`, `still`, indefinites such as `nothing`, …);
- any of its readings is a number (`CD`): `two`, `hundred`; or
- most of its occurrences have no **content reading**: a noun, verb, adjective or adverb reading
  (`NN*`, `VB*`, `JJ*`, `RB*`) whose lemma is not on the list. An occurrence with a content reading,
  or with no readings at all, counts as content.

A listed lemma therefore excludes a word only when it is the word's only content reading. `being`
(`be/VBG`), `others` (`other/NNS`), `’d` (`have/VBD`, `will/MD`) and `’ve` are function words, while
`evening` (`evening/NN`, `even/VBG`) and `willing` (`willing/JJ`, `will/VBG`) are content words and
echo.

The list was written for this crate; nothing comes from LanguageTool's resources. The per-occurrence
majority keeps frequent words whose tags include a stray function reading: `need` (`MD`), `come`
(`UH`), and the particles `up`, `down`, `out`, `back` and `away` (`IN`, `RP`) are content words and
echo. An earlier rule excluded any word with one non-content reading, which dropped 1,351 such tokens
on the Moby-Dick excerpt and classified `heads` differently from sentence to sentence. With the
current rule, function words are 13,999 of the excerpt's 25,743 words. Closed-class words that carry
a content reading (`his` has `hi/NNS`, `if` has `if/NN`) stay out through the list.

## Keys and ambiguous tokens

The tagger often gives a word several readings with different lemmas (`lit` → `lit`, `light`;
`thought` → `think`, `thought`), and disambiguation removes some readings in some contexts but not
others. On the public Moby-Dick excerpt, 365 of 25,743 word tokens carry a lemma set that differs
from the most common set for their spelling.

**Rule:** content words match by **lemma family**. The report builds a graph whose nodes are the
document's content spellings (lowercased, `’` folded to `'`) and their single-word lemmas. Each
spelling has an edge to every lemma the tagger gives it anywhere in the document, and to itself
treated as a lemma. Each connected component is one family. `thought` (lemmas `think`, `thought`)
joins `think`, `thinking` and `thoughts` (lemma `thought`), so the noun plural echoes the singular
even when `thought` is also a verb elsewhere. Because a spelling is also its own lemma, `thoughts`
joins `thought` even when every `thought` in the document is tagged as the verb, and `wounds` joins
`wound` after `He wound the clock`. Every spelling is in exactly one family, so a token can never echo itself through two
readings, and two ambiguous tokens produce at most one echo. Function words match by spelling, so
`was` and `is` stay distinct inside phrases. Families stay small: on the Moby excerpt the largest has
7 spellings (`lie`, `lay`, `laid`, `lain`, `lies`, `lying`, `laying`), followed by the `go`, `wind`
(with `wound`, `wounds`, `wounded`), `mean` and `see`/`saw` families.

**Keys and labels.** Every echo and every phrase position carries two strings:

- `key` (phrases: `keys`) is the family's name: its alphabetically first reading lemma, or the
  spelling when no member has a reading. It is the same for every match in the family, so it is the
  field to group or count by. Function words in phrases are keyed by spelling.
- `label` (phrases: `labels`) is the display form, and it always names the matched words. It is a
  lemma that every matched word carries in its readings, preferring one that is itself the spelling
  of a matched word: `wound` … `wounds` → `wound`, `fell` … `fell` → `fell`, `walked` … `walking` →
  `walk`. If the readings share no lemma, a shared spelling is used (`thought` tagged as a verb, then
  as a noun → `thought`). Failing that, the label is the earlier word's spelling. A third family
  member never becomes the label, so `good` … `well` is labelled `good`, not `best`.

The trade-offs:

- Homographs merge when the tagger gives one spelling both lemmas. `He wound the clock` puts
  `wound` in the `wind` family, so a later `wind` echoes it, labelled `wind`. On the Moby excerpt the
  `wind` family holds both senses. `left` (the side) can echo `leave` the same way.
- Families can chain through a third spelling. `better` carries both `good` and `well`, so `good`
  … `well` echo even though they share no lemma; on the Moby excerpt the PR #265 re-review counted 9 such
  chains among 1,168 echoes. The label then names the earlier word.
- A family can change when text elsewhere in the document adds a reading. Repeats of the same
  spelling always match each other regardless.

## Echoes

A content word **echoes** when its key occurred among the previous `window` words. The distance is
the difference in word index (adjacent words are 1 apart) and the window is inclusive: with the
default 50, a repeat 50 words later echoes and one 51 words later does not. Function words still
count toward the distance; skipped names do not. Windows cross sentence, paragraph and dialogue
boundaries.

Each echo links the later word to the **nearest** earlier occurrence of its key, so three uses of
`rain` close together produce two echoes, not three. `match_forms: false` keys words by spelling
only, so `walked` no longer echoes `walking`.

`EchoReport` returns every echo (`key`, `label`, the `earlier` and `later` spans, `distance`), the
`word_count` and `density_per_1000 = echoes × 1000 / word_count`. Empty documents report 0.

## Repeated phrases

A phrase is a run of `min_length`–`max_length` consecutive words (default 2–5) inside one sentence,
compared by family. Punctuation inside the sentence is ignored, so `turned to the window` repeats as
`turns, to the window`. A sentence end or a skipped name breaks the run. A phrase is **repeated** if
it occurs at least twice in the document. The report drops:

- phrases with fewer than `min_content_words` content words (default 2; see below), and
- phrases whose every occurrence lies inside an occurrence of a repeated phrase one word longer.
  This applies transitively, so a phrase inside a longer repeat is dropped. A phrase that also occurs
  on its own is kept with **all** its occurrences: in `The old grey house… The old grey house… A grey
  house…`, both `the old grey house` (2) and `grey house` (3) are reported.

A repeat longer than `max_length` appears as overlapping phrases of that length. Each span runs from
the first word's start to the last word's end, including the punctuation between them. `PhraseReport`
lists `keys` and `labels` (as above), `count` and every span, ordered by count, then length, then first
occurrence.

**Two content words by default.** #245 drops only phrases made entirely of function words. Under
that definition an article plus a noun is a repeated phrase, and on the Moby excerpt such pairs fill
the list: of 1,977 repeated phrases (hard-wrapped paragraphs), 19 of the top 20 are one content word
plus function words.
`min_content_words: 2` is a deliberate refinement, so a reported phrase repeats a combination of
words, not one word that the echoes report already covers. `with_min_content_words(1)` restores the
issue's definition.

| # | `min_content_words: 1` (#245 as written), 1,977 phrases | | `min_content_words: 2` (default), 284 phrases | |
|---:|---|---:|---|---:|
| 1 | the whale | 45 | new bedford | 14 |
| 2 | the sea | 29 | sperm whale | 9 |
| 3 | at last | 26 | father mapple | 8 |
| 4 | out of | 24 | go to sea | 7 |
| 5 | like a | 24 | take up | 6 |
| 6 | a whale | 22 | whaling voyage | 6 |
| 7 | the ship | 21 | the pequod meet the | 5 |
| 8 | think i | 21 | in new bedford | 5 |
| 9 | go to | 21 | landlord say i | 5 |
| 10 | sort of | 21 | right whale | 5 |
| 11 | i think | 20 | great whale | 5 |
| 12 | a little | 18 | sing out | 5 |
| 13 | the room | 18 | go down | 5 |
| 14 | seem to | 17 | go to sea as a | 4 |
| 15 | up the | 16 | sleep two in a bed | 4 |
| 16 | his head | 16 | out of sight | 4 |
| 17 | the landlord | 16 | say the landlord | 4 |
| 18 | the street | 14 | year ago | 4 |
| 19 | the first | 14 | turn round | 4 |
| 20 | new bedford | 14 | monkey jacket | 4 |

Proper names such as `new bedford` and `father mapple` remain unless the caller passes them as skip
names.

## Relation to EN_REPEATEDWORDS

LanguageTool's `EN_REPEATEDWORDS` (#234) stays in `nlprule` as a conformance rule. Echoes is a
separate report and differs in each of these ways:

| | `EN_REPEATEDWORDS` in `nlprule` | Echoes |
|---|---|---|
| Window | fixed: 150 words | configurable, default 50 words |
| Words checked | only words on LanguageTool's synonym list | every content word, by POS |
| Matching | lemma, only against earlier sentences: a repeat inside one sentence is not flagged | lemma (by default) or exact spelling, inside a sentence too |
| Names | skips proper nouns and capitalized words that do not start a sentence | skips the names the caller lists; other proper nouns echo |
| Dialogue | checks only sentences ending in `.`, `!` or `?`, so speech ending in a closing quote is skipped | all sentences, dialogue included |
| Scope | words seen within one `suggest` call | the whole document |
| Output | one match per repeat, with synonym suggestions; off by default (`picky`) | both spans, the distance and density per 1,000 words; no suggestions |

## Accuracy

The tier 2 suite in [`bench/style/README.md`](../bench/style/README.md#echoes-and-repeated-phrases-245)
injects echoes and repeated phrases into 80 public-domain passages, split 40 dev / 40 test with
separate vocabularies. It scores only flags that appear in the injected copy and not in the clean
one, so natural echoes in the source never count against the report. Besides invented words, it
copies a real word from the passage near (gold echo) or far from (control) its natural occurrence,
and it injects homographs (`He thought. It was the thought.` … `There were thoughts.`). Spurious
flags are therefore possible: a natural copy can mismatch the report's tags or window. The current
code finds every injected echo and phrase (dev 60/60 echoes and 40/40 phrases; test 62/62 and
40/40), with no spurious new flags and none on the 56 far controls. The code before the PR #265
review found 51/60 and 53/62. It missed homograph plurals (`feeling` … `feelings`, `wound` …
`wounds`, `rose` … `roses`, `ground` … `grounds`) and natural copies of `indeed`, `attention`, `whist`,
`opposite` and `first`, which a stray function reading had excluded. Natural echoes in the passages
are unlabeled, so the suite says nothing about which echoes a writer wants flagged.

## Time and memory

Native release build, Apple M5 Pro, `NLPRULE_PARALLELISM=off`, shared machine (load average 15–18
during the runs). Reports run on an already analyzed document; the budget from #243 is 1 second for
150 KB.

```sh
cargo build --release --manifest-path bench/tools/wmb-document/Cargo.toml
NLPRULE_PARALLELISM=off /usr/bin/time -l bench/tools/wmb-document/target/release/wmb-echoes \
  --benchmark storage/en_tokenizer.bin bench/public/moby150k.txt 5 --hard-wrapped
```

| Workload | Words | Document pass (median) | Echoes | Repeated phrases | Peak RSS, document only → with reports |
|---|---:|---:|---:|---:|---:|
| `moby150k.txt`, 149,091 bytes | 25,743 | 888 ms | 14 ms | 16 ms | 63.3 → 70.8 MiB |
| novel workload, 581,798 bytes | 102,972 | 3,775 ms | 55 ms | 130 ms | 120.4 → 144.5 MiB |

The novel workload is the #243 one (the Moby excerpt unwrapped and repeated four times). Because it
repeats, every phrase repeats too, which inflates its 16,709 phrases and the time spent labelling
them. Each report builds the document's lexicon (function words and families) itself. The 150 KB
total, 918 ms, stays under the 1 second budget. The reports add about 5% to the novel-length document
pass, which does not meet the budget and still belongs off the UI thread.
