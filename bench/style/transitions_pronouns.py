"""Public-only tier 2 development regression suite for #247."""
import argparse
import json
from pathlib import Path
import subprocess
import tarfile
import xml.etree.ElementTree as ET

from common import ROOT, fetch, jsonl, offsets, write_jsonl
from tier2 import public_passages, score


def whole(text):
    return [(0, len(text))]


def pairs(text, phrase):
    folded = text.lower().replace('’', "'")
    starts = [i for i in range(len(folded)) if folded.startswith(phrase, i)]
    return [(a, b + len(phrase)) for a, b in zip(starts, starts[1:])]


RUN_FIXTURES = [
    ('same-two-across-paragraph', 'She was home. She was tired.\n\nShe was done.', True),
    ('contraction', 'It’s late. It’s cold. It’s over.', True),
    ('line-separator', 'They were tired.\u2028They were cold.\u2028They were done.', True),
    ('one-speaker', '“I was there. I saw it. I know,” she said.', True),
    ('rhetorical', 'Who knew? Who cared? Who would ever tell?', True),
    ('same-word-runs', 'She walked home. She slept.\n\nShe woke early.', False),
    ('two-speakers', '“She left,” I said.\n\n“She did,” he said.\n\n“She will,” I said.', False),
    ('alternating', 'She walked home. He slept.\n\nShe woke early.', False),
    ('one-nominal', 'One evening she left. One day he came. One morning it rained.', False),
    ('determiners', 'Her coat was red. Her hat was blue. Her shoes were black.', False),
]
REPEAT_FIXTURES = [
    ('whats-more', 'What’s more, she paid. What’s more, he paid. What’s more, they paid.', "what's more"),
    ('narration-repeat', 'However, she stayed. However, he left.', 'however'),
    ('one-speaker-repeat', '“But I can’t. But I won’t,” she said.', 'but'),
    ('two-speaker-repeat', '“But I can’t,” she said.\n\n“But you must,” he said.\n\n“But why?” she asked.', None),
]
FIXTURES = ([(name, text, whole(text) if run else [], []) for name, text, run in RUN_FIXTURES]
            + [(name, text, [], pairs(text, phrase) if phrase else []) for name, text, phrase in REPEAT_FIXTURES])


def pride_chapters(count=6):
    chapters = []
    with tarfile.open(fetch('pride')) as archive:
        members = [m for m in archive.getmembers() if '/src/epub/text/chapter-' in m.name
                   and m.name.endswith('.xhtml')]
        for member in sorted(members, key=lambda m: int(Path(m.name).stem.removeprefix('chapter-')))[:count]:
            root = ET.fromstring(archive.extractfile(member).read())
            paragraphs = (''.join(p.itertext()).strip() for p in root.iter('{http://www.w3.org/1999/xhtml}p'))
            chapters.append('\n\n'.join(p for p in paragraphs if p))
    return '\n\n***\n\n'.join(chapters)


def probe(rows, executable):
    result = subprocess.run([str(executable)], input=''.join(json.dumps(row) + '\n' for row in rows),
                            text=True, capture_output=True, check=True, cwd=ROOT)
    results = [json.loads(line) for line in result.stdout.splitlines()]
    if len(results) != len(rows):
        raise ValueError('probe row count mismatch')
    return results


def span(row):
    return {unit: row['span'][unit] for unit in ('byte', 'utf16')}


