# Thought and speech span guidelines (#353)

You are judging spans that a detector marked as a character's **direct thought** or **direct
speech** in public-domain fiction. Judge each item on its own. Do not look for or use any other
labels or the detector's code.

Each item has an `id`, a `voice` (`thought` or `speech`) and a `context`: a short window of the
book in which the marked span is in `⟦ ⟧` and the tag that attributes it (*she thought*, *he
said*) is in `⟨ ⟩`. A context may start or end with `…` where the window cuts the text.

## The categories

These follow Semino and Short's speech and thought presentation categories.

- **Direct thought (DT)**: a character's thought in their own words, usually first person and
  present tense, with a tag: *I’m done with this, she thought.* *Where is he? I wondered.*
- **Free direct thought (FDT)**: the same without a tag. Here every span has a tag, so FDT applies
  to neighbouring sentences included in a span: *Where is he? Why doesn’t he call? I wondered.*
- **Indirect thought (IT)**: reported by the narrator: *She thought that he was late.*
- **Free indirect thought (FIT)**: the character's thought in the narrator's tense and person,
  without *that*: *She had to get out. Now.* *Was he lying? she wondered.*
- **Direct speech (DS)**: a character's spoken words, quoted or not: *Get out, he said.* Joyce's
  dash dialogue (*―I don’t know, Stephen said.*) is direct speech.
- **Indirect speech (IS)** and **free indirect speech (FIS)**: reported speech in the narrator's
  tense and person: *He said he would come.*
- **Narration**: anything else.

## The label

Answer `true` when the marked span is:

- for `voice: thought`: the tagged character's DT or FDT. Words said *to oneself* (*he said to
  himself*, *she told herself*) count as thought;
- for `voice: speech`: the tagged character's DS.

Answer `false` when the marked span is narration, IT, FIT, IS or FIS, when it belongs to a
different character than the tag, when the tag is not a thought or speech tag at all, or when
`voice: thought` marks words spoken aloud to someone else (or `voice: speech` marks unspoken
thought).

Boundaries: answer `true` if most of the marked words are the DT/FDT or DS, even when the span
leaves out a few words of it or takes in a word or two of narration. Answer `false` if a third or
more of the marked words are narration or another category.

Use `note` for a short reason when you answer `false` or when you were unsure.

## Output

One JSON object per line, in the same order as the items, and nothing else:

```
{"id": "thought-spans-001", "label": true, "note": ""}
{"id": "thought-spans-002", "label": false, "note": "FIT: past tense, third person"}
```

## Miss items

Miss items (`miss-NNN`) have no `voice` and no tag mark: each `context` holds, in `⟦ ⟧`, a
narration sentence with a thought verb that the detector left alone. Answer `true` when the
sentence contains a character's **direct thought** (DT, not IT or FIT) attached to that verb, and
`false` otherwise.
