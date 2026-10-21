#!/usr/bin/env python3
"""Derive style/data/spelling/regional.txt from VarCon 2020.12.07.

Usage: scripts/build-regional-spelling.py varcon-2020.12.07.tar.gz [out.txt]

The input is the release archive from
https://downloads.sourceforge.net/project/wordlist/VarCon/2020.12.07/varcon-2020.12.07.tar.gz.
Only varcon.txt is read; voc.tab (vocabulary, sourced from web pages) is not used.
"""
import hashlib
import re
import sys
import tarfile

ARCHIVE_SHA256 = '3b0720c5718008f37c02658a83d51d5598dd86531f0a2206d410c854006cb184'
VARCON_SHA256 = '340858b990255ec409adcf7faaf2b992aa2a76a16f30b4b9e2663863191dba33'
MAXIMUM_LEVEL = 70
# a: American, b: British -ise, z: British -ize (Oxford).
SYSTEMS = 'abz'

HEADER = f"""\
# Derived from VarCon 2020.12.07 (varcon.txt, sha256 {VARCON_SHA256},
# from varcon-2020.12.07.tar.gz, sha256 {ARCHIVE_SHA256}); converted and filtered by
# writemorebetter. This is a modified version: see style/data/licenses/varcon.txt for the
# Atkinson, Titze and Ispell notices. Built by scripts/build-regional-spelling.py.
# Changes: clusters at SCOWL level {MAXIMUM_LEVEL} or below; lowercase single words without apostrophes;
# American (A), British -ise (B) and British -ize/Oxford (Z) spellings only, Canadian, Australian
# and other categories dropped; each spelling marked with every system that accepts it (preferred,
# equal or "v" variant) in any usage group of its cluster; rows whose spellings all suit the same
# systems dropped; rare or archaic usages ("(-)") ignored. One row per variant set: spelling/systems
# separated by spaces; "~" marks a spelling whose systems differ between its usages (`check` the
# verb and `check`/`cheque` the bank order).
"""

CLUSTER = re.compile(r'^# (\S+).*\(level (\d+)\)')
WORD = re.compile(r'^[a-z]+$')


def entries(line):
    """Yields (spelling, systems) for one varcon line; nothing for a rare or archaic usage."""
    line, _, usage = line.split('#', 1)[0].partition(' | ')
    if usage.startswith('(-)'):
        return
    line = line.strip()
    parsed = []
    for part in line.split(' / '):
        # A missing " / " joins two entries ("A: x's  A. B C: y"); keep the last tag set.
        tags, _, word = part.rpartition(': ')
        tags = re.split(r'\s{2,}', tags)[-1].split()
        parsed.append((word.strip(), [t for t in tags if not t.isdigit()]))
    categories = {t[0] for _, tags in parsed for t in tags}
    for word, tags in parsed:
        systems = set()
        for tag in tags:
            category, indicator = tag[0], tag[1:]
            if indicator not in ('', '.', 'v'):
                continue
            if category == 'A':
                systems.add('a')
            elif category == 'B':
                systems.add('b')
                if 'Z' not in categories:
                    systems.add('z')
            elif category == 'Z':
                systems.add('z')
        yield word, systems


def main():
    source = sys.argv[1]
    out = sys.argv[2] if len(sys.argv) > 2 else 'style/data/spelling/regional.txt'
    with open(source, 'rb') as handle:
        digest = hashlib.sha256(handle.read()).hexdigest()
    if digest != ARCHIVE_SHA256:
        sys.exit(f'unexpected archive sha256 {digest}')
    with tarfile.open(source) as archive:
        data = archive.extractfile('varcon-2020.12.07/varcon.txt').read()
    if hashlib.sha256(data).hexdigest() != VARCON_SHA256:
        sys.exit('unexpected varcon.txt sha256')
    clusters = []
    level = None
    for line in data.decode('latin-1').splitlines():
        if line.startswith('# '):
            match = CLUSTER.match(line)
            level = int(match.group(2)) if match else None
            clusters.append([])
        elif line and not line.startswith('#') and level is not None and level <= MAXIMUM_LEVEL:
            clusters[-1].append(list(entries(line)))
    rows = set()
    usages = {}
    for cluster in clusters:
        for line in cluster:
            for word, systems in line:
                if systems:
                    usages.setdefault(word, set()).add(frozenset(systems))
    for cluster in clusters:
        accepted = {}
        for line in cluster:
            for word, systems in line:
                accepted.setdefault(word, set()).update(systems)
        for line in cluster:
            row = {}
            for word, _ in line:
                if WORD.match(word) and accepted[word]:
                    row[word] = ''.join(s for s in SYSTEMS if s in accepted[word])
            if len(row) > 1 and len(set(row.values())) > 1 and any(v != SYSTEMS for v in row.values()):
                rows.add(tuple(sorted(row.items())))
    # A spelling in several clusters (`tire`, `meter`) takes every system any cluster gives it.
    accepted = {}
    for row in rows:
        for word, systems in row:
            accepted.setdefault(word, set()).update(systems)
    final = set()
    for row in rows:
        row = {word: ''.join(s for s in SYSTEMS if s in accepted[word]) for word, _ in row}
        if len(set(row.values())) > 1:
            final.add(' '.join(f'{word}/{systems}' + ('~' if len(usages[word]) > 1 else '')
                               for word, systems in sorted(row.items())))
    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(HEADER)
        for row in sorted(final):
            handle.write(row + '\n')
    print(f'{len(final)} rows, {len({w for r in final for w in r.split()})} spellings -> {out}')


if __name__ == '__main__':
    main()
