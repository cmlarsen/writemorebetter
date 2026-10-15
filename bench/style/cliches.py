import json
from pathlib import Path
import re
import subprocess

from common import HERE, ROOT, offsets, prf, write_jsonl
import tier2

BASELINE = HERE / 'tier2-cliches-baseline.json'
MARK = re.compile(r'\[([^\]]*)\]')

TEMPLATES = [
    ('He had [kicked the bucket] at last, they said.', 'cliche', 'narration'),
    ('She [aired her dirty laundry] before the whole parish.', 'cliche', 'narration'),
    ('It was [a far cry] from the house she remembered.', 'cliche', 'narration'),
    ('That was, she knew, the [calm before the storm].', 'cliche', 'narration'),
    ('He [beat around the bush] for an hour.', 'cliche', 'narration'),
    ('Her refusal only [added insult to\ninjury].', 'cliche', 'narration'),
    ('The captain had [bitten the\ndust] by noon.', 'cliche', 'narration'),
    ('They arrived [in the nick of time].', 'cliche', 'narration'),
    ('It was [all in a day’s work] for the mate.', 'cliche', 'narration'),
    ('The view was a [bird’s-eye view] of the harbour.', 'cliche', 'narration'),
    ('He had been [caught red handed], of course.', 'cliche', 'narration'),
    ('Her uncle was [under the weather]—nothing worse.', 'cliche', 'narration'),
    ('This was only the [tip of the iceberg]…', 'cliche', 'narration'),
    ('By then she was [at her wits’ end].', 'cliche', 'narration'),
    ('Ahab [let the cat out of the bag] at supper.', 'cliche', 'narration'),
    ('He [cut to the chase] without ceremony.', 'cliche', 'narration'),
    ('“[Better safe than sorry],” said her mother.', 'cliche', 'dialogue'),
    ('“[Don’t count your chickens], my dear.”', 'cliche', 'dialogue'),
    ('"We shall [burn the midnight oil]," he said.', 'cliche', 'dialogue'),
    ('‘It is [easy as pie],’ she said.', 'cliche', 'dialogue'),
    ('“I am [at my wits end]!” cried Mrs. Bennet.', 'cliche', 'dialogue'),
    ('He checked the [GPS system] twice.', 'redundancy', 'narration'),
    ('She saved it in [PDF format].', 'redundancy', 'narration'),
    ('He had failed the [SAT test].', 'redundancy', 'narration'),
    ('The [UPC codes] were smudged.', 'redundancy', 'narration'),
    ('“Check the [RAM memory],” he said.', 'redundancy', 'dialogue'),
    ('The [NATO treaty organization] met at noon.', 'redundancy', 'narration'),
    ('She [aired the dirty laundry] at supper.', 'cliche', 'narration'),
    ('He was [at his wit’s end] by Friday.', 'cliche', 'narration'),
    ('“I have [put my foot down],” said Mr. Bennet.', 'cliche', 'dialogue'),
    ('The pin number was painted on the skittle.', None, None),
    ('He kicked the old bucket across the yard.', None, None),
    ('That was the insult. To injury he added nothing.', None, None),
    ('She walked around the bush in the garden.', None, None),
    ('The gps system was a joke among the crew.', None, None),
    ('They waited a good half hour.', None, None),
    ('She woke from a bad nightmare.', None, None),
    ('Every fiber of her was screaming.', None, None),
    ('They were shooting in the dark at shadows.', None, None),
    ('He was worse to the bone.', None, None),
    ('He kept his ear to the grinding stone.', None, None),
    ('It was easier said than did.', None, None),
    ('She let her hair down from its pins.', None, None),
    ('She put her foot down on the step.', None, None),
    ('The dog licked its wounds in the straw.', None, None),
    ('The cat landed on its feet.', None, None),
    ('It was all talk… No action came of it.', None, None),
    ('She was three years younger than her sister.', None, None),
    ('Sailors spoke of it in their dreams.', None, None),
    ('He sat on the rocking chair.', None, None),
]


def parse(template):
    text, spans, shift = '', [], 0
    for match in MARK.finditer(template):
        text += template[shift:match.start()]
        start = len(text)
        text += match.group(1)
        spans.append((start, len(text)))
        shift = match.end()
    return text + template[shift:], spans


def inject(clean, sentence):
    insertion = sentence + ' '
    return insertion + clean, 0, len(insertion)


def utf16_len(text):
    return len(text.encode('utf-16-le')) // 2


