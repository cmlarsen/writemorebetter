from collections import Counter, defaultdict
import json
from pathlib import Path
import statistics
import tempfile

from common import jsonl

from equivalence import TAGS, add_start, start_candidates


def numeric_only(value):
    if isinstance(value, dict):
        return all(numeric_only(item) for item in value.values())
    return value is None or isinstance(value, (int, float))


def percentile(values, percent):
    ordered = sorted(values)
    index = (len(ordered) - 1) * percent / 100
    lower = int(index)
    upper = min(lower + 1, len(ordered) - 1)
    return ordered[lower] + (ordered[upper] - ordered[lower]) * (index - lower)


def distribution(values):
    return {'mean': statistics.mean(values),
            'percentiles': {str(p): percentile(values, p) for p in (0, 5, 25, 50, 75, 95, 100)}}


def calibrate(analyzer, export, baseline):
    authors = defaultdict(lambda: {'sentences': 0, 'words': 0, 'tokens': 0,
                                   'starts': Counter(), 'fractional': Counter(), 'unresolved': 0, 'unknown': 0})
    units = 0
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit_id = row['unit_id']
        if not isinstance(unit_id, str) or Path(unit_id).name != unit_id or unit_id in ('.', '..'):
            raise ValueError('invalid unit identifier')
        if unit_id in seen:
            raise ValueError('duplicate unit')
        seen.add(unit_id)
        author = authors[row['author_key']]
        text = (export / 'units' / (unit_id + '.txt')).read_text(encoding='utf-8')
        units += 1
        for paragraph in text.splitlines():
            if not paragraph.strip():
                continue
            for sentence in analyzer.analyze(paragraph):
                tokens = [token for token in sentence['tokens'] if not token['is_whitespace']]
                words = [t for t in tokens if any(ch.isalnum() for ch in t['text'])]
                if not words:
                    continue
                author['sentences'] += 1
                author['words'] += len(words)
                author['tokens'] += sum(t['is_punctuation'] or any(ch.isalnum() for ch in t['text'])
                                        for t in tokens)
                candidates = start_candidates(tokens, skip_punctuation=True)
                add_start(author['starts'], author['fractional'], candidates)
                author['unresolved'] += len(candidates) != 1
                author['unknown'] += not candidates
    usable = [a for a in authors.values() if a['sentences']]
    resolved = [a for a in usable if sum(a['starts'].values())]
    known = [a for a in usable if sum(a['fractional'].values())]
    if not usable or not known:
        raise ValueError('no measurable sentences or known starts')
    lengths = {}
    for name, field in (('without_punctuation', 'words'), ('with_punctuation', 'tokens')):
        lengths[name] = distribution([a[field] / a['sentences'] for a in usable])
    starts = {}
    for method, field, cohort in (('resolved_only', 'starts', resolved),
                                  ('fractional', 'fractional', known)):
        starts[method] = {}
        for tag in TAGS:
            share = statistics.mean(100 * a[field][tag] / sum(a[field].values()) for a in cohort) if cohort else None
            reference = baseline['google_ngram']['first_word_percent'].get(tag)
            starts[method][tag] = {'measured_percent': share, 'baseline_percent': reference,
                                  'gap_percentage_points': share - reference
                                  if share is not None and reference is not None else None}
    mean = lengths['with_punctuation']['mean']
    return {'units': units, 'authors': len(authors), 'authors_with_sentences': len(usable),
            'authors_with_resolved_starts': len(resolved), 'authors_with_known_starts': len(known),
            'sentences': sum(a['sentences'] for a in usable),
            'unresolved_starts': sum(a['unresolved'] for a in usable),
            'unknown_starts': sum(a['unknown'] for a in usable),
            'ambiguous_starts': sum(a['unresolved'] - a['unknown'] for a in usable),
            'sentence_length': lengths, 'sentence_start': starts,
            'length_comparison': {
                'ngram_mean': baseline['google_ngram']['mean_tokens_per_sentence'],
                'ngram_gap': lengths['without_punctuation' if not baseline['google_ngram']['tokens_include_punctuation'] else 'with_punctuation']['mean'] - baseline['google_ngram']['mean_tokens_per_sentence'],
                'htrc_author_mean': baseline['htrc']['author_mean_tokens_per_sentence'],
                'htrc_author_mean_gap': mean - baseline['htrc']['author_mean_tokens_per_sentence'],
                'htrc_author_median': baseline['htrc']['author_median_tokens_per_sentence'],
                'htrc_author_median_gap': lengths['with_punctuation']['percentiles']['50'] - baseline['htrc']['author_median_tokens_per_sentence'],
                'htrc_volume_median': baseline['htrc']['volume_median_tokens_per_sentence']}}


