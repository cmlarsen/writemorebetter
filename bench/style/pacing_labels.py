"""Pacing hand-label set: item selection, labeler packets, validation, consensus and report scoring."""
import argparse
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
import hashlib
from itertools import combinations, product
import json
from pathlib import Path
import random
import re
import shutil
import sys
import tarfile
import xml.etree.ElementTree as ET

from common import HERE, ROOT, fetch, jsonl, prf, write_jsonl

LABELS = HERE / 'labels'
ITEMS = LABELS / 'pacing-items.jsonl'
STRATA = LABELS / 'pacing-strata.json'
GUIDELINES = LABELS / 'PACING-GUIDELINES.md'
GUIDELINES_VERSION = 2
LABELED_GUIDELINES = (1, LABELS / 'PACING-GUIDELINES-v1.md')
LABELER_DIR = LABELS / 'pacing'
CONSENSUS = LABELS / 'pacing-consensus.jsonl'
AGREEMENT = LABELS / 'pacing-agreement.json'
BASELINE = HERE / 'pacing-labels-baseline.json'
SWEEP_RESULTS = HERE / 'pacing-labels-sweep.json'
PUBLIC = {f'story{n}': f'bench/public/story{n}.txt' for n in range(1, 6)}
BOOKS = ('pride', 'moby', 'blue_castle', 'ackroyd', 'thirty_nine_steps', 'hound', 'age_of_innocence',
         'silas_marner', 'treasure_island', 'styles_ch1', 'gatsby_ch1', 'sign_ch1', 'avonlea_ch1')
SOURCES = tuple(PUBLIC) + BOOKS
CHAPTER = re.compile(r'^[^/]+/(src/epub/text/chapter-[\d-]+\.xhtml)$')
XHTML = '{http://www.w3.org/1999/xhtml}'
EPUB_TYPE = '{http://www.idpf.org/2007/ops}type'
SKIPPED = {'header', 'hgroup', 'footer', 'table', 'figure', 'aside'}
KINDS = ('backstory', 'exposition', 'interior', 'description', 'action', 'dialogue', 'mixed')
SEED = 251256
LOOSE = {'minimumSentenceLength': 15, 'minimumPastPerfectPercent': 10, 'minimumCognitionPercent': 10}
SWEEP = ({'minimumSentenceLength': 15, 'minimumPastPerfectPercent': 10, 'minimumCognitionPercent': 10},
         {'minimumSentenceLength': 20, 'minimumPastPerfectPercent': 10, 'minimumCognitionPercent': 10},
         {'minimumSentenceLength': 20, 'minimumPastPerfectPercent': 15, 'minimumCognitionPercent': 15},
         {},
         {'minimumSentenceLength': 25, 'minimumPastPerfectPercent': 30, 'minimumCognitionPercent': 30})
GRID = {'minimumSentenceLength': (0, 10, 12, 15, 18, 20, 25), 'minimumPastPerfectPercent': (5, 10, 15, 20, 30),
        'minimumCognitionPercent': (5, 10, 15, 20, 30), 'minimumParagraphs': (2, 3)}
OUTLIER_MARGIN = 0.1
STRATA_TARGET = {'flagged': 14, 'near_miss': 13, 'ordinary': 13}
MAX_PASSAGE_WORDS = 1800
LOADED = {}
DEFINITIONS = {
    'flagged': 'window around a slow run reported with default or loose options, with up to two context paragraphs each side',
    'near_miss': 'no run under either setting, but at least two speech-free paragraphs meeting two thresholds, '
                 'or one such paragraph plus three consecutive speech-free paragraphs meeting at least one',
    'ordinary': 'no run and no paragraph meeting two thresholds',
    'runs': 'defaultRuns and looseRuns are inclusive 1-based paragraph ranges within the passage',
}


