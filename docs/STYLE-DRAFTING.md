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
[STYLE-REPORTS.md](STYLE-REPORTS.md#decisions).

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
| v1 | `spokenCommand` | the mark, a line break, or a deletion | all |
| v1 | `filledPause` | deletion | narration (`includeDialogue` opts in) |
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
| `spokenCommands` | `true` | Report spoken commands, with edits. |
| `filledPauses` | `true` | Report filled pauses, with deletion edits. |
| `er` | `false` | Also report *er* as a filled pause (guarded; see below). |
| `restarts` | `true` | Report restarts (from v1.1). |
| `selfCorrections` | `true` | Report self-correction phrases, flag only (from v1.2). |
| `includeDialogue` | `false` | Also report filled pauses, restarts and self-corrections inside speech. Hesitation in dialogue is usually characterization. It never affects commands or placeholders, which are reported in every mode. |

An option for a category that hasn't shipped is an unknown key until it does.

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
  overlap. The report resolves collisions before returning: a filled pause inside a restart's first
  copy is folded into the restart, and when two matches would widen into the same whitespace, the
  later one gives up the shared character. A host can apply any subset in descending start order.
- **Spans come from tokens.** Byte offsets are taken from token spans and from scanning whole chars,
  so they are always char boundaries. Nothing slices text by arithmetic.
- **`drafting::apply(text, edits)`** sorts edits by start, skips any that overlap an earlier one,
  checks char boundaries, and returns the edited text. Tests, the bench script and the CLI use it.
  It isn't bound, because hosts apply UTF-16 ranges themselves.

### Deletion cleanup

Filled pauses, restarts and `scratch that` delete words. The edit widens the deleted words (the
core) so the text reads as if they were never there:

1. **Trailing pause punctuation.** Absorb one `,`, an ellipsis (`…`, `...`) or a dash (`—`, `–`,
   `--`) right after the core, with or without a space. Never absorb `.`, `?`, `!`, `;`, `:` or a
   closing quote or bracket: those belong to the sentence.
2. **Comma pairs.** When a comma comes right before the core and one was absorbed after it
   (`X, um, Y`):
   - after a sentence-initial discourse word or interjection (*Well*, *Oh*, *So*, *Yes*, *No*,
     *Okay*) or a vocative name at the sentence start, keep the first comma: `Well, um, I` → `Well, I`;
   - otherwise drop both: `I was, uh, thinking` → `I was thinking`.

   If the right context is a closing quote or closing punctuation, keep the first comma and don't
   absorb the second: `“I was, um,” she said` → `“I was,” she said`.
3. **Dash pairs.** A dash before and after the core goes with it, and the replacement is one space:
   `I was—um—thinking` → `I was thinking`.
4. **Whitespace.** Keep exactly one separator between the left and right context. Keep none when
   the right context starts with closing punctuation (`.,;:!?…”’)]`) or the left context is the
   paragraph start, an opening quote, `(` or `[`. If a collapsed whitespace run holds a U+2028,
   keep the U+2028 instead of a space.
5. **Paragraphs.** Never delete across a paragraph boundary (`\n`, `\r`, U+2029).
6. **Capitalization.** See below.

| Input | Edit span → replacement | Result |
|---|---|---|
| `So um I was` | `um ` → `` | `So I was` |
| `Well, um, I was` | `um, ` → `` | `Well, I was` |
| `I was, uh, thinking` | `, uh,` → `` | `I was thinking` |
| `I was—um—thinking` | `—um—` → ` ` | `I was thinking` |
| `I was… um… thinking` | `um… ` → `` | `I was… thinking` |
| `Um, the door` | `Um, t` → `T` | `The door` |
| `Um… the door` | `Um… t` → `T` | `The door` |
| `He left um.` | ` um` → `` | `He left.` |
| `He left um”` (dialogue, `includeDialogue`) | ` um` → `` | `He left”` |
| `“I was, um,” she said` (`includeDialogue`) | `, um` → `` | `“I was,” she said` |
| `“Um, I don’t know.”` (`includeDialogue`) | `Um, ` → `` | `“I don’t know.”` |
| `the um` U+2028 `next` | ` um` → `` | `the` U+2028 `next` |
| `He waited um` ⏎ `Then` | ` um` → `` | `He waited` ⏎ `Then` |
| `“Um.”` | no match | unchanged |

Tests repeat each case with straight and curly quotes, curly apostrophes, `—` and `--`, `…` and
`...`, and U+2028, as AGENTS.md requires.

### Capitalization

When a deletion or a dictated sentence-ending mark (*period*, *question mark*, *exclamation point*,
*full stop*) leaves a lowercase word at a sentence start, the edit extends over that word's first
letter and replaces it with its upper case: `Um, the` → `The`, `left period she` → `left. She`.
Sentence start means after the paragraph start, terminal punctuation or an opening quote. The change
applies only when `char::to_uppercase` returns exactly one char; otherwise the letter is left alone.
Without it, accepting the edit leaves a lowercase start that `UPPERCASE_SENTENCE_START` then flags.
This is the only edit that touches the author's words, and it changes nothing but case.

## Spoken commands

A command edit replaces the whitespace before the command and the command words with the mark.
Spacing after the mark stays as the author left it, except that an opening mark also takes the
space after it. If the dictation engine typed the mark too (`said comma, he`), the replacement is
`""`.

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

- **Noun guard.** A single-word command (and *new line*, *question mark*) is skipped when the
  previous word is a determiner, possessive, adjective, number or *of* (*a comma*, *the Edo period*,
  *a dash of salt*, *a new line of work*), when the next word is *of*, *in*, *for* or *between* (*a
  period of time*), or when it is the object of *made*, *took*, *put* or *used* (*made a dash*).
  *dash* is also skipped after *to* or a modal (*I have to dash*). *Period.* or *Full stop.* as its
  own sentence is emphasis, not a command.
- **Positive cue.** A single-word command also needs one of: the next word opens a clause (a
  pronoun, determiner, proper noun, capitalized word, or *and*/*but*/*so* + pronoun), the command
  ends the paragraph, or the paragraph has another command match. A lone *period* in a
  well-punctuated paragraph is probably the noun.
- **Marks follow the document.** Quotes and ellipses follow the document's majority, counted as the
  consistency report counts them (`doubleQuotes`, `singleQuotes`, `ellipses`), and default to curly
  quotes and `…` on a tie or with none to count. Dashes are always a closed `—`.
- **`scratch that`** deletes only the command. The words it refers to have no recoverable extent
  in text, so the report never deletes them; the host's message can say so.
- **Case commands** can't become a fixed edit without re-casing words, so they are flag only.

### Dictated quotes are speech

Raw dictation of dialogue has no quote marks (`open quote I’m fine um close quote she said`), so
the document finds no speech span there. The report pairs *open quote* or *begin quote* with the
next *close quote*, *end quote* or *unquote* in the same paragraph and treats the words between as
speech: filled pauses, restarts and self-corrections inside are skipped unless `includeDialogue`,
and matches inside have mode `dialogue`. The two commands are still reported, with their edits, in
every mode. An unpaired command still gets its edit but marks no speech.

The report inserts only what was dictated. A missing comma before a dialogue tag (`“I’m fine” she
said`) is a separate problem the grammar rules cover.

## Placeholders

Flag only, in every mode: a placeholder marks missing content, so deleting it would lose the
author's reminder.

| Pattern | Counted when | Not counted |
|---|---|---|
| `TK`, `TKTK`, `TKTKTK` | a whole token, case-sensitive | *Tkachenko*, *atkins*, *T.K.*, *tk* |
| `TODO`, `FIXME`, `TBD`, `TBA`, `XXX` | a whole token, case-sensitive | *todo* in prose |
| `XX`, `XXXX` | not a Roman numeral | *Chapter XX*, *Book XX*, *Part XX*, *Act XX*, *Pius XX*, *XX chromosome*, lowercase *xx* |
| `[…]`, `{{…}}`, `<<…>>` notes | 1–5 words from the placeholder vocabulary (*name*, *place*, *city*, *date*, *something*, *X*, …), starting with *insert*, *add*, *check*, *fix*, *need*, *research* or *describe*, or ending in `?` | *[sic]*, *[redacted]*, *[inaudible]*, *[…]*, *[1]* |
| `(…)` notes to self | 1–4 words starting with one of those verbs, or ending in `?`: *(check this)*, *(name?)*, *(?)*, *(TK)* | ordinary parentheticals |
| prefix notes | a paragraph starting *Note:*, *NOTE:*, *Note to self:*, *NTS:* or *TODO:* | |
| `???` | three or more `?` standing alone or inside brackets | *What???* |

Scene breaks (`#`, `###`, `* * *`, `***`) are never placeholders. Notes to self such as *(check
this)* are flag only, like *TK*. Placeholders are a report category only in v1. A live `check()`
builtin (`BUILTIN/DRAFT_PLACEHOLDER` in the fiction profile) is possible later, because they need no
speech logic, but isn't planned. If the speller already flags *TK*, the implementation records which
flag the host should show.

## Filled pauses

The default list is *um*, *uh*, *uhm*, *erm*, *ehm* and their stretched forms, as whole word tokens
matching `^u+m+$`, `^u+h+$`, `^u+h+m+$`, `^e+r+m+$`, `^e+h+m+$`. Each is a deletion with the cleanup
above.

- **Not listed:** *ah*, *eh*, *ha*, *hm*, *hmm*, *mm*, *mmm*, *oh*. These are deliberate fiction
  interjections (*Ah, there you are*, *Hmm. She frowned.*).
- ***er*** is off by default and behind the `er` option. Comic narration uses it on purpose (*He was,
  er, indisposed*). When on, it counts only between commas or dashes or before an ellipsis.
- **Guards:** skip all-caps tokens (*UM*, *UH*, *ER* are names and abbreviations), hyphenated forms
  (*uh-huh*, *uh-oh*, *um-hm*), skipped names, and speech unless `includeDialogue`.

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
  `bench/style/sources.json` and prints matches per 1,000 words per category and mode. Expect near
  zero. The books are the adversarial set: *Chapter XX*, *er* in Victorian dialogue, *at that
  period*, *made a dash for*. On story1–5 every flag must be judged correct. Hand-judge a held-out
  `--sample 50` per category; a category that falls below 0.95 (restarts 0.9) ships off by default.
  The owner can add drafting to the tier 3 flag-rate sweep for published rates.
- **Recall on fixtures.** `bench/style/drafting_fixtures.jsonl` holds self-written paragraphs as
  `{id, style, category, input, expected}`, plus `flags` for flag-only categories, `options` where
  `includeDialogue` is needed and a `note` where the doc is silent, covering each category in
  narration and dialogue, in Dragon-, Apple- and Whisper-style output (misheard *coma*, lowercase
  after *period*, doubled marks, kept or dropped fillers), and every typographic variant above.
  Each guard has a negative twin (*a comma*, *the Edo period*, *Chapter XX*, *She ran. She ran*,
  *quote, unquote*, *[sic]*, *uh-huh*). The last 49 fixtures are cleanup edge cases written from
  these rules rather than from the current report: adjacent pauses, comma-plus-ellipsis and
  comma-plus-dash shapes, punctuation orphaned before a pause, discourse-word comma pairs,
  Apple/Whisper filler, U+2028 separators, case-insensitive `[Name]` notes, `Chapter XXX` as a
  numeral, quoted mentions and commands next to pauses. Several are expected to fail until the
  cleanup fixes merge; their `expected` text must not be changed to match current behavior. Scoring
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
  and clean-control rate in `bench/style/drafting-baseline.json`. They are recorded at the current,
  lower recall because the cleanup fixes and spoken commands are still on other branches, and rise
  to the measured value when those merge. The report is style-only, so conformance and `recall.py`
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
