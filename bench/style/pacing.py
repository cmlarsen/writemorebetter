"""Public pacing regression suite and owner-only aggregate calibration."""
from collections import defaultdict
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from common import ROOT, fetch, jsonl, offsets, prf
from tier3 import distribution

BASELINE = ROOT / 'bench/style/pacing-baseline.json'
BACKSTORY = (
    'She remembered the old house and knew that her father had gone away. She had never been happy there.\n'
    'He wondered why she had forgotten the garden. He had always believed that she had understood.\n'
    'She recalled how he had been alone. She had imagined that he had known the truth.')
SIGNALS = ('dialoguePercent', 'meanSentenceLength', 'pastPerfectPercent', 'cognitionPercent')


class Probe:
    def __init__(self, tokenizer, executable=None, quiet=False):
        tool = ROOT / 'bench/tools/wmb-document'
        if executable is None:
            subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml'),
                            '--bin', 'pacing'], check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL if quiet else None)
            executable = tool / 'target/release/pacing'
        self.process = subprocess.Popen([str(executable), str(tokenizer)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, encoding='utf-8',
                                        env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})

    def report(self, text, hard_wrapped=False, options=None):
        request = {'text': text, 'hardWrapped': hard_wrapped}
        if options is not None:
            request['options'] = options
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('pacing probe exited')
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.wait() != 0:
            raise RuntimeError('pacing probe failed')


def cases():
    result = []
    for number in range(1, 6):
        source = (ROOT / f'bench/public/story{number}.txt').read_text(encoding='utf-8').splitlines()
        context = '\n'.join(source[:5])
        for name, injection, wrap in (
                ('backstory', BACKSTORY, False),
                ('unicode', BACKSTORY.replace('She had imagined', 'She’d imagined—she had remembered…'), False),
                ('long', ' '.join([BACKSTORY.replace('\n', ' ')] * 9), False),
                ('wrapped', BACKSTORY.replace('. ', '.\n').replace('\nHe wondered', '\n\nHe wondered')
                 .replace('\nShe recalled', '\n\nShe recalled'), True)):
            prefix = context.replace('\n', '\n\n') if wrap else context
            text = prefix + '\n\n\n' + injection + '\n\n\n' + prefix
            start = len(prefix) + 3
            result.append((f'story{number}-{name}', text, wrap,
                           [offsets(text, start, start + len(injection))]))
        for name, control in (
                ('natural', context),
                ('action', 'She ran. The door slammed.\nHe jumped over the wall.\nThey fled into the trees.'),
                ('dialogue', '\n'.join('“' + p + '”' for p in BACKSTORY.splitlines())),
                ('short', BACKSTORY.splitlines()[0]),
                ('scene', BACKSTORY.replace('\n', '\n\n\n'))):
            result.append((f'story{number}-{name}', context + '\n\n\n' + control, False, []))
    return result


def tier2(probe, gate=False):
    tp = predicted = gold = 0
    failures = []
    for name, text, wrapped, expected in cases():
        report = probe.report(text, wrapped)
        actual = [{key: run['span'][key] for key in ('byte', 'utf16')} for run in report['slowRuns']]
        gold += len(expected)
        predicted += len(actual)
        tp += sum(span in expected for span in actual)
        if actual != expected:
            failures.append(name)
    result = {'cases': len(cases()), 'exactSpans': prf(tp, predicted, gold), 'failures': len(failures)}
    if gate:
        baseline = json.loads(BASELINE.read_text())
        if result != baseline['tier2']:
            raise ValueError('pacing regression: ' + ', '.join(failures))
    return result


def signals_summary(report):
    return {name: distribution(values) if (values := [row['signals'][name] for row in report['paragraphs']
                                                     if row['signals'][name] is not None]) else None
            for name in SIGNALS}


def public_texts():
    for number in range(1, 6):
        path = ROOT / f'bench/public/story{number}.txt'
        yield path.name, path.read_text(encoding='utf-8'), False
    with (ROOT / 'bench/public/moby150k.txt').open(encoding='utf-8', newline='') as stream:
        text = stream.read()
    yield 'moby150k', text, True
    for key in ('styles_ch1', 'gatsby_ch1', 'sign_ch1', 'avonlea_ch1'):
        root = ET.fromstring(fetch(key).read_bytes())
        yield key, '\n'.join(''.join(p.itertext()).strip() for p in root.iter('{http://www.w3.org/1999/xhtml}p')), False


def measure_public(probe, out, options=None, suffix="public"):
    rows = []
    review = []
    for name, text, wrapped in public_texts():
        report = probe.report(text, wrapped, options)
        rows.append({'source': name, 'words': report['words']['all'], 'paragraphs': len(report['paragraphs']),
                     'qualifyingParagraphs': sum(p['slow'] for p in report['paragraphs']),
                     'runs': len(report['slowRuns']), 'signals': signals_summary(report)})
        raw = text.encode('utf-8')
        for index, run in enumerate(report['slowRuns']):
            span = run['span']['byte']
            review.append({'source': name, 'run': index, 'paragraphs': run['paragraphs'],
                           'signals': run['signals'], 'triggeredSignals': run['triggeredSignals'],
                           'text': raw[span['start']:span['end']].decode('utf-8')})
    out.mkdir(parents=True, exist_ok=True)
    (out / ('pacing-' + suffix + '.json')).write_text(json.dumps(rows, indent=2) + '\n')
    (out / ('pacing-' + suffix + '-review.json')).write_text(json.dumps(review, indent=2, ensure_ascii=False) + '\n')
    return rows


