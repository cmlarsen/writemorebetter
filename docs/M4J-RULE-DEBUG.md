# M4J: why four rules did not fire

Setup: LanguageTool 6.6 build dir generated as the CI 6.6 job does
(`build/fetch_lt.sh` + `build/make_build_dir.py`), current `main`, compiled to
`storage/en_*` and probed with `bench/tools/wmb-probe`. LT 6.9 behavior checked
with `bench/tools/lt_api.py` and, for chunk tags, by running the 6.6
`EnglishChunker` against the release jars (so chunk comparisons are exact).

All commands are on the merged tree (origin/main incl. M2H; format versions
tokenizer 3 / rules 5).

## 1. THOUGH_THROUGH — no engine bug, the premise is wrong

Neither our engine nor LanguageTool flags **"Digging though the drawer, she
found the key."** The rule's pattern starts with the verb list
`read|go|look|fly|drive|swim|run|walk|break|scroll`; "dig" is not in it. The
public API (6.9) returns no match for the sentence, and matches for
"Reading though the drawer…" and the rule's own examples. Our engine behaves
identically (`Reading though` → `TYPOS/THOUGH_THROUGH/0`), and the rule passes
its examples. Nothing to fix.

## 2. A_NNS_AND — fixed; A_NNS/1 — matches LT's chunker, blocked by an antipattern

**A_NNS_AND (fixed).** The rule needs `pens` in an `I-NP-singular` chunk and
`paper` in an `E-NP-singular` chunk. nlprule's number rule was LanguageTool
5.2's (a noun phrase is plural if *any* token has an NNS reading), so
"a pens and paper" was plural and the rule never matched. LanguageTool 5.9
added `endOfNounPhraseIsSingular`: a phrase is singular unless its **last**
token is plural. Our chunker now implements that, and `A_NNS_AND/0` passes its
example. The exact invented sentence ("…a boxes and crates.") is not a valid
trigger for LT either — the final noun must be singular (`paper`, not `crates`).

The behavior is version-specific, so it is gated by a
`tokenizer_options.json` key (`chunk_number_from_last_token`), which
`build/make_build_dir.py` writes for LT ≥ 5.9. The published 5.2 `en.zip` has no
options file and keeps the 5.2 behavior; without the gate, six 5.2 rules that
were written against the 5.2 chunker regressed.

**A_NNS/1 (not fixed).** Its `triggers_error` example is
"The black budget is an account expenses and spending related to military
research and covert operations." Our chunks match LT's exactly (`an`
B-NP-plural, `account` I-NP-plural, `expenses` E-NP-plural, `and` O,
`spending` B-VP), and the LT 6.9 API flags "an account expenses" with `A_NNS`.
Our engine suppresses the rule with the group antipattern
`an? NN|NN:UN? E-NP-plural NNS (and|or) (min=0 .-NP-plural) (min=0 E-NP-plural)`.

The difference is LanguageTool's optional-token matcher. In
`AbstractPatternRulePerformer.matchFrom`, an optional (`min="0"`) token that
fails at a position can only be skipped if a *later* pattern token matches at
that same position (the lookahead), and the following token is then tested at
its own computed position. Consecutive trailing optionals therefore cannot all
be skipped: LT rejects the prefix match, the antipattern does not match, and the
rule fires. nlprule's regex-like `Quantifier(0, 1)` lets the pattern match a
shorter prefix, so the antipattern matches and the rule is blocked. Reproducing
LT's lookahead exactly is a change to the composition engine, out of scope for
this fix; the rule still fails only this one `triggers_error` example.

## 3. PHRASE_REPETITION — fixed (compiler)

The rule is present in 6.6 but was dropped while compiling, not at match time:
one of its antipatterns contains `<match case_conversion="startupper" no="1"/>`
inside a `<token>`, and `parse_token` rejected *every* attribute on an in-token
`<match>` as "synthesis / case conversion … not implemented". Only the synthesis
attributes (`postag`, `postag_replace`, `regexp_match`, `regexp_replace`,
`include_skipped`) need rejecting. The reference matcher now applies
`case_conversion` to the referenced token's text before comparing
(`Matcher::reference_conversion`), so the antipatterns work and the rule fires:

    She said it, and this is this is the problem.
    → GRAMMAR/PHRASE_REPETITION/0 ["this is"]

It passes its examples, including the `case_conversion` antipattern "as big as
Big Ben".

## 4. IS_VBZ/7 — fixed (suggestion formatting)

For "It is appears to be broken." we emitted `IS_VBZ/7` with
`["is appearing", " appears"]`. The second suggestion is `\3 \4`, and token 3
(the optional `RB`) did not take part, so the reference expanded to "" and left
the literal space. LanguageTool's `PatternRuleMatcher.concatWithoutExtraSpace`
removes one adjacent space when a reference expands to the empty string (unless
the literal space is at the very start of a suggestion, e.g. `PUNCTUATION/
NO_COMMA_BEFORE_INDIRECT_QUESTION`'s `" where"`, which LT really does produce).
`Synthesizer::alternatives` now does the same. LT gives
`["is appearing", "it appears", "appears"]`.

Before/after over `bench/public` and the first 150 KB of Moby-Dick (5.2
binaries, `SPELL=`):

| | replacements with leading/trailing space |
|---|---|
| before | 3 (`A_NNS/1` ×2, `NO_COMMA_BEFORE_INDIRECT_QUESTION` ×1) |
| after  | 1 (`NO_COMMA_BEFORE_INDIRECT_QUESTION`, which LT also produces) |

## Gates (merged tree)

| | 5.2 | 6.6 |
|---|---|---|
| Grammar passing / runnable | 4320 / 4335 (floor 4319 / 4334) | 5977 / 6032 (floor 5959 / 6031) |
| Grammar regressions vs baseline | 0 | 0 |
| Disambiguation passing | 843 (floor 843) | 1012 (floor 1011) |
| Public recall | 54/60 (floor 54) | 55/60 (floor 55) |

`cargo fmt --all -- --check`, `cargo clippy --workspace --exclude nlprule-python
--all-features`, `cargo test --workspace --exclude nlprule-python --all-features
--release` (5.2 storage binaries) and the wasm build all pass. The rules binary
shrank from ~119 MB to ~95 MB loaded with M2H merged.
