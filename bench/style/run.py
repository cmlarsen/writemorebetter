#!/usr/bin/env python3
import argparse
import contextlib
import io
import json
from pathlib import Path
import sys

from common import Analyzer, DocumentProbe, SentenceProbe, ROOT


def private(argv):
    return 'tier3' in argv[1:]


def main():
    parser = argparse.ArgumentParser(description='Public style accuracy and owner-only calibration')
    sub = parser.add_subparsers(dest='tier', required=True)
    one = sub.add_parser('tier1')
    one.add_argument('--split', choices=['dev', 'test'], default='test')
    one.add_argument('--gate', action='store_true')
    one.add_argument('--output', type=Path)
    one.add_argument('--document-probe', type=Path)
    two = sub.add_parser('tier2')
    two.add_argument('action', choices=['smoke', 'generate', 'score', 'sentences'])
    two.add_argument('--out', type=Path, default=ROOT / 'bench/style/out')
    two.add_argument('--gold', type=Path)
    two.add_argument('--predictions', type=Path)
    two.add_argument('--manifest', type=Path)
    two.add_argument('--report')
    two.add_argument('--pattern')
    two.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    two.add_argument('--document-probe', type=Path)
    two.add_argument('--gate', action='store_true')
    two.add_argument('--output', type=Path)
    cliches = sub.add_parser('cliches')
    cliches.add_argument('--gate', action='store_true')
    cliches.add_argument('--probe', type=Path)
    cliches.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    cliches.add_argument('--out', type=Path, default=ROOT / 'bench/style/out/cliches')
    three = sub.add_parser('tier3')
    three.add_argument('--self-test', action='store_true')
    three.add_argument('--sentence-length', action='store_true')
    three.add_argument('--document-probe', type=Path)
    three.add_argument('--export', type=Path)
    three.add_argument('--baseline', type=Path, default=ROOT / 'bench/style/published-baseline.json')
    tags = sub.add_parser('dialogue-tags')
    tags.add_argument('suite', choices=['tier2', 'tier3'])
    tags.add_argument('--self-test', action='store_true')
    tags.add_argument('--export', type=Path)
    tags.add_argument('--probe', type=Path)
    tags.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
    tags.add_argument('--out', type=Path, default=ROOT / 'bench/style/out')
    for command in (one, three):
        command.add_argument('--tokenizer', type=Path, default=ROOT / 'storage/en_tokenizer.bin')
        command.add_argument('--analyzer', type=Path)
    if private(sys.argv):
        with contextlib.redirect_stderr(io.StringIO()):
            args = parser.parse_args()
    else:
        args = parser.parse_args()
    if args.tier == 'dialogue-tags':
        import dialogue_tags
        dialogue_tags.main(args)
        return
    if args.tier == 'tier2':
        if args.action == 'sentences':
            import sentences
            sentences.run(args)
            return
        import tier2
        tier2.run(args)
        return
    if args.tier == 'cliches':
        import cliches
        cliches.run(args)
        return
    if args.tier == 'tier3' and args.sentence_length:
        import tier3
        probe = SentenceProbe(args.tokenizer, args.document_probe, quiet=True)
        try:
            result = tier3.sentence_length_self_test(probe) if args.self_test else tier3.calibrate_sentence_length(probe, args.export)
            if not tier3.numeric_only(result):
                raise ValueError('non-numeric aggregate')
            print(json.dumps(result, indent=2, allow_nan=False))
        finally:
            probe.close()
        return
    analyzer = Analyzer(args.tokenizer, args.analyzer, quiet=args.tier == 'tier3')
    try:
        if args.tier == 'tier1':
            import tier1
            result = tier1.run(analyzer, args.split, args.gate,
                               DocumentProbe(args.document_probe))
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
    except BaseException as error:
        if isinstance(error, SystemExit) and error.code == 0:
            raise
        if private(sys.argv):
            sys.exit('calibration failed; details suppressed to protect source metadata')
        raise
