"""Held-out public-text predictions must equal the reviewed baseline."""
import json
import subprocess
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


class NameSpellingBaselineTest(unittest.TestCase):
    def test_natural_predictions(self):
        result = subprocess.run(['python3', str(HERE / 'name_spelling.py'), '--gate'],
                                check=True, capture_output=True, text=True)
        measured = json.loads(result.stdout)['test']
        baseline = json.loads((HERE / 'name-spelling-baseline.json').read_text())
        self.assertEqual({key: measured[key] for key in baseline}, baseline)


if __name__ == '__main__':
    unittest.main()
