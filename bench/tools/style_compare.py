#!/usr/bin/env python3
"""Compare prose-analysis foundations locally; no network calls or private-text output."""
import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import statistics
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[2]
BOOK_COMMIT = '3d900fc2224e55960c3363826ae28539b77b4204'


def span(text, surface, start=0):
    lo = text.index(surface, start)
    return [len(text[:lo].encode()), len(text[:lo + len(surface)].encode())]


def truth(case):
    if len(case['paragraphs']) != len(case['expected_sentences']):
        raise ValueError('each paragraph needs an expected sentence list')
    text = '\n'.join(case['paragraphs'])
    sentences = []
    offset = 0
    for paragraph, expected in zip(case['paragraphs'], case['expected_sentences']):
        cursor = 0
        for sentence in expected:
            lo = paragraph.index(sentence, cursor)
            sentences.append(span(text, sentence, offset + lo))
            cursor = lo + len(sentence)
        offset += len(paragraph) + 1
    quotes = []
    cursor = 0
    for quote in case['expected_quotes']:
        lo = text.index(quote, cursor)
        quotes.append(span(text, quote, lo))
        cursor = lo + len(quote)
    readings = [(span(text, item['surface']), item['lemma'], item['pos']) for item in case['expected_readings']]
    return sentences, quotes, readings


def prf(tp, predicted, expected):
    precision = tp / predicted if predicted else (1.0 if expected == 0 else 0.0)
    recall = tp / expected if expected else 1.0
    return dict(tp=tp, predicted=predicted, expected=expected, precision=precision, recall=recall,
                f1=2 * precision * recall / (precision + recall) if precision + recall else 0.0)


def score(fixtures, predictions, quotes_supported=True):
    if len(fixtures) != len(predictions):
        raise ValueError('prediction case count differs from fixture count')
    totals = {name: [0, 0, 0] for name in ['sentence_spans', 'sentence_ends', 'quote_spans', 'quote_bytes']}
    hits = count = 0
    differences = []
    for fixture, prediction in zip(fixtures, predictions):
        assert fixture['id'] == prediction['id']
        sentences, quotes, readings = truth(fixture)
        metrics = [('sentence_spans', {tuple(s) for s in prediction['sentences']}, {tuple(s) for s in sentences}),
                   ('sentence_ends', {s[1] for s in prediction['sentences']}, {s[1] for s in sentences}),
                   ('quote_spans', {tuple(s) for s in prediction.get('quotes', [])}, {tuple(s) for s in quotes}),
                   ('quote_bytes', {i for a, b in prediction.get('quotes', []) for i in range(a, b)},
                    {i for a, b in quotes for i in range(a, b)})]
        for name, actual, expected in metrics:
            if name.startswith('quote_') and not quotes_supported:
                continue
            totals[name] = [a + b for a, b in zip(totals[name], [len(actual & expected), len(actual), len(expected)])]
            if name in ['sentence_spans', 'quote_spans'] and actual != expected:
                differences.append(dict(case=fixture['id'], metric=name, predicted=sorted(actual), expected=sorted(expected)))
        for token_span, lemma, pos in readings:
            count += 1
            # Penn-derived LT noun number/countability and pronoun refinements map to Penn base tags.
            matched = any(t['span'] == token_span and any(r['lemma'] == lemma and
                r['pos'].split(':')[0].split('_')[0] == pos for r in t['readings']) for t in prediction['tokens'])
            hits += matched
            if not matched:
                differences.append(dict(case=fixture['id'], metric='expected_reading', span=token_span, lemma=lemma, pos=pos))
    result = {name: prf(*value) for name, value in totals.items()
              if quotes_supported or not name.startswith('quote_')}
    result['selected_reading_coverage'] = dict(hits=hits, expected=count, share=hits/count if count else None)
    result['differences'] = differences
    return result


