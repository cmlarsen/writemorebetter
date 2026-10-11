#!/usr/bin/env python3
"""Validate the public stand-in corpus in bench/public/.

Checks performed (all must pass):
  * every truth.json entry's "text" occurs in that exact 1-based line of that file
  * the kind counts are spell=30, repeat=8, grammar=14, ngram=4, punct=4 (60 total)
  * each story is 1,500-3,000 words and the corpus totals at least 10,000 words
  * the corpus contains at least 10 U+2028 LINE SEPARATOR characters
  * no straight double quote (") or straight apostrophe (') appears in any .txt file
  * paragraphs are single lines (no blank lines except the trailing newline)

Line numbers are derived by splitting on "\n" only: U+2028 is *not* a line
break here, which is the whole point of having it inside paragraphs.

Usage: python3 bench/public/validate.py
"""
import json
import sys
from pathlib import Path

PUB = Path(__file__).resolve().parent
LS = "\u2028"
EXPECTED = {"spell": 30, "repeat": 8, "grammar": 14, "ngram": 4, "punct": 4}
MIN_WORDS, MAX_WORDS = 1500, 3000


def main() -> int:
    truth = json.loads((PUB / "truth.json").read_text(encoding="utf-8"))
    problems = []

    kind_counts = {k: 0 for k in EXPECTED}
    total_words = 0
    total_ls = 0
    located = 0

    for stem, entries in truth.items():
        path = PUB / f"{stem}.txt"
        text = path.read_text(encoding="utf-8")
        lines = text.split("\n")  # intentionally NOT splitlines(): U+2028 stays inside a line

        # paragraphs must be one line each, no blank lines inside the file
        body, trailing = lines[:-1], lines[-1]
        for i, line in enumerate(body, start=1):
            if not line.strip():
                problems.append(f"{stem}: blank line at {i}")
        if trailing != "":
            problems.append(f"{stem}: file should end with a single newline")

        words = sum(len(line.split()) for line in body)
        ls_count = text.count(LS)
        total_words += words
        total_ls += ls_count

        # truth entries must land on the exact line given
        for e in entries:
            ln = e["line"]
            if not (1 <= ln <= len(body)):
                problems.append(f"{stem}:{ln}: line number out of range")
                continue
            if e["text"] not in body[ln - 1]:
                problems.append(f"{stem}:{ln}: {e['text']!r} not found in line")
                continue
            kind_counts[e["kind"]] += 1
            located += 1

        # straight ASCII quotes must not appear anywhere
        for ch, name in (('"', "straight double quote"), ("'", "straight apostrophe")):
            if ch in text:
                problems.append(f"{stem}: contains a {name}")

        print(f"{stem}: {len(body)} lines, {words} words, U+2028={ls_count}")

    print()
    print("kind counts: " + " ".join(f"{k}={kind_counts[k]}" for k in EXPECTED)
          + f" (total {sum(kind_counts.values())})")
    print(f"truth spans located: {located}/{sum(len(v) for v in truth.values())}")
    print(f"total words: {total_words}")
    print(f"total U+2028: {total_ls}")

    if kind_counts != EXPECTED:
        problems.append(f"kind counts {kind_counts} != expected {EXPECTED}")
    if total_words < 10000:
        problems.append(f"total words {total_words} < 10000")
    if total_ls < 10:
        problems.append(f"U+2028 count {total_ls} < 10")
    for stem in truth:
        w = sum(len(l.split()) for l in (PUB / f"{stem}.txt").read_text(encoding="utf-8").split("\n")[:-1])
        if not (MIN_WORDS <= w <= MAX_WORDS):
            problems.append(f"{stem}: {w} words outside [{MIN_WORDS}, {MAX_WORDS}]")

    print()
    if problems:
        print("FAIL:")
        for p in problems:
            print("  -", p)
        return 1
    print("OK: all checks passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
