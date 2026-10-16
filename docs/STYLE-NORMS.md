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
Linux), sensory balance takes 15 ms after a one-time 3.0 ms table load, and diction 11 ms plus 11 ms
to parse a 36,800-word TSV table the first time it is passed. `StyleDocument::diction` keeps the
last parsed table and reuses it while callers pass the same text, so a lens refresh with an
unchanged table skips the parse (the bindings still copy the text across the boundary). The cache
is process-global: it retains the last table’s full source text and parsed map until replacement or
process exit. Calls compare and replace that slot under a mutex; parsing happens outside the lock, so
concurrent cache misses can parse independently (including duplicate parses of the same input). A
retained table handle across the JS/Swift boundary remains deferred; the current API still copies text.

## Word lookup

Both reports look up every counted word (skipped names excluded) by lemma, using the document's
readings. The POS family is chosen as echoes chooses it (chunk and `-ing` disambiguation, falling
back to every content family) and the token's lemmas in that family are looked up. When the family
has a verb reading whose lemma differs from the surface form, only those lemmas are candidates: past
*felt* is *feel*, not the verb *to felt*, and *saw* in a verb chunk is *see* (in a noun phrase,
*the felt* and *the saw* keep their own lemmas). When exactly one candidate is in the table it is
used, when several are the one equal to the surface form is used, and when none is the lowercase
surface form is tried, unless it is itself the lemma of a rejected reading. Regular `-s`, `-ed` and
`-ing` forms (including a dropped final `e`) may fall back even when the surface has its own reading.
Sensory lookup treats a rating below the selected strength or margin as absent before making that
fallback; diction uses every rating. Thus *creaking* keeps its stronger surface rating when *creak*
is too weak, while past *felt* and *saw* keep their verb lemmas and never fall back to the homographs. Tokens without readings
use the surface form. A hyphenated compound with no entry of its own (*red-hot*, *sweet-smelling*,
*dance-music*) is looked up by its last part, then its first, and keeps the whole token’s span. Compounds ending in
`-eyed`, `-looking`, `-tinted` or `-clothed` use only their first part (*red-eyed*, *amber-tinted*,
*blue-clothed*); `-blooded`, `-hearted`, `-heartedness` and `-outs` have no part fallback. These
suffix guards reject *eagle-eyed*, *dubious-looking*, *cold-blooded*, *hard-heartedness* and
*look-outs*. Direct whole-word entries still take precedence; other lexicalized compounds can still
be misread, and *half-muffled* remains context-dependent (wrapped up versus a muted sound).
Excluded: echoes' function-word list, *say/ask/tell/talk/speak/reply/answer*, tokens whose readings
are all proper nouns, and capitalized words that do not open a sentence (after an opening quote,
bracket, dash or colon counts as opening). The last rule keeps character names such as *Daisy* out of
the counts when no skip list is supplied; supply `skipNames` to also drop sentence-initial names.

Table membership and sensory thresholds can resolve regular inflections differently. If a verb
lemma has an eligible rating it wins; only an absent or ineligible lemma permits the surface
fallback. This preserves the homograph fix without throwing away highly rated participles.

## Sensory balance

`style/data/norms/lancaster-perceptual.tsv` is derived from the Lancaster Sensorimotor Norms by
`scripts/build-lancaster-norms.py`. Each line is `word`, dominant modality letter, its mean strength
and its lead over the second-strongest modality, both in floored tenths (0–50). For a haptic word the lead
is measured over the strongest modality other than interoception: touch and interoception co-rate
on temperature and bodily-contact words (*cold* is haptic 4.11, interoceptive 3.53), so the plain
lead dropped *cold*, *heat* and *warmth* although no other sense competes. Single lowercase words
whose dominant modality rates ≥ 3.5 and leads by ≥ 0.5 ship. The #282 build has 11,456 words, 193,102 bytes (66,055 with `gzip -9`).
Decimal arithmetic preserves exact CSV boundaries; flooring tenths prevents rounding a word below
4.0/1.0 into the defaults. Custom thresholds between tenths are conservative by less than 0.1; the
public TSV format stays unchanged. The table is looser than the default options so that options can loosen as well as
tighten; values below 3.5/0.5 select nothing more. `PerceptualNorms::from_tsv` loads a caller table
in the same format (a leading UTF-8 BOM is ignored).

Modalities map Visual → `sight`, Auditory → `sound`, Haptic → `touch`, Olfactory → `smell`,
Gustatory → `taste`, Interoceptive → `interoception`. Interoception in these norms includes named
emotions (*fear*, *joy*, *love*) as well as bodily states (*ache*, *tired*).

A **sense word** is a looked-up word whose dominant modality has strength ≥ `minimumStrength` (4.0)
and leads the next modality by ≥ `dominanceMargin` (1.0); sight words must lead by ≥ `sightMargin`
(3.0). Lemmas in `style/data/norms/not-sensory.txt` (our own list of generic, speech-act and
reading words the norms rate perceptual, such as *way*, *up*, *story*, *tale*, *read*, *explain*,
the emotion noun *feeling*, and *pose* from dialect *s’pose*) never count. Speech verbs identified as dialogue attributions by the shared `dialogue_tags`
report also never count, even with a caller table that rates them strongly; a literal sound elsewhere
(*the thunder grumbled*) remains eligible. Neither do these uses,
judged on the next word or two when only whitespace separates them (a comma or other punctuation
breaks the link; double spaces and hard-wrapped line breaks do not):

