# Baseline: nlprule 0.6.4, English

The numbers later milestones are measured against. Recorded 2026-10-11 on an Apple M5 Pro, rustc 1.97.1,
release builds. Update this file only when a milestone deliberately moves the baseline (and say which).

## Conformance

LanguageTool's embedded rule examples, run with the `test` and `test_disambiguation` binaries on binaries
compiled from upstream nlprule's English build dir (LT 5.2, https://f000.backblazeb2.com/file/nlprule/en.zip).
The compiled rule set is identical to the 0.6.4 release; the release binaries themselves no longer load since
the rules format gained the speller (M0).

| Rule set | Rules in LT's XML | Compiled | Passing their examples | Pass rate (of all rules) |
|---|---|---|---|---|
| Grammar (`grammar.xml`) | 4,366 | 3,749 | 3,749 | 85.9% |
| Disambiguation (`disambiguation.xml`) | 855 | 843 | 843 | 98.6% |

"Rules in LT's XML" counts `<rule>` elements. Every rule that compiles passes, so the compiler, not the matcher,
limits the pass rate. The 531 grammar-rule compile failures, by cause:

| Cause | Rules |
|---|---|
| `postag` / `postag_regex` / `postag_replace` / text in `<match>` | 300 |
| Rules with no suggestion | 72 |
| Examples with `type` (`triggers_error`) | 67 |
| Rules with `<filter>` | 34 |
| `include_skipped` in `<match>` | 14 |
| XML structure the parser rejects (`tags`, `type`, missing `text`, …) | 39 |
| Regexes Oniguruma rejects | 4 |
| Control flow in parallel tokens | 1 |

## Speed and memory

nlprule 0.6.4 release binaries, measured with `bench/tools/probe` (load and check time) and `/usr/bin/time -l`
(peak RSS). The chapter is a 5,027-word chapter from the private corpus; Moby-Dick is the first 150,000 bytes of
Project Gutenberg's `pg2701.txt`.

| Input | Tokenizer load | Rules load | Check | RSS after load | Peak RSS | Suggestions |
|---|---|---|---|---|---|---|
| 5k-word chapter | 0.11 s | 0.02 s | 0.68 s | 141 MiB | 155 MiB | 308 |
| Moby-Dick, 150 KB | 0.10 s | 0.01 s | 2.72 s | 142 MiB | 160 MiB | 37 |

English data on disk: 11.6 MB tokenizer, 7.6 MB rules (9.1 MB with the speller since M0).

**Measure on an idle machine.** With three cargo builds running alongside, the same Moby-Dick check took 5–17 s.
Compare before/after numbers only from runs on the same machine under the same load.

## Recall

`python3 bench/tools/recall.py` on the private corpus (49 verified errors): 5 for nlprule 0.6.4. M0 raised the
in-tree engine to 33 (spelling 28/28); see `docs/FINDINGS.md`.
