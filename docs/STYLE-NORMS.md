# Sensory balance and diction (#250)

Two opt-in lenses built on per-word psycholinguistic norms. Both diagnose: they return counts, shares
and spans, never replacement words. Licenses for the data are in
[STYLE-DATA-LICENSES.md](STYLE-DATA-LICENSES.md#word-norms-250).

```rust
use writemorebetter_style::reports::{diction, sensory_balance};
let senses = sensory_balance::report(&document, sensory_balance::PerceptualNorms::bundled(),
    &Default::default())?;
let table = diction::ConcretenessTable::parse(&caller_table_text)?;
let diction = diction::report(&document, &table, &Default::default())?;
```

`StyleDocument::sensory_balance(options)` and `StyleDocument::diction(options, table)` wrap these;
the bindings are `sensoryBalance(optionsJSON)` / `diction(optionsJSON, tableText)` in WASM and
`sensoryBalance(optionsJson:)` / `diction(table:optionsJson:)` in Swift. Both need token readings
and reject compact documents. On the hard-wrapped moby150k document (native release build, 12-core
Linux), sensory balance takes 14 ms after a one-time 1.4 ms table load, and diction 13 ms plus 13 ms
to parse a 36,800-word TSV table, which every call does.

## Word lookup

Both reports look up every counted word (skipped names excluded) by lemma, using the document's
readings. The POS family is chosen as echoes chooses it (chunk and `-ing` disambiguation, falling
back to every content family) and the token's lemmas in that family are looked up; when exactly one
is in the table it is used, when several are the one equal to the surface form is used, and when
none is the lowercase surface form is tried. Tokens without readings use the surface form.
Excluded: echoes' function-word list, *say/ask/tell/talk/speak/reply/answer*, tokens whose readings
are all proper nouns, and capitalized words that do not open a sentence (after an opening quote,
bracket, dash or colon counts as opening). The last rule keeps character names such as *Daisy* out of
the counts when no skip list is supplied; supply `skipNames` to also drop sentence-initial names.

Because candidate lemmas are filtered by table membership, a smaller table can resolve a token that a
larger one leaves ambiguous: shrinking the bundled table from the 3.5/0.5 floor to the shipped
4.0/1.0 floor added 2.5% sense words on the public texts, mostly *-ing* forms (*holding*,
*ticking*, *creaking*).

## Sensory balance

`style/data/norms/lancaster-perceptual.tsv` is derived from the Lancaster Sensorimotor Norms by
`scripts/build-lancaster-norms.py`. Each line is `word`, dominant modality letter, its mean strength
and its lead over the second-strongest modality, both in tenths (0–50). Only single lowercase words
whose dominant modality rates ≥ 4.0 and leads by ≥ 1.0 ship: 4,492 words, 76,301 bytes (26,994
gzipped). Options can tighten these thresholds; values below them select nothing more from the
bundled table. `PerceptualNorms::from_tsv` loads a caller table in the same format.

Modalities map Visual → `sight`, Auditory → `sound`, Haptic → `touch`, Olfactory → `smell`,
Gustatory → `taste`, Interoceptive → `interoception`. Interoception in these norms includes named
emotions (*fear*, *joy*, *love*) as well as bodily states (*ache*, *tired*).

A **sense word** is a looked-up word whose dominant modality has strength ≥ `minimumStrength` (4.0)
and leads the next modality by ≥ `dominanceMargin` (1.0); sight words must lead by ≥ `sightMargin`
(3.0). Lemmas in `style/data/norms/not-sensory.txt` (our own list of generic and speech-act words the
norms rate perceptual, such as *way*, *up*, *story*, *explain*) never count, nor do *see* and *feel*
followed by a clause or comparison (*that*, *if*, *how*, *like*, *as*, …) or *feel* followed by an
adjective (*felt ashamed*).

The result has `senseWords` (span, lemma, modality, strength, mode), document `groups`, and the same
`groups` per `chapters` and `scenes`: `words`, `senseWords` and all six `modalities` as
`{modality, count, percent}`, split into `all`, `narration` and `dialogue` by the shared token mode.
Percent is null below `minimumSenseWords` (20) in that group. Scenes follow the document model:
headings, scene markers, two blank lines and U+2029 start one; chapters start at recognized chapter
headings. `singleSense` names the modality holding ≥ `singleSensePercent` (85) of a unit's
**narration** sense words, when it has at least `minimumSenseWords` of them; dialogue is reported but
not flagged.

### Calibration

Calibrated on `bench/public` (stories 1–5, line mode; Moby, hard-wrapped) and three CC0 Standard
Ebooks chapters fetched into the ignored `bench/style/out/se/`: Chopin, *The Awakening* ch. 10
(`kate-chopin_the-awakening` `49955125`), London, *The Call of the Wild* ch. 1
(`jack-london_the-call-of-the-wild` `561cc933`) and Fitzgerald, *The Great Gatsby* ch. 8
(`f-scott-fitzgerald_the-great-gatsby` `fe47c8c0`, seven scene breaks). Paragraphs are one per line,
`<hr/>` becomes two blank lines.

| Setting | Sense-word spans judged (60 random) | Sight share per scene |
|---|---|---|
| strength 3.0, margin 0.5 | 13 useful, 36 noise, 11 wrong | 63–92% |
| 4.0 / 1.0 + stoplist, names, *see/feel* rules, sight nouns 3.0 | 25 / 29 / 6 | — |
| 4.0 / 1.0, sight 3.0 (3.5/0.5 table) | 30 / 22 / 8 | 45–77% |
| shipped: 4.0 / 1.0, sight 3.0, 4.0/1.0 table | **33 / 20 / 7** | 45–77% |

Every concrete noun rates visual-dominant (*door*, *street*, *whale*, *head*), so without the sight
margin sight words are mostly objects and every scene is 75–92% sight. With it, sight keeps colour,
light, shadow and gaze words (*red*, *dim*, *shadow*, *glance*, *stare*) and loses generic actions
(*stand*, *draw*, *wave*) along with *watch* and *glitter*. Remaining noise: setting nouns that still
clear the margin (*house*, *valley*, *mainland*), named emotions, and size words (*vast*, *tall*).
Remaining errors are figurative senses the norms cannot separate: *sound* as plumbing a depth,
*green* as inexperienced, *booming* a gold find, *black distress*.

No published scene reaches the flag: of 20 scenes with ≥ 20 narration sense words, the most
one-sided is Gatsby's dawn (77% sight: "grey-turning, gold-turning light… ghostly birds began to
sing among the blue leaves"), which is plainly multi-sensory, then Moby's New Bedford street (73%).
85% therefore leaves no false flags on these texts while still flagging a scene that is all sight.

Reproduce (prints shares, scenes with ≥ 20 sense words and, with `WMB_SENSE_WORDS=1`, every sense
word in context):

```sh
cargo run -p writemorebetter-style --release --example norm_reports -- \
  bench/public/story1.txt bench/public/story2.txt bench/public/story3.txt \
  bench/public/story4.txt bench/public/story5.txt bench/public/moby150k.txt
```

## Diction

The Brysbaert, Warriner & Kuperman concreteness ratings are CC BY-NC-ND and are not vendored or used
in tests. `diction` needs a caller-supplied table; without one every entry point returns
`missingTable` (`StyleError::MissingTable { report: "diction" }`), never an empty report.

`ConcretenessTable::parse` accepts:

- **JSON:** an object of word → number, e.g. `{"lamp": 4.8, "idea": 1.6}`.
- **TSV:** `word<TAB>rating` lines, or a header row naming a `Word` column and a `Conc.M` or
  `Concreteness` column, in which case other columns are ignored (the layout of Brysbaert's
  distributed text file).

Words are trimmed and lowercased (curly apostrophes folded); ratings must be finite; an empty table
is `invalidTable`. Defaults assume a 1 (abstract) – 5 (concrete) scale; set `abstractBelow` for other
scales. The table is parsed on every call.

Each row of `sentences` has `span`, `mode`, `words`, `ratedWords` and `mean` (null below
`minimumRatedWords`, 3), and `abstract` (mean < `abstractBelow`, 2.5). `paragraphs` give the same
mean over their sentences' rated words plus their range in `sentences`. `groups` hold sentence,
rated-word and abstract-sentence counts and the mean over all rated words. `abstractRuns` are
`SentenceRun`s of at least `minimumRunSentences` (3) consecutive abstract **narration** sentences:
sentences without a mean neither extend nor break a run, while a rated concrete sentence, a sentence
opening in or containing speech, a scene break and the document end close it.

### Usefulness without a real table

Judged with a proxy derived locally from the Lancaster norms (1 + 0.8 × maximum perceptual
strength), never committed. The proxy is compressed toward the concrete end (sentence median 3.79,
p10 3.39), so the 2.5 default flags nothing on it; the review used `abstractBelow: 3.39`.

| Flag | Count | Useful | Noise | Wrong |
|---|---:|---:|---:|---:|
| Abstract runs (all texts) | 2 | 1 | 1 | 0 |
| Abstract sentences (20 sampled of 177) | 20 | 3 | 15 | 2 |

The useful run is London's summary of Buck ("the dignity that comes of good living and universal
respect… a sated aristocrat"); the noise run is Ishmael's comic rumination about sharing a bed with
the harpooneer. Single abstract sentences are mostly correct but unremarkable, so runs are the flag.
Real-world usefulness depends on the table Subplot supplies, which is an owner decision.
