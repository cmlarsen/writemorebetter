"""Drafting leftovers (#376): recall on fixtures and precision on published text.

  python3 bench/style/drafting.py                  recall over bench/style/drafting_fixtures.jsonl
  python3 bench/style/drafting.py --gate           recall plus the thresholds in drafting-baseline.json
  python3 bench/style/drafting.py --precision      bench/public and the 13 books in sources.json
  python3 bench/style/drafting.py --precision --public-only
  python3 bench/style/drafting.py --local-dir PATH owner-only private draft, aggregate counts only

The report is called through the release `drafting_jsonl` example, which also applies the edits with
`drafting::apply`. Fixtures are self-written; nothing from bench/corpus is read. The `--local-dir`
mode reads the owner's drafts only when the owner names them and prints no text, titles or paths.

`drafting_negatives.jsonl` holds ordinary prose that uses the command words and pause words as
ordinary words (*Coma patients rarely wake up.*, *a dash of salt*, *She said um and looked away.*),
so the report must find nothing in any line. Edits and spoken commands are offered only for
dictated text, so the negatives run twice, in default (typed) mode and with `dictated`. Recall
prints a row per mode and lists any line with a match; `--gate` requires every line silent in both
modes, and also that default mode returns no edits for any fixture or negative.
"""
import argparse
from collections import Counter, defaultdict
import json
from pathlib import Path
import random
import subprocess
import sys

# Keep `python3 -I` (isolated) working for the owner-only private scan.
sys.path.insert(0, str(Path(__file__).resolve().parent))

from common import HERE, ROOT, write_jsonl
from pacing_labels import BOOKS, source_documents

FIXTURES = HERE / 'drafting_fixtures.jsonl'
NEGATIVES = HERE / 'drafting_negatives.jsonl'
BASELINE = HERE / 'drafting-baseline.json'
OUT = HERE / 'out/drafting'
PUBLIC = ('story1', 'story2', 'story3', 'story4', 'story5', 'moby150k')
CATEGORIES = ('placeholder', 'filledPause', 'spokenCommand')
STYLES = ('dragon', 'apple', 'whisper')
MODES = ('all', 'narration', 'dialogue')
SEED = 376
SAMPLE = 50


def executable():
    result = subprocess.run(['cargo', 'build', '--release', '-p', 'writemorebetter-style',
                             '--example', 'drafting_jsonl'], cwd=ROOT,
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, text=True)
    if result.returncode:
        sys.stderr.write(result.stderr)
        raise SystemExit(result.returncode)
    return ROOT / 'target/release/examples/drafting_jsonl'


def report(probe, items):
    """Run one batch through the line-oriented probe. Items are {id, text, options?}."""
    payload = ''.join(json.dumps(item, ensure_ascii=False) + '\n' for item in items)
    result = subprocess.run([str(probe)], input=payload, text=True, encoding='utf-8',
                            cwd=ROOT, stdout=subprocess.PIPE, check=True)
    # U+2028 is a character in fixture and book text, not a line break; split on \n only.
    return {row['id']: row for row in
            (json.loads(line) for line in result.stdout.split('\n') if line.strip())}


def rank(value):
    return '—' if value is None else f'{value:.3f}'


def fixture_passes(fixture, result):
    """Exact edited text, declared flag terms, and silence for a negative."""
    matches = result['report']['matches']
    if result['applied'] != fixture['expected']:
        return False
    if 'flags' in fixture:
        terms = sorted(match['term'] for match in matches
                       if match['category'] == fixture['category'])
        return terms == sorted(fixture['flags'])
    if fixture['input'] == fixture['expected']:
        return not matches
    return True


def is_positive(fixture):
    return 'flags' in fixture or fixture['input'] != fixture['expected']


NEGATIVE_MODES = (('default', {}), ('dictated', {'dictated': True}))


def typed(options):
    """The same options for typed text: no `dictated`, so no `spokenCommands` either."""
    return {key: value for key, value in options.items()
            if key not in ('dictated', 'spokenCommands')}


