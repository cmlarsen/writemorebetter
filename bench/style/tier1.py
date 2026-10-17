from collections import Counter
import json
import re
import tarfile
from xml.etree import ElementTree

from common import DocumentProbe, HERE, fetch, offsets, prf
from tier2 import score
from equivalence import (EXCLUDED_XPOS, PENN, QUOTATION_MARKS, TAGS,
                         add_start, lexical_conventions, normalize_lemma,
                         normalize_pos, shares, start_candidates)


def ewt_paragraphs(path):
    paragraphs, sentences = [], []
    for block in path.read_text(encoding='utf-8').strip().split('\n\n'):
        lines = block.splitlines()
        if any(line.startswith(('# newdoc', '# newpar')) for line in lines) and sentences:
            paragraphs.append(sentences)
            sentences = []
        text = next(line[9:] for line in lines if line.startswith('# text = '))
        rows = [line.split('\t') for line in lines if not line.startswith('#')]
        components = {int(row[0]): row for row in rows if row[0].isdigit()}
        cursor, multi_end, excluded_components, excluded_surfaces = 0, 0, 0, 0
        tokens = []

        def append(row, start):
            tokens.append({'start': start, 'end': start + len(row[1]), 'form': row[1],
                           'pos': row[4], 'lemma': row[2], 'upos': row[3]})

        for row in rows:
            if '.' in row[0]:
                continue
            if '-' in row[0]:
                first, multi_end = map(int, row[0].split('-'))
                start = text.find(row[1], cursor)
                if start < 0 or text[cursor:start].strip():
                    raise ValueError('EWT multiword alignment failed')
                parts = [components[index] for index in range(first, multi_end + 1)]
                if ''.join(part[1] for part in parts) == row[1]:
                    inner = start
                    for part in parts:
                        append(part, inner)
                        inner += len(part[1])
                else:
                    excluded_components += len(parts)
                    excluded_surfaces += 1
                cursor = start + len(row[1])
                continue
            if int(row[0]) <= multi_end:
                continue
            start = text.find(row[1], cursor)
            if start < 0 or text[cursor:start].strip():
                raise ValueError('EWT surface alignment failed')
            append(row, start)
            cursor = start + len(row[1])
        if text[cursor:].strip():
            raise ValueError('unaccounted EWT surface text')
        sentences.append({'text': text, 'tokens': tokens, 'gold_count': len(components),
                          'excluded_components': excluded_components,
                          'excluded_surfaces': excluded_surfaces})
    if sentences:
        paragraphs.append(sentences)
    return paragraphs


def start_share_check(analyzer, paragraphs):
    gold, resolved, fractional = Counter(), Counter(), Counter()
    resolved_by_gold, total_by_gold = Counter(), Counter()
    known = 0
    for paragraph in paragraphs:
        for sentence in paragraph:
            tokens = list(sentence['tokens'])
            while tokens and all(ch in QUOTATION_MARKS for ch in tokens[0]['form']):
                tokens.pop(0)
            if not tokens:
                continue
            tag = PENN.get(tokens[0]['pos'], 'X')
            gold[tag] += 1
            total_by_gold[tag] += 1
            analysis = analyzer.analyze(sentence['text'])
            candidates = start_candidates(analysis[0]['tokens']) if analysis else set()
            add_start(resolved, fractional, candidates)
            known += bool(candidates)
            if len(candidates) == 1:
                resolved_by_gold[tag] += 1
    gold_shares, resolved_shares, fractional_shares = map(shares, (gold, resolved, fractional))
    per_tag = {}
    for tag in TAGS:
        difference = fractional_shares[tag] - gold_shares[tag]
        per_tag[tag] = {'gold_percent': gold_shares[tag],
                        'resolved_percent': resolved_shares[tag],
                        'fractional_percent': fractional_shares[tag],
                        'resolved_bias_pp': resolved_shares[tag] - gold_shares[tag],
                        'fractional_bias_pp': difference,
                        'absolute_fractional_bias_pp': abs(difference),
                        'resolved_fraction_within_gold': resolved_by_gold[tag] / total_by_gold[tag]
                        if total_by_gold[tag] else None}
    return {'sentences': sum(gold.values()), 'with_candidates': known,
            'resolved': sum(resolved.values()), 'unknown': sum(gold.values()) - known,
            'ambiguous': known - sum(resolved.values()),
            'total_absolute_bias_pp': sum(v['absolute_fractional_bias_pp'] for v in per_tag.values()),
            'per_tag': per_tag}


