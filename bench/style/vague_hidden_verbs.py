"""Vague words and hidden verbs (#307): counts and rates per mode.

  python3 bench/style/vague_hidden_verbs.py            bench/public (story1–5 and moby150k)
  python3 bench/style/vague_hidden_verbs.py --books    also the public-domain books in sources.json
  python3 bench/style/vague_hidden_verbs.py --sample N --report hiddenVerbs [--seed S]   print N random matches

Prints a Markdown table of matches, words and rates per 1,000 words for narration and dialogue, and
each report's most frequent terms. Nothing from bench/corpus is read.
"""
import argparse
from collections import Counter
import json
import random
import subprocess

from common import ROOT
from pacing_labels import BOOKS, source_documents

TOOL = ROOT / 'bench/tools/wmb-document'
PUBLIC = ('story1', 'story2', 'story3', 'story4', 'story5', 'moby150k')
REPORTS = ('vagueWords', 'hiddenVerbs')


def probe(texts):
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml'),
                    '--bin', 'vague_hidden_verbs'], check=True, stdout=subprocess.DEVNULL)
    result = subprocess.run([str(TOOL / 'target/release/vague_hidden_verbs'), str(ROOT / 'storage/en_tokenizer.bin')],
                            input=''.join(json.dumps(text) + '\n' for text in texts),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    return [json.loads(line) for line in result.stdout.splitlines()]


def sources(books):
    for name in PUBLIC:
        yield name, (ROOT / 'bench/public' / f'{name}.txt').read_text(encoding='utf-8')
    if books:
        for book in BOOKS:
            yield book, '\n'.join(document.text for document in source_documents(book).values())


def rate(group):
    value = group['perThousandWords']
    return '—' if value is None else f'{value:.2f}'


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--books', action='store_true')
    parser.add_argument('--sample', type=int, default=0)
    parser.add_argument('--report', choices=REPORTS, default='hiddenVerbs')
    parser.add_argument('--seed', type=int, default=307)
    args = parser.parse_args()
    named = list(sources(args.books or args.sample > 0))
    results = probe([text for _, text in named])
    totals = {report: Counter() for report in REPORTS}
    terms = {report: Counter() for report in REPORTS}
    print('| Text | Report | Narration matches / words | per 1,000 | Dialogue matches / words | per 1,000 |')
    print('|---|---|---|---|---|---|')
    for (name, _), result in zip(named, results):
        for report in REPORTS:
            groups = result[report]['groups']
            narration, dialogue = groups['narration'], groups['dialogue']
            print(f"| {name} | {report} | {narration['matches']} / {narration['words']} | {rate(narration)} "
                  f"| {dialogue['matches']} / {dialogue['words']} | {rate(dialogue)} |")
            for mode, group in (('narration', narration), ('dialogue', dialogue)):
                totals[report][f'{mode} matches'] += group['matches']
                totals[report][f'{mode} words'] += group['words']
            for term in result[report]['terms']:
                terms[report][term['term']] += term['count']
    for report in REPORTS:
        total = totals[report]
        print(f"\n{report}: narration {total['narration matches']} / {total['narration words']} "
              f"({1000 * total['narration matches'] / max(total['narration words'], 1):.2f} per 1,000), "
              f"dialogue {total['dialogue matches']} / {total['dialogue words']} "
              f"({1000 * total['dialogue matches'] / max(total['dialogue words'], 1):.2f} per 1,000)")
        print('  top terms:', ', '.join(f'{term} {count}' for term, count in terms[report].most_common(20)))
    if args.sample:
        rows = []
        for (name, text), result in zip(named, results):
            for match in result[args.report]['matches']:
                start, end = match['span']['char']['start'], match['span']['char']['end']
                window = text[max(0, start - 50):end + 40].replace('\n', ' ')
                rows.append((name, match['mode'], match['term'], text[start:end], window))
        for row in random.Random(args.seed).sample(rows, min(args.sample, len(rows))):
            print(' | '.join(row))


if __name__ == '__main__':
    main()
