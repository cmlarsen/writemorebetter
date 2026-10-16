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
from tier2 import WORD, identity, score

BOUNDARY = re.compile(r'(?:(?<=[.!?…])|(?<=[.!?…][”’])) (?=[“‘]?[A-Z])')
VOCABULARY = {
    'dev': {
        'nouns': [('lantern', 'lanterns'), ('thimble', 'thimbles'), ('kettle', 'kettles'),
                  ('ribbon', 'ribbons'), ('pebble', 'pebbles'), ('saddle', 'saddles'),
                  ('violin', 'violins'), ('puzzle', 'puzzles'), ('ladder', 'ladders'),
                  ('basket', 'baskets'), ('pillow', 'pillows'), ('blanket', 'blankets'),
                  ('mitten', 'mittens'), ('teacup', 'teacups'), ('umbrella', 'umbrellas'),
                  ('carpet', 'carpets')],
        'verbs': [('trembled', 'trembling'), ('whispered', 'whispering'), ('shivered', 'shivering'),
                  ('stumbled', 'stumbling'), ('wandered', 'wandering'), ('hesitated', 'hesitating'),
                  ('giggled', 'giggling'), ('frowned', 'frowning'), ('knelt', 'kneeling'),
                  ('wept', 'weeping'), ('crept', 'creeping'), ('slung', 'slinging')],
        'homographs': [('He thought.', 'thought', 'thoughts', ['think']),
                       ('They were feeling.', 'feeling', 'feelings', ['feel', 'felt']),
                       ('He wound it.', 'wound', 'wounds', ['wind'])],
        'phrases': [('the crooked brass ', 'gate', 'gates'), ('a faded velvet ', 'ribbon', 'ribbons'),
                    ('the narrow copper ', 'kettle', 'kettles'),
                    ('the chipped porcelain ', 'teacup', 'teacups'),
                    ('a battered tin ', 'lantern', 'lanterns'),
                    ('the miller’s silver ', 'thimble', 'thimbles'),
                    ('the frayed woollen ', 'blanket', 'blankets'), ('a crooked oak ', 'ladder', 'ladders')],
    },
    'test': {
        'nouns': [('trumpet', 'trumpets'), ('walnut', 'walnuts'), ('bucket', 'buckets'),
                  ('feather', 'feathers'), ('cushion', 'cushions'), ('lemon', 'lemons'),
                  ('needle', 'needles'), ('compass', 'compasses'), ('scarf', 'scarves'),
                  ('shelf', 'shelves')],
        'verbs': [('shrugged', 'shrugging'), ('blinked', 'blinking'), ('sneezed', 'sneezing'),
                  ('fidgeted', 'fidgeting'), ('swung', 'swinging'), ('dug', 'digging'),
                  ('spun', 'spinning'), ('clung', 'clinging')],
        'homographs': [('He ground it.', 'ground', 'grounds', ['grind']),
                       ('She rose.', 'rose', 'roses', ['rise']),
                       ('He saw it.', 'saw', 'saws', ['see', 'seen'])],
        'phrases': [('the dented pewter ', 'jug', 'jugs'), ('a cracked leather ', 'satchel', 'satchels'),
                    ('the rusty iron ', 'hinge', 'hinges'), ('a tattered silk ', 'scarf', 'scarves'),
                    ('the cook’s copper ', 'ladle', 'ladles'), ('a bent tin ', 'spoon', 'spoons')],
    },
}
DEV_PASSAGES = 20
ABBREVIATION = re.compile(r'\b(?:Mr|Mrs|Ms|Dr|St|Messrs|Mme|Capt|Col|Gen|Rev)\.$')
PASSAGES_PER_BOOK = 40
NEAR = (12, 25)
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


def split_of(index):
    return 'dev' if index < DEV_PASSAGES else 'test'


def function_words():
    source = (ROOT / 'style/src/reports/echoes.rs').read_text(encoding='utf-8')
    start = source.index('const FUNCTION_WORDS')
    return set(re.findall(r'"([^"]+)"', source[start:source.index('];', start)]))


def pair_positions(clean, far):
    points = boundaries(clean)
    for position, anchor in enumerate(points):
        candidates = [b for b in points[position + 1:] + [len(clean)]
                      if (words_between(clean, anchor, b) >= FAR if far
                          else NEAR[0] <= words_between(clean, anchor, b) <= NEAR[1])]
        if candidates:
            return anchor, candidates[0] if far else candidates[-1]
    return None


def locate(text, at, forms):
    spans = []
    for form in forms:
        start = text.lower().index(form, at)
        spans.append(offsets(text, start, start + len(form)))
        at = start + len(form)
    return spans


