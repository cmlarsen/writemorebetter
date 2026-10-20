# Passive voice

Issue #303 adds a **passive-voice rate**: passive clauses per 100 sentences, for narration and
dialogue separately, compared with a p5–p95 band. Like the other style reports it describes and
never rewrites. It is the aggregate rate, not a second per-instance rule: per-instance flags belong
to LanguageTool's `PASSIVE_VOICE` once the grammar-rule opt-in lands. Spans are still returned so a
UI can list the passives behind the number.

| Surface | Call |
|---|---|
| Rust | `passive::report(&Document, &PassiveOptions, Option<&PassiveBaseline>)` |
| Retained Rust | `document.passive(options_json, baseline_selector)` |
| WASM | `document.passive(optionsJSON?, baselineJSON?)` → `PassiveReport` |
| Swift | `document.passive(optionsJson:baseline:) -> PassiveReport` (`passiveJson` for raw JSON) |

The report needs token details (POS readings) and returns `StyleError::NeedsTokenDetails` on a
compact document. Options are camelCase JSON with snake_case aliases, and unknown keys are errors.
The only option is `minimumSentences` (default 50): below it a group's `perHundredSentences` is
null, though its counts and spans stay. `lowConfidence` is true below 200 sentences, or below a
higher caller minimum.

The baseline selector works as it does for the other baseline-aware reports. Omitted or `"bundled"`
uses `style/data/baselines/passive.json`, `"none"` turns comparison off, and a JSON object replaces
the bands: `{"bands": {"all": {"p5": 3, "p95": 28}, "narration": {…}, "dialogue": {…}}}`. Any
mode can be left out or set to null. Bands need `0 <= p5 <= p95`, and unknown keys are rejected.
A group is `highlighted` only when its rate is outside its own mode's band.

## Result

`{passives, groups}`. Each `PassiveMatch` holds:

- `span`: the auxiliary chain through the participle (`was being followed`, `’s been seen`,
  `Was he taken`);
- `participle`: the participle alone;
- global `sentence` index;
- `auxiliary`: `be` or `get`, from the last auxiliary in the chain (`is getting fixed` is `get`);
- `progressive`: true when the chain has *being* or *getting* after another auxiliary;
- `agent`: true when *by* directly follows;
- `mode`.

`groups: Split<PassiveGroup>` gives `sentences`, `lowConfidence`, `passives`,
`perHundredSentences`, the `band` in force, and `highlighted`. The rate counts passives, not
sentences: a sentence with two passives adds two. It can pass 100.

