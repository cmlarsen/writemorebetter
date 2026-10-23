# Voices: narration, speech, thought and caller-marked spans

#353 adds a voice layer to the style document. Every byte of a document resolves to one `Voice`:
narration, speech, thought, a quoted term, or a span the caller marks as embedded text, a
quotation or excluded. Lenses that compare narration with its frame (person drift #354, tense
shift #355) read it to leave out everything that is not narration. A new opt-in `voices` report
lists what the layer decided. Tracking issue: #352.

`Mode`, `mode_at`, `sentence_mode`, `dialogue`, `speech` and `DocumentStats` are unchanged. No
existing report, baseline, golden file or `usefulness-counts.json` moves:
`style/tests/cross_report.rs` checks every Mode-based report with detection on, off and with
caller spans.

## Taxonomy

The categories follow Semino and Short's speech, writing and thought presentation scale:

| Category | Example | Voice |
|---|---|---|
| Direct speech (DS), quoted | *“I’m done,” she said.* | `speech` (source `quote`) |
| Direct speech, unquoted with a tag | *Get out, he said.*, Joyce's *―I don’t know, Stephen said.* | `speech` (source `tag`) |
| Direct thought (DT), quoted | *“Fine,” he thought.* | `thought` (source `quote`) |
| Direct thought, unquoted with a tag | *I’m done with this, she thought.* | `thought` (source `tag`) |
| Free direct thought (FDT) next to a tagged thought | *Where is he? Why doesn’t he call? I wondered.* | `thought` (one span) |
| Indirect thought and speech (IT, IS) | *She thought that he was late.* | narration |
| Free indirect thought and speech (FIT, FIS) | *She had to get out. Now.* | narration |
| A quoted term, title or sign | *She pressed “Send.”* | `quoted` |
| A letter, note, block quote or epigraph | — | `embedded` / `quotation`, caller only |

Free indirect discourse stays narration on purpose: it is in the narrator's person and tense, so a
person or tense lens should see it.

## Where spans come from

1. **Quote detection** (the existing `dialogue` and `speech`): a speech span is `speech`, a
   non-speech quoted span is `quoted`. A speech span whose following tag has *think* or *wonder*,
   or a reflexive verb with a reflexive (*told herself*, *said to himself*), becomes `thought`.
   The other thought verbs (*conclude, muse, decide, ponder, reflect*, …) do not relabel a quote:
   after a quote they mostly tag spoken lines (*“I’ll leave you,” he concluded.*). One `,` `?` `!`
   `—` `–` outside the closing quote is skipped (*“Damn”, she thought.*). When the tag is followed
   by `,` `—` `–` or `:` and a second quote (*“Oh,” she thought, “it has come to this.”*), both
   halves become `thought`. `mode_at` still says dialogue there, and `speech` is unchanged.
2. **Unquoted thought**: a precision-first detector anchored on a tag (below).
3. **Unquoted speech**: the same anchors with a list of reporting verbs, giving `speech` with
   source `tag`. This replaces #354's planned local guard.
4. **Caller spans**: `DocumentOptions::marked_spans`, in byte, char or UTF-16 units, with a
   caller-only `MarkedVoice` (no `quoted`, so `"quoted"` is a JSON error).

Resolution priority, highest first: caller spans (the innermost, then the later index, wins;
a caller `narration` span carves narration out of everything below it), quote speech or thought,
tag-detected thought and speech, quoted terms, narration. Tag detections never overlap a quote or
caller span and never cross a paragraph.

## Detection

The detector runs once per paragraph in `analyze_document`, after quotes and speech flags exist
and before compact mode drops readings, so compact and full documents get identical voices.

**Tag.** A subject (*I, he, she, we, they, you*, a skipped name, up to three proper-name tokens
with an optional honorific or title noun such as *Aunt Hester*, or a determiner, up to two
modifiers and a noun), an optional adverb, and a past or present verb whose lemma is in the
lexicon. Inversion (*thought Ahab*, *said the old man*) is allowed after the clause; a noun subject
inverts only after a reporting verb. VBN/VBG forms, auxiliary or modal frames (*had thought*) and
the noun *a thought* are not tags. Up to four extension tokens follow (*again*, *suddenly*,
*then*, *for a moment*, adverbs). An indirect guard rejects a tag followed by *that, if, whether,
how, what, why, where, when, who, which, about, of, to, so, it, back, over, up, through, aloud* or
*out*. The tag must end at the sentence end, at linking punctuation followed by narration, or, for
a leading tag, at a comma, colon or dash before its clause.

**Lexicons** (`style/data/thought/`, project-authored, MIT OR Apache-2.0): `thought-verbs.txt`
(*think, wonder, muse, reason, realize, realise, decide, figure, suppose, reflect, pray, ponder,
calculate, conclude, reckon*) and `reflexive-verbs.txt` (*ask, tell, remind, promise, say,
whisper, mutter, murmur, repeat, warn, assure, scold*, only with a reflexive or *to* plus a
reflexive). *Knew, felt, believed, remembered, hoped* and similar verbs are left out: they mostly
introduce indirect thought. *Reflect* counts only after its clause. Unquoted speech uses
`unquoted-speech-verbs.txt`, 41 reporting verbs taken from the dialogue-tag speech list (*say, ask,
reply, whisper, mutter, shout, insist, snap, add, …*). Sound and action verbs (*cry, howl, bark,
roar, sob, groan, sing, echo, scream*) and verbs that also name a non-speech act (*read, continue,
agree, admit, return, observe*) are left out: without quote marks, *I walked the pier, the gulls
cried.* and *I said nothing, he continued.* are first-person narration, not speech. In a
paragraph that opens with a dialogue dash (`―`, `—`, `–`, Joyce's *―Come along, the professor
cried.*) the whole single-word dialogue-tag speech list applies, less the thought verbs and
*call* (which names more often than it speaks: *the sailor, called the mincer*). Outside such a
paragraph, a speech tag whose subject is a determiner and noun (*the old man whispered*, but
also *the wind whispered*) anchors only on a tense shift, not on a person shift alone.

**Anchors.**

- A, trailing: *clause* + `,` `?` `!` `—` `–` `…` + *tag*. An ASCII `...`, which the tokenizer
  splits into three `.` tokens, counts as `…` here, in B and in thought evidence, and is not a
  missed sentence end inside a clause.
- B, bare after a split: a sentence ending in `?` `!` `…` `...`, then a sentence that is only a
  tag (*Where is he? I wondered.*). U+2028 between them is allowed; a newline, CRLF or U+2029 is a
  paragraph break and stops it.
- C, leading: a sentence-initial tag + `,` `:` `—` + *clause* to the sentence end.
- D, mid: *clause* + `,`/`—` + *tag* + `,`/`—` + *clause*. The second clause joins only when it
  starts lowercase or with first-person *I* (*I can’t do this, she thought, I won’t.*), is not a
  coordinator or a participle or past predicate (*she thought, ladling
  out soup*), and carries the shift itself.

Unquoted speech uses A (comma and dashes only), C and D, never B and never expansion.

**Disagreement.** A clause is thought (or speech) only when it disagrees with its tag:

- person: a third-person tag and a first-person pronoun in the clause (a Roman-numeral or letter
  *I* does not count);
- tense: a past tag and a clause whose first finite chain is present or a present modal, or a
  clause that opens with an imperative.

The shift is recorded as `person`, `tense` or `both`. An agreeing, verbless or pronoun-free
clause stays narration (*She was a fool, she thought.*, *Idiot, she thought.*). A clause that
opens in the frame (*He grunted, or, rather, I might say, he barked.*) is narration. Second-person
tags never anchor; present tags anchor only on a person shift; *I think*, *I suppose* and *they
say* are hedges and never anchor.

**Expansion** (thought only). From an A or B anchor whose clause starts its sentence, the span
walks back; from a C anchor whose clause ends its sentence, it walks forward. A neighbouring
sentence joins only if it passes the disagreement test by the same path, has no past chain (for a
tense shift) or no third-person subject pronoun that could be the tag's subject (for a person
shift), and carries thought evidence: it ends in `?` `!` `…`, has a first-person pronoun, or opens
with an imperative. At most `MAX_EXPANSION` = 2 sentences join. A plain present declarative
(*The room is dark.*) next to a tagged thought therefore stays narration, so #355 still sees it.

