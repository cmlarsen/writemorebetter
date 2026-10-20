"""Concreteness build and rater regressions using invented ratings only."""
import contextlib
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


builder = load('concreteness_builder', ROOT / 'scripts/build-concreteness-norms.py')
rater = load('concreteness_rater', ROOT / 'bench/style/concreteness/rate.py')
gate = load('concreteness_gate', ROOT / 'bench/style/concreteness.py')

SHEET = ('<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>'
         '<row r="1"><c r="L1" t="s"><v>0</v></c></row>'
         '<row r="2"><c r="A2" t="s"><v>1</v></c><c r="L2" t="s"><v>2</v></c></row>{rows}'
         '</sheetData></worksheet>')
STRINGS = ('<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">{items}</sst>')


def workbook(path, entries):
    strings = ['CNC', 'word', 'M']
    rows = []
    for number, (word, cnc) in enumerate(entries, 3):
        if isinstance(word, bool):
            cell = f'<c r="A{number}" t="b"><v>{int(word)}</v></c>'
        else:
            strings.append(word)
            cell = f'<c r="A{number}" t="s"><v>{len(strings) - 1}</v></c>'
        rows.append(f'<row r="{number}">{cell}<c r="L{number}"><v>{cnc}</v></c></row>')
    items = ''.join(f'<si><t>{s}</t></si>' for s in strings)
    with zipfile.ZipFile(path, 'w') as archive:
        for name, text in (('xl/sharedStrings.xml', STRINGS.format(items=items)),
                           ('xl/worksheets/sheet1.xml', SHEET.format(rows=''.join(rows)))):
            archive.writestr(zipfile.ZipInfo(name, (1980, 1, 1, 0, 0, 0)), text)


def wordnet_archive(path):
    data = '\n'.join([
        '  1 This software and database is being provided to you',
        '00001740 03 n 01 entity 0 001 ~ 00001930 n 0000 | that which exists',
        '00001930 03 n 01 physical_entity 0 001 @ 00001740 n 0000 | a tangible entity',
        '00002137 03 n 01 abstraction 0 001 @ 00001740 n 0000 | a general concept',
        '00003000 06 n 01 spoon 0 001 @ 00001930 n 0000 | cutlery',
        '00003100 06 n 01 idea 0 001 @ 00002137 n 0000 | a thought',
        '00003200 06 n 01 pitcher 0 001 @i 00003000 n 0000 | an instance',
    ]) + '\n'
    index = '\n'.join([
        '  1 This software and database is being provided to you',
        'idea n 1 1 @ 1 0 00003100',
        'pitcher n 2 1 @ 2 0 00003200 00003100',
        'spoon n 1 1 @ 1 0 00003000',
    ]) + '\n'
    with tarfile.open(path, 'w:gz') as archive:
        for name, text in (('dict/data.noun', data), ('dict/index.noun', index)):
            payload = text.encode('latin-1')
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))


class StatisticsTests(unittest.TestCase):
    def test_ranks_average_ties(self):
        self.assertEqual(builder.ranks([3, 1, 3, 2]), [3.5, 1.0, 3.5, 2.0])

    def test_correlations(self):
        self.assertAlmostEqual(builder.pearson([1, 2, 3], [2, 4, 6]), 1.0)
        self.assertAlmostEqual(builder.spearman([1, 2, 3, 4], [1, 4, 9, 100]), 1.0)
        self.assertAlmostEqual(builder.spearman([1, 2, 3], [3, 2, 1]), -1.0)

    def test_fits(self):
        slope, intercept = builder.linear_fit([0, 1, 2], [1, 3, 5])
        self.assertAlmostEqual(slope, 2.0)
        self.assertAlmostEqual(intercept, 1.0)
        predict = builder.ridge([[0.0, 1.0], [1.0, 0.0], [2.0, 2.0], [3.0, 1.0]],
                                [2.0, 3.0, 7.0, 8.0], penalty=1e-9)
        self.assertAlmostEqual(predict([4.0, 0.0]), 9.0, places=6)

    def test_abstract_f1_and_scale(self):
        self.assertAlmostEqual(builder.abstract_f1([1, 2, 4, 4], [1, 4, 2, 4]), 0.5)
        self.assertEqual(builder.abstract_f1([4, 4], [4, 4]), 0.0)
        self.assertEqual(builder.to_five(7), 5)
        self.assertEqual(builder.to_five(1), 1)
        self.assertEqual(str(builder.rounded(2.25)), '2.2')
        self.assertEqual(str(builder.rounded(7.0)), '5.0')
        self.assertEqual(str(builder.rounded(-1.0)), '1.0')


