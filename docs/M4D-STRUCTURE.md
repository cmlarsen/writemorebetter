# M4D: LanguageTool 6.6 rule XML structure and a full rule accounting

Measured 2026-10-11, Apple M5 Pro, rustc 1.97.1, release builds, `RUST_LOG=WARN`.
Two build dirs:

- **5.2** — `data/en` (upstream `en.zip`, the LT 5.2 rule set; the set CI builds).
- **6.6** — `data/en-lt66`, generated from the LanguageTool 6.6 desktop release by
  `build/make_build_dir.py` as described in `build/README.md`.

The compiler now understands every structural construct the English 6.6 rules use:
the 6.6 compile reports **zero `[Structure]` errors** (was 322 error entries) and the
5.2 compile reports **zero** (was 39).

## Method for the accounting

`<rule>` counts come from `bench/tools/count_rules.py` (needs `lxml`). Naive counts
(`grep '<rule[ >]'`) include rules inside XML comments, which no parser sees; the
script splits those out. Rules inside a `<rulegroup>` are lost *as a group* when the
`<rulegroup>` element fails to deserialize: its `<rule>` children never reach the
per-rule stage, so one error entry can hide many rules. A temporary instrument in
`read_rules` reported, for every deserialization error, how many `</rule>` the failed
snippet contained; the "rules lost" columns below come from that. After the fix there
are no group-level failures left, so every remaining error loses exactly one rule.

`Runnable` / `Rules passing tests` are the `test` binary's counts
(`--list-passing` / `--list-failing` added in this milestone). `ignored` is rules
dropped by `RulesLangOptions.ignore_ids` in `nlprule/configs/en/rules.json`
(`GRAMMAR/PRP_MD_NN/2`, `TYPOS/VERB_APOSTROPHE_S/3`).

## Part 1 — account for every rule

### LanguageTool 6.6 (`data/en-lt66`, 6,098 `<rule>` elements)

| Where every `<rule>` goes | before | after |
|---|---:|---:|
| in the file (`grep`) | 6,098 | 6,098 |
| inside XML comments (never parsed) | 24 | 24 |
| **parsed by nlprule** | **6,074** | **6,074** |
| lost to `[Structure]` deserialization errors | 1,699 (322 entries) | 0 |
| lost to `[Rule]` conversion errors | 364 (364 entries) | 837 (837 entries) |
| dropped by `ignore_ids` | 0 | 2 |
| **compiled (`Runnable`)** | **4,011** | **5,235** |
| sum | 6,098 | 6,098 |

Before, the 1,699 structure-lost rules break down as (entries, rules lost):

| Structure error | entries | rules lost |
|---|---:|---:|
| `unknown variant example` in `<pattern>` (`<example>` inside `<antipattern>`) | 238 | 1,440 |
| `unknown field tags` on rule | 30 | 52 |
| `unknown field tags` on rulegroup | 20 | 127 |
| `unknown field type` on rulegroup | 8 | 42 |
| `unknown field type` on rule | 6 | 6 |
| `unknown field type` on `<regexp>` | 2 | 2 |
| `missing field text` (empty `<marker>`) | 10 | 10 |
| `missing field $value` (empty `<suggestion>`) | 3 | 8 |
| `unknown field raw_pos` | 2 | 2 |
| `unknown field distance_tokens` | 1 | 7 |
| `unknown field min_prev_matches` | 1 | 1 |
| `unknown field postag` on `<match>` in a `<token>` | 1 | 2 |
| **total** | **322** | **1,699** |

The 1,699 − 322 = 1,377 "missing" rules are exactly the rules inside the failed
rulegroups: a `<rulegroup>` with one bad child fails to deserialize as a whole, so one
error entry hides all of its rules. (This is the ~1,400 gap.)

The 364 before / 837 after `[Rule]` errors are the unsupported *features*
(`postag` in `<match>`, no suggestion, `filter`, `triggers_error` examples,
`include_skipped`, repetition attributes, `raw_pos`, control flow in parallel tokens,
two Oniguruma-rejected regexes). They are not structural and are unchanged in kind;
their counts grow only because more rules now get far enough to hit them.

### LanguageTool 5.2 (`data/en`, 4,366 `<rule>` elements)

| Where every `<rule>` goes | before | after |
|---|---:|---:|
| in the file (`grep`) | 4,366 | 4,366 |
| inside XML comments (never parsed) | 22 | 22 |
| **parsed by nlprule** | **4,344** | **4,344** |
| lost to `[Structure]` deserialization errors | 101 (39 entries) | 0 |
| lost to `[Rule]` conversion errors | 492 (492 entries) | 530 (530 entries) |
| dropped by `ignore_ids` | 2 | 2 |
| **compiled (`Runnable`)** | **3,749** | **3,812** |
| sum | 4,366 | 4,366 |

