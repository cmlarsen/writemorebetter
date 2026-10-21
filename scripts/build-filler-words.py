#!/usr/bin/env python3
"""Build style/data/vague/fillers.txt from words/fillers and mikpanko/expresso.

Usage: scripts/build-filler-words.py FILLERS_DATA EXPRESSO_FILLERS [out.txt]

FILLERS_DATA is data.txt from https://github.com/words/fillers at tag 2.0.1 (commit
d1924b589e6b76d7cd8c4ee3c6d4859aadae8fc7). EXPRESSO_FILLERS is corpora/fillers from
https://github.com/mikpanko/expresso at commit 2d58bbbcda716ba1e3acc4615c522ad538effe06, the last
commit that has it (`git show 2d58bbbc:corpora/fillers`). Only the words/fillers entries that
expresso also lists are kept: the other three have no recorded source (docs/STYLE-DATA-LICENSES.md).
"""
import hashlib
import sys
from pathlib import Path

FILLERS_SHA256 = 'd35af8f76a8989a0fa96ae20a784a20561838008bc90bf32033c8cc57c15d4af'
EXPRESSO_SHA256 = '011f59dc812b0d7b92a3937ce1d4f092605211ea32e422dd6ead65e6a49e4c03'


def read(path, expected):
    data = Path(path).read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected:
        sys.exit(f'{path}: unexpected sha256 {digest}')
    return data.decode('utf-8')


def main():
    fillers = read(sys.argv[1], FILLERS_SHA256)
    expresso = read(sys.argv[2], EXPRESSO_SHA256)
    out = sys.argv[3] if len(sys.argv) > 3 else 'style/data/vague/fillers.txt'
    entries = [line.strip() for line in fillers.splitlines() if line.strip() and not line.startswith('%')]
    backed = {line.strip() for line in expresso.splitlines() if line.strip()}
    kept = [entry for entry in entries if entry in backed]
    if len(kept) != len(backed):
        sys.exit(f'expresso entries missing from words/fillers: {sorted(backed - set(kept))}')
    Path(out).parent.mkdir(parents=True, exist_ok=True)
    with open(out, 'w', encoding='utf-8', newline='\n') as handle:
        handle.write(HEADER)
        for entry in kept:
            handle.write(f'{entry}\n')
    print(f'{len(kept)} of {len(entries)} entries kept')


HEADER = """\
# Filler words from words/fillers 2.0.1 (https://github.com/words/fillers, commit
# d1924b589e6b76d7cd8c4ee3c6d4859aadae8fc7, Copyright (c) 2014 Titus Wormer, MIT License), keeping
# only the entries also in mikpanko/expresso's corpora/fillers (https://github.com/mikpanko/expresso,
# commit 2d58bbbcda716ba1e3acc4615c522ad538effe06, Mikhail Panko, MIT License). Both notices are in
# style/data/licenses/fillers.txt. Order unchanged. Built by scripts/build-filler-words.py.
"""

if __name__ == '__main__':
    main()
