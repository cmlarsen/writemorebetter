# Drafting leftovers

Issue #376 (part of #241) adds an **opt-in lens** for what dictation, rapid drafting and
stream-of-consciousness writing leave behind: placeholders and notes to self (*TK*, *[name]*,
*(check this)*), spoken commands typed as words (*comma*, *new paragraph*, *open quote*), filled
pauses (*um*, *uh*), phrase restarts (*he walked to the he walked to the car*) and self-corrections
(*it was red I mean crimson*). Published fiction has almost none of these, so nearly every match is
something the author meant to clean up.

Unlike the other reports, drafting may return **edits**: deletions, whitespace, punctuation from a
fixed table, line breaks, and a case-only change to the first letter of the author's next word. It
never generates prose. Every replacement is the empty string, one or more marks from the tables
below, or the author's own letter in upper case. See the 2026-10-25 decision in
[STYLE-REPORTS.md](STYLE-REPORTS.md#decisions). **Edits are offered only when the host marks the
text as dictated** (`dictated: true`, the 2026-10-26 decision); typed text is flag only, and spoken
commands are detected only in dictated text. See [Edits only for dictated text](#edits-only-for-dictated-text).

| Surface | Call |
|---|---|
| Rust | `drafting::report(&Document, &DraftingOptions)` |
| Retained Rust | `document.drafting(options_json)` |
| WASM | `document.drafting(optionsJSON?)` → `DraftingReport` (`wasm/style-results.d.ts`) |
| Swift | `document.drafting(optionsJson:) -> DraftingReport` |
| Python | `"drafting"` in the report dispatch |
| Apply edits | `drafting::apply(text, &[&DraftingEdit]) -> String` (Rust only) |

The report needs token readings (`StyleError::NeedsTokenDetails` on compact documents), because
the noun guards for commands and the closed-class guard for restarts read POS. Options are camelCase
JSON with `deny_unknown_fields`. Unknown keys are `invalidOptions`.

Inspired by rule 2 of [dictation-cleanup-rules](https://github.com/AbhishekBarali/dictation-cleanup-rules)
(Abhishek Barali, MIT). No code or data is used from it: every list here is short and written for
this project. Its cleanup is also an example of what to avoid: it deletes `um` or `uh` plus one
following mark and collapses spaces, and its own README shows `So uhm I was, uh, thinking` becoming
`So I was, thinking`, with an orphan comma. Harper's `FillerWords` deletes the word and one space
and leaves a lowercase sentence start (`Um but I'll…` → `but I'll…`).

## Shipping order

| Version | Category | Edit | Modes |
|---|---|---|---|
| v1 | `placeholder` | none (flag only) | all |
| v1 | `spokenCommand` | the mark, a line break, or a deletion | dictated text only (`dictated`), all modes |
| v1 | `filledPause` | deletion in three unambiguous patterns (dictated text only), otherwise flag only | narration (`includeDialogue` opts in) |
| v1.1 | `restart` | delete the first copy | narration (`includeDialogue` opts in) |
| v1.2 | `selfCorrection` | none (flag only) | narration (`includeDialogue` opts in) |

Not planned:

- talk-style fillers (*like*, *basically*, *you know*), which vague words and overused words
  already count;
- ML or Switchboard-trained detectors, because of their licenses and the on-device rule;
- abandoned sentences, because a sentence that breaks off is often deliberate in fiction.

Dictation tools differ (Dragon keeps fillers and types misheard commands, Apple Dictation types
some commands literally, Whisper-based tools mostly drop fillers and keep restarts), and the report
isn't tuned for any one of them.

## Options

| Option | Default | Meaning |
|---|---|---|
| `placeholders` | `true` | Report placeholders and notes to self. |
| `dictated` | `false` | The host knows the text came from speech-to-text (a dictation session or an imported transcript). Only then are edits returned; with it off, every match has `edits: []`. It also turns on spoken commands. |
| `spokenCommands` | follows `dictated` | Report spoken commands, with edits. Unset, it follows `dictated`; `false` turns commands off in dictated text. `true` without `dictated` is `invalidOptions`, so `dictated` is the one way to turn edits on. With commands off, no command detection runs and dictated quote pairs aren't found. |
| `filledPauses` | `true` | Report filled pauses, with deletion edits in dictated text where the punctuation is unambiguous. |
| `er` | `false` | Also report *er* as a filled pause (guarded; see below). |
| `restarts` | `true` | Report restarts (from v1.1). |
| `selfCorrections` | `true` | Report self-correction phrases, flag only (from v1.2). |
| `includeDialogue` | `false` | Also report filled pauses, restarts and self-corrections inside speech and other quoted text. Hesitation in dialogue is usually characterization. It never affects commands or placeholders, which are reported in every mode. |

An option for a category that hasn't shipped is an unknown key until it does. The multi-word keys
also accept snake case (`spoken_commands`, `filled_pauses`, `include_dialogue`).

### Edits only for dictated text

The owner's decision of 2026-10-26, after five review rounds in which even the simplest pause
deletion damaged typed fiction (*She said um and looked away.* → *She said and looked away.*):

- **Typed text (the default).** Every match has `edits: []`. Placeholders, filled pauses and
  everything else are flags; spoken commands aren't detected and no dictated quote pair is found.
  Accepting a suggestion can never change the author's text.
- **`dictated: true`.** The host knows the text came from speech-to-text, where a filler really is
  a filler. Filled-pause edits in patterns (a), (b) and (c) are offered, and spoken commands are
  detected, with edits.

In typed fiction a filler is usually the author's choice. The flag guards below apply in both
modes: a false flag annoys the author, and in dictated text it would also come with an edit.
`python3 bench/style/drafting.py --gate` checks that default mode returns no edit for any fixture or
negative.

## Result

`{matches, terms, groups}`. Types are camelCase on the wire with `deny_unknown_fields` on options.

```text
DraftingReport   { matches: [DraftingMatch], terms: [DraftingTerm], groups: Split<DraftingGroup> }
DraftingMatch    { span, sentence, category, term, mode, edits: [DraftingEdit] }
DraftingEdit     { span, replacement }
DraftingTerm     { term, category, count, narration, dialogue }
DraftingGroup    { matches, edits, words, perThousandWords }
DraftingCategory = "placeholder" | "spokenCommand" | "filledPause" | "restart" | "selfCorrection"
```

- `span`: what the UI underlines: the word or phrase (`um`, `comma`, `TK`, `[name]`), or a restart's
  first copy.
- `term`: the list entry (`um`, `comma`, `open quote`, `TK`, `I mean`). Restarts use `restart`.
- `sentence`: the global sentence index. `mode`: the document's speech mode at the match start
  (`Document::mode_at`), or `dialogue` inside a dictated quote pair (below).
- `edits`: what to change if the author accepts. Empty means flag only. One match is one leftover,
  so the UI accepts or ignores its edits together: a dictated quote pair is two matches, one per
  command.

`terms` has one row per term found, most frequent first. `groups` counts matches, edits and words
per mode, with `perThousandWords` null when there are no words. Without `includeDialogue`, dialogue
words aren't scanned, so dialogue counts cover commands and placeholders only. Spans, edit spans
included, are UTF-16 `{start, end}` in WASM and Swift and char offsets in Python, through the same
`Span` serialization as every report. There is no `message` field: the host words it from
`category` and `term`, as for vague words.

## Edit model

Each edit is `{span, replacement}`. The span already covers the whitespace and punctuation cleanup,
so the host applies it as is.

- **A deletion is `""`**, never an empty list. nlprule's `apply_suggestions` skips a suggestion
  with no replacements, and the drafting edit keeps the same convention.
- **A paragraph break is `"\n"`**, matching how the document splits paragraphs. A host whose editor
  models paragraphs as blocks maps a replacement containing `"\n"` to a block split.
- **No overlaps.** Edits within one match never overlap, and edits of different matches never
  overlap. Adjacent filled pauses in one sentence, separated only by punctuation and spaces, form
  one match. Pauses and accepted commands separated only by spaces and punctuation form one
  **cluster**, and the cluster gets one edit, computed from the text on both sides of the cluster.
  The cluster's first command match carries it, or its pause match when the cluster has no
  command; the other matches in it carry no edits. Clusters are separated by words, so a host can
  apply any subset in descending start order.
- **Spans come from tokens.** Byte offsets are taken from token spans and from scanning whole chars,
  so they are always char boundaries. Nothing slices text by arithmetic.
- **`drafting::apply(text, edits)`** sorts edits by start, skips any that overlap an earlier one,
  checks char boundaries, and returns the edited text. Tests, the bench script and the CLI use it.
  It isn't bound, because hosts apply UTF-16 ranges themselves.

### Deletion cleanup

Filled pauses and `scratch that` delete words, in dictated text only. The owner's 2026-10-26
decision makes this conservative: an edit is returned only where the punctuation around the pause leaves one obvious
result. **Everywhere else the pause is flagged with no edit** (empty `edits`) and the author fixes
it. A missed convenience is acceptable; a damaged sentence is not.

The edit patterns, where *word* means a letter or digit, spaces are spaces or tabs, and a sentence
start is the first word token of one of the tokenizer's sentences (`paragraph.sentences`), never a
scan for `.` or a closing quote:

- **(a) Spaces on both sides**, with a word on each side and no punctuation next to the pause:
  `So um I was` → `So I was`. A pause right before terminal punctuation counts too:
  `He left um.` → `He left.` A run of pauses is deleted the same way only when every gap inside it
  is spaces alone (`we walked um uh to the river`). The word before may end in a period that
  doesn't end a tokenizer sentence, as after an abbreviation, and nothing is capitalized:
  `He left at 3 p.m. um the door` → `He left at 3 p.m. the door`.
- **(b) Sentence start**, then one comma, an ellipsis or nothing, then a word: `Um, the door`,
  `Um… the door` and `Um the door` all become `The door`, with the capital below. In dialogue,
  with `includeDialogue`, the first word inside an opening quote counts when the tokenizer starts
  a sentence there (`“Um, I don’t know.”`). A pause after U+2028, or at a sentence start that
  follows a sentence ending with a closing quote, is flag only: a lowercase dialogue tag may come
  next (`“Why?” um, she asked.`). So is a lowercase pause after an unquoted `?` or `!`, where the
  tokenizer starts a sentence but the author's sentence goes on (`He yelled Fire! um and ran.`,
  `Why? um, she wondered.`); a capitalized pause there starts the author's sentence
  (`What now? Um, the door` → `What now? The door`). A camel-cased next word (`iPhone`, `eBay`) is
  never capitalized, so `Um, iPhone sales` is flag only.
- **(c) A comma pair after a sentence-initial discourse word** keeps the first comma:
  `Well, um, I was` → `Well, I was`. The rule is that the discourse word must not belong to the
  phrase after the pause, or deleting the pause leaves a misplaced comma or flips the meaning
  (`No, um, one came` → `No, one came`). *Oh*, *Yes*, *Yeah*, *Okay*, *OK* and *Ok* can't start a
  following phrase, so any word may follow. *Well*, *So*, *No*, *Now*, *Right*, *Sure*, *Look* and
  *Listen* can (*well enough*, *so much*, *no one*, *now is*, *right here*, *sure enough*, *look
  at*, *listen to*), so they count only when the word after the pause is a subject pronoun (*I*,
  *you*, *he*, *she*, *we*, *they*, *it*) or capitalized, which none of those phrases continues
  with: `So, um, she left` → `So, she left`, but `So, um, much for that` is flag only. Every other
  comma pair is flag only, because the first comma may be the author's: `I was, uh, thinking`,
  `She laughed, um, cried, and left.`, `If you want, um, we can go.`, `bread, um, and milk`,
  `Mary, um, I was`.

**Flag only:** any dash next to the pause, a bracket or parenthesis, a quote edge, a pause between
two terminal marks (`He paused. Um. Then`), an ellipsis or comma on one side only, a comma pair
other than (c), a pause after a sentence that ends with a closing quote, a pause at a paragraph
end, U+2028 next to the pause, a run of pauses with a comma, dash or ellipsis inside it
(`The cat um, uh, sat`, `The cat um—uh sat`), and every other shape not listed above. The report
never deletes across a paragraph boundary.

Edits in this table are for dictated text; typed text gets the same matches, flag only.

| Input | Edit span → replacement | Result |
|---|---|---|
| `So um I was` | ` um` → `` | `So I was` |
| `Well, um, I was` | `um, ` → `` | `Well, I was` |
| `I was, uh, thinking` | flag only | unchanged |
| `I was—um—thinking` | flag only | unchanged |
| `I was… um… thinking` | flag only | unchanged |
| `Um, the door` | `Um, t` → `T` | `The door` |
| `Um… the door` | `Um… t` → `T` | `The door` |
| `He left um.` | ` um` → `` | `He left.` |
| `He left at 3 p.m. um the door` | ` um` → `` | `He left at 3 p.m. the door` |
| `“Why?” um, she asked.` | flag only | unchanged |
| `Why? um, she wondered.` | flag only | unchanged |
| `What now? Um, the door was open.` | `Um, t` → `T` | `What now? The door was open.` |
| `No, um, one came to the door.` | flag only | unchanged |
| `Um, iPhone sales were down.` | flag only | unchanged |
| `He left um”` (dialogue, `includeDialogue`) | flag only | unchanged |
| `“I was, um,” she said` (`includeDialogue`) | flag only | unchanged |
| `“Um, I don’t know.”` (`includeDialogue`) | `Um, ` → `` | `“I don’t know.”` |
| `the um` U+2028 `next` | flag only | unchanged |
| `He waited um` ⏎ `Then` | flag only | unchanged |
| `“Um.”` | no match | unchanged |

Tests repeat each pattern with straight and curly quotes, curly apostrophes, `—` and `--`, `…` and
`...`, and U+2028, and give a flag-only case for each excluded context, as AGENTS.md requires.

### Clusters

Pauses and accepted commands separated only by spaces, punctuation and U+2028 form one cluster
with one edit, carried by the cluster's first command match (or by its pause when it has no
command):

- A cluster of one is the pause's edit above or the command's own edit.
- A larger cluster gets an edit only when its members are separated by plain spaces, every pause
  in it would be deletable on its own, it sits between words or at the paragraph edge, and its
  commands' marks can follow each other: repeats collapse (`comma um comma` gives one `,`), and two
  sentence marks never meet, even across closing quotes or brackets (`left period um comma then`,
  `fine period close quote comma he` and `left period close paren period then` are flag only).
  U+2028 between members isn't a plain space, so `left period` U+2028 `period the` is flag only as
  a whole rather than two edits that would each insert a `.`. The edit replaces the whole
  stretch with the marks, each spaced as it would be alone: `milk um comma eggs` → `milk, eggs`,
  `left period new paragraph the` → `left.` ⏎ `The`, `open quote fine period close quote and` → `“fine.” and`.
- Otherwise every member of the cluster is flag only, so a flag-only pause never leaves its
  neighbouring command's mark doubled or dropped.
- **A dictated pair of marks is edited together or not at all.** Pairs come only from
  `mark_pairs`: an opening command (*open quote*, *open single quote*, *open paren*, *open
  bracket*) pairs with the next command phrase of its kind in the paragraph when that phrase is a
  closing command, both passed every guard and mention filter, and at least one word lies between
  them that is neither a filled pause nor part of another command. A pair around nothing but
  pauses and punctuation would leave empty or punctuation-only marks, so it is no pair and both
  sides are flag only: `open paren uh close paren`, `open quote um comma close quote`, and with
  `includeDialogue`, `She said open quote um close quote and left`. The next phrase counts whether or not the guards kept it, so a rejected phrase in between
  leaves both sides unpaired (`open paren a open paren b close paren`), and so does another
  opening (`open quote wait open quote stop close quote` pairs only the second opening:
  `open quote wait “stop”`). **Any quote, paren or bracket command outside a pair is flag only**,
  opening or closing, because its edit would insert an unmatched mark: `He said hi close quote and
  left.`, `the words open quote and close quote appeared`, `open quote hi close paren`. In `he said
  open quote hi close quote and then open quote bye close quote` the second opening fails its guard
  (*then* reads as an adjective), so only the first pair converts. If either side's cluster is
  flag only, both are (`open quote fine period close quote comma he said` is flag only as a whole,
  because `period close quote comma` would put two sentence marks together).

### Capitalization

When a deletion or a dictated sentence-ending mark (*period*, *question mark*, *exclamation point*,
*full stop*) leaves a lowercase word at a sentence start, the edit extends over that word's first
letter and replaces it with its upper case: `Um, the` → `The`, `left period she` → `left. She`.
After a deletion, sentence start means the first word of a tokenizer sentence that doesn't follow a
closing quote; after a dictated mark, the mark itself starts the sentence. Where the text already
has the mark (`left period. she`), the tokenizer decides. The change applies only when `char::to_uppercase` returns exactly one char; otherwise the letter is left alone.
Without it, accepting the edit leaves a lowercase start that `UPPERCASE_SENTENCE_START` then flags.
This is the only edit that touches the author's words, and it changes nothing but case. A
camel-cased word (a capital after its first letter, as in *iPhone* or *eBay*) is never capitalized.
The drafting report recognizes capitalization after a dictated terminal command only when that
command has passed its guards; an ordinary noun such as *period* does not trigger it.

## Spoken commands

Spoken commands are detected only in **dictated text** (`dictated: true`, which turns
`spokenCommands` on unless it is set to `false`). A host marks text as dictated only when it knows
it came from speech-to-text, such as a dictation session or an imported transcript; typed prose is
never edited for command words. With commands off no command detection runs, no command edits are
returned and dictated quote pairs aren't found.

A command edit replaces the whitespace before the command and the command words with the mark.
An opening mark keeps the whitespace before it and consumes the whitespace after it. Apostrophes,
hyphens joining two lowercase words, dashes and paragraph breaks also consume following whitespace.
If the dictation engine typed the mark too (`said comma, he` or `milk, comma, and`), the command
and a duplicate mark after it are deleted.

**The edit builder alone decides what a command absorbs.** The function that builds a command's
edit either returns an edit that fully absorbs the marks typed beside the command, or says it
can't, and then the command is flag only, and so is its pair, its dialogue tag mark and its
cluster. No other table decides which marks count as absorbed. A *mark* here is anything but a
letter, digit or whitespace. The edit absorbs:

- **Before the command**, the same sentence mark (`,` `.` `?` `!` `:` `;` `—` `-`) typed right
  before it across spaces or tabs, which then stands in for the command's own
  (`tired, comma and` → `tired, and`). A spaced mark, one with whitespace before it, isn't
  absorbed, because the edit would close it on one side only (`It was a long day — dash the
  longest.`, `She turned — dash and ran.`, `It was a well - hyphen known fact.`, `well -hyphen
  known` and `She paused , comma and left.` are flag only). The edit consumes only spaces and tabs before a command, so any other whitespace
  between a typed mark and the command stays, and the command is flag only rather than doubling
  the mark (`She paused,` U+2028 `comma and left.`, the same with a no-break space, and
  `It was late —` U+2028 `dash she left.`). An opening mark keeps the whitespace before it, so
  any typed mark before it, across any whitespace, is not absorbed (`he said, open quote, hi`).
- **A line break after typed terminal punctuation or a closing quote.** A *new paragraph*, *new
  line* or *next line* command right after a typed `.`, `?`, `!`, `…` or closing double quote
  keeps the typed mark, becomes `"\n"`, and consumes the spaces on both sides: `He left. New
  paragraph The next day` → `He left.` ⏎ `The next day`, `“Go.” New paragraph She left.` →
  `“Go.”` ⏎ `She left.` (owner decision, 2026-10-26). After any other mark it is flag only
  (`it was over, new paragraph the next day`).
- **Never beside a paragraph separator.** A line-break command with nothing after it in the
  paragraph, or with U+2028 after it, is flag only, alone or in a cluster, so it never inserts
  `"\n"` next to an existing separator (`He left. New paragraph` ⏎ `She stayed.`, `“Go.” New
  paragraph`).
- **After the command**, only its own mark typed right after it with no space between
  (`late period. she` → `late. She`, `tired comma, and` → `tired, and`). An ellipsis command
  never absorbs a typed period: `I don't know dot dot dot.` and `He paused dot dot dot. Then he
  spoke.` are flag only, never `….`, and so is `She trailed off ellipsis.`.
- **An apostrophe beside one the engine typed** is flag only: the word before already ends in
  `'` or `’`, with or without an `s` after it (`the dog's apostrophe s bone`), or the `s` after
  the command has one (`the dog apostrophe s' bone`).

`absorbed_before`, `absorbed_after` and `command_edit` hold these rules. The guards only ask
whether the command's own mark is typed right before it (`same_mark_before`, shared with
`absorbed_before`), which separates it from the word before so the noun guard doesn't apply.

