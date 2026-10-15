#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import xml.etree.ElementTree as ET

from common import ROOT, HERE, fetch, offsets, write_jsonl
from tier2 import WORD, score

BOUNDARY = re.compile(r'(?:(?<=[.!?…])|(?<=[.!?…][”’])) (?=[“‘]?[A-Z])')
NOUNS = [('lantern', 'lanterns'), ('thimble', 'thimbles'), ('kettle', 'kettles'),
         ('ribbon', 'ribbons'), ('pebble', 'pebbles'), ('saddle', 'saddles'),
         ('violin', 'violins'), ('puzzle', 'puzzles'), ('ladder', 'ladders'),
         ('basket', 'baskets'), ('pillow', 'pillows'), ('blanket', 'blankets'),
         ('mitten', 'mittens'), ('teacup', 'teacups'), ('umbrella', 'umbrellas'),
         ('carpet', 'carpets')]
VERBS = [('trembled', 'trembling'), ('whispered', 'whispering'), ('shivered', 'shivering'),
         ('stumbled', 'stumbling'), ('wandered', 'wandering'), ('hesitated', 'hesitating'),
         ('giggled', 'giggling'), ('frowned', 'frowning'), ('knelt', 'kneeling'),
         ('wept', 'weeping'), ('crept', 'creeping'), ('slung', 'slinging')]
PHRASES = [('the crooked brass ', 'gate', 'gates'), ('a faded velvet ', 'ribbon', 'ribbons'),
           ('the narrow copper ', 'kettle', 'kettles'), ('the chipped porcelain ', 'teacup', 'teacups'),
           ('a battered tin ', 'lantern', 'lanterns'), ('the miller’s silver ', 'thimble', 'thimbles'),
           ('the frayed woollen ', 'blanket', 'blankets'), ('a crooked oak ', 'ladder', 'ladders')]
ABBREVIATION = re.compile(r'\b(?:Mr|Mrs|Ms|Dr|St|Messrs|Mme|Capt|Col|Gen|Rev)\.$')
PASSAGES_PER_BOOK = 40
NEAR = (12, 35)
FAR = 70


def stem(word):
    return word.removesuffix('s')[:5].lower()


def absent(text, phrases):
    folded = text.lower()
    return all(stem(word) not in folded for phrase in phrases for word in WORD.findall(phrase)
               if word not in ('a', 'the'))


def passages():
    for book in ('pride', 'moby'):
        count = source_words = 0
        with tarfile.open(fetch(book)) as archive:
            chapters = [m for m in archive.getmembers() if '/src/epub/text/chapter-' in m.name
                        and m.name.endswith('.xhtml')]
            for member in sorted(chapters, key=lambda m: int(Path(m.name).stem.removeprefix('chapter-'))):
                root = ET.fromstring(archive.extractfile(member).read())
                for paragraph in root.iter('{http://www.w3.org/1999/xhtml}p'):
                    text = ' '.join(''.join(paragraph.itertext()).split())
                    word_count = len(WORD.findall(text))
                    source_words += word_count
                    if source_words - word_count < 2500 or word_count < 110:
                        continue
                    if len(boundaries(text)) >= 4:
                        yield book, count, text
                        count += 1
                        if count == PASSAGES_PER_BOOK:
                            break
                if count == PASSAGES_PER_BOOK:
                    break
        if count != PASSAGES_PER_BOOK:
            raise ValueError('not enough public source passages')


def boundaries(text):
    return [m.start() for m in BOUNDARY.finditer(text) if not ABBREVIATION.search(text[:m.start()])]


def insert(clean, insertions):
    text, shifted = clean, []
    for position, sentence in sorted(insertions, reverse=True):
        text = text[:position] + ' ' + sentence + text[position:]
    shift = 0
    for position, sentence in sorted(insertions):
        shifted.append(position + shift + 1)
        shift += len(sentence) + 1
    return text, shifted


def words_between(text, start, end):
    return len(WORD.findall(text[start:end]))


