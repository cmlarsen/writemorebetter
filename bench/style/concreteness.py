#!/usr/bin/env python3
"""Validation gate for the bundled concreteness table (#256).

Rebuilds style/data/norms/concreteness.tsv from the committed ratings and checks it is
byte-identical, then scores it against the Glasgow Norms held-out split (70%, never used by the
build), the Lancaster + WordNet ridge, run-to-run stability and coverage. Glasgow, Lancaster and
WordNet are downloaded into .cache and pinned in sources.json. --gate also requires the numbers to
match concreteness-results.json; --write records them there.
"""
import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

from common import ROOT

RESULTS = Path(__file__).resolve().parent / 'concreteness-results.json'
BUILDER = ROOT / 'scripts/build-concreteness-norms.py'
GATES = {
    'heldout_spearman': 0.88,
    'heldout_pearson': 0.87,
    'heldout_abstract_f1': 0.72,
    'convergent_spearman': 0.75,
    'stability_spearman': 0.97,
    'model_agreement': 0.85,
}
MAXIMUM_MAE = 0.35
MINIMUM_POOLED_COVERAGE = 0.94
MINIMUM_STORY_COVERAGE = 0.97
MODERN_STORIES = ['story1.txt', 'story2.txt', 'story3.txt', 'story4.txt', 'story5.txt']


def builder():
    spec = importlib.util.spec_from_file_location('concreteness_builder', BUILDER)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def table_values(path):
    values = {}
    for line in path.read_text(encoding='utf-8').splitlines():
        if line.startswith('#') or not line.strip():
            continue
        word, rating = line.split('\t')
        values[word] = float(rating)
    return values


def scored(b, table, gold, words):
    predicted = [table[w] for w in words]
    actual = [b.to_five(gold[w]) for w in words]
    return {
        'n': len(words),
        'spearman': round(b.spearman(predicted, actual), 4),
        'pearson': round(b.pearson(predicted, actual), 4),
        'abstract_f1': round(b.abstract_f1(predicted, actual), 4),
        'mae': round(sum(abs(p - a) for p, a in zip(predicted, actual)) / len(words), 4),
    }


def coverage(probe):
    texts = sorted(str(p) for p in (ROOT / 'bench/public').glob('*.txt'))
    output = subprocess.run([probe, *texts], cwd=ROOT, check=True, capture_output=True,
                            text=True).stdout
    files = {}
    for line in output.splitlines():
        path, rated, eligible = line.split('\t')[:3]
        files[Path(path).name] = {'rated': int(rated), 'eligible': int(eligible),
                                  'share': round(int(rated) / int(eligible), 4)}
    rated = sum(f['rated'] for f in files.values())
    eligible = sum(f['eligible'] for f in files.values())
    return {'files': files, 'pooled': round(rated / eligible, 4)}


def measure(probe=None):
    b = builder()
    sources = b.sources_from_cache()
    result, texts = b.outputs(sources)
    stale = b.stale_outputs(texts)
    table = table_values(b.TABLE)
    listed = {word for word, _ in b.read_words()}
    gold, senses = b.glasgow(sources['glasgow'])
    calibration, heldout = b.calibration_split(gold)
    held = sorted(w for w in heldout if w in table)
    unedited = {w: float(v) for w, v in result['unedited'].items()}
    sense_words = sorted(w for w in senses if w in table)
    sense_means = {w: sum(senses[w]) / len(senses[w]) for w in sense_words}
    ridge_words = [w for w in result['rated']
                   if b.features(w, result['norms'], result['nouns']) is not None]
    stability = {}
    for model, samples in sorted(result['ratings'].items()):
        if 1 in samples and 2 in samples:
            words = result['rated']
            stability[model] = round(b.spearman([samples[1][w] for w in words],
                                                [samples[2][w] for w in words]), 4)
    measured = {
        'entries': len(table),
        'list_entries': len(listed),
        'list_covered': len(listed & set(table)) / len(listed),
        'extra_entries': len(set(table) - listed),
        'models': {m: len(s) for m, s in sorted(result['ratings'].items())},
        'calibration': {'glasgow_words': len(calibration), 'fit_words': result['fit_words'],
                        'slope': round(result['slope'], 4),
                        'intercept': round(result['intercept'], 4)},
        'heldout': scored(b, table, gold, held),
        'heldout_unedited': scored(b, unedited, gold,
                                   sorted(w for w in heldout if w in unedited)),
        'senses': {'n': len(sense_words), 'spearman': round(b.spearman(
            [table[w] for w in sense_words], [sense_means[w] for w in sense_words]), 4)},
        'convergent': {'n': len(ridge_words), 'spearman': round(b.spearman(
            [table[w] for w in ridge_words],
            [result['predict'](b.features(w, result['norms'], result['nouns']))
             for w in ridge_words]), 4)},
        'agreement': {m: round(r, 4) for m, r in result['agreement'].items()},
        'stability': stability,
        'review_words': len(result['review']),
        'adjudicated': len(b.read_adjudication()),
    }
    if probe:
        measured['coverage'] = coverage(probe)
    return measured, stale


