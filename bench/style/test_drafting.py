"""Drafting fixtures are well formed and recall does not drop below the baseline."""
import json
from pathlib import Path
import subprocess
import sys
import unittest

HERE = Path(__file__).resolve().parent
FIXTURES = HERE / 'drafting_fixtures.jsonl'
NEGATIVE_FILE = HERE / 'drafting_negatives.jsonl'
BASELINE = HERE / 'drafting-baseline.json'
CATEGORIES = {'placeholder', 'filledPause', 'spokenCommand'}
STYLES = {'dragon', 'apple', 'whisper'}
OPTIONS = {'placeholders', 'dictated', 'spokenCommands', 'filledPauses', 'er', 'includeDialogue'}
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
# Every row of the deletion worked-case table (edits and flag-only rows), plus its no-match twin.
DELETIONS = (
    'So um I was', 'Well, um, I was', 'I was, uh, thinking', 'I was—um—thinking',
    'I was… um… thinking', 'Um, the door', 'Um… the door', 'He left um.', 'He left um”',
    '“I was, um,” she said', '“Um, I don’t know.”', 'the um\u2028next',
    'He waited um\nThen', '“Um.”', 'He left at 3 p.m. um the door was open.',
    '“Why?” um, she asked.', 'No, um, one came to the door.', 'Why? um, she wondered.',
    'Um, iPhone sales were down.', 'What now? Um, the door was open.',
)
NEGATIVES = (
    'Chapter XX', 'The atkins diet is popular.', 'The ledger said [sic] in the margin.',
    'What??? she demanded.', 'He said uh-huh and nodded.', '“Um.”',
    'Every clause needs a comma before it.', 'The Edo period lasted for centuries.',
    'Add a dash of salt to the soup.', 'He made a dash for the door.',
    'I have to dash or I’ll miss the train.', 'The phrase "quote, unquote" annoyed her.',
    'The word "comma" is in the lesson.',
)
# drafting_negatives.jsonl must cover each trigger word this many times.
NEGATIVE_WORDS = {
    'comma': 8, 'coma': 8, 'period': 8, 'full stop': 8, 'colon': 8, 'semicolon': 8,
    'dash': 8, 'hyphen': 8, 'ellipsis': 8, 'apostrophe': 8,
    'question mark': 8, 'exclamation point': 4, 'exclamation mark': 4,
    'quote': 8, 'unquote': 6, 'paren': 4, 'parenthesis': 4, 'bracket': 8,
    'new line': 6, 'next line': 6, 'new paragraph': 6,
    'scratch that': 8, 'delete that': 8, 'strike that': 8, 'undo that': 8,
    'caps': 4, 'all caps': 4,
    'uh-huh': 4, 'uh–huh': 3, 'um-hm': 3, 'Um.': 3, '*um*': 3, 'um.edu': 3,
    'TK': 4, 'XX': 4, 'XXX': 4,
}
# Note prefixes that keep the medical, typographic, time and markup senses apart.
NEGATIVE_SENSES = (
    'medical sense', 'typographic sense', 'time sense', 'noun sense', 'idiom',
    'verb sense', 'stage direction', 'emphasis markup', 'email domain',
    'Roman numeral',
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

    def test_edits_only_for_dictated_text(self):
        # Edits and spoken commands need `dictated`: every fixture with an edit, and every
        # command fixture with a match, sets it; a default-mode fixture expects its input back.
        for row in self.rows:
            options = row.get('options', {})
            self.assertNotIn('spokenCommands', options, row['id'])
            if row['input'] != row['expected'] or (
                    row['category'] == 'spokenCommand' and 'flags' in row):
                self.assertIs(options.get('dictated'), True, row['id'])
        typed = [row for row in self.rows if not row.get('options', {}).get('dictated')]
        self.assertTrue(any('flags' in row and row['category'] == 'filledPause' for row in typed))

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


class NegativeFixtureTest(unittest.TestCase):
    """Ordinary prose must come back unchanged, whatever the trigger word means."""

    @classmethod
    def setUpClass(cls):
        with NEGATIVE_FILE.open(encoding='utf-8') as stream:
            cls.rows = [json.loads(line) for line in stream if line.strip()]

    def test_shape_and_size(self):
        self.assertGreaterEqual(len(self.rows), 160)
        self.assertEqual(len({row['id'] for row in self.rows}), len(self.rows))
        for row in self.rows:
            self.assertEqual(set(row), {'id', 'word', 'input', 'note'})
            self.assertRegex(row['id'], r'^neg-\d{3}$')
            self.assertTrue(row['word'])
            self.assertIsInstance(row['input'], str)
            self.assertTrue(row['input'].strip())
            self.assertIsInstance(row['note'], str)
            self.assertTrue(row['note'].strip())

    def test_word_coverage(self):
        counts = {}
        for row in self.rows:
            counts[row['word']] = counts.get(row['word'], 0) + 1
        for word, minimum in NEGATIVE_WORDS.items():
            self.assertGreaterEqual(counts.get(word, 0), minimum, word)

    def test_varied_positions(self):
        notes = [row['note'] for row in self.rows]
        self.assertGreaterEqual(sum('sentence start' in note for note in notes), 15)
        self.assertGreaterEqual(sum('dialogue' in note for note in notes), 8)
        self.assertGreaterEqual(sum('before a comma' in note for note in notes), 6)
        self.assertGreaterEqual(sum('after an adjective' in note for note in notes), 6)

    def test_sense_coverage(self):
        notes = [row['note'] for row in self.rows]
        for sense in NEGATIVE_SENSES:
            self.assertGreaterEqual(sum(note.startswith(sense) for note in notes), 2, sense)

    def test_utf8_typography(self):
        marked = 0
        for row in self.rows:
            if any(mark in row['input'] for mark in ('“', '”', '’', '—', '…', '\u2028')):
                marked += 1
        self.assertGreaterEqual(marked, 30)
        text = '\n'.join(row['input'] for row in self.rows)
        for mark in ('“', '”', '’', '—', '…', '\u2028'):
            self.assertIn(mark, text, mark)


class BaselineTest(unittest.TestCase):
    def test_thresholds_cover_categories(self):
        baseline = json.loads(BASELINE.read_text(encoding='utf-8'))
        self.assertEqual(set(baseline['thresholds']), CATEGORIES)
        self.assertTrue(all(0 <= value <= 1 for value in baseline['thresholds'].values()))
        self.assertTrue(0 <= baseline['minimumControlRate'] <= 1)
        self.assertEqual(baseline['minimumNegativeRate'], 1.0)

    def test_gate(self):
        result = subprocess.run([sys.executable, str(HERE / 'drafting.py'), '--gate'],
                                cwd=HERE, capture_output=True, text=True)
        output = result.stdout + result.stderr
        self.assertIn('| negatives, default |', output)
        self.assertIn('| negatives, dictated |', output)
        self.assertIn('Every negative is silent in both modes.', output)
        self.assertRegex(output, r'without `dictated`: 0 with edits\.')
        self.assertEqual(result.returncode, 0, output)

if __name__ == '__main__':
    unittest.main()