def echo_case(index, clean):
    pool = [('noun', pair) for pair in NOUNS] + [('verb', pair) for pair in VERBS]
    kind, (first_form, second_form) = next(
        (kind, pair) for offset in range(len(pool))
        for kind, pair in [pool[(index + offset) % len(pool)]] if absent(clean, pair))
    style = index % 3
    if kind == 'noun':
        first = ['It was the {}.', '“It was the {}—”', 'It was the {}…'][style].format(first_form)
        second = ['There were {}.', '“{}…”', 'There were {}!'][style].format(
            second_form.capitalize() if style == 1 else second_form)
    else:
        first = ['He {}.', '“He {}—”', 'He {}…'][style].format(first_form)
        second = ['They were {}.', '“{}…”', 'They were {}!'][style].format(
            second_form.capitalize() if style == 1 else second_form)
    far = index % 3 == 0
    points = boundaries(clean)
    pairs = []
    for position, anchor in enumerate(points):
        candidates = [b for b in points[position + 1:] + [len(clean)]
                      if (words_between(clean, anchor, b) >= FAR if far
                          else NEAR[0] <= words_between(clean, anchor, b) <= NEAR[1])]
        if candidates:
            pairs.append((anchor, candidates[0] if far else candidates[-1]))
    if not pairs:
        return None
    anchor, later = pairs[0]
    text, (first_at, second_at) = insert(clean, [(anchor, first), (later, second)])
    spans = []
    for at, sentence, form in ((first_at, first, first_form), (second_at, second, second_form)):
        start = text.index(form if sentence.find(form) >= 0 else form.capitalize(), at)
        if text[start:start + len(form)].lower() != form:
            raise ValueError('injection offset mismatch')
        spans.append(offsets(text, start, start + len(form)))
    gold = None if far else {'pattern': 'echo', 'report': 'echoes', 'spans': spans}
    return text, [(anchor, first), (later, second)], gold


def phrase_case(index, clean):
    points = boundaries(clean)
    prefix, singular, plural = next(
        PHRASES[(index + offset) % len(PHRASES)] for offset in range(len(PHRASES))
        if absent(clean, [*PHRASES[(index + offset) % len(PHRASES)]]))
    first_phrase = prefix + singular
    variant = index % 3
    if variant == 0:
        second_phrase = prefix + plural
    elif variant == 1:
        words = prefix.split()
        second_phrase = ' '.join(words[:2]) + ', ' + ' '.join(words[2:]) + ' ' + singular
    else:
        second_phrase = first_phrase
    first = first_phrase[0].upper() + first_phrase[1:] + '.'
    second = ('“' + second_phrase[0].upper() + second_phrase[1:] + '!”' if variant == 2
              else second_phrase[0].upper() + second_phrase[1:] + '.')
    insertions = [(points[0], first), (points[-1], second)]
    text, (first_at, second_at) = insert(clean, insertions)
    spans = []
    for at, phrase in ((first_at, first_phrase), (second_at, second_phrase)):
        start = text.index(phrase[0].upper() + phrase[1:], at)
        spans.append(offsets(text, start, start + len(phrase)))
    return text, insertions, {'pattern': 'repeated_phrase', 'report': 'repeated_phrases', 'spans': spans}


def generate(out):
    out.mkdir(parents=True, exist_ok=True)
    documents, gold, cases = [], [], []
    for book, index, clean in passages():
        base = f'{book}-{index:03}'
        documents.append({'file': base + '-clean.txt', 'source': book, 'injected': False})
        (out / (base + '-clean.txt')).write_text(clean, encoding='utf-8')
        for suffix, builder in (('echo', echo_case), ('phrase', phrase_case)):
            case = builder(index, clean)
            if case is None:
                continue
            text, insertions, answer = case
            file = f'{base}-{suffix}.txt'
            (out / file).write_text(text, encoding='utf-8')
            documents.append({'file': file, 'source': book, 'injected': True,
                              'clean': base + '-clean.txt', 'control': answer is None})
            cases.append({'file': file, 'clean': base + '-clean.txt', 'insertions': insertions})
            if answer:
                gold.append({'file': file, **answer})
    write_jsonl(out / 'documents.jsonl', documents)
    write_jsonl(out / 'answers.jsonl', gold)
    return documents, gold, cases


