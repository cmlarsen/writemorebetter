#!/usr/bin/env python3
"""Compare this engine and its merge base using identical probe code and inputs."""
import argparse
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
from perf import ROOT, PROBE, run


def measure(label, probe, storage, output):
    rows = {}
    text = ROOT / 'bench/public/moby150k.txt'
    for rules in ['52', '66']:
        samples = []
        for i in range(5):
            time_log = output / f'{label}-{rules}-{i}.time'
            metrics, _ = run(probe, storage / rules, text, 'fiction',
                            prefix=['/usr/bin/time', '-v', '-o', str(time_log)])
            peak = re.search(r'Maximum resident set size \(kbytes\): (\d+)', time_log.read_text())
            metrics['peak_rss_mib'] = int(peak[1]) / 1024
            samples.append(metrics)
        rows[rules] = {key: statistics.median(s[key] for s in samples) for key in samples[0]}
        rows[rules]['samples'] = samples
    env = dict(os.environ, PROFILE='fiction', SPELL='en_US', NLPRULE_PARALLELISM='off')
    env.pop('SHOW', None)
    callgrind = output / f'{label}.callgrind'
    subprocess.run(['valgrind', '--tool=callgrind', '--cache-sim=no', '--branch-sim=no',
                    f'--callgrind-out-file={callgrind}', str(probe), str(storage / '66'), str(text)],
                   env=env, stdout=subprocess.DEVNULL, check=True)
    counts = re.findall(r'^summary: (\d+)$', callgrind.read_text(), re.MULTILINE)
    if len(counts) != 1:
        raise RuntimeError(f'Expected one instruction summary in {callgrind}')
    rows['66']['instructions'] = int(counts[0])
    allocation_log = output / f'{label}.allocations'
    subprocess.run(['valgrind', '--tool=memcheck', '--leak-check=no',
                    f'--log-file={allocation_log}', str(probe), str(storage / '66'), str(text)],
                   env=env, stdout=subprocess.DEVNULL, check=True)
    heap = re.search(r'total heap usage: ([\d,]+) allocs, ([\d,]+) frees, ([\d,]+) bytes allocated',
                     allocation_log.read_text())
    if not heap:
        raise RuntimeError(f'Missing allocation summary in {allocation_log}')
    rows['66']['allocations'] = int(heap[1].replace(',', ''))
    rows['66']['allocated_bytes'] = int(heap[3].replace(',', ''))
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--base', type=Path, required=True)
    parser.add_argument('--output', type=Path, default=ROOT / 'storage/perf-ci')
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    base = args.base.resolve()
    result = {
        'method': '5 native runs (parallel); callgrind/memcheck whole process (sequential), LT 6.6',
        'base': measure('base', base / 'bench/tools/wmb-probe/target/release/wmb-probe',
                        base / 'storage', args.output),
        'head': measure('head', PROBE, ROOT / 'storage', args.output),
    }
    lines = ['## Performance against merge base', '', result['method'], '',
             '| Rules | Metric | Base | PR | Delta |', '|---|---|---:|---:|---:|']
    for rules in ['52', '66']:
        for metric in ['check_seconds', 'peak_rss_mib', 'loaded_rss_mib', 'load_seconds',
                       'instructions', 'allocations', 'allocated_bytes']:
            if metric not in result['base'][rules]:
                continue
            before, after = result['base'][rules][metric], result['head'][rules][metric]
            delta = 100 * (after / before - 1) if before else 0
            lines.append(f'| {rules} | {metric} | {before:.4f} | {after:.4f} | {delta:+.2f}% |')
    lines.extend(['', 'Wall-clock values are informational; shared-runner timing never gates.',
                  'Instruction totals include loading and checking; both engines use the same probe,',
                  'dependency lockfiles, profile, public input, and sequential profiler settings.', ''])
    report = '\n'.join(lines)
    (args.output / 'results.json').write_text(json.dumps(result, indent=2) + '\n')
    (args.output / 'summary.md').write_text(report)
    print(report)
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as summary:
            summary.write(report)


if __name__ == '__main__':
    main()
