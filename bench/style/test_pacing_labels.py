import copy
import json
from pathlib import Path
import tempfile
import unittest

import pacing_labels as pl

PARAGRAPHS = ['“Run,” she said—once.', 'He’d gone… away and back 😀.', 'The mill had been old.',
              'It had always been old.', 'She ran.', 'Done.']
XHTML = '''<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><body>
<section><hgroup><h2>I</h2><p>The Title</p></hgroup>
<p>First   line
\t\tcontinues.<a href="#n1" epub:type="noteref">1</a></p>
<blockquote><p>A letter.</p></blockquote>
<hr/>
<p>After the <i>break</i>.</p>
<footer><p>Signed</p></footer></section></body></html>'''


def synthetic():
    text = '\n'.join(PARAGRAPHS)
    paragraphs, start = [], 0
    for paragraph in PARAGRAPHS:
        paragraphs.append({'start': start, 'end': start + len(paragraph), 'quoted': False, 'scene': False})
        start += len(paragraph) + 1
    return pl.Document('synthetic', 'doc.txt', text, paragraphs)


def synthetic_items(doc, sizes=(3, 3)):
    items, index = [], 0
    for number, size in enumerate(sizes, 1):
        for position in range(1, size + 1):
            items.append({'id': f'p{number:02d}.{position}', 'passage': f'p{number:02d}', 'paragraph': position,
                          'source': doc.source, 'document': doc.name, **doc.span(index),
                          'sha256': pl.digest(doc.paragraph(index))})
            index += 1
    return items


def labels(name, items, slow, kinds=None, severity=2):
    rows = []
    for passage, group in pl.passages(items).items():
        flags = [slow.get(item['id'], False) for item in group]
        rows.append({'labeler': name, 'passage': passage,
                     'paragraphs': [{'paragraph': item['paragraph'], 'slow': flag,
                                     'kind': (kinds or {}).get(item['id'], 'action')}
                                    for item, flag in zip(group, flags)],
                     'stretches': [{'start': s, 'end': e, 'severity': severity} for s, e in pl.stretches_of(flags)]})
    return rows


