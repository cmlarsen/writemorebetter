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
For single words, sensory lookup treats a rating below the selected strength as absent before making
that fallback; diction uses every rating. Thus *creaking* keeps its stronger surface rating when *creak*
is too weak, while past *felt* and *saw* keep their verb lemmas and never fall back to the homographs.
When two verb lemmas are rated and neither is the surface form, sensory lookup takes the one that
passes every threshold (*singing* is *sing*, not *singe*).
Tokens without readings use the surface form. A hyphenated compound with no entry of its own (*red-hot*, *sweet-smelling*,
*dance-music*) is looked up by its last part, then its first, and keeps the whole token’s span. Compounds ending in
`-eyed`, `-looking`, `-tinted` or `-clothed` use only their first part (*red-eyed*, *amber-tinted*,
*blue-clothed*); `-blooded`, `-footed`, `-handed`, `-headed`, `-hearted`, `-heartedness`, `-minded`,
`-mindedness`, `-mouthed`, `-nosed`, `-outs` and `-tempered` have no part fallback. These suffix
guards reject *eagle-eyed*, *dubious-looking*, *cold-blooded*, *hard-heartedness*, *look-outs*,
*hot-tempered*, *loud-mouthed*, *hard-nosed*, *light-footed* and *red-handed*. Direct whole-word entries still take precedence; other lexicalized compounds can still
be misread. Compound part selection uses table membership before applying the thresholds, so an
ineligible last part never makes a misleading first part win (*hard-boiled*, *looking-glass*).
*half-muffled* remains context-dependent (wrapped up versus a muted sound).
Excluded: echoes' function-word list, *say/ask/tell/talk/speak/reply/answer*, tokens whose readings
are all proper nouns, and capitalized words that do not open a sentence (after an opening quote,
bracket, dash or colon counts as opening). The last rule keeps character names such as *Daisy* out of
the counts when no skip list is supplied; supply `skipNames` to also drop sentence-initial names.

Table membership and sensory thresholds can resolve regular inflections differently. A verb lemma
that reaches `minimumStrength` wins even when it then fails a margin; only an absent lemma or one
below `minimumStrength` permits the surface fallback. This preserves the homograph fix without
throwing away highly rated participles, and keeps inflections from bypassing the margins their verb
fails: *watch*, *hold*, *tire* and *picture* pass strength but fail a margin, so *watched*,
*holding*, *tired* and *pictured* do not count through their own, more one-sided ratings.

## Sensory balance

`style/data/norms/lancaster-perceptual.tsv` is derived from the Lancaster Sensorimotor Norms by
`scripts/build-lancaster-norms.py`. Each line is `word`, dominant modality letter, its mean strength
and its lead over the second-strongest modality, both in floored tenths (0–50). For a haptic word the lead
is measured over the strongest modality other than interoception: touch and interoception co-rate
on temperature and bodily-contact words (*cold* is haptic 4.11, interoceptive 3.53), so the plain
lead dropped *cold*, *heat* and *warmth* although no other sense competes. Single lowercase words
whose dominant modality rates ≥ 3.5 and leads by ≥ 0.5 ship. The #282 build has 11,456 words,
193,102 bytes (66,055 with `gzip -9`).
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
the emotion noun *feeling*, and *pose* from dialect *s’pose*) never count. Speech verbs identified as
dialogue attributions by the shared `dialogue_tags`
report also never count, even with a caller table that rates them strongly; a literal sound elsewhere
(*the thunder grumbled*) remains eligible. Neither do these uses,
judged on the next word or two when only whitespace separates them (a comma or other punctuation
breaks the link; double spaces and hard-wrapped line breaks do not):

- *see* and *feel* followed by a clause or comparison (*that*, *if*, *how*, *like*, *as*, …), and
  *feel* followed by a predicative adjective or past participle (*felt ashamed*, *felt obliged*); an
  immediately following noun preserves literal touch (*felt broken glass*, *felt warm breath*,
  *felt smooth stone*, *felt sharp pain*). A word with a preposition reading is never that
  adjective (*felt over the table*, *felt along the wall*, *felt under his feet* count). The
  following word is not an object when it has a pronoun or adverb reading (*felt sure someone*,
  *felt certain nothing*, *felt ashamed yesterday*, *feel far away*), is capitalized (*felt glad
  Mother*) or follows an adjective phrase;
