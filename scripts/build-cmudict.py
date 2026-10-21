#!/usr/bin/env python3
"""Derive the tables in style/data/pronunciation/ from CMUdict.

Usage: scripts/build-cmudict.py cmudict.dict [out-directory]

Runs from any directory: relative paths are resolved against the current directory, and the
default output directory is style/data/pronunciation/ inside the repository.

The input is cmudict.dict from https://github.com/cmusphinx/cmudict at commit
74790861f652b15e4ac49015a90074ad62a27690:
https://raw.githubusercontent.com/cmusphinx/cmudict/74790861f652b15e4ac49015a90074ad62a27690/cmudict.dict
(BSD-2-Clause-style; LICENSE copied to style/data/licenses/cmudict.txt).

Each headword's syllable count is the number of vowel phones (the phones carrying a stress
digit) in its first pronunciation. Only words the heuristic counter in
style/src/reports/readability.rs gets wrong are written; the filtering runs that counter through
`cargo run -p writemorebetter-style --example cmudict_syllables`, so rebuild the table whenever
the heuristic changes; CI regenerates it and fails on any difference.

cmudict-onsets.tsv holds each headword's first phoneme (first pronunciation) as a lowercase
ARPAbet consonant label, or `-` for a vowel, for the alliteration report (#307). Only words the
spelling heuristic `spelled_onset` in style/src/pronunciation.rs gets wrong are written, filtered
through `cargo run -p writemorebetter-style --example cmudict_onsets`, so rebuild it whenever that
heuristic changes. Whole pronunciations are never written.
"""
import hashlib
import os
import re
import subprocess
import sys
import tempfile

COMMIT = '74790861f652b15e4ac49015a90074ad62a27690'
SHA256 = '81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22'
WORD = re.compile(r"[a-z]+(?:['-][a-z]+)*'?|(?:[a-z]\.){2,}")

SYLLABLES_HEADER = f"""\
# Derived from the CMU Pronouncing Dictionary (cmudict.dict), https://github.com/cmusphinx/cmudict
# at commit {COMMIT} (sha256 {SHA256}).
# Copyright (C) 1993-2015 Carnegie Mellon University. All rights reserved. BSD-2-Clause-style
# license, reproduced in style/data/licenses/cmudict.txt. Changes: syllable counts only (vowel phones
# in each headword's first pronunciation, at least 1), no phonemes; keys lowercased, initialisms
# without their final period (u.s.); trailing-period abbreviations dropped; only the {{kept}} of
# {{total}} headwords whose count the heuristic in style/src/reports/readability.rs gets wrong.
# Built by scripts/build-cmudict.py.
"""

ONSETS_HEADER = f"""\
# Derived from the CMU Pronouncing Dictionary (cmudict.dict), https://github.com/cmusphinx/cmudict
# at commit {COMMIT} (sha256 {SHA256}).
# Copyright (C) 1993-2015 Carnegie Mellon University. All rights reserved. BSD-2-Clause-style
# license, reproduced in style/data/licenses/cmudict.txt. Changes: each headword's first phoneme
# only (first pronunciation; lowercase ARPAbet consonant, `-` for a vowel); keys as in
# cmudict-syllables.tsv; only the {{kept}} of {{total}} headwords whose first phoneme the
# spelling heuristic in style/src/pronunciation.rs gets wrong. Built by scripts/build-cmudict.py.
"""


def first_pronunciations(path):
    """Yields (headword, phones) for each headword's first pronunciation, in file order."""
    with open(path, encoding='utf-8') as handle:
        for line in handle:
            line = line.split('#', 1)[0].split()
            if line and '(' not in line[0]:
                yield line[0], line[1:]


def key(word):
    """The lookup key: what the counter sees for this word in text, or None to skip it."""
    if not WORD.fullmatch(word):
        return None
    return word.rstrip('.') if '.' in word else word.strip("'")


def syllable_counts(path):
    counts = {}
    for word, phones in first_pronunciations(path):
        k = key(word)
        count = max(1, sum(phone[-1].isdigit() for phone in phones))
        # `dogs'` and `dogs` share a key; the unmarked headword wins.
        if k and (k not in counts or k == word):
            counts[k] = count
    return counts


def onsets(path):
    sounds = {}
    for word, phones in first_pronunciations(path):
        k = key(word)
        first = phones[0]
        sound = '-' if first[-1].isdigit() else first.lower()
        if k and (k not in sounds or k == word):
            sounds[k] = sound
    return sounds


def filtered(table, example, header, out):
    """Writes the rows of `table` that the heuristic run by `example` gets wrong."""
    with tempfile.TemporaryDirectory() as directory:
        full = os.path.join(directory, 'full.tsv')
        with open(full, 'w', encoding='utf-8', newline='\n') as handle:
            for word in sorted(table):
                handle.write(f'{word}\t{table[word]}\n')
        kept = subprocess.run(
            ['cargo', 'run', '--quiet', '--release', '-p', 'writemorebetter-style',
             '--example', example, '--', full],
            check=True, stdout=subprocess.PIPE, text=True).stdout
    lines = kept.splitlines()
    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(header.format(kept=f'{len(lines):,}', total=f'{len(table):,}'))
        for line in lines:
            handle.write(line + '\n')
    print(f'{os.path.basename(out)}: {len(lines)} of {len(table)} words')


def main():
    source = os.path.abspath(sys.argv[1])
    out = os.path.abspath(sys.argv[2]) if len(sys.argv) > 2 else None
    # cargo finds the workspace from the current directory.
    os.chdir(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    out = out or 'style/data/pronunciation'
    with open(source, 'rb') as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest != SHA256:
        sys.exit(f'unexpected sha256 {digest}')
    filtered(syllable_counts(source), 'cmudict_syllables', SYLLABLES_HEADER,
             os.path.join(out, 'cmudict-syllables.tsv'))
    filtered(onsets(source), 'cmudict_onsets', ONSETS_HEADER,
             os.path.join(out, 'cmudict-onsets.tsv'))


if __name__ == '__main__':
    main()
