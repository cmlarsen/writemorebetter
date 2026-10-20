"""Run the in-tree engine over bench/public and fail if recall drops below a floor.

Usage: python3 bench/tools/public_recall.py --min 39 [--profile fiction] [--enable PLAIN_ENGLISH,STYLE/PASSIVE_VOICE] [--probe bench/tools/wmb-probe/target/release/wmb-probe] [--bins storage]
"""
import argparse
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from flags import writemorebetter
from recall import caught


def main():
    root = Path(__file__).resolve().parents[2]
    ap = argparse.ArgumentParser()
    ap.add_argument("--min", type=int, required=True)
    ap.add_argument("--profile", default="default", help="rule profile name passed to the probe")
    ap.add_argument("--enable", default="", help="comma-separated rule selectors to enable after the profile")
    ap.add_argument("--probe", type=Path, default=root / "bench/tools/wmb-probe/target/release/wmb-probe")
    ap.add_argument("--bins", type=Path, default=root / "storage")
    args = ap.parse_args()

    public = root / "bench" / "public"
    truth = json.loads((public / "truth.json").read_text())
    hits, total, missed = {}, {}, []
    n_flags = 0

    with tempfile.TemporaryDirectory() as runs:
        runs = Path(runs)
        for name in truth:
            out = subprocess.run(
                [str(args.probe), str(args.bins), str(public / f"{name}.txt")],
                env={**os.environ, "SHOW": "1", "PROFILE": args.profile, "ENABLE": args.enable},
                check=True,
                capture_output=True,
                text=True,
            ).stdout
            (runs / f"wmb-{name}.log").write_text(out)

        for name, items in truth.items():
            flags = list(writemorebetter(runs, name))
            n_flags += len(flags)
            for e in items:
                total[e["kind"]] = total.get(e["kind"], 0) + 1
                if caught(flags, e["line"], e["text"]):
                    hits[e["kind"]] = hits.get(e["kind"], 0) + 1
                else:
                    missed.append(f"{name}:{e['line']} {e['text']}")

    for kind in sorted(total):
        print(f"{kind:>8} {hits.get(kind, 0):>3}/{total[kind]}")
    got = sum(hits.values())
    print(f"{'total':>8} {got:>3}/{sum(total.values())} (floor {args.min})")
    print(f"profile {args.profile}: {n_flags} flags across {len(truth)} stories")
    print("missed:", ", ".join(missed))
    if got < args.min:
        print(f"recall dropped below the floor: {got} < {args.min}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