def shift_rows(rows, clean, at, insertion):
    byte_at, utf16_at = len(clean[:at].encode('utf-8')), utf16_len(clean[:at])
    byte_add, utf16_add = len(insertion.encode('utf-8')), utf16_len(insertion)
    result = []
    for row in rows:
        moved = row['byte']['start'] >= byte_at
        result.append({**row,
                       'byte': {k: v + byte_add * moved for k, v in row['byte'].items()},
                       'utf16': {k: v + utf16_add * moved for k, v in row['utf16'].items()}})
    return result


def key(row):
    return (row['file'], row['byte']['start'], row['byte']['end'],
            row['utf16']['start'], row['utf16']['end'], row['pattern'])


def label(kind, context):
    return f'{kind}.{context}'


def generate(passages):
    documents, gold, pairs = [], [], []
    for number, (book, index, clean) in enumerate(passages):
        template, kind, context = TEMPLATES[number % len(TEMPLATES)]
        sentence, spans = parse(template)
        text, at, length = inject(clean, sentence)
        insertion = text[at:at + length]
        offset = at
        file = f'{book}-{index:03}.txt'
        for start, end in spans:
            gold.append({'file': file, 'report': 'cliches', 'pattern': label(kind, context),
                         **offsets(text, offset + start, offset + end)})
        documents.append({'file': file, 'source': book, 'injected': kind is not None})
        pairs.append((file, clean, text, at, insertion))
    return documents, gold, pairs


def predict(probe, tokenizer, rows):
    result = subprocess.run([str(probe), '--predict', str(tokenizer), '--hard-wrapped'],
                            input=''.join(json.dumps(row, ensure_ascii=False) + '\n' for row in rows),
                            capture_output=True, text=True, encoding='utf-8', check=True)
    predictions = [json.loads(line) for line in result.stdout.splitlines() if line.strip()]
    for row in predictions:
        row['pattern'] = label(row['pattern'], row['context'])
    return predictions


def evaluate(pairs, documents, gold, clean_predictions, injected_predictions):
    natural = set()
    by_file = {}
    for row in clean_predictions:
        by_file.setdefault(row['file'], []).append(row)
    for file, clean, _text, at, insertion in pairs:
        for row in shift_rows(by_file.get(file, []), clean, at, insertion):
            natural.add(key(row))
    new = [row for row in injected_predictions if key(row) not in natural]
    scored = tier2.score(gold, new, {row['file'] for row in documents}, report='cliches')
    return scored, len(clean_predictions), len(injected_predictions) - len(new)


def build_probe(probe):
    if probe:
        return probe
    tool = ROOT / 'bench/tools/wmb-cliches'
    subprocess.run(['cargo', 'build', '--release', '--manifest-path', str(tool / 'Cargo.toml')],
                   check=True, stdout=subprocess.DEVNULL)
    return tool / 'target/release/wmb-cliches'


def micro(scored):
    tp = sum(p['true_positive'] for p in scored['patterns'].values())
    predicted = sum(p['predicted'] for p in scored['patterns'].values())
    expected = sum(p['gold'] for p in scored['patterns'].values())
    return prf(tp, predicted, expected)


def run(args):
    probe = build_probe(args.probe)
    passages = list(tier2.public_passages())
    documents, gold, pairs = generate(passages)
    args.out.mkdir(parents=True, exist_ok=True)
    write_jsonl(args.out / 'documents.jsonl', documents)
    write_jsonl(args.out / 'answers.jsonl', gold)
    clean = predict(probe, args.tokenizer, [{'file': f, 'text': c} for f, c, *_ in pairs])
    injected = predict(probe, args.tokenizer, [{'file': f, 'text': t} for f, _c, t, *_ in pairs])
    write_jsonl(args.out / 'predictions.jsonl', injected)
    scored, natural_hits, natural_kept = evaluate(pairs, documents, gold, clean, injected)
    total = micro(scored)
    result = {'documents': len(documents),
              'injected': sum(row['injected'] for row in documents),
              'negative_controls': sum(not row['injected'] for row in documents),
              'gold': len(gold), 'natural_hits_in_clean_text': natural_hits,
              'natural_hits_carried_into_injected_text': natural_kept,
              'micro': total, **scored}
    print(json.dumps(result, indent=2))
    if args.gate:
        floors = json.loads(BASELINE.read_text())
        for metric in ('precision', 'recall', 'f1'):
            if total[metric] + 1e-12 < floors[metric]:
                raise ValueError(f'cliches tier 2 {metric} regressed below {floors[metric]}')
