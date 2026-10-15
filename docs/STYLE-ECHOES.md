# Echoes and repeated phrases

Issue [#245](https://github.com/cmlarsen/writemorebetter/issues/245). Both reports read the shared
document pass from [#243](STYLE-DOCUMENT.md) and return spans; neither suggests replacement words.

```rust
use writemorebetter_style::reports::echoes::{echoes, EchoOptions};
use writemorebetter_style::reports::phrases::repeated_phrases;
use writemorebetter_style::{analyze_document, DocumentOptions};

let options = DocumentOptions::new().with_skip_names(vec!["Mara".into()]);
let document = analyze_document(&tokenizer, text, &options);
let echo_report = echoes(&document, &EchoOptions::new().with_window(50));
let phrase_report = repeated_phrases(&document);
```

`EchoOptions` has `window` (default 50 words) and `match_forms` (default true; JSON alias
`matchForms`). Both reports need the document's token readings, which the default
`include_token_details: true` keeps. With compact documents, function words are recognized only by
the built-in list and words match by spelling. Swift and WASM bindings land in a follow-up.

## Words

A word is a token the document counts in `word_count`: it contains a letter or digit and is not a
skipped name. Clitics count separately (`can’t` is `ca` + `n’t`), so densities use the same
denominator as every other report.

A word is a **function word** if any of its readings has a POS tag outside the noun, verb, adjective
and adverb families (`NN*`, `VB*`, `JJ*`, `RB*`), or if its spelling or any lemma is on the crate's
built-in list of about 190 closed-class words, auxiliaries and light adverbs (`be`, `have`, `do`,
`not`, `very`, `then`, `still`, …). The list was written for this crate; nothing comes from
LanguageTool's resources. "Any reading" removes ambiguous closed-class words: `his` also has a
`hi/NNS` reading, `one` has `CD` and `PRP`, and `up`, `about` and `like` have `IN` or `RP`. The cost
is the rare content use of such a word (`a will`, `the outside`), which never echoes.

## Keys and ambiguous tokens

The tagger often gives a word several readings with different lemmas (`lit` → `lit`, `light`;
`left` → `left`, `leave`), and disambiguation removes some readings in some contexts but not in
others. On the public Moby-Dick excerpt, 365 of 25,743 word tokens carry a lemma set that differs
from the most common set for their spelling. Keying each token by its own readings would split
`crooked` (readings `crook`, `crooked`) from `crooked` (reading `crooked`) a few sentences later.

**Rule:** every content spelling gets one key for the whole document. The report lowercases the
spelling (folding `’` to `'`), collects every single-word lemma the tagger gives that spelling
anywhere in the document, and takes the alphabetically first lemma that differs from the spelling;
if there is none, the spelling itself is the key. `walked` and `walking` both become `walk`, `lit`
becomes `light`, and every `crooked` gets the same key. Function words are keyed by their lowercased
spelling, so `was` and `is` stay distinct inside phrases.

Each token has exactly one key, so a token can never echo itself through two readings, and two
ambiguous tokens produce at most one echo between them. The trade-offs:

- Homographs merge when the tagger gives one spelling both lemmas: `left` (the side) can echo
  `leave`. These pairs already share a reading in the tagger's own analysis.
- A key can change when text elsewhere in the document adds a reading for the same spelling.
  Repeats of the same spelling always match each other regardless.

## Echoes

A content word **echoes** when its key occurred among the previous `window` words. The distance is
the difference in word index (adjacent words are 1 apart) and the window is inclusive: with the
default 50, a repeat 50 words later echoes and one 51 words later does not. Function words still
count toward the distance; skipped names do not. Windows cross sentence, paragraph and dialogue
boundaries.

Each echo links the later word to the **nearest** earlier occurrence of its key, so three uses of
`rain` close together produce two echoes, not three. `match_forms: false` keys words by spelling
only, so `walked` no longer echoes `walking`.

`EchoReport` returns every echo (`lemma`, the `earlier` and `later` spans, `distance`), the
`word_count` and `density_per_1000 = echoes × 1000 / word_count`. Empty documents report 0.

## Repeated phrases

A phrase is a run of 2–5 consecutive words inside one sentence, compared by key. Punctuation inside
the sentence is ignored, so `turned to the window` repeats as `turns, to the window`. A sentence end
or a skipped name breaks the run. A phrase is **repeated** if it occurs at least twice in the
document. The report drops:

- phrases made only of function words (`it was in the`), and
- phrases whose every occurrence lies inside an occurrence of a repeated phrase one word longer.
  This applies transitively, so a phrase inside a longer repeat is dropped. A phrase that also occurs
  on its own is kept with **all** its occurrences: in `The old grey house… The old grey house… A grey
  house…`, both `the old grey house` (2) and `grey house` (3) are reported.

A repeat longer than five words appears as overlapping five-word phrases. Each span runs from the
first word's start to the last word's end, including the punctuation between them. `PhraseReport`
lists `lemmas` (the keys), `count` and every span, ordered by count, then length, then first
occurrence.

The issue keeps any phrase with at least one content word, so article-plus-noun bigrams count: on the
150 KB Moby-Dick excerpt, 1,878 phrases repeat and the most frequent is `the whale` (43). Callers that
want fewer should filter by count or length.

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
injects echoes and repeated phrases into 80 public-domain passages and scores only the flags that
appear in the injected version and not in the clean one, so natural echoes in the source never count
as false positives. Current results: echoes 51/51 injected pairs found with no extra flags and no
flags on 28 far-apart controls; repeated phrases 80/80 with no extra flags. The suite measures the
definitions above on injected text. Natural echoes in the passages are unlabeled, so it says nothing
about which echoes a writer would consider worth fixing.

## Time and memory

Native release build, Apple M5 Pro, `NLPRULE_PARALLELISM=off`, shared machine (load average 13–18
during the runs, so the document pass is slower than in
[STYLE-DOCUMENT-BENCHMARKS.md](STYLE-DOCUMENT-BENCHMARKS.md)). Reports run on an already analyzed
document; the budget from #243 is 1 second for 150 KB.

```sh
cargo build --release --manifest-path bench/tools/wmb-document/Cargo.toml
NLPRULE_PARALLELISM=off /usr/bin/time -l bench/tools/wmb-document/target/release/wmb-echoes \
  --benchmark storage/en_tokenizer.bin bench/public/moby150k.txt 5 --hard-wrapped
```

| Workload | Words | Document pass (median) | Echoes | Repeated phrases | Peak RSS, document only → with reports |
|---|---:|---:|---:|---:|---:|
| `moby150k.txt`, 149,091 bytes | 25,743 | 905 ms | 9 ms | 16 ms | 65.4 → 71.7 MiB |
| novel workload, 581,798 bytes | 102,972 | 3,756 ms | 32 ms | 61 ms | 120.6 → 150.9 MiB |

The novel workload is the #243 one (the Moby excerpt unwrapped and repeated four times). Because it
repeats, every phrase repeats too, which inflates its 20,377 phrases and their memory. The reports add
about 3% to the document pass. The 150 KB total stays under the 1 second budget; the novel-length
document pass does not, and still belongs off the UI thread.
