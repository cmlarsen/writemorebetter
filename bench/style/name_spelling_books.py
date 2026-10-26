"""Name spelling precision on public text with real rosters (#375).

  python3 bench/style/name_spelling_books.py            run, print per-text flags and the judged sample
  python3 bench/style/name_spelling_books.py --check    also fail if a sampled flag has no judgment

Runs the release report over bench/public (story1–5, moby150k) and the 13 public-domain books and
chapters in sources.json, one document per chapter, with the rosters in name-spelling-rosters.json.
A seeded sample of at most 150 flags is matched against labels/name-spelling-judgments.jsonl.
Nothing from bench/corpus is read.
"""
import argparse
import json
import random
import subprocess
from collections import Counter

from common import HERE, ROOT, write_jsonl
from name_spelling import probe
from pacing_labels import BOOKS, source_documents

ROSTERS = HERE / 'name-spelling-rosters.json'
JUDGMENTS = HERE / 'labels/name-spelling-judgments.jsonl'
OUT = HERE / 'out/name-spelling/books'
PUBLIC = ('story1', 'story2', 'story3', 'story4', 'story5', 'moby150k')
LABELS = ('useful', 'not useful', 'unlisted name')
SEED = 375
SAMPLE = 150


def documents():
    for name in PUBLIC:
        yield name, f'{name}.txt', (ROOT / 'bench/public' / f'{name}.txt').read_text(encoding='utf-8')
    for book in BOOKS:
        for chapter, document in source_documents(book).items():
            yield book, chapter, document.text


def key(flag):
    return (flag['source'], flag['document'], flag['char'], flag['form'], flag['name'])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='require a judgment for every sampled flag')
    args = parser.parse_args()
    subprocess.run(['cargo', 'build', '--release', '-p', 'writemorebetter-style',
                    '--example', 'name_spelling_jsonl'], cwd=ROOT, check=True, capture_output=True)
    rosters = json.loads(ROSTERS.read_text())
    docs = list(documents())
    cases = [{'file': f'{source}:{name}', 'text': text, 'names': rosters[source]} for source, name, text in docs]
    measured = probe(ROOT / 'target/release/examples/name_spelling_jsonl', cases)
    words, flags = Counter(), []
    for (source, name, text), case in zip(docs, cases):
        words[source] += len(text.split())
        for flag in measured[case['file']]['flags']:
            start, end = flag['span']['char']['start'], flag['span']['char']['end']
            flags.append({'source': source, 'document': name, 'char': start, 'form': flag['form'],
                          'name': flag['name'], 'reason': flag['reason'],
                          'context': ' '.join(text[max(0, start - 60):end + 40].split())})
    OUT.mkdir(parents=True, exist_ok=True)
    write_jsonl(OUT / 'flags.jsonl', flags)
    sample = random.Random(SEED).sample(flags, min(SAMPLE, len(flags)))
    sample.sort(key=key)
    write_jsonl(OUT / 'sample.jsonl', sample)
    judged = {}
    if JUDGMENTS.exists():
        for line in JUDGMENTS.read_text(encoding='utf-8').splitlines():
            row = json.loads(line)
            assert row['label'] in LABELS, row
            judged[key(row)] = row['label']
    missing = [flag for flag in sample if key(flag) not in judged]
    labels = Counter(judged[key(flag)] for flag in sample if key(flag) in judged)
    per_text = Counter(flag['source'] for flag in flags)
    print('| Text | Words | Flags | per 10,000 words |')
    print('|---|---:|---:|---:|')
    for source in PUBLIC + BOOKS:
        print(f'| {source} | {words[source]:,} | {per_text[source]} | {10000 * per_text[source] / words[source]:.2f} |')
    total = sum(words.values())
    print(f'| **all** | {total:,} | {len(flags)} | {10000 * len(flags) / total:.2f} |')
    print('\nreasons:', dict(Counter(flag['reason'] for flag in flags)))
    print('top forms:', ', '.join(f'{form} {count}' for form, count in
                                   Counter(f"{flag['form']}→{flag['name']}" for flag in flags).most_common(25)))
    rated = labels['useful'] + labels['not useful']
    summary = {'flags': len(flags), 'words': total, 'per10k': 10000 * len(flags) / total,
               'sample': len(sample), 'judged': sum(labels.values()), 'labels': dict(labels),
               'usefulRate': labels['useful'] / rated if rated else None, 'missing': len(missing)}
    print(json.dumps(summary, indent=2))
    if args.check:
        assert not missing, f'{len(missing)} sampled flags lack judgments; see {OUT / "sample.jsonl"}'


if __name__ == '__main__':
    main()
