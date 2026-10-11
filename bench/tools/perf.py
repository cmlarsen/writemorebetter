#!/usr/bin/env python3
"""Reproducible public-text benchmarks and suggestion identity checks."""
import argparse
import difflib
import json
import os
from pathlib import Path
import statistics
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PROBE = ROOT / 'bench/tools/wmb-probe/target/release/wmb-probe'
TEXTS = ['moby150k'] + [f'story{i}' for i in range(1, 6)]


def build():
    subprocess.run(['cargo', 'build', '--release', '--manifest-path',
                    str(ROOT / 'bench/tools/wmb-probe/Cargo.toml')], check=True, cwd=ROOT)


def run(probe, storage, text, profile, show=False, prefix=()):
    env = dict(os.environ, PROFILE=profile, SPELL='en_US', METRICS='1')
    env.pop('SHOW', None)
    if show:
        env['SHOW'] = '1'
    result = subprocess.run([*prefix, str(probe), str(storage), str(text)],
                            env=env, text=True, capture_output=True, check=True)
    metrics = next(line.removeprefix('METRICS ') for line in result.stderr.splitlines()
                   if line.startswith('METRICS '))
    suggestions = '\n'.join(line for line in result.stdout.splitlines() if ' | ' in line)
    return json.loads(metrics), suggestions + ('\n' if suggestions else '')


def table(rows):
    lines = ['| Rules | Text | Check (s) | Peak RSS (MiB) | Loaded RSS (MiB) | Load (s) |',
             '|---|---|---:|---:|---:|---:|']
    for row in rows:
        lines.append(f"| {row['rules']} | {row['text']} | {row['check_seconds']:.4f} | "
                     f"{row['peak_rss_mib']:.2f} | {row['loaded_rss_mib']:.2f} | "
                     f"{row['load_seconds']:.4f} |")
    return '\n'.join(lines) + '\n'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--golden', choices=['check', 'write'])
    parser.add_argument('--rules', nargs='+', choices=['52', '66'], default=['52', '66'])
    parser.add_argument('--texts', nargs='+', choices=TEXTS, default=TEXTS)
    parser.add_argument('--storage', type=Path, default=ROOT / 'storage')
    parser.add_argument('--probe', type=Path, default=PROBE)
    parser.add_argument('--skip-build', action='store_true')
    parser.add_argument('--runs', type=int, default=5)
    parser.add_argument('--json', type=Path, default=ROOT / 'storage/perf.json')
    parser.add_argument('--markdown', type=Path, default=ROOT / 'storage/perf.md')
    args = parser.parse_args()
    if args.runs < 1:
        parser.error('--runs must be positive')
    if not args.skip_build:
        build()
    rows, failed = [], False
    for rules in args.rules:
        for name in args.texts:
            text = ROOT / 'bench/public' / f'{name}.txt'
            if args.golden:
                for profile in ['default', 'fiction']:
                    _, output = run(args.probe, args.storage / rules, text, profile, show=True)
                    golden = ROOT / 'bench/golden' / rules / profile / f'{name}.txt'
                    if args.golden == 'write':
                        golden.parent.mkdir(parents=True, exist_ok=True)
                        golden.write_text(output)
                    elif not golden.exists():
                        print(f'Missing golden: {golden}', file=sys.stderr)
                        failed = True
                    elif golden.read_text() != output:
                        print(''.join(difflib.unified_diff(golden.read_text().splitlines(True),
                              output.splitlines(True), fromfile=str(golden), tofile='current')))
                        failed = True
                continue
            samples = [run(args.probe, args.storage / rules, text, 'fiction')[0]
                       for _ in range(args.runs)]
            row = {key: statistics.median(sample[key] for sample in samples)
                   for key in samples[0]}
            rows.append(dict(row, rules=rules, text=name, samples=samples))
    if args.golden:
        if failed:
            return 1
        print(f'Golden {args.golden}: {len(args.rules) * len(args.texts) * 2} files passed')
    else:
        args.json.parent.mkdir(parents=True, exist_ok=True)
        args.markdown.parent.mkdir(parents=True, exist_ok=True)
        args.json.write_text(json.dumps({'profile': 'fiction', 'spell': 'en_US',
                             'runs': args.runs, 'rows': rows}, indent=2) + '\n')
        args.markdown.write_text(table(rows))
        print(table(rows), end='')
    return 0


if __name__ == '__main__':
    sys.exit(main())
