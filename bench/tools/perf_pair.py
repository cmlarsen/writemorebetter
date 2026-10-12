#!/usr/bin/env python3
"""Interleave five before/after processes per public text under comparable local load."""
import argparse
import json
import os
from pathlib import Path
import statistics
from perf import ROOT, PROBE, TEXTS, run


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--before', type=Path, required=True,
                        help='snapshot containing wmb-probe and the 66 data directory')
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--texts', nargs='+', choices=TEXTS, default=TEXTS)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    before = args.before.resolve()
    rows = []
    load_start = os.getloadavg()
    for rules in ['66']:
        for name in args.texts:
            text = ROOT / 'bench/public' / f'{name}.txt'
            pairs = []
            for _ in range(5):
                a, _ = run(before / 'wmb-probe', before / rules, text, 'fiction')
                b, _ = run(PROBE, ROOT / 'storage' / rules, text, 'fiction')
                pairs.append(dict(before=a, after=b))
            medians = {side: {key: statistics.median(pair[side][key] for pair in pairs)
                             for key in pairs[0][side]} for side in ['before', 'after']}
            rows.append(dict(rules=rules, text=name, pairs=pairs, **medians))
    lines = ['Local timing under load; five interleaved A/B pairs per case.', '',
             '| Rules | Text | Check before/after (s) | Check Δ | Peak before/after (MiB) | Peak Δ (MiB) |',
             '|---|---|---|---:|---|---:|']
    for row in rows:
        a, b = row['before'], row['after']
        lines.append(f"| {row['rules']} | {row['text']} | {a['check_seconds']:.4f} / "
                     f"{b['check_seconds']:.4f} | {100 * (b['check_seconds']/a['check_seconds']-1):+.2f}% | "
                     f"{a['peak_rss_mib']:.2f} / {b['peak_rss_mib']:.2f} | "
                     f"{b['peak_rss_mib']-a['peak_rss_mib']:+.2f} |")
    result = dict(method='five interleaved A/B pairs; timing under load is informational',
                  load_start=load_start, load_end=os.getloadavg(), rows=rows)
    (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    report = '\n'.join(lines) + '\n'
    (args.output / 'summary.md').write_text(report)
    print(report)


if __name__ == '__main__':
    main()