def failures(measured, stale):
    problems = [f'not reproducible from the committed ratings: {path}' for path in stale]
    held = measured['heldout']
    checks = [
        ('table covers the word list', measured['list_covered'] == 1.0 and not measured['extra_entries']),
        (f"held-out Spearman {held['spearman']} >= {GATES['heldout_spearman']}",
         held['spearman'] >= GATES['heldout_spearman']),
        (f"held-out Pearson {held['pearson']} >= {GATES['heldout_pearson']}",
         held['pearson'] >= GATES['heldout_pearson']),
        (f"held-out abstract F1 {held['abstract_f1']} >= {GATES['heldout_abstract_f1']}",
         held['abstract_f1'] >= GATES['heldout_abstract_f1']),
        (f"held-out MAE {held['mae']} <= {MAXIMUM_MAE}", held['mae'] <= MAXIMUM_MAE),
        (f"convergent Spearman {measured['convergent']['spearman']} >= {GATES['convergent_spearman']}",
         measured['convergent']['spearman'] >= GATES['convergent_spearman']),
    ]
    checks += [(f'{m} sample 1 vs 2 Spearman {r} >= {GATES["stability_spearman"]}',
                r >= GATES['stability_spearman']) for m, r in measured['stability'].items()]
    checks += [(f'{m} agreement {r} >= {GATES["model_agreement"]}', r >= GATES['model_agreement'])
               for m, r in measured['agreement'].items()]
    if 'coverage' in measured:
        pooled = measured['coverage']['pooled']
        checks.append((f'bench/public narration coverage {pooled} >= {MINIMUM_POOLED_COVERAGE}',
                       pooled >= MINIMUM_POOLED_COVERAGE))
        files = measured['coverage']['files']
        shares = {name: files.get(name, {}).get('share') for name in MODERN_STORIES}
        checks += [(f'{name} narration coverage {share} >= {MINIMUM_STORY_COVERAGE}',
                    share is not None and share >= MINIMUM_STORY_COVERAGE)
                   for name, share in shares.items()]
    return problems + [label for label, passed in checks if not passed]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gate', action='store_true')
    parser.add_argument('--write', action='store_true')
    parser.add_argument('--probe', help='target/release/examples/diction_coverage')
    args = parser.parse_args()
    measured, stale = measure(args.probe)
    print(json.dumps(measured, indent=2))
    problems = failures(measured, stale)
    if args.write:
        RESULTS.write_text(json.dumps(measured, indent=2) + '\n', encoding='utf-8')
    if args.gate:
        recorded = json.loads(RESULTS.read_text(encoding='utf-8'))
        if not args.probe:
            recorded.pop('coverage', None)
        if recorded != measured:
            problems.append(f'results differ from {RESULTS.name}; rerun with --write and review')
    for problem in problems:
        print('FAIL:', problem, file=sys.stderr)
    if problems:
        sys.exit(1)
    print('concreteness: all checks passed')


if __name__ == '__main__':
    main()
