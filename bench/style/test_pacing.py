import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

import pacing


class FakeProbe:
    def report(self, text):
        dialogue = text.startswith('“')
        slow = 'remembered' in text
        return {'words': {'all': 10}, 'slowRuns': [{'paragraphs': {'start': 0, 'end': 1}}] if slow else [],
                'paragraphs': [{'signals': dict(zip(pacing.SIGNALS,
                               [100.0 if dialogue else 0.0, 10.0, 50.0 if slow else 0.0, 40.0 if slow else 0.0]))}]}


class PacingTests(unittest.TestCase):
    def test_author_pooling_and_no_metadata(self):
        with tempfile.TemporaryDirectory() as directory:
            export = Path(directory)
            (export / 'units').mkdir()
            rows = []
            for unit, author, text in [('a1', 'SECRET_A', 'remembered'), ('a2', 'SECRET_A', 'action'),
                                       ('b1', 'SECRET_B', 'action')]:
                rows.append({'unit_id': unit, 'author_key': author, 'title': 'SECRET_TITLE'})
                (export / 'units' / (unit + '.txt')).write_text(text)
            (export / 'units.jsonl').write_text('\n'.join(json.dumps(r) for r in rows))
            result = pacing.calibrate(FakeProbe(), export)
            self.assertEqual(result['authors'], 2)
            self.assertEqual(result['authorMeanSignals']['pastPerfectPercent']['mean'], 12.5)
            self.assertEqual(result['authorFlagRates']['runParagraphPercent']['mean'], 25)
            self.assertEqual(result['authorsWithRunsPercent'], 50)
            self.assertNotIn('words', json.dumps(result))
            self.assertNotIn('SECRET', json.dumps(result))
            self.assertTrue(pacing.aggregate_only(result))
            rows[0]['unit_id'] = '../escape'
            (export / 'units.jsonl').write_text(json.dumps(rows[0]))
            with self.assertRaises(ValueError):
                pacing.calibrate(FakeProbe(), export)

    def test_output_key_allowlist(self):
        self.assertTrue(pacing.aggregate_only({'authors': 2, 'paragraphSignals': {'mean': None}}))
        self.assertFalse(pacing.aggregate_only({'authorFlagRates': [{'runsPerThousandWords': 1.0}]}))
        self.assertFalse(pacing.aggregate_only({'PRIVATE_SENTINEL': 3}))
        self.assertFalse(pacing.aggregate_only({'words': 'PRIVATE_SENTINEL'}))

    def test_private_errors_are_suppressed(self):
        command = [sys.executable, str(pacing.ROOT / 'bench/style/run.py')]
        for args in [('tier3', '--pacing', '--export', 'PRIVATE_SENTINEL', '--typo'),
                     ('tier3', '--pacing', '--probe', 'PRIVATE_SENTINEL', '--self-test')]:
            result = subprocess.run(command + list(args), capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(result.stdout, '')
            self.assertEqual(result.stderr.strip(), 'calibration failed; details suppressed to protect source metadata')

    def test_injections_have_utf8_aligned_gold(self):
        for _, text, _, spans in pacing.cases():
            for span in spans:
                raw = text.encode('utf-8')[span['byte']['start']:span['byte']['end']].decode('utf-8')
                units = text.encode('utf-16-le')[2 * span['utf16']['start']:2 * span['utf16']['end']].decode('utf-16-le')
                self.assertEqual(raw, units)


if __name__ == '__main__':
    unittest.main()
