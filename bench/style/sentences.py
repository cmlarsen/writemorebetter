"""Tier 2 product probe for #244; public-domain inputs only."""
import json
from pathlib import Path
import re
import subprocess
import tarfile
import xml.etree.ElementTree as ET

from common import ROOT, Analyzer, SentenceProbe, fetch, jsonl, offsets, write_jsonl
from equivalence import PENN, normalize_pos, start_candidates
from tier2 import score

LABELS = ROOT / 'bench/style/labels/sentence-starts.jsonl'
FLOORS = ROOT / 'bench/style/sentence-floors.json'


def map_agreement(probe):
    tags = sorted(set(PENN) | {'NN:UN', 'PRP_SUB', 'PRP$_REF', 'RB_SENT', 'PCT', 'UNKNOWN', 'ORD', '_apostrophe_contraction_' })
    result = subprocess.run([str(probe.executable), '--penn-map'], input=json.dumps(tags),
                            capture_output=True, text=True, check=True)
    actual = json.loads(result.stdout)
    expected = {tag: PENN.get(normalize_pos(tag), '.' if tag == 'PCT' else None) for tag in tags}
    if actual != expected:
        raise ValueError('Rust/Python Penn map disagreement')


def opening_agreement(probe, analyzer):
    for text in ['“—and then he left.”', '…and then he left.', '(She ran.)', '*She* ran.', 'She ran.']:
        analysis = analyzer.analyze(text)
        starts = probe.analyze(text)['starts']['all']['starts']
        if len(analysis) != len(starts):
            raise ValueError('start convention sentence count mismatch')
        for sentence, start in zip(analysis, starts):
            if start_candidates(sentence['tokens'], skip_punctuation=True) != set(start['universalCandidates']):
                raise ValueError('Rust/Python universal opening convention disagreement')


def verify_labels():
    rows = jsonl(LABELS)
    if len(rows) < 300 or len({r['file'] for r in rows}) != len(rows):
        raise ValueError('insufficient or duplicate labels')
    for book in ('blue_castle', 'ackroyd'):
        chapters = {}
        with tarfile.open(fetch(book)) as archive:
            for member in archive.getmembers():
                if '/src/epub/text/chapter-' in member.name and member.name.endswith('.xhtml'):
                    root = ET.fromstring(archive.extractfile(member).read())
                    chapters[Path(member.name).stem] = [''.join(p.itertext()).strip()
                        for p in root.iter('{http://www.w3.org/1999/xhtml}p')]
        ordered = sorted(chapters, key=lambda name: int(name.removeprefix('chapter-')))
        prefix = '\n\n'.join(p for name in ordered[:4] for p in chapters[name])
        for row in rows:
            if row['source'] != book:
                continue
            original = chapters[row['chapter']][row['paragraph']] if 'chapter' in row else prefix
            span = row['sourceByte']
            if original.encode('utf-8')[span['start']:span['end']].decode('utf-8') != row['text']:
                raise ValueError('label source offset mismatch')
            if offsets(row['text'], 0, len(row['text'])) != {key: row[key] for key in ('byte', 'utf16')}:
                raise ValueError('label local offset mismatch')


def labeled(probe, path=LABELS):
    rows = jsonl(path)
    if path != LABELS:
        for row in rows:
            original = (ROOT / 'bench/public' / row['source']).read_bytes()
            span = row['sourceByte']
            if original[span['start']:span['end']].decode() != row['text']:
                raise ValueError('modern label source offset mismatch')

    gold, predictions = [], []
    for row in rows:
        gold.append({key: row[key] for key in ('file', 'byte', 'utf16', 'pattern')})
        report = probe.analyze(row['text'])
        for start in report['starts']['all']['starts']:
            predictions.append({'file': row['file'], 'pattern': start['category'],
                                'byte': start['span']['byte'], 'utf16': start['span']['utf16']})
    return {'sentences': len(rows), **score(gold, predictions, {r['file'] for r in rows})}


def adversarial(probe):
    cases = json.loads((ROOT / 'style/tests/fixtures/sentence-start-review.json').read_text())
    correct = 0
    for case in cases:
        result = probe.analyze(case['text'])['starts']['all']['starts']
        correct += bool(result and result[0]['category'] == case['category'])
    if correct != len(cases):
        raise ValueError('reviewed opening-function fixture regressed')
    return {'correct': correct, 'cases': len(cases)}