class SourceParserTests(unittest.TestCase):
    def test_glasgow_reads_words_booleans_and_senses(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'norms.xlsx'
            workbook(path, [('Spoon', 6.5), (True, 2.0), ('bank (river)', 6.0),
                            ('bank (money)', 4.0), ('idea', 1.5)])
            words, senses = builder.glasgow(path)
        self.assertEqual(words, {'spoon': 6.5, 'true': 2.0, 'idea': 1.5})
        self.assertEqual(senses, {'bank': [6.0, 4.0]})

    def test_wordnet_follows_hypernyms_and_instances(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'wn.tar.gz'
            wordnet_archive(path)
            nouns = builder.wordnet(path)
        self.assertEqual(nouns['spoon'], (1.0, 1.0))
        self.assertEqual(nouns['idea'], (0.0, 0.0))
        self.assertEqual(nouns['pitcher'], (1.0, 0.5))

    def test_split_is_seeded_and_stable(self):
        words = [f'w{i}' for i in range(100)]
        calibration, heldout = builder.calibration_split(words)
        self.assertEqual(len(calibration), 30)
        self.assertFalse(calibration & heldout)
        self.assertEqual(builder.calibration_split(list(reversed(words)))[0], calibration)


class RaterTests(unittest.TestCase):
    batch = ['spoon', 'theory', "o'clock"]

    def test_parse_accepts_any_order_and_fences(self):
        text = "```\ntheory\t1.2\nspoon  5\no'clock\t3.4\n```\n"
        self.assertEqual(rater.parse(text, self.batch),
                         {'spoon': '5', 'theory': '1.2', "o'clock": '3.4'})

    def test_parse_accepts_a_word_line_then_a_rating_line(self):
        text = "theory\n1.2\nspoon\n5\n\no'clock\n3.4\n"
        self.assertEqual(rater.parse(text, self.batch),
                         {'spoon': '5', 'theory': '1.2', "o'clock": '3.4'})
        for text, message in [
            ("theory\n1.2\nspoon\n6\no'clock\n3", 'unparsable'),
            ("theory\n1.2\nspoon\no'clock\n3", 'unparsable'),
            ("theory\t1.2\nspoon\n5\no'clock\t3", 'unparsable'),
        ]:
            with self.assertRaisesRegex(ValueError, message):
                rater.parse(text, self.batch)

    def test_parse_rejects_bad_replies(self):
        for text, message in [
            ('spoon\t5\ntheory\t1.2', 'missing'),
            ('spoon\t5\nspoon\t4\ntheory\t1\no\'clock\t3', 'duplicate'),
            ('spoon\t5\ntheory\t1\nfork\t4\no\'clock\t3', 'unexpected'),
            ('spoon\t6\ntheory\t1\no\'clock\t3', 'unparsable'),
            ('spoon\t4.25\ntheory\t1\no\'clock\t3', 'unparsable'),
            ('Here are the ratings:\nspoon\t5', 'unparsable'),
        ]:
            with self.assertRaisesRegex(ValueError, message):
                rater.parse(text, self.batch)

    def test_batches_are_seeded_per_model_and_sample(self):
        words = [f'w{i}' for i in range(120)]
        first = rater.batches(words, 'claude-opus-5-5', 1)
        self.assertEqual([len(b) for b in first], [50, 50, 20])
        self.assertEqual(first, rater.batches(words, 'claude-opus-5-5', 1))
        self.assertNotEqual(first, rater.batches(words, 'claude-opus-5-5', 2))
        self.assertEqual(sorted(sum(first, [])), sorted(words))


class BuildTests(unittest.TestCase):
    def build(self, directory, ratings, adjudication=None):
        root = Path(directory)
        raw = root / 'raw'
        raw.mkdir(exist_ok=True)
        for path in raw.glob('*'):
            path.unlink()
        words_text = 'word\trating_from\n' + ''.join(
            f'{w}\t{w}\n' for w in sorted(ratings['claude-opus-5-5'][1])) + 'colour\tcolor\n'
        words_text = ''.join(sorted(words_text.splitlines(True)[1:], key=lambda l: l.split('\t')[0]))
        words_path = root / 'words.tsv'
        words_path.write_text('word\trating_from\n' + words_text, encoding='utf-8')
        for model, samples in ratings.items():
            for sample, values in samples.items():
                rows = [{'word': w, 'model': model, 'sample': sample, 'rating': r,
                         'batch': f'{model}-s{sample}-b000'} for w, r in values.items()]
                (raw / f'{model}-s{sample}.jsonl').write_text(
                    ''.join(json.dumps(row) + '\n' for row in rows), encoding='utf-8')
                (raw / f'{model}-s{sample}.responses.jsonl').write_text(
                    json.dumps({'batch': f'{model}-s{sample}-b000',
                                'started': '2026-10-20T18:00:00+00:00'}) + '\n', encoding='utf-8')
        glasgow = root / 'glasgow.xlsx'
        workbook(glasgow, [(w, 1 + 1.5 * (r - 1)) for w, r in ratings['claude-opus-5-5'][1].items()])
        lancaster = root / 'lancaster.csv'
        lancaster.write_text(
            'Word,' + ','.join(f'{d}.mean' for d in builder.LANCASTER_DIMENSIONS) + '\n'
            + ''.join(f'{w},' + ','.join(str((r * (k + 1)) % 5) for k in range(11)) + '\n'
                      for w, r in ratings['claude-opus-5-5'][1].items()), encoding='utf-8')
        wordnet = root / 'wn.tar.gz'
        wordnet_archive(wordnet)
        adjudication_path = root / 'adjudication.tsv'
        if adjudication:
            adjudication_path.write_text('word\tbefore\tafter\treason\n' + adjudication,
                                         encoding='utf-8')
        sources = {'glasgow': glasgow, 'lancaster': lancaster, 'wordnet': wordnet}
        digest = hashlib.sha256(words_path.read_bytes()).hexdigest()
        supplement = root / 'supplement.tsv'
        supplement.write_text('word\trating_from\n', encoding='utf-8')
        with patch.object(builder, 'WORDS', words_path), \
                patch.object(builder, 'WORDS_SHA256', digest), \
                patch.object(builder, 'SUPPLEMENT', supplement), \
                patch.object(builder, 'SUPPLEMENT_SHA256',
                             hashlib.sha256(supplement.read_bytes()).hexdigest()), \
                patch.object(builder, 'ADJUDICATION', adjudication_path), \
                patch.object(builder, 'TABLE', root / 'table.tsv'), \
                patch.object(builder, 'REVIEW', root / 'review.tsv'), \
                patch.object(builder, 'CALIBRATION_SHARE', 1.0):
            result, texts = builder.outputs(sources, raw)
        return result, {path.name: text for path, text in texts.items()}

    def ratings(self):
        words = {'spoon': 5.0, 'brick': 4.8, 'idea': 1.2, 'theory': 1.0, 'color': 3.0,
                 'hope': 1.5, 'kick': 4.4, 'fair': 2.0}
        shifted = {w: min(5.0, r + 0.3) for w, r in words.items()}
        shifted['fair'] = 2.5
        return {'claude-opus-5-5': {1: words, 2: dict(words)},
                'claude-sonnet-5-5': {1: shifted}}

    def test_build_is_byte_reproducible_and_copies_spellings(self):
        with tempfile.TemporaryDirectory() as directory:
            result, first = self.build(directory, self.ratings())
            _, second = self.build(directory, self.ratings())
        self.assertEqual(first, second)
        rows = dict(line.split('\t') for line in first['table.tsv'].splitlines()
                    if not line.startswith('#'))
        self.assertEqual(rows['colour'], rows['color'])
        self.assertEqual(len(rows), 9)
        self.assertGreater(float(rows['spoon']), float(rows['idea']))
        self.assertTrue(all(len(value) == 3 for value in rows.values()))
        self.assertIn('claude-opus-5-5 (2 samples)', first['table.tsv'])
        self.assertIn('No Glasgow ratings are included', first['table.tsv'])
        self.assertEqual(set(result['agreement']), {'claude-opus-5-5', 'claude-sonnet-5-5'})

    def test_excluded_models_are_read_but_not_used(self):
        ratings = self.ratings()
        ratings['claude-haiku-5-5'] = {1: {w: 6 - r for w, r in ratings['claude-opus-5-5'][1].items()}}
        with tempfile.TemporaryDirectory() as directory:
            _, with_haiku = self.build(directory, ratings)
            _, without = self.build(directory, self.ratings())
        self.assertIn('# claude-haiku-5-5 was also rated but is not used: ', with_haiku['table.tsv'])
        self.assertNotIn('claude-haiku-5-5', without['table.tsv'])
        rows = [[line for line in text['table.tsv'].splitlines() if not line.startswith('#')]
                for text in (with_haiku, without)]
        self.assertEqual(rows[0], rows[1])

    def test_adjudication_applies_and_checks_the_before_value(self):
        with tempfile.TemporaryDirectory() as directory:
            result, texts = self.build(directory, self.ratings())
            before = result['unedited']['fair']
            _, edited = self.build(directory, self.ratings(), f'fair\t{before}\t3.9\tinvented\n')
            self.assertIn('fair\t3.9\n', edited['table.tsv'])
            with self.assertRaises(SystemExit), contextlib.redirect_stderr(io.StringIO()):
                self.build(directory, self.ratings(), 'fair\t4.9\t3.9\tinvented\n')
            with self.assertRaises(SystemExit):
                self.build(directory, self.ratings(), f'fair\t{before}\t3.9\t \n')

    def test_missing_words_fail_loudly(self):
        ratings = self.ratings()
        del ratings['claude-sonnet-5-5'][1]['kick']
        with tempfile.TemporaryDirectory() as directory, self.assertRaisesRegex(SystemExit, 'missing'):
            self.build(directory, ratings)


class GateTests(unittest.TestCase):
    def measured(self, shares):
        files = {name: {'rated': 0, 'eligible': 0, 'share': share} for name, share in shares.items()}
        held = {'spearman': 0.9, 'pearson': 0.9, 'abstract_f1': 0.8, 'mae': 0.3}
        return {'list_covered': 1.0, 'extra_entries': 0, 'heldout': held,
                'convergent': {'spearman': 0.8}, 'stability': {}, 'agreement': {},
                'coverage': {'files': files, 'pooled': 0.95}}

    def test_coverage_gate_checks_every_modern_story(self):
        shares = {f'story{i}.txt': 0.98 for i in range(1, 6)}
        shares['moby150k.txt'] = 0.93
        self.assertEqual(gate.failures(self.measured(shares), []), [])
        shares['story3.txt'] = 0.96
        self.assertEqual(gate.failures(self.measured(shares), []),
                         ['story3.txt narration coverage 0.96 >= 0.97'])
        del shares['story3.txt']
        self.assertEqual(gate.failures(self.measured(shares), []),
                         ['story3.txt narration coverage None >= 0.97'])


if __name__ == '__main__':
    unittest.main()