def char_spans(text, row):
    encoded = text.encode('utf-8')
    return tuple(sorted((len(encoded[:span['byte']['start']].decode('utf-8')),
                         len(encoded[:span['byte']['end']].decode('utf-8')))
                        for span in row['spans']))


def shift_spans(spans, insertions):
    def shift(position):
        return position + sum(len(sentence) + 1 for at, sentence in insertions if at < position)
    return tuple(sorted((shift(start), shift(end)) for start, end in spans))


def new_flags(cases, texts, predictions):
    by_file = {}
    for row in predictions:
        by_file.setdefault(row['file'], []).append(row)
    result, lost = [], {'echo': 0, 'repeated_phrase': 0}
    for case in cases:
        pattern = 'echo' if case['file'].endswith('-echo.txt') else 'repeated_phrase'
        clean = {shift_spans(char_spans(texts[case['clean']], row), case['insertions'])
                 for row in by_file.get(case['clean'], []) if row['pattern'] == pattern}
        injected = {char_spans(texts[case['file']], row): row
                    for row in by_file.get(case['file'], []) if row['pattern'] == pattern}
        result.extend(row for spans, row in injected.items() if spans not in clean)
        lost[pattern] += sum(spans not in injected for spans in clean)
    return result, lost


def run_probe(probe, tokenizer, out, documents):
    lines = ''.join(json.dumps({'file': d['file'], 'text': (out / d['file']).read_text(encoding='utf-8')},
                               ensure_ascii=False) + '\n' for d in documents)
    result = subprocess.run([str(probe), str(tokenizer)], input=lines, capture_output=True, text=True,
                            encoding='utf-8', check=True, env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def evaluate(documents, gold, cases, texts, predictions):
    flags, lost = new_flags(cases, texts, predictions)
    files = {d['file'] for d in documents}
    result = {}
    for report, pattern in (('echoes', 'echo'), ('repeated_phrases', 'repeated_phrase')):
        scored = score(gold, flags, files, report=report)['patterns'].get(
            pattern, {'precision': 1.0, 'recall': 1.0, 'f1': 1.0, 'true_positive': 0, 'predicted': 0, 'gold': 0})
        clean_flags = sum(row['pattern'] == pattern for row in predictions if row['file'].endswith('-clean.txt'))
        result[pattern] = {**scored, 'natural_flags_in_clean': clean_flags}
    result['echo']['far_controls'] = sum(d.get('control', False) for d in documents)
    result['echo']['far_control_flags'] = sum(
        row['pattern'] == 'echo' and any(d['file'] == row['file'] and d.get('control') for d in documents)
        for row in flags)
    for pattern, count in lost.items():
        result[pattern]['clean_flags_lost_after_injection'] = count
    return result


def main():
    parser = argparse.ArgumentParser(description='Tier 2 echoes and repeated-phrase suite')
    parser.add_argument('--probe', type=Path)
    parser.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    parser.add_argument('--out', type=Path, default=HERE / 'out/echoes')
    parser.add_argument('--gate', action='store_true')
    args = parser.parse_args()
    probe = args.probe
    if probe is None:
        tool = ROOT / 'bench/tools/wmb-document'
        subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml')],
                       check=True, stdout=subprocess.DEVNULL)
        probe = tool / 'target/release/wmb-echoes'
    documents, gold, cases = generate(args.out)
    texts = {d['file']: (args.out / d['file']).read_text(encoding='utf-8') for d in documents}
    predictions = run_probe(probe, args.tokenizer, args.out, documents)
    write_jsonl(args.out / 'predictions.jsonl', predictions)
    result = {'documents': len(documents), **evaluate(documents, gold, cases, texts, predictions)}
    print(json.dumps(result, indent=2))
    if args.gate:
        floors = json.loads((HERE / 'echoes-baseline.json').read_text())
        failures = [f'{pattern} {metric} {result[pattern][metric]} < {floor}'
                    for pattern, metrics in floors['floors'].items()
                    for metric, floor in metrics.items() if result[pattern][metric] < floor - 1e-12]
        if result['echo']['far_control_flags']:
            failures.append('far controls produced echo flags')
        if failures:
            raise SystemExit('echoes gate failed: ' + '; '.join(failures))


if __name__ == '__main__':
    main()
