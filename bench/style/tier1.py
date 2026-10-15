import json
import re
import tarfile

from common import HERE, fetch, prf


def ewt_documents(path):
    documents = []
    sentences = []
    for block in path.read_text(encoding='utf-8').strip().split('\n\n'):
        lines = block.splitlines()
        if any(line.startswith('# newdoc') for line in lines) and sentences:
            documents.append(sentences)
            sentences = []
        text = next(line[9:] for line in lines if line.startswith('# text = '))
        rows = [line.split('\t') for line in lines if not line.startswith('#')]
        cursor = 0
        tokens = []
        multi_end = 0
        for row in rows:
            if '.' in row[0]:
                continue
            if '-' in row[0]:
                _, multi_end = map(int, row[0].split('-'))
                start = text.find(row[1], cursor)
                if start < 0 or text[cursor:start].strip():
                    raise ValueError('EWT multiword alignment failed')
                end = start + len(row[1])
                tokens.append((start, end, None, None))
                cursor = end
                continue
            if int(row[0]) <= multi_end:
                continue
            start = text.find(row[1], cursor)
            if start < 0 or text[cursor:start].strip():
                raise ValueError('EWT surface alignment failed')
            end = start + len(row[1])
            tokens.append((start, end, row[4], row[2]))
            cursor = end
        if text[cursor:].strip():
            raise ValueError('unaccounted EWT surface text')
        sentences.append((text, tokens, sum(row[0].isdigit() for row in rows)))
    if sentences:
        documents.append(sentences)
    return documents


def score_ewt(analyzer, split):
    gold_boundaries, predicted_boundaries = set(), set()
    aligned = total = tag_any = tag_first = lemma_any = lemma_first = 0
    surface_multiword = 0
    for doc_id, sentences in enumerate(ewt_documents(fetch('ewt_' + split))):
        text = '\n'.join(sentence[0] for sentence in sentences)
        gold_tokens = {}
        offset = 0
        for sentence, tokens, count in sentences:
            total += count
            gold_boundaries.add((doc_id, offset + len(sentence.rstrip())))
            for start, end, pos, lemma in tokens:
                if pos is None:
                    surface_multiword += 1
                else:
                    gold_tokens[(start + offset, end + offset)] = (pos, lemma)
            offset += len(sentence) + 1
        for sentence in analyzer.analyze(text):
            end = sentence['span']['char']['end']
            while end and text[end - 1].isspace():
                end -= 1
            predicted_boundaries.add((doc_id, end))
            for token in sentence['tokens']:
                span = token['span']['char']
                key = (span['start'], span['end'])
                if key not in gold_tokens:
                    continue
                pos, lemma = gold_tokens.pop(key)
                readings = token['readings']
                aligned += 1
                tag_any += any(r['pos'] == pos for r in readings)
                lemma_any += any(r['lemma'] == lemma for r in readings)
                tag_first += bool(readings and readings[0]['pos'] == pos)
                lemma_first += bool(readings and readings[0]['lemma'] == lemma)
    if not aligned:
        raise ValueError('no aligned EWT tokens')
    return {'boundaries': prf(len(gold_boundaries & predicted_boundaries),
                              len(predicted_boundaries), len(gold_boundaries)),
            'gold_tokens': total, 'aligned_tokens': aligned,
            'aligned_fraction': aligned / total,
            'multiword_surface_tokens_excluded': surface_multiword,
            'tag_any': tag_any / aligned, 'tag_first': tag_first / aligned,
            'lemma_any': lemma_any / aligned, 'lemma_first': lemma_first / aligned}


def quote_spans(text):
    spans = []
    opened = None
    kind = None
    for match in re.finditer('[“”"]', text):
        char, index = match.group(), match.start()
        paragraph_start = not text[text.rfind('\n', 0, index) + 1:index].strip()
        if opened is None:
            if char != '”':
                opened, kind = index, char
        elif paragraph_start and char == kind:
            continue
        elif char == '”' or char == '"' and kind == '"':
            spans.append((opened, index + 1))
            opened = kind = None
    return spans


def score_litbank(split):
    splits = json.loads((HERE / 'splits.json').read_text())['litbank'][split]
    tp = predicted = gold_count = 0
    with tarfile.open(fetch('litbank')) as archive:
        members = {member.name.split('/quotations/tsv/')[-1]: member
                   for member in archive.getmembers() if '/quotations/tsv/' in member.name}
        for doc in splits:
            text = archive.extractfile(members[doc + '.txt']).read().decode('utf-8')
            rows = []
            offset = 0
            for line in text.splitlines(keepends=True):
                rows.append([(offset + m.start(), offset + m.end())
                             for m in re.finditer(r'\S+', line)])
                offset += len(line)
            gold = set()
            annotation = archive.extractfile(members[doc + '.ann']).read().decode('utf-8')
            for line in annotation.splitlines():
                fields = line.split('\t')
                if fields[0] == 'QUOTE':
                    start_sentence, start_token, end_sentence, end_token = map(int, fields[2:6])
                    span = (rows[start_sentence][start_token][0], rows[end_sentence][end_token][1])
                    expected = ' '.join(text[span[0]:span[1]].split())
                    if expected != ' '.join(fields[6].split()):
                        raise ValueError('LitBank annotation alignment failed')
                    gold.add(span)
            detected = set(quote_spans(text))
            tp += len(detected & gold)
            predicted += len(detected)
            gold_count += len(gold)
    return {'documents': len(splits), **prf(tp, predicted, gold_count)}


def run(analyzer, split, gate=False):
    result = {'split': split, 'ewt': score_ewt(analyzer, split),
              'litbank': score_litbank(split)}
    print(json.dumps(result, indent=2))
    if gate:
        if split != 'test':
            raise ValueError('only test has a regression floor')
        baseline = json.loads((HERE / 'tier1-baseline.json').read_text())
        for corpus, metrics in baseline.items():
            for name, floor in metrics.items():
                actual = result[corpus]
                for key in name.split('.'):
                    actual = actual[key]
                if actual + 1e-12 < floor:
                    raise ValueError('regression: ' + corpus + '.' + name)
    return result
