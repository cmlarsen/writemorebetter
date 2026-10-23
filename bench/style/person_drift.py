"""Person drift (#354): synthetic-perturbation recall and precision, traps, scene-inference accuracy
and the natural flag rate, with model-judged natural flags.

  python3 bench/style/person_drift.py perturb               write labels/person-drift-perturbations.jsonl
                                                            (seed 354; offsets and replacements only)
  python3 bench/style/person_drift.py flags                 print the natural flags at default and
                                                            declared options
  python3 bench/style/person_drift.py sample                write labels/person-drift-natural.jsonl
                                                            (every natural flag, or a seeded 150)
  python3 bench/style/person_drift.py packet LABELS OUT     blind labeler packet: id and context only
  python3 bench/style/person_drift.py merge LABELS A B [--adjudicate FILE] [--meta JSON]
                                                            fold two labelers' answers into LABELS
  python3 bench/style/person_drift.py score [--gate] [--write]
                                                            everything; --write updates
                                                            person-drift-baseline.json

The texts are the CC0 stories in bench/public and the Standard Ebooks (CC0) books in sources.json.
Nothing from bench/corpus is read. Perturbation rows hold character offsets into a chapter and the
replacement or inserted string (CC0, written for this file); scoring rebuilds each perturbed copy.
Blockquoted paragraphs (letters, notes) are passed as caller `embedded` spans, the markup a caller
with document structure would send.
"""
import argparse
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import random
import re
import subprocess
import sys

from common import HERE, ROOT, jsonl, write_jsonl
from pacing_labels import PUBLIC, source_documents

PERTURBATIONS = HERE / 'labels' / 'person-drift-perturbations.jsonl'
NATURAL = HERE / 'labels' / 'person-drift-natural.jsonl'
GUIDELINES = HERE / 'labels' / 'PERSON-DRIFT-GUIDELINES.md'
BASELINE = HERE / 'person-drift-baseline.json'
TOOL = ROOT / 'bench/tools/wmb-document'
SEED = 354
SAMPLE = 150
CONTEXT = 300
WORKERS = 3

# Book-level person. First-person narrators: the names and pronoun a third-person slip would use.
# The public stories' narrators are unnamed; their pronoun is a declaration for the perturbation.
FIRST = {
    'ackroyd': (['Sheppard', 'James Sheppard', 'Dr. Sheppard'], 'he'),
    'hound': (['Watson', 'John Watson', 'Dr. Watson'], 'he'),
    'sign_ch1': (['Watson', 'John Watson', 'Dr. Watson'], 'he'),
    'styles_ch1': (['Hastings', 'Arthur Hastings', 'Captain Hastings'], 'he'),
    'thirty_nine_steps': (['Hannay', 'Richard Hannay', 'Dick Hannay'], 'he'),
    'treasure_island': (['Jim', 'Jim Hawkins', 'Hawkins'], 'he'),
    'gatsby_ch1': (['Nick', 'Nick Carraway', 'Carraway'], 'he'),
    'moby': (['Ishmael'], 'he'),
    'story1': ([], 'she'),
    'story2': ([], 'she'),
    'story3': ([], 'she'),
    'story4': ([], 'she'),
    'story5': ([], 'she'),
}
THIRD = ('pride', 'blue_castle', 'age_of_innocence', 'silas_marner', 'avonlea_ch1')
SOURCES = tuple(FIRST) + THIRD
# More CC0 books from sources.json, for the natural flag rate and judged flags only (no book-level
# person label: several are multi-narrator or switch person).
EXTRA = ('to_the_lighthouse', 'jacobs_room', 'portrait', 'ulysses', 'sons_and_lovers', 'voyage_out',
         'night_and_day', 'women_in_love', 'howards_end', 'this_side_of_paradise', 'main_street', 'orlando',
         'sound_and_the_fury', 'as_i_lay_dying', 'a_farewell_to_arms')
TARGET = {'1in3': 300, '3in1a': 150, '3in1b': 150, 'name': 300}
TRAP_EACH = 22

