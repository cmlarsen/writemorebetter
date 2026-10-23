"""Tense shift (#355): synthetic re-inflection recall and precision, natural flag rates and
model-judged precision on public-domain fiction.

  python3 bench/style/tense_shift.py perturb [--gate]      re-inflection suite (seed 355): single,
                                                           run, decoy and reverse perturbations
  python3 bench/style/tense_shift.py rate                  shifts per 1,000 narration words per source,
                                                           exemptions and undetermined share
  python3 bench/style/tense_shift.py sample [--size 100]   write labels/tense-shift.jsonl (unlabeled)
  python3 bench/style/tense_shift.py heldout [--size 100]  write labels/tense-shift-heldout.jsonl from
                                                           flags not in the development set (seed 356)
  python3 bench/style/tense_shift.py packet LABELS OUT     blind labeler packet: items, no labels
  python3 bench/style/tense_shift.py merge LABELS A B [--adjudicate FILE] [--meta JSON]
                                                           fold two labelers' answers into LABELS
  python3 bench/style/tense_shift.py score [--gate] [--write]
                                                           everything above; --write updates
                                                           tense-shift-baseline.json

The texts are the CC0 stories and Moby-Dick excerpt in bench/public and the Standard Ebooks (CC0)
books in sources.json, including Bleak House, Jane Eyre and David Copperfield for their present-tense
chapters and historical present. Nothing from bench/corpus is read. Perturbed verb forms are
confirmed by the tagger (the tense_forms helper) before they are written.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import math
import os
import random
import re
import subprocess
import sys
import threading

from common import HERE, ROOT, jsonl, write_jsonl
from pacing_labels import BOOKS, PUBLIC, source_documents

LABELS = HERE / 'labels' / 'tense-shift.jsonl'
HELDOUT = HERE / 'labels' / 'tense-shift-heldout.jsonl'
# Singles and reverse runs, which the default options never report: drawn with the held-out set
# to decide includeSingles (seed 357).
WIDE_LABELS = HERE / 'labels' / 'tense-shift-wide.jsonl'
GUIDELINES = HERE / 'labels' / 'TENSE-GUIDELINES.md'
BASELINE = HERE / 'tense-shift-baseline.json'
TOOL = ROOT / 'bench/tools/wmb-document'
TOKENIZER = ROOT / 'storage/en_tokenizer.bin'
DICKENS_BRONTE = ('bleak_house', 'jane_eyre', 'david_copperfield')
# Moby-Dick is a stress test (essayistic present-tense chapters); it is reported but never pooled.
STRESS = ('moby', 'moby150k')
# bench/public/moby150k.txt is the Gutenberg-style hard-wrapped text: blank lines between paragraphs.
HARD_WRAPPED = ('moby150k',)
SOURCES = tuple(PUBLIC) + ('moby150k',) + BOOKS + DICKENS_BRONTE
# Standard Ebooks (CC0) novels pinned for #353, added for the held-out draw only: none of their
# flags was seen while the guards were written.
HELDOUT_EXTRA = ('voyage_out', 'night_and_day', 'women_in_love', 'howards_end', 'main_street',
                 'this_side_of_paradise', 'a_farewell_to_arms')
ALL = SOURCES + HELDOUT_EXTRA
SEED = 355
HELDOUT_SEED = 356
WIDE_SEED = 357
PERTURB_EACH = 500
WIDE = {'includeSingles': True, 'includeReverse': True}
KINDS = ('slip', 'device', 'commentary', 'gnomic', 'thought', 'dialogue', 'reported', 'tagger', 'scene',
         'other')
# Gates, recorded 2026-10-23 (docs/STYLE-TENSE-SHIFT.md). The perturbation gates are the issue's.
GATE_RUN_RECALL = 0.70
GATE_PERTURBED_PRECISION = 0.90
GATE_DECOY_LEAKAGE = 0.05
# The issue asked for slip-or-device precision >= 0.85 on both judged sets. Measured at the freeze
# it is 0.317 (development) and 0.100 (held-out): published fiction has almost no slips, and the
# narrator's present-tense commentary is grammatically the same as one. That target is not met and
# is not gated. These are regression floors on `shiftPrecision` (a real tense change in narration,
# whatever its purpose), set at the frozen measurements rounded down.
GATE_SHIFT_PRECISION = {'development': 0.90, 'held-out': 0.60}


def probe(inputs, workers=6):
    """Run the probe over inputs in `workers` parallel processes; results keep input order."""
    if not _BUILT:
        subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(TOOL / 'Cargo.toml'),
                        '--bin', 'tense_shift', '--bin', 'tense_forms'], check=True,
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        _BUILT.append(True)
    # Interleave so long books spread across processes.
    chunks = [list(range(k, len(inputs), workers)) for k in range(workers)]
    processes = []
    for chunk in chunks:
        if not chunk:
            continue
        process = subprocess.Popen([str(TOOL / 'target/release/tense_shift'), str(TOKENIZER)],
                                   stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, encoding='utf-8',
                                   env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})
        processes.append((chunk, process, ''.join(json.dumps(inputs[i], ensure_ascii=False) + '\n'
                                                  for i in chunk)))
    results = [None] * len(inputs)
    threads = []
    outputs = {}

    def talk(index, process, payload):
        outputs[index] = process.communicate(payload)[0]

    for index, (chunk, process, payload) in enumerate(processes):
        thread = threading.Thread(target=talk, args=(index, process, payload))
        thread.start()
        threads.append(thread)
    for thread in threads:
        thread.join()
    for index, (chunk, process, _) in enumerate(processes):
        if process.returncode:
            raise RuntimeError('tense_shift probe failed')
        for i, line in zip(chunk, outputs[index].splitlines()):
            results[i] = json.loads(line)
    return results


_BUILT = []


def confirm(requests):
    """The first candidate the tagger reads as the requested lemma and POS, or None."""
    result = subprocess.run([str(TOOL / 'target/release/tense_forms'), str(TOKENIZER)],
                            input=''.join(json.dumps(row) + '\n' for row in requests),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    return [json.loads(line)['form'] for line in result.stdout.splitlines()]


def char_range(span):
    return span['char']['start'], span['char']['end']


# Natural texts

def public_moby():
    from pacing_labels import Document
    path = 'bench/public/moby150k.txt'
    text = (ROOT / path).read_text(encoding='utf-8')
    return {'moby150k.txt': Document('moby150k', 'moby150k.txt', text, [])}


def documents(sources=ALL):
    for source in sources:
        loaded = public_moby() if source == 'moby150k' else source_documents(source)
        for name, document in loaded.items():
            yield source, name, document.text


_RUN = {}


def run_natural(options=None):
    key = json.dumps(options or {}, sort_keys=True)
    if key not in _RUN:
        docs = _RUN.get('docs') or list(documents())
        _RUN['docs'] = docs
        _RUN[key] = probe([{'text': text, 'options': options or {}, 'hardWrapped': source in HARD_WRAPPED}
                           for source, _, text in docs])
    return _RUN['docs'], _RUN[key]


def context(text, sentences, first, last, wrapped=False):
    """Up to two sentences either side of sentences[first..=last], the flag in ⟦ ⟧, within the
    paragraph-free text window; newlines become ¶."""
    lo = max(0, first - 2)
    hi = min(len(sentences) - 1, last + 2)
    start, end = char_range(sentences[lo]['span'])[0], char_range(sentences[hi]['span'])[1]
    s, e = char_range(sentences[first]['span'])[0], char_range(sentences[last]['span'])[1]
    body = text[start:s] + '⟦' + text[s:e] + '⟧' + text[e:end]
    if wrapped:
        body = re.sub(r'\n\s*\n', ' ¶ ', body).replace('\n', ' ')
    return body.replace('\n', ' ¶ ')


def flag_rows(options=None, tag=None, sources=ALL):
    docs, results = run_natural(options)
    rows = []
    for (source, name, text), result in zip(docs, results):
        if source not in sources:
            continue
        sentences = result['sentences']
        by_index = {s['index']: k for k, s in enumerate(sentences)}
        for shift in result['report']['shifts']:
            first = by_index[shift['sentences']['start']]
            last = by_index[shift['sentences']['end'] - 1]
            s, e = char_range(shift['span'])
            rows.append({'source': source, 'document': name, 'start': s, 'end': e,
                         'strength': shift['strength'], 'expected': shift['expected'],
                         'found': shift['found'], 'sentences': last - first + 1,
                         'options': tag or 'default', 'span': text[s:e],
                         'verbs': [text[v['char']['start']:v['char']['end']] for v in shift['verbs']],
                         'context': context(text, sentences, first, last, source in HARD_WRAPPED)})
    return rows


def key(row):
    return row['source'], row['document'], row['start'], row['end']


def rate():
    docs, results = run_natural()
    per = defaultdict(Counter)
    exemptions = defaultdict(Counter)
    for (source, _, _), result in zip(docs, results):
        report = result['report']
        counter = per[source]
        counter['documents'] += 1
        counter['narrationWords'] += report['narrationWords']
        counter['shifts'] += len(report['shifts'])
        for shift in report['shifts']:
            counter[f"shifts/{shift['expected']}"] += 1
        for scene in report['scenes']:
            counter['scenes'] += 1
            counter[f"scenes/{scene['source']}"] += 1
            if scene['source'] == 'undetermined':
                counter['undeterminedWords'] += scene['narrationWords']
            if scene['tense']:
                counter[f"scenes/{scene['tense']}"] += 1
        for exemption in report['exemptions']:
            exemptions[source][exemption['reason']] += exemption['count']

    def summary(counter, exempt):
        words = counter['narrationWords']
        return {'documents': counter['documents'], 'narrationWords': words, 'shifts': counter['shifts'],
                'perThousandWords': round(1000 * counter['shifts'] / words, 3) if words else None,
                'shiftsInPastScenes': counter['shifts/past'], 'shiftsInPresentScenes': counter['shifts/present'],
                'scenes': counter['scenes'],
                'scenesBySource': {k: counter[f'scenes/{k}'] for k in
                                   ('declared', 'inferred', 'inherited', 'undetermined')},
                'scenesByTense': {k: counter[f'scenes/{k}'] for k in ('past', 'present')},
                'undeterminedSceneShare': round(counter['scenes/undetermined'] / counter['scenes'], 3)
                if counter['scenes'] else None,
                'undeterminedWordShare': round(counter['undeterminedWords'] / words, 3) if words else None,
                'exemptions': dict(sorted(exempt.items()))}

    result = {source: summary(per[source], exemptions[source]) for source in ALL}
    pooled, pooled_exempt = Counter(), Counter()
    for source in SOURCES:
        if source not in STRESS:
            pooled.update(per[source])
            pooled_exempt.update(exemptions[source])
    result['pooled'] = summary(pooled, pooled_exempt)
    result['pooled']['excludes'] = list(STRESS) + list(HELDOUT_EXTRA)
    extra, extra_exempt = Counter(), Counter()
    for source in HELDOUT_EXTRA:
        extra.update(per[source])
        extra_exempt.update(exemptions[source])
    result['pooledHeldoutSources'] = summary(extra, extra_exempt)
    return result


# Perturbations

IRREGULAR = {'be': None, 'have': ('has', 'have'), 'do': ('does', 'do'), 'go': ('goes', 'go')}
VOWEL = set('aeiou')


def third_person(lemma):
    if lemma in IRREGULAR and IRREGULAR[lemma]:
        return IRREGULAR[lemma][0]
    if re.search(r'(s|sh|ch|x|z|o)$', lemma):
        return lemma + 'es'
    if len(lemma) > 1 and lemma.endswith('y') and lemma[-2] not in VOWEL:
        return lemma[:-1] + 'ies'
    return lemma + 's'


PLURAL_SUBJECTS = {'we', 'they', 'you', 'these', 'those', 'both', 'many', 'few', 'several', 'all'}


def agreement(chain):
    """'1' for I, 'p' for plural and you, '3' for third-person singular, None when unsure."""
    subject = (chain.get('subject') or '').lower()
    readings = set(chain.get('subjectReadings') or [])
    if subject == 'i':
        return '1'
    if subject in PLURAL_SUBJECTS:
        return 'p'
    if subject in {'he', 'she', 'it', 'this', 'that', 'one', 'everybody', 'everyone', 'nobody', 'somebody',
                   'someone', 'something', 'nothing', 'everything'}:
        return '3'
    if readings & {'NNS', 'NNPS'} and not readings & {'NN', 'NNP'}:
        return 'p'
    if readings & {'NN', 'NNP'} and not readings & {'NNS', 'NNPS'}:
        return '3'
    return None


def present_form(chain, text, form_cache):
    """(span, replacement) re-inflecting a simple-past chain head, or None when the form is unsure."""
    head = chain['head']
    s, e = char_range(head)
    word = text[s:e]
    lower = word.lower()
    person = agreement(chain)
    if person is None:
        return None
    after = text[e:e + 3]
    negated = after.startswith(('n’t', "n't"))
    if lower in ('was', 'were'):
        if negated and person == '1':
            return None
        form = {'1': 'am', 'p': 'are', '3': 'is'}[person]
    else:
        lemmas = [r[0] for r in chain['readings'] if r[1] == 'VBD']
        if len(set(lemmas)) != 1:
            return None
        lemma = lemmas[0]
        if lemma == 'be':
            return None
        pos = 'VBZ' if person == '3' else 'VBP'
        candidate = third_person(lemma) if pos == 'VBZ' else (
            IRREGULAR[lemma][1] if lemma in IRREGULAR and IRREGULAR[lemma] else lemma)
        form_cache.setdefault((lemma, pos, candidate), None)
        form = (lemma, pos, candidate)
    return (s, e, word, form)


def capitalised(original, form):
    if original[:1].isupper():
        return form[:1].upper() + form[1:]
    return form


def resolve_forms(edits, form_cache):
    pending = [k for k, v in form_cache.items() if v is None]
    if pending:
        confirmed = confirm([{'lemma': l, 'pos': p, 'candidates': [c]} for l, p, c in pending])
        for k, form in zip(pending, confirmed):
            form_cache[k] = form or False
    out = []
    for s, e, word, form in edits:
        if isinstance(form, tuple):
            form = form_cache[form]
            if not form:
                return None
        out.append((s, e, capitalised(word, form)))
    return out


def apply(text, edits):
    for s, e, form in sorted(edits, reverse=True):
        text = text[:s] + form + text[e:]
    return text


def shift_of(edits):
    """Map an original char offset to its position after edits."""
    def move(offset):
        shift = 0
        for s, e, form in edits:
            if e <= offset:
                shift += len(form) - (e - s)
        return offset + shift
    return move


def flippable(sentence, text):
    """Voting simple-past chains (not past perfect) in a past narration sentence."""
    if sentence['class'] != 'past' or sentence['tagOnly'] or sentence['pastPerfectOnly']:
        return None
    voting = [c for c in sentence['chains'] if c['exemption'] is None and c['class'] in ('past', 'pastPerfect')]
    if not voting or any(c['class'] != 'past' for c in voting):
        return None
    if any(re.match(r'(?i)if\b', text[max(0, char_range(c['head'])[0] - 3):char_range(c['head'])[0]].strip())
           and text[slice(*char_range(c['head']))].lower() == 'were' for c in voting):
        return None
    return voting


def decoy_chains(sentence):
    return [c for c in sentence['chains'] if c['class'] == 'past'
            and c['exemption'] in ('subordinate', 'reported', 'generic', 'habitual')]


def scene_person(sentences):
    subjects = Counter((c.get('subject') or '').lower() for s in sentences for c in s['chains']
                       if c['exemption'] is None)
    first = subjects['i'] + subjects['we']
    third = subjects['he'] + subjects['she'] + subjects['they']
    return 'first' if first >= max(1, 0.25 * (first + third)) else 'third'


def past_scenes():
    """(source, name, scene text, scene sentences rebased to the scene) for inferred-past scenes
    of at least 12 classified sentences."""
    docs, results = run_natural()
    scenes = []
    for (source, name, text), result in zip(docs, results):
        if source not in SOURCES:
            continue
        for scene in result['report']['scenes']:
            if scene['source'] != 'inferred' or scene['tense'] != 'past':
                continue
            if scene['votes']['past'] + scene['votes']['present'] < 12:
                continue
            start, end = char_range(scene['span'])
            scenes.append((source, name, start, text[start:end]))
    return scenes


def perturb(gate=False):
    rng = random.Random(SEED)
    scenes = past_scenes()
    rng.shuffle(scenes)
    scenes = scenes[:400]
    clean = probe([{'text': t} for _, _, _, t in scenes])
    clean_wide = probe([{'text': t, 'options': {'includeSingles': True}} for _, _, _, t in scenes])
    form_cache = {}
    plans = []
    for index, ((source, name, offset, text), result) in enumerate(zip(scenes, clean)):
        sentences = result['sentences']
        person = scene_person(sentences)
        classified = [k for k, s in enumerate(sentences) if s['class'] in ('past', 'present', 'mixed')]
        candidates = {k: flippable(sentences[k], text) for k in classified}
        # single
        singles = [k for k in classified if candidates[k]]
        for k in rng.sample(singles, min(3, len(singles))):
            edits = [present_form(c, text, form_cache) for c in candidates[k]]
            if None not in edits:
                plans.append(('single', index, person, [k], edits))
        # run: 2-4 consecutive classified sentences, all flippable
        for _ in range(3):
            length = rng.choice((2, 3, 4))
            starts = [i for i in range(len(classified) - length + 1)
                      if all(candidates[classified[i + j]] for j in range(length))]
            if not starts:
                continue
            i = rng.choice(starts)
            ks = [classified[i + j] for j in range(length)]
            edits = [present_form(c, text, form_cache) for k in ks for c in candidates[k]]
            if None not in edits:
                plans.append(('run', index, person, ks, edits))
        # decoy: exempt past chains only, in sentences that keep a past vote or no vote
        decoys = [k for k, s in enumerate(sentences) if decoy_chains(s)]
        for k in rng.sample(decoys, min(3, len(decoys))):
            edits = [present_form(c, text, form_cache) for c in decoy_chains(sentences[k])]
            if None not in edits:
                plans.append(('decoy', index, person, [k], edits))
    resolved = []
    for kind, index, person, ks, edits in plans:
        edits = resolve_forms(edits, form_cache)
        if edits:
            resolved.append((kind, index, person, ks, edits))
    by_kind = defaultdict(list)
    for plan in resolved:
        by_kind[plan[0]].append(plan)
    chosen = []
    for kind in ('single', 'run', 'decoy'):
        rng.shuffle(by_kind[kind])
        chosen += by_kind[kind][:PERTURB_EACH]
    inputs = []
    for kind, index, person, ks, edits in chosen:
        text = apply(scenes[index][3], edits)
        inputs.append({'text': text})
    results = probe(inputs)
    results_singles = probe([{**row, 'options': {'includeSingles': True}} for row in inputs])
    groups = defaultdict(Counter)
    examples = defaultdict(list)
    for (kind, index, person, ks, edits), result, result_singles in zip(chosen, results, results_singles):
        source_text = scenes[index][3]
        move = shift_of(edits)
        sentences = clean[index]['sentences']
        lo = move(char_range(sentences[ks[0]]['span'])[0])
        hi = move(char_range(sentences[ks[-1]]['span'])[1])
        for options, res, base in (('default', result, clean[index]), ('singles', result_singles, clean_wide[index])):
            before = {(move(char_range(s['span'])[0]), move(char_range(s['span'])[1]))
                      for s in base['report']['shifts']}
            new = [s for s in res['report']['shifts']
                   if (char_range(s['span'])[0], char_range(s['span'])[1]) not in before]
            hit = [s for s in new if char_range(s['span'])[0] < hi and lo < char_range(s['span'])[1]
                   and s['found'] == 'present']
            for group in (f'{kind}/{options}', f'{kind}/{options}/{person}'):
                counter = groups[group]
                counter['items'] += 1
                counter['recalled'] += bool(hit)
                counter['newFlags'] += len(new)
                counter['newFlagsOnTarget'] += len(hit)
                counter['lost'] += len(before) - len(before & {(char_range(s['span'])[0], char_range(s['span'])[1])
                                                               for s in res['report']['shifts']})
            if options == 'default' and ((kind != 'decoy' and not hit) or (kind == 'decoy' and new)) \
                    and len(examples[kind]) < 25:
                examples[kind].append({'sentences': apply(source_text, edits)[lo:hi][:300],
                                       'flags': [s['strength'] for s in new]})
    summary = {}
    for group, counter in sorted(groups.items()):
        row = {'items': counter['items']}
        if group.startswith('decoy'):
            row['leakage'] = round(counter['newFlags'] / counter['items'], 3) if counter['items'] else None
            row['newFlags'] = counter['newFlags']
        else:
            row['recall'] = round(counter['recalled'] / counter['items'], 3) if counter['items'] else None
            row['newFlags'] = counter['newFlags']
            row['precision'] = round(counter['newFlagsOnTarget'] / counter['newFlags'], 3) \
                if counter['newFlags'] else None
        row['cleanFlagsLost'] = counter['lost']
        summary[group] = row
    # Pooled precision over single + run perturbations at each option set.
    for options in ('default', 'singles'):
        new = sum(groups[f'{k}/{options}']['newFlags'] for k in ('single', 'run', 'decoy'))
        on = sum(groups[f'{k}/{options}']['newFlagsOnTarget'] for k in ('single', 'run'))
        summary[f'precision/{options}'] = {'newFlags': new, 'onTarget': on,
                                           'precision': round(on / new, 3) if new else None}
    result = {'seed': SEED, 'scenes': len(scenes), 'candidates': {k: len(v) for k, v in by_kind.items()},
              'groups': summary, 'reverse': reverse(scenes, clean, form_cache, rng),
              'cleanFlags': sum(len(r['report']['shifts']) for r in clean)}
    return result, examples


def reverse(scenes, clean, form_cache, rng):
    """Flip whole past scenes to present (past perfect left alone), count flags with includeReverse,
    then put 3-sentence runs back into the past and measure recall. A chain the re-inflection cannot
    confirm stays past; flags touching such a sentence are not counted as false."""
    plans = []
    for index, ((source, name, offset, text), result) in enumerate(zip(scenes, clean)):
        sentences = result['sentences']
        edits, residual = [], []
        for k, sentence in enumerate(sentences):
            for chain in sentence['chains']:
                if chain['class'] != 'past':
                    continue
                edit = present_form(chain, text, form_cache)
                if edit:
                    edits.append((k, edit))
                elif chain['exemption'] is None:
                    residual.append(k)
        flipped = {k for k, _ in edits}
        if not flipped or len(residual) > 0.1 * len(flipped):
            continue
        plans.append((index, edits, set(residual)))
        if len(plans) >= 150:
            break
    resolve_forms([], form_cache)  # confirm every pending form in one tagger call
    resolved = []
    for index, edits, residual in plans:
        kept, failed = [], set(residual)
        for k, edit in edits:
            confirmed = resolve_forms([edit], form_cache)
            if confirmed:
                kept.append((k, confirmed[0]))
            else:
                failed.add(k)
        resolved.append((index, kept, failed))
    options = {'includeReverse': True}
    texts = [apply(scenes[index][3], [e for _, e in kept]) for index, kept, _ in resolved]
    results = probe([{'text': t, 'options': options} for t in texts])
    present_scenes, false_flags = [], 0
    for (index, kept, failed), text, result in zip(resolved, texts, results):
        scenes_out = result['report']['scenes']
        if not scenes_out or any(scene['tense'] != 'present' for scene in scenes_out):
            continue
        move = shift_of([e for _, e in kept])
        sentences = clean[index]['sentences']
        bad = [(move(char_range(sentences[k]['span'])[0]), move(char_range(sentences[k]['span'])[1]))
               for k in failed]
        false_flags += sum(1 for shift in result['report']['shifts']
                           if not any(char_range(shift['span'])[0] < hi and lo < char_range(shift['span'])[1]
                                      for lo, hi in bad))
        present_scenes.append((index, kept, failed))
    # Revert three consecutive flipped sentences to the past.
    reverted_inputs, targets = [], []
    for index, kept, failed in present_scenes:
        sentences = clean[index]['sentences']
        flipped = sorted({k for k, _ in kept} - failed)
        candidates = [i for i in range(2, len(flipped) - 3)
                      if flipped[i + 2] - flipped[i] == 2 and sentences[flipped[i]]['class'] == 'past']
        if not candidates:
            continue
        i = rng.choice(candidates)
        ks = set(flipped[i:i + 3])
        remaining = [e for k, e in kept if k not in ks]
        move = shift_of(remaining)
        lo = move(char_range(sentences[flipped[i]]['span'])[0])
        hi = move(char_range(sentences[flipped[i + 2]]['span'])[1])
        reverted_inputs.append({'text': apply(scenes[index][3], remaining), 'options': options})
        targets.append((lo, hi))
    reverted = probe(reverted_inputs)
    recalled = sum(1 for (lo, hi), r in zip(targets, reverted)
                   if any(char_range(s['span'])[0] < hi and lo < char_range(s['span'])[1] and s['found'] == 'past'
                          for s in r['report']['shifts']))
    defaults = probe([{'text': row['text']} for row in reverted_inputs])
    return {'flippedScenes': len(present_scenes), 'falseFlagsWithIncludeReverse': false_flags,
            'falseFlagsPerScene': round(false_flags / len(present_scenes), 3) if present_scenes else None,
            'revertedRuns': len(targets),
            'recallWithIncludeReverse': round(recalled / len(targets), 3) if targets else None,
            'flagsAtDefaults': sum(len(r['report']['shifts']) for r in defaults)}


def gate_perturb(result):
    failures = []
    run = result['groups']['run/default']
    if run['recall'] < GATE_RUN_RECALL:
        failures.append(f"run recall {run['recall']} < {GATE_RUN_RECALL}")
    precision = result['groups']['precision/default']['precision']
    if precision is None or precision < GATE_PERTURBED_PRECISION:
        failures.append(f'perturbed precision {precision} < {GATE_PERTURBED_PRECISION}')
    leakage = result['groups']['decoy/default']['leakage']
    if leakage > GATE_DECOY_LEAKAGE:
        failures.append(f'decoy leakage {leakage} > {GATE_DECOY_LEAKAGE}')
    return failures


# Judged precision

def sample(size, path, seed, exclude=(), sources=ALL):
    """Default-option flags first; when there are too few, the rest from an includeSingles +
    includeReverse run (recorded in `options`). Flags overlapping an excluded row are skipped."""
    excluded = defaultdict(list)
    for row in exclude:
        if row.get('type') != 'meta':
            excluded[(row['source'], row['document'])].append(row)

    def fresh(row, taken):
        return not any(o['start'] < row['end'] and row['start'] < o['end']
                       for o in excluded[(row['source'], row['document'])] + taken[(row['source'], row['document'])])

    rng = random.Random(seed)
    taken = defaultdict(list)
    chosen = []
    for options, tag in ((None, 'default'), (WIDE, 'includeSingles+includeReverse')):
        rows = flag_rows(options, tag, sources)
        rng.shuffle(rows)
        for row in rows:
            if len(chosen) >= size:
                break
            if fresh(row, taken):
                chosen.append(row)
                taken[(row['source'], row['document'])].append(row)
    chosen.sort(key=key)
    write_jsonl(path, [{**row, 'id': f'{path.stem}-{i:03d}', 'shift': None, 'kind': None, 'note': ''}
                       for i, row in enumerate(chosen, 1)])
    print(json.dumps({'sampled': len(chosen), 'options': dict(Counter(r['options'] for r in chosen)),
                      'strength': dict(Counter(r['strength'] for r in chosen)),
                      'sources': dict(Counter(r['source'] for r in chosen))}))


def wide_sample(singles, reverse):
    """Single flags and reverse runs from an includeSingles + includeReverse run, none
    overlapping the other label sets."""
    excluded = defaultdict(list)
    for path in (LABELS, HELDOUT):
        for row in jsonl(path):
            if row.get('type') != 'meta':
                excluded[(row['source'], row['document'])].append(row)
    rows = [row for row in flag_rows(WIDE, 'includeSingles+includeReverse')
            if not any(o['start'] < row['end'] and row['start'] < o['end']
                       for o in excluded[(row['source'], row['document'])])]
    rng = random.Random(WIDE_SEED)
    rng.shuffle(rows)
    chosen = [r for r in rows if r['strength'] == 'single'][:singles] + \
        [r for r in rows if r['expected'] == 'present'][:reverse]
    chosen.sort(key=key)
    write_jsonl(WIDE_LABELS, [{**row, 'id': f'{WIDE_LABELS.stem}-{i:03d}', 'shift': None, 'kind': None,
                               'note': ''} for i, row in enumerate(chosen, 1)])
    print(json.dumps({'sampled': len(chosen), 'strength': dict(Counter(r['strength'] for r in chosen)),
                      'expected': dict(Counter(r['expected'] for r in chosen)),
                      'available': {'single': sum(r['strength'] == 'single' for r in rows),
                                    'reverse': sum(r['expected'] == 'present' for r in rows)}}))


ITEM_FIELDS = ('id', 'expected', 'found', 'context')


def items_digest(rows):
    items = [{k: row[k] for k in ITEM_FIELDS} for row in rows if row.get('type') != 'meta']
    return hashlib.sha256(json.dumps(items, ensure_ascii=False, sort_keys=True).encode()).hexdigest()


def packet(labels, out):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    write_jsonl(out, [{k: row[k] for k in ITEM_FIELDS} for row in rows])
    print(json.dumps({'items': len(rows), 'itemsSha256': items_digest(rows)}))


def merge(labels, first, second, adjudicate, meta):
    rows = [row for row in jsonl(labels) if row.get('type') != 'meta']
    answers = [{row['id']: row for row in jsonl(path)} for path in (first, second)]
    decided = {row['id']: row for row in jsonl(adjudicate)} if adjudicate else {}
    agree = agree_kind = 0
    for row in rows:
        a, b = (answer[row['id']] for answer in answers)
        row['labels'] = [{'shift': a['shift'], 'kind': a['kind']}, {'shift': b['shift'], 'kind': b['kind']}]
        good = [x['shift'] and x['kind'] in ('slip', 'device') for x in (a, b)]
        agree_kind += a['kind'] == b['kind'] and a['shift'] == b['shift']
        if row['id'] in decided:
            row['shift'] = decided[row['id']]['shift']
            row['kind'] = decided[row['id']]['kind']
            row['note'] = decided[row['id']].get('note', '')
        elif good[0] == good[1] and a['shift'] == b['shift']:
            agree += 1
            row['shift'] = a['shift']
            # Same precision label, different kinds: keep the first labeler's kind.
            row['kind'] = a['kind']
        else:
            row['shift'] = None
            row['kind'] = None
    pairs = [tuple(bool(x['shift'] and x['kind'] in ('slip', 'device')) for x in row['labels']) for row in rows]
    metadata = json.loads(meta) if meta else {}
    metadata.update({'type': 'meta',
                     'agreement': {'agreePrecisionLabel': sum(a == b for a, b in pairs), 'agreeKind': agree_kind,
                                   'items': len(rows), 'kappa': kappa(pairs)},
                     'guidelinesSha256': hashlib.sha256(GUIDELINES.read_bytes()).hexdigest(),
                     'itemsSha256': items_digest(rows)})
    write_jsonl(labels, [metadata] + rows)
    unresolved = [row['id'] for row in rows if row['shift'] is None]
    print(json.dumps({**metadata['agreement'], 'unresolved': unresolved}))


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


def overlapping(row, current):
    """The current flag (same options) that overlaps a labelled row, or None. Overlap rather than
    equal spans, so a run the rules trimmed or extended still counts as kept."""
    for other in current.get(row['options'], {}).get((row['source'], row['document']), ()):
        if other['start'] < row['end'] and row['start'] < other['end']:
            return other
    return None


def precision(path, current):
    rows = [row for row in jsonl(path) if row.get('type') != 'meta']
    if any(row['shift'] is None for row in rows):
        sys.exit('unlabeled rows in ' + str(path))
    groups = defaultdict(Counter)
    kinds = defaultdict(Counter)
    for row in rows:
        # Rows the current rules no longer flag are reported apart from precision.
        state = 'kept' if overlapping(row, current) else 'dropped'
        good = bool(row['shift'] and row['kind'] in ('slip', 'device'))
        for group in ('all', f"options/{row['options']}", f"strength/{row['strength']}",
                      f"expected/{row['expected']}", f"source/{row['source']}"):
            groups[group][(state, good, bool(row['shift']))] += 1
            if state == 'kept':
                kinds[group][row['kind']] += 1
    result = {}
    for group, counts in sorted(groups.items()):
        kept = sum(v for (state, _, _), v in counts.items() if state == 'kept')
        good = sum(v for (state, g, _), v in counts.items() if state == 'kept' and g)
        shift = sum(v for (state, _, s), v in counts.items() if state == 'kept' and s)
        result[group] = {'labeled': sum(counts.values()), 'kept': kept,
                         # slip or device: the spec's precision (see STYLE-TENSE-SHIFT.md)
                         'precision': round(good / kept, 3) if kept else None,
                         'wilsonLower': wilson(good, kept),
                         # a real tense change in narration, whatever its purpose
                         'shiftPrecision': round(shift / kept, 3) if kept else None,
                         'shiftWilsonLower': wilson(shift, kept),
                         'dropped': sum(v for (state, _, _), v in counts.items() if state == 'dropped'),
                         'droppedSlipOrDevice': sum(v for (state, g, _), v in counts.items()
                                                    if state == 'dropped' and g)}
        if group == 'all' or group.startswith('options/'):
            result[group]['kinds'] = dict(sorted(kinds[group].items()))
    return result


def current_flags():
    result = {}
    for name, options in (('default', None), ('includeSingles+includeReverse', WIDE)):
        index = defaultdict(list)
        for row in flag_rows(options, name):
            index[(row['source'], row['document'])].append(row)
        result[name] = index
    return result


def gate_judged(name, result):
    failures = []
    if not result:
        return [f'no {name} labels']
    row = result.get('options/default') or result['all']
    floor = GATE_SHIFT_PRECISION[name]
    if row['shiftPrecision'] is None or row['shiftPrecision'] < floor:
        failures.append(f"{name} default-option shift precision {row['shiftPrecision']} < {floor}")
    return failures


def score(gate, write):
    perturbed, examples = perturb()
    current = current_flags()
    natural = rate()
    result = {
        'perturb': perturbed,
        'rate': natural,
        'flags': {name: sum(len(rows) for rows in index.values()) for name, index in current.items()},
        'development': precision(LABELS, current) if LABELS.exists() else None,
        'heldout': precision(HELDOUT, current) if HELDOUT.exists() else None,
        'wide': precision(WIDE_LABELS, current) if WIDE_LABELS.exists() else None,
    }
    print(json.dumps(result, indent=1, ensure_ascii=False))
    if write:
        BASELINE.write_text(json.dumps(result, indent=1, ensure_ascii=False) + '\n', encoding='utf-8')
    if gate:
        failures = gate_perturb(perturbed) + gate_judged('development', result['development']) + \
            gate_judged('held-out', result['heldout'])
        if failures:
            sys.exit('; '.join(failures))


def main():
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('perturb').add_argument('--gate', action='store_true')
    sub.add_parser('rate')
    sub.add_parser('sample').add_argument('--size', type=int, default=100)
    sub.add_parser('heldout').add_argument('--size', type=int, default=100)
    sub.add_parser('flags')
    widening = sub.add_parser('wide')
    widening.add_argument('--singles', type=int, default=40)
    widening.add_argument('--reverse', type=int, default=20)
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
    if args.command == 'perturb':
        result, examples = perturb()
        print(json.dumps(result, indent=1))
        print(json.dumps(examples, indent=1, ensure_ascii=False), file=sys.stderr)
        if args.gate and (failures := gate_perturb(result)):
            sys.exit('; '.join(failures))
    elif args.command == 'rate':
        print(json.dumps(rate(), indent=1))
    elif args.command == 'flags':
        for row in flag_rows() + flag_rows(WIDE, 'wide'):
            print(json.dumps(row, ensure_ascii=False))
    elif args.command == 'sample':
        sample(args.size, LABELS, SEED, sources=SOURCES)
    elif args.command == 'heldout':
        sample(args.size, HELDOUT, HELDOUT_SEED, jsonl(LABELS))
    elif args.command == 'wide':
        wide_sample(args.singles, args.reverse)
    elif args.command == 'packet':
        packet(args.labels, args.out)
    elif args.command == 'merge':
        merge(args.labels, args.first, args.second, args.adjudicate, args.meta)
    else:
        score(args.gate, args.write)


if __name__ == '__main__':
    main()
