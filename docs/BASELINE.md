# Baseline: nlprule 0.6.4, English

The numbers later milestones are measured against. Recorded 2026-10-11 on an Apple M5 Pro, rustc 1.97.1,
release builds. Update this file only when a milestone deliberately moves the baseline (and say which).
As of release 0.7.0 the LT 5.2 rule set is no longer built; the 5.2 columns below are kept as history.

## Current (after M4F)

M4F compiles the rules that have no `<suggestion>` and the examples that carry a
`type` (`triggers_error`), so both compiler errors are gone. Measured on this
branch (2026-10-11, Apple M5 Pro, rustc 1.97.1, release builds); see
`docs/M4F-NOSUGG.md` for the full accounting:

| Rule set | LT 6.6 runnable | LT 6.6 passing | LT 5.2 runnable | LT 5.2 passing |
|---|---|---|---|---|
| Grammar (`grammar.xml` + `style.xml`) | 5,934 | 5,121 | 4,252 | 4,187 |
| Disambiguation (`disambiguation.xml`) | 1,015 | 979 | 843 | 843 |

Pass rate of all `<rule>` elements: 6.6 5,121 / 6,098 = 84.0%, 5.2 4,187 / 4,366
= 95.9%. Public recall: 55/60 with LT 6.6, 54/60 with LT 5.2. The CI floors in
`.github/workflows/ci.yml` (M4E) still hold; they could be raised to these
numbers, but that is another worker's file.

## After M4

M4 moves the engine to LanguageTool 6.6 and keeps 5.2 as a second CI job. Measured on current `main`
(2026-10-11, Apple M5 Pro, rustc 1.97.1, release builds), with the 6.6 build dir generated from the
pinned desktop release and the 5.2 build dir being the published Backblaze `en.zip`:

| Rule set | LT 6.6 runnable | LT 6.6 passing | LT 5.2 runnable | LT 5.2 passing |
|---|---|---|---|---|
| Grammar (`grammar.xml` + `style.xml`) | 5,718 | 4,998 | 4,055 | 4,052 |
| Disambiguation (`disambiguation.xml`) | 1,015 | 979 | 843 | 843 |

Public recall (`bench/tools/public_recall.py` on `bench/public`): 54/60 with LT 6.6, 53/60 with LT 5.2.
CI gates each version at its own floor (6.6: 4,998 / 5,718 grammar, 979 disambiguation, recall 54;
5.2: 4,052 / 4,055, 843, recall 53).

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