# CC0 trap insertions. {P} is the scene's main pronoun, {N} a gendered noun of the narrator's gender.
TRAPS = {
    'thought': 'I have to get out of here, {p} thought.',
    'speech': 'I’m not staying another night, {p} said.',
    'exclamation': 'God, I was a fool!',
    'colon': 'The note read: I went to the station at noon.',
    'aside': '{P} turned—I swear it—and walked away.',
    'present': 'I can’t do this.',
    'antecedent': 'The {n} by the window stood up. {P} picked up the knife.',
}
GENDERED = {'he': 'man', 'she': 'woman'}

PAST = re.compile(r"^(?:[a-z]+ed|was|had|went|saw|took|came|made|knew|thought|felt|said|told|found|gave|"
                  r"stood|sat|ran|left|kept|held|heard|began|brought|got|put|looked|did|met|lay|"
                  r"rose|spoke|wrote|drew|threw|caught|bought|fell|led|lost|paid|sent|spent|struck|"
                  r"taught|understood|won|woke|wore|became|forgot|hid|hung|meant|read|rode|sang|sank|"
                  r"shook|slept|stole|swam|swore|tore|turned|walked|waited)$")
SENTENCE = re.compile(r'(?:[^.!?]|(?<=\bMr)\.|(?<=\bMrs)\.|(?<=\bDr)\.|(?<=\bMs)\.|(?<=\bSt)\.)+[.!?]')
QUOTES = re.compile('[“”"‘]')
SAME_GENDER = {'he': re.compile(r'\b(?:he|him|his|himself)\b', re.I),
               'she': re.compile(r'\b(?:she|her|hers|herself)\b', re.I)}
CAPITAL = re.compile(r'(?<=[a-z,;] )[A-Z][a-z]+')


def probe_process():
    return subprocess.Popen([str(TOOL / 'target/release/person_drift'), str(ROOT / 'storage/en_tokenizer.bin')],
                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding='utf-8')


def build():
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml'),
                    '--bin', 'person_drift'], check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def probe(inputs):
    """Runs the probe over the inputs in WORKERS processes, keeping order."""
    build()
    chunks = [inputs[i::WORKERS] for i in range(WORKERS)]

    def run(chunk):
        result = subprocess.run([str(TOOL / 'target/release/person_drift'), str(ROOT / 'storage/en_tokenizer.bin')],
                                input=''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in chunk),
                                capture_output=True, text=True, encoding='utf-8', check=True)
        return [json.loads(line) for line in result.stdout.splitlines()]

    with ThreadPoolExecutor(WORKERS) as pool:
        outputs = list(pool.map(run, chunks))
    merged = [None] * len(inputs)
    for offset, output in enumerate(outputs):
        for index, row in enumerate(output):
            merged[offset + index * WORKERS] = row
    for row in merged:
        if 'error' in row:
            sys.exit('probe error: ' + row['error'])
    return merged


def digest(text):
    return hashlib.sha256(text.encode('utf-8')).hexdigest()


# Documents and options

def documents(sources=SOURCES):
    for source in sources:
        for name, document in source_documents(source).items():
            yield source, name, document


def marked(document):
    return [{'start': p['start'], 'end': p['end'], 'voice': 'embedded'}
            for p in document.paragraphs if p['quoted']]


def document_options(document, window=None):
    spans = marked(document)
    if window:
        lo, hi = window
        spans = [{**s, 'start': s['start'] - lo, 'end': s['end'] - lo} for s in spans
                 if s['start'] >= lo and s['end'] <= hi]
    return {'markedSpans': spans, 'markedSpanUnits': 'char'}


def declared(source):
    if source in FIRST:
        names, pronoun = FIRST[source]
        return {'person': 'first', 'narrator': {'names': names, 'pronoun': pronoun}}
    return {'person': 'third'}


def narrator_only(source):
    names, pronoun = FIRST[source]
    return {'narrator': {'names': names, 'pronoun': pronoun}}


_NATURAL = {}


