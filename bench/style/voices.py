"""Voices (#353): synthetic recall and precision, model-judged natural precision, a miss estimate and
the narration delta for tag-anchored unquoted thought and speech.

  python3 bench/style/voices.py synthetic [--write]       synthetic suite (seed 353)
  python3 bench/style/voices.py sample [--size 150]       write labels/thought-spans.jsonl (unlabeled)
  python3 bench/style/voices.py heldout [--size 100]      write labels/thought-spans-heldout.jsonl from
                                                          flags not in the development sample (seed 3531)
  python3 bench/style/voices.py misses [--size 100]       write labels/thought-misses.jsonl: unflagged
                                                          narration sentences with a thought verb
  python3 bench/style/voices.py packet LABELS OUT          blind labeler packet: items, no labels
  python3 bench/style/voices.py merge LABELS A B [--adjudicate FILE] [--meta JSON]
                                                          fold two labelers' answers into LABELS
  python3 bench/style/voices.py score [--gate] [--write]  everything above; --write updates
                                                          voices-baseline.json

The texts are the public-domain books and chapters in sources.json (Standard Ebooks, CC0) and the CC0
stories in bench/public. Labels hold one short public-domain context window per span; nothing from
bench/corpus is read. Synthetic thoughts and negatives come from thought-templates.json (CC0).
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
import random
import re
import subprocess
import sys

from common import HERE, ROOT, jsonl, write_jsonl
from pacing_labels import BOOKS, PUBLIC, source_documents

LABELS = HERE / 'labels' / 'thought-spans.jsonl'
HELDOUT = HERE / 'labels' / 'thought-spans-heldout.jsonl'
HELDOUT2 = HERE / 'labels' / 'thought-spans-heldout-2.jsonl'
MISSES = HERE / 'labels' / 'thought-misses.jsonl'
GUIDELINES = HERE / 'labels' / 'THOUGHT-GUIDELINES.md'
BASELINE = HERE / 'voices-baseline.json'
TEMPLATES = HERE / 'thought-templates.json'
# Modernist novels (CC0 at Standard Ebooks) hold most of the unquoted thought in the public set.
MODERN = ('to_the_lighthouse', 'jacobs_room', 'portrait', 'ulysses', 'sons_and_lovers')
# Added after the rules were frozen at 33b7f15, for the second held-out sample only.
FRESH = ('voyage_out', 'night_and_day', 'women_in_love', 'howards_end', 'this_side_of_paradise',
         'main_street', 'orlando', 'sound_and_the_fury', 'as_i_lay_dying', 'a_farewell_to_arms')
SOURCES = tuple(PUBLIC) + BOOKS + MODERN + FRESH
SEED = 353
HELDOUT_SEED = 3531
HELDOUT2_SEED = 3532
CONTEXT = 110
TOOL = ROOT / 'bench/tools/wmb-document'
THOUGHT_LEMMAS = ('think', 'wonder', 'muse', 'reason', 'realize', 'realise', 'decide', 'figure',
                  'suppose', 'reflect', 'pray', 'ponder', 'calculate', 'conclude', 'reckon')
THOUGHT_FORMS = re.compile(r'\b(thought|wondered|mused|reasoned|reali[sz]ed|decided|figured|supposed|'
                           r'reflected|prayed|pondered|calculated|concluded|reckoned|thinks|wonders)\b')
# A rough sentence for the miss sample: up to . ! ? that do not follow an honorific.
SENTENCE = re.compile(r'(?:[^.!?\n]|(?<=\bMr)\.|(?<=\bMrs)\.|(?<=\bDr)\.|(?<=\bMs)\.|(?<=\bSt)\.)*[.!?]')
LEMMA_OF = {'thought': 'think', 'thinks': 'think', 'wondered': 'wonder', 'wonders': 'wonder',
            'mused': 'muse', 'reasoned': 'reason', 'realized': 'realize', 'realised': 'realise',
            'decided': 'decide', 'figured': 'figure', 'supposed': 'suppose', 'reflected': 'reflect',
            'prayed': 'pray', 'pondered': 'ponder', 'calculated': 'calculate', 'concluded': 'conclude',
            'reckoned': 'reckon'}


def probe(inputs):
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml'),
                    '--bin', 'voices'], check=True, stdout=subprocess.DEVNULL,
                   stderr=subprocess.DEVNULL)
    result = subprocess.run([str(TOOL / 'target/release/voices'), str(ROOT / 'storage/en_tokenizer.bin')],
                            input=''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in inputs),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    return [json.loads(line) for line in result.stdout.splitlines()]


def char_range(span):
    return span['char']['start'], span['char']['end']


# Natural texts

def documents():
    for source in SOURCES:
        for name, document in source_documents(source).items():
            yield source, name, document.text


def window(text, start, end):
    left = max(text.rfind('\n', 0, start) + 1, start - CONTEXT)
    right = text.find('\n', end)
    right = min(len(text) if right < 0 else right, end + CONTEXT)
    return ('…' if left > 0 and text[left - 1] != '\n' else '') + text[left:right] + \
        ('…' if right < len(text) and text[right] != '\n' else ''), left


def marked(text, span, tag):
    """The context window with the span in ⟦ ⟧ and the tag in ⟨ ⟩."""
    lo, hi = min(span[0], tag[0]), max(span[1], tag[1])
    context, left = window(text, lo, hi)
    prefix = '…' if context.startswith('…') and left > 0 else ''
    body = context[len(prefix):]
    marks = sorted([(span[0] - left, '⟦'), (span[1] - left, '⟧'), (tag[0] - left, '⟨'), (tag[1] - left, '⟩')],
                   reverse=True)
    for offset, mark in marks:
        body = body[:offset] + mark + body[offset:]
    return prefix + body


def anchor_of(span, spans):
    s, t = char_range(span['span']), char_range(span['tag'])
    if sum(1 for other in spans if other.get('tag') == span['tag']) > 1:
        return 'mid'
    return 'leading' if t[1] <= s[0] else 'trailing'


_RUN = {}


def run_natural():
    if 'natural' not in _RUN:
        docs = list(documents())
        _RUN['natural'] = (docs, probe([{'text': text} for _, _, text in docs]))
    return _RUN['natural']


def flags():
    docs, results = run_natural()
    rows = []
    for (source, name, text), result in zip(docs, results):
        spans = result['voices']['spans']
        for span in spans:
            if span['source'] != 'tag':
                continue
            s, t = char_range(span['span']), char_range(span['tag'])
            verb = THOUGHT_FORMS.search(text[t[0]:t[1]].lower())
            rows.append({'source': source, 'document': name, 'start': s[0], 'end': s[1],
                         'tagStart': t[0], 'tagEnd': t[1], 'voice': span['voice'], 'shift': span['shift'],
                         'anchor': anchor_of(span, spans), 'span': text[s[0]:s[1]], 'tag': text[t[0]:t[1]],
                         'lemma': LEMMA_OF.get(verb.group(1)) if verb else None,
                         'context': marked(text, s, t)})
    return rows


def key(row):
    return row['source'], row['document'], row['start'], row['end']


def stratified(rows, size, rng, half=False):
    """Round-robin over (voice, anchor) strata so rare anchors and the thought voice are not
    crowded out. With `half`, a stratum gives at most half its rows (rounded up), leaving the rest
    for the held-out draw."""
    strata = defaultdict(list)
    for row in rows:
        strata[(row['voice'], row['anchor'])].append(row)
    for name, members in strata.items():
        rng.shuffle(members)
        if half:
            strata[name] = members[:math.ceil(len(members) / 2)]
    chosen = []
    while len(chosen) < size and any(strata.values()):
        for stratum in sorted(strata):
            if strata[stratum] and len(chosen) < size:
                chosen.append(strata[stratum].pop())
    return chosen


def sample(size, path, seed, exclude=(), half=False, sources=None):
    rows = [row for row in flags() if sources is None or row['source'] in sources]
    excluded = {key(row) for row in exclude if row.get('type') != 'meta'}
    rows = [row for row in rows if key(row) not in excluded]
    chosen = stratified(rows, size, random.Random(seed), half)
    chosen.sort(key=key)
    write_jsonl(path, [{**row, 'id': f'{path.stem}-{i:03d}', 'label': None, 'note': ''}
                       for i, row in enumerate(chosen, 1)])
    print(json.dumps({'flags': len(rows), 'sampled': len(chosen),
                      'strata': dict(Counter(f"{r['voice']}/{r['anchor']}" for r in chosen))}))


def misses(size):
    """Narration sentences with a thought-verb form that the detector left alone."""
    docs, results = run_natural()
    rows = []
    for (source, name, text), result in zip(docs, results):
        covered = [char_range(s['span']) + char_range(s['tag']) if s.get('tag') else char_range(s['span'])
                   for s in result['voices']['spans']]
        for match in SENTENCE.finditer(text):
            sentence = match.group(0).strip()
            start = match.start() + match.group(0).index(sentence[:1]) if sentence else match.start()
            end = start + len(sentence)
            if not THOUGHT_FORMS.search(sentence.lower()) or any(q in sentence for q in '“”"'):
                continue
            if any(c[0] < end and start < c[1] for c in covered) or \
                    any(len(c) == 4 and c[2] < end and start < c[3] for c in covered):
                continue
            context, left = window(text, start, end)
            prefix = '…' if context.startswith('…') and left > 0 else ''
            body = context[len(prefix):]
            body = body[:end - left] + '⟧' + body[end - left:]
            body = body[:start - left] + '⟦' + body[start - left:]
            rows.append({'source': source, 'document': name, 'start': start, 'end': end,
                         'sentence': sentence, 'context': prefix + body})
    chosen = random.Random(SEED + 2).sample(rows, min(size, len(rows)))
    chosen.sort(key=key)
    write_jsonl(MISSES, [{**row, 'id': f'miss-{i:03d}', 'label': None, 'note': ''}
                         for i, row in enumerate(chosen, 1)])
    print(json.dumps({'candidates': len(rows), 'sampled': len(chosen)}))


def items_digest(rows):
    items = [{k: row[k] for k in ('id', 'voice', 'context')} for row in rows if row.get('type') != 'meta']
    return hashlib.sha256(json.dumps(items, ensure_ascii=False, sort_keys=True).encode()).hexdigest()


def packet(labels, out):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    fields = ('id', 'voice', 'context') if 'voice' in rows[0] else ('id', 'context')
    write_jsonl(out, [{k: row[k] for k in fields} for row in rows])
    print(json.dumps({'items': len(rows), 'itemsSha256': items_digest(rows) if 'voice' in rows[0] else None}))


def merge(labels, first, second, adjudicate, meta):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    answers = [{row['id']: row for row in jsonl(path)} for path in (first, second)]
    decided = {row['id']: row for row in jsonl(adjudicate)} if adjudicate else {}
    agree = 0
    for row in rows:
        a, b = (answer[row['id']]['label'] for answer in answers)
        row['labels'] = [a, b]
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
                     'guidelinesSha256': hashlib.sha256(GUIDELINES.read_bytes()).hexdigest()})
    if 'voice' in rows[0]:
        metadata['itemsSha256'] = items_digest(rows)
    write_jsonl(labels, [metadata] + rows)
    print(json.dumps(metadata['agreement']))


def kappa(pairs):
    pairs = [p for p in pairs if None not in p]
    if not pairs:
        return None
    n = len(pairs)
    observed = sum(a == b for a, b in pairs) / n
    p_a = sum(a for a, _ in pairs) / n
    p_b = sum(b for _, b in pairs) / n
    expected = p_a * p_b + (1 - p_a) * (1 - p_b)
    return round((observed - expected) / (1 - expected), 3) if expected < 1 else 1.0


def wilson(successes, total, z=1.96):
    if not total:
        return None
    p = successes / total
    centre = p + z * z / (2 * total)
    margin = z * math.sqrt(p * (1 - p) / total + z * z / (4 * total * total))
    return round((centre - margin) / (1 + z * z / total), 3)


def precision(path, current):
    rows = [row for row in jsonl(path) if row.get('type') != 'meta']
    if any(row['label'] is None for row in rows):
        sys.exit('unlabeled rows in ' + str(path))
    groups = defaultdict(Counter)
    for row in rows:
        state = 'kept' if key(row) in current else 'dropped'
        # The anchor as the current rules classify it (the sample may predate a reclassification).
        anchor = current[key(row)]['anchor'] if key(row) in current else row['anchor']
        for group in ('all', row['voice'], f"{row['voice']}/{anchor}",
                      *(['lemma/' + row['lemma']] if row.get('lemma') else [])):
            groups[group][(state, row['label'])] += 1
    result = {}
    for group, counts in sorted(groups.items()):
        kept_true, kept_false = counts[('kept', True)], counts[('kept', False)]
        kept = kept_true + kept_false
        result[group] = {'labeled': sum(counts.values()), 'kept': kept,
                         'precision': round(kept_true / kept, 3) if kept else None,
                         'wilsonLower': wilson(kept_true, kept),
                         'droppedTrue': counts[('dropped', True)], 'droppedFalse': counts[('dropped', False)]}
    return result


def narration_delta():
    docs, results = run_natural()
    per_source = defaultdict(Counter)
    for (source, _, _), result in zip(docs, results):
        voices, stats = result['voices'], result['stats']
        counter = per_source[source]
        counter['modeNarrationSentences'] += stats['sentences']['narration']
        counter['voiceNarrationSentences'] += voices['narrationSentences']
        counter['modeNarrationWords'] += stats['words']['narration']
        counter['voiceNarrationWords'] += voices['narrationWords']
        counter['words'] += stats['words']['all']
        for voice, count in result['moved'].items():
            counter['words/' + voice] += count
        for span in voices['spans']:
            if span['source'] == 'tag':
                counter['tag/' + span['voice']] += 1
    result = {}
    for source, counter in per_source.items():
        narration = counter['modeNarrationWords']
        result[source] = {
            'modeNarrationSentences': counter['modeNarrationSentences'],
            'voiceNarrationSentences': counter['voiceNarrationSentences'],
            # The share of narration-mode words each voice takes out of narration.
            'wordsMovedFromNarration': {
                voice: round(counter['words/' + voice] / narration, 5) if narration else None
                for voice in ('thought', 'speech', 'quoted', 'embedded', 'quotation', 'excluded')
                if counter['words/' + voice]},
            'tagSpansPerThousandNarrationWords': {
                voice: round(1000 * counter['tag/' + voice] / narration, 3) if narration else None
                for voice in ('thought', 'speech')},
        }
    return result


# Synthetic suite

def straight(text):
    return text.replace('’', "'")


def capital(text):
    return text[:1].upper() + text[1:]


def hosts(rng, count):
    """Third-person past narration paragraphs from the books with no quotes and no voices."""
    candidates = []
    for source in BOOKS:
        for name, document in source_documents(source).items():
            for paragraph in document.text.split('\n'):
                words = paragraph.split()
                if not 15 <= len(words) <= 70 or any(q in paragraph for q in '“”"‘—'):
                    continue
                lowered = {w.strip('.,;:!?').lower() for w in words}
                if lowered & {'i', 'we', 'you', 'my', 'me', 'our', 'us', 'your', 'i’m', 'i’d'}:
                    continue
                if not lowered & {'he', 'she', 'his', 'her'} or not paragraph.rstrip().endswith('.'):
                    continue
                candidates.append(paragraph.strip())
    rng.shuffle(candidates)
    candidates = candidates[:count * 3]
    results = probe([{'text': text} for text in candidates])
    clean = [text for text, result in zip(candidates, results)
             if not result['voices']['spans'] and result['stats']['sentences']['dialogue'] == 0]
    return clean[:count]


def tag_text(subject, verb, gender, leading):
    pronoun = 'herself' if gender == 'feminine' else 'himself'
    verb = verb.replace('herself', pronoun).replace('himself', pronoun)
    return f'{capital(subject) if leading else subject} {verb}'


def synthetic_items(templates, rng):
    clauses = templates['clauses']
    subjects = [(s, g) for group in templates['subjects'].values() for s, g in group]
    thought_verbs, speech_verbs = templates['thoughtVerbs'], templates['speechVerbs']
    items = []

    def add(kind, position, text, expected, voice, separator, apostrophe):
        items.append({'kind': kind, 'position': position, 'text': text, 'expected': expected,
                      'voice': voice, 'separator': separator, 'apostrophe': apostrophe})

    for _ in range(80):
        for voice, verbs in (('thought', thought_verbs), ('speech', speech_verbs)):
            kinds = ['firstPersonPresent', 'imperative', 'thirdPersonPresent'] + \
                (['question'] if voice == 'thought' else [])
            kind = rng.choice(kinds)
            clause = rng.choice(clauses[kind])
            subject, gender = rng.choice(subjects)
            verb = rng.choice(verbs)
            apostrophe = rng.choice(['curly', 'straight'])
            fix = straight if apostrophe == 'straight' else (lambda t: t)
            # A trailing: clause, separator, tag.
            if kind == 'question':
                separator = '?'
                body = clause
                add(kind, 'trailing', fix(f'{body} {tag_text(subject, verb, gender, False)}.'),
                    [fix(body)], voice, separator, apostrophe)
            else:
                separator = rng.choice([',', '—', '…'] if voice == 'thought' else [',', '—'])
                gap = ' ' if separator == '…' else ('' if separator == '—' else ' ')
                text = f'{clause}{separator}{gap}{tag_text(subject, verb, gender, False)}.'
                expected = clause + ('…' if separator == '…' else '')
                add(kind, 'trailing', fix(text), [fix(expected)], voice, separator, apostrophe)
            # B bare after a split (thought only): question, space or U+2028, capitalised tag.
            if voice == 'thought':
                question = rng.choice(clauses['question'])
                separator = rng.choice([' ', ' '])
                add('question', 'bare', fix(f'{question}{separator}{tag_text(subject, verb, gender, True)}.'),
                    [fix(question)], voice, 'U+2028' if separator != ' ' else 'space', apostrophe)
            # C leading: tag, comma or colon, clause.
            lead_kind = rng.choice(['firstPersonPresent', 'imperative'])
            clause = rng.choice(clauses[lead_kind])
            separator = rng.choice([',', ':'])
            text = f'{tag_text(subject, verb, gender, True)}{separator} {clause}.'
            add(lead_kind, 'leading', fix(text), [fix(clause + '.')], voice, separator, apostrophe)
            # D mid: clause, dash or comma, tag, dash or comma, clause.
            first, second = rng.choice(templates['split'])
            separator = rng.choice([',', '—'])
            gap = ' ' if separator == ',' else ''
            text = f'{first}{separator}{gap}{tag_text(subject, verb, gender, False)}{separator}{gap}{second}.'
            add('split', 'mid', fix(text), [fix(first), fix(second + '.')], voice, separator, apostrophe)
    # Person and tense paths: an I-tag on a third-person present clause (tense only), a she-tag on a
    # first-person past clause (person only).
    for clause in clauses['thirdPersonPresent']:
        add('thirdPersonPresent', 'swapTense', f'{clause}, I thought.', [clause], 'thought', ',', 'curly')
    for clause in clauses['firstPersonPast']:
        add('firstPersonPast', 'swapPerson', f'{clause}, she thought.', [clause], 'thought', ',', 'curly')
    for clause in clauses['firstPersonPast']:
        add('firstPersonPast', 'agreeingFirstPerson', f'{clause}, I thought.', [], 'thought', ',', 'curly')
    # Negatives.
    for kind, texts in templates['negatives'].items():
        for text in texts:
            add(kind, 'negative', text, [], None, None, 'curly')
            if '’' in text:
                add(kind, 'negative', straight(text), [], None, None, 'straight')
    # The guard: a plain present declarative before a tagged question stays narration.
    guard = templates['guardSentence']
    for question in clauses['question']:
        add('question', 'guard', f'{guard} {question} I wondered.', [question], 'thought', 'space', 'curly')
    return items


def synthetic(write=False):
    templates = json.loads(TEMPLATES.read_text(encoding='utf-8'))
    rng = random.Random(SEED)
    items = synthetic_items(templates, rng)
    paragraphs = hosts(rng, 60)
    inputs = []
    for index, item in enumerate(items):
        host = paragraphs[index % len(paragraphs)]
        # Paragraph-break negatives must keep their own breaks; the rest join the host paragraph.
        item['host'] = host
        item['document'] = host + ' ' + item['text']
        inputs.append({'text': item['document']})
    results = probe(inputs)
    by = defaultdict(Counter)
    false_positives = []
    for item, result in zip(items, results):
        offset = len(item['host']) + 1
        detected = [(char_range(s['span'])[0] - offset, char_range(s['span'])[1] - offset, s['voice'])
                    for s in result['voices']['spans'] if s['source'] == 'tag']
        expected = []
        for text in item['expected']:
            start = item['text'].index(text)
            expected.append((start, start + len(text)))
        group = f"{item['voice']}/{item['position']}" if item['voice'] else 'negative'
        counter = by[group]
        counter['items'] += 1
        hit = expected and all(any(d[0] < e[1] and e[0] < d[1] and d[2] == item['voice'] for d in detected)
                               for e in expected)
        exact = expected and all(any((d[0], d[1]) == e and d[2] == item['voice'] for d in detected)
                                 for e in expected)
        extra = [d for d in detected if not any(d[0] < e[1] and e[0] < d[1] for e in expected)
                 or d[2] != item['voice']]
        counter['recalled'] += bool(hit)
        counter['exact'] += bool(exact)
        counter['falsePositives'] += len(extra)
        for category in ('separator', 'apostrophe', 'kind'):
            if item[category] and item['voice']:
                by[f"{item['voice']}/{category}/{item[category]}"]['items'] += 1
                by[f"{item['voice']}/{category}/{item[category]}"]['recalled'] += bool(hit)
        if extra or (expected and not exact):
            false_positives.append({'group': group, 'text': item['text'],
                                    'detected': [item['text'][max(0, d[0]):d[1]] for d in detected],
                                    'expected': item['expected']})
    summary = {}
    for group, counter in sorted(by.items()):
        row = {'items': counter['items']}
        if group != 'negative' and '/' in group and group.count('/') == 1 and \
                not group.endswith(('agreeingFirstPerson',)):
            row['recall'] = round(counter['recalled'] / counter['items'], 3)
            row['exact'] = round(counter['exact'] / counter['items'], 3)
        elif group.count('/') == 2:
            row['recall'] = round(counter['recalled'] / counter['items'], 3)
        if 'falsePositives' in counter or group == 'negative' or group.endswith('agreeingFirstPerson'):
            row['falsePositives'] = counter['falsePositives']
        summary[group] = row
    total_fp = sum(c['falsePositives'] for c in by.values())
    result = {'seed': SEED, 'items': len(items), 'hosts': len(paragraphs),
              'falsePositives': total_fp, 'groups': summary}
    return result, false_positives


def gate_synthetic(result):
    failures = []
    if result['falsePositives']:
        failures.append(f"synthetic false positives {result['falsePositives']} != 0")
    for group, row in result['groups'].items():
        if group.count('/') != 1 or 'recall' not in row:
            continue
        voice, position = group.split('/')
        if position in ('trailing', 'bare', 'leading', 'mid'):
            target = 0.90 if voice == 'thought' else 0.85
            if row['recall'] < target:
                failures.append(f'{group} recall {row["recall"]} < {target}')
            if voice == 'thought' and row['exact'] < 0.85:
                failures.append(f'{group} exact {row["exact"]} < 0.85')
    return failures


def score(gate, write):
    synthetic_result, misses_found = synthetic()
    rows = flags()
    current = {key(row): row for row in rows}
    result = {
        'synthetic': synthetic_result,
        'natural': {
            'flags': len(rows),
            'flagsByVoice': dict(Counter(row['voice'] for row in rows)),
            'flagsByAnchor': dict(Counter(f"{row['voice']}/{row['anchor']}" for row in rows)),
            'flagsBySource': dict(Counter(row['source'] for row in rows)),
            'development': precision(LABELS, current) if LABELS.exists() else None,
            'heldout': precision(HELDOUT, current) if HELDOUT.exists() else None,
            'heldout2': precision(HELDOUT2, current) if HELDOUT2.exists() else None,
        },
        'misses': miss_estimate() if MISSES.exists() else None,
        'narrationDelta': narration_delta(),
    }
    print(json.dumps(result, indent=1, ensure_ascii=False))
    if misses_found:
        print(json.dumps({'syntheticFailures': misses_found[:40]}, indent=1, ensure_ascii=False),
              file=sys.stderr)
    if write:
        BASELINE.write_text(json.dumps(result, indent=1, ensure_ascii=False) + '\n', encoding='utf-8')
    if gate:
        failures = gate_synthetic(synthetic_result) + gate_natural(result['natural']['heldout2'] or {})
        if failures:
            sys.exit('; '.join(failures))


def gate_natural(heldout):
    """Held-out precision >= 0.90 per voice; Wilson 95% lower bound >= 0.82 pooled and for each
    voice with at least 30 rows. The public sources hold too little tagged unquoted thought for a
    per-voice bound: 13 of 13 correct gives a lower bound of only 0.772 (see STYLE-VOICES.md)."""
    failures = []
    for group in ('all', 'thought', 'speech'):
        row = heldout.get(group)
        if not row or row['precision'] is None:
            failures.append(f'no held-out {group} rows')
            continue
        if row['precision'] < 0.90:
            failures.append(f"held-out {group} precision {row['precision']} < 0.90")
        if (group == 'all' or row['kept'] >= 30) and row['wilsonLower'] < 0.82:
            failures.append(f"held-out {group} Wilson lower bound {row['wilsonLower']} < 0.82")
    return failures


def miss_estimate():
    rows = [row for row in jsonl(MISSES) if row.get('type') != 'meta']
    if any(row['label'] is None for row in rows):
        return {'labeled': 0, 'unlabeled': len(rows)}
    missed = sum(1 for row in rows if row['label'])
    return {'labeled': len(rows), 'missedDirectThought': missed,
            'missRate': round(missed / len(rows), 3) if rows else None}


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('synthetic').add_argument('--write', action='store_true')
    sub.add_parser('sample').add_argument('--size', type=int, default=150)
    sub.add_parser('heldout').add_argument('--size', type=int, default=100)
    sub.add_parser('heldout2').add_argument('--size', type=int, default=100)
    sub.add_parser('misses').add_argument('--size', type=int, default=100)
    packing = sub.add_parser('packet')
    packing.add_argument('labels')
    packing.add_argument('out')
    merging = sub.add_parser('merge')
    merging.add_argument('labels')
    merging.add_argument('first')
    merging.add_argument('second')
    merging.add_argument('--adjudicate')
    merging.add_argument('--meta')
    scoring = sub.add_parser('score')
    scoring.add_argument('--gate', action='store_true')
    scoring.add_argument('--write', action='store_true')
    args = parser.parse_args()
    if args.command == 'synthetic':
        result, failures = synthetic()
        print(json.dumps(result, indent=1))
        print(json.dumps(failures[:60], indent=1, ensure_ascii=False), file=sys.stderr)
        if failures_list := gate_synthetic(result):
            print('gate: ' + '; '.join(failures_list), file=sys.stderr)
    elif args.command == 'sample':
        sample(args.size, LABELS, SEED, half=True)
    elif args.command == 'heldout':
        sample(args.size, HELDOUT, HELDOUT_SEED, jsonl(LABELS))
    elif args.command == 'heldout2':
        sample(args.size, HELDOUT2, HELDOUT2_SEED, sources=FRESH)
    elif args.command == 'misses':
        misses(args.size)
    elif args.command == 'packet':
        packet(args.labels, args.out)
    elif args.command == 'merge':
        merge(HERE / 'labels' / args.labels if '/' not in args.labels else args.labels,
              args.first, args.second, args.adjudicate, args.meta)
    else:
        score(args.gate, args.write)


if __name__ == '__main__':
    main()
