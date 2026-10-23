# Tense shift guidelines (#355)

You are judging passages that a detector marked as a **tense shift**: narration that leaves the
tense the rest of its scene is told in. The texts are public-domain fiction. Judge each item on its
own. Do not look for or use any other labels or the detector's code.

Each item has an `id`, an `expected` tense (the scene's tense, `past` or `present`), the `found`
tense of the marked narration, and a `context`: up to two sentences either side of the marked
sentences, which are in `⟦ ⟧`. `¶` is a paragraph break. The marked span may contain quoted speech;
the detector ignores quoted speech and judges only the narration around it.

## The two answers

**`shift`** (true or false): the marked narration really is in the `found` tense, it really is the
narrator speaking (not a character's speech or thought), and the scene around it really is told in
the `expected` tense. Answer `false` when any of these fails: the marked words are mostly dialogue,
a letter or a character's thought; the tagger misread a verb (*the leaves fall* read as a noun, a
past form read as present); or the surrounding scene is not in the `expected` tense.

**`kind`**: why the narration is in the other tense. Pick one:

- `slip`: an unintended change of tense, the error a copy editor would correct (*She opened the
  door. The room is dark. The air smells of smoke.* in a past-tense scene).
- `device`: a deliberate, sustained narrative shift the author uses as a technique: historical
  present for vividness (*It is noon; the steward announces dinner…* inside past narration), a
  present-tense chapter or frame, or a narrator moving into their own present to close a story
  (*The town is smaller now, and the garden still grows.*). A writer would want to know where it
  begins and ends.
- `commentary`: the narrator's aside or explanation in the present: a remark about how things are
  or were and still are (*He is now twenty-five.*, *Our village is very much like any other.*), an
  essayistic passage, an opinion, an address to the reader. Not a scene event.
- `gnomic`: a general truth or a timeless description (*The Sperm Whale blows as a clock ticks.*,
  *The river runs east.*).
- `thought`: a character's direct thought, untagged or tagged in a way the detector missed.
- `dialogue`: a character's speech, a quoted letter, inscription or document, or other text a
  character produced.
- `reported`: indirect speech or thought (*he said the train leaves at six*).
- `tagger`: a verb read in the wrong tense, or a noun read as a verb.
- `scene`: the scene is really told in the found tense, or the scene changes without a marked
  break, so the marked narration is not a shift away from its own scene.
- `other`: anything else; say what in `note`.

A `slip` or `device` answer goes with `shift: true`. `thought`, `dialogue`, `tagger` and `scene`
go with `shift: false`. `commentary`, `gnomic` and `reported` can be either: answer `shift: true`
when the marked narration is the narrator's own present-tense sentence in a past scene (or past in
a present scene), and `false` when it is not narration at all.

If several kinds apply to parts of the marked span, pick the one that covers most of its words.
Use `note` for a short reason, always for `other` and whenever you were unsure.

## Output

One JSON object per line, in the same order as the items, and nothing else:

```
{"id": "tense-shift-001", "shift": true, "kind": "slip", "note": ""}
{"id": "tense-shift-002", "shift": true, "kind": "commentary", "note": "narrator's aside about the village"}
{"id": "tense-shift-003", "shift": false, "kind": "dialogue", "note": "a letter"}
```
