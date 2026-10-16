"""Run wmb-probe over the local published-fiction units and compare runs.

The units are extracted from EPUBs that must never leave the machine, so every file this
tool writes goes under the fiction-baseline `out/` directory (git-ignored there), and it
prints only rule counts and short flagged spans.

Usage:
  python3 bench/tools/corpus_scan.py run NAME [--bins storage] [--profile fiction]
                                     [--only PREFIX,...] [--grep REGEX] [--jobs N]
  python3 bench/tools/corpus_scan.py diff A B [--limit 40]
  python3 bench/tools/corpus_scan.py rank NAME [--top 60]
  python3 bench/tools/corpus_scan.py sample NAME RULE_REGEX [--k 12] [--width 40]

`run --grep` checks only the lines (paragraphs) that match REGEX, with one line of context on
each side, keeps the flags on matching lines and maps their line numbers back, so a targeted
rule change can be verified in minutes instead of hours. Compare runs made with the same
--only and --grep: a subset of rules resolves overlaps differently (a rule left out can no
longer win a span), so a filtered run is not comparable with a full one. `--only` keeps
only rules whose id, or id without its category, starts with one of the prefixes (the
probe's ONLY variable), e.g. `--only EACH_OF_NNS_VBP,TYPOS/HE_THE,BUILTIN/EN_A_VS_AN`.
Set FICTION_BASELINE_OUT to use another out/ directory.
"""
import argparse
import collections
import json
import os
import random
import re
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = Path(os.environ.get("FICTION_BASELINE_OUT", Path.home() / "CalebApps/fiction-baseline/out"))
UNITS = OUT / "units"
FLAGS = OUT / "wmb-flags"
PROBE = ROOT / "bench/tools/wmb-probe/target/release/wmb-probe"
MAX_WIDTH = 60


def meta():
    path = OUT / "units.jsonl"
    if not path.exists():
        return {}
    return {row["unit_id"]: row for row in map(json.loads, path.open())}


def parse_log(path):
    flags = []
    for raw in path.read_text(errors="replace").splitlines():
        parts = raw.split(" | ")
        if len(parts) >= 3 and parts[0].isdigit():
            match = re.match(r'"(.*?)" ->', parts[2], re.S)
            flags.append((int(parts[0]), parts[1].strip(), match.group(1) if match else ""))
    return flags


def load(name):
    directory = FLAGS / name
    if not directory.is_dir():
        sys.exit(f"no run named {name!r} in {FLAGS}")
    return {path.stem: parse_log(path) for path in sorted(directory.glob("*.log"))}


def group(rule):
    return "/".join(rule.split("/")[:2])


def run_unit(unit, args, out_dir, scratch, pattern, env):
    lines = matched = None
    source = unit
    if pattern:
        text = unit.read_text(errors="replace").split("\n")
        matched = {i for i, line in enumerate(text, 1) if pattern.search(line)}
        if not matched:
            (out_dir / f"{unit.stem}.log").write_text("")
            return 0
        lines = sorted({j for i in matched for j in (i - 1, i, i + 1) if 1 <= j <= len(text)})
        source = scratch / unit.name
        source.write_text("\n".join(text[i - 1] for i in lines) + "\n")
    result = subprocess.run([str(args.probe), str(args.bins), str(source)], env=env,
                            capture_output=True, text=True, errors="replace")
    if source != unit:
        source.unlink()
    out = []
    for raw in result.stdout.splitlines():
        head, sep, rest = raw.partition(" | ")
        if sep and head.isdigit():
            n = lines[int(head) - 1] if lines else int(head)
            if matched is None or n in matched:
                out.append(f"{n} | {rest}")
    (out_dir / f"{unit.stem}.log").write_text("\n".join(out) + ("\n" if out else ""))
    return len(out)


def cmd_run(args):
    out_dir = FLAGS / args.name
    out_dir.mkdir(parents=True, exist_ok=True)
    scratch = OUT / "scan-tmp" / args.name
    scratch.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, PROFILE=args.profile, SHOW="1")
    if args.only:
        env["ONLY"] = args.only
    pattern = re.compile(args.grep) if args.grep else None
    units = sorted(UNITS.glob("*.txt"))
    with ThreadPoolExecutor(args.jobs) as pool:
        counts = list(pool.map(lambda u: run_unit(u, args, out_dir, scratch, pattern, env), units))
    scratch.rmdir()
    print(f"{args.name}: {len(units)} units, {sum(counts):,} flags -> {out_dir}")


