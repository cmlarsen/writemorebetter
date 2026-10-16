import argparse
import json
import os
from pathlib import Path
import subprocess

from common import HERE, ROOT, offsets, write_jsonl
from tier2 import public_passages, score

TABLE = HERE / 'diction-synthetic.json'
BASELINE = HERE / 'lexical-baseline.json'
SENSES = {
    'sight': ('red', 'It was red.'),
    'sound': ('music', 'The music ended.'),
    'touch': ('cold', 'It was cold.'),
    'smell': ('stench', 'A stench rose nearby.'),
    'taste': ('salty', 'It was salty.'),
    'interoception': ('nausea', 'The nausea grew worse.'),
}
OVERUSED = (('glance', 'glanced'), ('shrug', 'shrugged'), ('blink', 'blinked'))


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
    rows.extend(record(file, 'sensoryBalance', word['modality'], word['span'])
                for word in result['sensoryBalance']['senseWords'])
    rows.extend(record(file, 'diction', 'abstract', sentence['span'])
                for sentence in result['diction']['sentences'] if sentence['abstract'])
    return rows


def generate(split):
    for book, index, clean in public_passages():
        if (index < 30) != (split == 'dev'):
            continue
        prefix = clean + '\nThe traveller’s coat—worn…\u2028lay nearby.\n'
        for lemma, surface in OVERUSED:
            if lemma in prefix.casefold():
                continue
            sentence = f'She {surface}. '
            suffix = sentence * 6
            spans = []
            for n in range(6):
                start = len(prefix) + n * len(sentence) + 4
                spans.append((lemma, start, start + len(surface)))
            yield book, index, prefix, 'overusedWords', lemma, suffix, spans
            yield book, index, prefix, 'overusedWords', lemma + '_below_count', sentence * 4, []
        for modality, (word, sentence) in SENSES.items():
            start = len(prefix) + sentence.index(word)
            yield book, index, prefix, 'sensoryBalance', modality, sentence, [(modality, start, start + len(word))]
        abstract = 'An idea, a belief, a truth.'
        concrete = 'A stone, a lamp, velvet.'
        for pattern, sentence in (('abstract', abstract), ('concrete_control', concrete)):
            suffix = ' '.join([sentence] * 3)
            spans = [('abstract', len(prefix) + n * (len(sentence) + 1),
                      len(prefix) + n * (len(sentence) + 1) + len(sentence) + (n < 2)) for n in range(3)]
            yield book, index, prefix, 'diction', pattern, suffix, spans if pattern == 'abstract' else []


def evaluate(probe, split, out):
    out.mkdir(parents=True, exist_ok=True)
    gold, found, documents = [], [], []
    controls = changes = 0
    clean_cache = {}
    for book, index, prefix, report, pattern, suffix, spans in generate(split):
        file = f'{book}-{index:03}-{report}-{pattern}'
        text = prefix + suffix
        documents.append({'file': file, 'source': book, 'paragraph': index, 'split': split,
                          'report': report, 'pattern': pattern})
        if prefix not in clean_cache:
            clean_cache[prefix] = probe.measure(prefix)
        before = [row for row in predictions(file, clean_cache[prefix]) if row['report'] == report]
        after = [row for row in predictions(file, probe.measure(text)) if row['report'] == report]
        boundary = len(prefix.encode('utf-8'))
        natural = [row for row in after if row['byte']['start'] < boundary]
        controls += 1
        changes += natural != before
        found.extend(row for row in after if row['byte']['start'] >= boundary)
        gold.extend({'file': file, 'report': report, 'pattern': label, **offsets(text, start, end)}
                    for label, start, end in spans)
        (out / (file + '.txt')).write_text(text, encoding='utf-8')
    write_jsonl(out / 'answers.jsonl', gold)
    write_jsonl(out / 'predictions.jsonl', found)
    write_jsonl(out / 'documents.jsonl', documents)
    files = {row['file'] for row in documents}
    return {'split': split, 'documents': len(documents), 'paired_controls': controls,
            'changed_controls': changes,
            'reports': {report: score(gold, found, files, report=report)
                        for report in ('overusedWords', 'sensoryBalance', 'diction')}}


def gate(result, baseline):
    if result['documents'] != baseline['documents'] or result['paired_controls'] != baseline['paired_controls']:
        raise ValueError('lexical benchmark coverage changed')
    if result['changed_controls'] > baseline['changed_controls']:
        raise ValueError('untouched-prefix precision regressed')
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
    run(parser.parse_args())
