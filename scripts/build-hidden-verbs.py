#!/usr/bin/env python3
"""Derive style/data/hidden-verbs/nominalizations.tsv from Princeton WordNet 3.0.

Usage: scripts/build-hidden-verbs.py WordNet-3.0.tar.gz [out.tsv]

The input is the release tarball from https://wordnetcode.princeton.edu/3.0/WordNet-3.0.tar.gz,
read in place (nothing is extracted). Nouns the emotion lexicon lists
(style/data/emotions/emotions.tsv, built by scripts/build-emotion-lexicon.py) are left to the
emotion tells lens. The derivation is recorded in docs/STYLE-HIDDEN-VERBS.md.
"""
import hashlib
import sys
import tarfile
from collections import defaultdict
from pathlib import Path

SHA256 = '640db279c949a88f61f851dd54ebbb22d003f8b90b85267042ef85a3781d3a52'
EMOTIONS = Path(__file__).resolve().parent.parent / 'style/data/emotions/emotions.tsv'
SUFFIXES = ('tion', 'sion', 'ment', 'ance', 'ence', 'al', 'ure')
# Lexicographer files of events and abstractions: noun.act, noun.cognition, noun.communication,
# noun.event, noun.process, noun.state. Concrete things, groups, people and feelings are left out.
ABSTRACT = {4, 9, 10, 11, 22, 26}
# SemCor tag counts in index.sense: the verb must be common enough to be the plainer choice, and the
# noun must have been seen at least once.
VERB_MINIMUM = 5
NOUN_MINIMUM = 1

# Nouns the selection admits whose ordinary reading in fiction is not the action of their verb:
# lexicalized senses (information, government, reservation), things and places (shipment,
# prescription, signature), fixed phrases (in addition, in accordance with, pay attention), states
# a reader takes as feelings (arousal, fascination), and links whose verb is another word
# (destination → destine, incursion → incur). Action and assistance are left to LanguageTool's
# PLAIN_ENGLISH rules (TAKE_ACTION_TO, ASSIST_ASSISTANCE), so a noun is never reported twice.
EXCLUDED = set("""
accordance action addition administration appearance armament arousal assistance association
attention attraction centrifugation collection commandment commutation compensation competition
confidence constitution conviction damnation decoration destination difference disposition division
dominance enclosure engagement entertainment equation establishment exhaustion exhibition expiration
exposure expression fascination formation government ignorance imagination impression inclination
incursion induction information inheritance inspiration installation installment insurance
intersection location notation nourishment occupation operation organization possession precedence
preferment prescription presentment pretension procedure profession proposition provision puzzlement
qualification quotation regulation relation remembrance rendition requisition reservation
resignation resolution seizure sensation shipment signature sorption stance suction temptation
transportation
""".split())


def read_member(archive, name):
    return archive.extractfile(f'WordNet-3.0/dict/{name}').read().decode('latin-1')


def parse_data(text):
    synsets = {}
    for line in text.splitlines():
        if line.startswith('  '):
            continue
        fields = line.partition(' | ')[0].split()
        count = int(fields[3], 16)
        words = [fields[4 + 2 * i].lower().split('(')[0] for i in range(count)]
        at = 4 + 2 * count
        pointers = []
        for _ in range(int(fields[at])):
            pointers.append(tuple(fields[at + 1:at + 5]))
            at += 4
        synsets[fields[0]] = {'lex': int(fields[1]), 'words': words, 'pointers': pointers}
    return synsets


def parse_index(text):
    senses = {}
    for line in text.splitlines():
        if line.startswith('  '):
            continue
        fields = line.split()
        senses[fields[0]] = fields[-int(fields[2]):]
    return senses


def tag_counts(text):
    counts = defaultdict(int)
    for line in text.splitlines():
        key, _, _, tags = line.split()
        lemma, _, rest = key.partition('%')
        counts[(lemma, 'nv'[int(rest[0]) - 1] if rest[0] in '12' else rest[0])] += int(tags)
    return counts


def derived_from(noun, verb):
    """The noun is the verb's stem plus a suffix: decide → decision, explain → explanation."""
    if len(verb) >= len(noun):
        return False
    shared = 0
    while shared < len(verb) and noun[shared] == verb[shared]:
        shared += 1
    return shared >= max(3, len(verb) - 2)


def main():
    source = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else 'style/data/hidden-verbs/nominalizations.tsv'
    with open(source, 'rb') as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest != SHA256:
        sys.exit(f'unexpected sha256 {digest}')
    with tarfile.open(source, 'r:gz') as archive:
        nouns = parse_data(read_member(archive, 'data.noun'))
        verbs = parse_data(read_member(archive, 'data.verb'))
        noun_index = parse_index(read_member(archive, 'index.noun'))
        counts = tag_counts(read_member(archive, 'index.sense'))
    emotions = {line.split('\t')[0] for line in EMOTIONS.read_text(encoding='utf-8').splitlines()
                if not line.startswith('#')}

    # Noun → verb derivational pointers ("+") between single words, from a synset in an abstract
    # lexicographer file, for nouns one of whose two most frequent senses is abstract.
    rows = {}
    selected = set()
    for synset in nouns.values():
        if synset['lex'] not in ABSTRACT:
            continue
        for symbol, target, pos, words in synset['pointers']:
            if symbol != '+' or pos != 'v':
                continue
            noun = synset['words'][int(words[:2], 16) - 1]
            verb = verbs[target]['words'][int(words[2:], 16) - 1]
            if (not noun.isalpha() or not verb.isalpha() or not noun.endswith(SUFFIXES)
                    or not derived_from(noun, verb) or noun in emotions):
                continue
            if not any(nouns[offset]['lex'] in ABSTRACT for offset in noun_index[noun][:2]):
                continue
            if counts[(noun, 'n')] < NOUN_MINIMUM or counts[(verb, 'v')] < VERB_MINIMUM:
                continue
            selected.add(noun)
            if noun in EXCLUDED:
                continue
            # One verb per noun: the most frequent, then the first alphabetically.
            best = rows.get(noun)
            if best is None or (-counts[(verb, 'v')], verb) < (-counts[(best, 'v')], best):
                rows[noun] = verb
    unused = sorted(EXCLUDED - selected)
    if unused:
        sys.exit(f'excluded words that WordNet does not select: {unused}')

    Path(out).parent.mkdir(parents=True, exist_ok=True)
    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(HEADER)
        for noun in sorted(rows):
            handle.write(f'{noun}\t{rows[noun]}\n')
    print(f'{len(rows)} nominalizations')


HEADER = f"""\
# Derived from Princeton WordNet 3.0 (WordNet-3.0.tar.gz, sha256 {SHA256}).
# WordNet 3.0 Copyright 2006 by Princeton University. All rights reserved. Used under the WordNet
# License; the full notice and disclaimer are in style/data/licenses/WordNet-LICENSE. This is a
# modified extract: noun<TAB>verb pairs from WordNet's noun-to-verb derivational pointers, for
# single-word nouns ending -tion, -sion, -ment, -ance, -ence, -al or -ure that are the verb's stem
# plus that suffix, from a synset in noun.act, noun.cognition, noun.communication, noun.event,
# noun.process or noun.state, with one of their two most frequent senses there, a SemCor tag count
# of at least {NOUN_MINIMUM} (noun) and {VERB_MINIMUM} (verb), one verb per noun, minus a reviewed exclusion
# list and the nouns of style/data/emotions/emotions.tsv. Built by scripts/build-hidden-verbs.py.
"""

if __name__ == '__main__':
    main()