def run(command, request):
    env = dict(os.environ, NLPRULE_PARALLELISM='off', OMP_NUM_THREADS='1', OPENBLAS_NUM_THREADS='1', VECLIB_MAXIMUM_THREADS='1')
    start = time.perf_counter()
    completed = subprocess.run(['/usr/bin/time', '-l', *command], input=json.dumps(request),
                               text=True, capture_output=True, env=env)
    if completed.returncode:
        raise RuntimeError(f'{command[0]} failed: {completed.stderr}')
    output = json.loads(completed.stdout)
    output['process_seconds'] = time.perf_counter() - start
    match = re.search(r'(\d+)\s+maximum resident set size', completed.stderr)
    output['peak_rss_mib'] = int(match[1]) / 1024**2 if match else None
    for case in output['cases']:
        case['first_seconds'] = case['analysis_seconds'][0]
        warm = case['analysis_seconds'][1:]
        case['warm_median_seconds'] = statistics.median(warm) if warm else None
    return output


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--output', type=Path, default=ROOT/'storage/style-comparison.json')
    ap.add_argument('--runs', type=int, default=4, help='first analysis plus warm runs')
    ap.add_argument('--skip-performance', action='store_true')
    ap.add_argument('--keep-details', action='store_true', help='retain normalized fixture predictions')
    ap.add_argument('--tools', nargs='+', choices=['wmb', 'spacy', 'booknlp-quotes'], default=['wmb','spacy','booknlp-quotes'])
    args = ap.parse_args()
    if sys.platform != 'darwin':
        ap.error('RSS measurement currently uses macOS /usr/bin/time -l')
    if args.runs < 2:
        ap.error('--runs must be at least 2')
    python = ROOT/'data/style-bench/venv/bin/python'
    book = ROOT/'data/style-bench/booknlp'
    if 'booknlp-quotes' in args.tools:
        revision = subprocess.check_output(['git','-C',str(book),'rev-parse','HEAD'], text=True).strip()
        if revision != BOOK_COMMIT:
            ap.error(f'BookNLP checkout must be {BOOK_COMMIT}')
    commands = dict(wmb=[str(ROOT/'bench/tools/wmb-style/target/release/wmb-style'),str(ROOT/'storage/en_tokenizer.bin')],
                    spacy=[str(python),'-P',str(ROOT/'bench/tools/style_worker.py'),'spacy'],
                    **{'booknlp-quotes':[str(python),'-P',str(ROOT/'bench/tools/style_worker.py'),'booknlp-quotes',str(book)]})
    fixture_path = ROOT/'bench/style/fixtures.json'
    fixtures = json.loads(fixture_path.read_text())
    output = dict(measured_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                  hardware=subprocess.check_output(['sysctl','-n','machdep.cpu.brand_string'],text=True).strip(),
                  python=sys.version.split()[0],
                  tokenizer_sha256=hashlib.sha256((ROOT/'storage/en_tokenizer.bin').read_bytes()).hexdigest(),
                  fixture_sha256=hashlib.sha256(fixture_path.read_bytes()).hexdigest(), booknlp_commit=BOOK_COMMIT,
                  tools={}, performance={})
    for tool in args.tools:
        print(f'fixtures: {tool}', flush=True)
        result = run(commands[tool], dict(cases=fixtures,runs=args.runs,details=True))
        result['accuracy'] = score(fixtures, result['cases'], quotes_supported=tool != 'spacy')
        if not args.keep_details:
            for case in result['cases']:
                for key in ['sentences','tokens','quotes']:
                    case.pop(key,None)
        output['tools'][tool] = result
    if not args.skip_performance:
        # Preserve source line endings, matching the Rust document benchmark's blank-line paragraphs.
        text = (ROOT/'bench/public/moby150k.txt').read_bytes().decode()
        separator = '\r\n\r\n' if '\r\n' in text else '\n\n'
        for repeats in [1,10]:
            paragraphs = separator.join([text]*repeats).split(separator)
            case = dict(id=f'moby-{repeats}',paragraphs=paragraphs)
            output['performance'][case['id']] = dict(bytes=len('\n'.join(paragraphs).encode()),tools={})
            for tool in args.tools:
                print(f'{case["id"]}: {tool}', flush=True)
                output['performance'][case['id']]['tools'][tool] = run(commands[tool],dict(cases=[case],runs=args.runs,details=False))
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(output,indent=2)+'\n')
    print(f'Wrote {args.output}')


if __name__ == '__main__':
    main()