def cmd_diff(args):
    a, b = load(args.a), load(args.b)
    units = sorted(set(a) & set(b))
    if args.restrict:
        pattern = re.compile(args.restrict)
    ca, cb = collections.Counter(), collections.Counter()
    added, removed = [], []
    for unit in units:
        fa = collections.Counter(f for f in a[unit] if not args.restrict or pattern.search(f[1]))
        fb = collections.Counter(f for f in b[unit] if not args.restrict or pattern.search(f[1]))
        ca.update(group(f[1]) for f in fa.elements())
        cb.update(group(f[1]) for f in fb.elements())
        added += [(unit, f) for f in (fb - fa).elements()]
        removed += [(unit, f) for f in (fa - fb).elements()]
    print(f"{len(units)} units in both; flags {sum(ca.values()):,} -> {sum(cb.values()):,}")
    print("\nrule groups whose counts changed:")
    for rule in sorted(set(ca) | set(cb), key=lambda r: (-abs(cb[r] - ca[r]), r)):
        if ca[rule] != cb[rule]:
            print(f"{ca[rule]:8} -> {cb[rule]:8}  {rule}")
    for label, items in (("added", added), ("removed", removed)):
        print(f"\n{label} ({len(items)}):")
        for unit, (line, rule, text) in items[: args.limit]:
            print(f"  {unit}:{line}  {rule}  {text[:MAX_WIDTH]!r}")


def cmd_rank(args):
    runs, info = load(args.name), meta()
    words = sum(info.get(u, {}).get("words", 0) for u in runs) or 1
    counts, authors = collections.Counter(), collections.defaultdict(set)
    for unit, flags in runs.items():
        for _, rule, _ in flags:
            counts[group(rule)] += 1
            authors[group(rule)].add(info.get(unit, {}).get("author_key", unit))
    print(f"{len(runs)} units, {words:,} words, {sum(counts.values()):,} flags")
    for rule, n in counts.most_common(args.top):
        print(f"{n:8} {n / words * 1e6:9.1f}/M  authors {len(authors[rule]):3}  {rule}")


def cmd_sample(args):
    runs, pattern = load(args.name), re.compile(args.rule)
    hits = [(u, line, text) for u, flags in runs.items() for line, rule, text in flags if pattern.search(rule)]
    spans = collections.Counter(re.sub(r"\s+", " ", t).lower()[:MAX_WIDTH] for _, _, t in hits)
    per_unit = collections.Counter(u for u, _, _ in hits)
    print(f"{len(hits)} hits in {len(per_unit)} units; top units {per_unit.most_common(5)}")
    print("top flagged spans:")
    for span, n in spans.most_common(args.k):
        print(f"{n:7}  {span!r}")
    width = min(args.width, MAX_WIDTH)
    random.seed(args.seed)
    print("random contexts:")
    cache = {}
    for unit, line, text in random.sample(hits, min(args.k, len(hits))):
        lines = cache.setdefault(unit, (UNITS / f"{unit}.txt").read_text(errors="replace").split("\n"))
        para = lines[line - 1] if line - 1 < len(lines) else ""
        needle = text.encode().decode("unicode_escape", errors="ignore") if "\\" in text else text
        i = para.find(needle) if needle else -1
        context = para[max(0, i - width): i + len(needle) + width] if i >= 0 else para[: 2 * width]
        print(f"  {unit}:{line}  {context!r}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("run")
    p.add_argument("name")
    p.add_argument("--bins", type=Path, default=ROOT / "storage")
    p.add_argument("--probe", type=Path, default=PROBE)
    p.add_argument("--profile", default="fiction")
    p.add_argument("--only")
    p.add_argument("--grep")
    p.add_argument("--jobs", type=int, default=max(1, (os.cpu_count() or 2) - 2))
    p = sub.add_parser("diff")
    p.add_argument("a")
    p.add_argument("b")
    p.add_argument("--restrict", help="only compare rules matching this regex")
    p.add_argument("--limit", type=int, default=40)
    p = sub.add_parser("rank")
    p.add_argument("name")
    p.add_argument("--top", type=int, default=60)
    p = sub.add_parser("sample")
    p.add_argument("name")
    p.add_argument("rule")
    p.add_argument("--k", type=int, default=12)
    p.add_argument("--width", type=int, default=40)
    p.add_argument("--seed", type=int, default=1)
    args = ap.parse_args()
    {"run": cmd_run, "diff": cmd_diff, "rank": cmd_rank, "sample": cmd_sample}[args.cmd](args)


if __name__ == "__main__":
    main()
