"""Build-format regressions using invented ratings only."""
import contextlib
import csv
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


class LancasterBuildTests(unittest.TestCase):
    def test_flooring_and_exact_decimal_boundaries(self):
        script = Path(__file__).resolve().parents[2] / 'scripts/build-lancaster-norms.py'
        spec = importlib.util.spec_from_file_location('lancaster_builder', script)
        builder = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(builder)
        with tempfile.TemporaryDirectory() as directory:
            source, output = Path(directory) / 'input.csv', Path(directory) / 'output.tsv'
            with source.open('w', newline='') as handle:
                writer = csv.writer(handle)
                writer.writerow(['Word', *(name + '.mean' for name, _ in builder.COLUMNS)])
                writer.writerow(['belowstrength', 0, '3.952', 0, 0, 0, 0])
                writer.writerow(['belowmargin', '3.1', '4.0995', 0, 0, 0, 0])
                writer.writerow(['exactmargin', '3.333333333', '4.333333333', 0, 0, 0, 0])
                writer.writerow(['floorboundary', 3, '3.5', 0, 0, 0, 0])
            digest = hashlib.sha256(source.read_bytes()).hexdigest()
            with patch.object(builder, 'SHA256', digest), patch.object(builder.sys, 'argv',
                    ['builder', str(source), str(output)]), contextlib.redirect_stdout(io.StringIO()):
                builder.main()
            rows = {fields[0]: fields[1:] for line in output.read_text().splitlines()
                    if not line.startswith('#') and (fields := line.split('\t'))}
            self.assertEqual(rows['belowstrength'], ['a', '39', '39'])
            self.assertEqual(rows['belowmargin'], ['a', '40', '9'])
            self.assertEqual(rows['exactmargin'], ['a', '43', '10'])
            self.assertEqual(rows['floorboundary'], ['a', '35', '5'])