def natural():
    """Default and declared reports for every chapter."""
    if not _NATURAL:
        docs = list(documents())
        extra = list(documents(EXTRA))
        inputs = []
        for source, _, document in docs:
            inputs.append({'text': document.text, 'documentOptions': document_options(document)})
            inputs.append({'text': document.text, 'documentOptions': document_options(document),
                           'options': declared(source)})
        inputs += [{'text': d.text, 'documentOptions': document_options(d)} for _, _, d in extra]
        results = probe(inputs)
        _NATURAL['docs'] = docs
        _NATURAL['default'] = results[0:2 * len(docs):2]
        _NATURAL['declared'] = results[1:2 * len(docs):2]
        _NATURAL['extraDocs'] = extra
        _NATURAL['extra'] = results[2 * len(docs):]
    return _NATURAL


def variant_docs(variant):
    data = natural()
    return zip(data['extraDocs' if variant == 'extra' else 'docs'], data[variant])


# Perturbations

def sentences(text, start, end):
    for match in SENTENCE.finditer(text, start, end):
        lo = match.start()
        while lo < match.end() and text[lo].isspace():
            lo += 1
        yield lo, match.end()


def scene_of(report, char):
    for scene in report['scenes']:
        if scene['span']['char']['start'] <= char < scene['span']['char']['end']:
            return scene
    return None


def plain_paragraphs(document):
    for index, paragraph in enumerate(document.paragraphs):
        body = document.text[paragraph['start']:paragraph['end']]
        if paragraph['quoted'] or QUOTES.search(body) or body.lstrip().startswith(('—', '–')):
            continue
        yield index, paragraph


def simple(text):
    return not re.search(r'[;:()\[\]—–…?!]', text) and len(text.split()) <= 40


def perturb():
    rng = random.Random(SEED)
    data = natural()
    pools = defaultdict(list)
    traps = defaultdict(list)
    for (source, name, document), report in zip(data['docs'], data['default']):
        text = document.text
        for index, paragraph in plain_paragraphs(document):
            scene = scene_of(report, paragraph['start'])
            if scene is None or scene['suppressed'] not in (None, 'noNarrator'):
                continue
            for lo, hi in sentences(text, paragraph['start'], paragraph['end']):
                sentence = text[lo:hi]
                words = sentence.split()
                if len(words) < 3 or not simple(sentence) or not sentence.endswith('.'):
                    continue
                if source in THIRD and scene['person'] == 'third':
                    main = 'she' if text[scene['span']['char']['start']:scene['span']['char']['end']].count(' she ') \
                        > text[scene['span']['char']['start']:scene['span']['char']['end']].count(' he ') else 'he'
                    if words[0] == main.capitalize() and PAST.match(words[1]) \
                            and not re.search(r'\b(?:I|me|my|mine|myself|himself|herself)\b', sentence):
                        pools['1in3'].append({'source': source, 'document': name, 'start': lo,
                                              'end': lo + len(words[0]), 'replacement': 'I'})
                    traps['third'].append((source, name, hi, main))
                if source in FIRST and scene['person'] == 'first':
                    names, pronoun = FIRST[source]
                    if words[0] != 'I' or not PAST.match(words[1]):
                        continue
                    if re.search(r'\b(?:myself|me|my)\b', sentence):
                        continue
                    row = {'source': source, 'document': name, 'start': lo, 'end': lo + 1}
                    pools['3in1b'].append({**row, 'replacement': pronoun.capitalize()})
                    window = clean_window(document, index, lo, pronoun, names)
                    if window:
                        pools['3in1a'].append({**row, 'replacement': pronoun.capitalize(), 'window': window})
                    if names:
                        pools['name'].append({**row, 'replacement': names[0]})
                    traps['first'].append((source, name, hi, pronoun))
    rows = []
    for kind, target in TARGET.items():
        pool = pools[kind]
        rng.shuffle(pool)
        chosen = sorted(balanced(pool, target), key=lambda r: (r['source'], r['document'], r['start']))
        for number, row in enumerate(chosen, 1):
            rows.append({'id': f'{kind}-{number:03d}', 'kind': kind, **row})
    trap_rows = []
    for trap, template in TRAPS.items():
        pool = traps['first' if trap == 'antecedent' else 'third']
        for source, name, at, pronoun in sorted(rng.sample(pool, TRAP_EACH)):
            inserted = ' ' + template.format(p=pronoun, P=pronoun.capitalize(), n=GENDERED[pronoun])
            trap_rows.append({'id': f'trap-{trap}-{len(trap_rows) + 1:03d}', 'kind': 'trap', 'trap': trap,
                              'source': source, 'document': name, 'start': at, 'end': at,
                              'replacement': inserted})
    rows += trap_rows
    texts = {(s, n): d.text for s, n, d in data['docs']}
    for row in rows:
        row['textSha256'] = digest(texts[(row['source'], row['document'])])
    meta = {'type': 'meta', 'seed': SEED, 'date': '2026-10-22', 'counts': dict(Counter(r['kind'] for r in rows)),
            'note': 'offsets are code points into the chapter text pacing_labels.source_documents builds; '
                    'replacements and trap insertions are CC0, written for this file'}
    write_jsonl(PERTURBATIONS, [meta] + rows)
    print(json.dumps(meta['counts']))


