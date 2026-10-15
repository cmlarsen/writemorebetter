# Dialogue tag report

Issue [#249](https://github.com/cmlarsen/writemorebetter/issues/249), part of #241. The report finds the
tag attached to each quoted span from #243's document pass and reports:

- tag verbs other than *said* and *asked*, with counts (`other_verbs`);
- manner adverbs on tags (`said softly`), as `DialogueTag::adverb`;
- tags whose verb cannot produce speech (`“Fine,” she smiled.`), as `VerbClass::NonSpeech`;
- runs of more than 6 untagged exchanges (`untagged_runs`);
- action beats, counted separately and never flagged (`beats`).

```rust
use writemorebetter_style::reports::dialogue_tags::{dialogue_tags, DialogueTagOptions};
let report = dialogue_tags(&document, &DialogueTagOptions::new());
```

The document needs token details (the default). A compact document (`with_token_details(false)`) has
no POS readings, so it yields no tags or beats, and `untagged_runs` and `untagged_exchanges` are `None`
rather than flagging every exchange. `skip_names` tokens are treated as names: they can be a tag's subject and are
never its verb, so `“Go,” said Will.` with `Will` skipped is a *say* tag rather than a modal. No Swift
or WASM binding is added here; bindings for all reports land together after #244.

## Method

Each paragraph is split into quoted pieces (the document's dialogue spans) and the narration between
them. Clause boundaries come from the tokenizer's sentences and from `!`/`?` tokens, cut at the next
quote; a `.` alone never ends a clause, so `Mr.` and `St.` stay inside it.

- **After a quote**, the first narration clause is attached when the quote ends in `,`, `:` or `;`, or
  when the clause starts lowercase. Otherwise (after `.`, `?`, `!`, a dash, an ellipsis or no
  punctuation) a capitalized clause is attached only as a bare speech tag: a listed speech verb with
  nothing after subject and verb but an adverb, `to`, an object pronoun or a name, up to the next
  punctuation. `“Really?” Mara whispered.` and the common slip `“Fine.” Mara said.` are tags;
  `“Fine.” She smiled.` and `“Hello?” She said nothing.` are beats.
- **Before a quote**, the last clause is attached when it ends in `,` or `:` or with a lead-in speech
  verb (`She said “Go.”`). A paragraph ending in such a clause also tags a quote that opens the next
  paragraph (`John said:` ⏎ `“Hello.”`). Speech verbs that usually take a quoted object
  (`style/data/dialogue/object-verbs.txt`: `read`, `press`, `return`, `order`, `sing`, `call`, …) tag
  a following quote only with a person subject (pronoun or name), a `,` or `:` before the quote and
  speech punctuation at its end: `He returned, “No, thank you.”` is a tag; `She pressed “Send.”`,
  `They sang “Jerusalem”`, `The headline read: “War Ends.”` are not. After a quote the full list
  applies (`“No,” returned Holmes.`).
- **A tag** is a subject plus a verb at the edge of an attached clause: subject first (`he said`,
  `the old man said`, `she’d said`, `Anna’s mother said`) or inverted (`said the captain`, `said
  he`). The subject is a pronoun, a name, a skipped name, or a short noun phrase (possessives
  included); the verb is a token with a `VB*` reading. A listed verb wins over an unlisted one; an
  unlisted finite verb is still a tag (`VerbClass::Unlisted`), counted in `other_verbs` but never
  flagged. `be`, `have` and `do` are never tag verbs, so `“Yes” was the answer.` has no tag. The tag
  span runs from the subject through the verb, its particle (`he cut in`), auxiliaries (`He had said`)
  and adverb.
- **Split dialogue** (`“Wait,” she said, “there’s more.”`) is one tag with position `split` and both
  quotes in `dialogue`. A tag that ends with a full stop before more speech is `after`.
- **An action beat** is a narration clause touching a quote that is not attached and has a finite verb.
  One clause between two quotes is one beat.
- **Adverbs** are tokens with an `RB` reading ending in *-ly* next to the verb, after an inverted
  subject, or after an object pronoun or a one- or two-word name (`said softly`, `softly said`, `said
  the captain gruffly`, `asked him softly`, `told Mara Lynde sharply`), commas ignored. Time and
  degree words in `style/data/dialogue/adverb-exclusions.txt` are ignored.
- **Embedded quotes** preceded directly by a word in the same paragraph, other than a listed verb
  outside the object-verb list (`He called it “grub” and ate.`, `She pressed “Send.”`), are scare
  quotes, titles or quoted terms. They anchor no tag or beat, are not exchanges, and are left out of
  `speech_spans`, the denominator for span rates (`dialogue_spans` counts every document span).
- **Exchanges and runs.** An exchange is a paragraph where a dialogue span starts; paragraphs only
  continuing an earlier span are neutral. An exchange is attributed when one of its quotes has a tag
  or a beat, because a beat names who acts. A paragraph with no dialogue ends a run. Runs longer than
  `max_untagged_exchanges` (default 6) are reported with their span and quotes.

Percentages (`said_asked_percent`, `adverb_percent`, `non_speech_percent`) are shares of tags and are
`None` below `min_tags` (default 50). Every report accepts a `DialogueTagBaseline` with the same three
percentages, echoed in the report so callers can compare. No default baseline ships yet.

## Word lists

`style/data/dialogue/speech-verbs.txt`, `non-speech-verbs.txt` and `adverb-exclusions.txt` were written
for this crate. They are not copied or derived from LanguageTool or any other list. Speech verbs are
lemmas, including phrasal verbs such as `go on`, `cut in` and `cry out`. The non-speech list covers
facial expressions, gestures and bodily actions that cannot themselves produce words: `smile`,
`laugh`, `nod`, `shrug`, `sigh` and similar. Contested verbs (`snort`, `hesitate`) are on neither list
and are reported as unlisted. Thought verbs are on the speech list only when a dictionary sense is
saying something aloud in that manner: `muse`, `reflect` and `wonder` (to say thoughtfully or
wonderingly) and `recall` (to say in recollection) are listed; `think` has no such sense and is not.
`object-verbs.txt` marks speech verbs that usually take a quoted object, for the lead-in rule above.

## Accuracy

The tier 2 suite in [bench/style/README.md](../bench/style/README.md#dialogue-tags-249) scores 238
model-labeled records over 186 dialogue spans in four CC0 Standard Ebooks passages that are not in
LitBank. On all four passages: tag position precision is 100% for every position, with recall 100%
(after, split) and 85.71% (before); verb class recall is 97.10% for speech and 100% for non-speech;
adverb precision/recall is 100% / 100%; beats are 81.82% / 90.00%. Labels come from two independent
model sessions, owner adjudication is pending, and the test passages were read during development,
so the CI gate is provisional and these are regression floors, not a claim about modern fiction.

## Published baseline

The published-fiction baseline (said/asked share, adverb rate, non-speech share and more) needs
running text, so it is a tier 3 aggregate-only command that the owner runs:

```sh
python3 bench/style/run.py dialogue-tags tier3 --export /path/to/export/out
```

It weights authors equally and prints only numbers; see the benchmark README.

## Known limits

- One-word attribution verbs outside the lists in an unusual frame are missed; for example
  `her husband addressed me in his painstaking voice:` (the verb is unlisted and four words from the
  colon).
- Verbless participial beats (`Then, turning to his wife:`) are not beats.
- Narration that runs into a quote in the same sentence without a lead-in comma is counted as a beat.
- A capitalized speech clause after a quote with a longer object (`“Go.” She told Tom to leave.`) is a
  beat, not a tag, because only bare tags attach without a comma.
- Dash-led dialogue and unpunctuated single-quoted speech are outside #243's span detector, so they
  have no tags.