def run(args):
    executable = args.probe
    if executable is None:
        subprocess.run(['cargo', 'build', '-p', 'writemorebetter-style', '--example',
                        'transition_reports'], check=True, cwd=ROOT, stdout=subprocess.DEVNULL)
        executable = ROOT / 'target/debug/examples/transition_reports'
    labels = jsonl(ROOT / 'bench/style/labels/transitions.jsonl')
    rows = [{'text': row['text']} for row in labels]
    gold, predictions = [], []
    for index, (row, result) in enumerate(zip(labels, probe(rows, executable))):
        file = f'transition-{index}'
        for phrase, kind in row['expected']:
            start = row['text'].lower().index(phrase)
            gold.append({'file': file, 'pattern': kind, **offsets(row['text'], start, start + len(phrase))})
        for match in result['transitions']['matches']:
            predictions.append({'file': file, 'pattern': match['kind'], **span(match)})
    transition_scores = score(gold, predictions, {f'transition-{i}' for i in range(len(rows))})
    by_book = {}
    for book, index, clean in public_passages():
        by_book.setdefault(book, []).append(clean)
    injected, metadata, run_gold, repeat_gold = [], [], [], []
    for number, (name, fixture, runs, repeats) in enumerate(FIXTURES):
        book = ('pride', 'moby')[number % 2]
        source = by_book[book][number // 2]
        text = source + '\n\n' + fixture
        base = len(source) + 2
        file = f'{name}-{book}-{number // 2}'
        for start, end in runs:
            run_gold.append({'file': file, 'pattern': 'pronoun_run', **offsets(text, base + start, base + end)})
        for start, end in repeats:
            repeat_gold.append({'file': file, 'pattern': 'transition_repeat',
                                **offsets(text, base + start, base + end)})
        injected.append({'text': text})
        metadata.append((file, len(text[:base].encode())))
    run_predictions, repeat_predictions = [], []
    natural = {'pronounRuns': 0, 'transitionRepeats': 0}
    for (file, base), result in zip(metadata, probe(injected, executable)):
        for key, pattern, rows, out in (
                ('pronounRuns', 'pronoun_run', result['pronounStarts']['groups']['all']['runs'], run_predictions),
                ('transitionRepeats', 'transition_repeat', result['transitions']['repeats'], repeat_predictions)):
            for row in rows:
                if row['span']['byte']['start'] >= base:
                    out.append({'file': file, 'pattern': pattern, **span(row)})
                else:
                    natural[key] += 1
    files = {file for file, _ in metadata}
    run_scores = score(run_gold, run_predictions, files)
    repeat_scores = score(repeat_gold, repeat_predictions, files)
    public = []
    paths = [ROOT / f'bench/public/story{i}.txt' for i in range(1, 6)] + [ROOT / 'bench/public/moby150k.txt']
    names = [path.name for path in paths] + ['pride-ch1-6']
    texts = [path.read_bytes().decode() for path in paths] + [pride_chapters()]
    for name, result in zip(names, probe([{'text': text, 'hardWrapped': name.startswith('moby')}
                                         for name, text in zip(names, texts)], executable)):
        public.append({'file': name, 'sentences': result['transitions']['groups']['all']['sentences'],
                       'transitionMatches': len(result['transitions']['matches']),
                       'transitionRepeats': len(result['transitions']['repeats']),
                       'pronounStarts': len(result['pronounStarts']['starts']),
                       'pronounRuns': len(result['pronounStarts']['groups']['all']['runs'])})
    result = {'labels': len(labels), 'injectedDocuments': len(injected), 'transitions': transition_scores,
              'pronounRuns': run_scores, 'transitionRepeats': repeat_scores, 'naturalDiagnostic': natural,
              'public': public}
    args.out.mkdir(parents=True, exist_ok=True)
    write_jsonl(args.out / 'transition-gold.jsonl', gold)
    write_jsonl(args.out / 'transition-predictions.jsonl', predictions)
    write_jsonl(args.out / 'pronoun-gold.jsonl', run_gold)
    write_jsonl(args.out / 'pronoun-predictions.jsonl', run_predictions)
    write_jsonl(args.out / 'repeat-gold.jsonl', repeat_gold)
    write_jsonl(args.out / 'repeat-predictions.jsonl', repeat_predictions)
    (args.out / 'measurements.json').write_text(json.dumps(result, indent=2) + '\n')
    if args.gate:
        if not all(scores['patterns'] for scores in (transition_scores, run_scores, repeat_scores)):
            raise ValueError('tier 2 suite scored no patterns')
        baseline = json.loads((ROOT / 'bench/style/transitions-pronouns-baseline.json').read_text())
        if result != baseline:
            raise ValueError('tier 2 measurement changed; inspect before rebaselining')
    print(json.dumps(result, indent=2))


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--probe', type=Path)
    parser.add_argument('--gate', action='store_true')
    parser.add_argument('--out', type=Path, default=ROOT / 'bench/style/out/transitions-pronouns')
    run(parser.parse_args())
