"""Local spaCy/BookNLP quote-component adapter; invoked by style_compare.py."""
import importlib.util
import json
from pathlib import Path
import sys
import time
from types import SimpleNamespace


def main():
    import spacy
    spacy.require_cpu()
    request = json.load(sys.stdin)
    start = time.perf_counter()
    nlp = spacy.load('en_core_web_sm', exclude=['ner'])
    quote_tagger = None
    if sys.argv[1] == 'booknlp-quotes':
        source = Path(sys.argv[2]) / 'booknlp/english/litbank_quote.py'
        spec = importlib.util.spec_from_file_location('benchmark_booknlp_quote', source)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        quote_tagger = module.QuoteTagger()
    load = time.perf_counter() - start
    cases = []
    for case in request['cases']:
        timings = []
        result = None
        for run in range(request['runs']):
            # Release the previous snapshot before timing/allocating the next one.
            docs = quote_tokens = positions = None
            start = time.perf_counter()
            docs = list(nlp.pipe(case['paragraphs'], batch_size=32, n_process=1))
            quote_tokens = []
            positions = []
            offset = 0
            for paragraph_id, doc in enumerate(docs):
                for token in doc if quote_tagger else ():
                    if token.is_space:
                        continue
                    quote_tokens.append(SimpleNamespace(
                        text=token.text, paragraph_id=paragraph_id,
                        token_id=len(quote_tokens), inQuote=False))
                    positions.append((offset + token.idx, offset + token.idx + len(token.text)))
                offset += len(doc.text) + 1
            quotes = quote_tagger.tag(quote_tokens) if quote_tagger else []
            timings.append(time.perf_counter() - start)
            if run:
                continue
            text = '\n'.join(case['paragraphs'])
            # Python indices are Unicode scalars, never UTF-8 bytes.
            byte = [0]
            for ch in text:
                byte.append(byte[-1] + len(ch.encode('utf-8')))
            result = dict(id=case['id'], sentence_count=sum(len(list(doc.sents)) for doc in docs),
                          token_count=sum(len(doc) for doc in docs), quote_count=len(quotes))
            if request['details']:
                result.update(sentences=[], tokens=[], quotes=[])
                offset = 0
                for doc in docs:
                    for sentence in doc.sents:
                        surface = sentence.text
                        if surface.strip():
                            lo = offset + sentence.start_char + len(surface) - len(surface.lstrip())
                            hi = offset + sentence.start_char + len(surface.rstrip())
                            result['sentences'].append([byte[lo], byte[hi]])
                    for token in doc:
                        if not token.is_space:
                            result['tokens'].append(dict(span=[byte[offset + token.idx], byte[offset + token.idx + len(token.text)]],
                                                         readings=[dict(lemma=token.lemma_, pos=token.tag_)]))
                    offset += len(doc.text) + 1
                result['quotes'] = [[byte[positions[a][0]], byte[positions[b][1]]] for a, b in quotes]
        result['analysis_seconds'] = timings
        cases.append(result)
    print(json.dumps(dict(tool=sys.argv[1], version=spacy.__version__, model=nlp.meta['version'],
                          components=nlp.pipe_names, load_seconds=load, cases=cases)))


if __name__ == '__main__':
    main()