Everything else typed beside a command is flag only, for single- and multi-word commands alike:
`it was late period, she left`, `she left, full stop she never came back`, `where are you going,
question mark she asked`, `it was late question mark. she left`, `he was tired colon, the end`,
`he paused, dot dot dot then spoke`, `he said, open quote, hi, close quote`, `She said open quote
I don’t know. Close quote.` and `She said, "Open quote I don’t know close quote."`. A single-word
command after a typed mark other than its own is prose and not matched at all (`No more,
period.`).

| Command | Replacement | Guard |
|---|---|---|
| *comma*, *coma* | `,` | noun |
| *period*, *full stop* | `.` + capital | noun (*period*); not its own sentence |
| *question mark* | `?` + capital | noun |
| *exclamation point*, *exclamation mark* | `!` + capital | |
| *colon* | `:` | noun |
| *semicolon* | `;` | |
| *dash*, *em dash*, *m dash* | `—` | noun and verb (*dash*) |
| *hyphen* | `-` | noun |
| *ellipsis*, *dot dot dot* | `…` or `...` | noun (*ellipsis*) |
| *open quote*, *begin quote* | `“` or `"` | |
| *close quote*, *end quote*, *unquote* | `”` or `"` | not in *quote, unquote* / *quote-unquote* |
| *open single quote*, *close single quote* | `‘` / `’` or `'` | |
| *open paren*, *open parenthesis*, *close paren*, *close parenthesis* | `(` / `)` | |
| *open bracket*, *close bracket* | `[` / `]` | |
| *apostrophe* | `’` or `'`, no spaces | noun |
| *new paragraph*, *new line*, *next line* | `\n` | noun (*new line*) |
| *scratch that*, *delete that*, *strike that*, *undo that* | `""` (the command only) | |
| *caps on*, *caps off*, *all caps*, *no caps* | none (flag only) | |

