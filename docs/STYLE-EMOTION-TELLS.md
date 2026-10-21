# Emotion tells

Issue #304 (part of #296) adds an **opt-in lens** that flags narration naming a feeling outright:
*she was afraid*, *he felt angry*, *fear washed over her*, *she felt a surge of relief*. It
diagnoses only. It shows the span and the emotion word, never a replacement, and it doesn't say a
tell is wrong: writers use them on purpose, and published fiction is full of them (1.45 per 1,000
narration words below).

| Surface | Call |
|---|---|
| Rust | `emotion_tells::report(&Document, &EmotionTellsOptions)` |
| Retained Rust | `document.emotion_tells(options_json)` |
| WASM | `document.emotionTells(optionsJSON?)` → `EmotionTellsReport` (`wasm/style-results.d.ts`) |
| Swift | `document.emotionTells(optionsJson:) -> EmotionTellsReport` |

The report needs token readings (`StyleError::NeedsTokenDetails` on compact documents). Options are
camelCase JSON with snake_case aliases. Unknown keys are `invalidOptions`.

| Option | Default | Meaning |
|---|---|---|
| `includeDialogue` | `false` | Also flag tells inside quoted dialogue. A character saying *I was so angry* is speech, not the narrator telling, so it is off by default. |

## Result

`{tells, groups}`. Each tell has:

- `span`: from the verb to the feeling (`was afraid`, `felt a surge of relief`), or from the
  feeling to its verb for subject tells (`Fear washed`). Coordinated feelings on one verb are one
  tell whose span covers them all (`was angry and afraid`); `emotion` and `word` name the first.
- `emotion` and `verb`: the two tokens' spans.
- `word`: the lexicon entry. For an adjective or noun this is its lemma (`afraid`, `relief`). For a
  participle it is the verb whose participle names the feeling (`startled` → `startle`).
- `pattern`: `predicate`, `object` or `subject` (below).
- `sentence`: the global sentence index, and `mode`.

`groups: Split<EmotionTellsGroup>` holds `tells`, `words` and `perThousandWords` (null when there
are no words) for the text scanned. Without `includeDialogue`, dialogue words aren't scanned, so
`dialogue` is all zeros and `all` equals `narration`. Spans are UTF-16 `{start, end}` in the
bindings, like every other report.

## What counts

A tell is a feeling from the bundled lexicon in one of three constructions, inside one sentence:

- **Predicate.** A linking verb (*be, feel, become, get, grow, seem, look, sound, appear, remain,
  stay*) followed by an emotion adjective or participle. *felt* after *’d* counts as *feel* even
  though nlprule reads it as the verb *to felt* (*She’d felt angry*). Adverbs, negation, *a little* / *a bit*
  and coordinated adjectives may come between (*was not so well pleased*, *was tired and afraid*).
  Causatives with an object pronoun or name (*made her angry*, *left him shaken*) and states after
  *in* (*was in love*, *fell back in dismay*, *was in mortal fear*) count too.
- **Object.** *feel, sense* or *experience* with an emotion noun as object, optionally behind
  determiners, adjectives and a measure noun plus *of* (*a surge of*, *a pang of*, *a flicker of*,
  32 measures). It also counts a feeling after *with* or *by* following *filled, overcome,
  overwhelmed, consumed, seized, gripped, struck, racked, flooded, choked, full, sick* and a few
  more (*filled with dread*, *sick with unsatisfied love*).
- **Subject.** An emotion noun, with or without a measure, that opens a clause and is followed by a
  verb of movement through a body (*wash, surge, flood, rise, grip, seize, course, sweep, flare,
  bloom, …*, 48 verbs), with a person pronoun or possessive within the next five tokens before
  punctuation: *Fear washed over her*, *A wave of panic rose in her chest*. *The fear spread through
  the city* doesn't count.

Not counted: feelings named by a verb (*she feared him*, *he loved her*), attributive adjectives
(*a happy face*), nouns outside those frames (*her fear of the dark*), and bodily sensations, which
are the showing a writer would use instead (*a chill ran down her spine*).

### Guards

- *I'm afraid so / not*, and first-person present *I'm afraid* followed by a clause opener (*I, we,
  you, he, she, it, they, that, there, the*) or a comma (*I'm afraid I can't help you*, *had more, I
  am afraid, than*): hedges. *I am afraid.* at the end of a sentence is first-person narration and
  counts. *I'm sorry* without *for*: an apology.