- *see* and *feel* followed by a clause or comparison (*that*, *if*, *how*, *like*, *as*, …), and
  *feel* followed by a predicative adjective or past participle (*felt ashamed*, *felt obliged*); an
  immediately following noun preserves literal touch (*felt broken glass*, *felt warm breath*,
  *felt smooth stone*, *felt sharp pain*);
- *look* followed by *like*, *as if* or *as though*, and *sound* followed by *as if* or *as though*
  (appearance, not sight); these predicates and *feel* can look past up to two adverbs
  (*looks just as if*, *looked almost like*, *felt unduly concerned*);
- *hear* followed by *that*, *of*, *about* or *from* (hearsay);
- a flat adverb (an adjective spelling with its own adverb reading, such as *pretty* or *hard*)
  used as an intensifier before an adjective or adverb outside a noun phrase (*pretty soon*,
  *pretty cold*), or heading an adverb phrase not after *be* (*tried hard*; *the ground was hard*
  still counts).

With the default options *warm* (3.7), *crimson* (3.75) and *scarlet* (3.6) are in the table but
below `minimumStrength`; `minimumStrength: 3.5` counts them, at the cost of generic words such as
*turn*, *wake*, *forget* and *dream* (see the calibration below).

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

#### Re-review (#281)

An independent review sampled three other Standard Ebooks chapters: Crane, *The Red Badge of
Courage* ch. 1 (`stephen-crane_the-red-badge-of-courage` `ebe3876c`), Wharton, *Ethan Frome* ch. 1
(`edith-wharton_ethan-frome` `480cd944`) and Fitzgerald, *The Great Gatsby* ch. 1 (`fe47c8c0`).
It found 24 useful, 22 noise and 14 wrong sense words in 60, mostly past *felt* read as the fabric,
hearsay *heard*, *looked as if*, *read*, *tales*, *pretty soon* and *s’pose*. The fixes above were
judged by one judge, blind to which build each span came from, on 60 spans drawn with seed 281 from
each three-chapter set:

| Sample (60 sense words) | Before #281 fixes | After |
|---|---|---|
| Crane, Wharton, Gatsby ch. 1 | 14 useful, 26 noise, 20 wrong | **23 / 30 / 7** |
| Chopin, London, Gatsby ch. 8 | 21 / 31 / 8 | **24 / 28 / 8** |

This judge is stricter than the earlier rows: filter verbs (*looked at*, *heard*, *saw her*),
stage business (*laughed*, *nodded*) and epithets (*the tall soldier*) count as noise. A sample of 60
moves with any change to the pool, so every changed span in the six chapters was also judged. Of
963 sense words, 53 were dropped (42 wrong, 7 noise, 4 useful: *hurting*, *breathing quickly*,
*flew*, *withered*) and 46 added (21 useful, such as *cold* surface, *colder*, *amber-tinted*,
*snow-white*, *dance-music*; 17 noise; 8 wrong, such as *cold cash*, *cold judgment*, *eagle-eyed*,
*half-muffled* as wrapped up). Recall on the opening three paragraphs of *Ethan Frome* (24
hand-marked sensory words) rose from 13 to 15 (*cold* twice). *snow* (sight 4.10, touch 3.62) and
*windy* (touch 3.76, sound 3.59) stay out: each splits two senses, so the margins reject them by
design.

Remaining F3 limitations: *Do you see?* can mean understand, and a bare adverb (*felt dreadfully*)
does not establish an adjective complement. Longer modifier chains, noun-reading ambiguity and
figurative adjectives (*cold judgment*, *cloudy meanings*) still need semantic context.

Perception verbs still make up a large share: *look*, *see*, *hear* and *feel* are counted when
literal, and editors usually treat them as filter words. Reporting them separately is a follow-up.

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
is `invalidTable`. A leading UTF-8 BOM (common in tables saved from Excel) is ignored. Defaults
assume a 1 (abstract) – 5 (concrete) scale; set `abstractBelow` for other scales.

Each row of `sentences` has `span`, `mode`, `words`, `ratedWords` and `mean` (null below
`minimumRatedWords`, 3), and `abstract` (mean < `abstractBelow`, 2.5, in a narration sentence that
neither opens in nor contains speech; dialogue keeps its `mean` but is never `abstract`, because
speech is naturally abstract and would otherwise dominate the highlights). `paragraphs` give the same
mean over their sentences' rated words plus their range in `sentences`. `groups` hold sentence,
rated-word and abstract-sentence counts and the mean over all rated words (the dialogue group's
abstract count is therefore 0). `abstractRuns` are
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

The #281 re-review sampled 20 abstract sentences from the Crane, Wharton and Gatsby ch. 1 chapters
with the same proxy and threshold. Before the fixes, 9 of its 10 noise items were dialogue ("Let’s go
back, Tom."). Same judge, seed 281, before and after:

| Flag | Before #281 fixes | After |
|---|---|---|
| Abstract sentences in those chapters | 68 | 40 |
| 20 sampled: useful / noise / wrong | 7 / 11 / 2 | **15 / 3 / 2** |
| Abstract runs (all texts): useful / noise | 2 / 1 | 2 / 1 |

The useful runs are London's Buck summary and Crane's "He tried to mathematically prove to himself
that he would not run from a battle…" interior block (five sentences after the fixes: its opening
"He lay in his bunk" now rates *lay* as the verb *lie*, which the proxy rates concrete). The
remaining wrong items are concrete sentences with few rated words ("Four others stood waiting.").
Real-world usefulness depends on the table Subplot supplies, which is an owner decision.