- **Noun guard.** Every command is skipped when the
  previous word is a determiner, possessive, adjective, number or *of* (*a comma*, *the Edo period*,
  *a dash of salt*, *a new line of work*), when the next word is *of*, *in*, *for* or *between* (*a
  period of time*), or when it is the object of *made*, *took*, *put* or *used* (*made a dash*).
  *dash* is also skipped after *to* or a modal (*I have to dash*). *Period.* or *Full stop.* as its
  own sentence is emphasis, not a command.
- **Mentions.** A command word after *word*, *words*, *term*, *terms*, *phrase* or *phrases* is a
  mention (*The words open quote appeared*). After a verb of saying or writing (*say*, *said*,
  *says*, *saying*, *answered*, *replied*, *went*, *goes*, *typed*, *wrote*, *muttered*,
  *whispered*, *added*, …), a command is a mention unless a clause follows it (a pronoun,
  determiner, proper noun or capitalized word, or a pause before a pronoun or capitalized word) or
  it is one of a dictated pair of marks: `She typed new paragraph and hit enter.` and
  `I said question mark to myself.` are prose, `she said comma he left` and
  `she said open quote come in close quote` are dictation. A mention is no match, and no evidence
  for another command in the paragraph.
- **Dictated dialogue tag.** Two marks after a verb of saying are commands, not mentions, because
  they belong to dictated dialogue: *comma* right before a paired opening quote command
  (`she said comma open quote come in close quote` → `she said, “come in”`), and *period* after
  a closing quote when the paragraph end, a paragraph-break command or a paired opening quote
  command follows it. At the paragraph end the closing quote must be a paired closing command
  (`open quote I’m fine comma thanks close quote he said period` → `“I’m fine, thanks” he said.`);
  before a break or an opening quote it may also be typed, followed by at most one word and the
  verb: `open quote hi close quote he said period new paragraph open quote bye close quote she
  said period` → `“hi” he said.` ⏎ `“bye” she said.`, `… he said period new line She nodded` →
  `… he said.` ⏎ `She nodded`, and `“hello” she said period open quote hi close quote he said
  period` → `“hello” she said. “hi” he said.`. Each tag mark goes with the command it belongs
  to: the paired opening after the comma, the closing before a final period, or the break or
  opening after it. If that command is flag only, so is the tag mark. Every other command after
  a verb of saying keeps the mention rule (`He wanted the last word, so he said period.` is no
  match).
