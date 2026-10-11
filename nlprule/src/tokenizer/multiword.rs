//! Checks if the input text contains multi-token phrases from a finite list (might contain e. g. city names) and assigns lemmas and part-of-speech tags accordingly.

use crate::types::*;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct MultiwordTaggerFields {
    pub(crate) multiwords: Vec<(String, owned::PosId)>,
}

impl From<MultiwordTaggerFields> for MultiwordTagger {
    fn from(data: MultiwordTaggerFields) -> Self {
        // `AhoCorasick::new_auto_configured` enables a dense DFA for a pattern list this size
        // (<= 5000 phrases). The DFA's transition table costs ~15 MB to hold a ~50 KB phrase
        // list, because every state carries a row over the transition alphabet. The NFA has the
        // exact same search semantics - `auto_configure` never touches `match_kind`, so both are
        // `MatchKind::Standard`, which also makes `find_iter` report overlapping matches - but
        // stores the trie's transitions sparsely. Nothing about the serialized form changes:
        // `matcher` is `#[serde(skip)]` and is rebuilt here on every load.
        let matcher = AhoCorasickBuilder::new()
            .dfa(false)
            .build(data.multiwords.iter().map(|(word, _)| word.as_str()));
        MultiwordTagger {
            matcher,
            multiwords: data.multiwords,
        }
    }
}

/// A tagger which tags consecutive tokens depending on if they are contained in a list of phrases.
///
/// They key difference to the [Tagger][crate::tokenizer::tag::Tagger] is that this tagger looks at sequences of tokens
/// instead of at one token at a time.
#[derive(Deserialize, Serialize, Clone)]
#[serde(from = "MultiwordTaggerFields")]
pub struct MultiwordTagger {
    #[serde(skip)]
    matcher: AhoCorasick,
    multiwords: Vec<(String, owned::PosId)>,
}

impl MultiwordTagger {
    /// Populates the `.multiword_data` field of the passed tokens by checking if any known phrases are contained.
    pub fn apply<'t>(&'t self, sentence: &mut IncompleteSentence<'t>) {
        let tagger = sentence.tagger();

        let mut start_indices = DefaultHashMap::new();
        let mut end_indices = DefaultHashMap::new();
        let mut byte_index = 0;

        let joined = sentence
            .iter()
            .enumerate()
            .map(|(i, x)| {
                start_indices.insert(byte_index, i);
                byte_index += x.word().text().0.len();
                end_indices.insert(byte_index, i);
                byte_index += " ".len();

                x.word().as_str()
            })
            .collect::<Vec<_>>()
            .join(" ");

        for m in self.matcher.find_iter(&joined) {
            if let (Some(start), Some(end)) =
                (start_indices.get(&m.start()), end_indices.get(&m.end()))
            {
                let (word, pos) = &self.multiwords[m.pattern()];
                // end index is inclusive
                for token in sentence.iter_mut().skip(*start).take((end + 1) - start) {
                    let mut data =
                        WordData::new(tagger.id_word(word.as_str().into()), pos.as_ref_id());
                    data.freeze();

                    token.word_mut().push(data);
                    *token.ignore_spelling_mut() = true;
                }
            }
        }
    }
}
