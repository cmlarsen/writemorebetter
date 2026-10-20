#!/usr/bin/env python3
"""Build style/data/norms/concreteness.tsv from the committed LLM ratings.

Usage:
  scripts/build-concreteness-norms.py words
  scripts/build-concreteness-norms.py build [--glasgow PATH] [--lancaster PATH] [--wordnet PATH]

`words` writes bench/style/concreteness/words.tsv from the fiction lemma list.
`build` reads bench/style/concreteness/raw/*.jsonl, the adjudication file and the Glasgow
calibration split, writes the table, and writes the QA review list.
"""
import hashlib
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / 'bench/style/concreteness'
LEMMAS = ROOT / 'style/data/baselines/overused-words.json'
VARIANTS = ROOT / 'style/src/reports/overused_words.rs'
WORDS = DATA / 'words.tsv'


def word_list():
    lemmas = json.loads(LEMMAS.read_text(encoding='utf-8'))['baseline']['lemmas']
    rated = sorted(word for word in lemmas
                   if len(word) > 1 and not any(c.isdigit() for c in word))
    body = VARIANTS.read_text(encoding='utf-8').split('pub const SPELLING_VARIANTS', 1)[1]
    pairs = re.findall(r'\(\s*"([^"]+)",\s*"([^"]+)"\s*\)', body.split('];', 1)[0])
    keep = set(rated)
    rows = [(word, word) for word in rated]
    rows += [(british, american) for british, american in pairs if american in keep]
    return sorted(rows)


def write_words():
    rows = word_list()
    text = 'word\trating_from\n' + ''.join(f'{word}\t{source}\n' for word, source in rows)
    WORDS.write_bytes(text.encode('utf-8'))
    rated = sum(word == source for word, source in rows)
    digest = hashlib.sha256(text.encode('utf-8')).hexdigest()
    print(f'{len(rows)} entries, {rated} rated, {len(rows) - rated} spelling variants, sha256 {digest}')


def main():
    command = sys.argv[1] if len(sys.argv) > 1 else ''
    if command == 'words':
        write_words()
    else:
        sys.exit(__doc__)


if __name__ == '__main__':
    main()
