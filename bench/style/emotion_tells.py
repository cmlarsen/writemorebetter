"""Emotion tells (#304): flag rates and a hand-judged precision sample on published chapters.

  python3 bench/style/emotion_tells.py sample [--size 200]   write labels/emotion-tells.jsonl (unlabeled)
  python3 bench/style/emotion_tells.py heldout [--size 100]  write labels/emotion-tells-heldout.jsonl from
                                                             flags not in the development sample
  python3 bench/style/emotion_tells.py score [--gate] [--write]  precision of the current report on both
                                                             sets; --write updates emotion-tells-baseline.json

The development sample was judged before the last guards were added (content to/with, formulas,
attributive adjectives, untagged quotations); the held-out sample was drawn and judged after them.

The texts are the public-domain books and chapters in sources.json (Standard Ebooks, CC0) and the
CC0 stories in bench/public. Labels hold one short public-domain sentence window per tell; nothing
from bench/corpus is read.
"""
import argparse
from collections import Counter, defaultdict
import json
import random
import subprocess
import sys

from common import HERE, ROOT, jsonl, write_jsonl
from pacing_labels import BOOKS, PUBLIC, source_documents

LABELS = HERE / 'labels' / 'emotion-tells.jsonl'
HELDOUT = HERE / 'labels' / 'emotion-tells-heldout.jsonl'
BASELINE = HERE / 'emotion-tells-baseline.json'
SEED = 304
CONTEXT = 110
TOOL = ROOT / 'bench/tools/wmb-document'


def probe(texts, options=None):
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml'),
                    '--bin', 'emotion_tells'], check=True, stdout=subprocess.DEVNULL)
    result = subprocess.run([str(TOOL / 'target/release/emotion_tells'), str(ROOT / 'storage/en_tokenizer.bin')],
                            input=''.join(json.dumps({'text': text, 'options': options or {}}) + '\n'
                                          for text in texts),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    return [json.loads(line) for line in result.stdout.splitlines()]


def documents():
    for source in tuple(PUBLIC) + BOOKS:
        for name, document in source_documents(source).items():
            yield source, name, document.text


def flags():
    docs = list(documents())
    rows, words = [], Counter()
    for (source, name, text), report in zip(docs, probe([text for _, _, text in docs])):
        words[source] += report['groups']['narration']['words']
        for tell in report['tells']:
            start, end = tell['span']['char']['start'], tell['span']['char']['end']
            rows.append({'source': source, 'document': name, 'start': start, 'end': end,
                         'tell': text[start:end], 'word': tell['word'], 'pattern': tell['pattern'],
                         'context': window(text, start, end)})
    return rows, words


def window(text, start, end):
    left = max(text.rfind('\n', 0, start) + 1, start - CONTEXT)
    right = text.find('\n', end)
    right = min(len(text) if right < 0 else right, end + CONTEXT)
    return ('…' if left > 0 and text[left - 1] != '\n' else '') + text[left:right] + \
        ('…' if right < len(text) and text[right] != '\n' else '')


def key(row):
    return row['source'], row['document'], row['start'], row['end']


def sample(size, path, seed, exclude=()):
    rows, words = flags()
    excluded = {key(row) for row in exclude}
    rows = [row for row in rows if key(row) not in excluded]
    chosen = random.Random(seed).sample(rows, min(size, len(rows)))
    chosen.sort(key=key)
    write_jsonl(path, [{**row, 'label': None, 'note': ''} for row in chosen])
    print(json.dumps({'flags': len(rows), 'words': sum(words.values()), 'sampled': len(chosen)}))


def precision(path, current):
    labels = {key(row): row for row in jsonl(path)}
    if any(row['label'] is None for row in labels.values()):
        sys.exit('unlabeled rows in ' + str(path))
    by_pattern = defaultdict(Counter)
    for k, row in labels.items():
        state = 'kept' if k in current else 'dropped'
        by_pattern[row['pattern']][(state, row['label'])] += 1
        by_pattern['all'][(state, row['label'])] += 1
    result = {}
    for pattern, counts in sorted(by_pattern.items()):
        kept_true, kept_false = counts[('kept', True)], counts[('kept', False)]
        result[pattern] = {
            'labeled': sum(counts.values()), 'kept': kept_true + kept_false,
            'precision': round(kept_true / (kept_true + kept_false), 3) if kept_true + kept_false else None,
            'droppedTrue': counts[('dropped', True)], 'droppedFalse': counts[('dropped', False)]}
    return result


def score(gate, write):
    rows, words = flags()
    current = {key(row): row for row in rows}
    result = {'flags': len(rows), 'narrationWords': sum(words.values()),
              'perThousandWords': round(1000 * len(rows) / sum(words.values()), 3),
              'patternFlags': dict(Counter(row['pattern'] for row in rows)),
              'development': precision(LABELS, current), 'heldout': precision(HELDOUT, current)}
    print(json.dumps(result, indent=1))
    if write:
        BASELINE.write_text(json.dumps(result, indent=1) + '\n', encoding='utf-8')
    if gate and min(result[name]['all']['precision'] or 0 for name in ('development', 'heldout')) < 0.85:
        sys.exit('precision below 0.85')


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('sample').add_argument('--size', type=int, default=200)
    sub.add_parser('heldout').add_argument('--size', type=int, default=100)
    scoring = sub.add_parser('score')
    scoring.add_argument('--gate', action='store_true')
    scoring.add_argument('--write', action='store_true')
    args = parser.parse_args()
    if args.command == 'sample':
        sample(args.size, LABELS, SEED)
    elif args.command == 'heldout':
        sample(args.size, HELDOUT, SEED + 1, jsonl(LABELS))
    else:
        score(args.gate, args.write)


if __name__ == '__main__':
    main()