class PacingLabelTests(unittest.TestCase):
    def setUp(self):
        self.doc = synthetic()
        pl.LOADED['synthetic'] = {'doc.txt': self.doc}
        self.items = synthetic_items(self.doc)

    def tearDown(self):
        pl.LOADED.pop('synthetic', None)

    def test_xhtml_document(self):
        doc = pl.xhtml_document('book', 'chapter-1.xhtml', XHTML.encode('utf-8'))
        self.assertEqual(doc.text, 'First line continues.\nA letter.\n\n\nAfter the break.')
        self.assertEqual([p['quoted'] for p in doc.paragraphs], [False, True, False])
        self.assertEqual([p['scene'] for p in doc.paragraphs], [False, False, True])
        self.assertEqual(doc.paragraph(2), 'After the break.')

    def test_unicode_offsets(self):
        raw, units = self.doc.text.encode('utf-8'), self.doc.text.encode('utf-16-le')
        for index, paragraph in enumerate(PARAGRAPHS):
            span = self.doc.span(index)
            self.assertEqual(raw[span['byte']['start']:span['byte']['end']].decode('utf-8'), paragraph)
            self.assertEqual(units[2 * span['utf16']['start']:2 * span['utf16']['end']].decode('utf-16-le'), paragraph)
            self.assertEqual(self.doc.text[span['char']['start']:span['char']['end']], paragraph)

    def test_public_document_keeps_line_separator(self):
        for source, path in pl.PUBLIC.items():
            text = (pl.ROOT / path).read_text(encoding='utf-8')
            doc = pl.public_document(source)
            lines = [line for line in text.split('\n') if line.strip()]
            self.assertEqual([doc.paragraph(i) for i in range(len(doc.paragraphs))], lines)
        self.assertTrue(any(' ' in pl.public_document(s).text for s in pl.PUBLIC))

    def test_materialize_rejects_tampering(self):
        self.assertEqual(pl.materialize(self.items)['p01.2'], PARAGRAPHS[1])
        bad = copy.deepcopy(self.items)
        bad[1]['sha256'] = '0' * 64
        with self.assertRaises(ValueError):
            pl.materialize(bad)
        bad = copy.deepcopy(self.items)
        bad[1]['byte']['end'] -= 1
        with self.assertRaises(ValueError):
            pl.materialize(bad)

    def test_packet(self):
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory) / 'packet'
            written = pl.packet(out, self.items)
            self.assertEqual(sorted(p.name for p in written),
                             ['PACING-GUIDELINES.md', 'labels-template.jsonl', 'p01.md', 'p02.md'])
            passage = (out / 'p01.md').read_text(encoding='utf-8')
            self.assertIn('[2] ' + PARAGRAPHS[1], passage)
            self.assertNotIn('[4]', passage)
            template = pl.jsonl(out / 'labels-template.jsonl')
            self.assertEqual([row['passage'] for row in template], ['p01', 'p02'])
            self.assertTrue(pl.validate(template, self.items))
            for path in out.iterdir():
                content = path.read_text(encoding='utf-8')
                for leak in ('stratum', 'near_miss', 'triggeredSignals', 'pastPerfect', 'slowRuns', 'synthetic'):
                    self.assertNotIn(leak, content)
            with self.assertRaises(ValueError):
                pl.packet(out, self.items)
            pl.packet(out, self.items, force=True)

    def test_validate(self):
        good = labels('a', self.items, {'p01.2': True, 'p01.3': True})
        self.assertEqual(pl.validate(good, self.items), [])
        corruptions = {
            'missing passages': lambda rows: rows.pop(),
            'duplicate passage': lambda rows: rows.append(copy.deepcopy(rows[0])),
            'kind must be': lambda rows: rows[0]['paragraphs'][0].update(kind='scenery'),
            'slow must be': lambda rows: rows[0]['paragraphs'][0].update(slow='no'),
            'stretches cover': lambda rows: rows[0]['stretches'].clear(),
            'must not overlap': lambda rows: rows[0]['stretches'].append({'start': 3, 'end': 3, 'severity': 1}),
            'integer start': lambda rows: rows[0]['stretches'][0].update(severity=True),
            'severity must be': lambda rows: rows[0]['stretches'][0].update(severity=6),
            'outside paragraphs': lambda rows: rows[1]['stretches'].append({'start': 3, 'end': 4, 'severity': 1}),
            'paragraph number': lambda rows: rows[1]['paragraphs'][0].update(paragraph=2),
            'all 3 paragraphs': lambda rows: rows[1]['paragraphs'].pop(),
            'unknown keys': lambda rows: rows[1].update(stratum='flagged'),
            'one labeler per file': lambda rows: rows[1].update(labeler='b'),
            'unknown passage': lambda rows: rows[1].update(passage='p99'),
        }
        for message, corrupt in corruptions.items():
            rows = copy.deepcopy(good)
            corrupt(rows)
            errors = pl.validate(rows, self.items)
            self.assertTrue(any(message in error for error in errors), (message, errors))

    def test_fleiss_reference(self):
        table = [[0, 0, 0, 0, 14], [0, 2, 6, 4, 2], [0, 0, 3, 5, 6], [0, 3, 9, 2, 0], [2, 2, 8, 1, 1],
                 [7, 7, 0, 0, 0], [3, 2, 6, 3, 0], [2, 5, 3, 2, 2], [6, 5, 2, 1, 0], [0, 2, 2, 3, 7]]
        counts = [{k: v for k, v in enumerate(row)} for row in table]
        self.assertAlmostEqual(pl.fleiss(counts), 0.20993, places=4)
        self.assertEqual(pl.fleiss([{'yes': 3}, {'no': 3}]), 1.0)
        self.assertIsNone(pl.fleiss([{'no': 3}, {'no': 3}]))
        with self.assertRaises(ValueError):
            pl.fleiss([{'yes': 3}, {'no': 2}])

    def test_cohen_reference(self):
        first = ['y'] * 25 + ['n'] * 25
        second = ['y'] * 20 + ['n'] * 5 + ['y'] * 10 + ['n'] * 15
        self.assertAlmostEqual(pl.cohen(first, second), 0.4)
        self.assertIsNone(pl.cohen(['y', 'y'], ['y', 'y']))

    def test_merge(self):
        kinds = {'p01.2': 'backstory', 'p01.3': 'backstory'}
        a = labels('a', self.items, {'p01.2': True, 'p01.3': True}, kinds, severity=2)
        b = labels('b', self.items, {'p01.2': True, 'p01.3': True, 'p02.1': True}, kinds, severity=4)
        c = labels('c', self.items, {'p01.3': True}, {'p01.3': 'interior'}, severity=3)
        loaded = [(rows[0]['labeler'], {row['passage']: row for row in rows}) for rows in (a, b, c)]
        output, stats = pl.merge(loaded, self.items, 'sha')
        paragraphs = {row['id']: row for row in output if row['type'] == 'paragraph'}
        self.assertEqual(output[0], {'type': 'meta', 'labelers': ['a', 'b', 'c'], 'itemsSha256': 'sha'})
        self.assertIs(paragraphs['p01.2']['slow'], True)
        self.assertEqual(paragraphs['p01.2']['slowVotes'], {'yes': 2, 'no': 1})
        self.assertEqual(paragraphs['p01.2']['slowLabelers'], {'yes': ['a', 'b'], 'no': ['c']})
        self.assertIs(paragraphs['p02.1']['slow'], False)
        self.assertEqual(paragraphs['p01.3']['kind'], 'backstory')
        self.assertEqual(paragraphs['p01.3']['kindLabelers'], {'backstory': ['a', 'b'], 'interior': ['c']})
        passage = next(row for row in output if row['type'] == 'passage' and row['passage'] == 'p01')
        self.assertEqual(passage['stretches'], [{'start': 2, 'end': 3, 'severity': 3,
                                                  'severityVotes': {'a': 2, 'b': 4, 'c': 3}}])
        self.assertAlmostEqual(passage['stretchAgreement'], (1 + 0.5 + 0.5) / 3)
        self.assertEqual(stats['consensusSlow'], 2)
        self.assertEqual(stats['paragraphs'], 6)
        self.assertIsNotNone(stats['fleissKappa']['slow'])
        self.assertEqual(stats['cohenKappa']['a|b']['kind'], 1.0)
        tied, _ = pl.merge(loaded[:2], self.items)
        self.assertIsNone(next(row for row in tied if row.get('id') == 'p02.1')['slow'])
        with self.assertRaises(ValueError):
            pl.merge([loaded[0], loaded[0]], self.items)

    def test_score(self):
        a = labels('a', self.items, {'p01.2': True, 'p01.3': True, 'p02.3': True})
        b = labels('b', self.items, {'p01.2': True, 'p01.3': True, 'p02.3': True})
        loaded = [(rows[0]['labeler'], {row['passage']: row for row in rows}) for rows in (a, b)]
        consensus, _ = pl.merge(loaded, self.items)
        indexes = {item['id']: number for number, item in enumerate(self.items)}
        result = pl.score(consensus, self.items, {('synthetic', 'doc.txt'): [{1, 2}, {4}]}, indexes)
        self.assertEqual(result['paragraph']['true_positive'], 2)
        self.assertEqual(result['paragraph']['predicted'], 3)
        self.assertEqual(result['paragraph']['gold'], 3)
        self.assertEqual(result['runAnyOverlap']['matchedPredicted'], 1)
        self.assertEqual(result['runAnyOverlap']['predicted'], 2)
        self.assertEqual(result['runAnyOverlap']['matchedGold'], 1)
        self.assertEqual(result['runAnyOverlap']['gold'], 2)
        self.assertEqual(result['runIou50']['matchedPredicted'], 1)
        self.assertEqual(pl.overlap_scores([(1, 3)], [(3, 5)], 0.0)['matchedGold'], 1)
        self.assertEqual(pl.overlap_scores([(1, 3)], [(3, 5)], 0.5)['matchedGold'], 0)
        self.assertEqual(pl.overlap_scores([], [], 0.0)['f1'], 1.0)
        with self.assertRaises(ValueError):
            pl.score(consensus[:-2], self.items, {}, indexes)


