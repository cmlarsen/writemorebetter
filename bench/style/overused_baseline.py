#!/usr/bin/env python3
"""Build style/data/baselines/overused-words.json from Google Books Ngram English Fiction 1-grams.

Download the 2020 ("v3", 20200217) English Fiction 1-gram file into a scratch directory outside
the repository; it is about 940 MB and is never committed:

    curl -o SCRATCH/1-00000-of-00001.gz \
      https://storage.googleapis.com/books/ngrams/books/20200217/eng-fiction/1-00000-of-00001.gz
    python3 bench/style/overused_baseline.py extract SCRATCH/1-00000-of-00001.gz SCRATCH/forms.tsv
    python3 bench/style/overused_baseline.py build SCRATCH/forms.tsv

`extract` sums match counts for edition years 1990-2019. It keeps POS-tagged content forms
(NOUN, VERB, ADJ, ADV) and the filler forms, and counts the denominator as every untagged 1-gram
containing a letter or digit, the document's word-token convention. `build` lemmatizes each tagged
form with the tokenizer's own tagger (bench/tools/wmb-document `lemmatize_forms`), restricting
readings to the Penn tags of the form's universal tag, then writes the table.
"""

import argparse
import gzip
import json
import math
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / 'style/data/baselines/overused-words.json'
FIRST_YEAR, LAST_YEAR = 1990, 2019
CONTENT_TAGS = ('NOUN', 'VERB', 'ADJ', 'ADV')
FILLER_FORMS = {
    'that_ADP',
    'just_ADV', 'Just_ADV',
    'really_ADV', 'Really_ADV',
    'very_ADV', 'Very_ADV',
    'actually_ADV', 'Actually_ADV',
    'quite_ADV', 'Quite_ADV',
    'suddenly_ADV', 'Suddenly_ADV',
    'somehow_ADV', 'Somehow_ADV',
    'simply_ADV', 'Simply_ADV',
}
MINIMUM_FORM_COUNT = 1000
TABLE_SIZE = 50000
MINIMUM_RATE = 1.0
SIGNIFICANT_DIGITS = 3
SOURCE_URL = 'https://storage.googleapis.com/books/ngrams/books/20200217/eng-fiction/1-00000-of-00001.gz'
REPORT_SOURCE = ROOT / 'style/src/reports/overused_words.rs'


def window_count(fields):
    total = 0
    for field in fields:
        year = int(field[:4])
        if FIRST_YEAR <= year <= LAST_YEAR:
            total += int(field.split(',', 2)[1])
    return total


def extract(args):
    words = 0
    kept = 0
    with gzip.open(args.ngrams, 'rt', encoding='utf-8') as source, open(args.output, 'w', encoding='utf-8') as out:
        for line in source:
            token, _, rest = line.rstrip('\n').partition('\t')
            form, separator, tag = token.rpartition('_')
            if separator and form and tag in CONTENT_TAGS or token in FILLER_FORMS:
                count = window_count(rest.split('\t'))
                if count >= args.minimum or token in FILLER_FORMS:
                    out.write(f'{form}\t{tag}\t{count}\n')
                    kept += 1
            elif '_' not in token and any(c.isalnum() for c in token):
                words += window_count(rest.split('\t'))
        out.write(f'#words\t\t{words}\n')
    print(f'{kept} forms, {words} words', file=sys.stderr)


def lemmatize(forms_path, tokenizer):
    probe = ROOT / 'bench/tools/wmb-document'
    subprocess.run(['cargo', 'build', '--release', '--quiet', '--bin', 'lemmatize_forms'], cwd=probe, check=True)
    binary = probe / 'target/release/lemmatize_forms'
    with open(forms_path, encoding='utf-8') as source:
        result = subprocess.run([str(binary), str(tokenizer)], stdin=source, capture_output=True, text=True, check=True)
    return [line.split('\t') for line in result.stdout.splitlines()]


def rust_list(path, declaration):
    body = re.split(r'=\s*&?\[', path.read_text(encoding='utf-8').split(declaration, 1)[1], maxsplit=1)[1].split('];', 1)[0]
    return set(json.loads('[' + body.strip().rstrip(',') + ']'))


def excluded_lemmas():
    return (rust_list(ROOT / 'style/src/reports/echoes.rs', 'const FUNCTION_WORDS')
            | rust_list(REPORT_SOURCE, 'pub const EXCLUDED_LEMMAS')
            | rust_list(REPORT_SOURCE, 'pub const FILLER_WORDS'))


