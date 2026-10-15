from collections import Counter

TAGS = ('ADJ', 'ADP', 'ADV', 'CONJ', 'DET', 'NOUN', 'NUM', 'PRON', 'PRT', 'VERB', '.', 'X')
QUOTATION_MARKS = frozenset("\"'“”‘’«»‹›„‟‚‛`")
EXCLUDED_XPOS = frozenset('ADD GW AFX XX FW LS'.split())
PUNCTUATION_CLASSES = {
    '.': '.', '!': '.', '?': '.', ',': ',', ':': ':', ';': ':', '…': ':',
    '(': '-LRB-', ')': '-RRB-', '[': '-LRB-', ']': '-RRB-',
    '{': '-LRB-', '}': '-RRB-', '“': '``', '‘': '``', '”': "''", '’': "''",
    '"': "''", "'": "''", '-': 'HYPH', '—': 'HYPH', '/': 'NFP', '$': '$', '#': '#',
}
LEMMA_CANONICAL = {'could': 'can', 'would': 'will', 'these': 'this'}
PENN = {}
for universal, tags in {
    'ADJ': 'JJ JJR JJS', 'ADP': 'IN', 'ADV': 'RB RBR RBS WRB', 'CONJ': 'CC',
    'DET': 'DT EX PDT WDT', 'NOUN': 'NN NNS NNP NNPS', 'NUM': 'CD',
    'PRON': 'PRP PRP$ WP WP$', 'PRT': 'POS RP TO', 'VERB': 'VB VBD VBG VBN VBP VBZ MD',
    '.': "# $ '' ( ) , . : `` -LRB- -RRB- HYPH NFP", 'X': 'FW LS SYM UH'
}.items():
    for tag in tags.split():
        PENN[tag] = universal



def normalize_pos(pos, surface=''):
    if pos.startswith('NN:'):
        return 'NN'
    if pos.startswith('PRP$_'):
        return 'PRP$'
    if pos.startswith('PRP_'):
        return 'PRP'
    if pos == 'RB_SENT':
        return 'RB'
    if pos == 'PCT':
        return PUNCTUATION_CLASSES.get(surface, '.')
    return pos


def normalize_lemma(lemma):
    folded = lemma.casefold()
    return LEMMA_CANONICAL.get(folded, folded)


def lexical_conventions(gold, tags):
    pos = gold['pos']
    pairs = []
    if gold['upos'] == 'SCONJ' and pos == 'IN' and tags & {'CC', 'DT', 'WDT', 'WP'}:
        pairs.append('subordinator')
    for name, pair in (('WDT_WP', {'WDT', 'WP'}), ('NNPS_NNS', {'NNPS', 'NNS'}),
                       ('RBR_JJR', {'RBR', 'JJR'})):
        if pos in pair and tags & (pair - {pos}):
            pairs.append(name)
    return pairs


def skip_quotes(tokens):
    tokens = [token for token in tokens if not token['is_whitespace']]
    while tokens and tokens[0]['text'] and all(ch in QUOTATION_MARKS for ch in tokens[0]['text']):
        tokens = tokens[1:]
    return tokens


def start_candidates(tokens):
    tokens = skip_quotes(tokens)
    if not tokens:
        return set()
    first = tokens[0]
    tags = {normalize_pos(reading['pos'], first['text']) for reading in first['readings']}
    return {PENN[tag] for tag in tags if tag in PENN}


def add_start(resolved, fractional, candidates):
    if len(candidates) == 1:
        resolved.update(candidates)
    for tag in candidates:
        fractional[tag] += 1 / len(candidates)


def shares(counts):
    total = sum(counts.values())
    return {tag: 100 * counts[tag] / total if total else None for tag in TAGS}