def negatives(probe):
    """Ordinary prose that uses a command or pause word as an ordinary word must not match."""
    with NEGATIVES.open(encoding='utf-8') as stream:
        rows = [json.loads(line) for line in stream if line.strip()]
    print('\n## Ordinary-prose negatives\n')
    print('| Set | Clean | Total | Rate | Flagged |')
    print('|---|---:|---:|---:|---:|')
    modes = {}
    for name, options in NEGATIVE_MODES:
        results = report(probe, [{'id': row['id'], 'text': row['input'], 'options': options}
                                 for row in rows])
        scored = []
        for row in rows:
            result = results[row['id']]
            scored.append({
                'id': row['id'],
                'word': row['word'],
                'note': row['note'],
                'input': row['input'],
                'applied': result['applied'],
                'matches': len(result['report']['matches']),
                'clean': result['applied'] == row['input'] and not result['report']['matches'],
            })
        total = len(scored)
        clean = sum(row['clean'] for row in scored)
        flagged = sum(row['matches'] > 0 for row in scored)
        print(f'| negatives, {name} | {clean} | {total} | '
              f'{rank(clean / total if total else None)} | {flagged} |')
        modes[name] = {'total': total, 'clean': clean, 'flagged': flagged,
                       'failures': [row for row in scored if not row['clean']]}
    for name, found in modes.items():
        if found['failures']:
            print(f"\nFailing negatives, {name} ({len(found['failures'])}): "
                  + ', '.join(row['id'] for row in found['failures']))
    if not any(found['failures'] for found in modes.values()):
        print('\nEvery negative is silent in both modes.')
    return modes


def typed_edits(probe, fixtures):
    """Default (typed) mode never offers an edit, on any fixture or negative."""
    with NEGATIVES.open(encoding='utf-8') as stream:
        rows = [json.loads(line) for line in stream if line.strip()]
    items = [{'id': fixture['id'], 'text': fixture['input'],
              'options': typed(fixture.get('options', {}))} for fixture in fixtures]
    items += [{'id': row['id'], 'text': row['input'], 'options': {}} for row in rows]
    results = report(probe, items)
    edited = [item['id'] for item in items
              if any(match['edits'] for match in results[item['id']]['report']['matches'])]
    print('\n## Default mode\n')
    print(f'{len(items)} fixtures and negatives without `dictated`: '
          f'{len(edited)} with edits.')
    if edited:
        print('Edited in default mode: ' + ', '.join(edited))
    return {'total': len(items), 'edited': edited}


def recall(probe):
    with FIXTURES.open(encoding='utf-8') as stream:
        fixtures = [json.loads(line) for line in stream if line.strip()]
    results = report(probe, [{'id': fixture['id'], 'text': fixture['input'],
                              'options': fixture.get('options', {})} for fixture in fixtures])
    rows = []
    for fixture in fixtures:
        result = results[fixture['id']]
        rows.append({'id': fixture['id'], 'style': fixture['style'],
                     'category': fixture['category'], 'positive': is_positive(fixture),
                     'passed': fixture_passes(fixture, result),
                     'expected': fixture['expected'], 'applied': result['applied']})

    by_category = {}
    for category in CATEGORIES:
        group = [row for row in rows if row['category'] == category]
        positives = [row for row in group if row['positive']]
        controls = [row for row in group if not row['positive']]
        caught = sum(row['passed'] for row in positives)
        clean = sum(row['passed'] for row in controls)
        by_category[category] = {
            'positive': len(positives),
            'caught': caught,
            'recall': caught / len(positives) if positives else None,
            'controls': len(controls),
            'clean': clean,
            'controlRate': clean / len(controls) if controls else None,
        }

    by_style = {}
    for style in STYLES:
        group = [row for row in rows if row['style'] == style]
        positives = [row for row in group if row['positive']]
        by_style[style] = {
            'fixtures': len(group),
            'passed': sum(row['passed'] for row in group),
            'positive': len(positives),
            'caught': sum(row['passed'] for row in positives),
        }

    controls = [row for row in rows if not row['positive']]
    summary = {
        'fixtures': len(rows),
        'passed': sum(row['passed'] for row in rows),
        'positives': len(rows) - len(controls),
        'caught': sum(row['passed'] for row in rows if row['positive']),
        'controls': len(controls),
        'clean': sum(row['passed'] for row in controls),
        'categories': by_category,
        'styles': by_style,
        'failures': [row for row in rows if not row['passed']],
    }

    print('## Recall on fixtures\n')
    print('| Category | Positives | Caught | Recall | Controls | Clean | Control rate |')
    print('|---|---:|---:|---:|---:|---:|---:|')
    for category in CATEGORIES:
        stats = by_category[category]
        print(f"| {category} | {stats['positive']} | {stats['caught']} | "
              f"{rank(stats['recall'])} | {stats['controls']} | {stats['clean']} | "
              f"{rank(stats['controlRate'])} |")
    print(f"| all | {summary['positives']} | {summary['caught']} | "
          f"{rank(summary['caught'] / summary['positives'] if summary['positives'] else None)} | "
          f"{summary['controls']} | {summary['clean']} | "
          f"{rank(summary['clean'] / summary['controls'] if summary['controls'] else None)} |")
    print('\n| Style | Fixtures | Passed | Rate | Positives | Caught |')
    print('|---|---:|---:|---:|---:|---:|')
    for style in STYLES:
        stats = by_style[style]
        print(f"| {style} | {stats['fixtures']} | {stats['passed']} | "
              f"{rank(stats['passed'] / stats['fixtures'])} | {stats['positive']} | "
              f"{stats['caught']} |")
    print(f"\n{summary['passed']} / {summary['fixtures']} fixtures exact; "
          f"{summary['caught']} / {summary['positives']} positives; "
          f"{summary['clean']} / {summary['controls']} controls clean.")
    if summary['failures']:
        print('Failing fixtures:', ', '.join(row['id'] for row in summary['failures']))

    summary['negatives'] = negatives(probe)
    summary['typed'] = typed_edits(probe, fixtures)

    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / 'recall.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
    return summary


