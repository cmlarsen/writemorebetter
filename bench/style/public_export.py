"""Write public text in the tier 3 export layout so `tier3 --flag-rates` can build public bands (#303).

Units: story1-5 (one invented author), the unchanged `moby150k` (unwrapped, chapters as scene breaks)
and the pinned Standard Ebooks Pride and Prejudice (all chapters, chapters as scene breaks).
"""
import argparse
import json
from pathlib import Path
import re

from common import ROOT
from transitions_pronouns import pride_chapters


def moby():
    text = (ROOT / 'bench/public/moby150k.txt').read_text(encoding='utf-8').replace('\r\n', '\n')
    lines = []
    for block in re.split(r'\n\s*\n', text):
        paragraph = ' '.join(line.strip() for line in block.splitlines() if line.strip())
        if paragraph.startswith('CHAPTER ') and lines:
            lines.append('')
        if paragraph:
            lines.append(paragraph)
    return '\n'.join(lines) + '\n'


def pride():
    lines = []
    for block in pride_chapters(count=None).split('\n\n'):
        lines.append('' if block == '***' else block.replace('\n', ' '))
    return '\n'.join(lines) + '\n'


def units():
    for number in range(1, 6):
        text = (ROOT / f'bench/public/story{number}.txt').read_text(encoding='utf-8')
        yield f'story{number}', 'public-stories', text
    yield 'moby150k', 'melville', moby()
    yield 'pride', 'austen', pride()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    (args.output / 'units').mkdir(parents=True, exist_ok=True)
    rows = []
    for unit_id, author, text in units():
        (args.output / 'units' / (unit_id + '.txt')).write_text(text, encoding='utf-8')
        rows.append({'unit_id': unit_id, 'author_key': author})
    (args.output / 'units.jsonl').write_text(''.join(json.dumps(row) + '\n' for row in rows), encoding='utf-8')


if __name__ == '__main__':
    main()
