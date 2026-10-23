# Person drift

#354 adds `personDrift`, an opt-in lens that reads narration only. It gives each scene a
grammatical person and flags isolated slips:

- **`firstInThird`**: a singular *I*, *me*, *my*, *mine* or *myself* in a confidently
  third-person scene (*Valancy paused on the porch. I knocked twice.*);
- **`thirdInFirst`**: the declared narrator's *she* or *he* in a confidently first-person scene
  (*I crossed the room. She picked up the knife. I waited.*);
- **`narratorName`**: the declared narrator's name as a subject or possessive in a first-person
  scene (*Anna picked up the knife.*).

It diagnoses only and never proposes replacement text. It is part of the narrative-consistency
work tracked in #352 and builds on the voice layer from #353 ([STYLE-VOICES.md](STYLE-VOICES.md)).
The lens is threshold rules over counts plus local checks. It uses no pronoun feature vector and no
trained model.

## Options

`PersonDriftOptions` (Rust builders `new()`, `with_person`, `with_scene_persons`, `with_narrator`,
`with_cast`, `with_minimum_subjects`, `with_maximum_slips`, `with_include_witness_scenes`). JSON
keys are camelCase; the snake_case field names are accepted too, as with `DocumentOptions`.
Unknown keys and unknown persons or pronouns are errors.

```json
{"person": "first",
 "scenePersons": [{"scene": 2, "person": "third"}],
 "narrator": {"names": ["Anna", "Anna Reyes"], "pronoun": "she"},
 "cast": [{"name": "Mara", "pronoun": "she"}],
 "minimumSubjects": 8, "maximumSlips": 2, "includeWitnessScenes": false}
```

| Option | Default | Meaning |
|---|---|---|
| `person` | none | The document's person (`first`, `second`, `third`), applied to every scene. |
| `scenePersons` | `[]` | Per-scene declarations; `scene` is the index in the report's `scenes`. An entry beats `person`, and both beat inference. An index at or past the scene count is an options error. |
| `narrator` | none | `names` and `pronoun` (`she`, `he`, `they`) of the first-person narrator. Without it, first-person scenes raise nothing. A `they` narrator gets name flags only. |
| `cast` | `[]` | Other characters' names and pronouns. A same-pronoun cast member mentioned in a scene blocks `thirdInFirst` there; a name shared with the narrator's blocks `narratorName`. |
| `minimumSubjects` | 8 | Person-bearing subjects a scene needs before its person is inferred. Must be at least 1. |
| `maximumSlips` | 2 | Candidate sentences a third-person scene may hold; more means a voice choice, and the scene is suppressed. |
| `includeWitnessScenes` | false | Flags inferred third-person scenes in documents that also have a first-person scene. |

An empty narrator or cast name is an options error. A compact document (no token details) gives
`needsTokenDetails`.

## Result

```json
{"scenes": [{"scene": 0, "span": {"start": 0, "end": 812},
             "paragraphs": {"start": 0, "end": 6}, "sentences": {"start": 0, "end": 31},
             "person": "third", "source": "inferred", "tense": "past",
             "subjects": {"first": 1, "second": 0, "third": 19},
             "narrationWords": 640, "suppressed": null}],
 "drifts": [{"span": {"start": 402, "end": 403}, "sentenceSpan": {"start": 380, "end": 431},
             "sentence": 14, "scene": 0, "kind": "firstInThird", "word": "i",
             "expected": "third"}],
 "narrationWords": 640, "perThousandWords": 1.56}
```

Spans are `{start, end}` in the binding's unit: UTF-16 for WASM and Swift, code points for
Python, and all three units in Rust's `Span`. `source` is `declared`, `inferred`, `mixed` (enough
subjects, but in the dead band) or `undetermined` (too few subjects). `suppressed` says why a
first- or third-person scene raised nothing: `witnessNarrator`, `tooManySlips`, `noNarrator` or
`undeterminedTense`. `word` is the folded surface (`i`, `my`, `she`) or the declared narrator
name that matched. Drifts are sorted by span. `perThousandWords` is drifts per 1,000 narration
words, and is null when there are none.

Surfaces: `writemorebetter_style::reports::person_drift::report`, `StyleDocument::person_drift`,
WASM `personDrift(optionsJSON?)`, Swift `personDriftJson(optionsJson:)` and the typed
`personDrift(options:)`, Python `StyleDocument.person_drift(options)`.

## How a scene gets its person

1. **Scenes** come from the shared `narrative::scenes`: a new one at every scene break or
   chapter start. Nothing carries across scenes except the witness guard (step 4).
2. **Subjects.** In every narration run (`Document::narration_runs`), `narrative::chains` finds
   the finite verb chains, and `narrative::pronoun_person` classifies each chain's subject. *I* and
   *we* count as first, *you* as second, *he* and *she* as third. *they*, *it*, *one*, objects and
   possessives never count. Speech, thought, quoted terms and caller-marked spans are not narration,
   so a letter the caller marks `embedded` never counts.
