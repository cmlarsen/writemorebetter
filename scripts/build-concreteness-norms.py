#!/usr/bin/env python3
"""Build style/data/norms/concreteness.tsv from the committed LLM ratings.

Usage:
  scripts/build-concreteness-norms.py words
  scripts/build-concreteness-norms.py build [--check]

`words` writes bench/style/concreteness/words.tsv from the fiction lemma list. `build` reads
bench/style/concreteness/raw/*.jsonl and adjudication.tsv, fits the scale on the Glasgow Norms
calibration split, and writes the table and the QA review list; `--check` rebuilds into memory and
fails unless both committed files are byte-identical. The Glasgow, Lancaster and WordNet files are
downloaded into bench/style/.cache and pinned in bench/style/sources.json; none of them is copied
into the outputs.
"""
import argparse
import csv
from decimal import Decimal, ROUND_HALF_EVEN
import hashlib
import io
import json
import math
from pathlib import Path
import re
import sys
import tarfile
import xml.etree.ElementTree as ET
import zipfile

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / 'bench/style/concreteness'
RAW = DATA / 'raw'
LEMMAS = ROOT / 'style/data/baselines/overused-words.json'
VARIANTS = ROOT / 'style/src/reports/overused_words.rs'
WORDS = DATA / 'words.tsv'
SUPPLEMENT = DATA / 'words-supplement.tsv'
PROMPT = DATA / 'prompt.txt'
ADJUDICATION = DATA / 'adjudication.tsv'
REVIEW = DATA / 'qa-review.tsv'
TABLE = ROOT / 'style/data/norms/concreteness.tsv'
WORDS_SHA256 = 'd48a0c8791ab536eea60fb320ab9358d6d3093733d01b43d36e105e787378966'
SUPPLEMENT_SHA256 = 'a0232fd2483e57ca28ad5ba48ae23df0543c77209f254b7206296948ef48de7c'
PROMPT_SHA256 = '0317ef4beee2b11db9719e811af732839d40536bfaac54b1d20266922b990871'
SPLIT_SEED = 'glasgow-split-256'
CALIBRATION_SHARE = 0.3
MINIMUM_AGREEMENT = 0.85
REVIEW_SPREAD = 1.5
REVIEW_RIDGE = 1.5
RIDGE_LAMBDA = 1.0
EXCLUDED = {'claude-haiku-5-5': 'sample-to-sample Spearman 0.933, below the 0.97 stability gate'}
FAMILIES = {'claude-': 'Anthropic', 'deepseek-': 'DeepSeek', 'muse-': 'Meta'}
LANCASTER_DIMENSIONS = ['Auditory', 'Gustatory', 'Haptic', 'Interoceptive', 'Olfactory', 'Visual',
                        'Foot_leg', 'Hand_arm', 'Head', 'Mouth', 'Torso']
PHYSICAL_ENTITY = '00001930'
XLSX = '{http://schemas.openxmlformats.org/spreadsheetml/2006/main}'


def word_list():
    lemmas = json.loads(LEMMAS.read_text(encoding='utf-8'))['baseline']['lemmas']
    rated = sorted(word for word in lemmas
                   if len(word) > 1 and not any(c.isdigit() for c in word))
    body = VARIANTS.read_text(encoding='utf-8').split('pub const SPELLING_VARIANTS', 1)[1]
    pairs = re.findall(r'\(\s*"([^"]+)",\s*"([^"]+)"\s*\)', body.split('];', 1)[0])
    keep = set(rated)
    rows = [(word, word) for word in rated]
    rows += [(british, american) for british, american in pairs if american in keep]
    return sorted(rows)


def write_words():
    rows = word_list()
    text = 'word\trating_from\n' + ''.join(f'{word}\t{source}\n' for word, source in rows)
    WORDS.write_bytes(text.encode('utf-8'))
    rated = sum(word == source for word, source in rows)
    digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
    print(f'{len(rows)} entries, {rated} rated, {len(rows) - rated} spelling variants, sha256 {digest}')