- **Positive cue.** Every single-word command needs evidence beyond its spelling: the next word
  opens a clause (a pronoun, determiner, proper noun, capitalized word, or *and*/*but*/*so* plus a
  clause), the words around a comma or colon form a list, a hyphen joins words, an apostrophe joins
  a suffix, the dictation engine typed the comma on both sides too (`again, comma, and`), the
  command ends the paragraph, or the paragraph has another command match. A list or compound needs
  a noun or adjective before the command that isn't the verb after a subject, so attributive uses
  stay prose (*has colon cancer*, *Teachers mark comma errors*, *hated hyphen words*). Multi-word
  commands keep the noun guard. A lone *period* in a well-punctuated paragraph is probably the noun.
  *Full stop* is ordinary English too (*I love you full stop.*), so it needs the same evidence as a
  single-word command; without it the match is flag only rather than dropped. So is a lowercase
  *ellipsis* followed by nothing but typed marks to the paragraph end (`She trailed off
  ellipsis.`, `She trailed off ellipsis…`), which matches `dot dot dot.`.
- **Sentence start.** A command at the start of a sentence, after any quotes, brackets or filled
  pauses, is ordinary prose (*Full stop signs*, *Um comma the dog ran*), except one of a dictated
  pair (`Open quote hello close quote she said`) and a line break before a capital
  (*He left new paragraph The next day*), which may also follow a typed ellipsis or closing quote
  (*He left… New paragraph The next day*). A capital there that the tagger reads only as a proper
  noun may continue the command words as prose, so it is no match (*She read the first line
  aloud. Next line Tom read.*); a missed break costs less than deleting the words.
  Mid-sentence, a capitalized command word is a name (*His brother Colon then left*).
