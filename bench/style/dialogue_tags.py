from collections import Counter, defaultdict
import json
from pathlib import Path
import statistics
import subprocess
import tempfile
import xml.etree.ElementTree as ET

from common import HERE, ROOT, fetch, jsonl, write_jsonl
import tier2
from tier3 import distribution, numeric_only

PASSAGES = (
    ('styles-1.txt', 'styles_ch1', 12, 70, 'dev'),
    ('gatsby-1.txt', 'gatsby_ch1', 34, 72, 'dev'),
    ('sign-1.txt', 'sign_ch1', 3, 34, 'test'),
    ('avonlea-1.txt', 'avonlea_ch1', 0, 50, 'test'),
)
SPLITS = ('dev', 'test', 'all')
LABELS = HERE / 'labels/dialogue-tags.jsonl'
BASELINE = HERE / 'dialogue-tags-baseline.json'
REPORTS = ('dialogue_tag_position', 'dialogue_tag_verb', 'dialogue_tag_adverb', 'dialogue_beat')


def paragraphs(key):
    root = ET.fromstring(fetch(key).read_bytes())
    return [''.join(p.itertext()).strip() for p in root.iter('{http://www.w3.org/1999/xhtml}p')]


def passages(split='all'):
    for file, key, start, end, part in PASSAGES:
        if split in ('all', part):
            yield file, key, '\n'.join(paragraphs(key)[start:end])


def probe_build(quiet=False):
    tool = ROOT / 'bench/tools/wmb-document'
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml')],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL if quiet else None)
    return tool / 'target/release/dialogue_tags'


