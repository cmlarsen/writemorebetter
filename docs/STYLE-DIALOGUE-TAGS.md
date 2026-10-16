# Dialogue tag report

Issue [#249](https://github.com/cmlarsen/writemorebetter/issues/249), part of #241. The report finds the
tag attached to each quoted span from #243's document pass and reports:

- neutral tag verbs as counts (`neutralVerbs`) and showy verbs to review (`showyVerbs`);
- manner adverbs on tags (`said softly`), as `DialogueTag::adverb`;
- tags whose verb cannot produce speech (`“Fine,” she smiled.`), as `DialogueTagVerbClass::NonSpeech`;
- runs of more than 12 untagged exchanges (`untagged_runs`);
- action beats, counted separately and never flagged (`beats`).

```rust
use writemorebetter_style::reports::dialogue_tags::{self, DialogueTagBaseline, DialogueTagOptions};
let baseline = DialogueTagBaseline::new().with_said_asked_percent(80.0);
let report = dialogue_tags::report(&document, &DialogueTagOptions::new(), Some(&baseline))?;
```

The document needs token details (the default). A compact document (`with_token_details(false)`) has
no POS readings, so the report returns `StyleError::NeedsTokenDetails` rather than empty tags.
`skip_names` tokens are treated as names: they can be a tag's subject and are never its verb, so
`“Go,” said Will.` with `Will` skipped is a *say* tag rather than a modal. The report is part of the
combined `reports::run` call and the bindings' `styleReports`
([STYLE-REPORTS.md](STYLE-REPORTS.md#one-call-for-every-report)); typed Swift records come with the
bindings follow-up.

## Neutral and showy verbs

All parsed tags remain in `tags`; `otherVerbs` remains a descriptive count for compatibility.
Use `tag.showy` for individual verb flags. `showyVerbs` counts speech bookisms; non-speech verbs are counted separately in `nonSpeech`, so the aggregates do not duplicate them. `neutralVerbs` includes said/asked, replied,
answered, told, added, continued, called, whispered, shouted, muttered, repeated and went on.
Cried, explained, demanded, inquired, urged, remarked and other recognized attributions are showy.
The separate implementation-authored
`neutral-verbs.txt` and `showy-verbs.txt` lists contain no copied material.
Showy verbs include hissed, chortled, snarled, ejaculated, growled and breathed; non-speech tags
such as laughed, sighed and smiled are also showy. Unlisted verbs are counted and never flagged.
The older speech/nonSpeech classification remains a grammatical distinction, independent of style.

## Method

Each paragraph is split into quoted pieces (the document's **speech** spans, `Document::speech`) and
the narration between them. Clause boundaries come from the tokenizer's sentences and from `!`/`?` tokens, cut at the next
quote; a `.` alone never ends a clause, so `Mr.` and `St.` stay inside it.

Passive attributions such as “Go,” was the answer, “There she blows,” was sung out, and a cry of
“Where is he?” count as speech. A following listed attribution also keeps an embedded quote in speech
(Then “Fine,” she said). Titles with a title noun and copula remain narration. This behavior is shared
by every report.

- **After a quote**, the first narration clause is attached when the quote ends in `,`, `:` or `;`, or
  when the clause starts lowercase. Otherwise (after `.`, `?`, `!`, a dash, an ellipsis or no
  punctuation) a capitalized clause is attached only as a bare speech tag: a listed speech verb with
  nothing after subject and verb but an adverb, `to`, an object pronoun or a name, up to the next
  punctuation or preposition. `“Really?” Mara whispered in his ear.` and the common slip
  `“Fine.” Mara said with a shrug.` are tags; `“Fine.” She smiled.`, `“Hello?” She said nothing.`
  and `“Wait—” She began to cry.` are beats.
- **Before a quote**, the last clause is attached when it ends in `,` or `:` or with a lead-in speech
  verb (`She said “Go.”`). A paragraph ending in such a clause also tags a quote that opens the next
  paragraph (`John said:` ⏎ `“Hello.”`). Speech verbs that usually take a quoted object
  (`style/data/dialogue/object-verbs.txt`: `read`, `press`, `return`, `order`, `sing`, `call`, …) tag
  a following quote only with a `,` or `:` before the quote, speech punctuation at its end, and a
  speaker: a pronoun or name as the last word before the verb (adverbs and auxiliaries skipped) or
  right after it, or a noun there when the quote ends in `!` or `?`. `He returned, “No, thank you.”`
  and `The old man called, “Wait!”` are tags; `She pressed “Send.”`, `They sang “Jerusalem”`,
  `The headline read: “War Ends.”` and `The sign Anna saw read, “Closed.”` are not. After a quote the
  full list applies (`“No,” returned Holmes.`).
- **A tag** is a subject plus a verb at the edge of an attached clause: subject first (`he said`,
  `the old man said`, `she’d said`, `Anna’s mother said`) or inverted (`said the captain`, `said
  he`). The subject is a pronoun, a name, a skipped name, or a short noun phrase (possessives
  included); the verb is a token with a `VB*` reading. A listed verb wins over an unlisted one; an
  unlisted finite verb is still a tag (`DialogueTagVerbClass::Unlisted`), counted in `other_verbs` but never
  flagged. `be`, `have` and `do` are never tag verbs. An unpunctuated quote followed directly by a lowercase form
  of one of them, and not preceded by a lead-in verb, is the subject of that verb, not speech:
  `“Yes” was the answer.` and `Her favorite poem, “The Raven,” was long.` have no tag, are not
  exchanges, and are left out of `speech_spans`. This test and the embedded-quote test below run in
  the document pass, so every report uses the same speech spans (see
  [STYLE-DOCUMENT.md](STYLE-DOCUMENT.md#narration-and-dialogue)). The tag span runs from the subject through the verb,
  its particle (`he cut in`) and adverb, with any auxiliaries and adverbs between subject and verb
  (`She had always said`); an auxiliary is included only when nothing but adverbs separates it from the verb, so
  `It was then he said` gives `then he said`. An inverted possessive subject is kept whole
  (`said Anna’s mother`).
- **Split dialogue** (`“Wait,” she said, “there’s more.”`) is one tag with position `split` and both
  quotes in `quotes`. A tag that ends with a full stop before more speech is `after`.
- **An action beat** is a narration clause touching a quote that is not attached and has a finite verb.
  One clause between two quotes is one beat.
- **Adverbs** are tokens with an `RB` reading ending in *-ly* next to the verb, after an inverted
  subject, or after an object pronoun or a one- or two-word name (`said softly`, `softly said`, `said
  the captain gruffly`, `asked him softly`, `told Mara Lynde sharply`), commas ignored. Intensifiers such as *rather*, *quite* and *very* may intervene. Adverbs on non-speech verbs are
  not manner-adverb flags. Inverted subject spans stop at a comma and retain noun modifiers such as
  *the examining magistrate*. *Reflect* requires an attached comma or lead-in frame: *Poirot reflected for a
  minute* after a sentence-final quote is a beat, while *“No,” she reflected* remains an attribution. Time and
  degree words in `style/data/dialogue/adverb-exclusions.txt` are ignored.
- **Embedded quotes** preceded directly by a word in the same paragraph, other than a listed verb
  outside the object-verb list (`He called it “grub” and ate.`, `She pressed “Send.”`), are scare
  quotes, titles or quoted terms. They anchor no tag or beat, are not exchanges, and are left out of
  `speech_spans`, the denominator for span rates (`dialogue_spans` counts every document span).
- **Exchanges and runs.** An exchange is a paragraph where a dialogue span starts; paragraphs only
  continuing an earlier span are neutral. An exchange is attributed when one of its quotes has a tag
  or a beat, because a beat names who acts. A short standalone action/attribution paragraph such as
  *The magistrate cut her short* also attaches a beat to the preceding quote. A paragraph with no dialogue ends a run. Runs longer than
  `max_untagged_exchanges` (default 12) are reported with their span and quotes.

Tags, beats and untagged runs carry `quotes`: the spans of the quotes they attach to, so a binding
caller never needs the document to resolve them. `saidAsked`, `withAdverb` and `nonSpeech` are
`Share { count, percent, difference, highlighted }` records over all tags. `percent` is null below
`minimum_tags` (default 15), with `lowConfidence` below 50 tags. The optional `DialogueTagBaseline` is a separate argument, as for the
sentence reports, with three optional percentages (`saidAskedPercent`, `adverbPercent`,
`nonSpeechPercent`, each in 0..=100 or `InvalidBaseline`). `difference` is the share minus the
baseline in percentage points. Only `highlighted` warrants a baseline highlight: the percentage must
be strictly outside a supplied `saidAskedBand`, `adverbBand` or `nonSpeechBand` (`{p5,p95}`). Missing
bands never highlight. Bands must be ordered and within 0–100.
`DialogueTagBaseline::bundled()` returns `None`: no default baseline ships yet.

## Word lists

`style/data/dialogue/speech-verbs.txt`, `non-speech-verbs.txt` and `adverb-exclusions.txt` were written
for this crate. They are not copied or derived from LanguageTool or any other list. Speech verbs are
lemmas, including phrasal verbs such as `go on`, `cut in` and `cry out`. The non-speech list covers
facial expressions, gestures and bodily actions that cannot themselves produce words: `smile`,
`laugh`, `nod`, `shrug`, `sigh` and similar. Contested verbs (`snort`, `hesitate`) are on neither list
and are reported as unlisted. Thought verbs with speech uses remain heuristic; the writer review's narrative *reflect* case
is guarded by its attachment frame rather than removing legitimate comma-attached tags.
`object-verbs.txt` marks speech verbs that usually take a quoted object, for the lead-in rule above.

## Accuracy

The tier 2 suite in [bench/style/README.md](../bench/style/README.md#dialogue-tags-249) scores 238
model-labeled records over 186 dialogue spans in four CC0 Standard Ebooks passages that are not in
LitBank. On all four passages: tag position precision is 100% for every position, with recall 100%
(after, split) and 85.71% (before); verb class recall is 97.10% for speech and 100% for non-speech;
adverb precision/recall is 100% / 100%. Frozen-label beat precision/recall is now 69.23% / 90.00%:
the added standalone *I nodded* and *She smiled* beats are correct but absent from those labels.
The two older false beat predictions remain; the rebaseline records this explicitly. Labels come from two independent
model sessions, coordinator-adjudicated, and the test passages were read during development, so
these are regression floors, not a claim about modern fiction.

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
- A capitalized speech clause after a quote with a longer object (`“Go.” She told Tom to leave.`,
  `“Why?” Tom asked his father.`) is a beat, not a tag, because only bare tags attach without a comma.
- An object-taking verb with no comma before its quote (`He called “Come here!” across the yard.`) is
  read as naming the quote, so the quote is embedded and not an exchange.
- A noun subject before an object-taking verb counts as a speaker when the quote ends in `!` or `?`, so
  `The headline read: “War Ends!”` is a tag.
- Passive speech attributions count as speech even when their subject/verb frame does not yield an
  explicit tag. They can therefore remain untagged exchanges.
- Dash-led dialogue and unpunctuated single-quoted speech are outside #243's span detector, so they
  have no tags.
