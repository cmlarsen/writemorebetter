# M4H: why compiled LanguageTool 6.6 rules fail their own examples

Measured 2026-10-11 on an Apple M5 Pro, rustc 1.97.1, release builds, `RUST_LOG=WARN`.
Two build dirs, exactly as CI builds them:

- **5.2** — `data/en` (upstream `en.zip`, the LT 5.2 rule set).
- **6.6** — `data/en-lt66`, generated from the LanguageTool 6.6 desktop release by
  `build/make_build_dir.py` (build/README.md, "Generating a build dir from the latest
  LanguageTool release").

Before this milestone the 6.6 grammar run was **5,718 runnable / 4,998 passing
(720 failing)** and the disambiguation run **1,015 / 979 (36 failing)**; 5.2 was
4,055 / 4,052 and 843 / 843.

## Part 1 — categorizing the 720 grammar failures

Method: run `test --list-failing` and collect every failure message
(`RUST_LOG=WARN`; the grammar rule test logs one `warn!` per failing example). 720
rules produced 1,093 failing examples. Each rule was then matched against its
`<rule>` element in `data/en-lt66/grammar.xml` and the pattern/examples inspected by
hand. The counts below are rule counts and sum to 720.

| Cause | Rules | 2 example rule IDs | Evidence |
|---|---:|---|---|
| **English contraction tokenization differs between 5.2 and 6.6** | **632** | `STYLE/PASSIVE_VOICE_SIMPLE/0`, `TYPOS/IT_IS_2/24` | 664 of the 720 failing rules have an apostrophe in a failing example; enabling the 6.6 tokenizer behavior fixes 632 of them with no regressions (below). 5.2 tokenizes `haven't` as `haven` + `'` + `t`; 6.6 tokenizes it as `have` + `n't` (the auxiliary stem loses the `n`, the clitic keeps the apostrophe, and the apostrophe is ASCII even when the input is curly: `It’s` → `It` + `'s`). |
| Suggestion backreferences with two digits are parsed as one digit plus a literal | 34 | `STYLE/PASSIVE_VOICE_SIMPLE/1`, `STYLE/PASSIVE_VOICE_SIMPLE/12` | The suggestion `\13 \14 \15 have not \7 \9 <match no="10"/> \11 …` renders as `3 4 5 have not saved 1 the children`: `\13` becomes `\1` + `3`, `\11` becomes `\1` + `1`. |
| Marker / suggestion span boundary (the suggestion starts one token earlier upstream) | 5 | `GRAMMAR/NNP_AGREEMENT/0`, `PUNCTUATION/HYPHEN_TO_EM_OR_COMMA/1` | Expected span `byte 3..8`, produced `byte 4..8` (the `make` in `Tim <marker>make</marker>`); expected replacement `" makes"` with a leading space, produced `" makes"` at the wrong span. |
| Other matcher / tagger / disambiguation differences (chunker output, POS tags, regex semantics; includes 25 "expected suggestion not produced", 16 "matched a no-change example" and 8 mixed) | 49 | `TYPOS/WORD_CONTAINS_UNDERSCORE/0`, `CASING/CAPITALIZATION/10` | e.g. `PUNCTUATION/TWO_HYPHENS/0` never fires on `--`, `TYPOGRAPHY/CONSECUTIVE_SPACES/0` never fires on a double space, `GRAMMAR/A_NNS_AND/0` never fires on `a pens and paper`. |

The disambiguation failures (36) split the same way:

| Cause | Rules | 2 example rule IDs | Evidence |
|---|---:|---|---|
| Contraction tokenization (the 6.6 disambiguation rules are written for `do` + `n't` / `it` + `'s`) | 32 | `DISAMBIGUATION/DONT_VBNN_VB/0`, `DISAMBIGUATION/AIN_T_FILTER_NN/0` | fixed by the same tokenizer option, see below. |
| Tag assignment differs after the change (token texts equal, `WordData` tags differ) | 4 | `DISAMBIGUATION/VBZNNS_NNS/0`, `DISAMBIGUATION/UPPER_NNP/0` | the `Before`/`After` token texts are identical; the rule's expected tag change is missing. |

## Part 2 — the fix (the largest cause)

The largest cause is the English word tokenizer. LanguageTool 6.6 rewrote
`EnglishWordTokenizer`; nlprule's tokenizer implements the 5.2 behavior:

| Input | nlprule (5.2 behavior) | LanguageTool 6.6 |
|---|---|---|
| `haven't` | `haven` `'` `t` | `have` `n't` |
| `ain't` | `ain` `'` `t` | `ai` `n't` |
| `it's` | `it` `'` `s` | `it` `'s` |
| `It’s` (curly) | `It` `’` `s` | `It` `'s` (ASCII) |
| `o’clock` | `o` `’` `clock` | `o'clock` (whole, in the tagger dictionary) |

The 6.6 rules and disambiguation rules are written against the 6.6 tokenization, so
they never matched.

### Implementation

The behavior is a **per-build-directory tokenizer option**, not a code-path change:

- `TokenizerLangOptions` gains `split_contractions` (default `false` — the 5.2
  behavior), serialized into the tokenizer binary.
- `nlprule/src/compile/mod.rs` reads an optional `tokenizer_options.json` from the
  build directory and applies it. Without the file nothing changes.
- `build/make_build_dir.py` writes `tokenizer_options.json`
  (`{"split_contractions": true}`) when the LanguageTool release directory is
  `LanguageTool-6.6` or newer and the language is English.
- The tokenizer (`nlprule/src/tokenizer.rs`) implements LanguageTool 6.6's
  `EnglishWordTokenizer` word splitting: the `patternList` for negated auxiliaries
  (`(['’]?)(are|is|…|use)(n['’]t)`), apostrophe clitics
  (`(.+)(['’]m|['’]re|['’]ll|['’]ve|['’]d|['’]s)(['’-]?)`), the `'t` + `was/were/is`
  form, and the small whole-word list; a word whose normalized form is known to the
  tagger stays whole, otherwise it is split at apostrophes. Recognized contractions
  are emitted with a straight ASCII apostrophe (matching the released 6.6 jar), so
  the tokenizer keeps the original byte span but stores an owned normalized token
  text (`Word`/`WordId` already allow an owned `Cow`).

The 5.2 build directory has no `tokenizer_options.json`, so 5.2 is byte-for-byte the
same behavior as before.

## Conformance before/after

`test` / `test_disambiguation` with `--list-passing` / `--list-failing`:

| Rule set | Runnable | Before passing | After passing | After pass rate |
|---|---:|---:|---:|---:|
| 6.6 grammar | 5,718 | 4,998 | **5,630** | 5,630 / 5,718 = 98.5% |
| 6.6 disambiguation | 1,015 | 979 | **1,011** | 1,011 / 1,015 = 99.6% |
| 5.2 grammar | 4,055 | 4,052 | 4,052 | 99.9% |
| 5.2 disambiguation | 843 | 843 | 843 | 100% |

Diffed passing lists (`comm -13` = newly passing, `comm -23` = regressed):

| Rule set | Newly passing | Regressed |
|---|---:|---:|
| 6.6 grammar | 632 | **0** |
| 6.6 disambiguation | 32 | **0** |
| 5.2 grammar | 0 | **0** |
| 5.2 disambiguation | 0 | **0** |

The lists themselves are committed next to this document:

- `docs/m4h/lt66-grammar-newly-passing.txt` (632), `docs/m4h/lt66-grammar-regressed.txt` (empty)
- `docs/m4h/lt66-disamb-newly-passing.txt` (32), `docs/m4h/lt66-disamb-regressed.txt` (empty)
- `docs/m4h/lt52-grammar-regressed.txt`, `docs/m4h/lt52-disamb-regressed.txt` (empty)

No rule that passed before fails after, for either version.

## Public recall

`bench/tools/public_recall.py` on `bench/public`, both binaries (probing a directory
holding `en_tokenizer.bin` / `en_rules.bin`):

| Binaries | Result | Floor (.github/workflows/ci.yml) |
|---|---|---|
| 5.2 | 53 / 60 | 53 |
| 6.6 | 54 / 60 | 54 |

Unchanged.

## Gates

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features` — exit 0, no errors
  (the pre-existing warnings are unchanged).
- `cargo test --workspace --exclude nlprule-python --all-features --release` — all pass.

CI note: the existing floors (6.6 grammar 4,998, disambiguation 979; 5.2 unchanged)
still hold and now have headroom; only raising the floors would need a CI change, which
this milestone deliberately does not make.
