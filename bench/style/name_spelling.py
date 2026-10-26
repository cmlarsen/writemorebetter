"""Seeded public-text name-spelling evaluation; no private corpus inputs."""
import argparse
import hashlib
import json
import random
import re
import subprocess
from collections import Counter
from pathlib import Path

from common import HERE, ROOT, offsets, write_jsonl
from tier2 import score

BASELINE = HERE / 'name-spelling-baseline.json'
OUT = HERE / 'out/name-spelling'
SEED = 375
# Common English respellings of a listed name, written for this suite (not from a dataset).
# Some fall within one or two edits and match at the edit tier first; both tiers are reported.
PHONETIC = [('Katherine', 'Kathryn'), ('Stephen', 'Steven'), ('Shaun', 'Shawn'),
            ('Catherine', 'Kathryn'), ('Sean', 'Shawn'), ('Jeffrey', 'Geoffrey'), ('Phillip', 'Filip'),
            ('Allison', 'Alyson'), ('Caitlin', 'Kaitlyn'), ('Christopher', 'Kristopher'),
            ('Christine', 'Kristine'), ('Cynthia', 'Synthia'), ('Zachary', 'Zackary'), ('Isaac', 'Izaak'),
            ('Jillian', 'Gillian'), ('Madeline', 'Madelyn'), ('Michael', 'Mikael'), ('Nicholas', 'Nikolas'),
            ('Frederick', 'Fredrik'), ('Ashley', 'Ashleigh'), ('Sophia', 'Sofia'), ('Joseph', 'Josef'),
            ('Raphael', 'Rafael'), ('Eleanor', 'Elinor'), ('Deborah', 'Debra'), ('Gerald', 'Jerald'),
            ('Philippa', 'Filippa'), ('Tiffany', 'Tiphanie'), ('Cecily', 'Cecilie'), ('Josephine', 'Josefine'),
            ('Stuart', 'Stewart'), ('Kathleen', 'Cathleen'), ('Marcus', 'Markus'), ('Jasmine', 'Jazmyn'),
            ('Rebecca', 'Rebekah'), ('Hailey', 'Haleigh'), ('Jonathan', 'Johnathon'), ('Elliott', 'Eliot')]
FOLDED = [('Zoë', 'Zoe', 'diacritics'), ('Mara', 'MAra', 'case'),
          ('Van der Berg', 'Vanderberg', 'spacing')]
EDIT = [('Marianne', 'Mariann', 'deletion'), ('Marianne', 'Mariannne', 'insertion'),
        ('Marianne', 'Mariannf', 'substitution'), ('Marianne', 'Maraine', 'transposition')]


def passages():
    for path in sorted((ROOT / 'bench/public').glob('story*.txt')):
        paragraphs = [p.strip() for p in re.split(r'\n\s*\n', path.read_text(encoding='utf-8'))]
        for number, paragraph in enumerate(paragraphs):
            words = list(re.finditer(r'\S+', paragraph))
            for chunk, start in enumerate(range(0, len(words), 180)):
                group = words[start:start + 180]
                if len(group) >= 25:
                    yield f'{path.stem}-{number:03}-{chunk:02}', paragraph[group[0].start():group[-1].end()]


def probe(executable, cases):
    payload = ''.join(json.dumps({'file': c['file'], 'text': c['text'], 'names': c['names']}, ensure_ascii=False) + '\n' for c in cases)
    result = subprocess.run([str(executable)], input=payload, text=True, encoding='utf-8', cwd=ROOT,
                            stdout=subprocess.PIPE, check=True)
    return {row['file']: row for row in map(json.loads, result.stdout.splitlines())}


def rows(file, result):
    return [{'file': file, 'report': 'nameSpelling', 'pattern': flag['reason'],
             'byte': flag['span']['byte'], 'utf16': flag['span']['utf16'],
             'name': flag['name'], 'alternatives': flag['alternatives']}
            for flag in result['flags']]


def cases(split):
    rng = random.Random(SEED)
    public = list(passages())
    if not public:
        raise RuntimeError('bench/public story paragraphs are missing')
    selected = public[::2] if split == 'dev' else public[1::2]
    rng.shuffle(selected)
    selected = selected[:12]
    results = []
    variants = [(name, form, 'edit', category) for name, form, category in EDIT]
    variants.append(('Mara', 'Marra', 'edit', 'possessive'))
    variants += [(name, form, 'phonetic', 'phonetic') for name, form in PHONETIC]
    variants += [(name, form, reason, reason) for name, form, reason in FOLDED]
    for index, (source, paragraph) in enumerate(selected):
        for name, form, reason, category in variants:
            # Prefix comes from real public prose; suffix exercises Unicode boundaries and possessives.
            prefix = paragraph + '\n🙂 “A name”—spoken…\u2028'
            sentence = f'{form}’s coat arrived.' if category == 'possessive' else f'{form} arrived.'
            file = f'{source}-{index:02}-{category}-{name.replace(" ", "_")}'
            text = prefix + sentence
            start = len(prefix)
            results.append({'file': file, 'text': text, 'names': [name],
                            'gold': {'file': file, 'report': 'nameSpelling', 'pattern': reason,
                                     'name': name, **offsets(text, start, start + len(form))},
                            'boundary': len(prefix.encode('utf-8')), 'category': category, 'form': form})
        for label, names, form in [('control', ['Katherine', 'Mara'], None),
                                   ('both-listed', ['Mara', 'Mary'], 'Mary'),
                                   ('near-real', ['Mara'], 'Mary')]:
            prefix = paragraph + '\n🙂 “A name”—spoken…\u2028'
            suffix = f'{form} arrived.' if form else 'The boat sailed.'
            file = f'{source}-{index:02}-{label}'
            text = prefix + suffix
            gold = None
            if label == 'near-real':
                gold = {'file': file, 'report': 'nameSpelling', 'pattern': 'edit',
                        'name': 'Mara', **offsets(text, len(prefix), len(prefix) + len(form))}
            results.append({'file': file, 'text': text, 'names': names, 'gold': gold,
                            'boundary': len(prefix.encode('utf-8')), 'category': label})
    return results