- *look* followed by *like*, *as if* or *as though*, and *sound* followed by *as if* or *as though*
  (appearance, not sight); these predicates and *feel* can look past up to two adverbs
  (*looks just as if*, *looked almost like*, *felt unduly concerned*). For *feel* an adverb that
  also reads as an adjective is tested as the predicate, not skipped (*felt alone in space*, *felt
  less alone*);
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
headings. No unit is flagged: the `singleSense` flag was removed in #282 (see
[below](#singlesense-removed)), and the per-scene counts and percents are the lens.

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

In the initial calibration, no published scene reached the flag: of 20 scenes with ≥ 20 narration sense words, the most
one-sided is Gatsby's dawn (77% sight: "grey-turning, gold-turning light… ghostly birds began to
sing among the blue leaves"), which is plainly multi-sensory, then Moby's New Bedford street (73%).
85% therefore left no false flags on that sample while still flagging a synthetic all-sight scene.
The expanded #282 measurement below found no useful flag at any threshold, and the flag was removed.

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
does not establish an adjective complement. Longer modifier chains, noun-reading ambiguity, abstract nominal complements (*feeling vague relief*)
and figurative adjectives (*cold judgment*, *cloudy meanings*) still need semantic context.

Perception verbs still make up a large share: *look*, *see*, *hear* and *feel* are counted when
literal, and editors usually treat them as filter words. Reporting them separately is a follow-up.

Reproduce (prints shares, scenes with ≥ 20 sense words and, with `WMB_SENSE_WORDS=1`, every sense
word in context):

```sh
cargo run -p writemorebetter-style --release --example norm_reports -- \
  bench/public/story1.txt bench/public/story2.txt bench/public/story3.txt \
  bench/public/story4.txt bench/public/story5.txt bench/public/moby150k.txt
```

### Follow-up measurement (#282, 2026-10-16)

The before build is `d0fe086` (the merged #281 behavior); its nine-text total reproduces the
re-review’s **1,588** sense words exactly. After the changes it is **1,624**. The nine texts are the
six `bench/public` files and the same Crane, Wharton and Gatsby chapter 1 files used by the re-review.
No private text or Brysbaert data was accessed.

**N1 policy:** keep the verb lemma when it has an eligible rating; otherwise permit the surface
fallback, with the rejected-homograph protection above. On those nine texts this restores 13
candidate hits: *creaking* +1, *ticking* +2, *knocking* +1, *pricking* +1, *dimmed* +1, *withered* +1,
*flew* +5 and *breathing* +1. Reading all 13 in context: **10 useful, 2 noise, 1 wrong**. The wrong
one is a pricking conscience; knocking hats off and the withered old man are stage business/an
epithet. This recovers the reported imagery without preferring every surface over an eligible
base verb. As first shipped, the fallback also fired when the lemma failed only a margin, adding
*holding* ×10, *watching* ×8 and *watched* ×8 on these texts; the #286 review fixes below limit it to
lemmas below `minimumStrength`. *reflected* (two cognitive uses) and *waking* still fall back because
*reflect* and *wake* are below strength. Separate filter-verb reporting remains deferred.

**N2 thresholds:** the exact-decimal, floored build changes the raw default-qualifying set from
2,069 to 2,034 entries: **35 subthreshold entries removed**, no additions to that set. The review’s
46-word comparison also included deliberate haptic additions and floating-point boundary artifacts.
In the CSV *excited* and *pudding* have an exact 1.0 lead, so they correctly remain eligible; the
invented-data build test checks this boundary. Exact 0.5 build boundaries retain six additional
rows (11,450 → 11,456). Flooring preserves the integer-tenths TSV contract; arbitrary thresholds
between tenths can conservatively reject a rating less than 0.1 above the threshold.

One judge used the re-review’s criteria: sensory imagery is useful; filter verbs, stage business,
setting nouns, named emotions and epithets are noise; figurative uses, hearsay, homographs and speech
acts are wrong. Fresh samples draw 60 sense words from the three review chapters in Crane / Wharton /
Gatsby order with Python `Random(281)`. These judgments were not blind. Changing the pool changes
the sampled spans, so the complete changed-span audit is the more direct comparison.

| Review chapters | Before | After |
|---|---:|---:|
| Sense words | 549 | 552 |
| Fresh 60: useful / noise / wrong | 21 / 27 / 12 | **13 / 39 / 8** |
| Every removed span: useful / noise / wrong | **3 / 1 / 13** (17 removed) | — |
| Every added span: useful / noise / wrong | — | **4 / 13 / 3** (20 added) |
| Diction abstract sentences (same Lancaster proxy, 3.39 threshold) | 40 | 40 |
| Diction sample of 20: useful / noise / wrong | 15 / 3 / 2 | 15 / 3 / 2 |
| Diction runs across all nine texts | 2 (1 useful, 1 noise) | 2 (1 useful, 1 noise) |

The changed-span net is **+1 useful, +12 noise, −10 wrong**, not a precision win. The fresh sample’s
useful count falls; do not interpret the repairs as making this an issue list. The additions’ three
wrong hits are *feeling vague relief* and cognitive *reflected* twice. Useful losses are *drawl*
(now below the unrounded strength threshold), adjectival *a whispered “Listen”* (the shared
attribution detector reads it as a tag), and *her voice sang* introducing quoted speech. Existing
semantic ambiguity and attribution detection errors remain; *half-muffled* as wrapped up also remains
wrong. These costs are retained explicitly rather than tuning to this sample. Diction’s 20 sampled
sentences are identical before/after, as are the two runs; the proxy still does not establish
usefulness with the owner’s eventual table.

### #286 review fixes

The PR review (2026-10-16) found five follow-ups. The before build is the PR head `a2c8c0a`; the
sample is the nine texts above plus every chapter of the five pinned Standard Ebooks books listed
below (94 files, all public).

| | `a2c8c0a` | After fixes |
|---|---:|---:|
| Nine texts, sense words | 1,624 | **1,588** |
| Three review chapters | 552 | 542 |
| 94 public files | 9,722 | **9,506** (+3, −219) |

- **F1:** the surface fallback for regular inflections now applies only when the verb lemma is below
  `minimumStrength`, not when it passes strength and fails a margin. Across the 94 files this removes
  *holding* 41, *watched* 41, *watching* 33, *tired* 33, *pictured* 11, *waving* 11, *showing* 8,
  *laughing* 7 and *displayed* 6, with no additions. All 13 N1 restorations remain. The tie-break
  that picks the lemma passing every threshold keeps *singing* (8 of the 9 *singing* spans the
  strength-only filter would drop). Useful losses are *ringing* ×2 (*ring* rates as the visual
  noun), *waving* and *laughing* as stage business, and one *singing*.
- **F2:** *feel* counts literal groping and drops emotional predicates (rules above). The 94 files add
  *felt over innumerable feet of dark wall*, *ice he had felt under his feet* and *made itself felt
  by* (wrong), and drop 12: *felt alone in space*, *felt less alone*, *I feel far away*, *feel
  better* ×3, *feel pretty bad* ×2, *feel quite well*, *felt most intensely*, *feel most
  delightfully* and *feel very cold now* (a useful loss: *now* has an adverb reading). Abstract
  nominal objects (*felt instant panic*, *feeling vague relief*) still count.
- **F3:** six more compound suffixes have no part fallback (list above). The 94 files lose
  *light-footed* ×3, *soft-mindedness* and *red-handed*. Other lexicalized compounds (*fever-pitch*,
  *first-felt*, *green-eyed* with envy) can still be misread.
- **F4:** tier 2 `compounds` and `tags` now assert exact surfaces. The `tags` case uses *shouted* and
  *murmured*, which the bundled table rates ≥ 4.0, so the speech-tag detector (not the threshold)
  excludes *he shouted* while *the crowd shouted* still counts.
- **F5:** exact thresholds drop *pat* (haptic 3.968; 6 *patted* spans in the 94 files, such as
  *patted their sweating flanks*) and *luminous* (sight lead below 3.0). This is the intended result of
  making the documented 4.0 / 1.0 / sight 3.0 thresholds exact; do not restore them by rounding.

Every span changed on the nine texts was read: 36 removed, none added. Two were useful (*feel very
cold now*, *He’s singing away*), one wrong (*the engine was tired*), and the rest filter verbs,
stage business and named states.

### singleSense removed

**Owner decision (Caleb, 2026-10-16): remove the flag.** Chapters and scenes no longer carry
`singleSense`, and the `singleSensePercent` option is gone; passing it is rejected as an unknown
option (`invalidOptions`). The per-scene modality counts and percents remain the opt-in lens. The
evidence follows: on 114 eligible public scenes the flag fires on 0/1/4/11 scenes at 85/80/75/70%,
and none of them is a useful warning.

Expanded public-only sample: **243,160 words, 191 scenes, 114 eligible scenes** (≥ 20 narration sense
words). It includes `bench/public` and every chapter of five Standard Ebooks books, fetched at bench
time into ignored `bench/style/out/282/se/`. These are chapter/scene units from the document model,
not arbitrary windows; paragraphs occupy one line and `<hr/>` becomes two blank lines. Headings and
notes are omitted. This is a scene-weighted historical-fiction sample, not a modern-author baseline.

| Standard Ebooks repository | Pinned commit | Chapters |
|---|---|---:|
| `stephen-crane_the-red-badge-of-courage` | `ebe3876cf6d0d93428c247a67076dc927d03ccb8` | 24 |
| `edith-wharton_ethan-frome` | `480cd944a5951a87f342956152aa9bfa6a97ff45` | 9 |
| `f-scott-fitzgerald_the-great-gatsby` | `fe47c8c04f5175d45f9410f4e3ca64c257ae0acc` | 9 |
| `kate-chopin_the-awakening` | `49955125aa51c4d268543f96f2a1094058b4574d` | 39 |
| `jack-london_the-call-of-the-wild` | `561cc9334d5d9ea203ce9310b1d27a8b0f12cc3f` | 7 |

Sources are `https://github.com/standardebooks/<repository>/tree/<commit>/src/epub/text`.
No fetched prose is committed. Minimum top-modality share is 27.27%, median 55.56%, maximum 81.48%.

| Top-modality share among eligible scenes | Scenes |
|---|---:|
| < 50% | 42 |
| 50–<60% | 35 |
| 60–<70% | 26 |
| 70–<75% | 7 |
| 75–<80% | 3 |
| 80–<85% | 1 |
| ≥ 85% | 0 |

| Flag threshold | Scenes firing | Judged useful |
|---|---:|---:|
| 85% (unchanged default) | 0 | 0 |
| 80% | 1 | 0 |
| 75% | 4 | 0 |
| 70% | 11 | 0 |

Every scene firing at any tested threshold was read in full (the 11 scenes at 70% include all higher
threshold candidates). All were judged noise as writer-facing warnings:

| Scene (zero-based index within the chapter/file) | Top share | Why a warning is unhelpful |
|---|---:|---|
| *Ethan Frome* 8, scene 1 | 81.48% sight (22/27) | Ethan’s walk is intentionally visual; bird-call, laughter and the boy chirping coexist with light/road/house counts |
| *Gatsby* 8, scene 3 | 79.41% sight (27/34) | Dawn, singing birds, clicking footsteps and the hot day-coach; already multi-sensory |
| *Gatsby* 9, scene 5 | 78.79% sight (26/33) | Deliberate visual coda with remembered music/laughter and physical beach/sand action |
| Moby excerpt, scene 62 | 76.19% sight (16/21) | Father Mapple and the pulpit are visual descriptions; nautical “taste” is figurative and contributes noise |
| *Gatsby* 7, scene 4 | 73.33% sight (33/45) | Night vigil, rustling trees, voices, cold chicken and physical touch |
| *The Awakening* 29 | 72.73% sight (16/22) | Moving-house action and room/house/filter-verb counts, not a sensory deficiency |
| *Gatsby* 9, scene 3 | 72.09% sight (31/43) | Winter memories include cold air, bells, voices and wet laundry |
| *Gatsby* 4, scene 1 | 71.70% sight (38/53) | Car/city description with horn melody and motorcycle sound; many “see/look” tokens |
| Moby excerpt, scene 60 | 70.97% sight (22/31) | New Bedford street description includes howling and the extended musk/smell image |
| *The Awakening* 1 | 70.59% sight (36/51) | Opens with parrots, whistling, piano and high voices; visual share is not missing sound |
| *The Awakening* 12 | 70.45% sight (31/44) | Coffee/taste, sun biting the skin and ocean roar alongside purposeful colour description |

85% gives no evidence of a useful positive; lowering it produces warnings on purposeful published
scenes. An all-sight synthetic fixture still fired, which proved mechanics rather than writer value.
High sight share mostly measures setting nouns and filter verbs the norms cannot separate from
imagery, so a writer could not act on it. A better-defined signal, such as a scene with ≥ 20 sense
words and no sound or touch at all, fires on none of the eligible public scenes and would need draft
data to validate; it is a separate decision.

After the #286 review fixes, 113 scenes are eligible and the maximum is 84.62% (the same *Ethan
Frome* 8 scene, which lost one non-sight word); 0/1/5/10 scenes would reach 85/80/75/70%, all among
the scenes judged above.

Reproduce the synthetic suite with `python3 bench/style/norms.py` after building the `norm_reports`
example. For the public calibration, set `WMB_NORM_JSON` to an ignored output directory and run the
example on the six public files plus the pinned books’ chapter files. Count eligible `scenes` using
`groups.narration.senseWords >= 20`; top share is the maximum modality count divided by that count,
without rounding before threshold comparison. The before probe was built and retained before edits.

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
