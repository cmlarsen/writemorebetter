# public stand-in corpus

A small, self-contained fiction corpus with **deliberately planted errors**, so that recall can be
measured without touching the private corpus in `bench/corpus/`. It exists to be public, stable and
safe to ship in the repo.

## Contents

| File | What it is |
|---|---|
| `story1.txt` … `story5.txt` | Five short contemporary stories (first- or close-third-person, with dialogue). |
| `truth.json` | `{file stem: [{"line", "text", "kind"}, …]}` — the planted errors. |
| `validate.py` | Checks the corpus and the truth file against the rules below. |

All prose here is **invented for this repository**; nothing is copied from published fiction, song
lyrics, or real people. The text is dedicated to the public domain under **CC0 1.0**
(<https://creativecommons.org/publicdomain/zero/1.0/>).

## Conventions

* One paragraph per line, `\n`-terminated, no blank lines. Line numbers are **1-based** and are the
  only way errors are located.
* Typographic punctuation throughout: curly quotes `“ ” ‘ ’`, em dashes with no surrounding spaces
  (`the door—then`), en dashes in number ranges (`10–12`), and the ellipsis character `…`.
* U+2028 LINE SEPARATOR appears inside a few paragraphs. It is **not** a paragraph break: split the
  text on `\n` only (Python `.split("\n")`, not `.splitlines()`), or U+2028 will be treated as a line
  break and the line numbers will not match `truth.json`.
* 60 planted errors: **spell 30, repeat 8, grammar 14, ngram 4, punct 4**. At most one per line, none
  in the first three lines of a file.

## Scoring

Run any checker over `bench/public/*.txt` and compare its flags to `truth.json`. An error is *caught*
when the checker flags the same line and its flagged span overlaps the truth entry's `text` (the
substring-either-direction rule used by `bench/tools/recall.py`).

Write runs under any directory (named `<tool>-story1.log` etc., as in `bench/README.md`) and point the
scorer at it:

```bash
mkdir -p /tmp/public-runs
for n in story1 story2 story3 story4 story5; do
  SHOW=1 bench/tools/wmb-probe/target/release/wmb-probe storage bench/public/$n.txt > /tmp/public-runs/wmb-$n.log
done
python3 bench/tools/recall.py --truth bench/public/truth.json --runs /tmp/public-runs
```

Scores on 2026-10-11 (LanguageTool via its public API, free tier):

| Tool | Spell (30) | Repeat (8) | Grammar (14) | N-gram (4) | Punct (4) | Total (60) |
|---|---|---|---|---|---|---|
| LanguageTool 6.9 (API) | 30 | 8 | 12 | 2 | 4 | 56 |
| writemorebetter (M0, en_US speller) | 30 | 1 | 8 | 0 | 0 | 39 |
| nlprule 0.6.4 | 0 | 1 | 8 | 0 | 0 | 9 |

The ranking matches the private corpus (48 / 33 / 5 of 49).

CI runs `python3 bench/tools/public_recall.py --min <floor>` and fails when the engine's total drops below the
floor. Raise the floor in `.github/workflows/ci.yml` whenever a change raises recall.

The corpus is a stand-in, not a replacement for the private fiction corpus; treat its absolute
recall numbers as indicative rather than final.

## Validating the corpus itself

```bash
python3 bench/public/validate.py
```

It asserts every `truth.json` span occurs on its stated line, prints per-file word and U+2028 counts,
checks the per-kind totals against the expected 30/8/14/4/4, and rejects any straight `"` or `'`.

`moby150k.txt` is a public-domain Moby-Dick excerpt used only for performance and
suggestion identity checks (not the planted-error recall score). It contains 149,091
bytes from the supplied Project Gutenberg excerpt after removing the Gutenberg header
and start marker, with original CRLF line endings retained; no footer occurs in this
truncated input. See `docs/PERF.md` for the fixed measurement method.