def check_gate(summary):
    baseline = json.loads(BASELINE.read_text(encoding='utf-8'))
    failures = []
    for category, threshold in baseline['thresholds'].items():
        stats = summary['categories'].get(category)
        measured = stats['recall'] if stats and stats['recall'] is not None else 0.0
        if measured < threshold:
            failures.append(f'{category}: {measured:.3f} < {threshold:.3f}')
    clean = summary['clean'] / summary['controls'] if summary['controls'] else 1.0
    minimum = baseline['minimumControlRate']
    if clean < minimum:
        failures.append(f'controls: {clean:.3f} < {minimum:.3f}')
    negative_minimum = baseline.get('minimumNegativeRate', 1.0)
    for name, found in (summary.get('negatives') or {}).items():
        negative_rate = (found['clean'] / found['total']) if found.get('total') else 1.0
        if negative_rate < negative_minimum:
            failures.append(f"negatives, {name}: {found['clean']}/{found['total']} clean, "
                            f'needs {negative_minimum:.3f}')
    edited = (summary.get('typed') or {}).get('edited', [])
    if edited:
        failures.append(f'default mode: {len(edited)} inputs with edits')
    if failures:
        print('\nGate failed: ' + '; '.join(failures))
        return 1
    print('\nGate passed: category recall, controls, negatives in default and dictated mode, and '
          'no edits in default mode meet drafting-baseline.json.')
    return 0


def published():
    for name in PUBLIC:
        yield name, (ROOT / 'bench/public' / f'{name}.txt').read_text(encoding='utf-8')
    for book in BOOKS:
        yield book, '\n'.join(document.text for document in source_documents(book).values())


def precision(probe, books=True, sample_size=SAMPLE, seed=SEED):
    documents = list(published()) if books else [
        (name, (ROOT / 'bench/public' / f'{name}.txt').read_text(encoding='utf-8'))
        for name in PUBLIC]
    summary = {}
    OUT.mkdir(parents=True, exist_ok=True)
    for name, options in NEGATIVE_MODES:
        print(f'## Precision on published text, {name} mode\n')
        summary[name] = precision_mode(probe, documents, options, sample_size, seed,
                                       sample=not options)
        print()
    (OUT / 'precision.json').write_text(json.dumps(summary, indent=2) + '\n', encoding='utf-8')
    return summary


def precision_mode(probe, documents, options, sample_size, seed, sample):
    results = report(probe, [{'id': name, 'text': text, 'options': options}
                             for name, text in documents])

    counts = {category: Counter() for category in CATEGORIES}
    edited = {category: Counter() for category in CATEGORIES}
    sampled = defaultdict(list)
    words = Counter()
    samples = []
    for name, text in documents:
        result = results[name]
        groups = result['report']['groups']
        for mode in ('all', 'narration', 'dialogue'):
            words[mode] += groups[mode]['words']
        for match in result['report']['matches']:
            category, mode = match['category'], match['mode']
            counts[category][mode] += 1
            counts[category]['all'] += 1
            if match['edits']:
                edited[category][mode] += 1
                edited[category]['all'] += 1
            start = match['span']['char']['start']
            end = match['span']['char']['end']
            sampled[category].append({
                'id': f'{name}:{category}:{start}',
                'source': name,
                'category': category,
                'term': match['term'],
                'mode': mode,
                'sentence': match['sentence'],
                'match': text[start:end],
                'context': text[max(0, start - 40):end + 40],
                'span': match['span'],
            })

    print(f'{len(documents)} documents, {words["all"]} words.')
    print('\n| Category | Mode | Matches | With edits | Words | per 1,000 |')
    print('|---|---|---:|---:|---:|---:|')
    for category in CATEGORIES:
        for mode in MODES:
            found = counts[category][mode]
            total = words[mode]
            rate = 1000 * found / total if total else None
            print(f'| {category} | {mode} | {found} | {edited[category][mode]} | {total} | '
                  f'{rank(rate)} |')
    edits = sum(edited[category]['all'] for category in CATEGORIES)
    print(f'\n{edits} matches with edits in published text.')

    per_category = Counter()
    if sample:
        rng = random.Random(seed)
        pools = {category: list(sampled[category]) for category in CATEGORIES}
        for pool in pools.values():
            rng.shuffle(pool)
        while len(samples) < sample_size and any(pools.values()):
            for category in CATEGORIES:
                if pools[category] and len(samples) < sample_size:
                    samples.append(pools[category].pop())
        OUT.mkdir(parents=True, exist_ok=True)
        write_jsonl(OUT / 'sample.jsonl', samples)
        per_category = Counter(row['category'] for row in samples)
        print(f"\nWrote {len(samples)} seeded matches (seed {seed}) to "
              f"`bench/style/out/drafting/sample.jsonl` for hand judgment"
              + (': ' + ', '.join(f'{category} {per_category[category]}'
                                  for category in CATEGORIES) if samples else '.'))

    summary = {
        'documents': len(documents),
        'words': dict(words),
        'categories': {
            category: {
                mode: {'matches': counts[category][mode], 'edits': edited[category][mode],
                       'words': words[mode],
                       'perThousandWords': (1000 * counts[category][mode] / words[mode]
                                            if words[mode] else None)}
                for mode in MODES
            } for category in CATEGORIES
        },
        'sample': {'seed': seed, 'size': len(samples), 'categories': dict(per_category)},
        'edits': edits,
    }
    return summary