def calibrate(probe, export):
    authors = defaultdict(lambda: {'signals': {name: [] for name in SIGNALS}, 'paragraphs': 0,
                                   'slow': 0, 'runs': 0, 'words': 0})
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit = row['unit_id']
        author = row['author_key']
        if (not isinstance(unit, str) or not unit or Path(unit).name != unit or unit in ('.', '..')
                or unit in seen or not isinstance(author, str) or not author):
            raise ValueError('invalid metadata')
        seen.add(unit)
        report = probe.report((export / 'units' / (unit + '.txt')).read_text(encoding='utf-8'))
        totals = authors[author]
        totals['words'] += report['words']['all']
        totals['paragraphs'] += len(report['paragraphs'])
        totals['slow'] += sum(p['slow'] for p in report['paragraphs'])
        totals['runs'] += len(report['slowRuns'])
        for paragraph in report['paragraphs']:
            for name in SIGNALS:
                value = paragraph['signals'][name]
                if value is not None:
                    totals['signals'][name].append(value)
    if not authors:
        raise ValueError('empty export')
    summaries = []
    for totals in authors.values():
        summaries.append({
            'paragraphs': totals['paragraphs'], 'words': totals['words'], 'runs': totals['runs'],
            'qualifyingParagraphPercent': 100 * totals['slow'] / totals['paragraphs'] if totals['paragraphs'] else None,
            'runsPerThousandWords': 1000 * totals['runs'] / totals['words'] if totals['words'] else None,
            'signals': {name: distribution(values) if values else None for name, values in totals['signals'].items()},
        })
    summaries.sort(key=lambda r: (r['words'], r['paragraphs'], r['runs']))
    result = {'units': len(seen), 'authors': len(authors), 'authorAggregates': summaries,
              'authorMeanSignals': {name: distribution(values) if (values := [r['signals'][name]['mean'] for r in summaries
                                                                            if r['signals'][name] is not None]) else None
                                    for name in SIGNALS},
              'authorFlagRates': {name: distribution(values) if (values := [r[name] for r in summaries if r[name] is not None]) else None
                                  for name in ('qualifyingParagraphPercent', 'runsPerThousandWords')}}
    if not aggregate_only(result):
        raise ValueError('invalid aggregate')
    return result


KEYS = set(SIGNALS) | {'units', 'authors', 'authorAggregates', 'authorMeanSignals', 'authorFlagRates',
                      'paragraphs', 'words', 'runs', 'qualifyingParagraphPercent', 'runsPerThousandWords',
                      'signals', 'mean', 'percentiles', '0', '5', '25', '50', '75', '95', '100'}


def aggregate_only(value):
    if isinstance(value, dict):
        return all(key in KEYS and aggregate_only(item) for key, item in value.items())
    if isinstance(value, list):
        return all(aggregate_only(item) for item in value)
    return value is None or isinstance(value, (int, float))


def self_test(probe):
    with tempfile.TemporaryDirectory(prefix='style-pacing-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        rows = []
        for unit, author, text in (('one', 'a', BACKSTORY), ('two', 'a', 'She ran.'),
                                   ('three', 'b', '“Run!” she shouted.')):
            rows.append({'unit_id': unit, 'author_key': author, 'title': 'PRIVATE_SENTINEL'})
            (export / 'units' / (unit + '.txt')).write_text(text, encoding='utf-8')
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows))
        result = calibrate(probe, export)
        assert result['authors'] == 2 and result['units'] == 3
        assert sum(row['runs'] for row in result['authorAggregates']) == 1
        assert result['authorFlagRates']['qualifyingParagraphPercent']['mean'] == 37.5
        encoded = json.dumps(result)
        assert 'PRIVATE_SENTINEL' not in encoded and temporary not in encoded
        assert aggregate_only(result)
        return result


def run(args):
    private = args.tier == 'tier3'
    probe = Probe(args.tokenizer, args.probe if private else args.document_probe, quiet=private)
    try:
        result = (self_test(probe) if args.self_test else calibrate(probe, args.export)) if private else tier2(probe, args.gate)
        print(json.dumps(result, indent=2, allow_nan=False))
        if not private and args.output:
            args.output.write_text(json.dumps(result, indent=2) + '\n')
    finally:
        probe.close()


if __name__ == '__main__':
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument('--probe', type=Path)
    parser.add_argument('--sweep', action='store_true')
    parser.add_argument('--out', type=Path, default=ROOT / 'bench/style/out/pacing')
    args = parser.parse_args()
    probe = Probe(ROOT / 'storage/en_tokenizer.bin', args.probe)
    try:
        print(json.dumps(measure_public(probe, args.out), indent=2))
        if args.sweep:
            for length, perfect, cognition in ((15, 10, 10), (20, 10, 10), (20, 15, 15), (25, 30, 30)):
                options = dict(minimumSentenceLength=length, minimumPastPerfectPercent=perfect, minimumCognitionPercent=cognition)
                name = f'{length}-{perfect}-{cognition}'
                rows = measure_public(probe, args.out, options, name)
                print(json.dumps({'options': options, 'runs': sum(r['runs'] for r in rows)}))
    finally:
        probe.close()