- *I'd be glad to help* and other adjectives followed by *to* after *would / will*: offers.
  *pleased to call*, *happy to say*, *to term / report / inform / announce / approve*: formulas.
- *content to / with*: willing or settling (*was content to wait*, *content with less*).
- *concerned* only before *about, for, that, at*. *As far as she was concerned* and *where they
  were concerned* mean involved.
- *guilty* only after *feel, look, seem, sound*; *was guilty* is usually a verdict.
- *hurt* after an injury adverb (*badly, seriously, slightly, severely, critically, fatally,
  mortally, gravely*) or before *in* means injured (*was badly hurt in the crash*). *His pride was
  hurt* still counts.
- An adjective directly followed by a noun is attributive (*his mother’s troubled forehead*,
  *being shy and retiring persons*), unless a verb or modal follows the noun. That noun is the
  subject of a that-less clause (*She was afraid things would go wrong*).
- Subjects *it, this, that, there, what, which, everything* (*It was sad*). After a noun, *that* and
  *which* are relative pronouns, so *the man that was afraid* counts.
- Quoted text is dialogue even when the document has no speech evidence for it. An untagged
  quotation's feelings are still a character's words. This is stricter than the shared
  narration/dialogue split, which needs a tag or speech context.

Text is matched on folded tokens (lowercase, curly apostrophes as straight), so *I’m* and *I'm*
behave the same. Spans come from the tokenizer and stay on character boundaries for curly quotes,
em dashes, ellipses, U+2028 and non-ASCII names (tests in `style/tests/emotion_tells.rs`).

## Lexicon

`style/data/emotions/emotions.tsv` has 760 words: 438 nouns (`n`), 235 adjectives (`a`) and 112
verbs whose participle names a feeling (`v`). A word can carry several kinds (`anger	nv`). It is
10,042 bytes (3,671 with `gzip -9`). `style/data/licenses/WordNet-LICENSE` adds 1,625 bytes. Both
are `include_str!`'d into the style crate and parsed once on first use.

It is derived from **Princeton WordNet 3.0** under the WordNet License, so it is a separate data
file. It carries the copyright notice in its header and the full notice and disclaimer in
`style/data/licenses/WordNet-LICENSE` (verbatim from the release), and NOTICE has an entry. It is
not MIT/Apache. The derivation was vetted in
[STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#princeton-wordnet-30--31-304-307-hidden-verbs).
NRC EmoLex, WordNet-Affect and DepecheMood were rejected there.

`scripts/build-emotion-lexicon.py WordNet-3.0.tar.gz` rebuilds it byte for byte. It reads the
tarball in place, checks its sha256
(`640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52`) and reads the bundled
Lancaster table:

1. **Nouns:** single-word lemmas of every `noun.feeling` synset, plus the hyponym closure of
   `feeling.n.01` and `emotion.n.01`.
2. **Adjectives:** adjectives those nouns point to with a derivational (`+`) or attribute (`=`)
   pointer, plus their "similar to" satellites (*afraid* → *terrified*, *aghast*).
3. **Verbs:** `verb.emotion` synsets, and verbs derived from an emotion noun (*astonish* →
   *astonishment*), restricted to synsets with WordNet frame 10, "Something ----s somebody". In
   those verbs the stimulus is the subject, so the participle names the experiencer's state
   (*frightened*, *relieved*). *Love, hate, fear* lack the frame; *was loved* is not a feeling.
4. **Weighting by Lancaster.** A candidate is kept when its most frequent WordNet sense (first in
   `index.*`) is one of the selected synsets, or when the bundled Lancaster Sensorimotor Norms rate
   it dominantly interoceptive (`style/data/norms/lancaster-perceptual.tsv`, CC BY 4.0). That
   rescues words whose first sense is something else, such as *anxiety* (a state first), *despair*
   and *loneliness*. Satellites always need the Lancaster rating or review, because the "similar
   to" net pulls in traits (*funny, poor, mature*).
5. **Review.** A reviewed exclusion list removes bodily sensations (*chill, nausea, itch*), traits
   and attitudes (*arrogant, jovial, malicious*), stimulus words (*dreadful, scary*, every *-ing*
   adjective) and words whose common sense is not the feeling (*fire, heart, close, sober*). A
   verbs-only list removes cognitive or social participles (*puzzled, intoxicated, disgraced,
   comforted*). A reviewed inclusion list keeps 73 candidates that fail step 4 but name a feeling in
   fiction (*guilt, guilty, misery, humiliation, sheepish, shocked, sorry, relieve, surprise*). The
   script exits if a reviewed word isn't a WordNet candidate or is on both lists. Every word still
   comes from WordNet.

## Precision on published fiction (2026-10-20)

`python3 bench/style/emotion_tells.py score` runs the report with default options over the
public-domain books and chapters already used for pacing. Those are 13 Standard Ebooks texts (CC0):
*Pride and Prejudice*, *Moby-Dick*, *The Blue Castle*, *The Murder
of Roger Ackroyd*, *The Thirty-Nine Steps*, *The Hound of the Baskervilles*, *The Age of
Innocence*, *Silas Marner*, *Treasure Island*, and chapter 1 of *Styles*, *Gatsby*, *The Sign of
the Four* and *Anne of Avonlea*. The five CC0 stories in `bench/public` are included too. Nothing
from `bench/corpus` was read.

| | Value |
|---|---:|
| Narration words scanned | 555,918 |
| Tells | 808 (762 predicate, 42 object, 4 subject) |
| Per 1,000 narration words | 1.45 |

Two seeded samples were judged one at a time by Claude Opus 5.5. A row is true when narration
names a character's (or the narrator's) feeling through the flagged construction. The labels and
a note on each false row are in `bench/style/labels/emotion-tells.jsonl` and
`emotion-tells-heldout.jsonl`; the scores are in `bench/style/emotion-tells-baseline.json`.

