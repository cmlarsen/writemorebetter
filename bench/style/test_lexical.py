import copy
import unittest
from unittest.mock import patch

import lexical
from common import offsets
from tier2 import score


class LexicalTests(unittest.TestCase):
    def test_answer_key_spans_include_sentence_whitespace_and_unicode(self):
        passages = [('pride', 0, '“She’s here”—perhaps…\u2028she waited.'),
                    ('moby', 30, 'He waited by the gate.')]
        with patch.object(lexical, 'public_passages', return_value=iter(passages)):
            cases = list(lexical.generate('dev'))
        self.assertEqual(len(cases), 26)
        for _, index, prefix, report, pattern, suffix, spans in cases:
            self.assertEqual(index, 0)
            text = prefix + suffix
            for label, start, end in spans:
                span = offsets(text, start, end)
                self.assertEqual(text[start:end], text.encode()[span['byte']['start']:span['byte']['end']].decode())
                if label == 'abstract':
                    self.assertIn(text[start:end].strip(), (lexical.ABSTRACT, lexical.NEAR_ABSTRACT))
                elif label == 'run':
                    self.assertEqual(text[start:end], ' '.join([lexical.ABSTRACT] * 3))
                elif report == 'fillers':
                    self.assertEqual(text[start:end], label)
            if pattern.endswith(('_below_count', '_control')):
                self.assertEqual(spans, [])
        labels = {pattern: [label for label, _, _ in spans] for _, _, _, report, pattern, _, spans in cases
                  if report == 'diction'}
        self.assertEqual(labels['abstract'], ['abstract'] * 3 + ['run'])
        self.assertEqual(labels['broken_run'], ['abstract'] * 3)
        self.assertEqual(labels['near_threshold'], ['abstract'])

    def test_overused_controls_skip_paragraphs_with_the_lemma(self):
        passages = [('pride', 0, 'She glanced and said nothing.')]
        with patch.object(lexical, 'public_passages', return_value=iter(passages)):
            patterns = [pattern for *_, pattern, _, _ in lexical.generate('dev')]
        self.assertNotIn('glance', patterns)
        self.assertNotIn('below_ratio_control', patterns)
        self.assertIn('shrug', patterns)

    def test_gate_rejects_metric_and_control_regressions(self):
        natural = lexical.natural_summary([{'file': 'x', 'report': 'fillers', 'pattern': 'very',
                                            **offsets('very', 0, 4)}])
        baseline = {'documents': 2, 'paired_controls': 2, 'changed_controls': 0, 'natural': natural,
                    'reports': {'diction': {'patterns': {'abstract': {
                        'precision': 1, 'recall': 1, 'f1': 1, 'gold': 3}}}}}
        lexical.gate(baseline, baseline)
        for field in ('precision', 'recall', 'f1', 'gold'):
            actual = copy.deepcopy(baseline)
            actual['reports']['diction']['patterns']['abstract'][field] = 0
            with self.assertRaises(ValueError):
                lexical.gate(actual, baseline)
        actual = copy.deepcopy(baseline)
        actual['changed_controls'] = 1
        with self.assertRaises(ValueError):
            lexical.gate(actual, baseline)
        actual = copy.deepcopy(baseline)
        actual['reports']['diction']['patterns']['unexpected'] = {'precision': 0}
        with self.assertRaises(ValueError):
            lexical.gate(actual, baseline)
        actual = copy.deepcopy(baseline)
        actual['reports']['fillers'] = {'patterns': {}}
        with self.assertRaises(ValueError):
            lexical.gate(actual, baseline)
        actual = copy.deepcopy(baseline)
        actual['natural'] = lexical.natural_summary([{'file': 'x', 'report': 'fillers', 'pattern': 'very',
                                                      **offsets('a very', 2, 6)}])
        with self.assertRaises(ValueError):
            lexical.gate(actual, baseline)

    def test_paired_prefix_comparison_ignores_report_order(self):
        rows = [{'file': 'x', 'report': 'fillers', 'pattern': word, **offsets('very just', start, end)}
                for word, start, end in (('very', 0, 4), ('just', 5, 9))]
        self.assertEqual(sorted(map(lexical.identity, rows)), sorted(map(lexical.identity, rows[::-1])))
        self.assertEqual(lexical.natural_summary(rows), lexical.natural_summary(rows[::-1]))

    def test_wrong_modality_and_duplicate_are_false_positives(self):
        gold = {'file': 'x', 'report': 'sensoryBalance', 'pattern': 'sight', **offsets('red', 0, 3)}
        wrong = {**gold, 'pattern': 'sound'}
        result = score([gold], [wrong], {'x'}, report='sensoryBalance')
        self.assertEqual(result['confusion']['sight']['sound'], 1)
        result = score([gold], [gold, gold], {'x'}, report='sensoryBalance')
        self.assertEqual(result['patterns']['sight']['precision'], 0.5)
