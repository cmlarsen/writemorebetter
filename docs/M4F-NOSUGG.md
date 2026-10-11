# M4F: rules without a `<suggestion>` and examples with a `type`

Measured 2026-10-11, Apple M5 Pro, rustc 1.97.1, release builds, `RUST_LOG=WARN`.
Two build dirs as before: **5.2** (`data/en`, the published Backblaze `en.zip`) and
**6.6** (`data/en-lt66`, generated per `build/README.md`).

LanguageTool reports a match for a rule even when the rule has no `<suggestion>`:
the match carries the rule's `<message>` and no replacement. Its
`PatternRuleHandler` also classifies every `<example>` by its `type` attribute
(`correct` / `incorrect` / `triggers_error`) and, absent one, by whether the
example marks an error with `<marker>` or a `correction`. This milestone
implements both, so both compiler errors are gone and the affected rules compile
and are tested.

## What changed

- **Rules with no suggestion** compile instead of being rejected. `Suggestions`
  now emits a match with an empty replacement list for them (the speller already
  did). Rules that do have suggesters keep the previous behaviour of suppressing
  a match whose replacement list is empty.
- **Examples with a `type`** compile. Each example records its LanguageTool kind
  and, when present, the `<marker>` span, and is tested the way
  `PatternRuleTest` does:
  - `correct` (and untyped examples with no marker/correction): the rule must not
    match;
  - `incorrect` (or an untyped example with a `correction`, or a marker on a rule
    with no suggestion): the rule must match exactly once, at the marker span;
    `correction=""` (and a rule without a suggestion) is satisfied by an empty
    replacement list;
  - `triggers_error`: the rule must match; position and correction are not
    checked.

Sources: `rules.xsd`/`pattern.xsd` from `languagetool-core.jar`, and
`PatternRuleHandler` / `PatternRuleTest` (LT 6.6).

## 1. Compile errors before → after

| Error | LT 5.2 before | after | LT 6.6 before | after |
|---|---:|---:|---:|---:|
| `rules with no suggestion are not implemented` | 104 | **0** | 126 | **0** |
| `examples with type (... 'triggers_error') are not implemented` | 93 | **0** | 90 | **0** |
| **new error entries introduced** | — | **0** | — | **0** |

No new error kinds appear: the only difference in the full error sets is the
removal of the two rows above (verified with a `diff` of the sorted,
count-stripped error lines). The two Oniguruma regex errors (`target of repeat
operator is invalid` ×2, `Look-behind assertion without constant size` ×2) are
present before and after; they were previously reported alongside them. The
remaining failures are the other out-of-scope features (`filter` 34/71,
`+DT`/`+INDT` 33/35, `include_skipped` 15/14, regex 4/4, repetition 8 (6.6),
`attributes on <match> in <token>` 1/3, `raw_pos` 1/2, control flow 1/1,
`phraseref` 1/0).

## 2. Conformance

Grammar (`test` binary; `<rule>` elements: 4,366 for 5.2, 6,098 for 6.6):

| Rule set | Runnable before | Runnable after | Passing before | Passing after | Pass rate before | Pass rate after |
|---|---:|---:|---:|---:|---:|---:|
| 5.2 | 4,055 | 4,252 | 4,052 | **4,187** | 4,052 / 4,366 = 92.8% | **4,187 / 4,366 = 95.9%** |
| 6.6 | 5,718 | 5,934 | 4,998 | **5,121** | 4,998 / 6,098 = 82.0% | **5,121 / 6,098 = 84.0%** |

`comm` of the sorted passing-ID lists (before minus after): **0** rules that
passed before fail after, in either rule set; **0** rules were removed.
Disambiguation is unchanged: 5.2 843 / 843, 6.6 1,015 / 979.

## 3. Newly compiled rules

| Rule set | Newly compiled | Newly passing | Newly failing |
|---|---:|---:|---:|
| 5.2 | 197 | 135 | 62 |
| 6.6 | 216 | 123 | 93 |

The newly compiled no-suggestion rules are 104 (5.2) / 126 (6.6); of those 48 /
60 pass. Newly compiled rules whose examples include a `triggers_error` example
are 105 (5.2) / 97 (6.6); of those 91 / 64 pass. (A rule can have both a
`triggers_error` example and no suggestion, and a handful still fail for an
unrelated reason, so these counts differ slightly from the error counts above.)

