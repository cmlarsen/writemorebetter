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
from flag_rates import numeric_only
from tier3 import percentile

SENTINEL = 'PRIVATE_SENTINEL_FLAGS'
VOCABULARY = {'defaults': {'sentence_length_minimum_sentences': 2, 'sentence_starts_minimum_sentences': 2,
                           'phrase_min_length': 2, 'phrase_max_length': 5},
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
        self.vocabulary, self.peak_rss_bytes = VOCABULARY, None
        self.units = iter([item for item in units for _ in range(2)])

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
    def test_lexical_aggregates_pool_ratings_without_leaking_lemmas(self):
        a = {'words': 100, 'overused_words': {'overused': 2, 'occurrences': 12, 'fillers': 3},
             'sensory_balance': {'sense_words': 20, 'modalities': {'sight': 20}},
             'diction': {'abstract_sentences': 3, 'abstract_runs': 1, 'rated_words': 9, 'rating_sum': 18}}
        b = {'words': 200, 'diction': {'rated_words': 3, 'rating_sum': 12}}
        flag_rates.merge(a, b)
        result = flag_rates.lexical_reports([('A', a), ('B', {'words': 100})])
        self.assertEqual(result['overused_words']['categories']['overused']['flagged_share'], 0.5)
        self.assertEqual(result['sensory_balance']['categories']['sense_words']['flagged_share'], 0.5)
        self.assertEqual(result['diction']['mean_rating']['mean'], 2.5)
        flag_rates.check(result, flag_rates.allowed_keys(VOCABULARY))


    def test_discourse_and_sensory_measures_respect_floors_and_author_weights(self):
        def sample(sentences, containing, she, runs, sight, senses):
            types = {kind: {'containing': containing, 'opening': 0} for kind in flag_rates.TRANSITION_TYPES}
            transitions = {'sentences': sentences, 'containing': containing, 'opening': 0, 'types': types}
            pronouns = {'sentences': sentences, 'pronoun_starts': she, 'pronouns': {'she': she}, 'other': 0,
                        'runs': runs}
            sensory = {'words': 1000, 'sense_words': senses, 'modalities': {'sight': sight}}
            histogram = {'10': sentences}
            return {'words': 10 * sentences,
                    'sentence_length': {group: {'histogram': histogram} for group in flag_rates.GROUPS},
                    'transitions': {'groups': {group: transitions for group in flag_rates.GROUPS},
                                    'repeats': {'all': runs, 'narration': runs},
                                    'repeat_window': {'3': {'all': 0}, '5': {'all': runs}}},
                    'pronoun_starts': {'groups': {group: pronouns for group in flag_rates.GROUPS},
                                       'runs': {'all': runs, 'narration': runs}},
                    'sensory_balance': {'groups': {group: sensory for group in flag_rates.GROUPS}},
                    'overused_ratio_sweep': {'4': runs, '8': 0}}
        samples = [('a', sample(100, 10, 50, 1, 30, 40)), ('a', sample(100, 30, 30, 0, 10, 10)),
                   ('b', sample(40, 40, 40, 1, 20, 20))]
        defaults = {'transitions_minimum_sentences': 50, 'pronoun_starts_minimum_sentences': 50,
                    'sensory_minimum_sense_words': 20}
        transitions = flag_rates.transitions(samples, defaults)
        containing = transitions['measures']['all']['containing']
        self.assertEqual((containing['samples'], containing['authors'], containing['mean']), (2, 1, 20))
        self.assertAlmostEqual(transitions['measures']['narration']['eligible_share'], 0.5)
        self.assertEqual(transitions['flags_per_1000_words']['repeats']['all']['flagged_share'], 0.75)
        self.assertEqual(transitions['repeat_window']['3']['all']['flagged_share'], 0)
        pronouns = flag_rates.pronoun_starts(samples, defaults)
        self.assertEqual(pronouns['measures']['dialogue']['pronouns']['she']['mean'], 40)
        self.assertEqual(pronouns['measures']['all']['pronouns']['he']['mean'], 0)
        lexical = flag_rates.lexical_reports(samples, defaults)
        self.assertNotIn('diction', lexical)
        sight = lexical['sensory_balance']['measures']['all']['modality_percent']['sight']
        self.assertEqual((sight['samples'], sight['mean']), (2, (75 + 100) / 2))
        self.assertEqual(lexical['overused_words']['minimum_ratio']['8']['flagged_share'], 0)
        result = {'transitions': transitions, 'pronoun_starts': pronouns, **lexical}
        flag_rates.check(result, flag_rates.allowed_keys(VOCABULARY))
        samples[0][1]['pronoun_starts']['groups']['all']['pronouns'][SENTINEL] = 1
        self.assertNotIn(SENTINEL, json.dumps(flag_rates.pronoun_starts(samples, defaults)))

    def test_manuscripts_pool_units_and_chapters_weight_authors_equally(self):
        units = [unit(1000, 1, {'10': 2}, cliches=1), unit(3000, 0, {'10': 2}), unit(500, 2, {'20': 2})]
        with tempfile.TemporaryDirectory() as temporary:
            result = flag_rates.calibrate(FakeProbe(units), export(Path(temporary), ['a', 'a', 'b']))
        self.assertEqual((result['units'], result['authors'], result['words']), (3, 2, 4500))
        manuscript, chapter = result['manuscript'], result['chapter']
        self.assertEqual((manuscript['samples'], chapter['samples']), (2, 3))
        long = manuscript['reports']['sentence_length']['flags_per_1000_words']['all']['long_sentence']
        self.assertAlmostEqual(long['per_1000_words']['mean'], (1000 * 1 / 40 + 1000 * 2 / 40) / 2)
        long = chapter['reports']['sentence_length']['flags_per_1000_words']['all']['long_sentence']
        self.assertAlmostEqual(long['per_1000_words']['mean'], (0.5 * 50 + 0.5 * 0 + 50) / 2)
        self.assertAlmostEqual(long['flagged_share'], (0.5 + 1) / 2)
        self.assertEqual((long['per_1000_words']['samples'], long['per_1000_words']['authors']), (3, 2))
        cliche = manuscript['reports']['cliches']['patterns']['dead as a doornail']
        self.assertEqual(cliche['flagged_share'], 0.5)
        self.assertAlmostEqual(cliche['per_1000_words']['mean'], (1000 / 4000) / 2)
        self.assertEqual(chapter['reports']['cliches']['patterns']['dead as a doornail']['flagged_share'], 0.25)
        self.assertEqual(manuscript['reports']['cliches']['top_patterns'], {'dead as a doornail': 1})
        reference = flag_rates.baseline(flag_rates.LENGTH_BASELINE)['mean']
        mean = manuscript['reports']['sentence_length']['baseline_comparison']['all']['mean']
        self.assertEqual(mean['eligible_share'], 1)
        self.assertAlmostEqual(mean['difference']['mean'], 15 - reference)
        measured = chapter['reports']['sentence_length']['measures']['all']['mean']
        self.assertAlmostEqual(measured['mean'], (0.5 * 10 + 0.5 * 10 + 20) / 2)
        self.assertEqual(set(measured['percentiles']), {str(p) for p in flag_rates.PERCENTILES})
        dialogue = chapter['reports']['sentence_length']['baseline_comparison']['dialogue']['mean']
        self.assertEqual((dialogue['eligible_share'], dialogue['difference']), (0, None))
        pron = manuscript['reports']['sentence_starts']['baseline_comparison']['all']['universal']['PRON']
        self.assertAlmostEqual(pron['difference']['mean'], 100 - flag_rates.baseline(flag_rates.STARTS_BASELINE)['universalPercent']['PRON'])
        self.assertEqual(manuscript['reports']['sentence_starts']['measures']['all']['universal']['PRON']['mean'], 100)
        self.assertNotIn(SENTINEL, json.dumps(result))
        self.assertTrue(numeric_only(result))

    def test_windows_close_at_scene_breaks_and_merge_short_tails(self):
        rule = {'scene_break_words': 3, 'maximum_words': 6, 'minimum_words': 2}
        self.assertEqual(flag_rates.windows('a b c\nd e\n\nf g\nh\n\ni j k\n', rule),
                         ['a b c\nd e\n', 'f g\nh\n', 'i j k\n'])
        self.assertEqual(flag_rates.windows('one two three\n\nfour\n', rule), ['one two three\n\nfour\n'])
        self.assertEqual(flag_rates.windows('one two\nthree four five six seven\neight\n\n\nnine ten\n', rule),
                         ['one two\nthree four five six seven\n', 'eight\n\nnine ten\n'])
        text = flag_rates.synthetic_text(7, 1500)
        split = flag_rates.windows(text)
        self.assertEqual(''.join(split).split(), text.split())
        sizes = [flag_rates.window_words(window) for window in split]
        self.assertTrue(all(2000 <= size for size in sizes))
        self.assertTrue(all(size < 6000 + 60 for size in sizes[:-1]))

    def test_weighted_percentiles_match_linear_rule_at_equal_weights(self):
        values = [3, 1, 4, 1, 5, 9, 2, 6]
        pairs = flag_rates.weighted([(index, value) for index, value in enumerate(values)])
        for p in flag_rates.PERCENTILES:
            self.assertAlmostEqual(flag_rates.weighted_percentile(pairs, p), percentile(values, p))
        self.assertEqual(flag_rates.spread([('a', 1), ('a', 3), ('b', 10)])['mean'], 6)

    def test_author_keys_must_be_nonempty_strings(self):
        for author in (None, '', ['a']):
            with tempfile.TemporaryDirectory() as temporary, self.assertRaisesRegex(ValueError, 'author'):
                flag_rates.calibrate(FakeProbe([unit(10, 0, {'5': 2})]), export(Path(temporary), [author]))

    def test_histogram_statistics_match_report_interpolation(self):
        histogram = {'3': 2, '7': 1, '12': 4, '40': 1}
        lengths = [3, 3, 7, 12, 12, 12, 12, 40]
        measured = flag_rates.histogram_statistics(histogram)
        self.assertAlmostEqual(measured['mean'], sum(lengths) / len(lengths))
        for name in flag_rates.LENGTH_STATISTICS[1:]:
            self.assertAlmostEqual(measured[name], percentile(lengths, int(name)))

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
                     ['--e', '/books/' + SENTINEL],
                     ['--exp=/books/' + SENTINEL],
                     ['--f', '--e', '/books/' + SENTINEL],
                     ['tier3', '--e', '/books/' + SENTINEL],
                     ['tier3', '--flag', '--export', '/books/' + SENTINEL],
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
