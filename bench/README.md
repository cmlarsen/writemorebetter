# bench

Compares grammar checkers on a fiction corpus and scores recall against hand-verified errors.

## Corpus

`corpus/` holds the texts and `corpus/truth.json` (49 verified errors: `{file: [{line, text, kind}]}`, where
`kind` is `spell`, `repeat`, `grammar`, `ngram`, or `punct`). The current corpus is unpublished fiction and is
**git-ignored** — it is not part of the public repo. Each paragraph is one line; tools report by line number.

## Producing runs

Each tool writes to `runs/` (git-ignored) as `<tool>-<name>.<ext>`:

| Tool | Command | Output |
|---|---|---|
| LanguageTool (public API) | `python3 tools/lt_api.py corpus/draft1.txt runs/lt-draft1.json` | `lt-*.json` |
| Harper | `harper-cli lint --format json --quiet corpus/draft1.txt > runs/harper-draft1.json` | `harper-*.json` |
| nlprule 0.6.4 | `SHOW=1 tools/probe/target/release/probe <bins-dir> corpus/draft1.txt > runs/nlp-draft1.log` | `nlp-*.log` |
| writemorebetter | `SHOW=1 tools/wmb-probe/target/release/wmb-probe ../storage corpus/draft1.txt > runs/wmb-draft1.log` | `wmb-*.log` |
| nlprule fork | `python3 tools/fork_driver.py <check_server> <en_tokenizer.bin> <en_rules.bin> corpus/draft1.txt runs/fork-draft1.json` | `fork-*.json` |

`lt_api.py` sends text to api.languagetool.org — only use it on text that may leave the machine.

- **Harper:** `cargo install --locked --git https://github.com/Automattic/harper harper-cli`.
- **nlprule 0.6.4 probe:** `cargo build --release` in `tools/probe`. The English binaries come from
  `gh release download 0.6.4 -R bminixhofer/nlprule -p 'en_*'`; gunzip them into `<bins-dir>`. The probe also
  prints load time and RSS.
- **writemorebetter probe:** `cargo build --release` in `tools/wmb-probe`; it builds this repo's engine. The
  binaries come from `cargo run --release --all-features --bin compile -- --build-dir data/en --tokenizer-out
  storage/en_tokenizer.bin --rules-out storage/en_rules.bin`, run from the repo root with upstream's build dir
  (https://f000.backblazeb2.com/file/nlprule/en.zip) unzipped into `data/`. The speller uses `en_US`; set
  `SPELL=en_GB` to change the variant or `SPELL=` to turn it off.
- **nlprule fork:** clone `alexvinasdev/nlprule`, run `cargo build --release -p nlprule --features
  "compile,bin,zh,ja"` (it fails without `zh,ja`), and generate an English build dir with its
  `build/make_build_dirs.py --lt_dir <LanguageTool-6.6> --out_root data --langs en` (needs Java 11+, `lxml`,
  `wordfreq`). Copy `chunker.json` from the upstream build dir (https://f000.backblazeb2.com/file/nlprule/en.zip)
  into it, then run its `compile` binary.

## Scoring

```bash
python3 tools/recall.py               # recall per tool and error kind, plus each tool's misses
python3 tools/compare.py draft1 draft2  # per-line side-by-side of every tool's non-whitespace flags
python3 tools/weir_coverage.py grammar.xml style.xml   # how much of LT's English rule set Harper's Weir can express
```

## Style reports

The separate [`style/`](style/README.md) harness measures analysis against pinned public annotations,
provides injected-error scoring, and supports owner-only aggregate calibration. Its CI commands and
baseline scores are documented there; it does not use the private grammar-recall corpus.
