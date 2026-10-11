"""Score each checker against the hand-verified errors in bench/corpus/truth.json.

Usage: python3 bench/tools/recall.py [--runs bench/runs] [--truth bench/corpus/truth.json]
"""
import argparse
import collections
import json
from pathlib import Path

from flags import NOISE, TOOLS, available

KEEP_DESPITE_NOISE = {"COMMA_PARENTHESIS_WHITESPACE", "CommaFixes"}
SPAN_TOO_WIDE = {"LongSentences"}


def caught(flags, line, needle):
    n = needle.lower()
    for fl, rule, text, _ in flags:
        if fl != line or rule in SPAN_TOO_WIDE:
            continue
        if NOISE.search(rule) and rule not in KEEP_DESPITE_NOISE:
            continue
        t = text.strip().lower()
        if n in text.lower() or (t and t in n):
            return True
    return False


def main():
    root = Path(__file__).resolve().parents[1]
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", type=Path, default=root / "runs")
    ap.add_argument("--truth", type=Path, default=root / "corpus" / "truth.json")
    args = ap.parse_args()

    truth = json.loads(args.truth.read_text())
    kinds = sorted({e["kind"] for items in truth.values() for e in items})
    total = collections.Counter(e["kind"] for items in truth.values() for e in items)
    hit = collections.Counter()
    missed = collections.defaultdict(list)
    seen = set()

    for name, items in truth.items():
        for label, fn in available(args.runs, name):
            seen.add(label)
            flags = list(fn(args.runs, name))
            for e in items:
                if caught(flags, e["line"], e["text"]):
                    hit[(label, e["kind"])] += 1
                else:
                    missed[label].append(f"{name}:{e['line']} {e['text']}")

    print("tool".ljust(16) + "".join(k.rjust(10) for k in kinds) + "total".rjust(10))
    print("(n)".ljust(16) + "".join(str(total[k]).rjust(10) for k in kinds) + str(sum(total.values())).rjust(10))
    for label in TOOLS:
        if label not in seen:
            continue
        row = [hit[(label, k)] for k in kinds]
        print(label.ljust(16) + "".join(str(v).rjust(10) for v in row) + str(sum(row)).rjust(10))
    for label in TOOLS:
        if label in seen:
            print(f"\n{label} missed ({len(missed[label])}): {missed[label]}")


if __name__ == "__main__":
    main()