def pinned_text(path, expected):
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected:
        sys.exit(f'{path}: sha256 {digest}, expected {expected}')
    return data.decode('utf-8')


def read_words():
    rows = [line.split('\t') for path, digest in ((WORDS, WORDS_SHA256),
                                                  (SUPPLEMENT, SUPPLEMENT_SHA256))
            for line in pinned_text(path, digest).splitlines()[1:]]
    return sorted((word, source) for word, source in rows)


def mean(values):
    values = list(values)
    return sum(values) / len(values)


def pearson(xs, ys):
    mx, my = mean(xs), mean(ys)
    sxy = sum((x - mx) * (y - my) for x, y in zip(xs, ys))
    sxx = sum((x - mx) ** 2 for x in xs)
    syy = sum((y - my) ** 2 for y in ys)
    return sxy / math.sqrt(sxx * syy)


def ranks(values):
    order = sorted(range(len(values)), key=lambda i: values[i])
    result = [0.0] * len(values)
    start = 0
    while start < len(order):
        end = start
        while end + 1 < len(order) and values[order[end + 1]] == values[order[start]]:
            end += 1
        for position in range(start, end + 1):
            result[order[position]] = (start + end) / 2 + 1
        start = end + 1
    return result


def spearman(xs, ys):
    return pearson(ranks(xs), ranks(ys))


def zscores(values):
    m = mean(values)
    sd = math.sqrt(mean((v - m) ** 2 for v in values))
    return [(v - m) / sd for v in values]


def solve(matrix, vector):
    n = len(vector)
    rows = [list(matrix[i]) + [vector[i]] for i in range(n)]
    for column in range(n):
        pivot = max(range(column, n), key=lambda r: abs(rows[r][column]))
        rows[column], rows[pivot] = rows[pivot], rows[column]
        for r in range(n):
            if r != column and rows[r][column]:
                factor = rows[r][column] / rows[column][column]
                rows[r] = [a - factor * b for a, b in zip(rows[r], rows[column])]
    return [rows[i][n] / rows[i][i] for i in range(n)]


def linear_fit(xs, ys):
    mx, my = mean(xs), mean(ys)
    slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)
    return slope, my - slope * mx


def ridge(features, ys, penalty=RIDGE_LAMBDA):
    width = len(features[0])
    means = [mean(row[j] for row in features) for j in range(width)]
    sds = [math.sqrt(mean((row[j] - means[j]) ** 2 for row in features)) or 1.0
           for j in range(width)]

    def design(row):
        return [1.0] + [(row[j] - means[j]) / sds[j] for j in range(width)]

    rows = [design(row) for row in features]
    size = width + 1
    gram = [[sum(r[i] * r[j] for r in rows) + (penalty if i == j and i else 0.0)
             for j in range(size)] for i in range(size)]
    weights = solve(gram, [sum(r[i] * y for r, y in zip(rows, ys)) for i in range(size)])
    return lambda row: sum(w * x for w, x in zip(weights, design(row)))


def to_five(cnc):
    return 1 + (cnc - 1) * 4 / 6


def abstract_f1(predicted, gold, cut=2.5):
    tp = sum(p < cut and g < cut for p, g in zip(predicted, gold))
    flagged = sum(p < cut for p in predicted)
    actual = sum(g < cut for g in gold)
    precision = tp / flagged if flagged else 0.0
    recall = tp / actual if actual else 0.0
    return 2 * precision * recall / (precision + recall) if tp else 0.0