def balanced(pool, target):
    """Round-robin across sources so no one book dominates."""
    by_source = defaultdict(list)
    for row in pool:
        by_source[row['source']].append(row)
    chosen = []
    while len(chosen) < target and any(by_source.values()):
        for source in sorted(by_source):
            if by_source[source] and len(chosen) < target:
                chosen.append(by_source[source].pop())
    return chosen


def clean_window(document, index, start, pronoun, names):
    """A window of up to eight paragraphs ending with the perturbed one, with no same-gender
    pronoun, no quote mark and no capitalized mid-sentence word but the narrator's names: a scene
    with no detectable same-gender character. None unless the window holds nine or more *I*."""
    paragraphs = document.paragraphs
    allowed = {word for name in names for word in name.split()}

    def clean(body):
        return not (SAME_GENDER[pronoun].search(body) or QUOTES.search(body)
                    or any(word not in allowed for word in CAPITAL.findall(body)))

    own = paragraphs[index]
    body = document.text[own['start']:start] + document.text[start + 1:own['end']]
    if not clean(body):
        return None
    lo = index
    while lo > 0 and index - lo < 7 and not paragraphs[lo].get('scene'):
        previous = paragraphs[lo - 1]
        if previous['quoted'] or not clean(document.text[previous['start']:previous['end']]):
            break
        lo -= 1
    span = [paragraphs[lo]['start'], own['end']]
    if len(re.findall(r'\bI\b', document.text[span[0]:span[1]])) < 9:
        return None
    return span


# Scoring

def apply(row, texts):
    text = texts[(row['source'], row['document'])]
    if digest(text) != row['textSha256']:
        sys.exit(f"{row['id']}: the source text changed; rerun perturb")
    perturbed = text[:row['start']] + row['replacement'] + text[row['end']:]
    window = row.get('window')
    if window:
        shift = len(row['replacement']) - (row['end'] - row['start'])
        return perturbed[window[0]:window[1] + shift], window[0]
    return perturbed, 0


EXPECTED = {'1in3': 'firstInThird', '3in1a': 'thirdInFirst', '3in1b': 'thirdInFirst', 'name': 'narratorName'}


def runs(rows, docs):
    """(row, variant, input) for every perturbed copy that scoring runs."""
    texts = {(s, n): d.text for s, n, d in docs}
    by_key = {(s, n): d for s, n, d in docs}
    for row in rows:
        text, offset = apply(row, texts)
        document = by_key[(row['source'], row['document'])]
        options = document_options(document, row.get('window') and
                                   (row['window'][0], row['window'][1]))
        base = {'text': text, 'documentOptions': options}
        if row['kind'] == '1in3':
            yield row, 'undeclared', offset, {**base, 'options': {}}
            yield row, 'declared', offset, {**base, 'options': declared(row['source'])}
        elif row['kind'] in ('3in1a', '3in1b', 'name'):
            yield row, 'narrator', offset, {**base, 'options': narrator_only(row['source'])}
        elif row['trap'] == 'antecedent':
            yield row, 'narrator', offset, {**base, 'options': narrator_only(row['source'])}
            yield row, 'declared', offset, {**base, 'options': declared(row['source'])}
        else:
            yield row, 'undeclared', offset, {**base, 'options': {}}
            yield row, 'declared', offset, {**base, 'options': declared(row['source'])}