def local_directory(probe, path):
    files = sorted(file for file in path.rglob('*')
                   if file.is_file() and file.suffix.lower() in ('.txt', '.md', '.markdown'))
    if not files:
        print('No .txt or .md drafts in the given directory.', file=sys.stderr)
        return 1
    results = report(probe, [{'id': str(index), 'text': file.read_text(encoding='utf-8')}
                             for index, file in enumerate(files)])
    counts = {category: Counter() for category in CATEGORIES}
    words = Counter()
    for result in results.values():
        for mode in ('all', 'narration', 'dialogue'):
            words[mode] += result['report']['groups'][mode]['words']
        for match in result['report']['matches']:
            counts[match['category']][match['mode']] += 1
            counts[match['category']]['all'] += 1
    print(f'Private scan: {len(files)} files, {words["all"]} words. '
          'Only aggregate counts follow; no text, titles or paths are printed.')
    print('\n| Category | Matches | Narration | Dialogue | per 1,000 |')
    print('|---|---:|---:|---:|---:|')
    for category in CATEGORIES:
        found = counts[category]['all']
        narration = counts[category]['narration']
        dialogue = counts[category]['dialogue']
        rate = 1000 * found / words['all'] if words['all'] else None
        print(f'| {category} | {found} | {narration} | {dialogue} | {rank(rate)} |')
    return 0


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--recall', action='store_true', help='score the fixtures (the default)')
    parser.add_argument('--precision', action='store_true',
                        help='public text plus the 13 books and chapters')
    parser.add_argument('--books', action='store_true', help='alias for --precision')
    parser.add_argument('--public-only', action='store_true',
                        help='limit --precision to bench/public (no downloads)')
    parser.add_argument('--gate', action='store_true',
                        help='run recall and enforce drafting-baseline.json thresholds')
    parser.add_argument('--local-dir', metavar='PATH',
                        help='owner only: count flags in local drafts, never print text')
    parser.add_argument('--sample', type=int, default=SAMPLE,
                        help=f'matches to sample in precision mode (default {SAMPLE})')
    parser.add_argument('--seed', type=int, default=SEED, help=f'sample seed (default {SEED})')
    parser.add_argument('--failures', type=int, default=0,
                        help='print this many failing fixture rows')
    args = parser.parse_args()

    if args.local_dir:
        if args.gate or args.precision or args.books:
            parser.error('--local-dir cannot be combined with --gate or precision')
        return local_directory(executable(), Path(args.local_dir))
    if args.public_only and not (args.precision or args.books):
        parser.error('--public-only needs --precision')

    probe = executable()
    if args.precision or args.books:
        if args.gate:
            parser.error('--gate scores fixtures; it cannot be combined with precision')
        precision(probe, books=not args.public_only, sample_size=args.sample, seed=args.seed)
        return 0

    summary = recall(probe)
    if args.failures:
        print('\nFailing rows:')
        for row in summary['failures'][:args.failures]:
            print(f"{row['id']} {row['category']}: expected {row['expected']!r}, "
                  f"got {row['applied']!r}")
        for name, found in summary['negatives'].items():
            for row in found['failures'][:args.failures]:
                print(f"{row['id']} negative, {name} ({row['word']}): "
                      f"expected {row['input']!r}, got {row['applied']!r}")
    if args.gate:
        return check_gate(summary)
    return 0


if __name__ == '__main__':
    sys.exit(main())