def reference_patterns(file, text, sentences):
    # Independent interval enumeration over analysis tokens, not report predictions.
    lengths = [sum(any(c.isalnum() for c in t['text']) for t in s['tokens']) for s in sentences]
    starts = [next((t['text'].lower() for t in s['tokens'] if not t['is_whitespace']), '') for s in sentences]
    def span(start, end, pattern):
        return {'file': file, 'pattern': pattern,
                'byte': {'start': sentences[start]['span']['byte']['start'], 'end': sentences[end-1]['span']['byte']['end']},
                'utf16': {'start': sentences[start]['span']['utf16']['start'], 'end': sentences[end-1]['span']['utf16']['end']}}
    result = [span(i, i+1, 'longSentence') for i, n in enumerate(lengths) if n >= 40]
    windows = [(a,b) for a in range(len(lengths)) for b in range(a+4,len(lengths)+1)
               if max(lengths[a:b]) - min(lengths[a:b]) <= 2]
    result.extend(span(a,b,'monotony') for a,b in windows if not any(c <= a and b <= d and (a,b) != (c,d) for c,d in windows))
    start = 0
    while start < len(starts):
        end = start + 1
        while end < len(starts) and starts[end] == starts[start]:
            end += 1
        if end - start >= 3:
            result.append(span(start,end,'sameWord'))
        start = end
    return result


def injected(probe, out, analyzer):
    rows = jsonl(LABELS)
    selected = []
    for source in ('blue_castle', 'ackroyd'):
        candidates = [r for r in rows if r['source'] == source and not 'curated' in r['file']
                      and re.fullmatch(r"[A-Za-z][A-Za-z ,’'\-]+[.!?]\s*", r['text'])
                      and 5 <= len(r['text'].split()) <= 20]
        selected.extend(candidates[:20])
    if len(selected) < 20:
        raise ValueError('insufficient public source controls')
    gold, predictions, documents = [], [], []
    for i, row in enumerate(selected):
        clean = row['text'].strip()
        phrase = clean[:-1].replace(',', '')
        for pattern, text in [('monotony', ' '.join([clean] * 4)),
                              ('sameWord', ' '.join([clean] * 3)),
                              ('longSentence', ' '.join([phrase] * 12) + '.')]:
            file = f'{i:03}-{pattern}'
            documents.append({'file': file, 'source': row['file'], 'text': text})
            if pattern != 'control':
                gold.append({'file': file, 'pattern': pattern, **offsets(text, 0, len(text))})
            report = probe.analyze(text)
            spans = {'monotony': report['length']['all']['monotonyRuns'],
                     'sameWord': report['starts']['all']['sameWordRuns'],
                     'longSentence': [r['span'] for r in report['length']['all']['longSentences']]}
            # Each suite scores its target plus every untouched control.
            for target in ('monotony', 'sameWord', 'longSentence'):
                if pattern not in ('control', target):
                    continue
                predictions.extend({'file': file, 'pattern': target,
                                    'byte': s['byte'], 'utf16': s['utf16']} for s in spans[target])
    natural = {'monotony': 0, 'sameWord': 0, 'longSentence': 0}
    controls = 0
    for source in ('blue_castle', 'ackroyd'):
        count = 0
        with tarfile.open(fetch(source)) as archive:
            chapters = sorted([m for m in archive.getmembers() if '/src/epub/text/chapter-' in m.name and m.name.endswith('.xhtml')], key=lambda m: int(Path(m.name).stem.removeprefix('chapter-')))
            for chapter in chapters:
                root = ET.fromstring(archive.extractfile(chapter).read())
                for paragraph in root.iter('{http://www.w3.org/1999/xhtml}p'):
                    text = ''.join(paragraph.itertext()).strip()
                    if any(q in text for q in '\"“”‘’') or not 30 <= len(text.split()) <= 300:
                        continue
                    sentences = analyzer.analyze(text)
                    if len(sentences) < 4:
                        continue
                    file = f'{source}-natural-{count:03}'
                    documents.append({'file': file, 'source': chapter.name, 'text': text})
                    expected = reference_patterns(file, text, sentences)
                    gold.extend(expected)
                    for item in expected:
                        natural[item['pattern']] += 1
                    result = probe.analyze(text)
                    predicted = {'monotony': result['length']['all']['monotonyRuns'],
                                 'sameWord': result['starts']['all']['sameWordRuns'],
                                 'longSentence': [r['span'] for r in result['length']['all']['longSentences']]}
                    for target, spans in predicted.items():
                        predictions.extend({'file': file, 'pattern': target, 'byte': s['byte'], 'utf16': s['utf16']} for s in spans)
                    count += 1
                    if count == 20:
                        break
                if count == 20:
                    break
        if count != 20:
            raise ValueError('insufficient natural multi-sentence controls')
        controls += count
    out.mkdir(parents=True, exist_ok=True)
    write_jsonl(out / 'documents.jsonl', documents)
    write_jsonl(out / 'answers.jsonl', gold)
    write_jsonl(out / 'predictions.jsonl', predictions)
    return {'documents': len(documents), 'controls': controls, 'naturalDiagnostics': natural,
            **score(gold, predictions, {r['file'] for r in documents})}