def rounded(value):
    if value <= 0:
        return 0
    digits = SIGNIFICANT_DIGITS - 1 - math.floor(math.log10(value))
    return round(value, max(digits, 0)) if digits > 0 else int(round(value, digits))


def build(args):
    rows = lemmatize(args.forms, args.tokenizer)
    words = next(int(count) for form, tag, count, *_ in rows if form == '#words')
    excluded = excluded_lemmas()
    shares = defaultdict(float)
    candidates = []
    fillers = defaultdict(int)
    lowercase = {(form, tag): int(count) for form, tag, count, *_ in rows if form == form.lower()}
    for form, tag, count, lemmas, proper in rows:
        if form == '#words':
            continue
        count = int(count)
        filler = form.lower()
        if f'{form}_{tag}' in FILLER_FORMS:
            fillers[filler] += count
            continue
        if proper == '1' or form != filler and count > lowercase.get((filler, tag), 0):
            continue
        options = [lemma for lemma in lemmas.split('|') if lemma and lemma not in excluded]
        if not options:
            continue
        candidates.append((options, count))
        for lemma in options:
            shares[lemma] += count / len(options)
    totals = defaultdict(int)
    for options, count in candidates:
        totals[max(options, key=lambda lemma: (shares[lemma], lemma))] += count
    ranked = sorted(totals.items(), key=lambda item: (-item[1], item[0]))[:args.size]
    lemmas = {lemma: rounded(count * 1e6 / words) for lemma, count in sorted(ranked) if count * 1e6 / words >= args.minimum_rate}
    table = {
        'source': {
            'dataset': 'Google Books Ngram Viewer, English Fiction 1-grams, 2020 release (v3, 20200217)',
            'url': SOURCE_URL,
            'license': 'CC BY 3.0 (https://creativecommons.org/licenses/by/3.0/)',
            'retrieved': args.retrieved,
            'years': f'{FIRST_YEAR}-{LAST_YEAR} edition years, volume-weighted (no book or author dimension)',
            'denominator': f'{words} untagged 1-grams containing a letter or digit',
            'lemmatizer': 'writemorebetter en_tokenizer.bin tagger lookup, readings restricted to the Penn tags of the Ngram universal tag; '
                          'capitalized forms excluded when they have a proper-noun reading or outnumber their lowercase form; ambiguous forms assigned to their most frequent lemma',
            'tokenizer': args.tokenizer_version,
            'build': 'bench/style/overused_baseline.py',
            'content': f'{len(lemmas)} content lemmas (NOUN, VERB, ADJ, ADV; the report\'s function words, particles and fillers excluded) '
                       f'at or above {args.minimum_rate:g} per million, counted from forms with at least {MINIMUM_FORM_COUNT} occurrences; '
                       f'rates per million words to {SIGNIFICANT_DIGITS} significant digits',
        },
        'baseline': {
            'lemmas': lemmas,
            'fillers': {word: rounded(count * 1e6 / words) for word, count in sorted(fillers.items())},
        },
    }
    lines = ['{', '  "source": ' + json.dumps(table['source'], ensure_ascii=False) + ',', '  "baseline": {']
    lines.append('    "fillers": ' + json.dumps(table['baseline']['fillers']) + ',')
    lines.append('    "lemmas": {')
    entries = [f'{json.dumps(lemma, ensure_ascii=False)}:{json.dumps(rate)}' for lemma, rate in lemmas.items()]
    lines.append(',\n'.join(entries))
    lines += ['    }', '  }', '}', '']
    args.output.write_text('\n'.join(lines), encoding='utf-8')
    print(f'{len(lemmas)} lemmas, {args.output.stat().st_size} bytes -> {args.output}', file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest='command', required=True)
    p = commands.add_parser('extract')
    p.add_argument('ngrams', type=Path)
    p.add_argument('output', type=Path)
    p.add_argument('--minimum', type=int, default=MINIMUM_FORM_COUNT)
    p.set_defaults(run=extract)
    p = commands.add_parser('build')
    p.add_argument('forms', type=Path)
    p.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    p.add_argument('--tokenizer-version', default='nlprule English 6.6 en_tokenizer.bin')
    p.add_argument('--retrieved', default='2026-10-16')
    p.add_argument('--size', type=int, default=TABLE_SIZE)
    p.add_argument('--minimum-rate', type=float, default=MINIMUM_RATE)
    p.add_argument('--output', type=Path, default=OUTPUT)
    p.set_defaults(run=build)
    args = parser.parse_args()
    args.run(args)


if __name__ == '__main__':
    main()
