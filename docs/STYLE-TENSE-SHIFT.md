# Tense shift

Issue #355 adds an **opt-in lens** that flags narration leaving its scene's tense: *She opened the
door. The room is dark. The air smells of smoke.* in a past-tense scene. It diagnoses only. It
shows the sentences and the verbs that voted, never a replacement, and it does not say a shift is
wrong. Published fiction shifts tense on purpose (historical present, a present-tense frame, the
narrator's own commentary), and on published text most flags are those (see
[Precision](#precision-on-published-fiction-2026-10-23)). It works per scene, using the scenes and
the narration mask of #353, and shares its sentence votes with person drift (#354).

| Surface | Call |
|---|---|
| Rust | `tense_shift::report(&Document, &TenseShiftOptions)` |
| Retained Rust | `document.tense_shift(options_json)` |
| WASM | `document.tenseShift(optionsJSON?)` → `TenseShiftReport` (`wasm/style-results.d.ts`), UTF-16 spans |
| Swift | `document.tenseShift(optionsJson:) -> TenseShiftReport`; raw `tenseShiftJson(optionsJson:)` |
| Python | `document.tense_shift(options)` or the `"tense_shift"` report name, code-point spans |

The report needs token readings (`StyleError::NeedsTokenDetails` on compact documents). Options are
camelCase JSON with snake_case aliases. Unknown keys and unknown tense strings are `invalidOptions`.

| Option | Default | Meaning |
|---|---|---|
| `tense` | none | The document's declared tense, `"past"` or `"present"`. Every scene uses it unless `sceneTenses` names the scene. |
| `sceneTenses` | `[]` | `[{"scene": 1, "tense": "present"}]`: per-scene declarations by index in `scenes`; they beat `tense`. An index past the last scene is `invalidOptions`. |
| `minimumSceneSentences` | 6 | Classified (past or present) sentences a scene needs before its tense is inferred. Must be positive. |
| `minimumMajorityPercent` | 70 | The share one tense needs among them. Must be above 50 and at most 100. |
| `minimumRun` | 2 | Consecutive minority-tense sentences that make a `run`. Must be at least 2. |
| `includeSingles` | `false` | Also flag one sentence (`single`) when stricter checks hold. Measured 0 / 40 slip-or-device on published fiction, so it stays off. |
| `includeReverse` | `false` | Also flag past runs in a present scene. |
| `inheritDocumentTense` | `true` | Let a short scene take the document's tense when the whole document is at least 90% one tense over at least 30 classified sentences. |

## Result

`{scenes, shifts, narrationWords, perThousandWords, exemptions}`.

- `scenes`: one row per scene (`scene`, `span`, `paragraphs`, global `sentences`), its `tense`
  (null when undetermined) and `source` (`declared`, `inferred`, `inherited`, `undetermined`), the
  sentence `votes` (`past`, `present`, `mixed`, `unclassified`) and the scene's `narrationWords`.
- `shifts`: each has `span` (from the first minority sentence's first token to the last one's last
  token), global `sentences`, `verbs` (the voting verb heads), `scene`, `expected` (the scene's
  tense), `found` and `strength` (`run` or `single`).
- `narrationWords` counts words under the same narration mask the votes use;
  `perThousandWords` is shifts per 1,000 of them (null without narration).
- `exemptions`: chains and sentences that did not vote, by reason, zero counts left out.

```json
{"scenes":[{"scene":0,"span":{"start":0,"end":812},"paragraphs":{"start":0,"end":6},
  "sentences":{"start":0,"end":22},"tense":"past","source":"inferred",
  "votes":{"past":14,"present":2,"mixed":1,"unclassified":5},"narrationWords":301}],
 "shifts":[{"span":{"start":402,"end":447},"sentences":{"start":9,"end":11},
  "verbs":[{"start":411,"end":413},{"start":431,"end":437}],"scene":0,
  "expected":"past","found":"present","strength":"run"}],
 "narrationWords":301,"perThousandWords":6.64,"exemptions":[{"reason":"subordinate","count":3}]}
```

## What votes

Only narration votes. `Document::is_narration_at` (#353) takes out every quoted span, quoted and
tagged unquoted thought, tagged unquoted speech and caller-marked spans, token by token, so a tag
inside a quote's sentence still votes (*“Fine,” she says.* is present). A paragraph whose quotes
don't balance never votes (`unbalancedQuotes`).

Each sentence's narration runs are cut into finite verb chains (`narrative::chains`), each `past`,
`pastPerfect`, `present`, `modal` or `ambiguous` with its subject. A chain votes only when it is a
main clause with an explicit subject and none of these holds:

| Exemption | Rule | Example that does not vote |
|---|---|---|
| `modal` | Any modal head, present ones included | *She can see it now.* |
| `ambiguous` | VBD/VBP forms with no third-person-singular subject to settle them; NNS/VBZ homographs outside a verb chunk | *They put it down.* |
| `noSubject` | Imperatives, inversions, subjectless coordinated verbs | *Read the sign. Here comes the bus.* |
| `subordinate` | A subordinator between the clause start and the subject (B-SBAR, WDT/WP/WRB, *that if whether because while as though although since unless until when where than like once*), a relative after a noun, *, which …*, *as if / as though*, *the way*, a contact relative | *the house where she lives*, *the way you learn a scar* |
| `reported` | Within one clause of a speech, cognition or thought verb, or *see hear notice learn tell explain promise warn*, with an optional *that/if/whether*; text after a display colon (*said read wrote thought ran stated announced*, *sign note letter notice label message text card inscription headline*) | *She said the train leaves at six.* *The sign read: Rooms To Let.* |
| `generic` | Present chains only: *you*, *one*, indefinite pronouns, bare plural or mass subjects, *a/an* + noun, a demonstrative pronoun before a copula, expletive *it* + copula + complement + *to/that/how* | *People are strange.* *A man needs a job.* |
| `habitual` | Present chains only: *always never often usually sometimes seldom rarely ever*, *every/each* + time noun, *whenever* | *He goes every Sunday.* |
| `narratorFrame` | Present chains with *I/we* and a frame verb (*remember recall think know suppose wonder believe guess imagine admit confess mean hope doubt suspect say tell swear grant expect*, plus *recollect picture fear trust own fancy assure beg declare acknowledge regret venture presume*), a first-person present perfect, *I am glad/sorry/afraid/thankful/sure/…*; or *nowadays*, *to this day*, *these days*, *at present* in the sentence | *I remember the summer we met.* *I have known a lot of losses.* *Nowadays we have revues.* |
| `questionOrExclamation` | The last non-quote punctuation is `?` or `!`: every chain is neutral | *Where is he? Why is it always me!* |
| `mixedSentence` | Past and present votes in one sentence: counted, never flagged | *I told him he could visit, but June is a liar.* |
| `pastPerfect` | Counts as past for inference, never flagged, and never a minority signal in a present scene | *She’d gone before.* |

Deictics (*now, today, tomorrow, here, ago*) never vote. A sentence is **past** with at least one
past or past-perfect vote and no present vote, **present** the other way round, **mixed** with
both, and **unclassified** with none.

Two rules differ from the issue text, from the development labels and the perturbation suite:

- *Generic* and *habitual* exempt present chains only. A past chain with an indefinite subject or a
  habitual marker is narration (*A package came.*, *She always walked home.*) and votes past.
  Exempting it left past scenes with holes, so runs spanned past sentences they should have ended
  at.
- The narrator-frame extensions above. *now*, *still*, *feel*, *find* and *seem* were tried and
  dropped: they cost 9 points of synthetic run recall (0.834 → 0.744) for no slip-or-device
  precision.

## Scenes

Scenes are `narrative::scenes` (a new scene at paragraph 0 and at every scene break or chapter
start), the same indexing as #354. A scene's tense is:

1. **declared**: `sceneTenses[i]`, else `tense`;
2. **inferred** (`narrative::scene_tense`): past + present ≥ `minimumSceneSentences` and the
   majority share ≥ `minimumMajorityPercent`;
3. **inherited**: with `inheritDocumentTense`, a scene below the sentence floor takes the document
   tense when the document passes 90% over ≥ 30 classified sentences, unless the scene's own votes
   favour the other tense;
4. otherwise **undetermined**, with no flags. The dead band (a majority under 70%) hides drift that
   covers more than about 30% of a scene; declaring the tense fixes that.

## Flags

A **run** is a maximal sequence of minority-tense sentences in a scene. Unclassified sentences
inside it don't break it, and neither does a dialogue tag in the scene tense (*The room is dark.
“Hello?” she called. The air is cold.*); a sentence in the scene tense or a mixed sentence ends
it. A run of `minimumRun` or more is `run`.

A run of one is `single` only with `includeSingles`, when every voting chain has a specific subject
(a personal pronoun, a name, or a *the*/possessive/demonstrative noun phrase), it is not a gnomic
single (one present-simple dynamic verb with a definite inanimate subject: *The river runs east.*),
and one of the two preceding classified sentences is in the scene tense.

In a present scene the minority is past (not past perfect), flagged only with `includeReverse`,
only in runs of at least 3, and never with a backstory anchor in the run or the sentence before it
(*ago, last, once, years, back then, used to, when I/he/she was*, a *remember/recall* verb).

Cost is one pass over the tokens. Spans come from `Span::covering` over token spans, so nothing
slices a string; a U+2028 inside a paragraph splits sentences the way the analysis does.

## Evaluation

`python3 bench/style/tense_shift.py` ([bench README](../bench/style/README.md#tense-shift-355)).
Sources are CC0 only: the five stories and the Moby-Dick excerpt in `bench/public`, the Standard
Ebooks books and chapters pacing uses, and three Standard Ebooks texts pinned for this issue,
*Bleak House*, *Jane Eyre* and *David Copperfield*, for their present-tense chapters and historical
present. Seven more Standard Ebooks novels (*The Voyage Out*, *Night and Day*, *Women in Love*,
*Howards End*, *Main Street*, *This Side of Paradise*, *A Farewell to Arms*) were used only for the
held-out draw. Nothing from `bench/corpus` was read. Results are in
`bench/style/tense-shift-baseline.json`.

### Synthetic perturbations (seed 355)

400 scenes inferred past on the clean run (at least 12 classified sentences). Finite main-clause
heads are re-inflected to the present, VBZ or VBP by subject agreement, keeping capitals and curly
apostrophes and skipping *were* after *if*; a form is written only when the tagger reads it as the
wanted lemma and POS (`tense_forms`). 500 of each kind: **single** flips one past sentence, **run**
flips 2–4 consecutive classified past sentences, **decoy** flips only exempt chains (subordinate,
reported, generic, habitual). A flag is new when the clean scene didn't have it; precision is new
flags overlapping the flipped sentences over all new flags.

| | Default options | `includeSingles` |
|---|---:|---:|
| Run recall | **0.848** (first person 0.834, third 0.868) | 0.948 |
| Single recall | 0.028 (only where a flip joins an existing present sentence) | 0.756 (first 0.766, third 0.743) |
| Precision of new flags | **1.000** (438 / 438) | 1.000 (852 / 852) |
| Decoy leakage | **0 / 500** | 0 / 500 |

**Reverse.** 105 whole past scenes flipped to the present (past perfect left alone) gave no flags
with `includeReverse` (0 false flags). Putting 3 consecutive sentences back into the past was
found in 54 of 103 (0.524) with `includeReverse` and never at the defaults.

The gate (`perturb --gate`): run recall ≥ 0.70, precision ≥ 0.90, decoy leakage ≤ 0.05, the
issue's thresholds, recorded 2026-10-23. Synthetic re-inflection favours common verbs and clean
scenes; it measures the mechanics, not how real slips look.

### Natural rate

Default options, narration words under the voice mask:

| Sources | Narration words | Shifts | Per 1,000 | Undetermined scenes |
|---|---:|---:|---:|---:|
| Pooled (stories, pacing books, the three new texts; Moby-Dick out) | 953,157 | 79 | **0.083** | 2.3% (0.8% of words) |
| Held-out-only novels (seven) | 623,258 | 85 | 0.136 | 1.1% |
| *Moby-Dick* (stress test) | 163,627 | 37 | 0.226 | 27% |
| `bench/public/moby150k.txt` (hard-wrapped) | 17,919 | 5 | 0.279 | 84% |

The highest rates are first-person retrospective narrators (*The Murder of Roger Ackroyd* 0.61,
*Howards End* 0.48) and essayistic omniscience. Pooled exemption counts: subordinate 24,910,
no subject 23,790, modal 8,361, question or exclamation 5,333, reported 4,623, ambiguous 1,483,
narrator frame 928, habitual 609, generic 544, unbalanced quotes 469, mixed sentence 294, past
perfect 7.

### Precision on published fiction (2026-10-23)

Each row is a flag in a public-domain context of up to two sentences either side. Two blind model
labelers from different families, Codex `gpt-6-sol` (OpenAI, Codex CLI 0.163.0, reasoning medium)
and DeepSeek V4 Pro (`opencode-go/deepseek-v4-pro`, OpenCode 2.1.2), each ran in an empty
directory holding only the guidelines and the items (id, expected, found, context), and answered
`shift` (a real tense change in the narration) and a `kind`: `slip`, `device`, `commentary`,
`gnomic`, `thought`, `dialogue`, `reported`, `tagger`, `scene` or `other`
([guidelines](../bench/style/labels/TENSE-GUIDELINES.md)). The implementer adjudicated every item
where they differed on `shift` or on slip-or-device. These are **model labels**, not human labels.
The issue's precision is `shift` and a kind of `slip` or `device`; `shiftPrecision` counts every
real tense change in narration.

| Set | Rules | Kept | Slip or device | Shift | Agreement (slip-or-device) |
|---|---|---:|---:|---:|---:|
| Development, 100 default flags (seed 355) | drawn at `7448fbf`, scored at the freeze | 82 | **26 / 82 = 0.317** (Wilson lower 0.226) | 76 / 82 = 0.927 | 84 / 100, κ 0.46 |
| Held-out, 100 default flags (seed 356) | drawn after the freeze at `17bdcbc` | 100 | **10 / 100 = 0.100** (lower 0.055) | 64 / 100 = 0.640 | 95 / 100, κ 0.68 |
| Singles (`includeSingles`, seed 357) | `17bdcbc` | 40 | **0 / 40** | 27 / 40 = 0.675 | |
| Reverse runs (`includeReverse`, all 4) | `17bdcbc` | 4 | 1 / 4 | 4 / 4 | |

Kinds on the held-out set: commentary 35, dialogue 22, gnomic 19, device 10, thought 7, reported
6, tagger 1, slip 0. On the development set: commentary 36, device 26, gnomic 14, dialogue 6.
Before the guards, the development set was 27 / 100 slip or device and 93 / 100 shift; the guards
removed 18 rows, 1 of them a device.

**The issue's 0.85 precision gate is not met, and is not gated.** Published fiction has almost no
slips (0 in 244 judged flags), so every true positive here is a deliberate device, and the
narrator's present-tense commentary (*Our big town is Cranchester*, *The Sperm Whale blows as a
clock ticks*) is grammatically the same as the slip the lens is for. The held-out set adds what the
development books lacked: unquoted dialogue that #353 does not tag (*A Farewell to Arms*), letters
and notes quoted without quote marks (*Main Street*, *This Side of Paradise*, *The Voyage Out*),
free indirect argument (*Main Street*'s Carol) and meditation (*Women in Love*). Marking letters
with `markedSpans` takes them out. CI gates `shiftPrecision` at regression floors (development ≥
0.90, held-out ≥ 0.60), not at the issue's target.

`includeSingles` would have shipped on only if singles alone reached 0.85 on the held-out draw;
they are 0 / 40, so it is off.

### What the numbers mean for a writer

On published fiction the lens raises about one flag per 12,000 narration words, nearly all of
them intended. On a draft, a run of slipped sentences in a past scene is found 85% of the time in
the synthetic suite, and nothing else in a clean scene is flagged. The lens is opt-in for that
reason, and its flags read as "this passage is in the other tense", not "this is an error".

## Known limits

- Narrator commentary and definite-subject generic statements (*The river is wide.*) look like
  slips; only the guards above take some out.
- Untagged direct thought, unquoted dialogue without tags and unmarked letters are narration to
  #353, so they vote. The question/exclamation exemption absorbs some thought.
- POS and chunk errors (VBD/VBN/JJ, missing verb chunks) fail toward neutral by design.
- Unmarked scene changes and simple-past flashbacks in present-tense books mimic drift; reverse is
  off by default and needs a run of 3 and no backstory anchor.
- `narrative::sentence_tense` and `scene_tense` also drive #354's tense evidence; changing an
  exemption here changes #354, so both lenses' tests must run on any `narrative.rs` change.

## Size

`writemorebetter_wasm_bg.wasm` (release, wasm-bindgen 0.2.129, macOS, Rust's wasm32 target, no
`wasm-opt`) grows from 7,692,477 to 7,808,383 bytes (+115,906, +1.5%) over #353, and from
1,916,974 to 1,939,412 with `gzip -9` (+22,438). The lens ships no data file; its word lists are
short MIT/Apache-2.0 constants.
