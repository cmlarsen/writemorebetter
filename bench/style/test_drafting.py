"""Drafting fixtures are well formed and recall does not drop below the baseline."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / 'drafting_fixtures.jsonl'
BASELINE = HERE / 'drafting-baseline.json'
CATEGORIES = {'placeholder', 'filledPause', 'spokenCommand'}
STYLES = {'dragon', 'apple', 'whisper'}
OPTIONS = {'placeholders', 'filledPauses', 'er', 'includeDialogue'}
# Every row of the spoken-command table in docs/STYLE-DRAFTING.md.
COMMANDS = (
    'comma', 'coma', 'period', 'full stop', 'question mark', 'exclamation point',
    'exclamation mark', 'colon', 'semicolon', 'dash', 'em dash', 'm dash', 'hyphen',
    'ellipsis', 'dot dot dot', 'open quote', 'begin quote', 'close quote', 'end quote',
    'unquote', 'open single quote', 'close single quote', 'open paren', 'open parenthesis',
    'close paren', 'close parenthesis', 'open bracket', 'close bracket', 'apostrophe',
    'new paragraph', 'new line', 'next line', 'scratch that', 'delete that', 'strike that',
    'undo that', 'caps on', 'caps off', 'all caps', 'no caps',
)
# Every row of the deletion worked-case table, plus its documented no-match twin.
DELETIONS = (
    'So um I was', 'Well, um, I was', 'I was, uh, thinking', 'I was—um—thinking',
    'I was… um… thinking', 'Um, the door', 'Um… the door', 'He left um.', 'He left um”',
    '“I was, um,” she said', '“Um, I don’t know.”', 'the um\u2028next',
    'He waited um\nThen', '“Um.”',
)
NEGATIVES = (
    'Chapter XX', 'The atkins diet is popular.', 'The ledger said [sic] in the margin.',
    'What??? she demanded.', 'He said uh-huh and nodded.', '“Um.”',
    'Every clause needs a comma before it.', 'The Edo period lasted for centuries.',
    'Add a dash of salt to the soup.', 'He made a dash for the door.',
    'I have to dash or I’ll miss the train.', 'The phrase "quote, unquote" annoyed her.',
    'The word "comma" is in the lesson.',
)


class FixtureTest(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        with FIXTURES.open(encoding='utf-8') as stream:
            cls.rows = [json.loads(line) for line in stream if line.strip()]

    def test_shape_and_size(self):
        self.assertGreaterEqual(len(self.rows), 120)
        self.assertEqual(len({row['id'] for row in self.rows}), len(self.rows))
        for row in self.rows:
            self.assertEqual(set(row) - {'options', 'flags', 'note'},
                             {'id', 'style', 'category', 'input', 'expected'})
            self.assertIn(row['style'], STYLES)
            self.assertIn(row['category'], CATEGORIES)
            self.assertIsInstance(row['input'], str)
            self.assertIsInstance(row['expected'], str)
            if 'options' in row:
                self.assertLessEqual(set(row['options']), OPTIONS)
            if 'flags' in row:
                self.assertTrue(row['flags'])
                self.assertTrue(all(isinstance(term, str) and term for term in row['flags']))
                self.assertEqual(row['expected'], row['input'])
            if row['category'] == 'placeholder' and 'flags' in row:
                self.assertEqual(row['expected'], row['input'])
            if 'note' in row:
                self.assertIsInstance(row['note'], str)
                self.assertTrue(row['note'])

    def test_negative_share_and_coverage(self):
        negatives = [row for row in self.rows
                     if 'flags' not in row and row['input'] == row['expected']]
        self.assertGreaterEqual(len(negatives), len(self.rows) // 4)
        self.assertEqual({row['style'] for row in self.rows}, STYLES)
        self.assertEqual({row['category'] for row in self.rows}, CATEGORIES)
        for category in CATEGORIES:
            self.assertTrue(any(
                row['category'] == category
                and ('flags' in row or row['expected'] != row['input'])
                for row in self.rows), category)

    def test_utf8_typography(self):
        text = '\n'.join(row['input'] for row in self.rows)
        for mark in ('“', '”', '’', '—', '…', '\u2028', '--', '...', '"'):
            self.assertIn(mark, text, mark)

    def test_command_and_deletion_tables(self):
        inputs = {row['input'] for row in self.rows}
        for case in DELETIONS:
            self.assertIn(case, inputs)
        for command in COMMANDS:
            self.assertTrue(any(command in row['input'] for row in self.rows), command)
        for text in NEGATIVES:
            row = next((row for row in self.rows if row['input'] == text), None)
            self.assertIsNotNone(row, text)
            self.assertNotIn('flags', row, text)
            self.assertEqual(row['expected'], row['input'], text)


class BaselineTest(unittest.TestCase):
    def test_thresholds_cover_categories(self):
        baseline = json.loads(BASELINE.read_text(encoding='utf-8'))
        self.assertEqual(set(baseline['thresholds']), CATEGORIES)
        self.assertTrue(all(0 <= value <= 1 for value in baseline['thresholds'].values()))
        self.assertTrue(0 <= baseline['minimumControlRate'] <= 1)

    def test_gate(self):
        result = subprocess.run([sys.executable, str(HERE / 'drafting.py'), '--gate'],
                                cwd=HERE, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)


if __name__ == '__main__':
    unittest.main()
