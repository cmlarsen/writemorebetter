"""Public clarity flag rates: the six bench/public texts plus the pinned Standard Ebooks chapters.

Usage: python3 bench/style/clarity.py [--review]
Prints the sweep table recorded in docs/STYLE-CLARITY.md; --review writes the default flags'
text to the ignored bench/style/out/clarity-review.jsonl for spot judgment.
"""
import json
import subprocess
import sys

from common import ROOT
from pacing import public_texts


def main():
    review = '--review' in sys.argv
    rows = [json.dumps({'name': name, 'text': text, 'hardWrapped': wrapped})
            for name, text, wrapped in public_texts()]
    command = ['cargo', 'run', '-q', '--release', '-p', 'writemorebetter-style',
               '--example', 'clarity_calibration', '--'] + (['--review'] if review else [])
    result = subprocess.run(command, input='\n'.join(rows) + '\n', capture_output=True, text=True,
                            encoding='utf-8', cwd=ROOT, check=True)
    reports = [json.loads(line) for line in result.stdout.splitlines()]
    for report in reports:
        print(json.dumps({key: value for key, value in report.items() if key != 'review'}))
    if review:
        out = ROOT / 'bench/style/out'
        out.mkdir(exist_ok=True)
        with (out / 'clarity-review.jsonl').open('w', encoding='utf-8') as stream:
            for report in reports:
                for flag in report['review']:
                    stream.write(json.dumps({'source': report['name'], 'flag': flag}) + '\n')


if __name__ == '__main__':
    main()
