import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
CACHE = HERE / '.cache'


def fetch(key):
    entry = json.loads((HERE / 'sources.json').read_text())[key]
    CACHE.mkdir(exist_ok=True)
    path = CACHE / entry['filename']
    if not path.exists():
        with urllib.request.urlopen(entry['url'], timeout=120) as response:
            data = response.read()
        if hashlib.sha256(data).hexdigest() != entry['sha256']:
            raise ValueError('download checksum mismatch: ' + key)
        path.write_bytes(data)
    if hashlib.sha256(path.read_bytes()).hexdigest() != entry['sha256']:
        raise ValueError('cache checksum mismatch: ' + key)
    return path


class Analyzer:
    def __init__(self, tokenizer, executable=None, quiet=False):
        tool = ROOT / 'bench/tools/wmb-analyze'
        if executable is None:
            subprocess.run(['cargo', 'build', '--release', '--manifest-path',
                            str(tool / 'Cargo.toml')], check=True, stdout=subprocess.DEVNULL,
                           stderr=subprocess.DEVNULL if quiet else None)
            executable = tool / 'target/release/wmb-analyze'
        self.process = subprocess.Popen([str(executable), str(tokenizer)],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        text=True, encoding='utf-8',
                                        stderr=subprocess.DEVNULL if quiet else None,
                                        env={**os.environ, 'NLPRULE_PARALLELISM': 'off'})

    def analyze(self, text):
        self.process.stdin.write(json.dumps(text) + '\n')
        self.process.stdin.flush()
        result = self.process.stdout.readline()
        if not result:
            raise RuntimeError('analysis probe exited')
        return json.loads(result)

    def close(self):
        self.process.stdin.close()
        self.process.stdout.close()
        if self.process.wait() != 0:
            raise RuntimeError('analysis probe failed')


class DocumentProbe:
    def __init__(self, executable=None):
        tool = ROOT / 'bench/tools/wmb-document'
        if executable is None:
            subprocess.run(['cargo', 'build', '--release', '--manifest-path',
                            str(tool / 'Cargo.toml')], check=True, stdout=subprocess.DEVNULL)
            executable = tool / 'target/release/wmb-document'
        self.executable = executable

    def dialogue(self, documents):
        result = subprocess.run([str(self.executable), '--dialogue'],
                                input=''.join(json.dumps(row) + '\n' for row in documents),
                                capture_output=True, text=True, encoding='utf-8', check=True)
        return [json.loads(line) for line in result.stdout.splitlines() if line.strip()]


def prf(tp, predicted, gold):
    precision = tp / predicted if predicted else float(not gold)
    recall = tp / gold if gold else float(not predicted)
    return dict(precision=precision, recall=recall,
                f1=2 * tp / (predicted + gold) if predicted + gold else 1.0,
                true_positive=tp, predicted=predicted, gold=gold)


def offsets(text, start, end):
    return {'byte': {'start': len(text[:start].encode('utf-8')),
                     'end': len(text[:end].encode('utf-8'))},
            'utf16': {'start': len(text[:start].encode('utf-16-le')) // 2,
                      'end': len(text[:end].encode('utf-16-le')) // 2}}


def jsonl(path):
    with Path(path).open(encoding='utf-8') as stream:
        return [json.loads(line) for line in stream if line.strip()]


def write_jsonl(path, rows):
    with Path(path).open('w', encoding='utf-8') as stream:
        for row in rows:
            stream.write(json.dumps(row, ensure_ascii=False) + '\n')