3. **Person.** A declaration wins. Otherwise, with *n* = first + second + third:
   - *n* < `minimumSubjects`: undetermined;
   - second ≥ *n*/2 and second ≥ 4: second person (reported, never flagged);
   - first ≥ 3, first ≥ ¼ (first + third), and not every first-person subject is *we*: first;
   - third ≥ `minimumSubjects`, first ≤ `maximumSlips` and first ≤ third / 10: third;
   - otherwise mixed (never flagged).
4. **Witness guard.** If any scene in the document is first person, every *inferred*
   third-person scene is suppressed (`witnessNarrator`): a first-person narrator telling another
   character's story uses *I* on purpose. Declared third-person scenes are always checked.
5. **Tense.** `narrative::scene_tense` gives the scene's narrative tense. It counts past and
   past-perfect against present narration chains and needs at least 3 of them with a 70%
   majority. A third-person scene without a tense is suppressed (`undeterminedTense`), because the
   tense check below cannot run.

## What is flagged, and what is not

**`firstInThird`** looks at narration tokens outside voice tags in third-person scenes: *I*,
*me*, *my*, *mine*, *myself*. *we*, *us* and *our* are never flagged (authorial *we*), and neither
are Roman-numeral or initial *I* (*Henry I*, *I.O.O.F.*) or the noun *mine*. Each sentence gives at
most one drift. If more than `maximumSlips` sentences in the scene have candidates, none are
flagged (`tooManySlips`). A sentence is flagged only when every check passes:

| | Exemption |
|---|---|
| E1 | Not narration, or inside a thought or speech tag frame (*I’m leaving, he said.*). |
| E2 | A question, exclamation or trailing-off sentence: any `?` or `!`, or an ending in `…`, `...`, `—` or `:`, including a clause the splitter left joined (*I crossed the room— He was gone.*). |
| E3 | Tense disagreement: the sentence needs at least one narration chain, and every chain must match the scene tense. In a past scene only past and past-perfect chains qualify; any present, modal or ambiguous chain exempts the sentence. This covers unmarked thought (*I can’t do this.*) and authorial hedges (*I suppose*, *as I have said*). |
| E4 | Inside a dash pair, parentheses or brackets, or after a single dash that runs to the end of the sentence. |
| E5 | An interjection: *my God / Lord / goodness / word / dear / heavens / gosh*, *oh my*, *dear me*, *excuse me*, *God help me*, *bless me*. |
| E6 | After a writing or display colon (*The note read:*, *She wrote:*) in the same paragraph (`narrative::display_colon_lead_in`). |
| E7 | A dash-dialogue paragraph, or a line of six words or fewer with no verb chain (a salutation or signature). |
| E8 | A paragraph with an unpaired quote (`narrative::paragraph_quotes_balanced`). |

**`narratorName`** (first-person scenes, needs `narrator.names`) matches a declared name in
narration as a chain subject or with *’s*/*'s*/*’*. Matching is case-sensitive, folds curly
apostrophes and whitespace, and covers names in `skipNames`. A match is exempt after *named*,
*called*, *name*, *signed*, *wrote* or *Dear*, in *my name is/was*, as an appositive after *I*
(*I, Anna,*), after an honorific when the honorific form is not itself a listed name, and when a
word of it is shared with a cast member. E1–E4, E7 and E8 apply.

**`thirdInFirst`** (first-person scenes, needs `narrator.pronoun` *she* or *he*) looks at chain
subjects that match the narrator's pronoun. It needs every one of:

| | Condition |
|---|---|
| A1 | At most `maximumSlips` narration subjects with that pronoun in the scene: more is a character. |
| A2 | No same-pronoun cast member mentioned anywhere in the scene. |
| A3 | No same-gender pronoun in the scene's speech, quoted text or thought. |
| A4 | No gendered noun of that gender earlier in the scene (*woman*, *mother*, *queen*; for *she* also *ship*, *boat*, *vessel*, *car*). |
| A5 | No capitalized mid-sentence proper name earlier in the scene other than the narrator's or a different-pronoun cast member's. |
| A6 | The sandwich: the previous or next sentence in the paragraph has a first-person subject. |
| A7 | No self-distancing frame in the paragraph (*the girl I had been*, *my younger self*, *that boy*). |

