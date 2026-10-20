# Concreteness ratings (#256)

Inputs and raw ratings for `style/data/norms/concreteness.tsv`, diction's bundled table. Results,
decisions and the downstream review are in
[docs/STYLE-NORMS.md](../../../docs/STYLE-NORMS.md#bundled-concreteness-table-256); licences and
provenance in [docs/STYLE-DATA-LICENSES.md](../../../docs/STYLE-DATA-LICENSES.md#concreteness-256).

| File | What it is |
|---|---|
| `prompt.txt` | Rater instructions, `[system]` and `[user]` sections. Frozen before scoring (sha256 `0317ef4b…`). |
| `words.tsv` | 14,364 rated lemmas plus 195 British spellings that copy their American value (sha256 `d48a0c87…`); `scripts/build-concreteness-norms.py words` regenerates it. |
| `words-supplement.tsv` | 13 particles and adverbs diction rates but the fiction list drops (sha256 `a0232fd2…`). |
| `raw/<model>-s<sample>.jsonl` | Accepted ratings, one row per word. |
| `raw/<model>-s<sample>.responses.jsonl` | Every reply, rejected ones included, with the batch, attempt and start time. |
| `adjudication.tsv` | Hand edits: word, value before, value after, reason. The build checks `before`. |
| `qa-review.tsv` | Words for review: model spread > 1.5 or more than 1.5 from the Lancaster + WordNet ridge. |

`rate.py` and the build refuse a prompt or word list with another hash. Changing either means
re-rating everything.

## Rating

Run from the repository root, with an empty scratch directory as `--cwd` so the CLI picks up no
project context. Finished batches are skipped, so an interrupted run resumes.

```sh
python3 bench/style/concreteness/rate.py --supplement --jobs 8 --cwd <empty dir> \
  claude:claude-opus-5-5:{1,2,3} claude:claude-sonnet-5-5:{1,2,3} claude:claude-fable-5-1:{1,2,3}
```

Each call is `claude -p --model <id> --tools "" --system-prompt <[system] text>
--strict-mcp-config --setting-sources "" --no-session-persistence --output-format json <user
message>`, 50 words per batch in a shuffle seeded by `int(sha256("<model>:<sample>")[:8], 16)` (the
supplement adds 1). A reply must give every word exactly once with a rating `[1-5](\.\d)?`, one
`word<TAB>rating` per line, or the word and its rating on alternate lines throughout; otherwise
it is retried up to five times.

`claude-haiku-5-5` was rated (three samples) and is committed, but the build lists it in `EXCLUDED`:
its sample-to-sample Spearman is 0.933, below the 0.97 stability gate.

## Building and checking

```sh
python3 scripts/build-concreteness-norms.py build          # writes the table and qa-review.tsv
python3 scripts/build-concreteness-norms.py build --check  # fails unless both are reproducible
cargo build -p writemorebetter-style --release --example diction_coverage
python3 bench/style/concreteness.py --gate --probe target/release/examples/diction_coverage
```

The Glasgow Norms, Lancaster norms and WordNet are downloaded into the git-ignored
`bench/style/.cache` and pinned in `bench/style/sources.json`; none of them is committed. After an
intended change, `concreteness.py --write` records the new numbers in `concreteness-results.json`.
