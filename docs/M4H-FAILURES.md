# M4H: why compiled LanguageTool 6.6 rules fail their own examples

Measured 2026-10-11 on an Apple M5 Pro, rustc 1.97.1, release builds, `RUST_LOG=WARN`.
Two build dirs, exactly as CI builds them:

- **5.2** — `data/en` (upstream `en.zip`, the LT 5.2 rule set).
- **6.6** — `data/en-lt66`, generated from the LanguageTool 6.6 desktop release by
  `build/make_build_dir.py` (build/README.md, "Generating a build dir from the latest
  LanguageTool release").

The starting point is `origin/main` after M4F (`docs/BASELINE.md`, "Current (after
M4F)"): 6.6 grammar **5,934 runnable / 5,121 passing (813 failing)**, disambiguation
**1,015 / 979 (36 failing)**; 5.2 was 4,252 / 4,187 (65 failing) and 843 / 843. (When
the M4H task was written the baseline was the pre-M4F main with 720 grammar failures;
after merging `origin/main`, which brought in M4F, the runnable set grew and so did
the failing set. The cause below is unchanged.)

## Part 1 — categorizing the 813 grammar failures

Method: run `test --list-failing` and collect every failure message
(`RUST_LOG=WARN`; the grammar rule test logs one `warn!` per failing example). Each
rule was then matched against its `<rule>` element in `data/en-lt66/grammar.xml` and
its pattern and examples inspected by hand. The counts below are rule counts and sum
to 813.

| Cause | Rules | 2 example rule IDs | Evidence |
|---|---:|---|---|
| **English contraction tokenization differs between 5.2 and 6.6** | **663** | `STYLE/PASSIVE_VOICE_SIMPLE/0`, `TYPOS/IT_IS_2/24` | 715 of the 813 failing rules have an apostrophe in a failing example. 5.2 tokenizes `haven't` as `haven` + `'` + `t`; 6.6 tokenizes it as `have` + `n't` (the auxiliary stem loses the `n`, the clitic keeps the apostrophe, and the apostrophe is ASCII even when the input is curly: `It’s` → `It` + `'s`). Enabling the 6.6 behavior makes 663 of the failing rules pass with **zero** regressions (below). |
| Suggestion backreferences with two digits are parsed as one digit plus a literal | 34 | `STYLE/PASSIVE_VOICE_SIMPLE/1`, `STYLE/PASSIVE_VOICE_SIMPLE/12` | The suggestion `\13 \14 \15 have not \7 \9 <match no="10"/> \11 …` renders as `3 4 5 have not saved 1 the children`: `\13` becomes `\1` + `3`, `\11` becomes `\1` + `1`. |
| Marker / suggestion span boundary differs | 5 | `GRAMMAR/NNP_AGREEMENT/0`, `PUNCTUATION/HYPHEN_TO_EM_OR_COMMA/1` | Expected span `byte 3..8`, produced `byte 4..8` (the `make` in `Tim <marker>make</marker>`), with a leading-space replacement. |
| Other matcher / tagger differences (chunker output, POS tags, regex semantics): 76 rules match an example that should not match and 26 fail to match one that should | 111 | `TYPOS/WORD_CONTAINS_UNDERSCORE/0`, `CASING/CAPITALIZATION/10` | e.g. `PUNCTUATION/TWO_HYPHENS/0` never fires on `--`, `TYPOGRAPHY/CONSECUTIVE_SPACES/0` never fires on a double space, `GRAMMAR/A_NNS_AND/0` never fires on `a pens and paper`, `TYPOS/HAS_IT_NNS__IT_ITS/0` fires on `Has it feathers like a bird?`. |

Some rules have more than one cause: a `STYLE/PASSIVE_VOICE_SIMPLE` rule whose
contraction example the fix repairs still fails while its other example carries a
two-digit backreference, so it is counted under the backreference row rather than the
663. The 663 is therefore "rules whose only blocker was tokenization".

The disambiguation failures (36) split the same way:

| Cause | Rules | 2 example rule IDs | Evidence |
|---|---:|---|---|
| Contraction tokenization (the 6.6 disambiguation rules are written for `do` + `n't` / `it` + `'s`) | 32 | `DISAMBIGUATION/DONT_VBNN_VB/0`, `DISAMBIGUATION/AIN_T_FILTER_NN/0` | fixed by the same tokenizer option, below. |
| Tag assignment differs (token texts equal, `WordData` tags differ) | 4 | `DISAMBIGUATION/VBZNNS_NNS/0`, `DISAMBIGUATION/UPPER_NNP/0` | the `Before`/`After` token texts are identical; the rule's expected tag change is missing. |

## Part 2 — the fix (the largest cause)

The largest cause is the English word tokenizer. LanguageTool 6.6 rewrote
`EnglishWordTokenizer`; nlprule's tokenizer implemented the 5.2 behavior:

| Input | nlprule (5.2 behavior) | LanguageTool 6.6 |
|---|---|---|
| `haven't` | `haven` `'` `t` | `have` `n't` |
| `ain't` | `ain` `'` `t` | `ai` `n't` |
| `it's` | `it` `'` `s` | `it` `'s` |
| `It’s` (curly) | `It` `’` `s` | `It` `'s` (ASCII) |
| `o’clock` | `o` `’` `clock` | `o'clock` (whole, in the tagger dictionary) |

The 6.6 rules and disambiguation rules are written against the 6.6 tokenization, so
they never matched. The behaviour was verified against the released 6.6 jar
(`languagetool-commandline.jar -t`): `It’s detractors …` tags as `It` + `'s`,
`didn't` as `did` + `n't`, `o’clock` as a single `o'clock`.

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
  form and the small whole-word list; a word whose normalized form is known to the
  tagger stays whole, otherwise it is split at apostrophes. Recognized contractions
  are emitted with a straight ASCII apostrophe (matching the released 6.6 jar), so the
  tokenizer keeps the original byte span but stores an owned normalized token text
  (`Word`/`WordId` already allow an owned `Cow`).

The 5.2 build directory has no `tokenizer_options.json`, so 5.2 is unchanged.

## Conformance before/after

`test` / `test_disambiguation` with `--list-passing` / `--list-failing`.
"Before" is `origin/main` (3d7e368, post-M4F) built in a separate worktree;
"after" is this branch (which only differs by the tokenizer option).

| Rule set | Runnable | Before passing | After passing | After pass rate |
|---|---:|---:|---:|---:|
| 6.6 grammar | 5,934 | 5,121 | **5,784** | 5,784 / 5,934 = 97.5% |
| 6.6 disambiguation | 1,015 | 979 | **1,011** | 1,011 / 1,015 = 99.6% |
| 5.2 grammar | 4,252 | 4,187 | 4,187 | 98.5% |
| 5.2 disambiguation | 843 | 843 | 843 | 100% |

Diffed passing lists (`comm -13` = newly passing, `comm -23` = regressed):

| Rule set | Newly passing | Regressed |
|---|---:|---:|
| 6.6 grammar | 663 | **0** |
| 6.6 disambiguation | 32 | **0** |
| 5.2 grammar | 0 | **0** |
| 5.2 disambiguation | 0 | **0** |

No rule that passed before fails after, for either version. The lists themselves are
committed next to this document:

- `docs/m4h/lt66-grammar-newly-passing.txt` (663), `docs/m4h/lt66-grammar-regressed.txt` (empty)
- `docs/m4h/lt66-grammar-still-failing.txt` (150, the residual)
- `docs/m4h/lt66-disamb-newly-passing.txt` (32), `docs/m4h/lt66-disamb-regressed.txt` (empty)
- `docs/m4h/lt52-grammar-regressed.txt`, `docs/m4h/lt52-grammar-newly-passing.txt` (both empty)
- `docs/m4h/lt52-disamb-regressed.txt`, `docs/m4h/lt52-disamb-newly-passing.txt` (both empty)

## Public recall

`bench/tools/public_recall.py` on `bench/public`, probing a directory holding
`en_tokenizer.bin` / `en_rules.bin`:

| Binaries | Result | Floor (.github/workflows/ci.yml) |
|---|---|---|
| 5.2 | 54 / 60 | 54 |
| 6.6 | 55 / 60 | 55 |

Meets the floors; the numbers are the post-M4F values.

## Gates

- `cargo fmt --all -- --check` — clean.
- `cargo clippy --workspace --exclude nlprule-python --all-features` — exit 0, no errors
  (the pre-existing warnings are unchanged).
- `cargo test --workspace --exclude nlprule-python --all-features --release` — all pass.

CI note: the floors in `.github/workflows/ci.yml` (6.6 grammar 5,121 / 5,934,
disambiguation 979, recall 55; 5.2 4,187 / 4,252, 843, recall 54) still hold and now
have headroom; raising them is a CI change and deliberately not part of this
milestone.
