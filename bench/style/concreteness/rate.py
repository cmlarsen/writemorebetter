#!/usr/bin/env python3
"""Collect concreteness ratings from headless model CLIs into bench/style/concreteness/raw/.

Usage: rate.py PASS [PASS ...] [--jobs 8] [--limit N] [--supplement] [--no-main]

A PASS is RATER:MODEL:SAMPLE, for example claude:claude-opus-5-5:1 or
opencode:opencode-go/deepseek-v4-pro:1. Each pass shuffles the rated words with its own seed,
sends them 50 per request with the frozen prompt.txt, and appends validated ratings to
raw/<model>-s<sample>.jsonl and the unedited replies to raw/<model>-s<sample>.responses.jsonl.
Finished batches are skipped, so an interrupted pass resumes where it stopped.
"""
import argparse
from concurrent.futures import ThreadPoolExecutor, as_completed
import datetime
import hashlib
import json
import os
from pathlib import Path
import random
import re
import subprocess
import sys
import threading
import time

HERE = Path(__file__).resolve().parent
RAW = HERE / 'raw'
PROMPT = HERE / 'prompt.txt'
WORDS = HERE / 'words.tsv'
SUPPLEMENT = HERE / 'words-supplement.tsv'
PROMPT_SHA256 = '0317ef4beee2b11db9719e811af732839d40536bfaac54b1d20266922b990871'
WORDS_SHA256 = 'd48a0c8791ab536eea60fb320ab9358d6d3093733d01b43d36e105e787378966'
SUPPLEMENT_SHA256 = 'a0232fd2483e57ca28ad5ba48ae23df0543c77209f254b7206296948ef48de7c'
BATCH = 50
ATTEMPTS = 5
TIMEOUT = 600
LINE = re.compile(r"^\s*(\S+)\s+([1-5](?:\.\d)?)\s*$")
RATING = re.compile(r"^[1-5](?:\.\d)?$")
lock = threading.Lock()


def checked(path, expected):
    data = path.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != expected:
        sys.exit(f'{path.name}: sha256 {digest}, expected {expected}')
    return data.decode('utf-8')


def prompt_parts():
    text = checked(PROMPT, PROMPT_SHA256)
    system, user = text.split('[user]\n', 1)
    return system.split('[system]\n', 1)[1].strip(), user


def rated_words(path=WORDS, digest=WORDS_SHA256):
    rows = [line.split('\t') for line in checked(path, digest).splitlines()[1:]]
    return [word for word, source in rows if word == source]


def seed(model, sample):
    return int(hashlib.sha256(f'{model}:{sample}'.encode()).hexdigest()[:8], 16)


def slug(model):
    return model.split('/')[-1]


def batches(words, model, sample, salt=''):
    order = list(words)
    random.Random(seed(model, sample) + len(salt)).shuffle(order)
    return [order[i:i + BATCH] for i in range(0, len(order), BATCH)]


def command(rater, model, system, user):
    if rater == 'claude':
        return ['claude', '-p', '--model', model, '--tools', '', '--system-prompt', system,
                '--strict-mcp-config', '--setting-sources', '', '--no-session-persistence',
                '--output-format', 'json', user]
    if rater == 'opencode':
        return ['opencode', 'run', '-m', model, '--format', 'json', system + '\n\n' + user]
    raise ValueError(rater)


def reply_text(rater, stdout):
    if rater == 'claude':
        result = json.loads(stdout)
        if result.get('is_error'):
            raise RuntimeError(f"cli error: {str(result.get('result'))[:200]}")
        return result['result'], sorted(result.get('modelUsage', {})), result.get('usage')
    parts, models = [], set()
    for line in stdout.splitlines():
        try:
            event = json.loads(line)
        except ValueError:
            continue
        if event.get('type') == 'error':
            raise RuntimeError(f'cli error: {json.dumps(event)[:200]}')
        part = event.get('part') or {}
        if part.get('type') == 'text' and part.get('text'):
            parts.append(part['text'])
        for key in ('modelID', 'model'):
            if isinstance(part.get(key), str):
                models.add(part[key])
    return ''.join(parts), sorted(models), None


def pairs(text):
    lines = [line.strip() for line in text.strip().splitlines()
             if line.strip() and not line.strip().startswith('```')]
    matches = [LINE.match(line) for line in lines]
    if not any(matches) and len(lines) % 2 == 0 and all(map(RATING.match, lines[1::2])):
        return [(word, rating, word) for word, rating in zip(lines[::2], lines[1::2])]
    return [(*(match.groups() if match else (None, None)), line)
            for match, line in zip(matches, lines)]