def evaluate(split, executable):
    docs = cases(split)
    measured = probe(executable, docs)
    gold = [doc['gold'] for doc in docs if doc['gold']]
    found = [row for doc in docs for row in rows(doc['file'], measured[doc['file']])
             if row['byte']['start'] >= doc['boundary']]
    natural = [row for doc in docs for row in rows(doc['file'], measured[doc['file']])
               if row['byte']['start'] < doc['boundary']]
    # Exact predictions, including spans and names, are frozen on the held-out split.
    natural_keys = sorted((r['file'], r['pattern'], r['byte']['start'], r['byte']['end'], r['name']) for r in natural)
    digest = hashlib.sha256(json.dumps(natural_keys, ensure_ascii=False).encode()).hexdigest()
    stats = score(gold, found, {d['file'] for d in docs}, report='nameSpelling')
    expected = {(r['file'], r['byte']['start'], r['byte']['end']): r for r in gold}
    correct = sum(row['name'] == expected[key]['name'] or expected[key]['name'] in row['alternatives']
                  for row in found if (key := (row['file'], row['byte']['start'], row['byte']['end'])) in expected)
    control_flags = sum(d['category'] in ('control', 'both-listed') for d in docs for row in found if row['file'] == d['file'])
    found_spans = {(row['file'], row['byte']['start'], row['byte']['end']) for row in found}
    category_recall = {}
    for category in sorted({d['category'] for d in docs if d['gold']}):
        items = [d['gold'] for d in docs if d['category'] == category]
        caught = sum((g['file'], g['byte']['start'], g['byte']['end']) in found_spans for g in items)
        category_recall[category] = {'caught': caught, 'total': len(items), 'recall': caught / len(items)}
    reasons = {(row['file'], row['byte']['start'], row['byte']['end']): row['pattern'] for row in found}
    pairs = {}
    for d in docs:
        if d['category'] == 'phonetic':
            g = d['gold']
            reason = reasons.get((g['file'], g['byte']['start'], g['byte']['end']), 'missed')
            pairs.setdefault(f"{g['name']}/{d['form']}", Counter())[reason] += 1
    tiers = Counter(reason for counts in pairs.values() for reason in counts.elements())
    words = sum(len(d['text'].split()) for d in docs if d['category'] == 'control')
    times = [measured[d['file']]['reportNs'] / 1e6 for d in docs]
    summary = {'split': split, 'seed': SEED, 'documents': len(docs), 'gold': len(gold),
               'scores': stats, 'categoryRecall': category_recall,
               'phoneticTiers': dict(tiers), 'phoneticPairs': {k: dict(v) for k, v in sorted(pairs.items())},
               'correctSuggestions': correct, 'caughtGold': sum(
                   (g['file'], g['byte']['start'], g['byte']['end']) in found_spans for g in gold), 'controlFlags': control_flags,
               'controlWords': words, 'nearRealFlags': sum(row['file'].endswith('near-real') for row in found),
               'naturalCount': len(natural), 'naturalSha256': digest,
               'reportMsMedian': sorted(times)[len(times)//2]}
    out = OUT / split
    out.mkdir(parents=True, exist_ok=True)
    write_jsonl(out / 'answers.jsonl', gold)
    write_jsonl(out / 'predictions.jsonl', found)
    write_jsonl(out / 'natural.jsonl', natural)
    write_jsonl(out / 'documents.jsonl', [{k: d[k] for k in ('file', 'category', 'names')} for d in docs])
    (out / 'summary.json').write_text(json.dumps(summary, indent=2) + '\n')
    return summary


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--gate', action='store_true', help='compare held-out natural predictions with frozen baseline')
    parser.add_argument('--freeze', action='store_true', help='write the held-out baseline after review')
    args = parser.parse_args()
    subprocess.run(['cargo', 'build', '--release', '-p', 'writemorebetter-style',
                    '--example', 'name_spelling_jsonl'], cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    executable = ROOT / 'target/release/examples/name_spelling_jsonl'
    results = {split: evaluate(split, executable) for split in ('dev', 'test')}
    if args.freeze:
        BASELINE.write_text(json.dumps({k: results['test'][k] for k in ('naturalCount', 'naturalSha256')}, indent=2) + '\n')
    if args.gate:
        baseline = json.loads(BASELINE.read_text())
        assert {k: results['test'][k] for k in baseline} == baseline, 'held-out natural predictions changed'
        test = results['test']
        rates = test['categoryRecall']
        assert all(rates[category]['recall'] >= 0.95 for category in
                   ('deletion', 'insertion', 'substitution', 'transposition'))
        assert rates['phonetic']['recall'] >= 0.80
        assert rates['possessive']['recall'] == 1
        assert all(rates[category]['recall'] == 1 for category in ('case', 'diacritics', 'spacing'))
        # Every caught item names the intended entry; misses count against recall above.
        assert test['correctSuggestions'] == test['caughtGold']
        assert test['controlFlags'] * 10000 <= test['controlWords']
        assert rates['near-real']['recall'] == 1
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
