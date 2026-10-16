from collections import defaultdict
import json
import os
from pathlib import Path
import random
import resource
import statistics
import subprocess
import sys
import tempfile
import time

from common import ROOT, jsonl
from tier3 import numeric_only, percentile

GROUPS = ('all', 'narration', 'dialogue')
PERCENTILES = (5, 25, 50, 75, 95)
LENGTH_STATISTICS = ('mean',) + tuple(str(p) for p in PERCENTILES)
LENGTH_SWEEP = (0.5, 1, 2, 3, 5)
STARTS_SWEEP = (1, 2, 5, 10)
TOP = 10
BASELINES = ROOT / 'style/data/baselines'
LENGTH_BASELINE = BASELINES / 'sentence-length.json'
STARTS_BASELINE = BASELINES / 'sentence-starts.json'
TAG_FLAGS = ('other_speech_verb', 'non_speech_verb', 'manner_adverb', 'untagged_run')
TAG_CONTEXT = ('said_asked', 'unlisted_verb', 'beats')
STAGES = ('document', 'sentence_length', 'sentence_starts', 'echoes', 'repeated_phrases', 'cliches',
          'dialogue_tags')
STRUCTURE = frozenset(GROUPS + LENGTH_STATISTICS + TAG_FLAGS + TAG_CONTEXT + STAGES + tuple(
    str(t) for t in LENGTH_SWEEP + STARTS_SWEEP) + (
    'units', 'authors', 'words', 'run', 'seconds', 'probe_peak_rss_bytes', 'harness_peak_rss_bytes',
    'stage_seconds', 'defaults', 'tags', 'mean', 'reports', 'percentiles', 'flags_per_1000_words',
    'baseline_comparison',
    'categories', 'patterns', 'top_categories', 'top_patterns', 'patterns_with_flags',
    'authors_with_flag_share', 'per_1000_words', 'difference', 'highlighted_share', 'eligible_authors',
    'share_abs_difference_at_least', 'percent', 'universal', 'fine', 'total', 'long_sentence',
    'monotony_run', 'same_word_run', 'distance', 'form', 'length', 'kind', 'source', 'verb',
    'repeated_phrase', 'occurrence', 'tags_per_1000_words', 'context_per_1000_words', 'echo',
    'cliche', 'redundancy', 'same_form', 'different_form', '1_5', '6_10', '11_25', '26_50', 'over_50',
    'subject', 'adverb', 'gerund', 'infinitive', 'subordinatingConjunction', 'coordinatingConjunction',
    'pastParticiple', 'inversion', 'introductory', 'other',
    'ADJ', 'ADP', 'ADV', 'CONJ', 'DET', 'NOUN', 'NUM', 'PRON', 'PRT', 'VERB', '.', 'X',
    'sentence_length_minimum_sentences', 'long_sentence_words', 'sentence_starts_minimum_sentences',
    'echo_window', 'phrase_min_length', 'phrase_max_length', 'phrase_min_content_words',
    'max_untagged_exchanges', 'dialogue_tag_min_tags'))