class CommittedItemTests(unittest.TestCase):
    def test_items_and_strata(self):
        items = pl.load_items()
        strata = json.loads(pl.STRATA.read_text())
        groups = pl.passages(items)
        self.assertEqual(list(groups), [f'p{n:02d}' for n in range(1, len(groups) + 1)])
        self.assertTrue(35 <= len(groups) <= 45)
        self.assertTrue(280 <= len(items) <= 320)
        self.assertEqual(len({item['id'] for item in items}), len(items))
        self.assertEqual(set(groups), set(strata['passages']))
        allowed = {'id', 'passage', 'paragraph', 'source', 'document', 'char', 'byte', 'utf16', 'sha256'}
        for passage, group in groups.items():
            self.assertTrue(6 <= len(group) <= 10)
            self.assertEqual([item['paragraph'] for item in group], list(range(1, len(group) + 1)))
            self.assertEqual(len({(item['source'], item['document']) for item in group}), 1)
            self.assertEqual([item['char']['start'] for item in group], sorted(item['char']['start'] for item in group))
            for item in group:
                self.assertEqual(set(item), allowed)
                self.assertEqual(item['id'], f'{passage}.{item["paragraph"]}')
        sources = {meta['source'] for meta in strata['passages'].values()}
        self.assertTrue(set(pl.PUBLIC) <= sources)
        self.assertGreaterEqual(len(sources - set(pl.PUBLIC)), 6)
        counts = [sum(m['stratum'] == s for m in strata['passages'].values()) for s in pl.STRATA_TARGET]
        self.assertEqual(counts, list(pl.STRATA_TARGET.values()))
        self.assertTrue(all(m['defaultRuns'] or m['looseRuns'] for m in strata['passages'].values()
                            if m['stratum'] == 'flagged'))
        catalog = json.loads((pl.HERE / 'sources.json').read_text())
        self.assertTrue(all(item['source'] in pl.PUBLIC or item['source'] in catalog for item in items))

    def test_story_items_materialize_offline(self):
        items = [item for item in pl.load_items() if item['source'] in pl.PUBLIC]
        self.assertTrue(items)
        texts = pl.materialize(items)
        self.assertEqual(len(texts), len(items))


if __name__ == '__main__':
    unittest.main()
