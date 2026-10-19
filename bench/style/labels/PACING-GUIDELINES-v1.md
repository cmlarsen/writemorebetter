# Pacing labels: guidelines

You are labeling where a story stalls. Read each passage as a careful line editor would read a
manuscript, and mark the paragraphs where forward motion stops long enough for a reader to feel it.
These labels measure a tool that points editors at slow stretches. You are the reference, so
label what you judge, not what you guess a tool would flag.

Work alone. Do not look at other labelers' files, at any report or program output, or at the
repository the packet came from, and do not look up the source texts. If you recognize a book,
judge the passage as written, not its reputation. Spend the same care on every passage; there is
no quota, and many passages have no slow stretch at all.

## The packet

- `p01.md` … `p40.md`: one passage per file, 6–10 consecutive paragraphs from a novel or short
  story, numbered `[1]`, `[2]`, …. Passages are excerpts in shuffled order and are unrelated to
  each other. A passage can start or end in the middle of a scene or a slow stretch.
- `labels-template.jsonl`: one line per passage with every field blank. Fill it in and save it
  under a new name; the output format is described [below](#output).
- This file.

Some passages come from nineteenth- and early twentieth-century novels, others from short
contemporary stories that contain deliberate typos and grammar mistakes. Ignore spelling,
grammar, archaic diction and sentence length. None of them is slowness by itself: a long, ornate
sentence can carry a chase, and a run of short sentences can sit still.

## Per paragraph

### `slow`: true or false

Mark `true` when you would mark the paragraph as part of a stretch where the story stalls: the
scene's present action and conversation stop and the text dwells on

- **backstory**: events before the scene's present moment, told rather than dramatized;
- **exposition**: explanation of facts, history, social context, how something works, or the
  general situation;
- **interior rumination**: a character turning thoughts, memories or feelings over without
  deciding or doing anything that moves the scene;
- **extended static description**: a place, person or object described at length while nothing
  happens.

Mark `false` when the paragraph moves the story: something happens, someone speaks or acts, a
decision is made, the situation changes, or a short passage of setup gives the reader what the
next moment needs.

Judge each paragraph in context. The same paragraph of description can be a necessary breath after
a fight (false) or the fourth paragraph of scenery in a row (true). Ask: *if I were marking this
manuscript for pacing, would this paragraph sit inside the bracket I draw around the slow part?*

### `kind`: what the paragraph mostly is

| Kind | Use when the paragraph mostly… |
|---|---|
| `backstory` | recounts events from before the scene's present moment (a character's history, how things came to be). In a story told in the past tense, this means further in the past than the scene itself. |
| `exposition` | explains: facts, customs, setting, history in general, how something works, a summary of the state of affairs. |
| `interior` | gives a character's present thoughts, feelings, reflections or memories as they think them. |
| `description` | depicts how a place, person, object or scene looks, sounds or feels, with little or no event. |
| `action` | narrates events happening in the story's present, physical or not, including summarized present events (*for a week they searched the hills*). |
| `dialogue` | is mostly quoted speech, with or without tags and beats. |
| `mixed` | combines two or more kinds in roughly equal measure, with none clearly dominant. |

Choose the kind that takes up most of the paragraph. Use `mixed` only when you cannot name a
dominant kind; a line of dialogue inside a mostly descriptive paragraph is still `description`.
`kind` is independent of `slow`: a paragraph of action can be slow, though it rarely is, and a
paragraph of backstory can be fast.

## Per passage: stretches and severity

List each slow stretch as an inclusive paragraph range with a severity:

```json
{"start": 3, "end": 6, "severity": 3}
```

- Every `slow: true` paragraph belongs to exactly one stretch, and every paragraph in a stretch is
  `slow: true`. No passage needs a stretch; leave the list empty when nothing stalls.
- A stretch can be a single paragraph if that paragraph alone makes the story stall (often a long
  one). Two adjacent slow parts with clearly different severities can be two stretches; otherwise
  use one.
- A stretch can begin at paragraph 1 or end at the last paragraph when the passage cuts into it.
  Judge only what you can see.

Severity answers *how much would a reader feel the drag?*

| Severity | Meaning |
|---|---|
| 1 | Barely: a reader might drift for a moment, then the story picks up. |
| 2 | Mild: the pause is noticeable but the reader stays with it. |
| 3 | Moderate: a typical reader would start skimming. |
| 4 | Strong: most readers would skim, and some would stop reading here. |
| 5 | Severe: the story stops dead for a long time. |

## Edge cases

- **Dialogue-free action is not slow.** A fight, a chase or a search told without a word of
  speech keeps moving. Mark it `action`, `slow: false`.