def glasgow(path):
    with zipfile.ZipFile(path) as archive:
        shared = [''.join(t.text or '' for t in item.iter(XLSX + 't'))
                  for item in ET.fromstring(archive.read('xl/sharedStrings.xml'))]
        sheet = ET.fromstring(archive.read('xl/worksheets/sheet1.xml'))
    words, senses = {}, {}
    for row in sheet.iter(XLSX + 'row'):
        cells = {}
        for cell in row.iter(XLSX + 'c'):
            column = re.match(r'[A-Z]+', cell.get('r')).group(0)
            value = cell.find(XLSX + 'v')
            if value is not None:
                kind = cell.get('t')
                cells[column] = (shared[int(value.text)] if kind == 's'
                                 else ('false', 'true')[int(value.text)] if kind == 'b'
                                 else value.text)
        word, cnc = cells.get('A'), cells.get('L')
        if not word or int(row.get('r')) < 3:
            continue
        word = str(word).strip().lower()
        if '(' in word:
            senses.setdefault(word.split('(', 1)[0].strip(), []).append(float(cnc))
        else:
            words[word] = float(cnc)
    return words, senses


def lancaster(path):
    with open(path, encoding='utf-8', newline='') as handle:
        return {row['Word'].strip().lower(): [float(row[f'{d}.mean']) for d in LANCASTER_DIMENSIONS]
                for row in csv.DictReader(handle)}


def wordnet(path):
    with tarfile.open(path) as archive:
        index, data = (archive.extractfile('dict/' + name).read().decode('latin-1').splitlines()
                       for name in ('index.noun', 'data.noun'))
    parents = {}
    for line in data:
        if line.startswith('  '):
            continue
        fields = line.split(' | ', 1)[0].split()
        words = int(fields[3], 16)
        at = 4 + 2 * words
        pointers = int(fields[at])
        parents[fields[0]] = [fields[at + 2 + 4 * k] for k in range(pointers)
                              if fields[at + 1 + 4 * k] in ('@', '@i')]
    physical = {}

    def is_physical(offset):
        if offset not in physical:
            physical[offset] = False
            physical[offset] = offset == PHYSICAL_ENTITY or any(
                is_physical(parent) for parent in parents.get(offset, []))
        return physical[offset]

    nouns = {}
    for line in index:
        if line.startswith('  '):
            continue
        fields = line.split()
        synsets = int(fields[2])
        flags = [is_physical(offset) for offset in fields[-synsets:]]
        nouns[fields[0].lower()] = (float(flags[0]), mean(float(f) for f in flags))
    return nouns


def features(word, norms, nouns):
    if word not in norms:
        return None
    first, share = nouns.get(word, (0.5, 0.5))
    return norms[word] + [share, float(word not in nouns), first]


def calibration_split(words):
    ordered = sorted(words, key=lambda w: hashlib.sha256(f'{SPLIT_SEED}:{w}'.encode()).hexdigest())
    cut = round(len(ordered) * CALIBRATION_SHARE)
    return set(ordered[:cut]), set(ordered[cut:])


def proxy(gold, calibration, norms, nouns):
    train = [(features(w, norms, nouns), to_five(gold[w])) for w in sorted(calibration)]
    train = [(x, y) for x, y in train if x is not None]
    return ridge([x for x, _ in train], [y for _, y in train])


def family(model):
    return next(name for prefix, name in FAMILIES.items() if model.startswith(prefix))


def load_ratings(raw=RAW):
    ratings = {}
    for path in sorted(raw.glob('*.jsonl')):
        if path.name.endswith('.responses.jsonl'):
            continue
        for line in path.read_text(encoding='utf-8').splitlines():
            row = json.loads(line)
            if row['model'] in EXCLUDED:
                continue
            samples = ratings.setdefault(row['model'], {}).setdefault(row['sample'], {})
            if row['word'] in samples:
                sys.exit(f"duplicate rating for {row['word']} in {path.name}")
            samples[row['word']] = row['rating']
    return ratings


def rating_dates(raw=RAW):
    dates = set()
    for path in raw.glob('*.responses.jsonl'):
        for line in path.read_text(encoding='utf-8').splitlines():
            dates.add(json.loads(line)['started'][:10])
    return min(dates), max(dates)