def public_distribution(line_probe, wrapped_probe):
    from collections import Counter
    groups = {}
    for group, files, wrapped in [('stories', [f'story{i}.txt' for i in range(1, 6)], False),
                                  ('mobyDick', ['moby150k.txt'], True)]:
        counts = Counter()
        inputs = []
        shapes = Counter()
        probe = wrapped_probe if wrapped else line_probe
        for file in files:
            data = (ROOT / 'bench/public' / file).read_bytes()
            text = data.decode('utf-8')
            cut = None
            if group == 'mobyDick':
                headings = list(re.finditer(r'(?m)^CHAPTER 1\. Loomings\.\r?$', text))
                if len(headings) != 2:
                    raise ValueError('expected contents and narrative Loomings headings')
                byte_start = len(text[:headings[1].start()].encode('utf-8'))
                cut = {'marker': 'CHAPTER 1. Loomings.', 'occurrence': 2, 'sourceByteStart': byte_start}
                data = data[byte_start:]
                text = data.decode('utf-8')
            result = probe.analyze(text)
            starts = result['starts']['all']
            lines = [bool(line.strip()) for line in text.split('\n')]
            expected_paragraphs = sum(nonblank and (not wrapped or i == 0 or not lines[i - 1])
                                      for i, nonblank in enumerate(lines))
            if result['paragraphs'] != expected_paragraphs:
                raise ValueError(f'{file}: paragraph mode does not match source wrapping')
            inputs.append({'file': file, 'hardWrapped': wrapped, 'bytes': len(data),
                           'lineEndings': 'CRLF' if b'\r\n' in data else 'LF',
                           'paragraphs': result['paragraphs'], 'sentences': starts['sentenceCount'], 'cut': cut})
            counts.update({category: int(value['count']) for category, value in starts['categories'].items()})
            if group == 'mobyDick':
                for start in starts['starts']:
                    if start['category'] != 'other':
                        continue
                    span = start['span']['byte']
                    excerpt = data[span['start']:span['end']].decode('utf-8').lstrip()
                    if re.match(r'CHAPTER\s+\d', excerpt):
                        shape = 'chapterNumberHeading'
                    elif start['word'] is None:
                        shape = 'punctuationOrMarkupLed'
                    elif start['word'] in ('the', 'a', 'an'):
                        shape = 'articleLed'
                    elif start['word'] in ('call', 'look', 'let', 'go', 'tell', 'take', 'see', 'hear', 'come', 'get', 'give', 'read', 'say'):
                        shape = 'commandLikeOpening'
                    elif re.match(r'^[“‘\"\']*[A-Za-z][A-Za-z’\'-]*[,!?—:]', excerpt):
                        shape = 'vocativeOrInterjectionShape'
                    elif len(excerpt.split()) <= 5:
                        shape = 'shortFragmentShape'
                    elif 'VERB' in start['universalCandidates']:
                        shape = 'remainingVerbTaggedOpening'
                    else:
                        shape = 'remainingOpenings'
                    shapes[shape] += 1
        total = sum(counts.values())
        groups[group] = {'sentences': total, 'inputs': inputs, 'categories': {
            category: {'count': count, 'percent': 100 * count / total}
            for category, count in sorted(counts.items())}}
        if group == 'mobyDick':
            groups[group]['otherOpeningShapes'] = dict(shapes)
    return groups


def check_public_distribution(distributions, ceilings):
    for group, ceiling in ceilings.items():
        if distributions[group]['categories']['other']['percent'] > ceiling:
            raise ValueError(f'{group}/other share exceeds {ceiling}% sanity ceiling')


def run(args):
    probe = SentenceProbe(args.tokenizer, args.document_probe)
    analyzer = Analyzer(args.tokenizer)
    public_probe = SentenceProbe(args.tokenizer, args.document_probe, hard_wrapped=True)
    try:
        verify_labels()
        map_agreement(probe)
        opening_agreement(probe, analyzer)
        result = {'labelStatus': 'two labelers; coordinator adjudicated; pre-adjudication kappa 0.873', 'labeled': labeled(probe), 'modernLabelStatus': 'single-labeler diagnostic; not part of adjudicated floors', 'modernLabeled': labeled(probe, LABELS.with_name('sentence-starts-modern.jsonl')), 'adversarial': adversarial(probe), 'injected': injected(probe, args.out, analyzer), 'publicDistribution': public_distribution(probe, public_probe)}
        if args.gate:
            floors = json.loads(FLOORS.read_text())
            check_public_distribution(result['publicDistribution'], floors['publicOtherPercentCeilings'])
            for suite in ('labeled', 'injected'):
                for category, floor in floors[suite].items():
                    if result[suite]['patterns'].get(category, {}).get('f1', 0) + 1e-12 < floor:
                        raise ValueError(f'{suite}/{category} F1 regressed')
        if args.output:
            args.output.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result, indent=2))
    finally:
        probe.close()
        analyzer.close()
        public_probe.close()
