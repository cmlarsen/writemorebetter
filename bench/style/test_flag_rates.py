import contextlib
import io
import json
from pathlib import Path
import runpy
import sys
import tempfile
import unittest
from unittest.mock import patch

import common
import flag_rates
from tier3 import numeric_only, percentile

SENTINEL = 'PRIVATE_SENTINEL_FLAGS'
VOCABULARY = {'defaults': {'sentence_length_minimum_sentences': 2, 'sentence_starts_minimum_sentences': 2},
              'cliche_entries': ['dead as a doornail'], 'cliche_sources': ['no-cliches'],
              'speech_verbs': ['say', 'whisper'], 'non_speech_verbs': ['smile']}


def unit(words, long_sentences, histogram, cliches=0, entry='dead as a doornail', verb='whisper'):
    group = {'histogram': histogram, 'long_sentences': long_sentences, 'monotony_runs': 0}
    sentences = sum(histogram.values())
    starts = {'sentences': sentences, 'unknown_starts': 0, 'ambiguous_starts': 0,
              'categories': {'subject': sentences}, 'universal': {'PRON': sentences}, 'same_word_runs': 0}
    return {'words': words, 'timing_ms': {'document': 1.0},
            'sentence_length': {'all': group, 'narration': group, 'dialogue': {'histogram': {}}},
            'sentence_starts': {'all': starts, 'narration': starts, 'dialogue': {}},
            'echoes': {'echoes': 1, 'distance': {'1_5': 1}, 'form': {'same_form': 1}},
            'repeated_phrases': {'phrases': 0, 'occurrences': 0, 'length': {}},
            'cliches': {'matches': cliches, 'narration_words': words, 'dialogue_words': 0,
                        'kind': {'cliche': {'narration': cliches}}, 'source': {'no-cliches': {'narration': cliches}},
                        'entry': {entry: {'narration': cliches}}},
            'dialogue_tags': {'tags': 1, 'said_asked': 0, 'other_speech_verb': 1, 'non_speech_verb': 0,
                              'manner_adverb': 0, 'unlisted_verb': 0, 'untagged_run': 0, 'beats': 0,
                              'speech_spans': 1, 'exchanges': 1, 'verb': {verb: 1}},
            'unit_report': {}}


class FakeProbe:
    def __init__(self, units):
        self.vocabulary, self.units, self.peak_rss_bytes = VOCABULARY, iter(units), None

    def measure(self, text):
        return json.loads(json.dumps(next(self.units)))

    def close(self):
        self.peak_rss_bytes = 1


def export(directory, authors):
    (directory / 'units').mkdir()
    rows = []
    for index, author in enumerate(authors):
        unit_id = f'{SENTINEL}_UNIT_{index}'
        rows.append({'unit_id': unit_id, 'author_key': author, 'authors': [SENTINEL], 'title': SENTINEL})
        (directory / 'units' / (unit_id + '.txt')).write_text(SENTINEL + '\n', encoding='utf-8')
    (directory / 'units.jsonl').write_text('\n'.join(json.dumps(row) for row in rows) + '\n')
    return directory


