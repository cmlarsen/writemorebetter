import copy
import hashlib
import json
import contextlib
import io
from pathlib import Path
import tempfile
import runpy
import sys
import tarfile
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
    def test_public_distribution_uses_source_wrapping_and_preserves_crlf(self):
        from sentences import public_distribution
        class Probe:
            def __init__(self, paragraphs):
                self.paragraphs, self.calls = paragraphs, []
            def analyze(self, text):
                self.calls.append(text)
                group = {'sentences': 1, 'excludedSentences': 0,
                         'categories': [{'category': 'other', 'count': 1}],
                         'universal': [{'tag': 'VERB', 'difference': 1.0}]}
                return {'paragraphs': self.paragraphs, 'reports': {
                    'sentences': [{'span': {'byte': {'start': 0, 'end': 16}}}],
                    'sentenceLength': {'groups': {'all': {'meanDifference': 0.5}}},
                    'sentenceStarts': {'groups': {'all': group},
                                       'starts': [{'category': 'other', 'word': 'call', 'sentence': 0}]}}}
        text = b'CHAPTER 1. Loomings.\r\nCONTENTS\r\n\r\nCHAPTER 1. Loomings.\r\nCall me Ishmael.\r\nThe sea waited.\r\n'
        line, wrapped = Probe(2), Probe(1)
        with patch.object(Path, 'read_bytes', return_value=text):
            result = public_distribution(line, wrapped)
            self.assertEqual((len(line.calls), len(wrapped.calls)), (5, 1))
            self.assertIn('\r\n', wrapped.calls[0])
            self.assertTrue(result['mobyDick']['inputs'][0]['hardWrapped'])
            self.assertEqual(result['mobyDick']['inputs'][0]['cut']['occurrence'], 2)
            self.assertEqual(wrapped.calls[0], 'CHAPTER 1. Loomings.\r\nCall me Ishmael.\r\nThe sea waited.\r\n')
            self.assertFalse(result['stories']['inputs'][0]['hardWrapped'])
            with self.assertRaisesRegex(ValueError, 'paragraph mode'):
                public_distribution(wrapped, line)

    def test_sentence_distribution_gate_rejects_large_other_share(self):
        from sentences import check_public_distribution
        distributions = {'stories': {'categories': {'other': {'percent': 2.8}}},
                         'mobyDick': {'categories': {'other': {'percent': 14.81}},
                                      'bundledBaselineComparison': {'meanDifference': 1.2,
                                                                    'universalDifferences': 11}}}
        ceilings = {'stories': 5.0, 'mobyDick': 16.0}
        check_public_distribution(distributions, ceilings)
        distributions['mobyDick']['bundledBaselineComparison']['meanDifference'] = None
        with self.assertRaisesRegex(ValueError, 'bundled baselines'):
            check_public_distribution(distributions, ceilings)
        distributions['mobyDick']['bundledBaselineComparison']['meanDifference'] = 1.2
        distributions['mobyDick']['categories']['other']['percent'] = 17.0
        with self.assertRaisesRegex(ValueError, 'mobyDick/other'):
            check_public_distribution(distributions, ceilings)

    def test_ngram_start_convention_skips_one_punctuation_token(self):
        punctuation = {'text': '…', 'is_whitespace': False, 'is_punctuation': True, 'readings': [{'pos':'PCT'}]}
        word = {'text': 'She', 'is_whitespace': False, 'is_punctuation': False, 'readings': [{'pos':'PRP'}]}
        self.assertEqual(start_candidates([punctuation,word],skip_punctuation=True), {'PRON'})
        self.assertEqual(start_candidates([punctuation,word]), {'.'})
        self.assertEqual(start_candidates([punctuation,punctuation,word],skip_punctuation=True), {'.'})

    def test_tier3_argparse_suppresses_values_before_and_after_subcommand(self):
        sentinel = 'PRIVATE_ARG_SENTINEL_244'
        for argv in (['run.py', '--export', sentinel, 'tier3', '--sentence-length'],
                     ['run.py', 'tier3', '--bad-' + sentinel],
                     ['run.py', '--bad-' + sentinel, 'tier3']):
            output, errors = io.StringIO(), io.StringIO()
            with patch.object(sys, 'argv', argv), contextlib.redirect_stdout(output), contextlib.redirect_stderr(errors):
                with self.assertRaises(SystemExit) as result:
                    runpy.run_path(str(common.HERE / 'run.py'), run_name='__main__')
            combined = output.getvalue() + errors.getvalue() + str(result.exception)
            self.assertNotIn(sentinel, combined)
            self.assertIn('details suppressed', str(result.exception))

    def test_litbank_document_probe_scores_both_offsets_on_annotated_prefix(self):
        original = '😀 “She’s—here…\u2028yes.”\n\nUnused “later.”'
        prefix = original[:original.index('\n\n')]
        quoted = prefix[2:]
        tokens = '😀 “ She’s — here … yes . ”'
        contents = {'original/book.txt': original,
                    'quotations/tsv/book_brat.txt': tokens + '\n',
                    'quotations/tsv/book_brat.ann': 'QUOTE\t0\t0\t1\t0\t8\t' + ' '.join(tokens.split()[1:])}
        record = {'file': 'book_brat', 'pattern': 'dialogue',
                  **common.offsets(prefix, 2, 2 + len(quoted))}

        class Probe:
            def dialogue(self, documents):
                self.documents = documents
                return self.predictions

        probe = Probe()
        with tempfile.TemporaryDirectory() as directory:
            here = Path(directory)
            (here / 'splits.json').write_text(json.dumps({'litbank': {'test': ['book_brat']}}))
            archive_path = here / 'fixture.tar'
            with tarfile.open(archive_path, 'w') as archive:
                for name, text in contents.items():
                    data = text.encode('utf-8')
                    member = tarfile.TarInfo('fixture/' + name)
                    member.size = len(data)
                    archive.addfile(member, io.BytesIO(data))
            with patch.object(tier1, 'HERE', here), patch.object(tier1, 'fetch', return_value=archive_path):
                probe.predictions = [record]
                self.assertEqual(tier1.score_litbank('test', probe)['f1'], 1)
                self.assertEqual(probe.documents, [{'file': 'book_brat', 'text': prefix}])
                wrong = copy.deepcopy(record)
                wrong['utf16']['start'] -= 1
                probe.predictions = [wrong]
                self.assertEqual(tier1.score_litbank('test', probe)['true_positive'], 0)
                probe.predictions = [record, record]
                self.assertEqual(tier1.score_litbank('test', probe)['precision'], .5)
                probe.predictions = [{**record, 'file': 'unscored'}]
                with self.assertRaisesRegex(ValueError, 'manifest'):
                    tier1.score_litbank('test', probe)

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
        self.assertEqual(normalize_lemma('those'), normalize_lemma('that'))
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
        with self.assertRaisesRegex(ValueError, 'report field'):
            score([grouped], [{k: v for k, v in grouped.items() if k != 'report'}], {'test'}, report='echoes')

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
               'mean_distinct_normalized_tags_known': 1.7,
               'sentence_starts': {'total_absolute_bias_pp': 21.5,
                                   'per_tag': {'ADJ': {'absolute_fractional_bias_pp': .003}}}}
        baseline = {'minimum': {'ewt/normalized_tag_any': .9, 'ewt/single_normalized_correct': .6},
                    'maximum': {'ewt/mean_distinct_normalized_tags_known': 1.7,
                                'ewt/sentence_starts/total_absolute_bias_pp': 22.5}}
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
                already_ambiguous = {**ewt, 'normalized_tag_any': .95, 'mean_distinct_normalized_tags_known': 2.1}
                with patch.object(tier1, 'score_ewt', return_value=already_ambiguous):
                    with self.assertRaisesRegex(ValueError, 'mean_distinct_normalized_tags_known'):
                        tier1.run(None, 'test', gate=True)
                improved = copy.deepcopy(ewt)
                improved['sentence_starts']['per_tag']['ADJ']['absolute_fractional_bias_pp'] = .05
                improved['sentence_starts']['total_absolute_bias_pp'] = 21.6
                with patch.object(tier1, 'score_ewt', return_value=improved):
                    tier1.run(None, 'test', gate=True)
                biased = copy.deepcopy(ewt)
                biased['sentence_starts']['total_absolute_bias_pp'] = 22.6
                with patch.object(tier1, 'score_ewt', return_value=biased):
                    with self.assertRaisesRegex(ValueError, 'total_absolute_bias_pp'):
                        tier1.run(None, 'test', gate=True)


if __name__ == '__main__':
    unittest.main()
