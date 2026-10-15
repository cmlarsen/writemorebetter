from collections import Counter, defaultdict
import json
from pathlib import Path
import statistics
import tempfile

from common import jsonl

TAGS = ('ADJ', 'ADP', 'ADV', 'CONJ', 'DET', 'NOUN', 'NUM', 'PRON', 'PRT', 'VERB', '.', 'X')
PENN = {}
for universal, tags in {
    'ADJ': 'JJ JJR JJS', 'ADP': 'IN', 'ADV': 'RB RBR RBS WRB', 'CONJ': 'CC',
    'DET': 'DT EX PDT WDT', 'NOUN': 'NN NNS NNP NNPS', 'NUM': 'CD',
    'PRON': 'PRP PRP$ WP WP$', 'PRT': 'POS RP TO', 'VERB': 'VB VBD VBG VBN VBP VBZ MD',
    '.': "# $ '' ( ) , . : `` -LRB- -RRB-", 'X': 'FW LS SYM UH'
}.items():
    for tag in tags.split():
        PENN[tag] = universal


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
                                   'starts': Counter(), 'unresolved': 0})
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
                while tokens and all(ch in '“”"‘’' for ch in tokens[0]['text']):
                    tokens.pop(0)
                tags = {PENN[r['pos']] for r in tokens[0]['readings'] if r['pos'] in PENN} if tokens else set()
                if len(tags) == 1:
                    author['starts'][tags.pop()] += 1
                else:
                    author['unresolved'] += 1
    usable = [a for a in authors.values() if a['sentences']]
    resolved = [a for a in usable if sum(a['starts'].values())]
    if not usable or not resolved:
        raise ValueError('no measurable sentences or resolved starts')
    lengths = {}
    for name, field in (('without_punctuation', 'words'), ('with_punctuation', 'tokens')):
        lengths[name] = distribution([a[field] / a['sentences'] for a in usable])
    starts = {}
    for tag in TAGS:
        share = statistics.mean(100 * a['starts'][tag] / sum(a['starts'].values()) for a in resolved)
        reference = baseline['google_ngram']['first_word_percent'].get(tag)
        starts[tag] = {'measured_percent': share, 'baseline_percent': reference,
                       'gap_percentage_points': share - reference if reference is not None else None}
    mean = lengths['with_punctuation']['mean']
    return {'units': units, 'authors': len(authors), 'authors_with_sentences': len(usable),
            'authors_with_resolved_starts': len(resolved),
            'sentences': sum(a['sentences'] for a in usable),
            'unresolved_starts': sum(a['unresolved'] for a in usable),
            'sentence_length': lengths, 'sentence_start': starts,
            'length_comparison': {
                'ngram_mean': baseline['google_ngram']['mean_tokens_per_sentence'],
                'ngram_gap': mean - baseline['google_ngram']['mean_tokens_per_sentence'],
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
        assert abs(sum(s['measured_percent'] for s in result['sentence_start'].values()) - 100) < 1e-9
        assert 'PRIVATE_SENTINEL' not in json.dumps(result)
        def numeric_only(value):
            if isinstance(value, dict):
                return all(numeric_only(item) for item in value.values())
            return value is None or isinstance(value, (int, float))
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
    print(json.dumps(result, indent=2, allow_nan=False))