E1–E4, E7 and E8 apply too. When a sentence has both a name and a pronoun hit, the name wins.
Without a declared narrator, a first-person scene that has *she*/*he* subjects reports
`noNarrator`.

The gendered-noun, interjection and self-distancing lists are English and live in the English lens
module (MIT OR Apache-2.0). The lens loads no data file.

## Evaluation (2026-10-22)

`python3 bench/style/person_drift.py score --gate` (seed 354). All text is public: the CC0
`bench/public` stories and Standard Ebooks (CC0) books. Chapters are scored one document each,
with blockquoted paragraphs passed as caller `embedded` spans. Nothing from `bench/corpus` is
read. Recorded in `bench/style/person-drift-baseline.json`.

**Scene inference** at default options, over 437 scenes in 13 first-person and 5 third-person
sources. Of the scenes it decided, 330 were right and 23 wrong (accuracy 0.935), and it abstained
(undetermined or mixed) on 84 (0.192). Every wrong scene is in *Moby-Dick*. Its Ahab chapters
really are third person, but they are scored against the book-level label. Excluding *Moby-Dick*,
every decided scene is right. Abstention is highest in *Moby-Dick* (0.40), *Silas Marner* (0.33)
and *The Hound of the Baskervilles* (0.25).

**Perturbations** (`labels/person-drift-perturbations.jsonl`: offsets and replacement strings
only):

| Set | What changed | Recall |
|---|---|---|
| P-1in3, undeclared | a sentence-initial *She*/*He* (the scene's main pronoun) before a past verb → *I*, in a third-person book | 239/300 = 0.797 |
| P-1in3, declared `person: third` | the same | 242/300 = 0.807 |
| P-3in1 (a) | *I* → the narrator's pronoun, in a window of up to eight paragraphs with no same-gender pronoun, quote or unknown name | 12/25 = 0.48 |
| P-3in1 (b) | the same in the full chapter, where same-gender characters are present | 0/150 = 0.00 |
| P-name | *I* → the narrator's name | 249/300 = 0.83 |

Precision over every flag on the perturbed copies: 742/746 = 0.995. The four false ones are
natural flags in the same chapters. There were 154 traps, each run with default and declared
options (308 runs): a tagged thought, tagged unquoted speech, *God, I was a fool!*, a colon note,
an em-dash aside, a present *I can’t do this.*, and *The man/woman by the window* before a
narrator-pronoun sentence. They raised **0** false positives.

Most P-1in3 misses are E3 by design: the sentence also has a modal or present chain (*I
understood that his own eyes **must** be unbearable*). Most P-name misses are a modal elsewhere in
the sentence or a chapter whose tense is undetermined. P-3in1 (a) has only 25 items because public
first-person fiction rarely runs eight paragraphs without a same-gender pronoun. P-3in1 (b) is zero
by design: A1, A4 and A5 stop it whenever another man or woman is in the scene.

**Natural flags** at default options: 2 in 539,614 narration words (0.0037 per 1,000), against a
gate of 0.05. With declared persons and narrators: 5 (0.0093). On 15 more CC0 novels in
`sources.json` (1,339,803 narration words, the modernist and later sets voices uses): 0. Two blind
labelers from different model families judged all 7 natural flags:

- Codex `gpt-6-sol`, OpenAI, Codex CLI 0.163.0;
- DeepSeek V4 Pro, OpenCode 2.1.2.

They agreed on 6 of 7. Kappa is 0.0, which is degenerate when almost every label is `false`. The
implementer adjudicated the seventh as `false`. **0 of 7 natural flags are slips.** Two are
Ishmael's *my Queequeg* in chapters the lens infers as third person; one is George Eliot's
authorial *I once said*. Three are in *Treasure Island* chapters narrated by Dr. Livesey, flagged
only because the book-level declaration names Jim. The fourth is Jim reporting the wording of a
summons (*word was sent forward that Jim Hawkins was wanted in the cabin*): DeepSeek called it a
slip and Codex did not. Published, edited fiction has almost no true person
slips, so natural precision says little about drafts. The gate keeps natural noise near zero
instead.

**Gate** (CI): perturbed precision ≥ 0.90, trap false positives = 0, natural flags ≤ 0.05 per
1,000 narration words pooled at default options, and recall at or above the baseline per set.

**Cost**: O(tokens) plus antecedent scans bounded by the scene; no data loaded, so no RSS change.
WASM +30.2 KB gzipped (see CHANGELOG).

## Limitations

- **Unmarked scene changes look exactly like drift.** A POV switch without a scene break puts
  both persons in one scene. The slip cap and the dead band make that scene mixed or suppressed
  rather than flagged, and the sandwich rule limits `thirdInFirst`.
- **Witness and frame narrators** cost recall: a third-person chapter in a book with a
  first-person frame is suppressed unless it is declared or `includeWitnessScenes` is set.
- **`thirdInFirst` recall is low by design**: it needs a declared narrator, the sandwich and no
  possible antecedent.
- **Present-tense third-person scenes**: E3 cannot tell matching present-tense unmarked thought
  from a slip. No public present-tense third-person book was available to measure it; declare the
  person for such scenes and expect lower precision.
- **Perturbation recall overstates real recall**: a real slip often comes with other edits.
- **Declarations are by scene index** (`scenePersons`), which shifts when scene breaks are added
  or removed. See the open question in #354.
- The scene tense comes from `narrative::scene_tense`, which #355 is meant to own. Its thresholds
  (3 chains, a 70% majority) are provisional and live in `SceneTenseThresholds::default()`.
  Changing them changes E3 here.

## Patent distinction

US 10,191,975 (Eisenberg & Finlayson) describes classifying a text's narrative point of view by
stripping quoted text from the whole text, building a feature vector of the counts of 21 pronouns,
and running a trained classifier. This lens does not: it has no whole-text strip-and-classify
step, no pronoun feature vector and no trained classifier. It uses threshold rules on per-scene
counts of finite-chain subjects, followed by local sentence checks. This is an engineering
distinction recorded for review, **not legal advice**. Whether the owner wants a legal read before
merging is an open question on #354.