def perturbation_scores():
    rows = [row for row in jsonl(PERTURBATIONS) if row.get('type') != 'meta']
    docs = natural()['docs']
    plan = list(runs(rows, docs))
    results = probe([entry[3] for entry in plan])
    recall = defaultdict(Counter)
    flags = Counter()
    traps = Counter()
    trap_flags = []
    for (row, variant, offset, _), report in zip(plan, results):
        lo = row['start'] - offset
        hi = lo + len(row['replacement'])
        hits = [d for d in report['drifts'] if d['span']['char']['start'] < hi and d['span']['char']['end'] > lo]
        if row['kind'] == 'trap':
            traps[f"{row['trap']}/{variant}"] += 1
            if hits:
                trap_flags.append({'id': row['id'], 'variant': variant, 'kind': hits[0]['kind']})
            continue
        group = f"{row['kind']}/{variant}"
        found = any(d['kind'] == EXPECTED[row['kind']] for d in hits)
        recall[group]['total'] += 1
        recall[group]['found'] += found
        flags['true'] += sum(1 for d in hits if d['kind'] == EXPECTED[row['kind']])
        flags['false'] += len(report['drifts']) - sum(1 for d in hits if d['kind'] == EXPECTED[row['kind']])
    total = flags['true'] + flags['false']
    return {
        'recall': {group: {'found': c['found'], 'total': c['total'],
                           'recall': round(c['found'] / c['total'], 3) if c['total'] else None}
                   for group, c in sorted(recall.items())},
        'precision': {'true': flags['true'], 'false': flags['false'],
                      'precision': round(flags['true'] / total, 3) if total else None},
        'traps': {'runs': sum(traps.values()), 'falsePositives': len(trap_flags), 'flags': trap_flags},
    }


def inference():
    data = natural()
    books = defaultdict(Counter)
    for (source, _, _), report in zip(data['docs'], data['default']):
        truth = 'first' if source in FIRST else 'third'
        for scene in report['scenes']:
            person = scene['person']
            if person is None:
                books[source]['abstain'] += 1
            elif person == truth:
                books[source]['right'] += 1
            else:
                books[source]['wrong'] += 1
    result = {}
    pooled = Counter()
    for source in SOURCES:
        c = books[source]
        pooled.update(c)
        decided = c['right'] + c['wrong']
        scenes = decided + c['abstain']
        result[source] = {'scenes': scenes, 'right': c['right'], 'wrong': c['wrong'], 'abstain': c['abstain'],
                          'accuracy': round(c['right'] / decided, 3) if decided else None,
                          'abstention': round(c['abstain'] / scenes, 3) if scenes else None}
    decided = pooled['right'] + pooled['wrong']
    scenes = decided + pooled['abstain']
    result['pooled'] = {'scenes': scenes, 'right': pooled['right'], 'wrong': pooled['wrong'],
                        'abstain': pooled['abstain'], 'accuracy': round(pooled['right'] / decided, 3),
                        'abstention': round(pooled['abstain'] / scenes, 3)}
    return result


def natural_flags(variant):
    rows = []
    for (source, name, document), report in variant_docs(variant):
        for drift in report['drifts']:
            rows.append({'source': source, 'document': name, 'variant': variant,
                         'start': drift['span']['char']['start'], 'end': drift['span']['char']['end'],
                         'sentenceStart': drift['sentenceSpan']['char']['start'],
                         'sentenceEnd': drift['sentenceSpan']['char']['end'],
                         'kind': drift['kind'], 'word': drift['word'], 'expected': drift['expected'],
                         'scene': drift['scene']})
    return rows


