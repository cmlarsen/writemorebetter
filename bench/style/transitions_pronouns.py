"""Public-only tier 2 development regression suite for #247."""
import argparse
import json
from pathlib import Path
import subprocess

from common import ROOT, jsonl, offsets, write_jsonl
from tier2 import public_passages, score


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
    injected, metadata = [], []
    for book, index, clean in public_passages():
        if index >= 6:
            continue
        prefix = ('She walked home. She slept.\n\nShe woke early.' if index % 2 else
                  'She walked home. He slept.\n\nShe woke early.')
        injected.append({'text': prefix + '\n\n***\n\n' + clean})
        metadata.append((book, index, prefix))
    run_gold, run_predictions = [], []
    natural_runs = 0
    for (book, index, prefix), result in zip(metadata, probe(injected, executable)):
        file = f'{book}-{index}'
        if index % 2:
            run_gold.append({'file': file, 'pattern': 'pronoun_run', **offsets(prefix, 0, len(prefix))})
        for run in result['pronounStarts']['groups']['all']['runs']:
            if run['span']['byte']['start'] < len(prefix.encode()):
                run_predictions.append({'file': file, 'pattern': 'pronoun_run', **span(run)})
            else:
                natural_runs += 1
    run_scores = score(run_gold, run_predictions, {f'{b}-{i}' for b, i, _ in metadata})
    public = []
    paths = [ROOT / f'bench/public/story{i}.txt' for i in range(1, 6)] + [ROOT / 'bench/public/moby150k.txt']
    texts = [path.read_bytes().decode() for path in paths]
    for path, result in zip(paths, probe([{'text': text, 'hardWrapped': 'moby' in path.name}
                                         for path, text in zip(paths, texts)], executable)):
        public.append({'file': path.name, 'sentences': result['transitions']['groups']['all']['sentences'],
                       'transitionMatches': len(result['transitions']['matches']),
                       'transitionRepeats': len(result['transitions']['repeats']),
                       'pronounStarts': len(result['pronounStarts']['starts']),
                       'pronounRuns': len(result['pronounStarts']['groups']['all']['runs'])})
    result = {'labels': len(labels), 'injectedDocuments': len(injected), 'transitions': transition_scores,
              'pronounRuns': run_scores, 'naturalRunsDiagnostic': natural_runs, 'public': public}
    args.out.mkdir(parents=True, exist_ok=True)
    write_jsonl(args.out / 'transition-gold.jsonl', gold)
    write_jsonl(args.out / 'transition-predictions.jsonl', predictions)
    write_jsonl(args.out / 'pronoun-gold.jsonl', run_gold)
    write_jsonl(args.out / 'pronoun-predictions.jsonl', run_predictions)
    (args.out / 'measurements.json').write_text(json.dumps(result, indent=2) + '\n')
    if args.gate:
        for scores in (transition_scores, run_scores):
            if not scores['patterns'] or any(p['f1'] != 1.0 for p in scores['patterns'].values()):
                raise ValueError('tier 2 exact-span regression')
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