def model_means(ratings, rated):
    means = {}
    for model, samples in sorted(ratings.items()):
        for sample, values in sorted(samples.items()):
            missing = [w for w in rated if w not in values]
            if missing:
                sys.exit(f'{model} sample {sample} is missing {len(missing)} words, e.g. {missing[:5]}')
        means[model] = [mean(samples[s][w] for s in samples) for w in rated]
    return means


def consensus_of(zs, models):
    families = {}
    for model in models:
        families.setdefault(family(model), []).append(zs[model])
    per_family = [[mean(column) for column in zip(*members)] for members in families.values()]
    return [mean(column) for column in zip(*per_family)]


def rounded(value):
    return Decimal(repr(min(5.0, max(1.0, value)))).quantize(Decimal('0.1'), ROUND_HALF_EVEN)


def read_adjudication():
    if not ADJUDICATION.exists():
        return {}
    edits = {}
    for line in ADJUDICATION.read_text(encoding='utf-8').splitlines()[1:]:
        word, before, after, reason = line.split('\t')
        if not reason.strip():
            sys.exit(f'adjudication of {word} has no reason')
        edits[word] = (Decimal(before), Decimal(after))
    return edits


def compute(sources, raw=RAW):
    rows = read_words()
    rated = [word for word, source in rows if word == source]
    ratings = load_ratings(raw)
    models = sorted(ratings)
    means = model_means(ratings, rated)
    zs = {model: zscores(values) for model, values in means.items()}
    consensus = consensus_of(zs, models)
    agreement = {}
    for model in models:
        others = [m for m in models if m != model]
        if others:
            agreement[model] = spearman(zs[model], consensus_of(zs, others))
    gold, _ = glasgow(sources['glasgow'])
    calibration, _ = calibration_split(gold)
    index = {word: i for i, word in enumerate(rated)}
    fit = [w for w in rated if w in calibration]
    slope, intercept = linear_fit([consensus[index[w]] for w in fit], [to_five(gold[w]) for w in fit])
    values = {word: rounded(slope * consensus[index[word]] + intercept) for word in rated}
    unedited = dict(values)
    for word, (before, after) in read_adjudication().items():
        if word not in values:
            sys.exit(f'adjudicated word {word} is not in the list')
        if values[word] != before:
            sys.exit(f'adjudication of {word} expects {before}, the build gives {values[word]}')
        values[word] = after
    table = {word: values[source] for word, source in rows}
    norms, nouns = lancaster(sources['lancaster']), wordnet(sources['wordnet'])
    predict = proxy(gold, calibration, norms, nouns)
    review = []
    for word in rated:
        model_values = [means[m][index[word]] for m in models]
        spread = max(model_values) - min(model_values)
        row = features(word, norms, nouns)
        ridge_value = predict(row) if row is not None else None
        reasons = []
        if spread > REVIEW_SPREAD:
            reasons.append('spread')
        if ridge_value is not None and abs(float(unedited[word]) - ridge_value) > REVIEW_RIDGE:
            reasons.append('ridge')
        if reasons:
            review.append((word, unedited[word], ridge_value, spread, model_values, reasons))
    return {'rows': rows, 'rated': rated, 'models': models, 'ratings': ratings, 'means': means,
            'agreement': agreement, 'slope': slope, 'intercept': intercept, 'fit_words': len(fit),
            'table': table, 'unedited': unedited, 'review': review, 'consensus': consensus,
            'calibration': calibration, 'gold': gold, 'predict': predict, 'norms': norms,
            'nouns': nouns}