def self_test(analyzer, baseline):
    with tempfile.TemporaryDirectory(prefix='style-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        rows = []
        for unit_id, author, text in (
            ('one', 'a', 'She runs.\n\n“He walks.”\n'),
            ('two', 'a', 'Birds fly.\n'),
            ('three', 'b', 'The small birds fly.\n')):
            rows.append({'unit_id': unit_id, 'author_key': author, 'title': 'PRIVATE_SENTINEL',
                         'authors': ['PRIVATE_SENTINEL']})
            (export / 'units' / (unit_id + '.txt')).write_text(text, encoding='utf-8')
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows))
        result = calibrate(analyzer, export, baseline)
        assert result['units'] == 3 and result['authors'] == 2 and result['sentences'] == 4
        assert result['sentence_length']['without_punctuation']['mean'] == 3
        assert result['sentence_length']['without_punctuation']['percentiles']['50'] == 3
        assert result['sentence_length']['with_punctuation']['mean'] > 3
        assert abs(sum(s['measured_percent'] for s in result['sentence_start']['fractional'].values()) - 100) < 1e-9
        assert result['sentence_start']['resolved_only']['VERB']['measured_percent'] == 0
        ambiguous = [{'text': 'Water', 'is_whitespace': False,
                      'readings': [{'pos': 'NN:UN'}, {'pos': 'VB'}, {'pos': 'VBP'}]}]
        candidates = start_candidates(ambiguous)
        resolved_counts, fractional_counts = Counter(), Counter()
        add_start(resolved_counts, fractional_counts, candidates)
        assert candidates == {'NOUN', 'VERB'} and resolved_counts['VERB'] == 0
        assert fractional_counts['NOUN'] == fractional_counts['VERB'] == 0.5
        assert 'PRIVATE_SENTINEL' not in json.dumps(result)
        assert numeric_only(result)
        return result


def run(analyzer, args):
    baseline = json.loads(args.baseline.read_text(encoding='utf-8'))
    if args.self_test:
        result = self_test(analyzer, baseline)
    else:
        if args.export is None:
            raise ValueError('owner must pass an export directory')
        result = calibrate(analyzer, args.export, baseline)
    if not numeric_only(result):
        raise ValueError('non-numeric aggregate')
    print(json.dumps(result, indent=2, allow_nan=False))


def calibrate_sentence_length(probe, export):
    authors = defaultdict(list)
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit = row['unit_id']
        if not isinstance(unit, str) or Path(unit).name != unit or unit in ('.', '..') or unit in seen:
            raise ValueError('invalid or duplicate unit identifier')
        seen.add(unit)
        text = (export / 'units' / (unit + '.txt')).read_text(encoding='utf-8')
        result = probe.analyze(text)
        authors[row['author_key']].extend(s['words'] for s in result['length']['all']['sentences'])
    usable = [words for words in authors.values() if words]
    if not usable:
        raise ValueError('no measurable sentences')
    means = [statistics.mean(words) for words in usable]
    return {'authors': len(usable), 'sentences': sum(map(len, usable)),
            'baseline': {'mean': statistics.mean(means),
                         'percentiles': {str(p): statistics.mean(percentile(words, p) for words in usable)
                                         for p in (5, 25, 50, 75, 95)}},
            'authorMeanPercentiles': {str(p): percentile(means, p) for p in (5,25,50,75,95)}}


def sentence_length_self_test(probe):
    with tempfile.TemporaryDirectory(prefix='style-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        rows = []
        for unit, author, text in (('a', 'one', '“She runs.”\n\nHe walks.\n'),
                                   ('b', 'one', 'Birds fly.\n'),
                                   ('c', 'two', 'The small birds fly.\n')):
            rows.append({'unit_id': unit, 'author_key': author, 'title': 'PRIVATE_SENTINEL'})
            (export / 'units' / (unit + '.txt')).write_text(text)
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows))
        split = probe.analyze('Chapter One\nShe ran to the door\n“Wait—”\n“No.”\nHe left. Then he came back.\n')
        assert [s['words'] for s in split['length']['all']['sentences']] == [2, 5, 1, 1, 2, 4]
        result = calibrate_sentence_length(probe, export)
        assert result['authors'] == 2 and result['sentences'] == 4
        assert result['baseline']['mean'] == 3
        assert result['baseline']['percentiles']['50'] == 3
        assert numeric_only(result) and 'PRIVATE_SENTINEL' not in json.dumps(result)
        return result
