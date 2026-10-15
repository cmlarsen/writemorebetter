import copy
import hashlib
import json
import contextlib
import io
from pathlib import Path
import tempfile
import runpy
import sys
import unittest
from unittest.mock import patch

import common
import tier1
from tier1 import align_litbank, ewt_paragraphs, quote_spans
from tier2 import score, toy_detector
from tier3 import calibrate, distribution
from equivalence import PENN, add_start, normalize_lemma, normalize_pos, start_candidates
from collections import Counter


class BenchTests(unittest.TestCase):
    def test_unicode_offsets_and_detector(self):
        text = '😀 “She’s—quiet…\u2028very very.”'
        rows = toy_detector('test', text)
        self.assertEqual(len(rows), 1)
        row = rows[0]
        self.assertEqual(text.encode()[row['byte']['start']:row['byte']['end']].decode(), 'very very')
        utf16 = text.encode('utf-16-le')
        self.assertEqual(utf16[row['utf16']['start'] * 2:row['utf16']['end'] * 2].decode('utf-16-le'), 'very very')
        self.assertEqual(score(rows, rows, {'test'})['patterns']['repeated_word']['f1'], 1)

    def test_score_false_positives_misses_duplicates_and_confusion(self):
        gold = toy_detector('a', 'one one two two')
        wrong = copy.deepcopy(gold[1])
        wrong['pattern'] = 'echo'
        predictions = [gold[0], gold[0], wrong, {**gold[0], 'file': 'untouched'}]
        result = score(gold, predictions, {'a', 'untouched'})
        self.assertEqual(result['patterns']['repeated_word']['precision'], 1 / 3)
        self.assertEqual(result['patterns']['repeated_word']['recall'], 1 / 2)
        self.assertEqual(result['confusion']['repeated_word']['echo'], 1)
        self.assertEqual(result, score(gold, list(reversed(predictions)), {'a', 'untouched'}))
        self.assertEqual(score(gold, [wrong, *gold], {'a'})['patterns']['repeated_word']['recall'], 1)
        self.assertEqual(score(gold, [], {'a'})['confusion']['repeated_word']['__none__'], 2)
        with self.assertRaises(ValueError):
            score(gold + gold, [], {'a'})
        bad = copy.deepcopy(gold[0])
        bad['byte']['start'] = -1
        with self.assertRaises(ValueError):
            score([], [bad], {'a'})

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
            path.write_text(conllu + '\n' + conllu.replace('# newdoc id = one', '# newpar id = two'), encoding='utf-8')
            paragraphs = ewt_paragraphs(path)
            path.write_text(conllu.replace('2\tca\tcan', '2\tcan\tcan'), encoding='utf-8')
            mismatch = ewt_paragraphs(path)[0][0]
        self.assertEqual(len(paragraphs), 2)
        self.assertEqual(mismatch['excluded_components'], 2)
        sentence = paragraphs[0][0]
        self.assertEqual(sentence['gold_count'], 5)
        self.assertEqual(sentence['excluded_components'], 0)
        self.assertEqual([sentence['text'][t['start']:t['end']] for t in sentence['tokens']],
                         ['I', 'ca', 'n’t', 'go', '.'])

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
        self.assertEqual(result['sentence_start']['fractional']['PRON']['measured_percent'], 100)
        self.assertEqual(result['sentence_length']['without_punctuation']['mean'], 1)
        self.assertEqual(result['sentence_length']['with_punctuation']['mean'], 4)
        self.assertNotIn('PRIVATE_SENTINEL', json.dumps(result))

    def test_m1_and_lemma_conventions(self):
        for raw, expected in (('NN:UN', 'NN'), ('PRP_S1S', 'PRP'), ('PRP$_A1S', 'PRP$'),
                              ('RB_SENT', 'RB'), ('VBG', 'VBG')):
            self.assertEqual(normalize_pos(raw), expected)
        self.assertEqual(normalize_pos('PCT', '?'), '.')
        self.assertEqual(normalize_pos('PCT', ';'), ':')
        self.assertEqual(normalize_lemma('Could'), normalize_lemma('can'))
        self.assertEqual(normalize_lemma('These'), normalize_lemma('this'))
        token = {'text': 'Water', 'is_whitespace': False,
                 'readings': [{'pos': p} for p in ('NN:UN', 'VB', 'VBP')]}
        candidates = start_candidates([token])
        resolved, fractional = Counter(), Counter()
        add_start(resolved, fractional, candidates)
        self.assertEqual(candidates, {'NOUN', 'VERB'})
        self.assertEqual(resolved['VERB'], 0)
        self.assertEqual(fractional, {'NOUN': .5, 'VERB': .5})

    def test_manifest_report_filter_and_multiple_occurrences(self):
        rows = toy_detector('test', 'one one two two')
        grouped = {'file': 'test', 'pattern': 'echo', 'report': 'echoes',
                   'spans': [{key: row[key] for key in ('byte', 'utf16')} for row in rows]}
        reversed_group = {**grouped, 'spans': list(reversed(grouped['spans']))}
        other = {**grouped, 'report': 'other'}
        self.assertEqual(score([grouped, other], [reversed_group], {'test'}, report='echoes')['patterns']['echo']['f1'], 1)
        with self.assertRaises(ValueError):
            score(rows, [{**rows[0], 'file': 'typo'}], {'test'})

    def test_litbank_original_mapping_and_paragraph_start(self):
        original = '“Hello,” she said.\n\n“More.”\n\nUnannotated remainder.'
        tokenized = '“ Hello , ” she said .\n“ More . ”\n'
        mapped = align_litbank(tokenized, original)
        self.assertEqual(original[mapped[0][0][0]:mapped[0][3][1]], '“Hello,”')
        self.assertEqual(original[mapped[1][0][0]:mapped[1][-1][1]], '“More.”')
        self.assertEqual(quote_spans('"One.\n"Two."'), [(0, 7)])

    def test_calibration_suppresses_base_exceptions(self):
        stdout, stderr = io.StringIO(), io.StringIO()
        with patch.object(sys, 'argv', ['run.py', 'tier3', '--self-test']), \
                patch.object(common, 'Analyzer', side_effect=KeyboardInterrupt('PRIVATE_SENTINEL')), \
                contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            with self.assertRaises(SystemExit) as raised:
                runpy.run_path(str(common.HERE / 'run.py'), run_name='__main__')
        self.assertEqual(str(raised.exception), 'calibration failed; details suppressed to protect source metadata')
        self.assertEqual(stdout.getvalue() + stderr.getvalue(), '')

    def test_accuracy_ambiguity_and_bias_gates(self):
        ewt = {'normalized_tag_any': .9, 'single_normalized_correct': .6,
               'sentence_starts': {'per_tag': {'NOUN': {'absolute_fractional_bias_pp': 1}}}}
        baseline = {'minimum': {'ewt/normalized_tag_any': .9, 'ewt/single_normalized_correct': .6},
                    'maximum': {'ewt/sentence_starts/per_tag/NOUN/absolute_fractional_bias_pp': 1}}
        with tempfile.TemporaryDirectory() as directory:
            here = Path(directory)
            (here / 'tier1-baseline.json').write_text(json.dumps(baseline))
            with patch.object(tier1, 'HERE', here), patch.object(tier1, 'score_litbank', return_value={}), \
                    contextlib.redirect_stdout(io.StringIO()):
                with patch.object(tier1, 'score_ewt', return_value=ewt):
                    tier1.run(None, 'test', gate=True)
                ambiguous = {**ewt, 'normalized_tag_any': .95, 'single_normalized_correct': .59}
                with patch.object(tier1, 'score_ewt', return_value=ambiguous):
                    with self.assertRaisesRegex(ValueError, 'single_normalized_correct'):
                        tier1.run(None, 'test', gate=True)
                biased = copy.deepcopy(ewt)
                biased['sentence_starts']['per_tag']['NOUN']['absolute_fractional_bias_pp'] = 1.1
                with patch.object(tier1, 'score_ewt', return_value=biased):
                    with self.assertRaisesRegex(ValueError, 'absolute_fractional_bias_pp'):
                        tier1.run(None, 'test', gate=True)


if __name__ == '__main__':
    unittest.main()
