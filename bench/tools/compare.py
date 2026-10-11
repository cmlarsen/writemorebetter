"""Per-line side-by-side of every checker's non-whitespace flags for one or more corpus files.

Usage: python3 bench/tools/compare.py draft1 draft2 [--runs bench/runs]
Prefixes: L = LanguageTool API, F = nlprule fork, N = nlprule 0.6.4, H = Harper.
"""
import argparse
import collections
from pathlib import Path

from flags import NOISE, available

PREFIX = {"LT 6.9 (API)": "L", "nlprule fork": "F", "nlprule 0.6.4": "N", "Harper": "H"}


def main():
    root = Path(__file__).resolve().parents[1]
    ap = argparse.ArgumentParser()
    ap.add_argument("names", nargs="+")
    ap.add_argument("--runs", type=Path, default=root / "runs")
    args = ap.parse_args()

    for name in args.names:
        rows = collections.defaultdict(list)
        totals = {}
        for label, fn in available(args.runs, name):
            n = k = 0
            for line, rule, text, repl in fn(args.runs, name):
                n += 1
                if NOISE.search(rule):
                    continue
                k += 1
                rows[line].append(f"{PREFIX[label]}:{rule}:{text!r}->{repl}")
            totals[PREFIX[label]] = {"all": n, "non_ws": k}
        print(f"##### {name} {totals}")
        for line in sorted(rows):
            print(line, " || ".join(rows[line]))


if __name__ == "__main__":
    main()
