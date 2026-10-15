#!/usr/bin/env python3
"""Compute private, explicitly provisional corpus values without changing catalog review flags.

Only body paragraphs from included, contemporary, untranslated novels/novellas with authors
are eligible. Collections/anthologies need reviewed story attribution and are excluded here.
All extracted text and intermediate output stay under fiction-baseline/out/.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def prepare(baseline):
    catalog = tomllib.loads((baseline/'catalog.toml').read_text())['books']
    cases, manifest = [], []
    for file in sorted((baseline/'out/books').glob('*.json')):
        book = json.loads(file.read_text())
        entry = catalog.get(book['id'], {})
        if (book['drm'] or not entry.get('include', True) or entry.get('translated', False)
            or entry.get('form') not in ['novel','novella'] or entry.get('first_published',0)<1990
            or not entry.get('authors')):
            continue
        paragraphs = []
        excluded = {label.strip().casefold() for label in entry.get('exclude', [])}
        for section in book['sections']:
            if section['kind'] != 'Body' or section['label'].strip().casefold() in excluded:
                continue
            for item in section['files']:
                for block in item['blocks']:
                    if isinstance(block,dict) and 'Para' in block:
                        value = block['Para']
                        if not paragraphs and value.strip().casefold() == section['label'].strip().casefold():
                            continue
                        paragraphs.append(value)
                    elif block == 'Break':
                        paragraphs.append('')
        if not any(paragraphs):
            continue
        identifier = hashlib.sha256(book['id'].encode()).hexdigest()[:16]
        cases.append(dict(id=identifier, paragraphs=paragraphs))
        manifest.append(dict(id=identifier,author_key=' & '.join(sorted(entry['authors'])),
                             reviewed=entry.get('reviewed',False),book_sha256=book['sha256']))
    return cases,manifest


def aggregate(counts, manifest):
    metadata={entry['id']:entry for entry in manifest}
    authors=defaultdict(lambda: {context:dict(sentences=0,lengths=Counter(),starts=Counter()) for context in ['narration','dialogue']})
    for case in counts['cases']:
        for context, values in case['report_counts'].items():
            target=authors[metadata[case['id']]['author_key']][context]
            target['sentences']+=values['sentences']
            target['lengths'].update(values['lengths'])
            target['starts'].update(values['starts'])
    baselines={}
    for context in ['narration','dialogue']:
        # Pool occurrences within an author, normalize, then give each author equal weight.
        eligible=[a[context] for a in authors.values() if a[context]['sentences']>=200]
        baseline={}
        for source,target in [('lengths','length_baseline'),('starts','start_baseline')]:
            means=Counter()
            for author in eligible:
                for key,count in author[source].items():
                    means[key]+=count/author['sentences']/len(eligible)
            baseline[target]=dict(sorted(means.items()))
        baseline['authors']=len(eligible)
        baseline['sentences']=sum(a['sentences'] for a in eligible)
        baselines[context]=baseline
    report_options={key:{context:baselines[context][key] for context in baselines}
                    for key in ['length_baseline','start_baseline']}
    dialogue_fractions=[]
    for case in counts['cases']:
        values=case['report_counts']
        narration=values['narration']['sentences']
        dialogue=values['dialogue']['sentences']
        if narration+dialogue:
            dialogue_fractions.append(dialogue/(narration+dialogue))
    return dict(provisional=True,reason='Catalog entries are unreviewed; no metadata flags were changed.',
                books=len(manifest),reviewed_books=sum(m['reviewed'] for m in manifest),author_units=len(authors),
                method='Pooled sentence counts per author, then equal-weight mean across authors with >=200 sentences per context.',
                selection='Included, untranslated novels/novellas, original publication >=1990, known authors, body sections only; no anthologies/collections.',
                baselines=baselines,report_options=report_options,
                diagnostics=dict(books_over_90_percent_dialogue=sum(v>0.9 for v in dialogue_fractions),
                                 note='High dialogue fractions are review flags, not verified errors. Validate quotation extraction before shipping baseline values.'))


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline-root',type=Path,default=Path.home()/'calebapps/fiction-baseline')
    parser.add_argument('--prepare-only',action='store_true')
    args=parser.parse_args()
    baseline=args.baseline_root.resolve()
    output=baseline/'out/provisional-style'
    output.mkdir(parents=True,exist_ok=True)
    cases,manifest=prepare(baseline)
    if not cases: raise SystemExit('No eligible cached books')
    (output/'input.json').write_text(json.dumps(dict(cases=cases,runs=1,details=False,reports=True)))
    (output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    print(f'Prepared {len(cases)} provisional units; extracted text remains under baseline out/.',flush=True)
    if args.prepare_only: return
    start=time.perf_counter()
    with (output/'input.json').open() as source, (output/'counts.json').open('w') as dest, (output/'stderr.log').open('w') as errors:
        completed=subprocess.run([str(ROOT/'bench/tools/wmb-style/target/release/wmb-style'),str(ROOT/'storage/en_tokenizer.bin')],
                                 stdin=source,stdout=dest,stderr=errors,env=dict(os.environ,NLPRULE_PARALLELISM='off'))
    if completed.returncode: raise SystemExit('Corpus analysis failed; details are in private out/provisional-style/stderr.log')
    counts=json.loads((output/'counts.json').read_text())
    stats=aggregate(counts,manifest)
    stats['process_seconds']=time.perf_counter()-start
    (output/'baseline.json').write_text(json.dumps(stats,indent=2)+'\n')
    print(f'Analyzed {stats["books"]} books / {stats["author_units"]} author units in {stats["process_seconds"]:.1f}s. Provisional baseline saved locally.',flush=True)


if __name__=='__main__': main()