class FlagProbe:
    def __init__(self, executable, tokenizer):
        self.vocabulary = json.loads(subprocess.run(
            [str(executable), '--vocabulary', str(tokenizer)], capture_output=True, text=True,
            encoding='utf-8', check=True).stdout)
        self.process = subprocess.Popen(
            [str(executable), str(tokenizer), str(LENGTH_BASELINE), str(STARTS_BASELINE)],
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
        unit_id = row['unit_id']
        if not isinstance(unit_id, str) or Path(unit_id).name != unit_id or unit_id in ('.', '..'):
            raise ValueError('invalid unit identifier')
        if unit_id in seen:
            raise ValueError('duplicate unit')
        seen.add(unit_id)
        yield row['author_key'], (export / 'units' / (unit_id + '.txt')).read_text(encoding='utf-8')


def collect(probe, export):
    authors = defaultdict(dict)
    stages = defaultdict(float)
    count = 0
    for author, text in units(export):
        unit = probe.measure(text)
        for stage, milliseconds in unit.pop('timing_ms').items():
            stages[stage] += milliseconds / 1000
        unit.pop('unit_report')
        merge(authors[author], unit)
        count += 1
    return count, list(authors.values()), dict(stages)


def spread(values):
    if not values:
        return None
    return {'authors': len(values), 'mean': statistics.mean(values),
            'percentiles': {str(p): percentile(values, p) for p in PERCENTILES}}


def share(flags, total):
    return flags / total if total else None


def category(authors, count, words):
    eligible = [a for a in authors if words(a)]
    return {'authors_with_flag_share': share(sum(count(a) > 0 for a in eligible), len(eligible)),
            'per_1000_words': spread([1000 * count(a) / words(a) for a in eligible])}


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
    for p in PERCENTILES:
        index = (total - 1) * p / 100
        low = int(index)
        high = min(low + 1, total - 1)
        result[str(p)] = at(low) + (at(high) - at(low)) * (index - low)
    return result


def length_differences(author, group, baseline, minimum):
    histogram = author.get('sentence_length', {}).get(group, {}).get('histogram', {})
    if sum(histogram.values()) < max(minimum, 1):
        return None
    measured = histogram_statistics(histogram)
    reference = {'mean': baseline.get('mean'), **{str(k): v for k, v in baseline.get('percentiles', {}).items()}}
    return {name: measured[name] - reference[name] for name in LENGTH_STATISTICS if reference.get(name) is not None}


def starts_percentages(author, group, minimum):
    starts = author.get('sentence_starts', {}).get(group, {})
    sentences = starts.get('sentences', 0)
    if sentences < max(minimum, 1):
        return None
    known = sentences - starts.get('unknown_starts', 0)
    return {'fine': {name: 100 * count / sentences for name, count in starts.get('categories', {}).items()},
            'universal': {tag: 100 * count / known for tag, count in starts.get('universal', {}).items()}
            if known else {}}


def comparison(values, total, sweep):
    return {'eligible_authors': len(values), 'highlighted_share': share(len(values), total),
            'difference': spread(values),
            'share_abs_difference_at_least': {str(t): share(sum(abs(v) >= t for v in values), total)
                                              for t in sweep}}


def group_words(author, group):
    return histogram_words(author.get('sentence_length', {}).get(group, {}).get('histogram', {}))


def sentence_length(authors, baseline, defaults):
    flags = {}
    for group in GROUPS:
        words = lambda a, group=group: group_words(a, group)
        long = lambda a, group=group: get(a, 'sentence_length', group, 'long_sentences')
        runs = lambda a, group=group: get(a, 'sentence_length', group, 'monotony_runs')
        flags[group] = {'total': category(authors, lambda a: long(a) + runs(a), words),
                        'long_sentence': category(authors, long, words),
                        'monotony_run': category(authors, runs, words)}
    comparisons = {}
    for group in GROUPS:
        differences = [length_differences(a, group, baseline, defaults['sentence_length_minimum_sentences'])
                       for a in authors]
        comparisons[group] = {name: comparison([d[name] for d in differences if d and name in d],
                                               len(authors), LENGTH_SWEEP)
                              for name in LENGTH_STATISTICS}
    return {'flags_per_1000_words': flags, 'baseline_comparison': comparisons}


def sentence_starts(authors, baseline, defaults):
    flags = {group: {'same_word_run': category(
        authors, lambda a, group=group: get(a, 'sentence_starts', group, 'same_word_runs'),
        lambda a, group=group: group_words(a, group))} for group in GROUPS}
    comparisons, fine = {}, {}
    for group in GROUPS:
        shares = [starts_percentages(a, group, defaults['sentence_starts_minimum_sentences']) for a in authors]
        comparisons[group] = {'universal': {
            tag: comparison([s['universal'][tag] - reference for s in shares if s and tag in s['universal']],
                            len(authors), STARTS_SWEEP)
            for tag, reference in baseline.get('universalPercent', {}).items()}}
        if baseline.get('categoryPercent'):
            comparisons[group]['fine'] = {
                name: comparison([s['fine'][name] - reference for s in shares if s and name in s['fine']],
                                 len(authors), STARTS_SWEEP)
                for name, reference in baseline['categoryPercent'].items()}
        names = sorted({name for s in shares if s for name in s['fine']})
        fine[group] = {name: {'percent': spread([s['fine'][name] for s in shares if s])} for name in names}
    return {'flags_per_1000_words': flags, 'baseline_comparison': comparisons, 'categories': fine}


def words(author):
    return author.get('words', 0)


def family(authors, path, names):
    return {name: category(authors, lambda a, name=name: get(a, *path, name), words) for name in names}


def keys(authors, *path):
    found = set()
    for author in authors:
        value = author
        for key in path:
            value = value.get(key, {})
        found.update(value)
    return sorted(found)


def echoes(authors):
    categories = {'distance': family(authors, ('echoes', 'distance'), keys(authors, 'echoes', 'distance')),
                  'form': family(authors, ('echoes', 'form'), keys(authors, 'echoes', 'form'))}
    return {'flags_per_1000_words': {'all': {'echo': category(authors, lambda a: get(a, 'echoes', 'echoes'), words)}},
            'categories': categories,
            'top_categories': {name: ranked(stats) for name, stats in categories.items()}}


def repeated_phrases(authors):
    lengths = keys(authors, 'repeated_phrases', 'length')
    categories = {'length': {n: category(authors, lambda a, n=n: get(a, 'repeated_phrases', 'length', n, 'phrases'),
                                        words) for n in lengths}}
    return {'flags_per_1000_words': {'all': {
                'repeated_phrase': category(authors, lambda a: get(a, 'repeated_phrases', 'phrases'), words),
                'occurrence': category(authors, lambda a: get(a, 'repeated_phrases', 'occurrences'), words)}},
            'categories': categories,
            'top_categories': {'length': ranked(categories['length'])}}


def context_words(author, context):
    if context == 'all':
        return get(author, 'cliches', 'narration_words') + get(author, 'cliches', 'dialogue_words')
    return get(author, 'cliches', context + '_words')


def context_count(author, path, context):
    if context == 'all':
        return get(author, *path, 'narration') + get(author, *path, 'dialogue')
    return get(author, *path, context)


def contexts(authors, path):
    return {context: category(authors, lambda a, context=context: context_count(a, path, context),
                              lambda a, context=context: context_words(a, context)) for context in GROUPS}


def cliches(authors):
    flags = {context: {'total': category(
        authors, lambda a, context=context: sum(context_count(a, ('cliches', 'kind', kind), context)
                                                for kind in keys([a], 'cliches', 'kind')),
        lambda a, context=context: context_words(a, context))} for context in GROUPS}
    categories = {'kind': {kind: contexts(authors, ('cliches', 'kind', kind)) for kind in keys(authors, 'cliches', 'kind')},
                  'source': {source: contexts(authors, ('cliches', 'source', source))['all']
                             for source in keys(authors, 'cliches', 'source')}}
    patterns = {entry: contexts(authors, ('cliches', 'entry', entry))['all']
                for entry in keys(authors, 'cliches', 'entry')}
    return {'flags_per_1000_words': flags, 'categories': categories, 'patterns_with_flags': len(patterns),
            'patterns': patterns,
            'top_categories': {'kind': ranked({kind: stats['all'] for kind, stats in categories['kind'].items()}),
                               'source': ranked(categories['source'])},
            'top_patterns': ranked(patterns)}


def dialogue_tags(authors):
    categories = family(authors, ('dialogue_tags',), TAG_FLAGS)
    verbs = {verb: category(authors, lambda a, verb=verb: get(a, 'dialogue_tags', 'verb', verb), words)
             for verb in keys(authors, 'dialogue_tags', 'verb') if verb not in ('say', 'ask')}
    return {'flags_per_1000_words': {'all': {'total': category(
                authors, lambda a: sum(get(a, 'dialogue_tags', name) for name in TAG_FLAGS), words)}},
            'categories': categories, 'patterns_with_flags': len(verbs), 'patterns': verbs,
            'context_per_1000_words': {'tags': spread([1000 * get(a, 'dialogue_tags', 'tags') / words(a)
                                                       for a in authors if words(a)]),
                                       **family(authors, ('dialogue_tags',), TAG_CONTEXT)},
            'top_categories': ranked(categories), 'top_patterns': ranked(verbs)}


def aggregate(count, authors, vocabulary, length_baseline, starts_baseline):
    defaults = vocabulary['defaults']
    return {'units': count, 'authors': len(authors), 'words': sum(map(words, authors)), 'defaults': defaults,
            'reports': {'sentence_length': sentence_length(authors, length_baseline, defaults),
                        'sentence_starts': sentence_starts(authors, starts_baseline, defaults),
                        'echoes': echoes(authors), 'repeated_phrases': repeated_phrases(authors),
                        'cliches': cliches(authors), 'dialogue_tags': dialogue_tags(authors)}}


def allowed_keys(vocabulary):
    return STRUCTURE | {str(n) for n in range(1000)} | set(vocabulary['cliche_entries']) | set(
        vocabulary['cliche_sources']) | set(vocabulary['speech_verbs']) | set(vocabulary['non_speech_verbs'])


def check(result, allowed):
    if not numeric_only(result):
        raise ValueError('non-numeric aggregate')
    stack = [result]
    while stack:
        value = stack.pop()
        if isinstance(value, dict):
            if any(key not in allowed for key in value):
                raise ValueError('unexpected aggregate key')
            stack.extend(value.values())


def baseline(path):
    return json.loads(path.read_text(encoding='utf-8'))['baseline']


def calibrate(probe, export):
    started = time.perf_counter()
    count, authors, stages = collect(probe, export)
    if not authors or not any(map(words, authors)):
        raise ValueError('no measurable words')
    result = aggregate(count, authors, probe.vocabulary, baseline(LENGTH_BASELINE), baseline(STARTS_BASELINE))
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


def self_test(probe):
    with tempfile.TemporaryDirectory(prefix='style-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        plan = (('PRIVATE_SENTINEL_UNIT_1', 'PRIVATE_SENTINEL_AUTHOR_A', synthetic_text(1, 140)),
                ('PRIVATE_SENTINEL_UNIT_2', 'PRIVATE_SENTINEL_AUTHOR_A', synthetic_text(2, 30)),
                ('PRIVATE_SENTINEL_UNIT_3', 'PRIVATE_SENTINEL_AUTHOR_B', synthetic_text(3, 160)),
                ('PRIVATE_SENTINEL_UNIT_4', 'PRIVATE_SENTINEL_AUTHOR_C', synthetic_text(4, 12, dialogue=False)))
        rows = []
        for unit_id, author, text in plan:
            rows.append({'unit_id': unit_id, 'author_key': author, 'authors': [author + '_NAME'],
                         'title': 'PRIVATE_SENTINEL_TITLE', 'book_id': 'PRIVATE_SENTINEL_BOOK',
                         'source': 'PRIVATE_SENTINEL_SOURCE', 'publisher_type': 'PRIVATE_SENTINEL_PUBLISHER',
                         'genre': 'PRIVATE_SENTINEL_GENRE', 'form': 'PRIVATE_SENTINEL_FORM', 'year': 2001, 'words': 1})
            (export / 'units' / (unit_id + '.txt')).write_text(text, encoding='utf-8')
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows) + '\n')
        unit = probe.measure(plan[2][2])
        reported = unit['unit_report']
        author = {key: value for key, value in unit.items() if key not in ('timing_ms', 'unit_report')}
        defaults = probe.vocabulary['defaults']
        assert author['sentence_starts']['all']['sentences'] >= defaults['sentence_starts_minimum_sentences']
        length = length_differences(author, 'all', baseline(LENGTH_BASELINE), defaults['sentence_length_minimum_sentences'])
        assert abs(length['mean'] - reported['sentence_length']['all']['mean_difference']) < 1e-9
        for p in PERCENTILES:
            assert abs(length[str(p)] - reported['sentence_length']['all']['percentile_differences'][str(p)]) < 1e-9
        starts = starts_percentages(author, 'all', defaults['sentence_starts_minimum_sentences'])
        for tag, values in reported['sentence_starts']['all']['universal'].items():
            assert abs(starts['universal'][tag] - values['percent']) < 1e-9
        result = calibrate(probe, export)
        output = json.dumps(result)
        assert result['units'] == 4 and result['authors'] == 3
        reports = result['reports']
        doornail = reports['cliches']['patterns']['dead as a doornail']
        assert doornail['authors_with_flag_share'] == 1
        assert reports['cliches']['flags_per_1000_words']['dialogue']['total']['authors_with_flag_share'] == 0
        assert reports['dialogue_tags']['categories']['non_speech_verb']['authors_with_flag_share'] == 2 / 3
        assert reports['dialogue_tags']['patterns']['smile']['authors_with_flag_share'] == 2 / 3
        assert reports['sentence_length']['flags_per_1000_words']['narration']['long_sentence']['authors_with_flag_share'] == 1
        assert reports['sentence_starts']['flags_per_1000_words']['narration']['same_word_run']['authors_with_flag_share'] > 0
        assert reports['sentence_length']['baseline_comparison']['all']['mean']['highlighted_share'] == 2 / 3
        assert reports['echoes']['flags_per_1000_words']['all']['echo']['authors_with_flag_share'] == 1
        assert result['run']['probe_peak_rss_bytes'] > 0 and result['run']['seconds'] > 0
        assert 'sentinel' not in output.lower() and temporary not in output
        assert numeric_only(result)
        return result


def run(args):
    if args.sentence_length:
        raise ValueError('choose one tier 3 mode')
    probe = FlagProbe(args.probe or probe_build(), args.tokenizer)
    try:
        if args.self_test:
            result = self_test(probe)
        elif args.export is None:
            raise ValueError('owner must pass an export directory')
        else:
            result = calibrate(probe, args.export)
    finally:
        if probe.process.returncode is None:
            probe.process.kill()
            probe.process.wait()
    check(result, allowed_keys(probe.vocabulary))
    print(json.dumps(result, indent=2, allow_nan=False))