class Document:
    def __init__(self, source, name, text, paragraphs):
        self.source, self.name, self.text, self.paragraphs = source, name, text, paragraphs

    def span(self, index):
        start, end = self.paragraphs[index]['start'], self.paragraphs[index]['end']
        return {'char': {'start': start, 'end': end},
                'byte': {'start': len(self.text[:start].encode('utf-8')), 'end': len(self.text[:end].encode('utf-8'))},
                'utf16': {'start': len(self.text[:start].encode('utf-16-le')) // 2,
                          'end': len(self.text[:end].encode('utf-16-le')) // 2}}

    def paragraph(self, index):
        return self.text[self.paragraphs[index]['start']:self.paragraphs[index]['end']]


def inline_text(element):
    parts = [element.text or '']
    for child in element:
        if 'noteref' not in child.get(EPUB_TYPE, ''):
            parts.append(inline_text(child))
        parts.append(child.tail or '')
    return ''.join(parts)


def blocks(element, quoted=False):
    for child in element:
        tag = child.tag.removeprefix(XHTML)
        if tag in SKIPPED:
            continue
        if tag == 'hr':
            yield 'hr', None, quoted
        elif tag == 'p':
            yield 'p', ' '.join(inline_text(child).split()), quoted
        else:
            yield from blocks(child, quoted or tag == 'blockquote')


def xhtml_document(source, name, data):
    text, paragraphs, scene = '', [], False
    for kind, content, quoted in blocks(ET.fromstring(data).find(XHTML + 'body')):
        if kind == 'hr':
            scene = bool(paragraphs)
            continue
        if not content:
            continue
        if paragraphs:
            text += '\n\n\n' if scene else '\n'
        paragraphs.append({'start': len(text), 'end': len(text) + len(content), 'quoted': quoted, 'scene': scene})
        text += content
        scene = False
    return Document(source, name, text, paragraphs)


def public_document(source):
    text = (ROOT / PUBLIC[source]).read_text(encoding='utf-8')
    paragraphs, start = [], 0
    for line in text.split('\n'):
        if line.strip():
            paragraphs.append({'start': start, 'end': start + len(line), 'quoted': False, 'scene': False})
        start += len(line) + 1
    return Document(source, Path(PUBLIC[source]).name, text, paragraphs)


def chapter_number(name):
    return [int(part) for part in re.findall(r'\d+', Path(name).stem)]


def documents(source):
    if source in PUBLIC:
        return [public_document(source)]
    path = fetch(source)
    if path.suffix == '.xhtml':
        return [xhtml_document(source, 'src/epub/text/chapter-1.xhtml', path.read_bytes())]
    result = []
    with tarfile.open(path) as archive:
        for member in archive.getmembers():
            if match := CHAPTER.match(member.name):
                result.append(xhtml_document(source, match.group(1), archive.extractfile(member).read()))
    return sorted(result, key=lambda document: chapter_number(document.name))


def source_documents(source):
    if source not in LOADED:
        LOADED[source] = {d.name: d for d in documents(source)}
    return LOADED[source]


def document(source, name):
    return source_documents(source)[name]


def digest(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


def row_indexes(report, doc):
    starts = {p['start']: index for index, p in enumerate(doc.paragraphs)}
    result = {}
    for number, row in enumerate(report['paragraphs']):
        index = starts.get(row['span']['char']['start'])
        if index is not None and row['span']['char']['end'] == doc.paragraphs[index]['end']:
            result[index] = number
    return result


def run_paragraphs(report, doc):
    mapped = {row: index for index, row in row_indexes(report, doc).items()}
    return [sorted(mapped[row] for row in range(run['paragraphs']['start'], run['paragraphs']['end']) if row in mapped)
            for run in report['slowRuns']]


def profile(probe, doc):
    default = probe.report(doc.text)
    loose = probe.report(doc.text, options=LOOSE)
    mapped = row_indexes(default, doc)
    result = []
    for index, paragraph in enumerate(doc.paragraphs):
        row = default['paragraphs'][mapped[index]] if index in mapped else None
        result.append({'row': row is not None, 'words': row['signals']['words']['all'] if row else 0,
                       'speech': bool(row and row['hasSpeech']),
                       'signals': len(row['triggeredSignals']) if row else 0,
                       'eligible': bool(row and not row['hasSpeech'] and row['signals']['words']['all'] > 0
                                        and len(row['triggeredSignals']) >= 2)})
    return result, run_paragraphs(default, doc), run_paragraphs(loose, doc)


def windows_ok(doc, info, start, end):
    return (start >= 0 and end <= len(doc.paragraphs)
            and all(info[i]['row'] and not doc.paragraphs[i]['quoted'] for i in range(start, end))
            and not any(doc.paragraphs[i]['scene'] for i in range(start + 1, end))
            and sum(info[i]['words'] for i in range(start, end)) <= MAX_PASSAGE_WORDS)


def candidates(doc, info, default_runs, loose_runs, rng):
    runs = [run for run in default_runs + loose_runs if run]
    flagged = {i for run in runs for i in run}
    result = []
    seen = set()
    for run in runs:
        key = (run[0], run[-1])
        if key in seen:
            continue
        seen.add(key)
        for size in range(min(10, max(6, len(run) + 4)), 5, -1):
            before = max(0, min(run[0], (size - len(run) + 1) // 2))
            start = run[0] - before
            if windows_ok(doc, info, start, start + size):
                result.append(('flagged', start, start + size))
                break
    for size in (8, 7, 6, 9, 10):
        for start in range(len(doc.paragraphs) - size + 1):
            end = start + size
            if not windows_ok(doc, info, start, end) or flagged & set(range(start, end)):
                continue
            span = info[start:end]
            eligible = sum(p['eligible'] for p in span)
            signalled = max_streak(p['signals'] >= 1 and not p['speech'] for p in span)
            if eligible >= 2 or (eligible and signalled >= 3):
                result.append(('near_miss', start, end))
            elif not eligible and all(p['signals'] < 2 for p in span):
                result.append(('ordinary', start, end))
    rng.shuffle(result)
    return result


def max_streak(values):
    best = current = 0
    for value in values:
        current = current + 1 if value else 0
        best = max(best, current)
    return best


def overlapping(runs, start, end):
    return [[max(run[0], start) - start + 1, min(run[-1], end - 1) - start + 1]
            for run in runs if run and run[0] < end and run[-1] >= start]


def select(probe, seed=SEED):
    rng = random.Random(seed)
    pools = {stratum: {} for stratum in STRATA_TARGET}
    for source in SOURCES:
        for doc in documents(source):
            info, default_runs, loose_runs = profile(probe, doc)
            for stratum, start, end in candidates(doc, info, default_runs, loose_runs, rng):
                pools[stratum].setdefault(source, []).append(
                    {'doc': doc, 'start': start, 'end': end, 'stratum': stratum,
                     'default': overlapping(default_runs, start, end), 'loose': overlapping(loose_runs, start, end)})
    for pool in pools.values():
        for source in pool:
            rng.shuffle(pool[source])
            pool[source].sort(key=lambda c: not c['default'])
    chosen, taken = [], {}
    for stratum, target in STRATA_TARGET.items():
        pool = pools[stratum]
        sources = sorted(pool)
        rng.shuffle(sources)
        count = 0
        while count < target and any(pool[s] for s in sources):
            for source in sources:
                while count < target and pool[source]:
                    candidate = pool[source].pop(0)
                    used = taken.setdefault((source, candidate['doc'].name), set())
                    if used & set(range(candidate['start'] - 1, candidate['end'] + 1)):
                        continue
                    used.update(range(candidate['start'], candidate['end']))
                    chosen.append(candidate)
                    count += 1
                    break
        if count < target:
            raise ValueError('not enough candidates for ' + stratum)
    rng.shuffle(chosen)
    items, strata = [], {}
    for number, candidate in enumerate(chosen, 1):
        passage, doc = f'p{number:02d}', candidate['doc']
        for position, index in enumerate(range(candidate['start'], candidate['end']), 1):
            items.append({'id': f'{passage}.{position}', 'passage': passage, 'paragraph': position,
                          'source': doc.source, 'document': doc.name, **doc.span(index),
                          'sha256': digest(doc.paragraph(index))})
        strata[passage] = {'stratum': candidate['stratum'], 'source': doc.source, 'document': doc.name,
                           'documentParagraphs': {'start': candidate['start'], 'end': candidate['end']},
                           'defaultRuns': candidate['default'], 'looseRuns': candidate['loose']}
    return items, {'seed': seed, 'looseOptions': LOOSE, 'maxPassageWords': MAX_PASSAGE_WORDS,
                   'definitions': DEFINITIONS, 'passages': strata}


def load_items(path=ITEMS):
    return jsonl(path)


def passages(items):
    result = {}
    for item in items:
        result.setdefault(item['passage'], []).append(item)
    return result


def materialize(items):
    result = {}
    for item in items:
        doc = document(item['source'], item['document'])
        index = next((i for i, p in enumerate(doc.paragraphs) if p['start'] == item['char']['start']), None)
        if index is None or doc.span(index) != {key: item[key] for key in ('char', 'byte', 'utf16')}:
            raise ValueError('item offsets do not match the pinned text: ' + item['id'])
        text = doc.paragraph(index)
        if digest(text) != item['sha256']:
            raise ValueError('item text does not match its sha256: ' + item['id'])
        result[item['id']] = text
    return result


def template(items):
    return [{'labeler': '', 'passage': passage,
             'paragraphs': [{'paragraph': item['paragraph'], 'slow': None, 'kind': None} for item in group],
             'stretches': [], 'notes': ''}
            for passage, group in passages(items).items()]


def packet(out, items, force=False):
    out = Path(out)
    if out.exists() and any(out.iterdir()) and not force:
        raise ValueError(f'{out} is not empty; pass --force to overwrite')
    out.mkdir(parents=True, exist_ok=True)
    texts = materialize(items)
    written = []
    for passage, group in passages(items).items():
        lines = [f'# Passage {passage}', '', f'{len(group)} paragraphs.', '']
        for item in group:
            lines += [f'[{item["paragraph"]}] {texts[item["id"]]}', '']
        path = out / f'{passage}.md'
        path.write_text('\n'.join(lines), encoding='utf-8')
        written.append(path)
    write_jsonl(out / 'labels-template.jsonl', template(items))
    shutil.copyfile(GUIDELINES, out / GUIDELINES.name)
    return written + [out / 'labels-template.jsonl', out / GUIDELINES.name]


def is_int(value):
    return isinstance(value, int) and not isinstance(value, bool)


def validate(rows, items):
    expected = {passage: len(group) for passage, group in passages(items).items()}
    errors, seen, labelers = [], set(), set()
    for line, row in enumerate(rows, 1):
        def error(message):
            errors.append(f'line {line}: {message}')
        if not isinstance(row, dict):
            error('not a JSON object')
            continue
        if extra := set(row) - {'labeler', 'passage', 'paragraphs', 'stretches', 'notes'}:
            error('unknown keys ' + ', '.join(sorted(extra)))
        if not isinstance(row.get('labeler'), str) or not row['labeler'].strip():
            error('labeler must be a nonempty string')
        else:
            labelers.add(row['labeler'])
        if not isinstance(row.get('notes', ''), str):
            error('notes must be a string')
        passage = row.get('passage')
        if passage not in expected:
            error(f'unknown passage {passage!r}')
            continue
        if passage in seen:
            error(f'duplicate passage {passage}')
        seen.add(passage)
        count, slow = expected[passage], set()
        paragraphs = row.get('paragraphs')
        if not isinstance(paragraphs, list) or len(paragraphs) != count:
            error(f'{passage}: paragraphs must list all {count} paragraphs')
            paragraphs = paragraphs if isinstance(paragraphs, list) else []
        for position, paragraph in enumerate(paragraphs, 1):
            where = f'{passage} paragraph {position}'
            if not isinstance(paragraph, dict):
                error(where + ': not an object')
                continue
            if extra := set(paragraph) - {'paragraph', 'slow', 'kind', 'note'}:
                error(where + ': unknown keys ' + ', '.join(sorted(extra)))
            if paragraph.get('paragraph') != position or not is_int(paragraph.get('paragraph')):
                error(where + f': paragraph number must be {position}')
            if not isinstance(paragraph.get('slow'), bool):
                error(where + ': slow must be true or false')
            elif paragraph['slow']:
                slow.add(position)
            if paragraph.get('kind') not in KINDS:
                error(where + ': kind must be one of ' + ', '.join(KINDS))
            if not isinstance(paragraph.get('note', ''), str):
                error(where + ': note must be a string')
        stretches = row.get('stretches')
        if not isinstance(stretches, list):
            error(f'{passage}: stretches must be a list')
            continue
        covered, previous = set(), 0
        for stretch in stretches:
            if (not isinstance(stretch, dict) or set(stretch) != {'start', 'end', 'severity'}
                    or not all(is_int(stretch[key]) for key in stretch)):
                error(f'{passage}: each stretch needs integer start, end and severity only')
                continue
            if not 1 <= stretch['start'] <= stretch['end'] <= count:
                error(f'{passage}: stretch {stretch["start"]}-{stretch["end"]} is outside paragraphs 1-{count}')
            if stretch['start'] <= previous:
                error(f'{passage}: stretches must be in order and must not overlap')
            if not 1 <= stretch['severity'] <= 5:
                error(f'{passage}: severity must be 1-5')
            previous = stretch['end']
            covered.update(range(stretch['start'], stretch['end'] + 1))
        if covered != slow:
            error(f'{passage}: stretches cover paragraphs {sorted(covered)} but slow paragraphs are {sorted(slow)}')
    if missing := sorted(set(expected) - seen):
        errors.append('missing passages: ' + ', '.join(missing))
    if len(labelers) > 1:
        errors.append('one labeler per file; found ' + ', '.join(sorted(labelers)))
    return errors


def load_labels(path, items):
    try:
        rows = jsonl(path)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f'{path}: unreadable: {error}')
    if errors := validate(rows, items):
        raise ValueError(f'{path}:\n' + '\n'.join(errors))
    return rows[0]['labeler'], {row['passage']: row for row in rows}


def fleiss(counts):
    counts = [c for c in counts if sum(c.values())]
    if not counts:
        return None
    raters = sum(counts[0].values())
    if raters < 2 or any(sum(c.values()) != raters for c in counts):
        raise ValueError('Fleiss kappa needs the same number (at least 2) of ratings per item')
    categories = {key for c in counts for key in c}
    total = len(counts) * raters
    expected = sum((sum(c.get(k, 0) for c in counts) / total) ** 2 for k in categories)
    observed = sum((sum(v * v for v in c.values()) - raters) / (raters * (raters - 1)) for c in counts) / len(counts)
    return None if expected == 1 else (observed - expected) / (1 - expected)


def cohen(first, second):
    if len(first) != len(second) or not first:
        raise ValueError('Cohen kappa needs two equal, nonempty label lists')
    observed = sum(a == b for a, b in zip(first, second)) / len(first)
    left, right = Counter(first), Counter(second)
    expected = sum(left[k] * right[k] for k in left) / len(first) ** 2
    return None if expected == 1 else (observed - expected) / (1 - expected)


def majority(votes, total):
    ranked = Counter(votes).most_common()
    if not ranked or (len(ranked) > 1 and ranked[0][1] == ranked[1][1]):
        return None
    return ranked[0][0] if total is None or ranked[0][1] * 2 > total else None


def stretches_of(flags):
    result, start = [], None
    for position, flag in enumerate(flags + [False], 1):
        if flag and start is None:
            start = position
        elif not flag and start is not None:
            result.append((start, position - 1))
            start = None
    return result


def jaccard(first, second):
    return 1.0 if not first and not second else len(first & second) / len(first | second)


def mean(values):
    values = [v for v in values if v is not None]
    return sum(values) / len(values) if values else None


def median(values):
    values = sorted(values)
    if not values:
        return None
    middle = len(values) // 2
    return values[middle] if len(values) % 2 else (values[middle - 1] + values[middle]) / 2


def file_digest(path=ITEMS):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def labeled_guidelines():
    version, path = LABELED_GUIDELINES
    return {'version': version, 'file': path.name, 'sha256': file_digest(path)}


def merge(labels, items, items_sha=None, guidelines=None):
    names = [name for name, _ in labels]
    if len(names) < 2 or len(set(names)) != len(names):
        raise ValueError('merge needs at least two labelers with distinct names')
    output = [{'type': 'meta', 'labelers': names, 'itemsSha256': items_sha, 'guidelines': guidelines}]
    slow_counts, kind_counts, has_stretch = [], [], []
    slow_by, kind_by = {n: [] for n in names}, {n: [] for n in names}
    agreements = []
    for passage, group in passages(items).items():
        rows = {name: data[passage] for name, data in labels}
        consensus_slow = []
        for item in group:
            marks = {name: rows[name]['paragraphs'][item['paragraph'] - 1] for name in names}
            slow = {'yes': [n for n in names if marks[n]['slow']], 'no': [n for n in names if not marks[n]['slow']]}
            kinds = {kind: [n for n in names if marks[n]['kind'] == kind] for kind in KINDS}
            kinds = {kind: voters for kind, voters in kinds.items() if voters}
            verdict = majority({'yes': len(slow['yes']), 'no': len(slow['no'])}, len(names))
            consensus_slow.append(verdict == 'yes')
            for name in names:
                slow_by[name].append(marks[name]['slow'])
                kind_by[name].append(marks[name]['kind'])
            slow_counts.append({k: len(v) for k, v in slow.items()})
            kind_counts.append({k: len(v) for k, v in kinds.items()})
            output.append({'type': 'paragraph', 'id': item['id'], 'passage': passage, 'paragraph': item['paragraph'],
                           'slow': None if verdict is None else verdict == 'yes',
                           'slowVotes': {k: len(v) for k, v in slow.items()}, 'slowLabelers': slow,
                           'kind': majority({k: len(v) for k, v in kinds.items()}, None),
                           'kindVotes': {k: len(v) for k, v in kinds.items()}, 'kindLabelers': kinds})
        sets = {name: {p for s in rows[name]['stretches'] for p in range(s['start'], s['end'] + 1)} for name in names}
        pairwise = mean([jaccard(sets[a], sets[b]) for a, b in combinations(names, 2)])
        agreements.append(pairwise)
        has_stretch.append({'yes': sum(bool(sets[n]) for n in names), 'no': sum(not sets[n] for n in names)})
        merged = []
        for start, end in stretches_of(consensus_slow):
            severity = {}
            for name in names:
                hits = [s['severity'] for s in rows[name]['stretches'] if s['start'] <= end and s['end'] >= start]
                if hits:
                    severity[name] = max(hits)
            merged.append({'start': start, 'end': end, 'severity': median(severity.values()),
                           'severityVotes': severity})
        output.append({'type': 'passage', 'passage': passage, 'stretches': merged,
                       'labelerStretches': {name: rows[name]['stretches'] for name in names},
                       'stretchAgreement': pairwise,
                       'anyStretchVotes': has_stretch[-1]})
    stats = {'labelers': names, 'paragraphs': len(slow_counts), 'passages': len(agreements),
             'fleissKappa': {'slow': fleiss(slow_counts), 'kind': fleiss(kind_counts),
                             'passageHasStretch': fleiss(has_stretch)},
             'cohenKappa': {f'{a}|{b}': {'slow': cohen(slow_by[a], slow_by[b]), 'kind': cohen(kind_by[a], kind_by[b])}
                            for a, b in combinations(names, 2)},
             'slowTies': sum(row['slow'] is None for row in output if row['type'] == 'paragraph'),
             'kindTies': sum(row['kind'] is None for row in output if row['type'] == 'paragraph'),
             'consensusSlow': sum(row['slow'] is True for row in output if row['type'] == 'paragraph'),
             'meanStretchAgreement': mean(agreements),
             'passageStretchAgreement': dict(zip(passages(items), agreements)),
             'unanimousSlow': sum(max(c.values()) == len(names) for c in slow_counts),
             'slowVoteSplits': vote_splits(slow_counts, len(names)),
             'slowNarrowMajority': narrow_majority(slow_counts, len(names)),
             'perLabeler': per_labeler(names, slow_by, kind_by, slow_counts)}
    stats['outlier'] = outlier(stats['perLabeler'])
    return output, stats


def vote_splits(counts, total):
    splits = Counter(f'{max(c.values())}-{total - max(c.values())}' for c in counts)
    return dict(sorted(splits.items(), key=lambda pair: -int(pair[0].split('-')[0])))


def narrow_majority(counts, total):
    narrow = sum(2 * max(c.values()) - total == 1 for c in counts)
    return {'paragraphs': narrow, 'share': narrow / len(counts) if counts else None}


def per_labeler(names, slow_by, kind_by, slow_counts):
    result = {}
    for name in names:
        others = [n for n in names if n != name]
        own, rest = [], []
        for position, flag in enumerate(slow_by[name]):
            yes = sum(slow_by[n][position] for n in others)
            verdict = majority({'yes': yes, 'no': len(others) - yes}, len(others))
            if verdict is not None:
                own.append(flag)
                rest.append(verdict == 'yes')
        without = [{'yes': c.get('yes', 0) - flag, 'no': c.get('no', 0) - (not flag)}
                   for c, flag in zip(slow_counts, slow_by[name])]
        result[name] = {'slowParagraphs': sum(slow_by[name]),
                        'meanCohenSlow': mean([cohen(slow_by[name], slow_by[n]) for n in others]),
                        'meanCohenKind': mean([cohen(kind_by[name], kind_by[n]) for n in others]),
                        'cohenSlowVsOthersMajority': cohen(own, rest) if own else None,
                        'othersMajorityTies': len(slow_by[name]) - len(own),
                        'fleissSlowWithout': fleiss(without) if len(others) > 1 else None}
    return result


def outlier(per):
    means = {name: row['meanCohenSlow'] for name, row in per.items() if row['meanCohenSlow'] is not None}
    if len(means) < 3:
        return {'labeler': None, 'lowest': None, 'gap': None, 'margin': OUTLIER_MARGIN}
    lowest = min(means, key=means.get)
    gap = median([value for name, value in means.items() if name != lowest]) - means[lowest]
    return {'labeler': lowest if gap > OUTLIER_MARGIN else None, 'lowest': lowest, 'gap': gap,
            'margin': OUTLIER_MARGIN}


def predicted_runs(probe, items, options):
    result = {}
    for (source, name), group in group_by_document(items).items():
        doc = document(source, name)
        report = probe.report(doc.text, options=options or None)
        result[(source, name)] = [set(run) for run in run_paragraphs(report, doc) if run]
    return result


def group_by_document(items):
    result = {}
    for item in items:
        result.setdefault((item['source'], item['document']), []).append(item)
    return result


def document_index(item):
    doc = document(item['source'], item['document'])
    return next(i for i, p in enumerate(doc.paragraphs) if p['start'] == item['char']['start'])


def overlap_scores(predicted, gold, minimum_iou):
    def iou(a, b):
        a, b = set(range(a[0], a[1] + 1)), set(range(b[0], b[1] + 1))
        return len(a & b) / len(a | b)
    pred_hit = sum(any(iou(p, g) > 0 and iou(p, g) >= minimum_iou for g in gold) for p in predicted)
    gold_hit = sum(any(iou(p, g) > 0 and iou(p, g) >= minimum_iou for p in predicted) for g in gold)
    precision = pred_hit / len(predicted) if predicted else float(not gold)
    recall = gold_hit / len(gold) if gold else float(not predicted)
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
    return {'precision': precision, 'recall': recall, 'f1': f1, 'matchedPredicted': pred_hit,
            'predicted': len(predicted), 'matchedGold': gold_hit, 'gold': len(gold)}


def score(consensus, items, runs_by_document, indexes):
    paragraphs = {row['id']: row for row in consensus if row['type'] == 'paragraph'}
    gold_passages = {row['passage']: row for row in consensus if row['type'] == 'passage'}
    if set(paragraphs) != {item['id'] for item in items}:
        raise ValueError('consensus paragraphs do not match the items')
    tp = predicted = gold = excluded = 0
    run_predicted, run_gold = [], []
    for passage, group in passages(items).items():
        runs = runs_by_document[(group[0]['source'], group[0]['document'])]
        flags = []
        for item in group:
            flag = any(indexes[item['id']] in run for run in runs)
            flags.append(flag)
            truth = paragraphs[item['id']]['slow']
            if truth is None:
                excluded += 1
                continue
            tp += flag and truth
            predicted += flag
            gold += truth
        window = [indexes[item['id']] for item in group]
        clipped = []
        for run in runs:
            positions = [position for position, index in enumerate(window, 1) if index in run]
            if positions:
                clipped.append((positions[0], positions[-1]))
        run_predicted.append(clipped)
        run_gold.append([(s['start'], s['end']) for s in gold_passages[passage]['stretches']])
    return {'paragraph': {**prf(tp, predicted, gold), 'excludedTies': excluded},
            'runAnyOverlap': pooled(run_predicted, run_gold, 0.0), 'runIou50': pooled(run_predicted, run_gold, 0.5)}


def pooled(run_predicted, run_gold, minimum):
    totals = Counter()
    for p, g in zip(run_predicted, run_gold):
        result = overlap_scores(p, g, minimum)
        totals.update({k: result[k] for k in ('matchedPredicted', 'predicted', 'matchedGold', 'gold')})
    precision = totals['matchedPredicted'] / totals['predicted'] if totals['predicted'] else float(not totals['gold'])
    recall = totals['matchedGold'] / totals['gold'] if totals['gold'] else float(not totals['predicted'])
    f1 = 2 * precision * recall / (precision + recall) if precision + recall else 0.0
    return {'precision': precision, 'recall': recall, 'f1': f1, **{k: totals[k] for k in
            ('matchedPredicted', 'predicted', 'matchedGold', 'gold')}}


def held_out(consensus, excluded):
    names = [name for name in consensus[0]['labelers'] if name != excluded]
    result, flags = [], {}
    for row in consensus:
        if row['type'] == 'paragraph':
            yes = sum(name in names for name in row['slowLabelers']['yes'])
            verdict = majority({'yes': yes, 'no': len(names) - yes}, len(names))
            result.append({**row, 'slow': None if verdict is None else verdict == 'yes'})
            flags.setdefault(row['passage'], []).append(verdict == 'yes')
        elif row['type'] == 'passage':
            result.append({**row, 'stretches': [{'start': s, 'end': e} for s, e in stretches_of(flags[row['passage']])]})
        else:
            result.append({**row, 'labelers': names})
    return result


def labeler_score(consensus, name):
    tp = predicted = gold = excluded = 0
    run_predicted, run_gold, own = [], [], {}
    for row in held_out(consensus, name):
        if row['type'] == 'paragraph':
            flag = name in row['slowLabelers']['yes']
            own.setdefault(row['passage'], []).append(flag)
            if row['slow'] is None:
                excluded += 1
                continue
            tp += flag and row['slow']
            predicted += flag
            gold += row['slow']
        elif row['type'] == 'passage':
            run_predicted.append(stretches_of(own[row['passage']]))
            run_gold.append([(s['start'], s['end']) for s in row['stretches']])
    return {'paragraph': {**prf(tp, predicted, gold), 'excludedTies': excluded},
            'runAnyOverlap': pooled(run_predicted, run_gold, 0.0), 'runIou50': pooled(run_predicted, run_gold, 0.5)}


def mean_f1(results):
    return {level: mean([r[level]['f1'] for r in results]) for level in ('paragraph', 'runAnyOverlap', 'runIou50')}


def labelers_against_others(consensus):
    names = consensus[0]['labelers']
    result = {name: labeler_score(consensus, name) for name in names}
    return {'labelers': result, 'meanF1': mean_f1(result.values())}


def settings():
    return [('defaults', {})] + [('-'.join(str(o[k]) for k in ('minimumSentenceLength', 'minimumPastPerfectPercent',
                                                               'minimumCognitionPercent')), o) for o in SWEEP if o]


def report_scores(consensus, items, runs, indexes):
    held = {name: score(held_out(consensus, name), items, runs, indexes) for name in consensus[0]['labelers']}
    return {**score(consensus, items, runs, indexes),
            'againstOthersMajority': {'labelers': {name: {level: r[level]['f1'] for level in r}
                                                   for name, r in held.items()},
                                      'meanF1': mean_f1(held.values())}}


def evaluate(probe, consensus, items):
    indexes = {item['id']: document_index(item) for item in items}
    return {'settings': {name: {'options': options,
                                **report_scores(consensus, items, predicted_runs(probe, items, options), indexes)}
                         for name, options in settings()},
            'labelersAgainstOthersMajority': labelers_against_others(consensus)}


def setting_runs(probe, items, options, sources=SOURCES):
    windows = {}
    for item in items:
        windows.setdefault((item['source'], item['document']), set()).add(document_index(item))
    runs, volume = {}, Counter()
    for source in sources:
        for name, doc in source_documents(source).items():
            report = probe.report(doc.text, options=options or None)
            mapped = {row: index for index, row in row_indexes(report, doc).items()}
            window = windows.get((source, name), set())
            if (source, name) in windows:
                runs[(source, name)] = [set(run) for run in run_paragraphs(report, doc) if run]
            flagged = [row for run in report['slowRuns'] for row in range(run['paragraphs']['start'],
                                                                           run['paragraphs']['end'])]
            volume.update({'paragraphs': len(report['paragraphs']), 'runs': len(report['slowRuns']),
                           'flagged': len(flagged), 'outsideWindows': sum(mapped.get(row) not in window
                                                                          for row in flagged)})
    return runs, flag_volume(volume)


def flag_volume(volume):
    flagged = volume['flagged']
    return {'paragraphs': volume['paragraphs'], 'runs': volume['runs'], 'flagged': flagged,
            'flaggedPerThousand': 1000 * flagged / volume['paragraphs'] if volume['paragraphs'] else 0.0,
            'outsideWindows': volume['outsideWindows'],
            'outsideWindowsShare': volume['outsideWindows'] / flagged if flagged else 0.0}


def grid():
    keys = list(GRID)
    return [dict(zip(keys, values)) for values in product(*GRID.values())]


def sweep(executable, consensus, items, workers=3):
    indexes = {item['id']: document_index(item) for item in items}
    options = grid()
    for source in SOURCES:
        source_documents(source)
    def run(chunk):
        probe = probe_for(executable)
        try:
            return [(o, *setting_runs(probe, items, o)) for o in chunk]
        finally:
            probe.close()
    with ThreadPoolExecutor(workers) as pool:
        chunks = [options[i::workers] for i in range(workers)]
        runs = [triple for chunk in pool.map(run, chunks) for triple in chunk]
    rows = []
    for o, predicted, volume in runs:
        result = report_scores(consensus, items, predicted, indexes)
        rows.append({'options': o,
                     **{level: {k: result[level][k] for k in ('precision', 'recall', 'f1', 'predicted', 'gold')}
                        for level in ('paragraph', 'runAnyOverlap', 'runIou50')},
                     'againstOthersMajorityMeanF1': result['againstOthersMajority']['meanF1'],
                     'sourceFlags': volume})
    rows.sort(key=lambda row: (-row['paragraph']['f1'], -row['runAnyOverlap']['f1'],
                               tuple(row['options'][k] for k in GRID)))
    return {'consensusSha256': file_digest(CONSENSUS), 'grid': {k: list(v) for k, v in GRID.items()},
            'objective': 'paragraph-level F1 for slow against the five-labeler majority',
            'sourceFlags': 'paragraphs the setting flags across every document of the label sources; '
                           'precision is unmeasured for the outsideWindows share',
            'results': rows}


def labeler_files(directory=LABELER_DIR):
    return sorted(Path(directory).glob('*.jsonl'))


def merge_files(files, items_path=ITEMS):
    items = load_items(items_path)
    return merge([load_labels(path, items) for path in files], items, file_digest(items_path), labeled_guidelines())


def gate(result, consensus_path, baseline_path=BASELINE, labeler_dir=LABELER_DIR, items_path=ITEMS):
    if not baseline_path.exists():
        raise ValueError(f'no baseline at {baseline_path}; record one with --write-baseline')
    baseline = json.loads(baseline_path.read_text())
    if baseline['consensusSha256'] != file_digest(consensus_path):
        raise ValueError('consensus file differs from the recorded baseline')
    merged, _ = merge_files(labeler_files(labeler_dir), items_path)
    if merged != jsonl(consensus_path):
        raise ValueError('consensus does not match a fresh merge of the labeler files')
    failures = [f'{level}.{metric}' for level in ('paragraph', 'runAnyOverlap')
                for metric in ('precision', 'recall', 'f1')
                if result['settings']['defaults'][level][metric] < baseline['defaults'][level][metric] - 1e-9]
    if failures:
        raise ValueError('pacing label regression: ' + ', '.join(failures))


def probe_for(path):
    from pacing import Probe
    return Probe(ROOT / 'storage/en_tokenizer.bin', path)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    command = commands.add_parser('select', help='rebuild the item and strata files (needs the pacing probe)')
    command.add_argument('--probe', type=Path)
    command = commands.add_parser('packet', help='write a labeler packet')
    command.add_argument('out', type=Path)
    command.add_argument('--force', action='store_true')
    commands.add_parser('check', help='verify that every item materializes from the pinned downloads')
    command = commands.add_parser('validate', help='validate labeler files')
    command.add_argument('files', type=Path, nargs='+')
    command = commands.add_parser('merge', help='merge labeler files into a consensus file')
    command.add_argument('files', type=Path, nargs='*', help='default: every file in labels/pacing/')
    command.add_argument('--out', type=Path, default=CONSENSUS)
    command.add_argument('--stats', type=Path, default=AGREEMENT)
    command = commands.add_parser('score', help='score the pacing report against the consensus')
    command.add_argument('--consensus', type=Path, default=CONSENSUS)
    command.add_argument('--probe', type=Path)
    command.add_argument('--output', type=Path)
    command.add_argument('--gate', action='store_true')
    command.add_argument('--write-baseline', action='store_true')
    command = commands.add_parser('sweep', help='score the report over the threshold grid against the consensus')
    command.add_argument('--probe', type=Path, required=True)
    command.add_argument('--output', type=Path, default=SWEEP_RESULTS)
    command.add_argument('--workers', type=int, default=3)
    args = parser.parse_args(argv)
    if args.command == 'select':
        probe = probe_for(args.probe)
        try:
            items, strata = select(probe)
        finally:
            probe.close()
        write_jsonl(ITEMS, items)
        STRATA.write_text(json.dumps(strata, indent=2) + '\n')
        print(json.dumps({'items': len(items), 'passages': len(strata['passages']),
                          'strata': Counter(p['stratum'] for p in strata['passages'].values())}))
        return
    items = load_items()
    if args.command == 'packet':
        written = packet(args.out, items, args.force)
        print(json.dumps({'out': str(args.out), 'files': len(written), 'passages': len(passages(items)),
                          'paragraphs': len(items)}))
    elif args.command == 'check':
        materialize(items)
        print(json.dumps({'items': len(items), 'passages': len(passages(items)), 'materialized': True}))
    elif args.command == 'validate':
        failed = False
        for path in args.files:
            try:
                name, rows = load_labels(path, items)
                print(f'{path}: ok ({name}, {len(rows)} passages)')
            except ValueError as error:
                failed = True
                print(error, file=sys.stderr)
        if failed:
            sys.exit(1)
    elif args.command == 'merge':
        output, stats = merge_files(args.files or labeler_files())
        write_jsonl(args.out, output)
        args.stats.write_text(json.dumps(stats, indent=2) + '\n')
        print(json.dumps(stats, indent=2))
    elif args.command == 'score':
        if not args.consensus.exists():
            sys.exit(f'no consensus file at {args.consensus}; merge labeler files first')
        consensus = jsonl(args.consensus)
        if consensus[0].get('itemsSha256') != file_digest():
            sys.exit('consensus was merged against a different item file')
        probe = probe_for(args.probe)
        try:
            result = evaluate(probe, consensus, items)
        finally:
            probe.close()
        print(json.dumps(result, indent=2))
        if args.output:
            args.output.write_text(json.dumps(result, indent=2) + '\n')
        if args.write_baseline:
            BASELINE.write_text(json.dumps({'consensusSha256': file_digest(args.consensus),
                                            'defaults': result['settings']['defaults'],
                                            'labelersAgainstOthersMajority':
                                                result['labelersAgainstOthersMajority']['meanF1']},
                                           indent=2) + '\n')
        elif args.gate:
            gate(result, args.consensus)
    elif args.command == 'sweep':
        consensus = jsonl(CONSENSUS)
        if consensus[0].get('itemsSha256') != file_digest():
            sys.exit('consensus was merged against a different item file')
        result = sweep(args.probe, consensus, items, args.workers)
        args.output.write_text(json.dumps(result, indent=2) + '\n')
        print(json.dumps(result['results'][:10], indent=2))


if __name__ == '__main__':
    try:
        main()
    except ValueError as error:
        sys.exit(str(error))
