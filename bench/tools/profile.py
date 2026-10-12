#!/usr/bin/env python3
"""Run wmb-profile repeatedly and print median tables for the profile breakdown."""
import argparse
import os
import statistics
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / 'bench/tools/wmb-profile'
BIN = TOOL / 'target/release/wmb-profile'
HEAP_BIN = TOOL / 'target/heap/release/wmb-profile'
TEXT = ROOT / 'bench/public/moby150k.txt'


def build(heap):
    cmd = ['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml')]
    env = os.environ.copy()
    if heap:
        cmd += ['--features', 'dhat-heap']
        env['CARGO_TARGET_DIR'] = str(TOOL / 'target/heap')
    subprocess.run(cmd, check=True, env=env)


def run(binary, mode, rules, extra_env=None):
    env = os.environ.copy()
    env.update({'PROFILE': 'fiction', 'SPELL': 'en_US'})
    env.update(extra_env or {})
    out = subprocess.run([str(binary), mode, str(ROOT / 'storage' / rules), str(TEXT)],
                         check=True, capture_output=True, text=True, env=env).stdout
    return [line.split('\t') for line in out.splitlines() if '\t' in line]


def median_table(runs, tag, value_columns):
    rows = {}
    order = []
    for lines in runs:
        for fields in lines:
            if fields[0] != tag:
                continue
            key = fields[1]
            if key not in rows:
                rows[key] = [[] for _ in value_columns]
                order.append(key)
            for i in range(len(value_columns)):
                rows[key][i].append(float(fields[2 + i]))
    return [(key, [statistics.median(col) for col in rows[key]],
             [(min(col), max(col)) for col in rows[key]]) for key in order]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=['phases', 'stages'])
    parser.add_argument('--rules', choices=['66'], required=True)
    parser.add_argument('--runs', type=int, default=7)
    parser.add_argument('--no-build', action='store_true')
    args = parser.parse_args()
    if not args.no_build:
        build(False)
    env = {'NLPRULE_PARALLELISM': 'off'} if args.mode == 'stages' else {}
    runs = [run(BIN, args.mode, args.rules, env) for _ in range(args.runs)]
    print(f'LT {args.rules} fiction en_US, median of {args.runs} fresh processes')
    print()
    print('| phase | time ms (median, min–max) | RSS growth MiB (median, min–max) |')
    print('|---|---:|---:|')
    for name, (ms, mib), (ms_range, mib_range) in median_table(runs, 'PHASE', ['ms', 'mib']):
        print(f'| {name} | {ms:.1f} ({ms_range[0]:.1f}–{ms_range[1]:.1f}) | '
              f'{mib:.2f} ({mib_range[0]:.2f}–{mib_range[1]:.2f}) |')
    if args.mode == 'stages':
        print()
        print('| stage | time ms (median, min–max) |')
        print('|---|---:|')
        for name, (ms,), (ms_range,) in median_table(runs, 'STAGE', ['ms']):
            print(f'| {name} | {ms:.0f} ({ms_range[0]:.0f}–{ms_range[1]:.0f}) |')
        counts = [fields for lines in runs for fields in lines if fields[0] == 'COUNTS'][0]
        print()
        print(f'{counts[1]} sentences, {counts[2]} tokens, {counts[3]} suggestions')


if __name__ == '__main__':
    main()
