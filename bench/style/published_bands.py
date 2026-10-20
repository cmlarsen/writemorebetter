"""Build published-author p5/p95 bands from an owner-run `tier3 --flag-rates` aggregate JSON."""
import argparse
import json
from pathlib import Path

from flag_rates import (ABSTRACT_BELOW, DICTION_MINIMUM_SENTENCES, GROUPS, MODALITIES, PRONOUN_KEYS,
                        TRANSITION_TYPES, WINDOW_RULE, numeric_only)

SCOPE = ('chapter windows (scene-break split, target 2,000-6,000 words: closed after reaching 6,000 or at a scene '
         'break past 3,000; short tails merged), author-weighted (each author counts equally)')


def band(stats):
    if not stats:
        return None
    return {'p5': stats['percentiles']['5'], 'p95': stats['percentiles']['95']}


def calibration(stats):
    if not stats:
        return None
    return {key: stats[key] for key in ('samples', 'authors', 'mean', 'percentiles')}


def measures(reports, convert):
    transitions = reports['transitions']['measures']
    pronouns = reports['pronoun_starts']['measures']
    sensory = reports['sensory_balance']['measures']
    return {
        'transitions': {group: {
            'containing': convert(transitions[group]['containing']),
            'opening': convert(transitions[group]['opening']),
            'types': {kind: {name: convert(transitions[group]['types'][kind][name])
                             for name in ('containing', 'opening')} for kind in TRANSITION_TYPES}}
            for group in GROUPS},
        'pronounStarts': {group: {
            'pronounStarts': convert(pronouns[group]['pronoun_starts']),
            'pronouns': {pronoun: convert(pronouns[group]['pronouns'][pronoun]) for pronoun in PRONOUN_KEYS}}
            for group in GROUPS},
        'sensoryBalance': {group: {
            'senseWordsPerThousandWords': convert(sensory[group]['sense_word_density']),
            'modalities': {modality: convert(sensory[group]['modality_percent'][modality])
                           for modality in MODALITIES}}
            for group in GROUPS},
    }


def eligible(reports):
    return {'transitions': {group: reports['transitions']['measures'][group]['eligible_share'] for group in GROUPS},
            'pronounStarts': {group: reports['pronoun_starts']['measures'][group]['eligible_share']
                              for group in GROUPS},
            'sensoryBalance': {group: reports['sensory_balance']['measures'][group]['eligible_share']
                               for group in GROUPS}}


def diction_measures(reports, convert):
    measures = reports['diction']['measures']
    return {group: {'meanRating': convert(measures[group]['mean_rating']),
                    'sentenceMean': convert(measures[group]['sentence_mean'])} for group in GROUPS}


def flag_rate(stats):
    return {'flaggedShare': stats['flagged_share'], 'perThousandWords': calibration(stats['per_1000_words'])}


def diction_flags(reports):
    sweep = reports['diction']['abstract_below']
    return {below: {'abstractSentences': flag_rate(sweep[below]['abstract_sentences']),
                    'abstractRuns': flag_rate(sweep[below]['abstract_runs']),
                    'abstractSharePercent': calibration(sweep[below]['abstract_share'])}
            for below in ABSTRACT_BELOW}


def check_aggregate(result):
    if result['window_rule'] != WINDOW_RULE or not numeric_only(result):
        raise ValueError('unexpected flag-rate aggregate')


