#!/usr/bin/env python3
"""Summarize a dhat-heap.json written by `wmb-profile load|heap` (build with --features dhat-heap).

  dhat_summary.py MODE FILE [-n 25]

MODE is `end` (bytes live at program end, for `load` runs that leak the loaded structures),
`peak` (bytes live at the global heap peak, t-gmax) or `total` (allocation volume over the run).
`delta` takes two files, `load` and `heap`, and prints peak-minus-loaded bytes per structure.
Each program point is assigned to the first structure whose regex matches any frame of its stack;
the remainder is reported as `other`.
"""
import argparse
import collections
import json
import re

STRUCTURES = [
    ('sentencizer (srx regexes)', r'srx::'),
    ('multiword tagger (Aho-Corasick)', r'aho_corasick|MultiwordTagger'),
    ('chunker', r'tokenizer::chunk::|MaxentChunker|MaxentPosTagger'),
    ('tagger (FSTs, word store)', r'tokenizer::tag::|Tagger'),
    ('tokenizer disambiguation rules', r'DisambiguationRule'),
    ('speller', r'spell::|Spell'),
    ('builtin rules', r'builtin::'),
    ('regex pattern pool', r'intern_pattern|PATTERN_POOL|utils::regex::'),
    ('grammar rules (matcher parts, examples, suggesters)',
     r'composition::|rule::|Rules>::new|PartPool|Synthesizer'),
    ('check: sentence tokens and tags', r'IncompleteToken|WordData|Sentence|tokenize'),
]

FIELD = {'end': ('eb', 'ebk'), 'peak': ('gb', 'gbk'), 'total': ('tb', 'tbk'), 'delta': ('gb', 'gbk')}


def classify(data, key, patterns):
    frames = data['ftbl']
    by_structure = collections.Counter()
    for point in data['pps']:
        size = point[key]
        if size == 0:
            continue
        stack = [frames[i] for i in point['fs']]
        label = next((label for label, pattern in patterns
                      if any(pattern.search(frame) for frame in stack)), 'other')
        by_structure[label] += size
    return by_structure


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=FIELD)
    parser.add_argument('file')
    parser.add_argument('other', nargs='?', help='heap file for `delta` (FILE is the load file)')
    parser.add_argument('-n', type=int, default=25)
    parser.add_argument('--by-blocks', action='store_true', help='rank sites by block count')
    args = parser.parse_args()
    patterns = [(label, re.compile(regex)) for label, regex in STRUCTURES]
    if args.mode == 'delta':
        loaded = classify(json.load(open(args.file)), 'eb', patterns)
        peak = classify(json.load(open(args.other)), 'gb', patterns)
        print('| structure | loaded MiB | at peak MiB | transient MiB |')
        print('|---|---:|---:|---:|')
        for label in sorted(peak, key=lambda k: peak[k] - loaded.get(k, 0), reverse=True):
            print(f'| {label} | {loaded.get(label, 0) / 1048576:.2f} | {peak[label] / 1048576:.2f} | '
                  f'{(peak[label] - loaded.get(label, 0)) / 1048576:+.2f} |')
        print(f'| total | {sum(loaded.values()) / 1048576:.2f} | {sum(peak.values()) / 1048576:.2f} | '
              f'{(sum(peak.values()) - sum(loaded.values())) / 1048576:+.2f} |')
        return
    data = json.load(open(args.file))
    frames = data['ftbl']
    bytes_key, blocks_key = FIELD[args.mode]
    by_structure = collections.Counter()
    blocks_by_structure = collections.Counter()
    by_site = collections.Counter()
    total = 0
    for point in data['pps']:
        size, blocks = point[bytes_key], point[blocks_key]
        if size == 0:
            continue
        total += size
        stack = [frames[i] for i in point['fs']]
        label = next((label for label, pattern in patterns
                      if any(pattern.search(frame) for frame in stack)), 'other')
        by_structure[label] += size
        blocks_by_structure[label] += blocks
        site = next((frame for frame in stack if 'nlprule' in frame or 'srx' in frame
                     or 'aho_corasick' in frame or 'fst' in frame), stack[0])
        by_site[re.sub(r'^0x[0-9a-f]+: ', '', site)] += blocks if args.by_blocks else size
    print(f'{args.mode}: {total} bytes ({total / 1048576:.2f} MiB)')
    print('\n| structure | MiB | share | blocks |')
    print('|---|---:|---:|---:|')
    for label, size in by_structure.most_common():
        print(f'| {label} | {size / 1048576:.2f} | {100 * size / total:.1f}% | {blocks_by_structure[label]} |')
    print('\nlargest sites (first nlprule/srx/aho-corasick/fst frame of the stack):')
    for site, size in by_site.most_common(args.n):
        if args.by_blocks:
            print(f'{size:>11} blocks  {site[:150]}')
        else:
            print(f'{size / 1048576:9.2f} MiB  {site[:150]}')


if __name__ == '__main__':
    main()