- **Title case.** A multi-word command with any word after the first capitalized is a name or
  title, never a command: *She worked in film. New Line Cinema hired her.*, *“Where?” New Line
  Cinema, he said.*
- **Deletion command boundary.** *Scratch that*, *delete that*, *strike that* and *undo that* need
  punctuation immediately after *that* or the end of the paragraph; when followed by an object,
  they are ordinary prose. A subject or modal immediately before the phrase also blocks it, and so
  do verb uses: after *to* or a conjunction (*likes to scratch that*, *and delete that*), before
  `?` or `!`, or with *do*, *does*, *did* or a modal earlier in the sentence
  (*Did Mary delete that?*). A deletion command is deleted with the filled-pause patterns, so
  `spoke scratch that. Then` → `spoke. Then`, and other shapes are flag only.
- **Pause beside a command.** A pause and a command next to each other form one cluster with
  one edit (see [Clusters](#clusters)), carried by the command match: `I said uh question mark
  then` → `I said? Then`, and the `uh` match has no edit. If the pause is flag only, so is the
  command beside it.
- **Caps flags.** *all caps* and *no caps* after *in* or *of* describe lettering (*printed in all
  caps*); the command comes before the words it caps.
- **Marks follow the document, per kind.** Inserted double quotes follow the document's double
  quotes, counted as the consistency report counts `doubleQuotes`, and inserted single quotes
  follow its `singleQuotes`. A quote kind the document doesn't use falls back to the
  `apostrophes` count. An inserted apostrophe (one character, `’` or `'`) follows the
  `apostrophes` count, then `singleQuotes` (`He said 'no' twice. I don’t know. … The dog apostrophe
  s bone` → `dog’s`). Curly is the default with nothing to count, and curly wins a tie. So a manuscript with curly dialogue keeps curly inserted quotes beside straight
  apostrophes (`“I can't,” she said. … He said open quote fine close quote` → `He said “fine”`),
  and a dictated quote in text without quotes matches the engine's apostrophes: `she said open
  quote I don't know close quote and left` → `she said "I don't know" and left`. Ellipses follow
  the `ellipses` majority and default to `…`. Dashes are always a closed `—`.
- **`scratch that`** deletes only the command. The words it refers to have no recoverable extent
  in text, so the report never deletes them; the host's message can say so.
- **Case commands** can't become a fixed edit without re-casing words, so they are flag only.

### Dictated quotes are speech

Raw dictation of dialogue has no quote marks (`open quote I’m fine um close quote she said`), so
the document finds no speech span there. With `spokenCommands` on, the report pairs *open quote*
or *begin quote* with the next *close quote*, *end quote* or *unquote* in the same paragraph and
treats the words between as speech: filled pauses, restarts and self-corrections inside are skipped
unless `includeDialogue`, and matches inside have mode `dialogue`. The two commands are still
reported, with their edits, in every mode, and are edited together or not at all
([Clusters](#clusters)). An unpaired opening or closing command is flag only and marks no speech,
even where the author typed the other mark (`She said "I don't know close quote and left.`).

The report inserts only what was dictated. A missing comma before a dialogue tag (`“I’m fine” she
said`) is a separate problem the grammar rules cover.

## Placeholders

Flag only, in every mode: a placeholder marks missing content, so deleting it would lose the
author's reminder.

| Pattern | Counted when | Not counted |
|---|---|---|
| `TK`, `TKTK`, `TKTKTK` | a whole token, case-sensitive | *Tkachenko*, *atkins*, *T.K.*, *tk* |
| `TODO`, `FIXME`, `TBD`, `TBA`, `XXX` | a whole token, case-sensitive | *todo* in prose |
| `XX`, `XXX`, `XXXX` | not a Roman numeral | *Chapter XX*, *Chapter XXX*, *Book XX*, *Part XX*, *Act XX*, *Pius XX*, *XXX* after a name (*Henry XXX*), *XX chromosome*, lowercase *xx* |
| `[…]`, `{{…}}`, `<<…>>` notes | 1–5 words from the placeholder vocabulary (*name*, *place*, *city*, *date*, *something*, *X*, …), starting with *insert*, *add*, *check*, *fix*, *need*, *research* or *describe*, or ending in `?` | *[sic]*, *[redacted]*, *[inaudible]*, *[…]*, *[1]* |
| `(…)` notes to self | 1–4 words starting with one of those verbs, or ending in `?`: *(check this)*, *(name?)*, *(?)*, *(TK)* | ordinary parentheticals |
| prefix notes | a paragraph starting *Note:*, *NOTE:*, *Note to self:*, *NTS:* or *TODO:* | |
| `???` | three or more `?` standing alone or inside brackets | *What???* |

Scene breaks (`#`, `###`, `* * *`, `***`) are never placeholders. Notes to self such as *(check
this)* are flag only, like *TK*. Placeholders are a report category only in v1. A live `check()`
builtin (`BUILTIN/DRAFT_PLACEHOLDER` in the fiction profile) is possible later, because they need no
speech logic, but isn't planned. If the speller already flags *TK*, the implementation records which
flag the host should show.

The vocabulary inside bracketed and parenthesized notes is case-insensitive: `[name]`, `[Name]`
and `[NAME]` are equivalent. Standalone placeholder marks remain case-sensitive.

## Filled pauses

The default list is *um*, *uh*, *uhm*, *erm*, *ehm* and their stretched forms, as whole word tokens
matching `^u+m+$`, `^u+h+$`, `^u+h+m+$`, `^e+r+m+$`, `^e+h+m+$`. Each is deleted in the patterns
above and flagged with no edit everywhere else.

- **Not listed:** *ah*, *eh*, *ha*, *hm*, *hmm*, *mm*, *mmm*, *oh*. These are deliberate fiction
  interjections (*Ah, there you are*, *Hmm. She frowned.*).
- ***er*** is off by default and behind the `er` option. Comic narration uses it on purpose (*He was,
  er, indisposed*). When on, it counts only between commas or dashes or before an ellipsis.
- **Guards** (both modes): skip
  - all-caps tokens (*UM*, *UH*, *ER* are names and abbreviations);
  - **names**: a capitalized pause word directly before another capitalized word other than *I*,
    whatever the spelling (*Um Kulthum sang*, *Umm Qasr*), and a capitalized pause word that isn't
    the first word of a tokenizer sentence (*They listened to Umm Kulthum*), unless it follows
    another pause with only spaces and pause punctuation between: `Um, Umm, the door.` is one
    two-pause match (flag only, since the run has a comma inside);
  - **interjections**: hyphenated forms (*uh-huh*, *uh-oh*, *um-hm*) and the same pairs written with
    a space, comma or dash, in any case: *uh oh*, *uh huh*, *Uh, oh*, *um hm*, *um hmm*;
  - **a pause word reported or used as a noun**: right after a verb of saying (the list under
    *Mentions* in [Spoken commands](#spoken-commands): *She said um and looked away*, *Had he
    really just said um?*, *The baby went uh uh uh*), after *an* (*with an um and a shrug*), and
    after another article or possessive determiner (*a*, *the*, *my*, *your*, *his*, *its*, *our*,
    *their*, *whose*, *every*, *each*, *another*, a word ending in *'s*) unless a noun or adjective
    follows, which gives the determiner its own head (*his um more than his silence* is a noun; *to
    the um store* is a pause). Every pause right after a reported one, with only spaces and pause
    punctuation between, is reported too (*He said um--uh--no*). Words that also stand alone are
    left out of the determiner list, because dictated pauses follow them: *her* (*I told her um
    the*), *this*, *that*, *some*, *any* and *no*;
  - **a pause word after a determiner and an adjective**, which makes the pause their noun
    (*He let out a long uhhhh.*, *He let out a long uhhhh and sat down.*, *a long uhhhh that
    filled the room*, *She heard a faint um from the back row.*, *a loud uh of surprise*), unless
    a noun follows (*a long um day* is still a pause) or the next word, maybe after *I mean*, is
    itself a determiner from the list above or an adjective that isn't also a verb or adverb,
    which makes it a restart (*She grabbed the red um the blue one.*, *She picked a big um big
    one.*, *It was a good um I mean a great day.*) that pattern (a) deletes in dictated text;
  - **a pause word as the subject of a sentence naming it**: at a sentence start or after *and*,
    *but* or *or*, then *is* or *was* (after at most one comma-enclosed aside of one to three
    words), then an article, a possessive or *one*, optionally after *not* or *never*: *and um was
    her favorite word*, *Um, she thought, was not a word.* A subject before the pause (*I um was
    going*), no article after the verb (*and um was going to leave*) or a question (*Um, he said,
    was the door open?*) keeps the pause. One shape is ambiguous, *and um was the best*, where a
    dictated filler is possible but rare; it is skipped, since a missed flag costs less than a
    false one;
  - skipped names, and speech or quoted text unless `includeDialogue` (see [Speech](#speech)).

## Restarts (v1.1)

A restart is a phrase of 2 to 6 words immediately repeated: `he walked to the he walked to the car`.
The first copy is deleted, with the whitespace after it, and the second copy's first letter is
capitalized if the first copy began the sentence (`He walked to the h` → `H`).

- Compare word forms after case and apostrophe folding, not lemmas: *walked* then *walks* is a
  substitution, not a restart.
- Both copies are in one sentence, with only whitespace between them, or one filled pause that the
  restart's edit absorbs. *She ran. She ran until…* and *She ran, she ran until…* are deliberate and
  never match.
- The phrase holds a closed-class word (determiner, preposition, *to*, pronoun, possessive, modal,
  conjunction, or a form of *be*, *have* or *do*), or is at least 3 words. This skips *tick tock
  tick tock* and *left right left right*.
- Longest phrase wins, matches don't overlap, and a phrase repeated three or more times is a chant,
  not a restart.
- One word repeated (*no no*, *the the*) is `ENGLISH_WORD_REPEAT_RULE`'s and isn't reported here.
- The repeated-phrases lens ([STYLE-ECHOES.md](STYLE-ECHOES.md#repeated-phrases)) counts phrases
  repeated across the document; drafting flags one adjacent seam with an edit.

## Self-corrections (v1.2)

*I mean*, *no wait*, *wait no*, *let me rephrase*, *sorry I mean*, *actually no*. Flag only:
deleting the phrase alone leaves both versions (`it was red crimson`), and how far back the
corrected words go can't be recovered from text.

Written prose punctuates these (`It was red—I mean, crimson`); dictation leaves them bare. So they
count only mid-sentence in narration with no punctuation on either side, and *I mean* not before
*it*, *that*, *to* or *what*. *or rather* is left out: it is ordinary narration (*he walked, or
rather staggered*), and `bench/public` has three.

## Speech

- Filled pauses, restarts and self-corrections are skipped where `Document::mode_at(start)` is
  dialogue, or inside a dictated quote pair, unless `includeDialogue`.
- **Text inside any quotes**, speech or not (scare quotes, titles, written text, single quotes:
  *They called it “the um thing”*, *He wrote “I was um going”*, *He said 'I was um going'*), is
  skipped the same way, and with `includeDialogue` a pause there is flag only unless the document
  detected the quote as speech: the quoted words are the author's verbatim wording.
- **The report's own speech rule.** The document finds speech only between an opening and a closing
  quote, so two more cases count as speech for drafting: the text from an opening double quote that
  never closes in its paragraph to the paragraph end (`She said, “I was going home and um the door
  was open.`, the first paragraph of a multi-paragraph speech, or a draft's last line), and a
  paragraph that opens with an em dash (or `--`) before a word, which is dialogue set with dashes
  (`—Um, I don't know, she said.`). The whole paragraph counts, tag included, which only costs
  recall. Matches there have mode `dialogue`, and with `includeDialogue` they are flag only, because
  the rule is a heuristic. The document's speech detection, shared with every report, is unchanged.
- Commands and placeholders are reported in every mode.
- **A quoted word on its own is never flagged.** When a quoted span, speech or not, holds only a
  listed word or command (*the word “um”*, *she typed “period”*, *‘TK’ is journalese*), it is a
  mention, not a leftover.

## Overlap

- **Grammar flags.** `INTERJECTIONS_PUNCTUATION` (a comma after a sentence-initial *Umm*) and
  `DT_PRP` and `DT_DT` (at a restart seam: *to the he walked*) can fire on spans a drafting edit
  covers. When both run, hosts should prefer the drafting edit and hide a grammar flag whose span
  overlaps it. After the edit is applied, the next check settles whatever remains.
- **Other reports.** Vague words and overused words own talk-style fillers, and the repeated-phrases
  lens owns document-wide repeats, so no word is reported by two lenses for the same reason.

## Data

Four project-authored files in `style/data/drafting/`, MIT OR Apache-2.0 like the engine:
`fillers.txt`, `commands.tsv` (`phrase<TAB>replacement key<TAB>guard`), `placeholders.tsv` and
`self-corrections.tsv`. Command names are the ordinary English names of the marks. Apple, Microsoft,
Nuance and Talon command lists were used to check coverage only. Nothing is vendored, so no NOTICE
entry is needed.

The command replacement key selects a fixed mark, the document-majority quote or ellipsis form,
`newline`, `delete` or `flag`; it is resolved by the report rather than copied into an edit.

## Evaluation

- **Precision on published fiction.** `python3 bench/style/drafting.py --precision` (modelled on
  `vague_hidden_verbs.py`) runs `bench/public` and the 13 public-domain books in
  `bench/style/sources.json` and prints matches, matches with edits, and matches per 1,000 words
  per category and mode. Expect near zero, and zero matches with edits. The books are the
  adversarial set: *Chapter XX*, *er* in Victorian dialogue, *at that period*, *made a dash for*.
  On story1–5 every flag must be judged correct. Hand-judge a held-out
  `--sample 50` per category, drawn from each mode: the default-mode sample goes to
  `out/drafting/sample.jsonl` and the dictated-mode sample, with matches that have edits drawn
  first, to `out/drafting/sample-dictated.jsonl`. Dictated-mode matches with edits are printed per
  category and must be 0 on published text; a category that falls below 0.95 (restarts 0.9) ships off by default.
  The owner can add drafting to the tier 3 flag-rate sweep for published rates.
- **Recall on fixtures.** `bench/style/drafting_fixtures.jsonl` holds self-written paragraphs as
  `{id, style, category, input, expected}`, plus `flags` for flag-only matches, `options` where
  `dictated` or `includeDialogue` is needed (every fixture with an edit, and every command fixture
  with a match, sets `dictated`; default-mode fixtures expect the input unchanged, with `flags`)
  and a `note` where the doc is silent, covering each category in
  narration and dialogue, in Dragon-, Apple- and Whisper-style output (misheard *coma*, lowercase
  after *period*, doubled marks, kept or dropped fillers), and every typographic variant above.
  Each guard has a negative twin (*a comma*, *the Edo period*, *Chapter XX*, *She ran. She ran*,
  *quote, unquote*, *[sic]*, *uh-huh*). The last 49 fixtures are cleanup edge cases written from
  these rules rather than from the current report: adjacent pauses, comma-plus-ellipsis and
  comma-plus-dash shapes, punctuation orphaned before a pause, discourse-word comma pairs,
  Apple/Whisper filler, U+2028 separators, case-insensitive `[Name]` notes, `Chapter XXX` as a
  numeral, quoted mentions and commands next to pauses. After the 2026-10-26 decision, fixtures
  whose punctuation is no longer an edit pattern expect the input unchanged plus a `flags` entry,
  with a note citing decision 2, and the inputs from the D2g review are fixtures too. After the D2h
review, the pattern (d) fixtures are flag only with a note citing that decision, and the D2h
inputs are fixtures, or negatives where they are ordinary prose (*Umm Kulthum*). After the D2j
review, fixtures that changed under its guards carry a note citing it, and its inputs are fixtures
in both modes, or negatives (*She said um and looked away.*, *Uh oh, he thought.*,
*They called it “the um thing”*). After the D2k review, its inputs and the dictated `rv5`–`rv8`
and command `rv3` probe lines are fixtures with `dictated` (pair, typed-mark, dialogue-tag and
mark-style cases), or negatives (*He let out a long uhhhh.*, *Um, she thought, was not a word.*,
*He wanted the last word, so he said period.*). After the D2l review, its inputs are fixtures
with `dictated` (absorption across U+2028 and no-break spaces, spaced dashes, ellipsis before a
typed period, pause-only pairs, the dialogue-tag period before a break or quote, restarts and
per-kind mark style), and `He left. New paragraph The next day` expects a line break, with a note
citing the owner's decision and its old flag-only value. After the D2m review, its 164 new
inputs are fixtures with `dictated` (title-cased commands, breaks at the paragraph end or after
U+2028, spaced typed marks, typed possessives, apostrophe style) or negatives (*New Line Cinema*,
*He let out a long uhhhh and sat down.*), plus the U+2028 break cases; the negatives are also
fixture controls in both modes. An `expected`
  value is derived from the rules here, never copied from the report's output. Scoring
  is exact equality of the text after `drafting::apply` per category and style, with no flags on a
  negative; this catches orphan commas and spacing bugs that span scores miss.
- **Private draft, owner only.** `bench/corpus/` holds a partly dictated draft. The owner runs, on
  their machine,
  `python3 -I bench/style/drafting.py --local-dir bench/corpus/<draft-directory>`, which prints
  only aggregate counts per category and mode and never text, titles or paths. The owner judges the
  flags locally. Nothing leaves the machine: no `parity.py` API leg, no `lt_api.py`, no agent or
  LLM. Only aggregate numbers (flags per category, judged precision) are published. Workers never
  read `bench/corpus/`.
- **Gates.** `python3 bench/style/drafting.py --gate` enforces the per-category recall thresholds
  and clean-control rate in `bench/style/drafting-baseline.json`, requires every line of
  `drafting_negatives.jsonl` to produce no match at all in default mode and with `dictated`, and
  requires default mode to return no edit for any fixture or negative. `--precision` runs the
  published text in both modes. Every threshold is 1.0:
  each fixture's expected text, including a flag-only `flags` entry, must match exactly. The
  report is style-only, so conformance and `recall.py`
  should be unchanged; the PR confirms it with `bench/tools/gates.sh`. The tagger, speller and rule
  loading are untouched, so the RSS gate doesn't apply. Report check time on moby150k and the WASM
  size change.

## Out of scope

- Generating or reordering words, beyond the first-letter case change above.
- Deleting what *scratch that* refers to, or a self-correction's first version.
- Substitution repairs and restarts without an exact repeat (*I went to the I drove to the store*).
- Abandoned sentences, talk-style fillers, ML detectors.
- Missing dialogue punctuation and other grammar the rules already cover.
- Tuning for one dictation engine.
- A live `check()` rule for placeholders in v1.
