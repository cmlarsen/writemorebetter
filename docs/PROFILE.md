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
  `"BUILTIN/WHITESPACE_RULE/INDENTATION"`, `DIRECT_SPEECH` of the three
  `TOO_OFTEN_USED_*_EN` builtins, e.g.
  `"BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH"`, and
  `"BUILTIN/EN_CONSISTENT_APOS/MAJORITY"` (the apostrophes of the more common style,
  so only the rarer style is flagged).

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

## Enabling rules

A profile can also enable rules, after its disable list: `Profile::enable` takes the
selectors `Rules::enable_rules` takes (below), and `apply_profile` reports the ones
that match nothing with the unmatched disable selectors. `fiction` enables one rule
this way (see [The fiction profile](#the-fiction-profile)). LanguageTool also ships rules that are off until a user turns
them on: `default="off"` rules and categories, and `tags="picky"` rules (its
Level.PICKY). The compiler keeps them in the rules binary but disabled, so these
style rules ship off:

| Selector | Off rules it enables | What it flags |
|---|---|---|
| `PLAIN_ENGLISH` | 98 | wordy phrases with a plainer form, e.g. `IN_MY_OPINION`, `TIRED_INTENSIFIERS` ("really", "very") |
| `BUILTIN/EN_PLAIN_ENGLISH_REPLACE` | 1 | the word-list companion of `PLAIN_ENGLISH` |
| `REDUNDANCY` | 28 (159 after `fiction`) | redundant phrases, e.g. "free gift", `IN_ORDER_TO` |
| `BUILTIN/EN_REDUNDANCY_REPLACE` | 1 | the word-list companion of `REDUNDANCY` |
| `STYLE/PASSIVE_VOICE`, `STYLE/PASSIVE_VOICE_SIMPLE` | 7, 37 | passive constructions |
| `STYLE/SERIAL_COMMA_ON`, `STYLE/SERIAL_COMMA_OFF` | 8, 1 | a missing / a present Oxford comma (pick one) |
| `STYLE/ELLIPSIS` | 1 | three periods instead of `…` |

`EN_PLAIN_ENGLISH_REPLACE`'s word list (`wordiness.txt`) writes "delete this phrase" as the
replacement `[OMIT]` (`as a whole=[OMIT]`, `as a matter of fact=in fact|[OMIT]`). LanguageTool
6.6 suggests that marker literally. Here it becomes a deletion that leaves well-spaced prose:
a phrase between paired commas or dashes goes with both marks (`was, as a matter of fact, late`
→ `was late`); one before the end of a clause goes with the space, comma or dash before it; one
before another word goes with the space after it, and at a sentence start also with a following
comma, capitalizing the next word (`As a whole, the plan failed.` → `The plan failed.`). When the
deletion reaches past the phrase, the span grows to cover it and the other replacements carry the
same surrounding text. Where no clean deletion exists, only the other replacements are offered.
The message says "omit it" instead of `[OMIT]`.

Counts are compiled rules (each rule of a `<rulegroup>` counts once) on the default
profile. `PASSIVE_VOICE`, `SERIAL_COMMA_*` and `ELLIPSIS` are rule groups inside the
`STYLE` category, so a bare `PASSIVE_VOICE` is an unknown selector; `STYLE` alone
would enable all 112 off `STYLE` rules.

`Rules::enable_rules(&selectors)` turns rules on by the profile selector syntax:
`CATEGORY`, `CATEGORY/RULE_ID` (any `Selector` prefix) or `BUILTIN/ID`. The builtin
case form (`BUILTIN/ID/CASE`) only disables, so it is not accepted here. Callers
apply it **after** the profile, so an enabled selector wins: `fiction` plus
`REDUNDANCY` runs the redundancy rules, and enabling a `TOO_OFTEN_USED_*_EN` builtin
under `fiction` keeps the profile's `DIRECT_SPEECH` skip.

Every selector is checked before any rule changes. If one matches no rule, nothing is
enabled and the call fails with `Error::UnknownSelectors` listing the unmatched ones,
as an unknown profile name fails. On success it returns how many rules went from off
to on (0 if they were already on). Enabling a rule changes no other rule's matches
beyond LanguageTool's usual overlap handling between rules that flag the same span.

With no selectors the rule set is exactly the profile's, so conformance and recall
are unchanged.

Surfaces:

- Swift: `checker.enableRules(selectors: ["PLAIN_ENGLISH", "STYLE/PASSIVE_VOICE"])` returns
  the count and throws `CheckError.UnknownSelectors(selectors:)`. It applies on top
  of the constructor's profile (`"fiction"` by default) and to the next `check`.
- WASM: `checker.enableRules(["PLAIN_ENGLISH", "STYLE/PASSIVE_VOICE"])` returns the count
  and throws a string naming the unknown selectors.
- `bench/tools/wmb-probe` reads `ENABLE`, a comma-separated selector list applied
  after `PROFILE`; `bench/tools/public_recall.py --enable <list>` passes it through.

### Word-frequency threshold

The `TOO_OFTEN_USED_*_EN` builtins flag a word that makes up at least 5% of the
adjectives, nouns or verbs of a text: the whole document, also when it is checked paragraph by
paragraph (`check_paragraph` + `resolve_document`). LanguageTool lets a user set this minimum
percentage per rule (a `RuleOption` from 1 to 100). `Rules::set_too_often_used_percent(percent)`
sets it for all three rules, and `TooOftenUsed::set_min_percent`, reached through
`Rules::builtins_mut` and `BuiltinRule::builtin_mut`, sets it for one. A value outside
1–100 is `Error::OptionOutOfRange` and changes nothing. Setting it does not enable the
rules. Swift has `checker.setTooOftenUsedPercent(percent:)`, which throws
`CheckError.InvalidOption(message:)`, WASM has `checker.setTooOftenUsedPercent(n)`,
which throws a string, and Python has `checker.set_too_often_used_percent(n)`, which raises
`ValueError`. A paragraph's digest does not depend on the percentage, so changing it needs no
recheck, only a new `resolve_document`.

### Readability level

`READABILITY_RULE_SIMPLE` flags a paragraph whose Flesch reading level (0 very difficult to 6
very easy) is above a level, 3 by default, and `READABILITY_RULE_DIFFICULT` one whose level is
below it; LanguageTool lets a user set the level per rule (a `RuleOption` from 0 to 6).
`Rules::set_readability_level(level)` sets it for both rules, and `Readability::set_level`,
reached through `Rules::builtins_mut` and `BuiltinRule::builtin_mut`, sets it for one. A level
above 6 is `Error::OptionOutOfRange` and changes nothing. Setting it does not enable the rules,
which are off by default and in the fiction profile. Swift has
`checker.setReadabilityLevel(level:)`, which throws `CheckError.InvalidOption(message:)`, WASM has
`checker.setReadabilityLevel(n)`, which throws a string, and Python has
`checker.set_readability_level(n)`, which raises `ValueError`. Digests do not depend on the
level either: changing it needs only a new `resolve_document`.

## The fiction profile

Disables exactly these:

| Selector | Why (corpus flags) |
|---|---|
| `TYPOGRAPHY/EN_QUOTES` | straight-to-curly quote suggestions (592) |
| `PUNCTUATION/DASH_RULE` | dash typography (18) |
| `PUNCTUATION/COMMA_COMPOUND_SENTENCE` | comma before "and" (23) |
| `PUNCTUATION/COMMA_COMPOUND_SENTENCE_2` | comma before "and", "but" or "so" between clauses (4 on the private corpus; 1,166 for 61 of 70 authors in published fiction) |
| `PUNCTUATION/MISSING_COMMA_AFTER_INTRODUCTORY_PHRASE` | (6) |
| `STYLE/GONNA` | dialogue informality (9) |
| `STYLE/GOTTA` | dialogue informality (4) |
| `STYLE/WANNA` | dialogue informality (1) |
| `GRAMMAR/YOU_GOOD` | verbless dialogue questions, "You okay?" (2) |
| `REDUNDANCY` | stylistic redundancy advice, not errors (whole category) |
| `BUILTIN/WHITESPACE_RULE/INDENTATION` | spaces that indent a paragraph or line; LT 6.6 flags them after a sentence end or a CRLF (403 on moby150k). Repeated spaces inside a line are still flagged. |
| `TYPOGRAPHY/CONSECUTIVE_SPACES` | two spaces between words; the `WHITESPACE_RULE` builtin flags the same spaces, so only its message ("Possible typo: repeated spaces") is shown |
| `BUILTIN/ENGLISH_WORD_REPEAT_BEGINNING_RULE` | three sentences in a row that begin with the same word ("He waited. He listened. He...") are a deliberate rhythm in fiction |
| `BUILTIN/TOO_OFTEN_USED_ADJECTIVE_EN/DIRECT_SPEECH`, `…_NOUN_EN/…`, `…_VERB_EN/…` | these word-frequency hints are off by default, as in LT. Once an author enables them, words inside dialogue are neither counted nor flagged (LT's `setWithoutDirectSpeech`): characters repeat words on purpose, and the narration is what the hint is for. |
| `BUILTIN/EN_CONSISTENT_APOS/MAJORITY` | the rule (enabled below) flags only the less common apostrophe style, not every apostrophe as in LT: one stray straight `'` in a curly manuscript flags that one mark, not the hundreds of curly ones (267 → 1 flag on moby150k) |

And enables these two, which are off by default:

| Selector | Why |
|---|---|
| `BUILTIN/EN_CONSISTENT_APOS` | a manuscript that mixes straight (`'`) and curly (`’`) apostrophes, often from text pasted in from another editor. Once the text has both, the fiction profile flags only the apostrophe tokens of the less common style, with the more common one as the suggestion (on a tie, the straight ones, as curly is the fiction norm); a text that uses one style throughout gets no flags. This is the `BUILTIN/EN_CONSISTENT_APOS/MAJORITY` entry in the disable list above: LanguageTool, and the rule outside this profile, flag the tokens of both styles. The counts cover the whole document (every current digest passed to `resolve_document`), so checking it whole or paragraph by paragraph flags the same tokens, and an edit in one paragraph can change which style is flagged in others. A lone `‘` or `’` (a single quotation mark, or a clipped `jus’` or plural `dogs’` the tokenizer splits the same way) is not an apostrophe to it, so curly-quoted dialogue does not trigger it. A lone straight `'` does count, as in LanguageTool: straight single quotation marks around dialogue in a curly-apostrophe text are flagged. |
| `BUILTIN/PARAGRAPH_OPENER_REPEAT` | writemorebetter's own rule, not LanguageTool's: a paragraph that begins with the same word as one of the three paragraphs before it. Every paragraph of a match is marked once ("Nearby paragraphs begin with the same word"), from its first token to the repeated word. Beginnings compare as in `PARAGRAPH_REPEAT_BEGINNING_RULE` (a shared leading quote and a shared determiner are skipped, the word is case-sensitive, a dash or ellipsis lead never matches), so `“I…` dialogue paragraphs match each other but not narration that opens `I…`. A first-person draft whose paragraphs open `I…`, `Shaking, I…`, `I…`, `I…`, `Okay…`, `Before…`, `I…` gets paragraphs 1, 3, 4 and 7 marked; LanguageTool's rule marks only 3 and 4. It fires often on first-person and close-third narration, which is deliberate there as often as not: the owner chose (2026-10-24) to show it and let the writer judge each one. |

`EN_REPEATEDWORDS` (synonyms for a word repeated within 150 words) needs no
entry: it is off by default (`picky` in LT), and when enabled it already skips
every sentence that does not end in `.`, `!` or `?`, which leaves out dialogue
that ends in a closing quote. Dialogue followed by a tag (`“Definitely,” she
said.`) is still checked. Its word list is short (about 40 words such as
"nice", "amazing", "definitely"), so an author who turns it on gets few flags.

Applying it to the compiled English rules disables 172 rules, enables 2, and reports 0
unmatched selectors.

The fiction profile does not need to name rules that are off by default. Of the
builtins ported for [#81](https://github.com/cmlarsen/writemorebetter/issues/81),
`PROFANITY` (swearing is deliberate in dialogue), `EN_PLAIN_ENGLISH_REPLACE` and
`EN_REDUNDANCY_REPLACE` (style advice, not errors) and `EN_DASH_RULE` (it reads an
em dash between two words, as in `well—being`, as a typo for a hyphen) are off by
default as in LanguageTool 6.6, so they stay off in fiction too. `EN_COMPOUNDS`
(`part time` → `part-time`) and `EN_SPECIFIC_CASE` (`Harry potter` → `Harry Potter`)
flag spelling errors and stay on.

The whole-text paragraph rules of [#206](https://github.com/cmlarsen/writemorebetter/issues/206)
and [#207](https://github.com/cmlarsen/writemorebetter/issues/207) stay off in fiction too, as in
LanguageTool 6.6. `READABILITY_RULE_SIMPLE` would flag most dialogue-heavy paragraphs: short
lines of speech score as "very easy", and once the whole text does too, every such paragraph of
ten or more words is flagged (`READABILITY_RULE_DIFFICULT` rarely fires on fiction).
`PARAGRAPH_REPEAT_BEGINNING_RULE` flags two successive paragraphs that open with the same word,
which close-third narration ("She…", "He…") and runs of dialogue (`“I…` / `“I…`) do on
purpose. The fiction profile enables `PARAGRAPH_OPENER_REPEAT` (above) instead, which looks
three paragraphs back and marks each paragraph once.

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
