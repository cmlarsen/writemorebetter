import unittest

import common
from echoes import insert, new_flags, shift_spans


def record(file, text, pattern, *pieces):
    spans = []
    for piece, occurrence in pieces:
        start = -1
        for _ in range(occurrence + 1):
            start = text.index(piece, start + 1)
        spans.append(common.offsets(text, start, start + len(piece)))
    return {'file': file, 'pattern': pattern, 'report': 'echoes', 'spans': spans}


class EchoSuiteTests(unittest.TestCase):
    def test_insert_reports_sentence_starts_in_injected_text(self):
        clean = '“Rain—” fell… It ended.'
        text, starts = insert(clean, [(7, 'It was the lantern.'), (13, 'There were lanterns.')])
        self.assertEqual(text, '“Rain—” It was the lantern. fell… There were lanterns. It ended.')
        self.assertEqual([text[start:].split('.')[0] for start in starts],
                         ['It was the lantern', 'There were lanterns'])

    def test_only_flags_absent_from_the_clean_version_are_scored(self):
        clean = 'Rain’s fall. Rain’s gone… The end.'
        insertions = [(12, 'It was the lantern.'), (24, 'There were lanterns.')]
        injected, _ = insert(clean, insertions)
        texts = {'a-clean.txt': clean, 'a-echo.txt': injected}
        natural = [record('a-clean.txt', clean, 'echo', ('Rain', 0), ('Rain', 1))]
        shifted = record('a-echo.txt', injected, 'echo', ('Rain', 0), ('Rain', 1))
        new = record('a-echo.txt', injected, 'echo', ('lantern', 0), ('lantern', 1))
        self.assertEqual(shift_spans(((13, 17),), insertions), ((33, 37),))
        flags, lost = new_flags([{'file': 'a-echo.txt', 'clean': 'a-clean.txt', 'insertions': insertions}],
                                texts, natural + [shifted, new])
        self.assertEqual(flags, [new])
        self.assertEqual(lost, {'echo': 0, 'repeated_phrase': 0})
        flags, lost = new_flags([{'file': 'a-echo.txt', 'clean': 'a-clean.txt', 'insertions': insertions}],
                                texts, natural + [new])
        self.assertEqual(lost['echo'], 1)


if __name__ == '__main__':
    unittest.main()
