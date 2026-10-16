import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess

from common import HERE, ROOT, offsets, write_jsonl
from tier2 import public_passages, score

TABLE = HERE / 'diction-synthetic.json'
BASELINE = HERE / 'lexical-baseline.json'
REPORTS = ('overusedWords', 'fillers', 'sensoryBalance', 'diction')
SENSES = {
    'sight': ('red', 'It was red.'),
    'sound': ('music', 'The music ended.'),
    'touch': ('cold', 'It was cold.'),
    'smell': ('stench', 'A stench rose nearby.'),
    'taste': ('salty', 'It was salty.'),
    'interoception': ('nausea', 'The nausea grew worse.'),
}
SENSORY_CONTROLS = {
    'figurative_control': 'She looked like her mother. She felt that it was late.',
    'flat_adverb_control': 'It was pretty late.',
    'not_sensory_control': 'The speech ended.',
    'sight_margin_control': 'It was beautiful.',
}
OVERUSED = (('glance', 'glanced'), ('shrug', 'shrugged'), ('blink', 'blinked'))
FILLERS = ('It was very cold. She just left. He knew that she had gone.', ('very', 'just', 'that'))
FILLER_CONTROL = 'He was a just man. That house was old.'
ABSTRACT = 'An idea, a belief, a truth.'
CONCRETE = 'A stone, a lamp, velvet.'
NEAR_ABSTRACT = 'A wish, a doubt, a whim.'
DICTION = {
    'abstract': [ABSTRACT] * 3,
    'concrete_control': [CONCRETE] * 3,
    'dialogue_control': ['“An idea, a belief, a truth,” she said.'],
    'near_threshold': [NEAR_ABSTRACT],
    'near_concrete_control': ['A habit, a season, a message.'],
    'two_rated_control': ['An idea and a belief.'],
    'broken_run': [ABSTRACT, ABSTRACT, CONCRETE, ABSTRACT],
}


