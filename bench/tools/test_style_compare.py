import unittest
from style_compare import prf, score, span, truth
from style_corpus import aggregate

class ScoringTests(unittest.TestCase):
    def test_unicode_offsets_and_repeated_substrings(self):
        case = dict(id='test', paragraphs=['😀 “She left.”','She left.'],
                    expected_sentences=[['😀 “She left.”'],['She left.']],
                    expected_quotes=['“She left.”'],expected_readings=[])
        sentences,quotes,_ = truth(case)
        text='\n'.join(case['paragraphs'])
        self.assertEqual(text.encode()[sentences[1][0]:sentences[1][1]].decode(),'She left.')
        self.assertEqual(text.encode()[quotes[0][0]:quotes[0][1]].decode(),'“She left.”')
        self.assertEqual(span('😀 go','go'),[5,7])

    def test_false_positives_and_unsupported_quote_task(self):
        case=dict(id='test',paragraphs=['Hi.'],expected_sentences=[['Hi.']],expected_quotes=[],expected_readings=[])
        prediction=dict(id='test',sentences=[[0,3]],quotes=[[0,3]],tokens=[])
        result=score([case],[prediction])
        self.assertEqual(result['quote_spans']['f1'],0)
        self.assertEqual(result['sentence_spans']['f1'],1)
        self.assertNotIn('quote_spans',score([case],[prediction],quotes_supported=False))
        self.assertEqual(prf(0,0,0)['f1'],1)
        with self.assertRaises(ValueError): score([case],[])

    def test_ambiguous_reading_coverage_and_byte_union(self):
        case=dict(id='test',paragraphs=['walk'],expected_sentences=[['walk']],expected_quotes=['walk'],
                  expected_readings=[dict(surface='walk',lemma='walk',pos='NN')])
        prediction=dict(id='test',sentences=[[0,4]],quotes=[[0,4],[1,3]],
                        tokens=[dict(span=[0,4],readings=[dict(lemma='walk',pos='VB'),dict(lemma='walk',pos='NN:UN')])])
        result=score([case],[prediction])
        self.assertEqual(result['selected_reading_coverage']['hits'],1)
        self.assertEqual(result['quote_bytes']['predicted'],4)
        self.assertLess(result['quote_spans']['precision'],1)

    def test_corpus_author_weighting_and_consumable_options(self):
        def counts(n, subject):
            return dict(sentences=n,lengths={'2':n},starts={'subject':subject,'adverb':n-subject})
        cases=[]
        manifest=[]
        for identifier,author,n,subject in [('a1','A',200,200),('a2','A',400,0),('b1','B',200,200)]:
            cases.append(dict(id=identifier,report_counts=dict(narration=counts(n,subject),dialogue=counts(0,0))))
            manifest.append(dict(id=identifier,author_key=author,reviewed=False))
        result=aggregate(dict(cases=cases),manifest)
        self.assertAlmostEqual(result['report_options']['start_baseline']['narration']['subject'],2/3)
        self.assertEqual(result['baselines']['narration']['authors'],2)
        self.assertEqual(result['report_options']['start_baseline']['dialogue'],{})
        self.assertTrue(result['provisional'])

if __name__=='__main__': unittest.main()