def score_ewt(analyzer, split):
    gold_boundaries, predicted_boundaries, terminal_boundaries = set(), set(), set()
    final_gold, final_predicted = set(), set()
    counts, lexical_pairs = Counter(), Counter()
    paragraphs = ewt_paragraphs(fetch('ewt_' + split))
    for paragraph_id, sentences in enumerate(paragraphs):
        text = ' '.join(sentence['text'] for sentence in sentences)
        gold_tokens = {}
        offset = 0
        for index, sentence in enumerate(sentences):
            counts['gold_tokens'] += sentence['gold_count']
            counts['excluded_mwt_components'] += sentence['excluded_components']
            counts['excluded_mwt_surfaces'] += sentence['excluded_surfaces']
            boundary = (paragraph_id, offset + len(sentence['text'].rstrip()))
            if index == len(sentences) - 1:
                final_gold.add(boundary)
            else:
                gold_boundaries.add(boundary)
                if re.search(r'[.!?]["\'”’»›]*$', sentence['text'].rstrip()):
                    terminal_boundaries.add(boundary)
            for token in sentence['tokens']:
                gold_tokens[(token['start'] + offset, token['end'] + offset)] = token
            offset += len(sentence['text']) + 1
        for sentence in analyzer.analyze(text):
            end = sentence['span']['char']['end']
            while end and text[end - 1].isspace():
                end -= 1
            boundary = (paragraph_id, end)
            if end == len(text.rstrip()):
                final_predicted.add(boundary)
            else:
                predicted_boundaries.add(boundary)
            for token in sentence['tokens']:
                span = token['span']['char']
                key = (span['start'], span['end'])
                if key not in gold_tokens:
                    continue
                gold = gold_tokens.pop(key)
                readings = token['readings']
                counts['aligned_tokens'] += 1
                counts['raw_tag_any'] += any(r['pos'] == gold['pos'] for r in readings)
                counts['raw_lemma_any'] += any(r['lemma'] == gold['lemma'] for r in readings)
                counts['raw_tag_first'] += bool(readings and readings[0]['pos'] == gold['pos'])
                counts['raw_lemma_first'] += bool(readings and readings[0]['lemma'] == gold['lemma'])
                if gold['upos'] in ('PUNCT', 'SYM'):
                    counts['punctuation_tokens'] += 1
                    agreement = token['is_punctuation'] == (gold['upos'] == 'PUNCT')
                    counts['punctuation_agreement'] += agreement
                    counts[gold['upos'] + '_tokens'] += 1
                    counts[gold['upos'] + '_agreement'] += agreement
                    continue
                counts['non_punctuation_tokens'] += 1
                counts['no_reading_tokens'] += not readings
                if gold['pos'] in EXCLUDED_XPOS:
                    counts['excluded_xpos_tokens'] += 1
                    continue
                counts['lexical_tokens'] += 1
                counts['lexical_tokens_with_readings'] += bool(readings)
                tags = {normalize_pos(r['pos'], token['text']) for r in readings}
                correct = gold['pos'] in tags
                counts['normalized_tag_any'] += correct
                counts['single_normalized_correct'] += correct and len(tags) == 1
                counts['distinct_normalized_tags'] += len(tags)
                conventions = lexical_conventions(gold, tags) if not correct else []
                lexical_pairs.update(conventions)
                counts['with_lexical_conventions'] += correct or bool(conventions)
                counts['normalized_lemma_any'] += any(normalize_lemma(r['lemma']) == normalize_lemma(gold['lemma'])
                                                       for r in readings)
    aligned, lexical = counts['aligned_tokens'], counts['lexical_tokens']
    if not aligned or not lexical:
        raise ValueError('no aligned EWT tokens')
    return {'boundaries': prf(len(gold_boundaries & predicted_boundaries),
                              len(predicted_boundaries), len(gold_boundaries)),
            'paragraph_final_boundaries': prf(len(final_gold & final_predicted), len(final_predicted), len(final_gold)),
            'terminal_boundary_recall': len(terminal_boundaries & predicted_boundaries) / len(terminal_boundaries),
            'terminal_boundaries': len(terminal_boundaries),
            **{key: counts[key] for key in ('gold_tokens', 'aligned_tokens', 'excluded_mwt_components',
                                           'excluded_mwt_surfaces', 'excluded_xpos_tokens', 'lexical_tokens',
                                           'punctuation_tokens', 'no_reading_tokens', 'non_punctuation_tokens')},
            'aligned_fraction': aligned / counts['gold_tokens'],
            **{key: counts[key] / aligned for key in ('raw_tag_any', 'raw_tag_first', 'raw_lemma_any', 'raw_lemma_first')},
            **{key: counts[key] / lexical for key in ('normalized_tag_any', 'single_normalized_correct',
                                                     'normalized_lemma_any', 'with_lexical_conventions')},
            'mean_distinct_normalized_tags': counts['distinct_normalized_tags'] / lexical,
            'lexical_tokens_with_readings': counts['lexical_tokens_with_readings'],
            'mean_distinct_normalized_tags_known': counts['distinct_normalized_tags'] / counts['lexical_tokens_with_readings'],
            'lexical_convention_additional': dict(lexical_pairs),
            'no_reading_share': counts['no_reading_tokens'] / counts['non_punctuation_tokens'],
            'punctuation_flag_accuracy': counts['punctuation_agreement'] / counts['punctuation_tokens'],
            'punctuation_by_upos': {tag: {'tokens': counts[tag + '_tokens'],
                                         'accuracy': counts[tag + '_agreement'] / counts[tag + '_tokens']
                                         if counts[tag + '_tokens'] else None} for tag in ('PUNCT', 'SYM')},
            'sentence_starts': start_share_check(analyzer, paragraphs)}


