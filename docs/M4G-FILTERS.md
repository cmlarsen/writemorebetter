# M4G: rule `<filter>` support

Support for LanguageTool's `RuleFilter` (`<filter class="…" args="…"/>` inside a
`<rule>`). Before M4G the compiler rejected every rule with a filter
("rules with filter are not implemented"); that blocked 71 rules in the LT 6.6
rule set and 34 in LT 5.2. The filters now live in `nlprule/src/rule/filter/`
(engine code, MIT/Apache-2.0); the one data table they need is LGPL data under
`build/builtin-data/en/`.

## Part 1 — inventory (LT 6.6 `grammar.xml`, sorted by rule count)

| # | Filter class | Rules | What it does | Status |
|---|---|---:|---|---|
| 1 | `org.languagetool.rules.en.FutureDateFilter` | 16 | Vetoes a match when the matched date is not after "today" (past tense + future date). Args are resolved token texts (`year:\5 month:\4 day:\3`). | implemented |
| 2 | `…rules.spelling.multitoken.MultitokenSpellerFilter` | 14 | Replaces a misspelled multiword expression with suggestions from LanguageTool's multitoken speller. | skipped — needs the multitoken-speller data table |
| 3 | `org.languagetool.rules.en.DateCheckFilter` | 9 | Vetoes a date whose weekday is wrong; 6.6 also suggests the corrected weekday and day of month. | implemented (both 6.6 and 5.2 semantics) |
| 4 | `org.languagetool.rules.UnderlineSpacesFilter` | 6 | Extends the highlighted range over whitespace before/after the marker. | implemented |
| 5 | `org.languagetool.rules.en.OrdinalSuffixFilter` | 5 | Fixes the ordinal suffix of the first suggestion ("11nd" -> "11th"). | implemented |
| 6 | `…rules.en.EnglishSuppressMisspelledSuggestionsFilter` | 4 | Drops suggestions that are themselves misspelled (and the match if none remain). | implemented (#71) |
| 7 | `org.languagetool.rules.en.FindSuggestionsFilter` | 4 | Generates spelling suggestions for a token and filters them by a target POS tag. | implemented (#71) |
| 8 | `org.languagetool.rules.en.NewYearDateFilter` | 4 | Accepts a match in January for a date whose year is one behind, rewriting `{realYear}`. | implemented |
| 9 | `org.languagetool.rules.DateRangeChecker` | 2 | Keeps the match only when `x >= y` (a reversed day range). | implemented |
| 10 | `org.languagetool.rules.patterns.RegexAntiPatternFilter` | 2 | Vetoes a `<regexp>`-rule match overlapped by one of the `antipatterns` regexes. | implemented |
| 11 | `org.languagetool.rules.patterns.ApostropheTypeFilter` | 2 | Keeps the match only if the chosen word does/doesn't carry a typographic apostrophe. | implemented |
| 12 | `org.languagetool.rules.en.EnglishNumberInWordFilter` | 1 | Suggests the number word / de-numbered form and its spelling suggestions. | implemented (#71) |
| 13 | `org.languagetool.rules.en.AdverbFilter` | 1 | Rewrites a suggested adverb into an adjective from a fixed table. | implemented (LGPL table loaded from the build dir) |
| 14 | `org.languagetool.rules.en.YMDNewYearDateFilter` | 1 | `NewYearDateFilter` for a single `yyyy-mm-dd` token (`{realDate}`). | implemented |

LT 5.2 uses 34 filters over 8 classes; all 8 are implemented (`FutureDateFilter`
16, `DateCheckFilter` 8, `NewYearDateFilter` 4, `DateRangeChecker` 2,
`AdverbFilter` 1, `YMDNewYearDateFilter` 1, `YMDDateCheckFilter` 1,
`RegexAntiPatternFilter` 1). `YMDDateCheckFilter` only exists in 5.2.

### Argument handling

`args` is resolved as LanguageTool's `RuleFilterEvaluator` does: a value that
starts with a backslash is the text of that 1-based pattern token (`\1`), any
other value is a literal. Filters on `<regexp>` rules get their arguments
verbatim (`RegexRuleFilterEvaluator`), which matters for
`antipatterns:\d[-–—]…`, whose leading `\` is a regex escape, not a token
reference.

`args` conventions differ between versions for `DateCheckFilter`: 6.6 passes
pattern-token *indices* (`year:5 month:4 day:3 weekDay:1`) and suggests
corrections; 5.2 passes the token *texts* (`year:\5 …`) and only vetoes. Both are
supported and selected by whether `weekDay` is a token reference.

## Part 2 — implementation

- New module `nlprule/src/rule/filter/` with a serialized `RuleFilter` enum, one
  variant per implemented class (plus `date.rs` for the small Gregorian-calendar
  arithmetic the date filters need — no date library is pulled in).
- The filter runs in `Suggestions::next` (after unification, before the
  "suggestion equals the matched text" cleanup, so `OrdinalSuffixFilter` sees the
  raw suggestion). A filter may veto the match (`None`), rewrite its
  suggestions, adjust its range, or substitute `{realYear}`/`{realDate}`.
- Time-dependent filters get "today" injected. The rule test harness passes
  LanguageTool's fixed test date (2014-01-01, its `TestHackHelper`); production
  uses the system clock (a fixed date on wasm, which has no clock).
- `build/builtin-data/en/adverb_adjective.txt` holds the LGPL adverb -> adjective
  table; the compiler reads it from `<build_dir>/builtin/` so it is serialized
  into the rules binary and never enters the engine sources.
- Unknown filter classes and the classes that are skipped are compile errors that
  name the class, so a rule can never silently ignore its filter.

## Acceptance numbers

These were measured after merging `origin/main` at the point of the final
commit, i.e. on top of M4F (no-suggestion rules and example types), M4H (LT 6.6
contraction tokenization) and the curly-apostrophe fix. The "before" column is
that same base with this milestone's changes reverted (the filter rules are
blocked). Two build dirs as in `build/README.md`: 5.2 (`data/en`) and 6.6
(`data/en-lt66`, with its `tokenizer_options.json`).

### 1. Filter errors before/after

| Rule set | Before ("rules with filter are not implemented") | After |
|---|---|---|
| LT 6.6 | 71 | 0 by that message; 23 by class-specific skips (`MultitokenSpellerFilter` 14, `FindSuggestionsFilter` 4, `EnglishSuppressMisspelledSuggestionsFilter` 4, `EnglishNumberInWordFilter` 1) |
| LT 5.2 | 34 | 0 (every 5.2 filter class is implemented) |

Per class: 10 of the 14 LT 6.6 classes implemented (48 of 71 rules); 4 classes
skipped (23 rules). All 8 LT 5.2 classes implemented (34 of 34 rules).

### 2. Grammar conformance (`test --list-passing`)

| Rule set | Runnable before -> after | Passing before -> after | Pass rate (of `<rule>`) before -> after | Regressions |
|---|---|---|---|---|
| LT 6.6 | 5934 -> **5982** | 5784 -> **5884** | 5784/6098 94.85% -> 5884/6098 96.49% | none |
| LT 5.2 | 4252 -> **4286** | 4187 -> **4273** | 4187/4366 95.90% -> 4273/4366 97.87% | none |

Every rule that passed before still passes (diffed the `PASS` lists). CI floors
(6.6: 5784 passing / 5934 runnable, disambiguation 1011; 5.2: 4187 / 4252,
disambiguation 843) are met and raised. Disambiguation is unchanged from the
base: 6.6 1011/1015, 5.2 843/843.

### 3. Newly compiled rules

| Rule set | Newly compiled | Newly passing | Newly failing |
|---|---:|---:|---:|
| LT 6.6 | 48 | 46 | 2 |
| LT 5.2 | 34 | 34 | 0 |

The newly failing LT 6.6 rules are pre-existing tokenizer differences — the
pattern never matches, so the filter is never reached:

1. `TYPOGRAPHY/TYPEWRITER_APOSTROPHE/0` — the tokenizer yields `actress` `'s`
   `role` for `actress’s` (the curly apostrophe is normalised), so the
   `<token>’.+` pattern does not match.
2. `PUNCTUATION/DASH_RULE/4` — the tokenizer keeps `069—68` as one token, so
   `<number> <dash> <number>` does not match.

The other 46 newly compiled rules pass, including all 16 future-date rules, the
9 date-check rules and the 5 ordinal-suffix rules.

### 4. Public recall and flags (`bench/tools/public_recall.py`)

| Rule set | Recall (default) | Recall (fiction) | Flags default | Flags fiction |
|---|---|---|---|---|
| LT 6.6 | 55/60 (floor 55) | 55/60 | 96 -> 96 | 87 -> 87 |
| LT 5.2 | 54/60 (floor 54) | 54/60 | 103 -> 103 | 93 -> 93 |

Recall and the total flags on `bench/public` are unchanged from the base
(every newly compiled rule is January-only or not triggered by the corpus).

### 5. Gates

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features` — no errors.
- `cargo test --workspace --exclude nlprule-python --all-features --release` —
  passes (includes new unit tests for the calendar math and the ordinal-suffix
  and day-of-month rewrites).

## Integration note

M4F classified an untyped example as incorrect when the rule had no
`<suggestion>` and the example carried a `<marker>`. LanguageTool classifies by
`type` or the `correction` attribute only (`PatternRuleHandler.java`, LT 6.6)
and `PatternRuleTest.testCorrectSentences` asserts a correct example does not
match regardless of markers. The marker clause misclassified the negative
examples of the no-suggestion date-filter rules (e.g. the future-date rules'
`7 October 2010`). Removing it is a one-line change (committed separately) and
raises conformance by 100 (6.6) / 86 (5.2) rules over the base, with zero
regressions.
