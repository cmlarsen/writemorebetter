# Concreteness table (#256): handoff, 2026-10-20

Paused on the owner's instruction (moving to Deltron). The plan being executed is Section 4 of the
diction-table research note, with the coordinator's brief for lane `concreteness-table`. Branch
`cmlarsen/concreteness-table`; no PR opened.

## Decisions so far

- **Claude-only.** OpenCode Go was rejected for the second model family. Its Terms of Service
  (<https://opencode.ai/legal/terms-of-service>, effective 2026-08-15) say "You will only use the
  Services for your own internal use, and not on behalf of or for the benefit of any third party"
  and prohibit use that "automatically or programmatically extracts data or Output". The Go API key
  had also expired (`api_key_expired`, "Signing in again will not renew this key"), so headless
  `opencode run` failed. DeepSeek's own terms would allow the use and Meta's are unclear
  (geography clause). Quotes are in `docs/STYLE-DATA-LICENSES.md#concreteness-256`. The
  coordinator confirmed Claude-only is fine. Codex was not used.
- **Coverage gate (coordinator, option A).** Add 13 lemmas that the fiction list drops (the
  overused-words report excludes them as particles and fillers) but diction's lookup rates:
  `bench/style/concreteness/words-supplement.tsv` (sha256 `a0232fd2…`). Rate them in one extra
  batch per model-sample with the same frozen prompt. Gate pooled bench/public narration coverage
  ≥ 0.94 and every modern story (story1–5) ≥ 0.97, and document Moby-Dick's archaic tail. Do not
  widen the list to Lancaster's word list.
- **Glasgow.** The workbook stores *true* and *false* as boolean cells; the stdlib parser reads
  them as words. That gives 4,682 single-word items, which matches openpyxl on all values once
  booleans are kept, plus 871 sense items.

## Frozen inputs

| File | sha256 |
|---|---|
| `prompt.txt` (pilot instructions and anchors, `[system]` / `[user]` sections) | `0317ef4beee2b11db9719e811af732839d40536bfaac54b1d20266922b990871` |
| `words.tsv` (Section 4.1: 14,364 rated lemmas + 195 British spellings rated as the American key) | `d48a0c8791ab536eea60fb320ab9358d6d3093733d01b43d36e105e787378966` |
| `words-supplement.tsv` (13 lemmas, option A) | `a0232fd2483e57ca28ad5ba48ae23df0543c77209f254b7206296948ef48de7c` |

`rate.py` and `scripts/build-concreteness-norms.py` refuse files with other hashes. The prompt and
the main list were committed (86c1a06) before any scoring.

## Ratings

Complete and committed under `raw/` (14,364 words each; `*.responses.jsonl` holds every reply,
including rejected ones):

| Model | Samples complete | Remaining |
|---|---|---|
| claude-opus-5-5 | 1, 2 | 3 |
| claude-sonnet-5-5 | 1, 2 | 3 |
| claude-fable-5-1 | 1, 2 | 3 |
| claude-haiku-5-5 | 1 | 2, 3 (sample 2 stopped at 3,750 words and was dropped, not committed) |

No pass has the supplement batch (`x000`) yet. The build now requires those 13 words, so
`build` exits with "missing 13 words" until the supplement is rated.

How they were run: Claude Code CLI 2.1.304, Claude Team subscription (Commercial Terms), with
`--jobs 8`. Each call ran from an empty scratch directory:

```sh
claude -p --model <id> --tools "" --system-prompt "<[system] text of prompt.txt>" \
  --strict-mcp-config --setting-sources "" --no-session-persistence --output-format json "<user message>"
```

The context is about 645 tokens (the system prompt plus the CLI's account and environment
reminders). Temperature and effort are the CLI defaults; Opus used 0 thinking tokens on a test
batch, and responses record `output_tokens` and `thinking_tokens`. Each batch is 50 words. Each
pass shuffles with seed `int(sha256("<model>:<sample>")[:8], 16)`, and the supplement adds 1 to
the seed. Replies are parsed strictly: every word exactly once, rating `[1-5](\.\d)?`, any
order. Malformed replies are retried up to 5 times; Haiku needed 7 retries in sample 1, all
recorded.

To resume (finished batches are skipped), run from the repo root with a scratch `--cwd`:

```sh
python3 bench/style/concreteness/rate.py \
  claude:claude-haiku-5-5:2 claude:claude-opus-5-5:3 claude:claude-sonnet-5-5:3 \
  claude:claude-fable-5-1:3 claude:claude-haiku-5-5:3 --jobs 8 --cwd <empty dir>
# then the supplement for every pass (12 single batches):
python3 bench/style/concreteness/rate.py --supplement --no-main \
  claude:claude-{opus-5-5,sonnet-5-5,fable-5-1,haiku-5-5}:{1,2,3} --jobs 8 --cwd <empty dir>
```

A full pass took about 4–10 minutes at 8 concurrent calls (about 0.4–1 batch/s overall).

## Interim results (7 complete passes, supplement excluded, before adjudication)

Fit on 1,221 calibration words: 0.8747 × z + 3.1162. Held-out Glasgow (70%, 2,879 words):
Spearman **0.921**, Pearson **0.923**, abstract F1 at 2.5 **0.753**, MAE **0.288**. Lancaster +
WordNet ridge convergent Spearman **0.807**. Sample-1 vs sample-2 Spearman: Opus 0.991, Fable
0.990, Sonnet 0.985. Agreement with the other models' consensus: Opus 0.987, Fable 0.982,
Sonnet 0.981, Haiku 0.933. QA review list: 226 words. Every gate in Section 4.6 passes so far.

Coverage of narration tokens diction can rate (`style/examples/diction_coverage.rs`; the
denominator is diction's own lookup over a table that rates every lemma and surface), with the
Opus-only provisional table:

| | Pooled | Moby | Stories 1–5 |
|---|---:|---:|---:|
| Section 4.1 list | 0.910 | 0.895 | 0.932–0.958 |
| + 13 supplement words | 0.946 | 0.931 | 0.978–0.984 |

## Code state (WIP, committed)

- Rust: `ConcretenessTable::bundled()` (`include_str!` of `style/data/norms/concreteness.tsv`); TSV
  parsing skips `#` lines; `StyleDocument::diction(options, None)` uses the bundled table, and a
  caller table still overrides it. `StyleError::MissingTable` is kept for compatibility but no
  longer returned (its constructor was removed). Tests are updated and added in
  `style/tests/diction.rs` and `cross_report.rs`, and they pass with the provisional table.
- WASM and Swift: doc comments updated, including the regenerated-binding text in
  `Generated/WriteMoreBetter.swift` (the CI diff check needs the regenerated file to match).
  Swift's `diction(table: String? = nil, …)`. `wasm/test.mjs` and the Swift tests are updated but
  were **not run** (the Swift test suite needs the XCFramework).
- `style/examples/norm_reports.rs` uses the bundled table by default. `diction_coverage.rs` is the
  coverage probe.
- `scripts/build-concreteness-norms.py`: `words`, `build [--check]`. It z-scores each model's
  sample mean, averages per family (only one family now), fits OLS on the Glasgow calibration split
  (hash split, seed `glasgow-split-256`, 30%), clamps to 1–5, rounds half-even to one decimal and
  applies `adjudication.tsv` (word, before, after, reason; `before` must match). British rows copy
  their American value. It fails if any model's agreement with the other models' consensus is
  below 0.85, and writes `qa-review.tsv` (spread > 1.5 or |table − ridge| > 1.5).
- `bench/style/concreteness.py [--gate] [--write] [--probe …]` and `test_concreteness.py` (13
  tests, passing). `sources.json` pins `glasgow`, `lancaster` (OSF 48wsc) and `wordnet` (WNdb-3.0,
  sha256 `658b1ba1…`). `ci.yml` runs the gate after `norms.py`.
- Docs done: the STYLE-DATA-LICENSES Concreteness section, NOTICE, the API text in STYLE-REPORTS,
  and the wasm and swift READMEs.

**`style/data/norms/concreteness.tsv` is provisional**: it was built from Opus sample 1 only so the
crate compiles. Regenerate it with `build` once every pass is rated.

## Remaining

1. Finish the ratings above (Haiku s2, all s3, 12 supplement batches) and commit each pass.
2. Build, then review `qa-review.tsv` as a lexicographer and record few, principled edits in
   `adjudication.tsv`, then rebuild.
3. Coverage gate per option A: change `MINIMUM_POOLED_COVERAGE` to 0.94 and add a per-story ≥ 0.97
   check for story1–5 in `bench/style/concreteness.py`. Run `--write`, review and commit
   `concreteness-results.json`; `--gate` then compares against it.
4. Downstream review: run `norm_reports` (bundled table, `abstractBelow` 2.5) on bench/public
   plus Crane *Red Badge* ch. 1 (standardebooks `ebe3876c`), Wharton *Ethan Frome* ch. 1
   (`480cd944`) and Gatsby ch. 1 (`fe47c8c0`), converted with
   `pacing_labels.xhtml_document`. Sample 20 abstract sentences from the three chapters with
   `Random(281)` (proxy baseline 15/3/2) and judge the known runs (Crane's interior block,
   Ishmael's harpooneer rumination). The provisional table flags 21 sentences in those chapters,
   all precise on a skim, and **no runs in any of the nine texts**. Decide whether to retune
   `abstractBelow` or `minimumRatedWords` using bench/public only, and record the decision.
5. Docs still owed: STYLE-REPORTS rows 63 / 420–423 and a decision row replacing the 2026-10-16
   `missingTable` decision; the STYLE-NORMS diction section ("Bundled concreteness table (#256)"
   anchor, already linked from STYLE-DATA-LICENSES); a CHANGELOG `Unreleased` section; and the
   WASM size delta (baseline `writemorebetter_wasm.wasm` before bundling: 5,640,672 bytes, release
   build without wasm-bindgen).
6. Gates: fmt, clippy (`-p writemorebetter-style --all-targets --no-deps`), `cargo test -p
   writemorebetter-style`, every bench/style gate in ci.yml, the unittest discover command, WASM
   build + `node wasm/test.mjs`. Then the PR "Bundle writemorebetter's own concreteness table for
   diction (#256)" with "Refs #256, #250, #241" and the acceptance tables.

## Known problems

- Haiku sometimes returns tool-call markup or drops a word, so it needs retries.
- The OLS calibration compresses the scale: *spoon* gets 4.5 and *theory* 1.8 on the Opus-only
  table. That is MAE-optimal, but it is why few sentences reach a mean below 2.5.
- The coverage denominator includes particles and adverbs that diction's lookup accepts; it is
  diction's own definition, not a POS-only one.