def predict(executable, tokenizer, documents):
    result = subprocess.run([str(executable), str(tokenizer)],
                            input=''.join(json.dumps(row) + '\n' for row in documents),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def validate(gold, texts):
    for row in gold:
        text = texts[row['file']].encode('utf-8')
        surface = text[row['byte']['start']:row['byte']['end']].decode('utf-8')
        if surface != row['text']:
            raise ValueError(f'label text mismatch in {row["file"]}: {surface!r} != {row["text"]!r}')
        if row['report'] not in REPORTS:
            raise ValueError('unknown dialogue tag report')


def evaluate(executable, tokenizer, out):
    out.mkdir(parents=True, exist_ok=True)
    documents = [{'file': file, 'text': text} for file, _, text in passages()]
    texts = {row['file']: row['text'] for row in documents}
    gold = jsonl(LABELS)
    validate(gold, texts)
    predictions = predict(executable, tokenizer, documents)
    write_jsonl(out / 'dialogue-tag-predictions.jsonl', predictions)
    result = {'documents': len(documents), 'labels': len(gold), 'splits': {}}
    for split in SPLITS:
        files = {file for file, *_ in passages(split)}
        split_gold = [row for row in gold if row['file'] in files]
        split_predictions = [row for row in predictions if row['file'] in files]
        result['splits'][split] = {
            'dialogue_spans': sum(row['report'] == 'dialogue_span' for row in split_predictions),
            'labels': len(split_gold),
            'reports': {report: tier2.score(split_gold, split_predictions, files, report=report)
                        for report in REPORTS}}
    return result


def gate(result):
    floors = json.loads(BASELINE.read_text())['floors']
    failures = []
    for report, patterns in floors.items():
        for pattern, metrics in patterns.items():
            measured = result['splits']['all']['reports'][report]['patterns'].get(pattern)
            for metric, floor in metrics.items():
                value = measured[metric] if measured else 0.0
                if value + 1e-12 < floor:
                    failures.append(f'{report}/{pattern} {metric} {value:.6f} < {floor:.6f}')
    if failures:
        raise ValueError('dialogue tag benchmark regressed: ' + '; '.join(failures))


class CountProbe:
    def __init__(self, executable, tokenizer):
        self.process = subprocess.Popen([str(executable), str(tokenizer), '--counts'],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL, text=True, encoding='utf-8')

    def counts(self, text):
        self.process.stdin.write(json.dumps({'file': '', 'text': text}) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            raise RuntimeError('dialogue tag probe exited')
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.wait() != 0:
            raise RuntimeError('dialogue tag probe failed')


FIELDS = ('words', 'dialogue_spans', 'speech_spans', 'tagged_spans', 'exchanges', 'tags', 'said_asked', 'with_adverb',
          'non_speech', 'unlisted', 'beats', 'untagged_runs', 'untagged_run_exchanges')
RATES = {
    'said_asked_percent': ('said_asked', 'tags', 100),
    'adverb_percent': ('with_adverb', 'tags', 100),
    'non_speech_percent': ('non_speech', 'tags', 100),
    'unlisted_percent': ('unlisted', 'tags', 100),
    'tagged_span_percent': ('tagged_spans', 'speech_spans', 100),
    'beats_per_100_spans': ('beats', 'speech_spans', 100),
    'untagged_run_exchange_percent': ('untagged_run_exchanges', 'exchanges', 100),
    'tags_per_1000_words': ('tags', 'words', 1000),
}


def calibrate(probe, export):
    authors = defaultdict(lambda: {'counts': Counter(), 'positions': Counter(), 'verbs': Counter()})
    seen = set()
    for row in jsonl(export / 'units.jsonl'):
        unit_id = row['unit_id']
        if not isinstance(unit_id, str) or Path(unit_id).name != unit_id or unit_id in ('.', '..'):
            raise ValueError('invalid unit identifier')
        if unit_id in seen:
            raise ValueError('duplicate unit')
        seen.add(unit_id)
        result = probe.counts((export / 'units' / (unit_id + '.txt')).read_text(encoding='utf-8'))
        author = authors[row['author_key']]
        author['counts'].update({field: result[field] for field in FIELDS})
        author['positions'].update(result['positions'])
        author['verbs'].update(result['verbs'])
    rates = {}
    for name, (numerator, denominator, scale) in RATES.items():
        values = [scale * a['counts'][numerator] / a['counts'][denominator]
                  for a in authors.values() if a['counts'][denominator]]
        rates[name] = dict(distribution(values), authors=len(values)) if values else None
    tagged = [a for a in authors.values() if a['counts']['tags']]
    positions = {position: statistics.mean(100 * a['positions'][position] / a['counts']['tags'] for a in tagged)
                 if tagged else None for position in ('before', 'after', 'split')}
    lemmas = sorted({lemma for a in tagged for lemma in a['verbs']})
    verbs = {lemma: statistics.mean(100 * a['verbs'][lemma] / a['counts']['tags'] for a in tagged)
             for lemma in lemmas}
    totals = Counter()
    for author in authors.values():
        totals.update(author['counts'])
    return {'units': len(seen), 'authors': len(authors), 'authors_with_tags': len(tagged),
            'totals': {field: totals[field] for field in FIELDS}, 'author_means': rates,
            'position_percent': positions, 'listed_verb_percent': verbs}


def self_test(probe):
    with tempfile.TemporaryDirectory(prefix='style-synthetic-') as temporary:
        export = Path(temporary)
        (export / 'units').mkdir()
        rows = []
        for unit_id, author, text in (
            ('one', 'a', '“Go,” she said.\n“Why?” he asked softly.\n'),
            ('two', 'a', '“Fine,” she smiled.\n\n“Wait,” he said, “there’s more.”\n'),
            ('three', 'b', '“A.”\n“B.”\n“C.”\n“D.”\n“E.”\n“F.”\n“G.”\n“H.”\nShe left. “Now,” she whispered.\n')):
            rows.append({'unit_id': unit_id, 'author_key': author, 'title': 'PRIVATE_SENTINEL',
                         'authors': ['PRIVATE_SENTINEL']})
            (export / 'units' / (unit_id + '.txt')).write_text(text, encoding='utf-8')
        (export / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows))
        result = calibrate(probe, export)
        assert result['units'] == 3 and result['authors'] == 2 and result['authors_with_tags'] == 2
        assert result['totals']['tags'] == 5 and result['totals']['said_asked'] == 3
        assert result['totals']['non_speech'] == 1 and result['totals']['with_adverb'] == 1
        assert result['totals']['untagged_runs'] == 1 and result['totals']['untagged_run_exchanges'] == 8
        said = result['author_means']['said_asked_percent']
        assert said['authors'] == 2 and abs(said['mean'] - 37.5) < 1e-9
        assert result['author_means']['adverb_percent']['percentiles']['50'] == 12.5
        assert abs(sum(result['position_percent'].values()) - 100) < 1e-9
        assert abs(result['listed_verb_percent']['whisper'] - 50) < 1e-9
        assert 'PRIVATE_SENTINEL' not in json.dumps(result)
        assert numeric_only(result)
        return result


def tier3(args):
    try:
        probe = CountProbe(args.probe or probe_build(quiet=True), args.tokenizer)
        try:
            if args.self_test:
                result = self_test(probe)
            elif args.export is None:
                raise ValueError('owner must pass an export directory')
            else:
                result = calibrate(probe, args.export)
        finally:
            probe.close()
        if not numeric_only(result):
            raise ValueError('non-numeric aggregate')
        print(json.dumps(result, indent=2, allow_nan=False))
    except BaseException:
        raise SystemExit('calibration failed; details suppressed to protect source metadata') from None


def main(args):
    if args.suite == 'tier3':
        tier3(args)
        return
    result = evaluate(args.probe or probe_build(), args.tokenizer, args.out)
    gate(result)
    print(json.dumps(result, indent=2))