Cause of the newly failing rules: 59/62 (5.2) and 70/93 (6.6) match a *correct*
example (an nlprule tagger/matcher gap, the same kind of gap M4D reported);
3/23 instead fail to produce the correction their example expects. The first 20
newly failing in each set:

### 5.2 — first 20 newly failing

| Rule | First failing example | Kind of failure |
|---|---|---|
| `CREATIVE_WRITING/E_PRIME_LOOSE/0` | This is conforming with my current knowledge. | matched a correct example |
| `CREATIVE_WRITING/E_PRIME_STRICT/0` | This does not conform with my current knowledge. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/0` | It is a commonly used tool for spammers. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/1` | It is a popular commonly used tool for spammers. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/2` | The initially installed software is working. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/3` | Barbecue Chicken Sandwich is one of the most frequently ordered dishes. | matched a correct example |
| `GRAMMAR/ADVERB_WORD_ORDER/0` | I am always happy. | matched a correct example |
| `GRAMMAR/ADVERB_WORD_ORDER/1` | Ever since Portugal handed over sovereignty | matched a correct example |
| `GRAMMAR/ADVERB_WORD_ORDER/2` | I am always happy. | matched a correct example |
| `GRAMMAR/ADVISE_VBG/0` | He reminded them to stay calm. | matched a correct example |
| `GRAMMAR/ARTICLE_ADJECTIVE_OF/8` | We are not responsible for the delusions of your client. | matched a correct example |
| `GRAMMAR/A_INFINITIVE/0` | This is a complete catastrophe | matched a correct example |
| `GRAMMAR/A_LOT_OF_NN/0` | A lot of California used to be under water. | matched a correct example |
| `GRAMMAR/BE_I_BE_GERUND/0` | What was I doing again? | matched a correct example |
| `GRAMMAR/BE_WILL/0` | How would this approach be useful? | matched a correct example |
| `GRAMMAR/DID_BASEFORM/1` | The earliest anarchist themes can be found in the 6th century BC. | matched a correct example |
| `GRAMMAR/DOUBLE_NEGATIVE/15` | I haven't said no yet. | matched a correct example |
| `GRAMMAR/MANY_NN/0` | The best ones succeed, but many fall by the wayside… | matched a correct example |
| `GRAMMAR/MISSING_PAST_TENSE/0` | I couldn't go to Australia last year. | matched a correct example |
| `GRAMMAR/PROGRESSIVE_VERBS/0` | I doubt these results. | matched a correct example |

The 3 remaining (not in the first 20) that expect a correction:
`GRAMMAR/PRP_HAVE_VB/0`, `TYPOS/FROM_FORM/4`,
`TYPOS/WORD_CONTAINS_UNDERSCORE/0`.

### 6.6 — first 20 newly failing

| Rule | First failing example | Kind of failure |
|---|---|---|
| `COMPOUNDING/SOME_TIMES/0` | Some times don't really work for us. | matched a correct example |
| `CONFUSED_WORDS/TO_TOO/9` | It's to early. | expected a match/correction |
| `CREATIVE_WRITING/E_PRIME_LOOSE/0` | This is conforming with my current knowledge. | matched a correct example |
| `CREATIVE_WRITING/E_PRIME_STRICT/0` | This does not conform with my current knowledge. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/0` | It's a common used tool for spammers. | expected a match/correction |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/1` | It's a popular common used tool for spammers. | expected a match/correction |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/2` | The initially installed software is working. | matched a correct example |
| `GRAMMAR/ADVERB_OR_HYPHENATED_ADJECTIVE/3` | Barbecue Chicken Sandwich is one of the most frequently ordered dishes. | matched a correct example |
| `GRAMMAR/ADVISE_VBG/0` | He reminded them to stay calm. | matched a correct example |
| `GRAMMAR/ARTICLE_ADJECTIVE_OF/8` | We are not responsible for the delusions of your client. | matched a correct example |
| `GRAMMAR/AUXILIARY_DO_WITH_INCORRECT_VERB_FORM/0` | Didn’t there used to be a link in the menu… | expected a match/correction |
| `GRAMMAR/A_INFINITIVE/2` | … but that happens very rarely. | matched a correct example |
| `GRAMMAR/A_NNS/1` | The black budget is an account expenses and spending… | matched a correct example |
| `GRAMMAR/A_NNS/3` | In a perfect world spreaders don't exist. | matched a correct example |
| `GRAMMAR/BEEN_PART_AGREEMENT/9` | As mentioned earlier, there are some docs on this topic… | expected a match/correction |
| `GRAMMAR/BE_I_BE_GERUND/0` | its distribution is I'm trying to be nice | expected a match/correction |
| `GRAMMAR/BE_I_BE_GERUND/1` | What was I doing again? | matched a correct example |
| `GRAMMAR/BE_WILL/0` | How would this approach be useful? | matched a correct example |
| `GRAMMAR/DID_BASEFORM/0` | Mine doesn’t exactly squealing though. | expected a match/correction |
| `GRAMMAR/DID_BASEFORM/1` | The earliest anarchist themes can be found in the 6th century BC. | matched a correct example |

