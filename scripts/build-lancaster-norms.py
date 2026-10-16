#!/usr/bin/env python3
"""Derive style/data/norms/lancaster-perceptual.tsv from the Lancaster Sensorimotor Norms CSV.

Usage: scripts/build-lancaster-norms.py Lancaster_sensorimotor_norms_for_39707_words.csv [out.tsv]

The input is the aggregate CSV from https://osf.io/48wsc/ (OSF project 7emr6, CC BY 4.0).
"""
import csv
import hashlib
import sys

SHA256 = '445d363fb1f9f3e50b86d88e2f46cdc9a22b5dd8a713ce4e7be2a773d57f43c5'
COLUMNS = [('Visual', 'v'), ('Auditory', 'a'), ('Haptic', 'h'), ('Olfactory', 'o'),
           ('Gustatory', 'g'), ('Interoceptive', 'i')]
MINIMUM_STRENGTH = 4.0
MINIMUM_MARGIN = 1.0

HEADER = f"""\
# Derived from the Lancaster Sensorimotor Norms (Lynott, Connell, Brysbaert, Brand & Carney, 2020),
# https://osf.io/7emr6/, file Lancaster_sensorimotor_norms_for_39707_words.csv (sha256 {SHA256}),
# licensed CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/). Changes: single words only,
# lowercased; only words whose strongest perceptual modality rates at least {MINIMUM_STRENGTH} and leads the
# next modality by at least {MINIMUM_MARGIN}; columns reduced to the dominant modality (v visual, a auditory,
# h haptic, o olfactory, g gustatory, i interoceptive), its mean strength and its lead over the
# second modality, both in tenths of a rating point (0-50). Built by scripts/build-lancaster-norms.py.
"""


def main():
    source, out = sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else 'style/data/norms/lancaster-perceptual.tsv'
    with open(source, 'rb') as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest != SHA256:
        sys.exit(f'unexpected sha256 {digest}')
    rows = {}
    with open(source, encoding='utf-8', newline='') as handle:
        for row in csv.DictReader(handle):
            word = row['Word'].strip().lower()
            if not word or not all(c.isalpha() or c in "-'" for c in word):
                continue
            ranked = sorted(((float(row[f'{name}.mean']), letter) for name, letter in COLUMNS),
                            key=lambda item: -item[0])
            (top, letter), (second, _) = ranked[0], ranked[1]
            if top >= MINIMUM_STRENGTH and top - second >= MINIMUM_MARGIN:
                rows[word] = (letter, round(top * 10), round((top - second) * 10))
    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(HEADER)
        for word in sorted(rows):
            letter, top, margin = rows[word]
            handle.write(f'{word}\t{letter}\t{top}\t{margin}\n')
    print(f'{len(rows)} words')


if __name__ == '__main__':
    main()
