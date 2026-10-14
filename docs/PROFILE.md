# Rule profiles

Milestone M7. Adds an opt-in way to turn off a named set of rules, and a default
`fiction` profile for the engine's primary input (fiction drafts).

Everything here is behavior-preserving by default: `profile = "default"` disables
nothing, so the rule set, conformance, and recall are unchanged unless a caller
asks for a profile.

## Mechanism

`nlprule::profile::Profile` is a name plus a list of *selector strings*:

- an XML rule selector as understood by `Rules::select_mut`, e.g.
  `"TYPOGRAPHY/EN_QUOTES"`, `"PUNCTUATION/DASH_RULE"`, or a whole category such as
  `"REDUNDANCY"`; or
- a builtin rule id prefixed with `BUILTIN/`, e.g.
  `"BUILTIN/UPPERCASE_SENTENCE_START"`; or
- a builtin rule id plus one of its cases, which disables only that case and keeps
  the rule on (`BuiltinRule::disable_case`). The cases are
  `"BUILTIN/WHITESPACE_RULE/INDENTATION"` and `DIRECT_SPEECH` of the three
  `TOO_OFTEN_USED_*_EN` builtins, e.g.
  `"BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH"`.

`Profile::by_name` accepts `"default"` (disables nothing) and `"fiction"`.
`Rules::apply_profile(&profile)` disables every matching grammar rule and builtin
and returns the selectors that matched nothing, which it also logs at `warn`.
A test asserts the fiction profile has no unmatched selectors, so a typo or a
renamed rule is a hard failure rather than a silent no-op.

The selectors are rule *identifiers*, not rule data, so the list lives in engine
source under the engine's MIT/Apache-2.0 license.

Surfaces:

- `bench/tools/wmb-probe` reads `PROFILE` (default `"default"`), applies it after
  loading the rules, and prints `profile <name>: <k> rules disabled` (k is the
  number of grammar + builtin rules actually disabled).
- `wasm::Checker::new(tokenizer, rules, spell_variant, profile)` takes the profile
  name as a new, last, optional argument.
- `bench/tools/public_recall.py --profile <name>` passes `PROFILE` to the probe and
  prints the flag count.

## The fiction profile

Disables exactly these:

| Selector | Why (corpus flags) |
|---|---|
| `TYPOGRAPHY/EN_QUOTES` | straight-to-curly quote suggestions (592) |
| `PUNCTUATION/DASH_RULE` | dash typography (18) |
| `PUNCTUATION/COMMA_COMPOUND_SENTENCE` | comma before "and" (23) |
| `PUNCTUATION/MISSING_COMMA_AFTER_INTRODUCTORY_PHRASE` | (6) |
| `STYLE/GONNA` | dialogue informality (9) |
| `STYLE/GOTTA` | dialogue informality (4) |
| `STYLE/WANNA` | dialogue informality (1) |
| `GRAMMAR/YOU_GOOD` | verbless dialogue questions, "You okay?" (2) |
| `REDUNDANCY` | stylistic redundancy advice, not errors (whole category) |
| `BUILTIN/WHITESPACE_RULE/INDENTATION` | spaces that indent a paragraph or line; LT 6.6 flags them after a sentence end or a CRLF (403 on moby150k). Repeated spaces inside a line are still flagged. |
| `BUILTIN/TOO_OFTEN_USED_ADJECTIVE_EN/DIRECT_SPEECH`, `…_NOUN_EN/…`, `…_VERB_EN/…` | these word-frequency hints are off by default, as in LT. Once an author enables them, words inside dialogue are neither counted nor flagged (LT's `setWithoutDirectSpeech`): characters repeat words on purpose, and the narration is what the hint is for. |

`EN_REPEATEDWORDS` (synonyms for a word repeated within 150 words) needs no
entry: it is off by default (`picky` in LT), and when enabled it already skips
every sentence that does not end in `.`, `!` or `?`, which leaves out dialogue
that ends in a closing quote. Dialogue followed by a tag (`“Definitely,” she
said.`) is still checked. Its word list is short (about 40 words such as
"nice", "amazing", "definitely"), so an author who turns it on gets few flags.

Applying it to the compiled English rules disables 147 rules and reports 0
unmatched selectors.

### Rules found while searching `data/en/grammar.xml`

- **Verbless dialogue questions:** the rule is `<rulegroup id="YOU_GOOD">`
  (line 25204), already covered by the list above. Its message is
  `Did you mean <suggestion>are you \4 \5</suggestion>?` (rule 3 of the group;
  other variants are `are <match ...> \4 \5`). Invented example it flags:
  `"You ready?" she asked.` → suggests "Are you ready?".
- **Sentence starts with the same word** (LT's `ENGLISH_WORD_REPEAT_BEGINNING_RULE`):
  no XML equivalent exists in this LT 5.2 build dir. That rule compares two
  consecutive sentences, which the XML pattern format cannot express; searching the
  rule ids and names in `data/en/grammar.xml` for repeat/repetition/beginning/same
  word turns up only `PHRASE_REPETITION` ("at the at the"),
  `ADVERB_VERB_ADVERB_REPETITION`, and `REPEAT_AGAIN` — none is the progressive
  word-repeat rule. Nothing was added for it.

## Measurements

All numbers from the in-tree engine (LT 5.2 rules + builtins), en_US speller on.

### Public corpus (`bench/tools/public_recall.py`, floor 51)

| Profile | Recall | Flags across the 5 stories |
|---|---|---|
| `default` | 51/60 | 97 |
| `fiction` | 51/60 | 87 |

Recall does not drop; the fiction profile removes 10 flags on the public corpus.

### Moby-Dick, first 150,000 bytes

| Profile | Flags |
|---|---|
| `default` | 981 |
| `fiction` | 967 |

Most of the flags are the speller on the front matter (Gutenberg header, extracts);
the profile removes 14 rules flags. In this excerpt `EN_QUOTES` never fires.

## Gates

- `cargo fmt --all -- --check`: clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features`: no errors
  (pre-existing `result_large_err` warnings only).
- `cargo test --workspace --exclude nlprule-python --all-features --release`: all
  pass (nlprule 139 integration tests + 10 unit tests, including the 4 new profile
  tests; build/srx/wasm/doc-tests pass).
- Conformance unchanged (profiles are opt-in): grammar `test` 3749/3749 runnable
  and passing, disambiguation `test_disambiguation` 843/843.
