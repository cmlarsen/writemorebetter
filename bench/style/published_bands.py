"""Build published-author p5/p95 bands from an owner-run `tier3 --flag-rates` aggregate JSON."""
import argparse
import json
from pathlib import Path

from flag_rates import GROUPS, MODALITIES, PRONOUN_KEYS, TRANSITION_TYPES, WINDOW_RULE, numeric_only


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


def build(result, source):
    if result['window_rule'] != WINDOW_RULE or not numeric_only(result):
        raise ValueError('unexpected flag-rate aggregate')
    chapter = result['chapter']['reports']
    defaults = result['defaults']
    return {
        'source': {**source, 'scope': 'chapter windows (scene-break split, 2,000-6,000 words), author-weighted '
                                      '(each author counts equally)',
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
    args = parser.parse_args()
    result = json.loads(args.aggregate.read_text(encoding='utf-8'))
    bands = build(result, {'dataset': args.dataset, 'tool': args.tool, 'run_date': args.run_date})
    args.output.write_text(json.dumps(bands, indent=1) + '\n', encoding='utf-8')


if __name__ == '__main__':
    main()