def quote_spans(text):
    spans = []
    opened = kind = None
    for match in re.finditer('[“”"]', text):
        char, index = match.group(), match.start()
        paragraph_start = re.search(r'(?:\A|\n[ \t]*\n)[ \t]*$', text[:index]) is not None
        if opened is None:
            if char != '”':
                opened, kind = index, char
        elif paragraph_start and char == kind:
            continue
        elif char == '”' or char == '"' and kind == '"':
            spans.append((opened, index + 1))
            opened = kind = None
    return spans


def align_litbank(tokenized, original):
    rows, cursor = [], 0
    for line in tokenized.splitlines():
        tokens = []
        for token in line.split():
            start = original.find(token, cursor)
            if start < 0 or original[cursor:start].strip():
                raise ValueError('LitBank original alignment failed')
            cursor = start + len(token)
            tokens.append((start, cursor))
        rows.append(tokens)
    return rows


def score_litbank(split, document_probe=None):
    splits = json.loads((HERE / 'splits.json').read_text())['litbank'][split]
    documents, gold_records = [], []
    with tarfile.open(fetch('litbank')) as archive:
        files = {m.name.split('/', 1)[1]: archive.extractfile(m).read().decode('utf-8')
                 for m in archive.getmembers() if m.isfile() and
                 ('/quotations/tsv/' in m.name or '/original/' in m.name)}
    for doc in splits:
        tokenized = files['quotations/tsv/' + doc + '.txt']
        original = files['original/' + doc.removesuffix('_brat') + '.txt']
        rows = align_litbank(tokenized, original)
        original = original[:next(row[-1][1] for row in reversed(rows) if row)]
        token_rows = [line.split() for line in tokenized.splitlines()]
        gold = set()
        annotation = files['quotations/tsv/' + doc + '.ann']
        for line in annotation.splitlines():
            fields = line.split('\t')
            if fields[0] == 'QUOTE':
                ss, st, es, et = map(int, fields[2:6])
                gold.add((rows[ss][st][0], rows[es][et][1]))
                annotated = []
                for sentence_id in range(ss, es + 1):
                    words = token_rows[sentence_id]
                    annotated.extend(words[st if sentence_id == ss else 0: et + 1 if sentence_id == es else len(words)])
                if ' '.join(annotated) != ' '.join(fields[6].split()):
                    raise ValueError('LitBank annotation alignment failed')
        documents.append({'file': doc, 'text': original})
        gold_records.extend({'file': doc, **offsets(original, start, end), 'pattern': 'dialogue'}
                            for start, end in sorted(gold))
    predictions = (document_probe or DocumentProbe()).dialogue(documents)
    if any(row.get('pattern') != 'dialogue' for row in predictions):
        raise ValueError('unexpected document probe pattern')
    result = score(gold_records, predictions, set(splits), pattern='dialogue')
    return {'documents': len(splits), **result['patterns']['dialogue']}


