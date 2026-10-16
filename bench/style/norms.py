#!/usr/bin/env python3
"""Tier 2 synthetic sensory regressions; never reads a fiction corpus."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile

CASES = {
    'participles': (
        'The floorboards were creaking. The clock kept ticking. Someone was knocking. '
        'The roses had withered. The lamp dimmed.',
        {'creaking', 'ticking', 'knocking', 'withered', 'dimmed'}, set()),
    'predicates': (
        'He felt broken glass. He felt warm breath. She felt smooth stone. He felt sharp pain. '
        'He felt ashamed. He felt obliged to go. He felt cold and tired.',
        {'feel'}, {'felt'}),
    'compounds': (
        '“Her lamp’s dimmed…”—\u2028hard-heartedness, cold-blooded, eagle-eyed, dubious-looking '
        'look-outs; red-hot, ice-cold, sweet-smelling, dance-music, amber-tinted, half-muffled, '
        'blue-clothed, red-eyed, hot-tempered, loud-mouthed.',
        {'hot', 'cold', 'smelling', 'music', 'amber', 'muffled', 'blue', 'red'},
        {'hard', 'eyed', 'looking', 'look', 'loud'}),
    'figurative': (
        'It looks just as if it will rain. It looked almost like a ship. '
        'I felt very nice and snug. He felt unduly concerned.',
        set(), {'look', 'feel'}),
    'tags': (
        '“I’m stiff,” she complained. “Listen,” she whispered. “Run!” he shouted. '
        '“Hush,” she murmured. The crowd shouted. A dove murmured.',
        {'shout', 'murmur'}, {'complain', 'whisper'}),
}
SURFACES = {
    'compounds': ['dimmed', 'red-hot', 'ice-cold', 'sweet-smelling', 'dance-music', 'amber-tinted',
                  'half-muffled', 'blue-clothed', 'red-eyed'],
    'tags': ['Listen', 'shouted', 'murmured'],
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--probe', default='target/release/examples/norm_reports')
    args = parser.parse_args()
    with tempfile.TemporaryDirectory(prefix='wmb-norms-') as directory:
        root = Path(directory)
        paths = []
        for name, (text, _, _) in CASES.items():
            path = root / (name + '.txt')
            path.write_text(text, encoding='utf-8')
            paths.append(str(path))
        env = dict(os.environ, WMB_NORM_JSON=str(root), WMB_SENSE_WORDS='1')
        env.pop('WMB_CONCRETENESS', None)
        env.pop('WMB_SENSORY_OPTIONS', None)
        subprocess.run([args.probe, *paths], env=env, check=True, capture_output=True, text=True)
        for name, (text, required, excluded) in CASES.items():
            words = json.loads((root / (name + '.json')).read_text())['sensory']['senseWords']
            lemmas = {word['lemma'] for word in words}
            assert required <= lemmas, (name, 'missing', required - lemmas)
            assert not excluded & lemmas, (name, 'unexpected', excluded & lemmas)
            if name == 'predicates':
                assert sum(word['lemma'] == 'feel' for word in words) == 4
            surfaces = []
            for word in words:
                span = word['span']
                byte = span['byte']
                utf16 = span['utf16']
                surface = text.encode()[byte['start']:byte['end']].decode()
                assert surface == text.encode('utf-16-le')[2 * utf16['start']:2 * utf16['end']].decode('utf-16-le')
                surfaces.append(surface)
            if name in SURFACES:
                assert surfaces == SURFACES[name], (name, surfaces)
    print(f'norms tier 2: {len(CASES)} synthetic cases passed')


if __name__ == '__main__':
    main()