def parse(text, batch):
    expected = set(batch)
    ratings = {}
    for word, rating, line in pairs(text):
        if word is None:
            raise ValueError(f'unparsable line {line[:80]!r}')
        if word not in expected:
            raise ValueError(f'unexpected word {word!r}')
        if word in ratings:
            raise ValueError(f'duplicate word {word!r}')
        ratings[word] = rating
    missing = expected - set(ratings)
    if missing:
        raise ValueError(f'{len(missing)} missing words, e.g. {sorted(missing)[:5]}')
    return ratings


def done_batches(path):
    if not path.exists():
        return set()
    return {json.loads(line)['batch'] for line in path.read_text(encoding='utf-8').splitlines()}


def append(path, rows):
    with lock, path.open('a', encoding='utf-8') as handle:
        for row in rows:
            handle.write(json.dumps(row, ensure_ascii=False) + '\n')


def run_batch(rater, model, sample, label, batch, system, template, cwd):
    name = slug(model)
    batch_id = f'{name}-s{sample}-{label}'
    user = template.replace('{count}', str(len(batch))).replace('{words}', '\n'.join(batch))
    errors = []
    for attempt in range(1, ATTEMPTS + 1):
        started = datetime.datetime.now(datetime.timezone.utc).isoformat(timespec='seconds')
        try:
            process = subprocess.run(command(rater, model, system, user), cwd=cwd,
                                     capture_output=True, text=True, timeout=TIMEOUT,
                                     stdin=subprocess.DEVNULL)
            text, served, usage = reply_text(rater, process.stdout)
            if process.returncode != 0 and not text:
                raise RuntimeError(f'exit {process.returncode}: {process.stderr[-200:]}')
        except Exception as error:  # noqa: BLE001
            errors.append(f'attempt {attempt}: {error}')
            time.sleep(min(60, 5 * attempt * attempt))
            continue
        response = {'batch': batch_id, 'model': model, 'sample': sample, 'attempt': attempt,
                    'started': started, 'served_by': served, 'words': batch, 'text': text}
        if usage:
            response['output_tokens'] = usage.get('output_tokens')
            response['thinking_tokens'] = (usage.get('output_tokens_details') or {}).get('thinking_tokens')
        try:
            ratings = parse(text, batch)
        except ValueError as error:
            response['rejected'] = str(error)
            append(RAW / f'{name}-s{sample}.responses.jsonl', [response])
            errors.append(f'attempt {attempt}: {error}')
            continue
        append(RAW / f'{name}-s{sample}.responses.jsonl', [response])
        append(RAW / f'{name}-s{sample}.jsonl',
               [{'word': word, 'model': model, 'sample': sample, 'rating': float(ratings[word]),
                 'batch': batch_id} for word in batch])
        return batch_id, None
    return batch_id, '; '.join(errors)


def main():
    parser = argparse.ArgumentParser(description=__doc__,
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('passes', nargs='+')
    parser.add_argument('--jobs', type=int, default=8)
    parser.add_argument('--limit', type=int, help='only the first N batches of each pass')
    parser.add_argument('--cwd', default=os.environ.get('TMPDIR', '/tmp'))
    parser.add_argument('--supplement', action='store_true',
                        help='also rate words-supplement.tsv, in batches labelled x000')
    parser.add_argument('--main', action=argparse.BooleanOptionalAction, default=True,
                        help='rate words.tsv (--no-main rates only the supplement)')
    args = parser.parse_args()
    system, template = prompt_parts()
    words = rated_words() if args.main else []
    supplement = rated_words(SUPPLEMENT, SUPPLEMENT_SHA256)
    RAW.mkdir(exist_ok=True)
    tasks = []
    for spec in args.passes:
        rater, rest = spec.split(':', 1)
        model, sample = rest.rsplit(':', 1)
        sample = int(sample)
        done = done_batches(RAW / f'{slug(model)}-s{sample}.jsonl')
        labelled = [(f'b{index:03d}', batch) for index, batch in
                    enumerate(batches(words, model, sample)[:args.limit])]
        if args.supplement:
            labelled += [(f'x{index:03d}', batch) for index, batch in
                         enumerate(batches(supplement, model, sample, 'x'))]
        for label, batch in labelled:
            if f'{slug(model)}-s{sample}-{label}' not in done:
                tasks.append((rater, model, sample, label, batch))
    print(f'{len(tasks)} batches to rate', flush=True)
    failures = []
    with ThreadPoolExecutor(args.jobs) as pool:
        futures = [pool.submit(run_batch, *task, system, template, args.cwd) for task in tasks]
        for count, future in enumerate(as_completed(futures), 1):
            batch_id, error = future.result()
            if error:
                failures.append((batch_id, error))
                print(f'FAILED {batch_id}: {error}', flush=True)
            if count % 20 == 0 or count == len(futures):
                print(f'{count}/{len(futures)} batches finished, {len(failures)} failed', flush=True)
    if failures:
        sys.exit(f'{len(failures)} batches failed')


if __name__ == '__main__':
    main()