def rates(variant):
    words = Counter()
    drifts = Counter()
    for (source, _, _), report in variant_docs(variant):
        words[source] += report['narrationWords']
        drifts[source] += len(report['drifts'])
    result = {source: {'drifts': drifts[source], 'narrationWords': words[source],
                       'perThousand': round(1000 * drifts[source] / words[source], 4) if words[source] else None}
              for source in (EXTRA if variant == 'extra' else SOURCES)}
    total_words, total = sum(words.values()), sum(drifts.values())
    result['pooled'] = {'drifts': total, 'narrationWords': total_words,
                        'perThousand': round(1000 * total / total_words, 4)}
    return result


# Natural labels

def context(text, start, end, sentence_start, sentence_end):
    """The flagged sentence with the word in ⟦ ⟧, and up to CONTEXT characters each side."""
    left = max(0, sentence_start - CONTEXT)
    right = min(len(text), sentence_end + CONTEXT)
    return ('…' if left else '') + text[left:start] + '⟦' + text[start:end] + '⟧' + text[end:right] + \
        ('…' if right < len(text) else '')


def key(row):
    return (row['source'], row['document'], row['variant'], row['start'], row['kind'])


def sample():
    rows = natural_flags('default') + natural_flags('declared') + natural_flags('extra')
    rng = random.Random(SEED)
    if len(rows) > SAMPLE:
        rows = rng.sample(rows, SAMPLE)
    rows.sort(key=lambda r: (r['source'], r['document'], r['start'], r['variant']))
    texts = {(s, n): d.text for s, n, d in natural()['docs'] + natural()['extraDocs']}
    out = []
    for number, row in enumerate(rows, 1):
        text = texts[(row['source'], row['document'])]
        person = 'first' if row['source'] in FIRST else 'third' if row['source'] in THIRD else 'unknown'
        out.append({'id': f'person-drift-{number:03d}', **row, 'bookPerson': person,
                    'context': context(text, row['start'], row['end'], row['sentenceStart'], row['sentenceEnd']),
                    'label': None})
    write_jsonl(NATURAL, out)
    print(json.dumps({'items': len(out), 'itemsSha256': items_digest(out)}))


def items_digest(rows):
    items = [{'id': r['id'], 'bookPerson': r['bookPerson'], 'context': r['context']} for r in rows]
    return hashlib.sha256(json.dumps(items, ensure_ascii=False, sort_keys=True).encode('utf-8')).hexdigest()


def packet(labels, out):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    write_jsonl(out, [{'id': r['id'], 'bookPerson': r['bookPerson'], 'context': r['context']} for r in rows])
    print(json.dumps({'items': len(rows), 'itemsSha256': items_digest(rows)}))


def kappa(pairs):
    pairs = [p for p in pairs if None not in p]
    if not pairs:
        return None
    n = len(pairs)
    observed = sum(a == b for a, b in pairs) / n
    p_a = sum(a for a, _ in pairs) / n
    p_b = sum(b for _, b in pairs) / n
    expected = p_a * p_b + (1 - p_a) * (1 - p_b)
    return round((observed - expected) / (1 - expected), 3) + 0.0 if expected < 1 else 1.0


def merge(labels, first, second, adjudicate, meta):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    answers = [{row['id']: row for row in jsonl(path)} for path in (first, second)]
    decided = {row['id']: row for row in jsonl(adjudicate)} if adjudicate else {}
    agree = 0
    for row in rows:
        a, b = (answer[row['id']]['label'] for answer in answers)
        row['labels'] = [a, b]
        row['notes'] = [answer[row['id']].get('note', '') for answer in answers]
        if a == b:
            agree += 1
            row['label'] = a
        elif row['id'] in decided:
            row['label'] = decided[row['id']]['label']
            row['note'] = decided[row['id']].get('note', '')
        else:
            row['label'] = None
    metadata = json.loads(meta) if meta else {}
    metadata.update({'type': 'meta', 'agreement': {'agree': agree, 'items': len(rows),
                                                   'kappa': kappa([r['labels'] for r in rows])},
                     'guidelinesSha256': hashlib.sha256(GUIDELINES.read_bytes()).hexdigest(),
                     'itemsSha256': items_digest(rows)})
    write_jsonl(labels, [metadata] + rows)
    print(json.dumps(metadata['agreement']))