class LexicalProbe:
    def __init__(self, tokenizer, executable=None, table=TABLE):
        tool = ROOT / 'bench/tools/wmb-document'
        if executable is None:
            subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml'),
                            '--bin', 'lexical_reports'], check=True, stdout=subprocess.DEVNULL)
            executable = tool / 'target/release/lexical_reports'
        self.process = subprocess.Popen([str(executable), str(tokenizer), str(table)],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL, text=True, encoding='utf-8',
                                        env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})

    def measure(self, text):
        self.process.stdin.write(json.dumps(text) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('lexical probe exited')
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.wait() != 0:
            raise RuntimeError('lexical probe failed')


def record(file, report, pattern, span):
    return {'file': file, 'report': report, 'pattern': pattern,
            'byte': span['byte'], 'utf16': span['utf16']}


def predictions(file, result):
    rows = []
    for word in result['overusedWords']['overused']:
        rows.extend(record(file, 'overusedWords', word['lemma'], span) for span in word['spans'])
    for word in result['overusedWords']['fillers']:
        rows.extend(record(file, 'fillers', word['word'], span) for span in word['spans'])
    rows.extend(record(file, 'sensoryBalance', word['modality'], word['span'])
                for word in result['sensoryBalance']['senseWords'])
    rows.extend(record(file, 'diction', 'abstract', sentence['span'])
                for sentence in result['diction']['sentences'] if sentence['abstract'])
    rows.extend(record(file, 'diction', 'run', run['span']) for run in result['diction']['abstractRuns'])
    return rows


def sentences(prefix, items):
    spans, start = [], len(prefix)
    for n, sentence in enumerate(items):
        end = start + len(sentence) + (n < len(items) - 1)
        if sentence in (ABSTRACT, NEAR_ABSTRACT):
            spans.append(('abstract', start, end))
        start = end
    return ' '.join(items), spans


def runs(spans, minimum=3):
    groups = []
    for span in spans:
        if groups and span[1] == groups[-1][-1][2]:
            groups[-1].append(span)
        else:
            groups.append([span])
    return [('run', group[0][1], group[-1][2]) for group in groups if len(group) >= minimum]


def words(prefix, suffix, labels):
    spans = []
    for label in labels:
        match = re.search(rf'\b{label}\b', suffix)
        spans.append((label, len(prefix) + match.start(), len(prefix) + match.end()))
    return spans


def generate(split):
    for book, index, clean in public_passages():
        if (index < 30) != (split == 'dev'):
            continue
        prefix = clean + '\nThe traveller’s coat—worn…\u2028lay nearby.\n'
        for lemma, surface in OVERUSED:
            if lemma in prefix.casefold():
                continue
            sentence = f'She {surface}. '
            spans = []
            for n in range(5):
                start = len(prefix) + n * len(sentence) + 4
                spans.append((lemma, start, start + len(surface)))
            yield book, index, prefix, 'overusedWords', lemma, sentence * 5, spans
            yield book, index, prefix, 'overusedWords', lemma + '_below_count', sentence * 4, []
        if not re.search(r'\bsa(y|ys|id|ying)\b', prefix, re.IGNORECASE):
            yield (book, index, prefix, 'overusedWords', 'below_ratio_control',
                   'She said so. ' * 5 + 'It is as it was. ' * 45, [])
        text, labels = FILLERS
        yield book, index, prefix, 'fillers', 'fillers', text, words(prefix, text, labels)
        yield book, index, prefix, 'fillers', 'adjective_determiner_control', FILLER_CONTROL, []
        for modality, (word, sentence) in SENSES.items():
            start = len(prefix) + sentence.index(word)
            yield book, index, prefix, 'sensoryBalance', modality, sentence, [(modality, start, start + len(word))]
        for pattern, sentence in SENSORY_CONTROLS.items():
            yield book, index, prefix, 'sensoryBalance', pattern, sentence, []
        for pattern, items in DICTION.items():
            suffix, spans = sentences(prefix, items)
            yield book, index, prefix, 'diction', pattern, suffix, spans + runs(spans)


def identity(row):
    return row['report'], row['pattern'], row['byte']['start'], row['byte']['end']


def natural_summary(rows):
    counts = {}
    for row in rows:
        report = counts.setdefault(row['report'], {})
        report[row['pattern']] = report.get(row['pattern'], 0) + 1
    keys = sorted((row['file'], *identity(row)) for row in rows)
    return {'predictions': len(rows),
            'counts': {report: dict(sorted(patterns.items())) for report, patterns in sorted(counts.items())},
            'sha256': hashlib.sha256(json.dumps(keys).encode()).hexdigest()}


def evaluate(probe, split, out):
    out.mkdir(parents=True, exist_ok=True)
    gold, found, documents, natural = [], [], [], []
    controls = changes = 0
    clean_cache = {}
    for book, index, prefix, report, pattern, suffix, spans in generate(split):
        file = f'{book}-{index:03}-{report}-{pattern}'
        text = prefix + suffix
        documents.append({'file': file, 'source': book, 'paragraph': index, 'split': split,
                          'report': report, 'pattern': pattern})
        if prefix not in clean_cache:
            clean_cache[prefix] = probe.measure(prefix)
            natural.extend(predictions(f'{book}-{index:03}', clean_cache[prefix]))
        before = [row for row in predictions(file, clean_cache[prefix]) if row['report'] == report]
        after = [row for row in predictions(file, probe.measure(text)) if row['report'] == report]
        boundary = len(prefix.encode('utf-8'))
        natural_after = [row for row in after if row['byte']['start'] < boundary]
        controls += 1
        changes += sorted(map(identity, natural_after)) != sorted(map(identity, before))
        found.extend(row for row in after if row['byte']['start'] >= boundary)
        gold.extend({'file': file, 'report': report, 'pattern': label, **offsets(text, start, end)}
                    for label, start, end in spans)
        (out / (file + '.txt')).write_text(text, encoding='utf-8')
    write_jsonl(out / 'answers.jsonl', gold)
    write_jsonl(out / 'predictions.jsonl', found)
    write_jsonl(out / 'documents.jsonl', documents)
    write_jsonl(out / 'natural.jsonl', natural)
    files = {row['file'] for row in documents}
    return {'split': split, 'documents': len(documents), 'paired_controls': controls,
            'changed_controls': changes, 'natural': natural_summary(natural),
            'reports': {report: score(gold, found, files, report=report) for report in REPORTS}}


def gate(result, baseline):
    if result['documents'] != baseline['documents'] or result['paired_controls'] != baseline['paired_controls']:
        raise ValueError('lexical benchmark coverage changed')
    if result['changed_controls'] > baseline['changed_controls']:
        raise ValueError('untouched-prefix precision regressed')
    if result['natural'] != baseline['natural']:
        raise ValueError('predictions on the untouched public paragraphs changed: '
                         f'{baseline["natural"]["counts"]} -> {result["natural"]["counts"]}; '
                         'rerun with --write-baseline only for an intended report change')
    if set(result['reports']) != set(baseline['reports']):
        raise ValueError('lexical report coverage changed')
    for report, suite in baseline['reports'].items():
        if set(result['reports'][report]['patterns']) != set(suite['patterns']):
            raise ValueError('unexpected prediction category')
        for pattern, floor in suite['patterns'].items():
            actual = result['reports'][report]['patterns'][pattern]
            for metric in ('precision', 'recall', 'f1'):
                if actual[metric] + 1e-12 < floor[metric]:
                    raise ValueError(f'{report}/{pattern}/{metric} regressed')
            if actual['gold'] != floor['gold']:
                raise ValueError('answer-key coverage changed')


def run(args):
    probe = LexicalProbe(args.tokenizer, args.probe)
    try:
        result = evaluate(probe, args.split, args.out)
    finally:
        probe.close()
    if args.gate:
        gate(result, json.loads(BASELINE.read_text())[args.split])
    if args.write_baseline:
        baseline = json.loads(BASELINE.read_text())
        baseline[args.split] = result
        BASELINE.write_text(json.dumps(baseline, indent=2) + '\n')
    if args.output:
        args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--split', choices=('dev', 'test'), default='test')
    parser.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    parser.add_argument('--probe', type=Path)
    parser.add_argument('--out', type=Path, default=HERE / 'out/lexical')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--gate', action='store_true')
    parser.add_argument('--write-baseline', action='store_true')
    run(parser.parse_args())