## 4. Precision: flags from newly compiled rules (LT 5.2 binaries)

The 5.2 binaries with `bench/tools/wmb-probe` (`SHOW=1`, en_US speller), on
`bench/public/story1..5` and the first 150 KB of Moby-Dick:

| Input | Profile | Flags from new rules | … from new no-sugg rules | … with no replacement |
|---|---|---:|---:|---:|
| story1..5 | default | 4 | 2 | 2 |
| story1..5 | fiction | 4 | 2 | 2 |
| Moby-Dick 150 KB | default | 19 | 9 | 9 |
| Moby-Dick 150 KB | fiction | 19 | 9 | 9 |

Every no-suggestion rule's flag has an empty replacement list, as intended. The
fiction profile does not change any of these counts.

For the 6.6 binaries: 2 flags from new rules on story1..5 (0 from no-sugg rules)
and 13 on Moby-Dick (2 from no-sugg rules, both empty).

### Every public-corpus flag from a newly compiled 5.2 rule, classified

| Where | Rule | Flagged | Replacement | Real error? |
|---|---|---|---|---|
| story1:25 | `GRAMMAR/DT_DT/0` | the the | `the` | **Yes** — matches truth `story1:25 "the the"` (`repeat`) |
| story1:33 | `GRAMMAR/A_NNS/1` | a boxes | `a box` | **Yes** — matches truth `story1:33 "a boxes"` (`grammar`) |
| story2:86 | `GRAMMAR/A_INFINITIVE/0` (no suggestion) | the heal | *(none)* | **Yes** — fires on the `heal`/`heel` typo that truth lists as `story2:86 "heal of"` (`ngram`); the message is a different (still correct) diagnosis |
| story5:17 | `GRAMMAR/DID_BASEFORM/1` (no suggestion) | fell | *(none)* | **No** — "a watering can fell over": correct past tense, a false positive |

So of 4 public flags from newly compiled rules, 3 are real errors and 1 is a
false positive; the 2 no-suggestion-rule flags are one real (the `heal` typo) and
one false positive (`fell`).

## 5. Public recall

`bench/tools/public_recall.py` on `bench/public` (60 verified errors):

| Binaries | Floor | Result |
|---|---:|---:|
| 5.2 (`storage/en_*.bin`, `--min 53`) | 53 | **54 / 60** |
| 6.6 (`storage/lt66bins/en_*.bin`, `--min 0`) | 0 | **55 / 60** |

(The 5.2 floor is the one in `.github/workflows/ci.yml`; the 6.6 job's floor is
54, also met.) Missed by 5.2: `story1:27 is would`, `story1:34 peaked through`,
`story2:86 heal of`, `story3:44 Your welcome`, `story4:46 whether turned`,
`story5:45 dogs are`.

## 6. Speed and memory

`bench/tools/wmb-probe` on Moby-Dick (150 KB), 5.2 binaries, en_US speller, on a
machine with other cargo builds running: tokenizer load 81 ms / 37 MB, rules load
21 ms / 67 MB, speller enabled 68 MB, check 150,000 chars → 1,014 suggestions in
3.25 s at 92 MB. The engine's touch on this milestone is in rule loading and rule
evaluation, not the tagger/speller.

## 7. Gates

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features` — exit 0, no
  errors; no new warnings in the touched files.
- `cargo test --workspace --exclude nlprule-python --all-features --release` —
  all pass (140 `nlprule` integration tests among them). Two tests were made
  robust: a newly compiled no-suggestion rule (`GRAMMAR/PRP_MD_NN/4`) matches
  "can due" and masked the due/do suggestion, so those tests disable that group.
- `test_disambiguation` (5.2) — 843 / 843, unchanged.

Two build dirs and a `git merge origin/main` (which brought the M4E CI changes)
were applied before these gates were re-run.