def natural_precision():
    if not NATURAL.exists():
        return None
    rows = jsonl(NATURAL)
    meta = next((r for r in rows if r.get('type') == 'meta'), {})
    rows = [r for r in rows if r.get('type') != 'meta']
    current = {key(r) for r in natural_flags('default') + natural_flags('declared') + natural_flags('extra')}
    groups = defaultdict(Counter)
    for row in rows:
        state = 'kept' if key(row) in current else 'dropped'
        for group in ('all', row['variant'], f"{row['variant']}/{row['kind']}"):
            groups[group][(state, row['label'])] += 1
    result = {}
    for group, c in sorted(groups.items()):
        kept = c[('kept', True)] + c[('kept', False)]
        result[group] = {'kept': kept, 'true': c[('kept', True)], 'false': c[('kept', False)],
                         'unlabeled': c[('kept', None)],
                         'dropped': sum(v for (state, _), v in c.items() if state == 'dropped'),
                         'precision': round(c[('kept', True)] / kept, 3) if kept else None}
    unlabeled_current = len(current - {key(r) for r in rows})
    return {'agreement': meta.get('agreement'), 'groups': result, 'currentFlagsNotInSample': unlabeled_current}


def score(gate, write):
    result = {
        'perturbations': perturbation_scores(),
        'inference': inference(),
        'natural': {'default': rates('default'), 'declared': rates('declared'), 'extra': rates('extra')},
        'naturalPrecision': natural_precision(),
    }
    print(json.dumps(result, indent=1, ensure_ascii=False))
    if write:
        BASELINE.write_text(json.dumps({'seed': SEED, **result}, indent=1, ensure_ascii=False) + '\n')
    if gate:
        failures = []
        p = result['perturbations']
        if p['precision']['precision'] is None or p['precision']['precision'] < 0.90:
            failures.append(f"perturbed precision {p['precision']['precision']} < 0.90")
        if p['traps']['falsePositives']:
            failures.append(f"{p['traps']['falsePositives']} trap false positives")
        pooled = result['natural']['default']['pooled']['perThousand']
        if pooled > 0.05:
            failures.append(f'natural flags {pooled} per 1,000 narration words > 0.05')
        if BASELINE.exists():
            baseline = json.loads(BASELINE.read_text())['perturbations']['recall']
            for group, row in baseline.items():
                now = p['recall'].get(group, {}).get('recall')
                if row['recall'] is not None and (now is None or now < row['recall']):
                    failures.append(f'{group} recall {now} < baseline {row["recall"]}')
        if failures:
            sys.exit('gate failed: ' + '; '.join(failures))
        print('gate passed')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('perturb')
    sub.add_parser('flags')
    sub.add_parser('sample')
    p = sub.add_parser('packet')
    p.add_argument('labels')
    p.add_argument('out')
    m = sub.add_parser('merge')
    m.add_argument('labels')
    m.add_argument('first')
    m.add_argument('second')
    m.add_argument('--adjudicate')
    m.add_argument('--meta')
    s = sub.add_parser('score')
    s.add_argument('--gate', action='store_true')
    s.add_argument('--write', action='store_true')
    args = parser.parse_args()
    if args.command == 'perturb':
        perturb()
    elif args.command == 'flags':
        texts = {(s, n): d.text for s, n, d in natural()['docs'] + natural()['extraDocs']}
        for variant in ('default', 'declared', 'extra'):
            for row in natural_flags(variant):
                text = texts[(row['source'], row['document'])]
                print(variant, row['source'], row['kind'], row['word'],
                      json.dumps(text[row['sentenceStart']:row['sentenceEnd']][:240], ensure_ascii=False))
    elif args.command == 'sample':
        sample()
    elif args.command == 'packet':
        packet(args.labels, args.out)
    elif args.command == 'merge':
        merge(args.labels, args.first, args.second, args.adjudicate, args.meta)
    elif args.command == 'score':
        score(args.gate, args.write)


if __name__ == '__main__':
    main()
