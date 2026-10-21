"""Public pacing regression suite and owner-only aggregate calibration."""
from collections import defaultdict
import json
import os
from pathlib import Path
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from common import ROOT, fetch, jsonl, offsets, prf
from flag_rates import PERCENTILES, share, spread, windows
from tier3 import distribution

BASELINE = ROOT / 'bench/style/pacing-baseline.json'
BACKSTORY = (
    'She remembered the old house and knew that her father had gone away. She had never been happy there.\n'
    'He wondered why she had forgotten the garden. He had always believed that she had understood.\n'
    'She recalled how he had been alone. She had imagined that he had known the truth.')
SIGNALS = ('dialoguePercent', 'meanSentenceLength', 'pastPerfectPercent', 'cognitionPercent')
SWEEP_LENGTH = (15, 18, 20, 22, 25)
SWEEP_PERCENT = (10, 15, 20, 25, 30)
SWEEP = [(length, perfect, cognition) for length in SWEEP_LENGTH for perfect in SWEEP_PERCENT
         for cognition in SWEEP_PERCENT]


class Probe:
    def __init__(self, tokenizer, executable=None, quiet=False):
        tool = ROOT / 'bench/tools/wmb-document'
        if executable is None:
            subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml'),
                            '--bin', 'pacing'], check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL if quiet else None)
            executable = tool / 'target/release/pacing'
        self.executable = executable
        self.process = subprocess.Popen([str(executable), str(tokenizer)], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        text=True, encoding='utf-8',
                                        env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})

    def report(self, text, hard_wrapped=False, options=None, sweep=None):
        request = {'text': text, 'hardWrapped': hard_wrapped}
        if options is not None:
            request['options'] = options
        if sweep:
            request['sweep'] = sweep
        self.process.stdin.write(json.dumps(request) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('pacing probe exited')
        return json.loads(line)

    def defaults(self):
        result = subprocess.run([str(self.executable), '--defaults'], capture_output=True,
                                text=True, check=True)
        return json.loads(result.stdout)

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
        if probe.defaults() != baseline['defaults']:
            raise ValueError('pacing defaults differ from the recorded decision')
        if result != baseline['tier2']:
            raise ValueError('pacing regression: ' + ', '.join(failures))
        drifted = public_drift(public_rows(probe)[0], baseline['public'])
        if drifted:
            raise ValueError('pacing public calibration drift: ' + ', '.join(drifted))
    return result


def public_drift(rows, recorded):
    def gated(table):
        return {row['source']: (row['runs'], row['twoSignalParagraphs']) for row in table}
    measured, expected = gated(rows), gated(recorded)
    return sorted(source for source in measured.keys() | expected.keys()
                  if measured.get(source) != expected.get(source))


def two_signal_rows(report):
    return sum(not p['hasSpeech'] and p['signals']['words']['all'] > 0 and len(p['triggeredSignals']) >= 2
               for p in report['paragraphs'])


def run_paragraphs(report):
    return sum(run['paragraphs']['end'] - run['paragraphs']['start'] for run in report['slowRuns'])


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


def public_rows(probe, options=None):
    rows = []
    review = []
    for name, text, wrapped in public_texts():
        report = probe.report(text, wrapped, options)
        rows.append({'source': name, 'words': report['words']['all'], 'paragraphs': len(report['paragraphs']),
                     'twoSignalParagraphs': two_signal_rows(report), 'runParagraphs': run_paragraphs(report),
                     'runs': len(report['slowRuns']), 'signals': signals_summary(report)})
        raw = text.encode('utf-8')
        for index, run in enumerate(report['slowRuns']):
            span = run['span']['byte']
            review.append({'source': name, 'run': index, 'paragraphs': run['paragraphs'],
                           'signals': run['signals'], 'triggeredSignals': run['triggeredSignals'],
                           'text': raw[span['start']:span['end']].decode('utf-8')})
    return rows, review


def measure_public(probe, out, options=None, suffix="public"):
    rows, review = public_rows(probe, options)
    out.mkdir(parents=True, exist_ok=True)
    (out / ('pacing-' + suffix + '.json')).write_text(json.dumps(rows, indent=2) + '\n')
    (out / ('pacing-' + suffix + '-review.json')).write_text(json.dumps(review, indent=2, ensure_ascii=False) + '\n')
    return rows


def sweep_options():
    return [dict(minimumSentenceLength=length, minimumPastPerfectPercent=perfect,
                 minimumCognitionPercent=cognition) for length, perfect, cognition in SWEEP]


def eligible(paragraph):
    return not paragraph['hasSpeech'] and paragraph['signals']['words']['all'] > 0


def nested(values):
    result = {}
    for (length, perfect, cognition), value in zip(SWEEP, values):
        result.setdefault(str(length), {}).setdefault(str(perfect), {})[str(cognition)] = value
    return result


def chapter_rates(samples, index=None):
    def pick(sample, key):
        return sample[key] if index is None else sample['sweep'][index][key]
    return {'flaggedShare': share([(author, float(pick(s, 'runs') > 0)) for author, s in samples]),
            'runsPerThousandWords': spread([(author, 1000 * pick(s, 'runs') / s['words'])
                                            for author, s in samples if s['words']]),
            'runParagraphPercent': spread([(author, 100 * pick(s, 'runParagraphs') / s['paragraphs'])
                                           for author, s in samples if s['paragraphs']])}


def calibrate(probe, export):
    authors = defaultdict(lambda: {'signals': {name: [] for name in SIGNALS}, 'paragraphs': 0,
                                   'runParagraphs': 0, 'runs': 0, 'words': 0,
                                   'sweep': [{'runs': 0, 'runParagraphs': 0} for _ in SWEEP]})
    chapters, paragraphs, two_signal = [], {name: [] for name in SIGNALS}, []
    options = sweep_options()
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit = row['unit_id']
        author = row['author_key']
        if (not isinstance(unit, str) or not unit or Path(unit).name != unit or unit in ('.', '..')
                or unit in seen or not isinstance(author, str) or not author):
            raise ValueError('invalid metadata')
        seen.add(unit)
        text = (export / 'units' / (unit + '.txt')).read_text(encoding='utf-8')
        measured = probe.report(text, sweep=options)
        report = measured['report']
        totals = authors[author]
        for total, swept in zip(totals['sweep'], measured['sweep']):
            total['runs'] += swept['runs']
            total['runParagraphs'] += swept['runParagraphs']
        for window in windows(text):
            measured = probe.report(window, sweep=options)
            chapters.append((author, {'words': measured['report']['words']['all'],
                                      'paragraphs': len(measured['report']['paragraphs']),
                                      'runs': len(measured['report']['slowRuns']),
                                      'runParagraphs': run_paragraphs(measured['report']),
                                      'sweep': measured['sweep']}))
        totals['words'] += report['words']['all']
        totals['paragraphs'] += len(report['paragraphs'])
        totals['runParagraphs'] += run_paragraphs(report)
        totals['runs'] += len(report['slowRuns'])
        for paragraph in report['paragraphs']:
            for name in SIGNALS:
                value = paragraph['signals'][name]
                if value is not None:
                    totals['signals'][name].append(value)
                    if eligible(paragraph):
                        paragraphs[name].append((author, value))
            if eligible(paragraph):
                two_signal.append((author, float(len(paragraph['triggeredSignals']) >= 2)))
    if not authors:
        raise ValueError('empty export')
    means = {name: [sum(t['signals'][name]) / len(t['signals'][name]) for t in authors.values() if t['signals'][name]]
             for name in SIGNALS}
    rates = {'runParagraphPercent': [100 * t['runParagraphs'] / t['paragraphs'] for t in authors.values() if t['paragraphs']],
             'runsPerThousandWords': [1000 * t['runs'] / t['words'] for t in authors.values() if t['words']]}
    pooled = {name: [value for t in authors.values() for value in t['signals'][name]] for name in SIGNALS}
    result = {'units': len(seen), 'authors': len(authors),
              'authorsWithRunsPercent': 100 * sum(t['runs'] > 0 for t in authors.values()) / len(authors),
              'paragraphSignals': {name: distribution(values) if values else None for name, values in pooled.items()},
              'authorMeanSignals': {name: distribution(values) if values else None for name, values in means.items()},
              'authorFlagRates': {name: distribution(values) if values else None for name, values in rates.items()},
              'eligibleParagraphSignals': {name: spread(values) for name, values in paragraphs.items()},
              'twoSignalParagraphShare': share(two_signal),
              'chapter': {'samples': len(chapters), 'authorsWithWindows': len({a for a, _ in chapters}),
                          **chapter_rates(chapters)},
              'sweep': nested([{'chapter': chapter_rates(chapters, index),
                                'authorsWithRunsPercent': 100 * sum(t['sweep'][index]['runs'] > 0
                                                                    for t in authors.values()) / len(authors),
                                'authorRunsPerThousandWords': distribution(
                                    [1000 * t['sweep'][index]['runs'] / t['words'] for t in authors.values()
                                     if t['words']] or [0])}
                               for index in range(len(SWEEP))])}
    if not aggregate_only(result):
        raise ValueError('invalid aggregate')
    return result


KEYS = set(SIGNALS) | {'units', 'authors', 'authorsWithRunsPercent', 'paragraphSignals', 'authorMeanSignals',
                      'authorFlagRates', 'runParagraphPercent', 'runsPerThousandWords',
                      'mean', 'percentiles', '0', '5', '25', '50', '75', '95', '100',
                      'eligibleParagraphSignals', 'twoSignalParagraphShare', 'chapter', 'samples',
                      'authorsWithWindows', 'flaggedShare', 'sweep', 'authorRunsPerThousandWords'} | {
                          str(p) for p in PERCENTILES}
SWEEP_KEYS = tuple({str(n) for n in values} for values in (SWEEP_LENGTH, SWEEP_PERCENT, SWEEP_PERCENT))


def aggregate_only(value, sweep=()):
    if isinstance(value, dict):
        if sweep:
            return set(value) <= sweep[0] and all(aggregate_only(item, sweep[1:]) for item in value.values())
        return all(key in KEYS and aggregate_only(item, SWEEP_KEYS if key == 'sweep' else ())
                   for key, item in value.items())
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
        assert result['authorsWithRunsPercent'] == 50
        assert result['authorFlagRates']['runParagraphPercent']['mean'] == 37.5
        assert result['chapter']['samples'] == 3 and result['chapter']['flaggedShare'] == 0.25
        assert result['sweep']['18']['20']['20']['chapter']['flaggedShare'] == 0.25
        assert result['sweep']['25']['30']['30']['authorsWithRunsPercent'] <= 50
        assert result['twoSignalParagraphShare'] > 0
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
            for length, perfect, cognition in ((15, 10, 10), (20, 10, 10), (20, 15, 15), (18, 15, 20), (18, 20, 20),
                                              (18, 20, 15), (25, 30, 30)):
                options = dict(minimumSentenceLength=length, minimumPastPerfectPercent=perfect, minimumCognitionPercent=cognition)
                name = f'{length}-{perfect}-{cognition}'
                rows = measure_public(probe, args.out, options, name)
                print(json.dumps({'options': options, 'runs': sum(r['runs'] for r in rows)}))
    finally:
        probe.close()
