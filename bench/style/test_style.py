import copy
import hashlib
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import common
from tier1 import ewt_documents, quote_spans
from tier2 import score, toy_detector
from tier3 import PENN, calibrate, distribution


class BenchTests(unittest.TestCase):
    def test_unicode_offsets_and_detector(self):
        text = '😀 “She’s—quiet…\u2028very very.”'
        rows = toy_detector('test', text)
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual(text.encode()[row['byte']['start']:row['byte']['end']].decode(), 'very very')
        utf16 = text.encode('utf-16-le')
        self.assertEqual(utf16[row['utf16']['start'] * 2:row['utf16']['end'] * 2].decode('utf-16-le'), 'very very')
        self.assertEqual(score(rows, rows)['patterns']['repeated_word']['f1'], 1)

    def test_score_false_positives_misses_duplicates_and_confusion(self):
        gold = toy_detector('a', 'one one two two')
        wrong = copy.deepcopy(gold[1])
        wrong['pattern'] = 'echo'
        predictions = [gold[0], gold[0], wrong, {**gold[0], 'file': 'untouched'}]
        result = score(gold, predictions)
        self.assertEqual(result['patterns']['repeated_word']['precision'], 1 / 3)
        self.assertEqual(result['patterns']['repeated_word']['recall'], 1 / 2)
        self.assertEqual(result['confusion']['repeated_word']['echo'], 1)
        self.assertEqual(result, score(gold, list(reversed(predictions))))
        self.assertEqual(score(gold, [wrong, *gold])['patterns']['repeated_word']['recall'], 1)
        self.assertEqual(score(gold, [])['confusion']['repeated_word']['__none__'], 2)
        with self.assertRaises(ValueError):
            score(gold + gold, [])
        bad = copy.deepcopy(gold[0])
        bad['byte']['start'] = -1
        with self.assertRaises(ValueError):
            score([], [bad])

    def test_quote_state_crosses_paragraphs(self):
        for text in ('“One.\n\n“Two.”', '"One.\n\n"Two."', '“She’s—here…\u2028yes.”'):
            self.assertEqual(quote_spans(text), [(0, len(text))])
        self.assertEqual(quote_spans('”orphan “unfinished'), [])
        self.assertEqual(quote_spans('“a” and "b"'), [(0, 3), (8, 11)])

    def test_ewt_offsets_multiword_and_empty_nodes(self):
        conllu = ('# newdoc id = one\n# text = I can’t go.\n'
                  '1\tI\tI\tPRON\tPRP\t_\t0\troot\t_\t_\n'
                  '2-3\tcan’t\t_\t_\t_\t_\t_\t_\t_\t_\n'
                  '2\tca\tcan\tAUX\tMD\t_\t1\taux\t_\t_\n'
                  '3\tn’t\tnot\tPART\tRB\t_\t1\tadvmod\t_\t_\n'
                  '3.1\textra\textra\tX\tXX\t_\t_\t_\t_\t_\n'
                  '4\tgo\tgo\tVERB\tVB\t_\t1\txcomp\t_\t_\n'
                  '5\t.\t.\tPUNCT\t.\t_\t1\tpunct\t_\t_\n')
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'fixture.conllu'
            path.write_text(conllu, encoding='utf-8')
            documents = ewt_documents(path)
        sentence, tokens, count = documents[0][0]
        self.assertEqual(count, 5)
        self.assertEqual([sentence[s:e] for s, e, _, _ in tokens], ['I', 'can’t', 'go', '.'])

    def test_checksum_checked_even_on_cache_hits(self):
        with tempfile.TemporaryDirectory() as directory:
            here = Path(directory)
            cache = here / 'cache'
            cache.mkdir()
            entry = {'filename': 'one', 'url': 'https://invalid.example',
                     'sha256': hashlib.sha256(b'correct').hexdigest()}
            (here / 'sources.json').write_text(json.dumps({'fixture': entry}))
            (cache / 'one').write_bytes(b'wrong')
            with patch.object(common, 'HERE', here), patch.object(common, 'CACHE', cache):
                with self.assertRaises(ValueError):
                    common.fetch('fixture')

    def test_pins_splits_and_statistics(self):
        splits = json.loads((common.HERE / 'splits.json').read_text())['litbank']
        self.assertFalse(set(splits['dev']) & set(splits['test']))
        self.assertEqual(len(splits['dev']) + len(splits['test']), 100)
        self.assertEqual(PENN['IN'], 'ADP')
        self.assertEqual(PENN['VBG'], 'VERB')
        self.assertEqual(PENN['TO'], 'PRT')
        self.assertEqual(distribution([2, 4])['mean'], 3)
        self.assertEqual(distribution([2, 4])['percentiles']['50'], 3)

    def test_calibration_skips_single_quotes_without_exposing_metadata(self):
        class SyntheticAnalyzer:
            def analyze(self, text):
                tokens = []
                for word, pos in (("'", "''"), ('She', 'PRP'), ('.', '.'), ("'", "''")):
                    tokens.append({'text': word, 'is_whitespace': False,
                                   'is_punctuation': not word.isalpha(),
                                   'readings': [{'pos': pos}]})
                return [{'tokens': tokens}]

        with tempfile.TemporaryDirectory() as directory:
            export = Path(directory)
            (export / 'units').mkdir()
            (export / 'units' / 'synthetic.txt').write_text("'She.'\n")
            (export / 'units.jsonl').write_text(json.dumps({
                'unit_id': 'synthetic', 'author_key': 'PRIVATE_SENTINEL',
                'title': 'PRIVATE_SENTINEL', 'authors': ['PRIVATE_SENTINEL']}))
            baseline = json.loads((common.HERE / 'published-baseline.json').read_text())
            result = calibrate(SyntheticAnalyzer(), export, baseline)
        self.assertEqual(result['sentence_start']['PRON']['measured_percent'], 100)
        self.assertEqual(result['sentence_length']['without_punctuation']['mean'], 1)
        self.assertEqual(result['sentence_length']['with_punctuation']['mean'], 4)
        self.assertNotIn('PRIVATE_SENTINEL', json.dumps(result))


if __name__ == '__main__':
    unittest.main()