| Sample | Rows | Kept by current code | True | Precision |
|---|---:|---:|---:|---:|
| Development (seed 304), judged before the last guards | 200 | 187 | 176 | **0.941** (178 / 200 = 0.89 before the guards) |
| Held-out (seed 305), drawn and judged after them, from flags not in the development sample | 100 | 100 | 91 | **0.910** |

By pattern on the held-out sample: predicate 85 / 94 (0.904), object 5 / 5, subject 1 / 1. Across
both samples, object tells are 15 / 15. Nineteenth-century narration rarely writes *fear washed
over her*, so the subject and object patterns have small samples. Their precision here is
anecdotal; the fixtures cover them.

The guards added after the development sample removed 11 false and 1 true row. They were *content
to / with* (7), formulas such as *graciously pleased to approve* and *are pleased to call* (3), the
attributive *mother’s troubled forehead* and *shy and retiring persons* (2), and an untagged
quotation read as narration (1).

The review fixes (PR #320) changed 24 flags: 810 became 808. Thirteen overlapping tells merged into
the coordinated tell beside them (*was considerably upset and worried* is now one tell, not two).
Four labeled rows were rekeyed to the merged span with their labels unchanged. One more, *became
pensive*, merged into a row the development sample already labels, so it counts as dropped true. Eleven
flags are new, nine of them from the fixed *a little* path. They are outside both seeded samples and
left unlabeled there, but judged by hand they are 10 true and 1 false. The false one is *that wild
madness that’s only calm*, a non-sentient subject after a relative *that*. The *’d felt*, that-less
clause, *hurt* and hedge fixes changed no flags in these texts.

The held-out sample's 9 false rows, left untuned, show the remaining limits:

- **Non-sentient subjects** (4): *the dining-room was chillier and gloomier*, *Mr. Wickham’s adieus
  were much more affectionate*, *her acceptance of the invitation was most ready and grateful*, *the
  face was wonderfully calm* (a dead man). The report doesn't know which nouns can feel. Pronoun
  and name subjects are almost always right; a common-noun subject is where most noise comes from.
  The development sample adds *the sea is moderately calm* and *my ideas were completely upset*.
- **Other senses** (5): *mad* meaning insane (*made him mad*, *must be stark mad*), *bored* meaning
  drilled (*his broad fins are bored*), *annoyed by a show of hostilities* (harassed) and
  *uneasiness had been equally excited* (aroused). The development sample adds *relieved by crowds
  of the daylight lookouts* and *in this confusion* (disorder).

There is no recall measure yet: the samples judge flags, not missed tells. A writer or owner
adjudication of these labels is owed, as for the other judged sets.

## Size

Bundled into WASM, the lexicon and report grow `writemorebetter_wasm_bg.wasm` (release,
wasm-bindgen 0.2.129, macOS, Rust's wasm32 target, no `wasm-opt`) from 5,678,182 to 5,724,344
bytes (+46,162, +0.8%), and from 1,461,541 to 1,474,140 with `gzip -9` (+12,599). The data file itself
is 10,042 bytes (3,671 gzipped).
