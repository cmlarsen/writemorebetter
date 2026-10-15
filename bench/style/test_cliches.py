import unittest

import cliches


class ClicheSuiteTest(unittest.TestCase):
    def test_parse_marks_gold_spans(self):
        text, spans = cliches.parse('It was [a far cry] from [home].')
        self.assertEqual(text, 'It was a far cry from home.')
        self.assertEqual([text[a:b] for a, b in spans], ['a far cry', 'home'])

    def test_templates_have_consistent_labels(self):
        for template, kind, context in cliches.TEMPLATES:
            _text, spans = cliches.parse(template)
            self.assertEqual(bool(spans), kind is not None, template)
            self.assertIn(context, (None, 'narration', 'dialogue'))

    def test_natural_hits_are_shifted_and_excluded(self):
        clean = 'He was “high and dry”.'
        sentence, spans = cliches.parse('It was [a far cry].')
        text, at, length = cliches.inject(clean, sentence)
        insertion = text[at:at + length]
        begin = clean.index('high')
        natural = {'file': 'f', 'pattern': 'cliche.dialogue',
                   **cliches.offsets(clean, begin, begin + len('high and dry'))}
        moved = cliches.shift_rows([natural], clean, at, insertion)[0]
        self.assertEqual(text.encode()[moved['byte']['start']:moved['byte']['end']].decode(),
                         'high and dry')
        start, end = spans[0]
        gold = [{'file': 'f', 'report': 'cliches', 'pattern': 'cliche.narration',
                 **cliches.offsets(text, start, end)}]
        hit = {**gold[0], 'context': 'narration'}
        scored, natural_count, carried = cliches.evaluate(
            [('f', clean, text, at, insertion)], [{'file': 'f'}], gold, [natural], [hit, moved])
        self.assertEqual((natural_count, carried), (1, 1))
        self.assertEqual(scored['patterns']['cliche.narration']['f1'], 1.0)


if __name__ == '__main__':
    unittest.main()