def vocabulary_words(kind, entry):
    return [entry[1], entry[2], *entry[3]] if kind == 'homograph' else list(entry)


def echo_case(index, clean):
    vocabulary = VOCABULARY[split_of(index)]
    style = index % 3
    if index % 4 == 1:
        pool = [('homograph', entry) for entry in vocabulary['homographs']]
    else:
        pool = ([('noun', entry) for entry in vocabulary['nouns']]
                + [('verb', entry) for entry in vocabulary['verbs']])
    kind, entry = next(choice for offset in range(len(pool))
                       for choice in [pool[(index + offset) % len(pool)]]
                       if absent(clean, vocabulary_words(*choice)))
    if kind == 'homograph':
        verb_sentence, form, plural, _ = entry
        first = f'{verb_sentence} It was the {form}.'
        second = ['There were {}.', '“{}…”', 'There were {}!'][style].format(
            plural.capitalize() if style == 1 else plural)
        first_forms, second_form = [form, form], plural
    else:
        first_form, second_form = entry
        frames = (['It was the {}.', '“It was the {}—”', 'It was the {}…'] if kind == 'noun'
                  else ['He {}.', '“He {}—”', 'He {}…'])
        first = frames[style].format(first_form)
        second = (['There were {}.', '“{}…”', 'There were {}!'] if kind == 'noun'
                  else ['They were {}.', '“{}…”', 'They were {}!'])[style].format(
            second_form.capitalize() if style == 1 else second_form)
        first_forms = [first_form]
    far = index % 3 == 0
    positions = pair_positions(clean, far)
    if positions is None:
        return None
    anchor, later = positions
    text, (first_at, second_at) = insert(clean, [(anchor, first), (later, second)])
    first_spans = locate(text, first_at, first_forms)
    second_span = locate(text, second_at, [second_form])[0]
    pairs = list(zip(first_spans, first_spans[1:]))
    if not far:
        pairs.append((first_spans[-1], second_span))
    gold = [{'pattern': 'echo', 'report': 'echoes', 'spans': list(pair)} for pair in pairs]
    return text, [(anchor, first), (later, second)], gold


def natural_case(index, clean, excluded):
    tokens = list(WORD.finditer(clean))
    far = index % 3 == 0
    points = boundaries(clean) + [len(clean)]
    for position, token in enumerate(tokens):
        word = token.group()
        if len(word) < 5 or not word.isalpha() or not word.islower() or word in excluded:
            continue
        family = [i for i, other in enumerate(tokens) if other.group().lower().startswith(word[:4])]
        for point in points:
            gap = words_between(clean, token.end(), point)
            if point < token.end() or not (gap >= FAR if far else NEAR[0] <= gap <= NEAR[1]):
                continue
            after = sum(1 for other in tokens if other.start() < point)
            if far:
                clear = all(not after - 75 <= i < after + 60 for i in family)
            else:
                clear = all(i == position or not position - 55 <= i < after + 60 for i in family)
            if not clear:
                continue
            sentence = [word.capitalize() + '.', '“' + word.capitalize() + '…”',
                        word.capitalize() + '!'][index % 3]
            text, (at,) = insert(clean, [(point, sentence)])
            copy = locate(text, at, [word])[0]
            gold = [] if far else [{'pattern': 'echo', 'report': 'echoes',
                                    'spans': [offsets(text, token.start(), token.end()), copy]}]
            return text, [(point, sentence)], gold
    return None


