# AGENTS.md

Guidance for Claude Code and other agents working in this repo.

## Project

writemorebetter is an embeddable Rust grammar and spelling checker that runs LanguageTool's open English rule set
without a JVM — targeting WASM (browser/Electron), native, and Swift (iOS/macOS). It builds on nlprule. The
first consumer is Subplot, a writing app for fiction authors, so fiction prose (dialogue, curly quotes,
em dashes, deliberate fragments) is the primary input.

Read `docs/FINDINGS.md` before making architectural decisions; it records what was measured and why this
approach was chosen over Harper or the alexvinasdev nlprule fork.

Prose style reports (sentence starts, echoes, overused words, clichés, dialogue tags, …) are planned as a
separate `style/` crate. Read `docs/STYLE-REPORTS.md` before working on them or on `analyze()`. It records
the structure, the baseline data sources, and the decisions behind them. Tracking issue: #241.

## Hard rules

- **Never use the name "LanguageTool"** for the project, crates, packages, or binaries — it is a trademark of
  LanguageTooler GmbH. Credit LanguageTool as the source of the rules; do not imply affiliation.
- **Keep the license split.** Engine code is MIT/Apache-2.0. Anything compiled or copied from LanguageTool's
  resources (rules XML, dictionaries, tagger/speller data) is LGPL-2.1 and stays in separately loaded data
  binaries, never inlined into engine source.
- **The corpus in `bench/corpus/` is private, unpublished fiction.** It is git-ignored; never commit it, quote
  it at length in public docs, or send it to third-party services. The one approved exception is
  LanguageTool's public API via `bench/tools/lt_api.py`.
- **Text handling must be UTF-8 safe.** Never slice strings at byte offsets without checking char boundaries.
  Every builtin rule needs tests with curly quotes, curly apostrophes, em dashes, ellipses, and U+2028.

## Gates

- LanguageTool's embedded rule examples (`<example correction=…>`) are the conformance suite; report the
  pass rate when touching the compiler or matcher.
- `python3 bench/tools/recall.py` is the regression check against verified fiction errors. Recall must not
  drop; current baseline is in `docs/FINDINGS.md`.
- Report memory (peak RSS) and check time for the English binaries when touching the tagger, speller, or
  rule loading. Baseline: nlprule 0.6.4 English, ~145 MB loaded.

## Rule fixes

- `bench/tools/gates.sh` runs every gate above (conformance, public and private recall with the
  private flag diff since its last run, golden files) and prints one summary. Run it after each
  data rebuild.
- `bench/tools/parity.py "text"` shows which of writemorebetter, LanguageTool 6.6 (local) and
  LanguageTool's API flag each span. The API leg sends the text out, so use it only on sentences
  you wrote; `--no-api` otherwise.
- `bench/tools/corpus_scan.py` runs the probe over a local published-fiction corpus and ranks,
  samples and diffs the flags; `run --only RULE --grep REGEX` checks one rule in minutes.
- Before writing a fix, check whether LanguageTool fixed it after 6.6 (released 2025-03-27): list
  the commits that touch the file, e.g.
  `gh api 'repos/languagetool-org/languagetool/commits?path=languagetool-language-modules/en/src/main/resources/org/languagetool/rules/en/grammar.xml&since=2025-03-27'`,
  or search them for the rule id. Backport into `build/lt-rules/` and list the edit in that
  directory's README.
- `target/release/test --list-failing` names failing conformance rules; `RUST_LOG=warn` prints
  the failing examples.

## Scope

English first. Other languages come later as data plus per-language builtins behind a per-language
interface — keep English specifics out of the generic engine, but do not build other languages' special
cases yet. N-gram confusion rules are out of scope for the on-device engine.

The `style` crate diagnoses and never generates replacement prose; the drafting report's mechanical edits
(deletions, whitespace, table punctuation, line breaks, a first-letter case change) are the one exception. It
holds no LGPL data, doesn't duplicate LanguageTool rules that `nlprule` ports, and never ships text from any
corpus, only aggregate numbers (see `docs/STYLE-REPORTS.md`).
