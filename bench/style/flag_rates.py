from collections import Counter, defaultdict
import json
import os
from pathlib import Path
import random
import resource
import subprocess
import sys
import tempfile
import time

from common import ROOT, jsonl

GROUPS = ('all', 'narration', 'dialogue')
PERCENTILES = (5, 10, 25, 50, 75, 90, 95)
LENGTH_STATISTICS = ('mean', '5', '25', '50', '75', '95')
LENGTH_SWEEP = (0.5, 1, 2, 3, 5)
STARTS_SWEEP = (1, 2, 5, 10)
WINDOW_RULE = {'scene_break_words': 3000, 'maximum_words': 6000, 'minimum_words': 2000}
TOP = 10
BASELINES = ROOT / 'style/data/baselines'
LENGTH_BASELINE = BASELINES / 'sentence-length.json'
STARTS_BASELINE = BASELINES / 'sentence-starts.json'
TAG_FLAGS = ('other_speech_verb', 'non_speech_verb', 'manner_adverb', 'untagged_run')
TAG_CONTEXT = ('tags', 'said_asked', 'unlisted_verb', 'beats')
STAGES = ('document', 'sentence_length', 'sentence_starts', 'echoes', 'repeated_phrases', 'cliches',
          'dialogue_tags', 'overused_words', 'sensory_balance', 'diction')
FINE = ('subject', 'adverb', 'gerund', 'infinitive', 'subordinatingConjunction', 'coordinatingConjunction',
        'pastParticiple', 'inversion', 'introductory', 'other')
UNIVERSAL = ('ADJ', 'ADP', 'ADV', 'CONJ', 'DET', 'NOUN', 'NUM', 'PRON', 'PRT', 'VERB', '.', 'X')
LEXICAL_KEYS = ('overused_words', 'sensory_balance', 'diction', 'overused', 'occurrences', 'fillers',
                'sense_words', 'modalities', 'sight', 'sound', 'touch', 'smell', 'taste',
                'interoception', 'abstract_sentences', 'abstract_runs', 'rated_words', 'rating_sum', 'mean_rating')
STRUCTURE = frozenset(LEXICAL_KEYS + GROUPS + LENGTH_STATISTICS + TAG_FLAGS + TAG_CONTEXT + STAGES + FINE + UNIVERSAL
                      + tuple(WINDOW_RULE) + (
    'units', 'authors', 'samples', 'words', 'mean', 'percentiles', 'run', 'seconds', 'probe_peak_rss_bytes',
    'harness_peak_rss_bytes', 'stage_seconds', 'defaults', 'window_rule', 'chapter', 'manuscript',
    'sample_words', 'reports', 'flags_per_1000_words', 'baseline_comparison', 'measures', 'categories',
    'patterns', 'top_categories', 'top_patterns', 'patterns_with_flags', 'flagged_share', 'per_1000_words',
    'difference', 'eligible_share', 'share_abs_difference_at_least', 'percent', 'universal', 'fine', 'total',
    'long_sentence', 'monotony_run', 'same_word_run', 'distance', 'form', 'length', 'kind', 'source', 'verb',
    'repeated_phrase', 'occurrence', 'context_per_1000_words', 'echo', 'cliche', 'redundancy', 'same_form',
    'different_form', '1_5', '6_10', '11_25', '26_50', 'over_50', 'sentence_length_minimum_sentences',
    'long_sentence_words', 'sentence_starts_minimum_sentences', 'echo_window', 'phrase_min_length',
    'phrase_max_length', 'phrase_min_content_words', 'max_untagged_exchanges', 'dialogue_tag_min_tags'))