**Clause bounds.** A trailing clause runs back to the sentence start, or to the last quote,
caller span, `;` `:` `—` `–`, unclosed `(`, missed sentence end (not after *Mrs.*) or earlier tag
in the sentence. A clause that began inside a parenthesis that has closed is narration.

## Exemptions

Every exemption in the #353 spec holds, including: free indirect discourse, indirect thought and
speech, excluded verbs, agreeing clauses, verbless clauses, present first-person hedges, untagged
present generalisations and questions, VBN/VBG/modal tag forms, the noun *thought*, second-person
and *it*/*one* subjects, curly apostrophes in contractions and elisions (*I’m*, *’Tis*, *’em* are
tokens, not quote marks), the Roman-numeral *I*, and the expansion cap. Italics and markup are
never read as thought; letters, signs, block quotes and epigraphs come only from caller spans.

## The lens-author API

```rust
let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
document.voice_at(byte);                 // Voice, binary search
document.voice_span_at(byte);            // Option<&VoiceSpan>
document.is_narration_at(byte);          // voice_at == Narration: every quoted span is excluded
document.in_voice_tag(byte);             // inside a tag-source span's tag (narration, but skip it)
document.sentence_narration_words(&s);   // Sentence::words() that are narration
document.narration_runs(&s);             // maximal runs of token indexes whose words are narration
document.sentence_is_narration(&s);      // the sentence's first word is narration
```

`Document::voices` holds the resolved `VoiceSpan`s (`span`, `voice`, `source`, and `tag` and
`shift` for a tag source or `marked` for a caller source), sorted and non-overlapping;
`ignored_marked_spans` lists caller spans that were dropped (empty or reversed, past the end, or
not on a character boundary in their unit, including a UTF-16 offset inside a surrogate pair).

`writemorebetter_style::narrative` holds the helpers #354 and #355 share. `NarrativePerson` and
`NarrativeTense` are public API. The rest is public only so `style/tests/narrative.rs` can pin it,
hidden from the docs and unstable: `scenes` (a scene starts at paragraph 0 and at every
`scene_break_before || chapter_start`, the rule sensory balance uses; sentence ranges are global),
`chains` and `ChainClass` (`Past`, `PastPerfect`, `Present`, `Modal { present }`, `Ambiguous`),
`clause_starts`, `pronoun_person` with `Case`, `imperative_start` and
`paragraph_quotes_balanced`. The chain classifier treats *must* as ambiguous (past narration uses
it), a perfect modal (*must have been*, *will have gone*) as past, a VBD/VBP form of one lemma
(*set*, *put*) as past only with a singular subject, and a particle the chunker opened a verb
phrase on (*set it down*) as no verb. #355 extends this module with sentence-tense voting and
scene-tense inference; extend it rather than fork it.

## Options and bindings

| Option | Rust | JSON key | Default |
|---|---|---|---|
| caller spans | `marked_spans: Vec<MarkedSpan>` | `markedSpans` | `[]` |
| their unit | `marked_span_units: Option<OffsetUnits>` | `markedSpanUnits` (`"byte"`, `"char"`, `"utf16"`) | Rust bytes; WASM UTF-16; Swift always UTF-16; Python code points |
| thought detection | `detect_thought` | `detectThought` | `true` |
| unquoted-speech detection | `detect_unquoted_speech` | `detectUnquotedSpeech` | `true` |

Builders: `with_marked_spans(units, spans)`, `with_detect_thought`, `with_detect_unquoted_speech`.
`MarkedSpan` rejects unknown keys and requires `voice`.

```json
{"markedSpanUnits": "utf16", "markedSpans": [{"start": 120, "end": 188, "voice": "embedded"}]}
```

The report, `voices::report(&Document, &VoicesOptions)` (no options yet), works on compact
documents. For *Dear Sam, I am gone. I’m done with this, she thought.* with the first sentence
marked `embedded` (the embedded span is left out of `spans` here):

```json
{"spans": [{"span": {"start": 21, "end": 39}, "voice": "thought", "source": "tag",
            "tag": {"start": 41, "end": 52}, "shift": "both", "marked": null}],
 "counts": {"embedded": {"spans": 1, "words": 5}, "thought": {"spans": 1, "words": 5}, …},
 "narrationWords": 2, "narrationSentences": 0, "sentences": 2, "ignoredMarkedSpans": []}
```

`counts` lists every non-narration voice, zero when absent, so typed bindings get a fixed record.
`narrationWords` plus every voice's words equals `stats().words.all`. `narrationSentences` counts
sentences whose first word is narration (the voice counterpart of narration mode).

WASM `styleDocument` and `analyzeDocument` accept the keys and `StyleDocument.voices()` returns
the report (types in `style-results.d.ts`). Swift's `DocumentOptions` record gains `markedSpans`
(`MarkedSpan(start:end:voice:)`, UTF-16), `detectThought` and `detectUnquotedSpeech`;
`StyleDocument.voicesJson()` and the typed `voices()` return the report. Python gets
`StyleDocument.voices()` and the `"voices"` report name.

## Evaluation

Sources are public only: the CC0 stories in `bench/public`, the Standard Ebooks books in
`bench/style/sources.json`, five modernist novels added for this lens (*To the Lighthouse*,
*Jacob's Room*, *A Portrait of the Artist*, *Ulysses*, *Sons and Lovers*), because the original
books hold almost no tagged unquoted thought, and ten more added after the rules were frozen for a
second held-out sample. `bench/corpus/` is never opened. Commands and files are in
[`bench/style/README.md`](../bench/style/README.md#voices-353); results are in
`bench/style/voices-baseline.json`.

**Synthetic** (`voices.py synthetic`, seed 353, CC0 templates in
`bench/style/thought-templates.json`): 624 items inserted after 60 third-person past host
paragraphs from the books (633 since the review fixes added nine first-person splice negatives),
over all four positions, comma, dash, ellipsis, `?`, `!`, U+2028, curly and straight apostrophes,
and pronoun, name and titled-name subjects. Thought recall: trailing
0.963, bare 1.000, leading 0.938, mid 1.000, with the same exact-span rates; speech: trailing
0.975, leading 0.938, mid 1.000. The person and tense swaps and the *The room is dark.* guard are
all recalled exactly. False positives: 0, over 51 negatives (indirect forms, agreeing clauses,
untagged FID, excluded verbs, hedges, the noun *thought*, tags split by a paragraph break, present
frames with agreeing tags, first-person past splices with a sound or action verb such as *the
gulls cried*) and every positive. The remaining misses are tagger limits: *Trust*
has no verb reading, and a capitalized verb after a colon is sometimes read only as a name.

**Natural precision**, labelled blind by two model families, Codex `gpt-6-sol` (OpenAI) and
DeepSeek V4 Pro via OpenCode, under `bench/style/labels/THOUGHT-GUIDELINES.md` (only DT/FDT, or
direct speech for the speech voice, counts), with the implementer adjudicating disagreements.
These are model labels, not hand labels.

| Set | Rules | Thought | Speech | All |
|---|---|---|---|---|
| development (150) | as sampled | 29/29 | 120/121 | 149/150, agreement 150/150 |
| held-out 1 (100) | frozen at `08fce42` | **24/29 = 0.828**, Wilson lower 0.655 | 71/71 = 1.000, lower 0.949 | 95/100, agreement 100/100 |
| held-out 2 (100, ten new books) | frozen at `33b7f15` | **13/13 = 1.000**, lower 0.772 | **83/87 = 0.954**, lower 0.888 | 96/100, lower 0.902, agreement 99/100 |

Held-out 1 missed the 0.90 target for thought. Its five errors (a participle joined to a split
thought, a clause crossing a closed parenthesis, an inverted tag that kept only *Lily*, and two
spoken *concluded he* lines in *Ulysses*) led to fixes in `33b7f15`, which made held-out 1
development data. Held-out 2 was then drawn from books added after that freeze. Its four speech
errors led to the last fixes (`5d6f178`); with the current rules it scores thought 13/13 and speech
84 kept, 83 correct (all 97 kept, 96 correct), but those current-rule numbers are no longer
held-out. *Conclude* has two rows in held-out 1, both false (*Ulysses*), and two in held-out 2, both
true (*Orlando*); the spec's rule cuts a lemma below 0.80 only with at least five development
rows, so it stays. *Decide*, *figure* and *realize* had one, zero and zero development rows, so
none was cut; only *think* reached five (24 rows, precision 1.000).

The thought voice cannot meet a per-voice Wilson lower bound of 0.82 on this data: the public
books hold 72 tag-source thought spans in all, and 13 of 13 correct gives a lower bound of only
0.772. The CI gate (`voices.py score --gate`) therefore requires held-out precision ≥ 0.90 per
voice, a Wilson lower bound ≥ 0.82 pooled and for each voice with at least 30 rows, synthetic
false positives = 0 and the synthetic recall targets.

**Review fixes** (after `5c3192a`): the reporting-verb list for unquoted speech, the dash
paragraph rule and the noun-subject tense requirement drop 3 of 121 development and 4 of 71
held-out-1 speech rows that were correct (Joyce tags with *cried*, *pursued*, *interrupted*,
*began* and *continued* outside a dash paragraph). No labelled false row was dropped, and
held-out 2 is unchanged at 97 kept, 96 correct. Natural tag spans: 808 (72 thought, 736 speech)
against 807 before.

**Quoted thought** (`voices.py quotes`, `labels/quote-thought.jsonl`): before the review the
quote relabel had no precision measurement, and *concluded*, *mused*, *reflected*, *decided* and
*pondered* (84 of 261 relabelled quotes) mostly tagged spoken lines. With the narrower list and
the split-quote and outside-punctuation rules, 254 quotes are relabelled across the public
sources. A random sample of 60 (seed 3533), **labelled by the implementer and not blind**, has
59 correct, precision 0.983, Wilson lower 0.911; the one error is *“Ecstasy! Where’s the post
office?” she wondered.* right after *“Ecstasy!” she cried.* The gate requires this sample's
precision ≥ 0.90. An independent blind labelling, like the tag-source sets, has not been done.

**Misses** (informational): of 100 unflagged narration sentences with a thought-verb form, 6 hold
direct thought (*it’s just as I thought, he’s a terrible bedfellow*, a verbless *Life: she
thought*, a parenthetical *this cannot last, she thought*, fixed afterwards). Labeller agreement
95/100, kappa 0.525; the coordinator decided the five disagreements.

**Narration delta** (per source in `voices-baseline.json`): narration-mode sentences against
voice-narration sentences, the share of narration-mode words each voice takes out of narration,
and tag spans per 1,000 narration words. Tagged unquoted thought is rare: 0.633 per 1,000
narration words in *To the Lighthouse*, 0.12 in *Jacob's Room*, about 0.01–0.09 in *Moby-Dick*,
*Orlando*, *Ulysses* and *The Sound and the Fury*, 0 in most nineteenth-century books. Tagged
unquoted speech is 2.69 per 1,000 in *Portrait* and 1.75 in *Ulysses* (dash dialogue). Quoted
terms take 0.03–1.9% of narration words out of narration.

**Regression.** `bench/tools/gates.sh`: conformance 6085/6085, public recall 60/60, golden and
chunker files unchanged (the private gate was not run). `cargo test -p writemorebetter-style`,
the workspace tests, `node wasm/test.mjs`, `swift test` and the Python tests pass.
`analyze_document` on the hard-wrapped `bench/public/moby150k.txt` (9 runs, median, same
machine): 1216 and 1243 ms on `origin/main` against 1220 and 1234 ms on this branch in two
alternating rounds, peak RSS 68–71 MB for both: the overhead is within run-to-run noise.

## Known limits

- Natural tagged unquoted thought is rare in public-domain fiction, so the thought precision
  estimate rests on 72 spans; the held-out sample after the last freeze has 13.
- *Conclude* tags speech in *Ulysses*; dash dialogue there sometimes reads as thought.
- A first-person splice with a reporting verb and a pronoun or name subject (*I looked away, he
  said.*) is still unquoted speech: in that shape unquoted dialogue (Faulkner, Hemingway) is the
  commoner reading. *The wind whispered* and other non-person noun subjects anchor speech only on
  a tense shift.
- The quote-thought sample is implementer-labelled, not blind.
- A capitalized imperative after a tag (*She thought, Stay calm.*) depends on a lexicon fallback;
  nouns the tagger never reads as verbs (*Trust the plan*) are missed.
- Optional Lancaster SW&TP scoring (CC BY-NC-SA) was not run.