class FlagRateTests(unittest.TestCase):
    def test_rates_pool_units_within_author_then_weight_authors_equally(self):
        units = [unit(1000, 1, {'10': 2}, cliches=1), unit(3000, 0, {'10': 2}), unit(500, 2, {'20': 2})]
        with tempfile.TemporaryDirectory() as temporary:
            result = flag_rates.calibrate(FakeProbe(units), export(Path(temporary), ['a', 'a', 'b']))
        self.assertEqual((result['units'], result['authors'], result['words']), (3, 2, 4500))
        long = result['reports']['sentence_length']['flags_per_1000_words']['all']['long_sentence']
        self.assertAlmostEqual(long['per_1000_words']['mean'], (1000 * 1 / 40 + 1000 * 2 / 40) / 2)
        cliche = result['reports']['cliches']['patterns']['dead as a doornail']
        self.assertEqual(cliche['authors_with_flag_share'], 0.5)
        self.assertAlmostEqual(cliche['per_1000_words']['mean'], (1000 / 4000) / 2)
        self.assertEqual(result['reports']['cliches']['top_patterns'], {'dead as a doornail': 1})
        mean = result['reports']['sentence_length']['baseline_comparison']['all']['mean']
        self.assertEqual((mean['eligible_authors'], mean['highlighted_share']), (2, 1))
        reference = flag_rates.baseline(flag_rates.LENGTH_BASELINE)['mean']
        self.assertAlmostEqual(mean['difference']['mean'], 15 - reference)
        dialogue = result['reports']['sentence_length']['baseline_comparison']['dialogue']['mean']
        self.assertEqual((dialogue['highlighted_share'], dialogue['difference']), (0, None))
        pron = result['reports']['sentence_starts']['baseline_comparison']['all']['universal']['PRON']
        self.assertAlmostEqual(pron['difference']['mean'], 100 - 35.31)
        self.assertNotIn(SENTINEL, json.dumps(result))
        self.assertTrue(numeric_only(result))

    def test_histogram_statistics_match_report_interpolation(self):
        histogram = {'3': 2, '7': 1, '12': 4, '40': 1}
        lengths = [3, 3, 7, 12, 12, 12, 12, 40]
        measured = flag_rates.histogram_statistics(histogram)
        self.assertAlmostEqual(measured['mean'], sum(lengths) / len(lengths))
        for p in flag_rates.PERCENTILES:
            self.assertAlmostEqual(measured[str(p)], percentile(lengths, p))

    def test_unlisted_keys_from_the_probe_are_rejected(self):
        for leaked in (unit(1000, 0, {'5': 2}, entry=SENTINEL), unit(1000, 0, {'5': 2}, verb=SENTINEL)):
            with tempfile.TemporaryDirectory() as temporary, self.assertRaisesRegex(ValueError, 'unexpected'):
                flag_rates.calibrate(FakeProbe([leaked]), export(Path(temporary), ['a']))

    def run_cli(self, argv):
        output, errors = io.StringIO(), io.StringIO()
        with patch.object(sys, 'argv', ['run.py'] + argv), contextlib.redirect_stdout(output), \
                contextlib.redirect_stderr(errors), self.assertRaises(SystemExit) as raised:
            runpy.run_path(str(common.HERE / 'run.py'), run_name='__main__')
        return output.getvalue() + errors.getvalue() + str(raised.exception.code)

    def test_argument_errors_hide_values_in_any_order(self):
        for argv in (['tier3', '--flag-rates', '--export', '/books/' + SENTINEL, SENTINEL],
                     ['--flag-rates', '--export', '/books/' + SENTINEL, 'tier3'],
                     ['--export', '/books/' + SENTINEL, '--flag-rates'],
                     ['--flag-rates', SENTINEL],
                     ['tier3', '--flag-rate', SENTINEL],
                     ['tier3', '--flag-rates', '--bogus-' + SENTINEL],
                     ['--bogus', SENTINEL, 'tier3', '--flag-rates'],
                     ['tier3', '--flag-rates', '--sentence-length', '--export', '/books/' + SENTINEL,
                      '--probe', '/nonexistent/' + SENTINEL]):
            combined = self.run_cli(argv)
            self.assertNotIn(SENTINEL, combined)
            self.assertIn('details suppressed', combined)

    def test_runtime_failures_hide_paths(self):
        combined = self.run_cli(['tier3', '--flag-rates', '--export', '/books/' + SENTINEL,
                                 '--probe', '/nonexistent/' + SENTINEL + '/flag_rates'])
        self.assertNotIn(SENTINEL, combined)
        self.assertIn('details suppressed', combined)
        with tempfile.TemporaryDirectory() as temporary:
            directory = export(Path(temporary), ['a'])
            with patch.object(flag_rates, 'FlagProbe', return_value=FakeProbe([])):
                combined = self.run_cli(['tier3', '--flag-rates', '--export', str(directory), '--probe', 'x'])
        self.assertNotIn(SENTINEL, combined)
        self.assertNotIn(temporary, combined)
        self.assertIn('details suppressed', combined)


if __name__ == '__main__':
    unittest.main()