def build_diction(result, source):
    check_aggregate(result)
    chapter = result['chapter']['reports']
    defaults = result['defaults']
    return {
        'source': {**source, 'scope': SCOPE, 'window_rule': WINDOW_RULE, 'samples': result['chapter']['samples'],
                   'units': result['units'], 'authors': result['authors'],
                   'table': 'bundled style/data/norms/concreteness.tsv (ConcretenessTable::bundled())',
                   'method': 'p5/p95 are weighted percentiles (linear interpolation between cumulative-weight '
                             'midpoints). meanRating: per-window mean over the mode\'s rated words, over windows '
                             'with at least the floor of rated sentences in that mode, each window weighted '
                             '1 / (its author\'s eligible windows). sentenceMean: every rated sentence of the mode '
                             '(mean rounded to 0.01), each sentence weighted 1 / (its author\'s rated sentences)',
                   'floors': {'minimumRatedSentences': DICTION_MINIMUM_SENTENCES,
                              'minimumRatedWords': defaults['diction_minimum_rated_words']},
                   'defaults': {'abstractBelow': defaults['diction_abstract_below'],
                                'minimumRatedWords': defaults['diction_minimum_rated_words'],
                                'minimumRunSentences': defaults['diction_minimum_run_sentences']}},
        'units': {'meanRating': 'concreteness, 1 (abstract) to 5 (concrete), mean over rated words',
                  'sentenceMean': 'concreteness, 1 (abstract) to 5 (concrete), one sentence\'s mean',
                  'flagRates': 'flaggedShare: share of windows (chapter) or authors (manuscript) with at least one '
                               'flag; perThousandWords: flags per 1,000 words; abstractSharePercent: abstract '
                               'sentences as a percent of rated narration sentences without speech'},
        'bandStatus': 'Reference only: diction takes no baseline argument, so nothing highlights against these '
                      'bands.',
        'bands': diction_measures(chapter, band),
        'eligibleShare': {group: chapter['diction']['measures'][group]['eligible_share'] for group in GROUPS},
        'calibration': {'chapter': diction_measures(chapter, calibration),
                        'manuscript': diction_measures(result['manuscript']['reports'], calibration)},
        'flagRates': {'chapter': diction_flags(chapter),
                      'manuscript': diction_flags(result['manuscript']['reports'])},
    }


def build(result, source):
    check_aggregate(result)
    chapter = result['chapter']['reports']
    defaults = result['defaults']
    return {
        'source': {**source, 'scope': SCOPE,
                   'window_rule': WINDOW_RULE, 'samples': result['chapter']['samples'],
                   'units': result['units'], 'authors': result['authors'],
                   'method': 'p5/p95 are weighted percentiles (linear interpolation between cumulative-weight '
                             'midpoints) of per-window percentages over windows meeting the report floor in '
                             'that mode, each window weighted 1 / (its author\'s eligible windows)',
                   'floors': {'transitionsMinimumSentences': defaults['transitions_minimum_sentences'],
                              'pronounStartsMinimumSentences': defaults['pronoun_starts_minimum_sentences'],
                              'sensoryMinimumSenseWords': defaults['sensory_minimum_sense_words']}},
        'units': {'transitions': 'percent of eligible sentences containing or opening with a transition',
                  'pronounStarts': 'percent of eligible sentences opening with the pronoun',
                  'sensoryBalance': 'modalities: percent of sense words; senseWordsPerThousandWords: rate'},
        'bandStatus': 'Reference only: transitions, pronoun starts and sensory balance take no baseline '
                      'argument yet, so nothing highlights against these bands.',
        'bands': measures(chapter, band),
        'eligibleShare': eligible(chapter),
        'calibration': {'chapter': measures(chapter, calibration),
                        'manuscript': measures(result['manuscript']['reports'], calibration)},
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('aggregate', type=Path)
    parser.add_argument('--dataset', required=True)
    parser.add_argument('--tool', required=True)
    parser.add_argument('--run-date', required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--report', choices=('transitions-pronouns-sensory', 'diction'),
                        default='transitions-pronouns-sensory')
    args = parser.parse_args()
    result = json.loads(args.aggregate.read_text(encoding='utf-8'))
    builder = build_diction if args.report == 'diction' else build
    bands = builder(result, {'dataset': args.dataset, 'tool': args.tool, 'run_date': args.run_date})
    args.output.write_text(json.dumps(bands, indent=1) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
