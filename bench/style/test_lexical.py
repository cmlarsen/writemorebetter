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
        self.assertEqual(len(cases), 14)
        for _, index, prefix, report, pattern, suffix, spans in cases:
            self.assertEqual(index, 0)
            text = prefix + suffix
            for label, start, end in spans:
                span = offsets(text, start, end)
                self.assertEqual(text[start:end], text.encode()[span['byte']['start']:span['byte']['end']].decode())
                if report == 'diction':
                    self.assertEqual(text[start:end].strip(), 'An idea, a belief, a truth.')
            if pattern.endswith('_below_count') or pattern == 'concrete_control':
                self.assertEqual(spans, [])

    def test_gate_rejects_metric_and_control_regressions(self):
        baseline = {'documents': 2, 'paired_controls': 2, 'changed_controls': 0,
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

    def test_wrong_modality_and_duplicate_are_false_positives(self):
        gold = {'file': 'x', 'report': 'sensoryBalance', 'pattern': 'sight', **offsets('red', 0, 3)}
        wrong = {**gold, 'pattern': 'sound'}
        result = score([gold], [wrong], {'x'}, report='sensoryBalance')
        self.assertEqual(result['confusion']['sight']['sound'], 1)
        result = score([gold], [gold, gold], {'x'}, report='sensoryBalance')
        self.assertEqual(result['patterns']['sight']['precision'], 0.5)