Before structure loss (39 entries, 101 rules): `tags` on rule 38, `type` on rulegroup
30, `tags` on rulegroup 13, `missing text` 10, `type` on rule 7, `raw_pos` 1,
`case_conversion` on `<match>` in a `<token>` 1, `missing $value` 1.

## Part 2 — what each structural construct means and what was done

Sources: LanguageTool `languagetool-core` `rules.xsd` / `pattern.xsd` (extracted from
`languagetool-core.jar`) and the Java that consumes the attributes
(`PatternRuleHandler`, `PatternRuleMatcher`, `Match`/`MatchState`,
`CaseConversionHelper`).

| Construct | Meaning in LT | Action |
|---|---|---|
| `<example>` inside `<pattern>`/`<antipattern>` | Allowed by `rules.xsd` in `<antipattern>` ("this element should appear at the end"); documentation for the antipattern, not part of matching | **Parsed and ignored** (`PatternPart::Example`) |
| `tags="picky"` on rule/rulegroup | `Tag.PICKY` = LanguageTool's `Level.PICKY`; off by default | **Supported**: propagated from rulegroup to rules and the rule is `disable()`d by default |
| `type="…"` on rule/rulegroup | ITS 2.0 localization-quality label (`grammar`, `typographical`, …); no effect on matching/suggestions | **Parsed and ignored** (`kind`) |
| `type="smart"\|"exact"` on `<regexp>` | `smart` (default) replaces literal spaces outside `[...]` with a whitespace matcher (`PatternRuleHandler.replaceSpacesInRegex`); `exact` leaves the regexp | **Supported** (`replace_spaces_in_regex`) |
| `missing field text` (empty `<marker></marker>`) | A zero-width marker marks an insertion/deletion point | **Supported**: marker text is optional (empty) |
| `missing field $value` (empty `<suggestion></suggestion>`) | Suggest deleting the match | **Supported**: suggestion parts default to empty (empty replacement) |
| `case_conversion="preserve"` | `CaseConversionHelper`: uppercase the replacement if the match was uppercase (all-uppercase if the match was all-uppercase, else first char) | **Supported** (`Conversion::Preserve`, uses the matched text as the sample) |
| `(?)` in a regexp (`(?)id`, `(?)ife`) | Java accepts `(?)` as an empty inline-flag group (no-op) | **Supported**: dropped during Java→Rust translation |
| `setpos` on `<match>` | Writes the matched POS back onto the token | **Parsed**; only appears together with `postag`, which is already rejected, so it has no effect on the result |
| `postag` on `<match>` inside a `<token>` | Inflection synthesis of the referenced match | **Parsed**, rejected explicitly (changes behaviour; synthesis is M4C's scope) |
| `raw_pos="yes"` on `<pattern>` | Match POS tags on the tagger output *before* disambiguation | **Rejected explicitly** (nlprule keeps only disambiguated readings at runtime) |
| `distance_tokens` / `min_prev_matches` on rule/rulegroup | Repetition experiment: fire only after N earlier matches within M tokens; needs cross-match state | **Rejected explicitly** |
| `<phraseref>` | Reference to a `<phrase>`; nlprule does not expand `<phrases>` | **Rejected explicitly** |

So the only construct that changes matching or suggestions and is still unimplemented
is the listed set: `raw_pos`, `distance_tokens`/`min_prev_matches`, `postag`/synthesis
on a token's `<match>`, and `<phraseref>`.

## Conformance

Grammar (the `test` binary):

| Rule set | Runnable before | Passing before | Runnable after | Passing after | Pass rate after |
|---|---:|---:|---:|---:|---:|
| 6.6 | 4,011 | 3,576 | 5,235 | 4,644 | 4,644 / 6,098 = 76.2% |
| 5.2 | 3,749 | 3,749 | 3,812 | 3,811 | 3,811 / 4,366 = 87.3% |

(6.6 before pass rate: 3,576 / 6,098 = 58.6%; 5.2 before: 85.9%.)

`comm` of the sorted passing-ID lists: **no rule that passed before fails after** in
either rule set. Newly compiled / newly passing / newly failing:

| Rule set | newly compiled | newly passing | newly failing |
|---|---:|---:|---:|
| 6.6 | 1,224 | 1,068 | 156 |
| 5.2 | 63 | 62 | 1 |

### First 20 newly failing 6.6 rules (reason = the first failing example)

| Rule | Example / reason |
|---|---|
| `CASING/CAPITALIZATION/10` | "The main settlements on the west coast are around Sai Khao…" needs no change (fire on proper-noun run) |
| `CASING/CAPITALIZATION/2` | "What did you do on Valentine's day last year?" expected `Valentine's Day` |
| `CASING/CAPITALIZATION/3` | "What did you do on New Year's eve?" expected `New Year's Eve` |
| `COLLOCATIONS/ATD_VERBS_TO_COLLOCATION/1` | "Men's wants become greater…" expected no match |
| `COLLOCATIONS/COLLOCATION_ERRORS_BOKOMARU/135` | "Isn't this your father?" expected no match |
| `COLLOCATIONS/COLLOCATION_ERRORS_BOKOMARU/161` | "I'm not what we call perfectly bilingual." expected a match |
| `COLLOCATIONS/COLLOCATION_ERRORS_BOKOMARU/164` | "He was sitting at the kitchen's table." expected no match |
| `COLLOCATIONS/COLLOCATION_ERRORS_BOKOMARU/169` | "Over the next year, APL and the Army will decide on the center's location…" expected no match |
| `COLLOCATIONS/COLLOCATION_ERRORS_BOKOMARU/64` | "He lives from his brother’s money." expected a match |
| `COMPOUNDING/PEER_REVIEW_HYPHEN/2` | "We didn't peer review it." expected `peer-review` |
| `COMPOUNDING/PEER_REVIEW_HYPHEN/3` | "Let's peer review your work." expected `peer-review` |
| `COMPOUNDING/PEER_REVIEW_HYPHEN/5` | "We're peer reviewing your work." expected `peer-review` |
| `CONFUSED_WORDS/AD_ADD/3` | "I wouldn't ad a new one." expected `add` |
| `CONFUSED_WORDS/ADJECTIVE_ADVERB/0` | "It won't possible train you." expected `possibly` |
| `CONFUSED_WORDS/ADJECTIVE_ADVERB/15` | "I didn't voluntary do this." expected `voluntarily` |
| `CONFUSED_WORDS/ADJECTIVE_ADVERB/16` | "This couldn't potential increase our dues…" expected `potentially` |
| `CONFUSED_WORDS/ADJECTIVE_ADVERB/31` | "Tom didn't automatic disable the feature." expected `automatically` |
| `CONFUSED_WORDS/ADJECTIVE_ADVERB/7` | "I didn't understandable explain the issue." expected `understandably` |
| `CONFUSED_WORDS/ALREADY_ALL_READY/7` | "I've all ready bought it." expected `already` |
| `CONFUSED_WORDS/AN_AND/14` | "Do you and your team think there's and opportunity for us?" expected `an` |

The pattern is consistent: many newly compiled rules are built around apostrophe
contractions (`didn't`, `won't`) and negation/auxiliary patterns that nlprule's tagger
resolves differently from LT's. They are matcher/tagger gaps, not structure bugs, and
are reported here rather than fixed.

The single newly failing 5.2 rule is
`PUNCTUATION/UNLIKELY_OPENING_PUNCTUATION/0`: the example `:-|` should be excluded by
an antipattern for smileys, but nlprule now flags it as a loose punctuation mark. The
rule only compiles because empty `<suggestion/>` is now supported; the antipattern
mismatch is a pre-existing matcher limitation.

### Disambiguation (`test_disambiguation`)

| Rule set | Runnable | Passing |
|---|---:|---:|
| 6.6 before | 1,015 | 979 |
| 6.6 after | 1,015 | 979 |
| 5.2 before | 843 | 843 |
| 5.2 after | 843 | 843 |

Unchanged.

### Picky rules kept compiled but off by default

`tags="picky"` (`Level.PICKY`) rules are compiled and tested but `disable()`d so the
default behaviour matches LT. Of the rules that now compile: **83** picky rules in
6.6, **11** in 5.2 (24 / 169 picky rules in the XML overall; the rest still fail for
unrelated feature reasons). Verified at runtime: `REDUNDANCY/REASON_WHY`,
`TYPOGRAPHY/ELLIPSIS` and other picky rules no longer fire, while non-picky rules do.

## Recall (`bench/tools/public_recall.py`)

| Binaries | Result | Floor |
|---|---|---|
| 5.2 (`storage/en_*.bin`) | 51 / 60 | 51 |
| 6.6 (`storage/lt66bins/en_*.bin`) | 52 / 60 | 0 |

## Gates

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features` — no errors
  (and no warnings in the touched files).
- `cargo test --workspace --exclude nlprule-python --all-features --release` — all
  pass. One pre-existing test, `suggest_indices_are_relative_to_input_text`, was made
  robust: newly compiled rules (e.g. `TYPOGRAPHY/CURRENCY` on `10€`) add suggestions,
  so it looks the two `due` matches up by position instead of by index.

## Note for integration

`.github/workflows/ci.yml` runs the grammar conformance step on 5.2 and asserts
`Runnable == Rules passing tests`. 5.2 is now 3,812 runnable / 3,811 passing because
of the single newly compiled rule above, so that step will fail until it is updated
(e.g. assert `passing >= <new floor>` and allow a small known-failure list, or keep
the strict equality on a runnable subset). The `test_disambiguation` floor (843) still
holds. This milestone deliberately reports passing < runnable; the CI check needs a
decision from the coordinator.