Mode is the sentence's mode, as in transitions (#244). A tagged line like `“I was followed,” she
said.` is a dialogue sentence. In a narration sentence with a quote inside it, the quoted passive
counts as narration. Sentences without a word, such as a lone `…`, are left out of the
denominator. All spans are UTF-16 offsets in the bindings.

## Detection

These are token heuristics over nlprule readings, not a parser:

1. **Auxiliary.** A token with a *be* or *get* verb reading. Contractions count (`’s`, `’re`,
   `’m`, `wasn’t`). The possessive `’s` is tagged `POS` and does not count.
2. **Chain.** After the auxiliary, the matcher steps over further *be*/*get* forms (`been`,
   `being`, `getting`), up to three adverbs (including *not*, *n’t* and *never*), and one subject
   pronoun when the auxiliary opens the sentence (`Was he taken?`). Indefinite pronouns tagged RB
   (`nothing`, `something`, …) end the chain, so `there was nothing left` is not a passive.
   Punctuation between tokens ends the chain. Whitespace, including U+2028, does not.
3. **Participle.** The first remaining token must have a `VBN` reading.
4. **Guards.** The match is dropped when:
   - a degree adverb comes before the participle (`very`, `so`, `too`, `quite`, `rather`, `more`,
     `most`, `less`, …). `She was very surprised` is adjectival.
   - the participle is on a list of 57 participles that usually read as adjectives after *be*/*get*:
     tired, married, interested, worried, scared, bored, lost, drunk, dressed, complicated, wet,
     acquainted, obliged and others. A following *by* turns them back into passives (`She was
     frightened by the noise`), but a degree word still blocks them.
   - the participle is *gone*, *come* or *arrived*. These verbs are intransitive, so *be* + the
     participle is an old perfect or a state.
   - it is *used to* / *supposed to* before something that is not a base verb: `I got used to it`
     and `She was supposed to stay` are dropped, while `It is used to cut rope` counts.
   - it is *done* or *finished* after a personal pronoun at clause end or before *with* (`I’m done`,
     `We were finished with it`). `It was done` counts.
   - the auxiliary is `’s` straight before a participle with an object (`She’s taken the car`).
     That `’s` is *has*.
   - the "participle" is a base form followed by an object pronoun (`all you can do is let him go`).

`style/tests/passive.rs` covers *be* and *get* passives, progressive passives (`was being
followed`, `is getting fixed`), adjectival participles, passives inside dialogue, inversion,
agents, and spans with curly quotes and apostrophes, em dashes, ellipses and U+2028.

**Known limits.** Stative readings that are not on the list still count: `was not enough
recovered`, `ship that’s bound for Tarshish`, archaic `have been used to consider`. So do
ambiguous resultatives like `the door was closed`, which can be an event or a state. Coordinated
participles (`was beaten and robbed`) count once. Reduced passives with no auxiliary (`the man
shot yesterday`) are not detected. Quoted epigraphs and extracts count as dialogue because the
document treats them as quotes.

## Public precision

The implementation author judged 37 matches with no hand-picking, every 41st from the 1,497 found
in the public calibration texts (below): **33 clear passives, 4 stative readings**. Those four were
*much obliged*, *bound for*, *enough recovered* and *been used to consider*. *Obliged* was then
added to the list. An earlier sample of 39 found *were acquainted* (×2) and archaic *was arrived*,
which led to the *acquainted* and *arrived* guards. This is one judge, not blind, and mostly
nineteenth-century prose. It is a development check, not held-out accuracy.

## Band (provisional, public stand-in)

**The owner-only published-fiction export (`cmlarsen/fiction-baseline`) was not available to this
change.** That export produced the transitions, pronoun and sentence-start bands. So the bundled
band comes from public text, through the same pipeline. The source records say so
(`style/data/baselines/passive.json` → `source.status`). Treat the band as a replaceable baseline
argument, not a published-fiction reference.

- **Texts.** `bench/style/public_export.py` writes the tier 3 export layout from: `bench/public`
  story1–5 (one invented author, CC0, 10,168 words); `moby150k.txt`, unwrapped with chapters as
  scene breaks (Melville, 25,264 words); and all of the pinned Standard Ebooks *Pride and
  Prejudice* (Austen, 122,017 words, chapters as scene breaks), the same source
  `transitions_pronouns.py` uses. That is 7 units, 3 authors and 157,961 counted words.
- **Method.** `python3 bench/style/run.py tier3 --flag-rates --export DIR --skip-diction` and then
  `published_bands.py --passive`. This is the same 3,000/6,000/2,000-word chapter-window rule and
  author-weighted p5/p95 as the published bands. A window counts for a mode once it has 50
  sentences of that mode. The run took 18.3 s, with 215 MB peak probe RSS; the passive stage took
  15 ms of that, over both scales.

Passives per 100 sentences, chapter windows (38 windows; dialogue eligible in 29 windows from 2
authors, because the stories have too little dialogue per window):

| Mode | p5 | p25 | Median | p75 | p95 |
|---|---:|---:|---:|---:|---:|
| All | 2.50 | 4.43 | 12.50 | 18.39 | 27.70 |
| Narration | 2.84 | 5.72 | 12.68 | 28.15 | 40.94 |
| Dialogue | 4.78 | 8.22 | 11.67 | 18.39 | 20.47 |

Over whole manuscripts (one sample per author), the same run gives p5–p95 of 5.3–31.5 for narration
and 2.3–14.4 for dialogue. Over all sentences, the modern stories are about 3–6 per 100,
Melville about 12 and Austen about 23. So
the band is dominated by the two authors' periods rather than by a fiction norm. A modern novel
near the bottom of this band is ordinary, not unusual. This is why the band should be replaced
before Subplot highlights with it.

**To replace it** (owner): run `tier3 --flag-rates` on the export as described in
[bench/style/README.md](../bench/style/README.md#flag-rate-calibration-241). The probe now reports
`passive` counts per window and `measures.<mode>.per_hundred_sentences`. Then:

```sh
python3 bench/style/published_bands.py aggregate.json --passive --dataset "…" --tool "…" \
    --run-date YYYY-MM-DD --output style/data/baselines/passive.json
```

Callers can pass their own band through the baseline argument at any time without a release.