- **Brief necessary exposition is not slow.** One or two paragraphs that orient the reader (who
  these people are, where we are, what the rule is) before the scene uses that information
  are `false`. They become slow when they keep going past what the scene needs.
- **Dialogue can be exposition.** When a character lectures, recites history or explains the
  situation at length to someone who would already know it, the conversation has stopped
  dramatizing. Label the kind `dialogue`, and mark it `slow: true` if the exchange has turned
  into an information dump. A lively argument about the past is not slow.
- **Interior moments that change things are not slow.** A paragraph in which a character
  realizes, decides or resolves to act moves the story, even if it is all thought.
- **Summary that moves time forward is not slow.** *They marched for three days and lost two
  horses* is fast storytelling. Summary of how things have always been is exposition.
- **Description inside action** (the room as the character bursts into it) is usually not slow.
  A stretch of scenery or portraiture with no event usually is, once it goes on.
- **Backstory that is the scene's point**, such as a confession or a revelation told to someone who
  reacts, can be fast. Judge whether the reader is waiting for the story to resume.
- **Letters, sermons, legends and other embedded texts** are judged the same way: do they move
  the story, or does the story wait for them to end?
- Be consistent with yourself. When you hesitate, decide, and record your doubt in `note`.

## Invented examples

These examples were written for these guidelines and come from no published text.

> [1] Mara shouldered the door, and it gave on the third try. The hallway beyond was dark and
> smelled of wet plaster. She ran.

`action`, `slow: false`.

> [2] The Halloway mill had been her grandfather's before it was her father's, and before that it
> had belonged to a man whose name nobody remembered. In the dry summer of her twelfth year the
> wheel had stopped, and it had never really started again. Her father had spent the next decade
> patching it, borrowing against it, and talking about it at supper as if it were a sick relative.

`backstory`. On its own, after a tense scene, a reader might take it as a breath (`false`). If
two more paragraphs about the mill's history follow it, all three are one stretch (`true`,
severity 2 or 3).

> [3] She wondered whether he had meant it. She thought about the way he had said it, the
> half-turn of his head, the pause before the last word. Perhaps he had not meant anything at all.
> Perhaps she was making something out of nothing again, the way she always did, the way her
> mother always said she did.

`interior`. Rumination that circles without moving: `true` if it continues or sits inside other
slow material.

> [4] Then she knew. The key had never been in the drawer, which meant someone had taken it
> before the fire. She stood up.

`interior`, `slow: false`: a realization that changes the scene.

> [5] “As you know, Captain,” said the engineer, “the reactor was built in the third year of the
> colony, after the old one failed, and it has run on salvaged parts ever since, which is why
> the council voted to ration power in the winter months, as they have every winter since.”

`dialogue`, `slow: true`: exposition in quotation marks, told to someone who already knows it.

> [6] The square was paved with gray stones worn smooth in the middle. Pigeons crowded the
> fountain's rim. Around the edges the shops kept their shutters half down against the heat, and
> the café awnings were faded to the color of weak tea.

`description`. One such paragraph between events is `false`; the third in a row with nothing
happening is likely `true`.

## Output

Fill in `labels-template.jsonl` and save it as `<your labeler id>.jsonl`. The file has exactly one
JSON object per line, one line per passage, every passage once. Keys:

| Key | Type | Content |
|---|---|---|
| `labeler` | string | your labeler id, identical on every line |
| `passage` | string | the passage id, as in the template |
| `paragraphs` | array | one object per paragraph in order: `paragraph` (1-based integer), `slow` (`true` or `false`), `kind` (one of the seven kinds), optional `note` (string) |
| `stretches` | array | zero or more `{"start", "end", "severity"}` objects with integers, inclusive 1-based paragraph numbers, in order and not overlapping |
| `notes` | string | optional short rationale for the passage; may be empty |

No other keys are allowed, and `null` is not a valid label. Example line for an eight-paragraph
passage with one slow stretch:

```json
{"labeler": "labeler-a", "passage": "p07", "paragraphs": [{"paragraph": 1, "slow": false, "kind": "dialogue"}, {"paragraph": 2, "slow": false, "kind": "action"}, {"paragraph": 3, "slow": true, "kind": "backstory"}, {"paragraph": 4, "slow": true, "kind": "backstory"}, {"paragraph": 5, "slow": true, "kind": "interior", "note": "could be read as the decision point"}, {"paragraph": 6, "slow": false, "kind": "interior"}, {"paragraph": 7, "slow": false, "kind": "action"}, {"paragraph": 8, "slow": false, "kind": "dialogue"}], "stretches": [{"start": 3, "end": 5, "severity": 3}], "notes": "Family history interrupts the arrival; the scene resumes at 6."}
```

The coordinator checks each file with `python3 bench/style/pacing_labels.py validate <file>`,
which reports every schema error with its line number.