def phrase_case(index, clean):
    phrases = VOCABULARY[split_of(index)]['phrases']
    points = boundaries(clean)
    prefix, singular, plural = next(
        phrases[(index + offset) % len(phrases)] for offset in range(len(phrases))
        if absent(clean, [*phrases[(index + offset) % len(phrases)]]))
    phrase = prefix + singular
    variant = index % 5
    if variant == 0:
        phrases_at = [(points[0], phrase), (points[1], prefix + plural)]
    elif variant == 1:
        phrases_at = [(points[0], phrase), (points[-1], phrase)]
    elif variant == 2:
        phrases_at = [(point, phrase) for point in (points[0], points[len(points)//2], points[-1])]
    elif variant == 3:
        phrase = 'I got up'
        phrases_at = [(point, phrase) for point in (points[0], points[len(points)//2], points[-1])]
    else:
        phrase = 'Piglet said that'
        phrases_at = [(point, phrase) for point in (points[0], points[len(points)//2], points[-1])]
    insertions = [(at, phrase[0].upper() + phrase[1:] + '.') for at, phrase in phrases_at]
    text, positions = insert(clean, insertions)
    spans = [offsets(text, at, at + len(phrase)) for at, (_, phrase) in zip(positions, phrases_at)]
    far = variant == 1 and words_between(text, positions[0], positions[-1]) > 300
    gold = [] if far else [{'pattern': 'repeated_phrase', 'report': 'repeated_phrases', 'spans': spans}]
    return text, insertions, gold


def phrase_far_case(index, clean):
    pool = VOCABULARY[split_of(index)]['phrases']
    prefix, singular, _ = next(entry for entry in pool if absent(clean, entry))
    phrase = prefix + singular
    points = boundaries(clean)
    insertions = [(points[0], phrase.capitalize() + '.'),
                  (points[len(points)//2], 'They waited. ' * 151),
                  (points[-1], phrase.capitalize() + '.')]
    text, _ = insert(clean, insertions)
    return text, insertions, []


def generate(out):
    out.mkdir(parents=True, exist_ok=True)
    excluded = function_words()
    documents, gold, cases = [], [], []
    for book, index, clean in passages():
        base = f'{book}-{index:03}'
        split = split_of(index)
        documents.append({'file': base + '-clean.txt', 'source': book, 'split': split, 'injected': False})
        (out / (base + '-clean.txt')).write_text(clean, encoding='utf-8')
        builders = (('echo', echo_case), ('natural', lambda i, c: natural_case(i, c, excluded)),
                    ('phrase', phrase_case), ('phrase-far', phrase_far_case))
        for suffix, builder in builders:
            case = builder(index, clean)
            if case is None:
                continue
            text, insertions, answers = case
            file = f'{base}-{suffix}.txt'
            (out / file).write_text(text, encoding='utf-8')
            control = not answers
            documents.append({'file': file, 'source': book, 'split': split, 'injected': True,
                              'kind': suffix, 'clean': base + '-clean.txt', 'control': control})
            cases.append({'file': file, 'clean': base + '-clean.txt', 'insertions': insertions})
            gold.extend({'file': file, 'split': split, **answer} for answer in answers)
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
        pattern = 'repeated_phrase' if '-phrase' in case['file'] else 'echo'
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
    by_file = {d['file']: d for d in documents}
    result = {}
    for split in ('dev', 'test'):
        in_split = [row for row in flags if by_file[row['file']]['split'] == split]
        expected = [row for row in gold if row['split'] == split]
        result[split] = {}
        for report, pattern in (('echoes', 'echo'), ('repeated_phrases', 'repeated_phrase')):
            scored = score(expected, in_split, files, report=report)['patterns'].get(
                pattern, {'precision': 1.0, 'recall': 1.0, 'f1': 1.0, 'true_positive': 0,
                          'predicted': 0, 'gold': 0})
            result[split][pattern] = scored
        for kind in ('echo', 'natural', 'phrase'):
            kind_files = {d['file'] for d in documents if d['split'] == split and (d.get('kind') == kind or kind == 'phrase' and d.get('kind') == 'phrase-far')}
            kind_flags = [row for row in in_split if row['file'] in kind_files and row['pattern'] == ('repeated_phrase' if kind == 'phrase' else 'echo')]
            kind_gold = [row for row in expected if row['file'] in kind_files]
            result[split][f'{kind}_injections'] = score(kind_gold, kind_flags, files, report='repeated_phrases' if kind == 'phrase' else 'echoes')[
                'patterns'].get('repeated_phrase' if kind == 'phrase' else 'echo', {'predicted': 0, 'gold': 0})
            result[split][f'{kind}_far_controls'] = sum(d['file'] in kind_files and d['control'] for d in documents)
            expected_keys = {identity(row) for row in kind_gold}
            result[split][f'{kind}_far_control_flags'] = sum(
                by_file[row['file']]['control'] and identity(row) not in expected_keys for row in kind_flags)
    result['natural_flags_in_clean'] = {
        pattern: sum(row['pattern'] == pattern for row in predictions if row['file'].endswith('-clean.txt'))
        for pattern in ('echo', 'repeated_phrase')}
    result['clean_flags_lost_after_injection'] = lost
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
        failures = [f'{split} {pattern} {metric} {result[split][pattern][metric]} < {floor}'
                    for split, patterns in floors['floors'].items()
                    for pattern, metrics in patterns.items()
                    for metric, floor in metrics.items() if result[split][pattern][metric] < floor - 1e-12]
        failures += [f'{split} {kind} far controls produced echo flags'
                     for split in ('dev', 'test') for kind in ('echo', 'natural', 'phrase')
                     if result[split][f'{kind}_far_control_flags']]
        if failures:
            raise SystemExit('echoes gate failed: ' + '; '.join(failures))

if __name__ == '__main__':
    main()
