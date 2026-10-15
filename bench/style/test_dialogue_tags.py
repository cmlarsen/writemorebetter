import contextlib
import io
import json
import unittest
from unittest.mock import patch

import common
import dialogue_tags


class DialogueTagTests(unittest.TestCase):
    def test_labels_are_well_formed_and_unique_per_report(self):
        rows = common.jsonl(dialogue_tags.LABELS)
        files = {file for file, *_ in dialogue_tags.PASSAGES}
        seen = set()
        for row in rows:
            self.assertIn(row['file'], files)
            self.assertIn(row['report'], dialogue_tags.REPORTS)
            self.assertEqual(len(row['labelers']), 2)
            self.assertTrue(row['text'])
            key = (row['report'], row['file'], row['byte']['start'], row['byte']['end'])
            self.assertNotIn(key, seen)
            seen.add(key)
        tags = sum(row['report'] == 'dialogue_tag_position' for row in rows)
        self.assertEqual(tags, sum(row['report'] == 'dialogue_tag_verb' for row in rows))

    def test_validate_rejects_shifted_labels(self):
        text = '“Go,” she said.'
        row = {'file': 'a.txt', 'report': 'dialogue_tag_verb', 'pattern': 'speech',
               **common.offsets(text, 10, 14), 'text': 'said'}
        dialogue_tags.validate([row], {'a.txt': text})
        with self.assertRaises(ValueError):
            dialogue_tags.validate([dict(row, text='she ')], {'a.txt': text})

    def test_gate_fails_below_floor_and_on_missing_patterns(self):
        floors = {'floors': {'dialogue_beat': {'beat': {'recall': 0.5}}}}
        passing = {'splits': {'all': {'reports': {'dialogue_beat': {'patterns': {
            'beat': {'recall': 0.5}}}}}}}
        with patch.object(dialogue_tags.Path, 'read_text', return_value=json.dumps(floors)):
            dialogue_tags.gate(passing)
            failing = json.loads(json.dumps(passing))
            failing['splits']['all']['reports']['dialogue_beat']['patterns']['beat']['recall'] = 0.4
            with self.assertRaises(ValueError):
                dialogue_tags.gate(failing)
            missing = {'splits': {'all': {'reports': {'dialogue_beat': {'patterns': {}}}}}}
            with self.assertRaises(ValueError):
                dialogue_tags.gate(missing)

    def test_tier3_failure_hides_details(self):
        class Args:
            probe = '/nonexistent/PRIVATE_SENTINEL'
            tokenizer = '/nonexistent'
            self_test = False
            export = None

        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr), self.assertRaises(SystemExit) as raised:
            dialogue_tags.tier3(Args())
        self.assertNotIn('PRIVATE_SENTINEL', str(raised.exception.code) + stderr.getvalue())


if __name__ == '__main__':
    unittest.main()
