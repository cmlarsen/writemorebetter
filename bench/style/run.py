#!/usr/bin/env python3
import argparse
import json
from pathlib import Path
import sys

from common import Analyzer, ROOT


def main():
    parser = argparse.ArgumentParser(description='Public style accuracy and owner-only calibration')
    sub = parser.add_subparsers(dest='tier', required=True)
    one = sub.add_parser('tier1')
    one.add_argument('--split', choices=['dev', 'test'], default='test')
    one.add_argument('--gate', action='store_true')
    one.add_argument('--output', type=Path)
    two = sub.add_parser('tier2')
    two.add_argument('action', choices=['smoke', 'generate', 'score'])
    two.add_argument('--out', type=Path, default=ROOT / 'bench/style/out')
    two.add_argument('--gold', type=Path)
    two.add_argument('--predictions', type=Path)
    three = sub.add_parser('tier3')
    three.add_argument('--self-test', action='store_true')
    three.add_argument('--export', type=Path)
    three.add_argument('--baseline', type=Path, default=ROOT / 'bench/style/published-baseline.json')
    for command in (one, three):
        command.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
        command.add_argument('--analyzer', type=Path)
    args = parser.parse_args()
    if args.tier == 'tier2':
        import tier2
        tier2.run(args)
        return
    analyzer = Analyzer(args.tokenizer, args.analyzer, quiet=args.tier == 'tier3')
    try:
        if args.tier == 'tier1':
            import tier1
            result = tier1.run(analyzer, args.split, args.gate)
            if args.output:
                args.output.write_text(json.dumps(result, indent=2) + '\n')
        else:
            import tier3
            tier3.run(analyzer, args)
    finally:
        analyzer.close()


if __name__ == '__main__':
    try:
        main()
    except Exception:
        if len(sys.argv) > 1 and sys.argv[1] == 'tier3':
            sys.exit('calibration failed; details suppressed to protect source metadata')
        raise