def header(result, sources, raw=RAW):
    first, last = rating_dates(raw)
    counts = ', '.join(f'{m} ({len(result["ratings"][m])} samples)' for m in result['models'])
    edits = len(read_adjudication())
    digest = hashlib.sha256(Path(sources['glasgow']).read_bytes()).hexdigest()
    excluded = ''.join(f'\n# {model} was also rated but is not used: {reason}.'
                       for model, reason in sorted(EXCLUDED.items())
                       if any(raw.glob(f'{model}-s*.jsonl')))
    return f"""\
# writemorebetter concreteness ratings, AI-generated, 1 (abstract) to 5 (concrete), MIT OR Apache-2.0.
# Generated {first}{'' if first == last else ' to ' + last} by prompting {counts}
# with writemorebetter's own instructions (bench/style/concreteness/prompt.txt, sha256 {PROMPT_SHA256})
# over bench/style/concreteness/words.tsv (sha256 {WORDS_SHA256}): the fiction lemmas
# of style/data/baselines/overused-words.json without numerals and single letters, plus British spellings
# rated under their American lemma, and words-supplement.tsv (sha256 {SUPPLEMENT_SHA256}): 13 particles
# and adverbs that list leaves out but diction rates. Each model's ratings are averaged over its samples and z-scored, the
# models averaged, and the mean mapped to 1-5 by a linear fit to a 30% calibration split of the
# concreteness ratings in the Glasgow Norms (Scott, Keitel, Becirspahic, Yao & Sereno, 2019,
# https://doi.org/10.3758/s13428-018-1099-3; https://osf.io/42s65/, GlasgowNorms.xlsx, sha256
# {digest}, CC BY 4.0), then rounded to one decimal.
# No Glasgow ratings are included. {edits} values were adjudicated by hand
# (bench/style/concreteness/adjudication.tsv). Built by scripts/build-concreteness-norms.py.{excluded}
"""


def table_text(result, sources, raw=RAW):
    lines = ''.join(f'{word}\t{value}\n' for word, value in sorted(result['table'].items()))
    return header(result, sources, raw) + lines


def review_text(result):
    out = io.StringIO()
    models = result['models']
    out.write('\t'.join(['word', 'table', 'ridge', 'spread', *models, 'reasons']) + '\n')
    for word, value, ridge_value, spread, model_values, reasons in result['review']:
        cells = [word, str(value), '' if ridge_value is None else f'{ridge_value:.2f}',
                 f'{spread:.2f}', *(f'{v:.2f}' for v in model_values), ','.join(reasons)]
        out.write('\t'.join(cells) + '\n')
    return out.getvalue()


def sources_from_cache():
    sys.path.insert(0, str(ROOT / 'bench/style'))
    import common
    return {key: common.fetch(key) for key in ('glasgow', 'lancaster', 'wordnet')}


def outputs(sources, raw=RAW):
    result = compute(sources, raw)
    low = {m: round(r, 3) for m, r in result['agreement'].items() if r < MINIMUM_AGREEMENT}
    if low:
        sys.exit(f'model agreement with the other models below {MINIMUM_AGREEMENT}: {low}')
    return result, {TABLE: table_text(result, sources, raw), REVIEW: review_text(result)}


def stale_outputs(texts):
    return [str(path.relative_to(ROOT)) for path, text in texts.items()
            if not path.exists() or path.read_bytes() != text.encode('utf-8')]


def build(check):
    pinned_text(PROMPT, PROMPT_SHA256)
    sources = sources_from_cache()
    result, texts = outputs(sources)
    if check:
        stale = stale_outputs(texts)
        if stale:
            sys.exit(f'not reproducible from the committed ratings: {", ".join(stale)}')
    else:
        for path, text in texts.items():
            path.write_bytes(text.encode('utf-8'))
    print(f"{len(result['table'])} entries from {len(result['models'])} models; "
          f"fit on {result['fit_words']} calibration words: "
          f"{result['slope']:.4f} x z + {result['intercept']:.4f}; "
          f"{len(result['review'])} words to review; agreement "
          + ', '.join(f'{m} {r:.3f}' for m, r in result['agreement'].items()))


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('command', choices=['words', 'build'])
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.command == 'words':
        write_words()
    else:
        build(args.check)


if __name__ == '__main__':
    main()
