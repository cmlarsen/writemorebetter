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
  `"BUILTIN/UPPERCASE_SENTENCE_START"`.

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
| `BUILTIN/ENGLISH_WORD_REPEAT_BEGINNING_RULE` | three sentences in a row that begin with the same word ("He waited. He listened. He...") are a deliberate rhythm in fiction |

Applying it to the compiled English rules disables 162 rules and reports 0
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
  word-repeat rule. Nothing was added for it then; it is now a builtin
  ([#82](https://github.com/cmlarsen/writemorebetter/issues/82)) that the fiction
  profile disables.

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
