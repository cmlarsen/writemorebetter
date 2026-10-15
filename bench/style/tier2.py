from collections import Counter, defaultdict
import json
from pathlib import Path
import re
import tarfile
import xml.etree.ElementTree as ET

from common import fetch, jsonl, offsets, prf, write_jsonl

WORD = re.compile(r'[^\W\d_]+(?:[’\'][^\W\d_]+)*', re.UNICODE)


def toy_detector(file, text):
    words = list(WORD.finditer(text))
    result = []
    for left, right in zip(words, words[1:]):
        if (left.group().casefold() == right.group().casefold()
                and text[left.end():right.start()].isspace()):
            result.append({'file': file, 'pattern': 'repeated_word',
                           **offsets(text, left.start(), right.end())})
    return result


def public_passages():
    for book in ('pride', 'moby'):
        count = source_words = 0
        with tarfile.open(fetch(book)) as archive:
            for member in sorted(archive.getmembers(), key=lambda m: m.name):
                if '/src/epub/text/chapter-' not in member.name or not member.name.endswith('.xhtml'):
                    continue
                root = ET.fromstring(archive.extractfile(member).read())
                for paragraph in root.iter('{http://www.w3.org/1999/xhtml}p'):
                    text = ''.join(paragraph.itertext()).strip()
                    word_count = len(list(WORD.finditer(text)))
                    source_words += word_count
                    if source_words - word_count < 2500:
                        continue
                    if word_count >= 20 and not toy_detector('', text):
                        yield book, count, text
                        count += 1
                        if count == 60:
                            break
                if count == 60:
                    break
        if count != 60:
            raise ValueError('not enough public source passages')


def generate(out):
    out.mkdir(parents=True, exist_ok=True)
    answer = []
    documents = []
    for book, index, clean in public_passages():
        text = clean
        file = f'{book}-{index:03}.txt'
        if index % 3:
            word = list(WORD.finditer(clean))[7]
            insertion = ' ' + word.group()
            text = clean[:word.end()] + insertion + clean[word.end():]
            answer.append({'file': file, 'pattern': 'repeated_word',
                           **offsets(text, word.start(), word.end() + len(insertion))})
        (out / file).write_text(text, encoding='utf-8')
        documents.append({'file': file, 'source': book, 'injected': bool(index % 3)})
    write_jsonl(out / 'answers.jsonl', answer)
    write_jsonl(out / 'documents.jsonl', documents)
    return answer, documents


def identity(row):
    if not isinstance(row['file'], str) or not isinstance(row['pattern'], str) or row['pattern'] == '__none__':
        raise ValueError('file and pattern must be strings; __none__ is reserved')
    spans = row.get('spans', [row])
    if not isinstance(spans, list) or not spans:
        raise ValueError('spans must be a nonempty list')
    keys = []
    for span in spans:
        values = []
        for unit in ('byte', 'utf16'):
            start, end = span[unit]['start'], span[unit]['end']
            if type(start) is not int or type(end) is not int or not 0 <= start < end:
                raise ValueError('expected nonempty half-open span')
            values.extend((start, end))
        keys.append(tuple(values))
    if len(set(keys)) != len(keys):
        raise ValueError('duplicate occurrence span')
    return row['file'], tuple(sorted(keys))


def score(gold, predictions, files, report=None, pattern=None):
    for row in [*gold, *predictions]:
        if row['file'] not in files:
            raise ValueError('record file is absent from the manifest')
    def selected(row):
        return (report is None or row.get('report') == report) and (pattern is None or row['pattern'] == pattern)
    gold = [row for row in gold if selected(row)]
    predictions = [row for row in predictions if selected(row)]
    expected = {}
    for row in gold:
        key = identity(row)
        if key in expected:
            raise ValueError('duplicate gold span')
        expected[key] = row['pattern']
    counts = defaultdict(Counter)
    matrix = defaultdict(Counter)
    for row in gold:
        counts[row['pattern']]['gold'] += 1
    by_span = defaultdict(Counter)
    for row in predictions:
        key, label = identity(row), row['pattern']
        counts[label]['predicted'] += 1
        by_span[key][label] += 1
    for key, labels in by_span.items():
        actual = expected.get(key, '__none__')
        if actual != '__none__':
            selected = actual if labels[actual] else sorted(labels)[0]
            matrix[actual][selected] += 1
            counts[actual]['tp'] += selected == actual
            labels[selected] -= 1
        for label, count in labels.items():
            if count:
                matrix['__none__'][label] += count
    for key, label in expected.items():
        if key not in by_span:
            matrix[label]['__none__'] += 1
    return {'patterns': {label: prf(c['tp'], c['predicted'], c['gold'])
                         for label, c in sorted(counts.items())},
            'confusion': {label: dict(c) for label, c in sorted(matrix.items())}}


def run(args):
    if args.action == 'score':
        if not args.gold or not args.predictions:
            raise ValueError('score requires --gold and --predictions')
        manifest = args.manifest or args.gold.parent / 'documents.jsonl'
        files = {row['file'] for row in jsonl(manifest)}
        print(json.dumps(score(jsonl(args.gold), jsonl(args.predictions), files,
                               report=args.report, pattern=args.pattern), indent=2))
        return
    gold, documents = generate(args.out)
    if args.action == 'generate':
        print(json.dumps({'documents': len(documents), 'injections': len(gold)}))
        return
    predictions = []
    for document in documents:
        text = (args.out / document['file']).read_text(encoding='utf-8')
        predictions.extend(toy_detector(document['file'], text))
    write_jsonl(args.out / 'predictions.jsonl', predictions)
    result = score(gold, predictions, {row['file'] for row in documents})
    if result['patterns']['repeated_word']['f1'] != 1:
        raise ValueError('tier2 smoke regressed')
    print(json.dumps({'documents': len(documents),
                      'untouched_controls': sum(not d['injected'] for d in documents),
                      **result}, indent=2))