class FlagProbe:
    def __init__(self, executable, tokenizer, diction_table):
        self.vocabulary = json.loads(subprocess.run(
            [str(executable), '--vocabulary', str(tokenizer)], capture_output=True, text=True,
            encoding='utf-8', check=True).stdout)
        self.process = subprocess.Popen(
            [str(executable), str(tokenizer), str(LENGTH_BASELINE), str(STARTS_BASELINE), str(diction_table)],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
            encoding='utf-8', env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})
        self.peak_rss_bytes = None

    def measure(self, text):
        self.process.stdin.write(json.dumps(text) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('flag-rate probe exited')
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        _, status, usage = os.wait4(self.process.pid, 0)
        self.process.returncode = os.waitstatus_to_exitcode(status)
        self.peak_rss_bytes = rss_bytes(usage.ru_maxrss)
        if self.process.returncode != 0:
            raise RuntimeError('flag-rate probe failed')


def probe_build(quiet=True):
    tool = ROOT / 'bench/tools/wmb-document'
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml')],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL if quiet else None)
    return tool / 'target/release/flag_rates'


def rss_bytes(value):
    return value if sys.platform == 'darwin' else value * 1024


def merge(target, source):
    for key, value in source.items():
        if isinstance(value, dict):
            merge(target.setdefault(key, {}), value)
        else:
            target[key] = target.get(key, 0) + value


def units(export):
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit_id, author = row['unit_id'], row['author_key']
        if not isinstance(unit_id, str) or Path(unit_id).name != unit_id or unit_id in ('.', '..'):
            raise ValueError('invalid unit identifier')
        if not isinstance(author, str) or not author:
            raise ValueError('invalid author key')
        if unit_id in seen:
            raise ValueError('duplicate unit')
        seen.add(unit_id)
        yield author, (export / 'units' / (unit_id + '.txt')).read_text(encoding='utf-8')


def window_words(paragraph):
    return sum(any(ch.isalnum() for ch in word) for word in paragraph.split())


def windows(text, rule=WINDOW_RULE):
    result, current, words, scene_break = [], [], 0, False
    for line in text.splitlines():
        if not line.strip():
            scene_break = True
            continue
        if current and (words >= rule['maximum_words'] or (scene_break and words >= rule['scene_break_words'])):
            result.append((current, words))
            current, words = [], 0
        current.append((scene_break, line))
        words += window_words(line)
        scene_break = False
    if current:
        if result and words < rule['minimum_words']:
            previous, total = result.pop()
            current, words = previous + current, words + total
        result.append((current, words))
    return ['\n'.join(('\n' if index and scene else '') + line for index, (scene, line) in enumerate(window)) + '\n'
            for window, _ in result]


def measure(probe, text, stages):
    unit = probe.measure(text)
    for stage, milliseconds in unit.pop('timing_ms').items():
        stages[stage] += milliseconds / 1000
    unit.pop('unit_report')
    return unit


def collect(probe, export):
    pooled = defaultdict(dict)
    chapters = []
    stages = defaultdict(float)
    count = 0
    for author, text in units(export):
        merge(pooled[author], measure(probe, text, stages))
        chapters.extend((author, measure(probe, window, stages)) for window in windows(text))
        count += 1
    return count, list(pooled.items()), chapters, dict(stages)


def weighted(values):
    per_author = Counter(author for author, _ in values)
    return sorted((value, 1 / per_author[author]) for author, value in values)


def weighted_percentile(pairs, percent):
    if len(pairs) == 1:
        return pairs[0][0]
    total = sum(weight for _, weight in pairs)
    span = total - pairs[0][1] / 2 - pairs[-1][1] / 2
    positions, seen = [], 0.0
    for _, weight in pairs:
        seen += weight
        positions.append((seen - weight / 2 - pairs[0][1] / 2) / span)
    target = percent / 100
    for index in range(len(pairs) - 1):
        low, high = positions[index], positions[index + 1]
        if target <= high:
            fraction = (target - low) / (high - low) if high > low else 0.0
            return pairs[index][0] + (pairs[index + 1][0] - pairs[index][0]) * fraction
    return pairs[-1][0]


def spread(values):
    if not values:
        return None
    pairs = weighted(values)
    total = sum(weight for _, weight in pairs)
    return {'samples': len(values), 'authors': len({author for author, _ in values}),
            'mean': sum(value * weight for value, weight in pairs) / total,
            'percentiles': {str(p): weighted_percentile(pairs, p) for p in PERCENTILES}}


def share(values):
    if not values:
        return None
    pairs = weighted(values)
    return sum(value * weight for value, weight in pairs) / sum(weight for _, weight in pairs)


def category(samples, count, words):
    eligible = [(author, data) for author, data in samples if words(data)]
    return {'flagged_share': share([(author, float(count(data) > 0)) for author, data in eligible]),
            'per_1000_words': spread([(author, 1000 * count(data) / words(data)) for author, data in eligible])}


def ranked(categories):
    rates = {name: stats['per_1000_words']['mean'] for name, stats in categories.items()
             if stats['per_1000_words'] and stats['per_1000_words']['mean'] > 0}
    order = sorted(rates, key=lambda name: (-rates[name], name))[:TOP]
    return {name: rank for rank, name in enumerate(order, 1)}


def get(value, *path):
    for key in path:
        value = value.get(key, {}) if isinstance(value, dict) else {}
    return value if isinstance(value, (int, float)) else 0


def histogram_words(histogram):
    return sum(int(length) * count for length, count in histogram.items())


def histogram_statistics(histogram):
    items = sorted((int(length), count) for length, count in histogram.items() if count)
    total = sum(count for _, count in items)
    if not total:
        return None

    def at(rank):
        seen = 0
        for length, count in items:
            seen += count
            if rank < seen:
                return length
        return items[-1][0]

    result = {'mean': sum(length * count for length, count in items) / total}
    for p in (5, 25, 50, 75, 95):
        index = (total - 1) * p / 100
        low = int(index)
        high = min(low + 1, total - 1)
        result[str(p)] = at(low) + (at(high) - at(low)) * (index - low)
    return result


def length_measures(data, group, minimum):
    histogram = data.get('sentence_length', {}).get(group, {}).get('histogram', {})
    if sum(histogram.values()) < max(minimum, 1):
        return None
    return histogram_statistics(histogram)


def length_differences(data, group, baseline, minimum):
    measured = length_measures(data, group, minimum)
    if measured is None:
        return None
    reference = {'mean': baseline.get('mean'), **{str(k): v for k, v in baseline.get('percentiles', {}).items()}}
    return {name: measured[name] - reference[name] for name in LENGTH_STATISTICS if reference.get(name) is not None}


def starts_percentages(data, group, minimum):
    starts = data.get('sentence_starts', {}).get(group, {})
    sentences = starts.get('sentences', 0)
    if sentences < max(minimum, 1):
        return None
    known = sentences - starts.get('unknown_starts', 0)
    return {'fine': {name: 100 * count / sentences for name, count in starts.get('categories', {}).items()},
            'universal': {tag: 100 * count / known for tag, count in starts.get('universal', {}).items()}
            if known else {}}


def comparison(present, sweep):
    differences = [(author, value) for author, value in present if value is not None]
    return {'eligible_share': share([(author, float(value is not None)) for author, value in present]),
            'difference': spread(differences),
            'share_abs_difference_at_least': {
                str(t): share([(author, float(value is not None and abs(value) >= t)) for author, value in present])
                for t in sweep}}


def group_words(data, group):
    return histogram_words(data.get('sentence_length', {}).get(group, {}).get('histogram', {}))


def sentence_length(samples, baseline, defaults):
    minimum = defaults['sentence_length_minimum_sentences']
    flags, comparisons, measures = {}, {}, {}
    for group in GROUPS:
        words = lambda d, group=group: group_words(d, group)
        long = lambda d, group=group: get(d, 'sentence_length', group, 'long_sentences')
        runs = lambda d, group=group: get(d, 'sentence_length', group, 'monotony_runs')
        flags[group] = {'total': category(samples, lambda d: long(d) + runs(d), words),
                        'long_sentence': category(samples, long, words),
                        'monotony_run': category(samples, runs, words)}
        differences = [(author, length_differences(data, group, baseline, minimum) or {}) for author, data in samples]
        comparisons[group] = {name: comparison([(author, value.get(name)) for author, value in differences],
                                               LENGTH_SWEEP) for name in LENGTH_STATISTICS}
        values = [(author, length_measures(data, group, minimum)) for author, data in samples]
        measures[group] = {name: spread([(author, value[name]) for author, value in values if value])
                           for name in LENGTH_STATISTICS}
    return {'flags_per_1000_words': flags, 'baseline_comparison': comparisons, 'measures': measures}


def sentence_starts(samples, baseline, defaults):
    minimum = defaults['sentence_starts_minimum_sentences']
    flags = {group: {'same_word_run': category(
        samples, lambda d, group=group: get(d, 'sentence_starts', group, 'same_word_runs'),
        lambda d, group=group: group_words(d, group))} for group in GROUPS}
    comparisons, measures = {}, {}
    for group in GROUPS:
        shares = [(author, starts_percentages(data, group, minimum)) for author, data in samples]
        comparisons[group] = {kind: {
            name: comparison([(author, value[kind][name] - reference if value and name in value[kind] else None)
                              for author, value in shares], STARTS_SWEEP)
            for name, reference in baseline.get(key, {}).items()}
            for kind, key in (('universal', 'universalPercent'), ('fine', 'categoryPercent'))}
        measures[group] = {kind: {
            name: spread([(author, value[kind][name]) for author, value in shares if value and name in value[kind]])
            for name in names} for kind, names in (('universal', UNIVERSAL), ('fine', FINE))}
    return {'flags_per_1000_words': flags, 'baseline_comparison': comparisons, 'measures': measures}


def words(data):
    return data.get('words', 0)


def family(samples, path, names):
    return {name: category(samples, lambda d, name=name: get(d, *path, name), words) for name in names}


def keys(samples, *path):
    found = set()
    for _, data in samples:
        value = data
        for key in path:
            value = value.get(key, {})
        found.update(value)
    return sorted(found)


def echoes(samples):
    categories = {name: family(samples, ('echoes', name), keys(samples, 'echoes', name)) for name in ('distance', 'form')}
    return {'flags_per_1000_words': {'all': {'echo': category(samples, lambda d: get(d, 'echoes', 'echoes'), words)}},
            'categories': categories,
            'top_categories': {name: ranked(stats) for name, stats in categories.items()}}


def repeated_phrases(samples):
    categories = {'length': {n: category(samples, lambda d, n=n: get(d, 'repeated_phrases', 'length', n, 'phrases'),
                                         words) for n in keys(samples, 'repeated_phrases', 'length')}}
    return {'flags_per_1000_words': {'all': {
                'repeated_phrase': category(samples, lambda d: get(d, 'repeated_phrases', 'phrases'), words),
                'occurrence': category(samples, lambda d: get(d, 'repeated_phrases', 'occurrences'), words)}},
            'categories': categories,
            'top_categories': {'length': ranked(categories['length'])}}


def context_words(data, context):
    if context == 'all':
        return get(data, 'cliches', 'narration_words') + get(data, 'cliches', 'dialogue_words')
    return get(data, 'cliches', context + '_words')


def context_count(data, path, context):
    if context == 'all':
        return get(data, *path, 'narration') + get(data, *path, 'dialogue')
    return get(data, *path, context)


def contexts(samples, path):
    return {context: category(samples, lambda d, context=context: context_count(d, path, context),
                              lambda d, context=context: context_words(d, context)) for context in GROUPS}


def cliches(samples):
    kinds = keys(samples, 'cliches', 'kind')
    flags = {context: {'total': category(
        samples, lambda d, context=context: sum(context_count(d, ('cliches', 'kind', kind), context) for kind in kinds),
        lambda d, context=context: context_words(d, context))} for context in GROUPS}
    categories = {'kind': {kind: contexts(samples, ('cliches', 'kind', kind)) for kind in kinds},
                  'source': {source: contexts(samples, ('cliches', 'source', source))['all']
                             for source in keys(samples, 'cliches', 'source')}}
    patterns = {entry: contexts(samples, ('cliches', 'entry', entry))['all']
                for entry in keys(samples, 'cliches', 'entry')}
    return {'flags_per_1000_words': flags, 'categories': categories, 'patterns_with_flags': len(patterns),
            'patterns': patterns,
            'top_categories': {'kind': ranked({kind: stats['all'] for kind, stats in categories['kind'].items()}),
                               'source': ranked(categories['source'])},
            'top_patterns': ranked(patterns)}


def dialogue_tags(samples):
    categories = family(samples, ('dialogue_tags',), TAG_FLAGS)
    verbs = {verb: category(samples, lambda d, verb=verb: get(d, 'dialogue_tags', 'verb', verb), words)
             for verb in keys(samples, 'dialogue_tags', 'verb') if verb not in ('say', 'ask')}
    return {'flags_per_1000_words': {'all': {'total': category(
                samples, lambda d: sum(get(d, 'dialogue_tags', name) for name in TAG_FLAGS), words)}},
            'categories': categories, 'patterns_with_flags': len(verbs), 'patterns': verbs,
            'context_per_1000_words': family(samples, ('dialogue_tags',), TAG_CONTEXT),
            'top_categories': ranked(categories), 'top_patterns': ranked(verbs)}


def lexical_reports(samples):
    reports = {
        'overused_words': {'categories': family(samples, ('overused_words',),
                                                ('overused', 'occurrences', 'fillers'))},
        'sensory_balance': {'categories': family(samples, ('sensory_balance',),
                                                 ('sense_words',)),
                            'modalities': family(samples, ('sensory_balance', 'modalities'),
                                                 ('sight', 'sound', 'touch', 'smell', 'taste', 'interoception'))},
        'diction': {'categories': family(samples, ('diction',), ('abstract_sentences', 'abstract_runs')),
                    'mean_rating': spread([(author, get(data, 'diction', 'rating_sum') / get(data, 'diction', 'rated_words'))
                                           for author, data in samples if get(data, 'diction', 'rated_words')])},
    }
    return reports


def section(samples, defaults, length_baseline, starts_baseline):
    return {'samples': len(samples), 'sample_words': spread([(author, words(data)) for author, data in samples]),
            'reports': {'sentence_length': sentence_length(samples, length_baseline, defaults),
                        'sentence_starts': sentence_starts(samples, starts_baseline, defaults),
                        'echoes': echoes(samples), 'repeated_phrases': repeated_phrases(samples),
                        'cliches': cliches(samples), 'dialogue_tags': dialogue_tags(samples),
                        **lexical_reports(samples)}}


def aggregate(count, authors, chapters, vocabulary, length_baseline, starts_baseline):
    defaults = vocabulary['defaults']
    return {'units': count, 'authors': len(authors), 'words': sum(words(data) for _, data in authors),
            'defaults': defaults, 'window_rule': WINDOW_RULE,
            'chapter': section(chapters, defaults, length_baseline, starts_baseline),
            'manuscript': section(authors, defaults, length_baseline, starts_baseline)}


def allowed_keys(vocabulary):
    defaults = vocabulary['defaults']
    digits = {str(n) for n in PERCENTILES + LENGTH_SWEEP + STARTS_SWEEP} | {
        str(n) for n in range(defaults['phrase_min_length'], defaults['phrase_max_length'] + 1)}
    return STRUCTURE | digits | set(vocabulary['cliche_entries']) | set(vocabulary['cliche_sources']) | set(
        vocabulary['speech_verbs']) | set(vocabulary['non_speech_verbs'])


def numeric_only(value):
    if isinstance(value, dict):
        return all(numeric_only(item) for item in value.values())
    return value is None or (isinstance(value, (int, float)) and not isinstance(value, bool))


def check(result, allowed):
    if not numeric_only(result):
        raise ValueError('non-numeric aggregate')
    stack = [result]
    while stack:
        value = stack.pop()
        if isinstance(value, dict):
            if any(key not in allowed and not key.isdecimal() for key in value):
                raise ValueError('unexpected aggregate key')
            stack.extend(value.values())


def baseline(path):
    return json.loads(path.read_text(encoding='utf-8'))['baseline']


def calibrate(probe, export, started=None):
    started = time.perf_counter() if started is None else started
    count, authors, chapters, stages = collect(probe, export)
    if not authors or not any(words(data) for _, data in authors):
        raise ValueError('no measurable words')
    result = aggregate(count, authors, chapters, probe.vocabulary, baseline(LENGTH_BASELINE), baseline(STARTS_BASELINE))
    probe.close()
    result['run'] = {'seconds': time.perf_counter() - started, 'probe_peak_rss_bytes': probe.peak_rss_bytes,
                     'harness_peak_rss_bytes': rss_bytes(resource.getrusage(resource.RUSAGE_SELF).ru_maxrss),
                     'stage_seconds': stages}
    check(result, allowed_keys(probe.vocabulary))
    return result


SENTINEL = 'Quxsentinel'
NOUNS = ('door', 'window', 'lamp', 'river', 'garden', 'letter', 'coat', 'table', 'road', 'bridge')
VERBS = ('opened', 'closed', 'watched', 'crossed', 'found', 'carried', 'touched', 'painted')
PRONOUNS = ('She', 'He', 'They')


def synthetic_text(seed, paragraphs, dialogue=True):
    rng = random.Random(seed)
    lines = []
    for index in range(paragraphs):
        noun, other = rng.sample(NOUNS, 2)
        pronoun = rng.choice(PRONOUNS)
        sentence = f'{pronoun} {rng.choice(VERBS)} the {noun} near the {other}.'
        if index % 25 == 0 and index:
            lines.append('')
        if dialogue and index % 3 == 1:
            lines.append(f'“Bring the {noun} here,” {pronoun.lower()} said. {sentence}')
        elif dialogue and index % 7 == 2:
            lines.append(f'“Fine,” {pronoun.lower()} smiled. “Leave the {noun},” she whispered softly.')
        elif index % 11 == 3:
            lines.append(f'The {SENTINEL} sat by the {noun}, and the {SENTINEL} was as dead as a doornail.')
        elif index % 13 == 4:
            lines.append(' '.join([f'{pronoun} walked along the {noun} and past the {other} and over the bridge'] * 3)
                         + ' until night.')
        elif index % 17 == 5:
            lines.append('She ran home. She sat down. She stood up. She left again.')
        else:
            lines.append(sentence + f' The {noun} was old.')
    return '\n'.join(lines) + '\n'


def self_test(probe, started=None):
    lexical = probe.measure('She glanced. ' * 6 + 'It was red. ' * 20 + 'It was very cold. ' * 3
                            + 'She just left. ' + 'An idea, a belief, a truth. ' * 3)
    assert lexical['overused_words']['overused'] >= 1
    assert lexical['overused_words']['fillers'] == 4
    assert lexical['sensory_balance']['modalities']['sight'] == 26
    assert lexical['sensory_balance']['modalities']['touch'] == 3
    assert lexical['diction']['abstract_sentences'] == 3
    assert lexical['diction']['abstract_runs'] == 1
    with tempfile.TemporaryDirectory(prefix='style-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        plan = (('PRIVATE_SENTINEL_UNIT_1', 'PRIVATE_SENTINEL_AUTHOR_A', synthetic_text(1, 700)),
                ('PRIVATE_SENTINEL_UNIT_2', 'PRIVATE_SENTINEL_AUTHOR_A', synthetic_text(2, 30)),
                ('PRIVATE_SENTINEL_UNIT_3', 'PRIVATE_SENTINEL_AUTHOR_B', synthetic_text(3, 500)),
                ('PRIVATE_SENTINEL_UNIT_4', 'PRIVATE_SENTINEL_AUTHOR_C', synthetic_text(4, 12, dialogue=False)))
        rows = []
        for unit_id, author, text in plan:
            rows.append({'unit_id': unit_id, 'author_key': author, 'authors': [author + '_NAME'],
                         'title': 'PRIVATE_SENTINEL_TITLE', 'book_id': 'PRIVATE_SENTINEL_BOOK',
                         'source': 'PRIVATE_SENTINEL_SOURCE', 'publisher_type': 'PRIVATE_SENTINEL_PUBLISHER',
                         'genre': 'PRIVATE_SENTINEL_GENRE', 'form': 'PRIVATE_SENTINEL_FORM', 'year': 2001, 'words': 1})
            (export / 'units' / (unit_id + '.txt')).write_text(text, encoding='utf-8')
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows) + '\n')
        split = windows(plan[0][2])
        assert len(split) > 1 and ''.join(split).split() == plan[0][2].split()
        assert all(window_words(window) >= WINDOW_RULE['minimum_words'] for window in split)
        unit = probe.measure(plan[2][2])
        reported = unit['unit_report']
        data = {key: value for key, value in unit.items() if key not in ('timing_ms', 'unit_report')}
        defaults = probe.vocabulary['defaults']
        assert data['sentence_starts']['all']['sentences'] >= defaults['sentence_starts_minimum_sentences']
        length = length_differences(data, 'all', baseline(LENGTH_BASELINE), defaults['sentence_length_minimum_sentences'])
        assert abs(length['mean'] - reported['sentence_length']['all']['mean_difference']) < 1e-9
        for p in (5, 25, 50, 75, 95):
            assert abs(length[str(p)] - reported['sentence_length']['all']['percentile_differences'][str(p)]) < 1e-9
        starts = starts_percentages(data, 'all', defaults['sentence_starts_minimum_sentences'])
        for tag, values in reported['sentence_starts']['all']['universal'].items():
            assert abs(starts['universal'][tag] - values['percent']) < 1e-9
        result = calibrate(probe, export, started)
        output = json.dumps(result)
        assert result['units'] == 4 and result['authors'] == 3
        chapter, manuscript = result['chapter'], result['manuscript']
        assert manuscript['samples'] == 3 and chapter['samples'] > 4
        assert chapter['sample_words']['authors'] == 3
        reports = manuscript['reports']
        assert reports['cliches']['patterns']['dead as a doornail']['flagged_share'] == 1
        assert reports['cliches']['flags_per_1000_words']['dialogue']['total']['flagged_share'] == 0
        assert reports['dialogue_tags']['categories']['non_speech_verb']['flagged_share'] == 2 / 3
        assert reports['dialogue_tags']['patterns']['smile']['flagged_share'] == 2 / 3
        assert reports['sentence_length']['flags_per_1000_words']['narration']['long_sentence']['flagged_share'] == 1
        assert reports['sentence_length']['baseline_comparison']['all']['mean']['eligible_share'] == 2 / 3
        assert reports['sentence_length']['measures']['all']['mean']['samples'] == 2
        assert set(reports['sentence_starts']['measures']['narration']['universal']['PRON']['percentiles']) == {
            str(p) for p in PERCENTILES}
        assert chapter['reports']['echoes']['flags_per_1000_words']['all']['echo']['flagged_share'] == 1
        assert chapter['reports']['sentence_length']['baseline_comparison']['all']['mean']['eligible_share'] > 0
        assert result['run']['probe_peak_rss_bytes'] > 0 and result['run']['seconds'] > 0
        assert 'sentinel' not in output.lower() and temporary not in output
        for name in ('overused_words', 'sensory_balance', 'diction'):
            assert name in reports and name in chapter['reports']
        assert reports['sensory_balance']['categories']['sense_words']['flagged_share'] == 1
        assert numeric_only(result)
        return result


def run(args):
    if args.sentence_length:
        raise ValueError('choose one tier 3 mode')
    executable = args.probe or probe_build()
    started = time.perf_counter()
    table = ROOT / 'bench/style/diction-synthetic.json' if args.self_test else args.diction_table
    if table is None:
        raise ValueError('owner must supply a diction table')
    probe = FlagProbe(executable, args.tokenizer, table)
    try:
        if args.self_test:
            result = self_test(probe, started)
        elif args.export is None:
            raise ValueError('owner must pass an export directory')
        else:
            result = calibrate(probe, args.export, started)
    finally:
        if probe.process.returncode is None:
            probe.process.kill()
            probe.process.wait()
    check(result, allowed_keys(probe.vocabulary))
    print(json.dumps(result, indent=2, allow_nan=False))