def quoteli3_chapters(path):
    parts, quotes, length = [], [], 0

    def walk(element):
        nonlocal length
        start = length
        if element.text:
            parts.append(element.text)
            length += len(element.text)
        for child in element:
            walk(child)
            if child.tail:
                parts.append(child.tail)
                length += len(child.tail)
        if element.tag == 'quote':
            quotes.append((start, length))

    walk(ElementTree.parse(path).getroot().find('text'))
    text = ''.join(parts)
    headings = list(re.finditer(r'CHAPTER [IVXLC]+ \((\d+)\)', text))
    if not headings or any(start < headings[0].start() for start, _ in quotes):
        raise ValueError('QuoteLi3 quote outside a chapter')
    chapters = {}
    for heading, following in zip(headings, headings[1:] + [None]):
        start, end = heading.start(), following.start() if following else len(text)
        spans = [(a - start, b - start) for a, b in quotes if start <= a < end]
        if any(b > end - start for _, b in spans):
            raise ValueError('QuoteLi3 quote crosses a chapter')
        chapters['pp-%02d' % int(heading.group(1))] = (text[start:end], spans)
    if len(chapters) != len(headings):
        raise ValueError('duplicate QuoteLi3 chapter')
    return chapters


def score_quoteli3(split, document_probe=None):
    splits = json.loads((HERE / 'splits.json').read_text())['quoteli3'][split]
    chapters = {}
    for key in ('quoteli3_pp_full', 'quoteli3_pp_test'):
        parsed = quoteli3_chapters(fetch(key))
        if chapters.keys() & parsed.keys():
            raise ValueError('duplicate QuoteLi3 chapter')
        chapters.update(parsed)
    documents, gold_records = [], []
    for chapter in splits:
        text, spans = chapters[chapter]
        documents.append({'file': chapter, 'text': text})
        gold_records.extend({'file': chapter, **offsets(text, start, end), 'pattern': 'dialogue'}
                            for start, end in spans)
    predictions = (document_probe or DocumentProbe()).dialogue(documents)
    if any(row.get('pattern') != 'dialogue' for row in predictions):
        raise ValueError('unexpected document probe pattern')
    result = score(gold_records, predictions, set(splits), pattern='dialogue')
    return {'documents': len(splits), **result['patterns']['dialogue']}


def run(analyzer, split, gate=False, document_probe=None):
    result = {'split': split, 'ewt': score_ewt(analyzer, split),
              'litbank': score_litbank(split, document_probe),
              'quoteli3': score_quoteli3(split, document_probe)}
    print(json.dumps(result, indent=2))
    if gate:
        if split != 'test':
            raise ValueError('only test has a regression floor')
        baseline = json.loads((HERE / 'tier1-baseline.json').read_text())
        for direction, metrics in baseline.items():
            for name, threshold in metrics.items():
                actual = result
                for key in name.split('/'):
                    actual = actual[key]
                if direction == 'minimum' and actual + 1e-12 < threshold or direction == 'maximum' and actual - 1e-12 > threshold:
                    raise ValueError('regression: ' + name)
    return result
