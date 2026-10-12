//! Checks if the input text contains multi-token phrases from a finite list (might contain e. g. city names) and assigns lemmas and part-of-speech tags accordingly.

use crate::types::*;
use aho_corasick::{AhoCorasick, AhoCorasickBuilder, AhoCorasickKind};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) struct MultiwordTaggerFields {
    pub(crate) multiwords: Vec<(String, owned::PosId)>,
}

impl From<MultiwordTaggerFields> for MultiwordTagger {
    fn from(data: MultiwordTaggerFields) -> Self {
        let matcher = AhoCorasickBuilder::new()
            .kind(Some(AhoCorasickKind::ContiguousNFA))
            .build(data.multiwords.iter().map(|(word, _)| word.as_str()))
            .expect("multiword automaton must build");
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

        let mut start_indices = DefaultHashMap::default();
        let mut end_indices = DefaultHashMap::default();
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
                let (word, pos) = &self.multiwords[m.pattern().as_usize()];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_matcher_preserves_standard_match_order_and_utf8_offsets() {
        let patterns = [
            "he",
            "she",
            "hers",
            "his",
            "New York",
            "York",
            "São Paulo",
            "Paulo",
            "“a”—…",
            "\u{2028}",
            "she",
        ];
        let old = aho_corasick_old::AhoCorasickBuilder::new()
            .dfa(false)
            .build(patterns);
        let compact = AhoCorasickBuilder::new()
            .kind(Some(AhoCorasickKind::ContiguousNFA))
            .build(patterns)
            .unwrap();
        for text in [
            "ushers his she hers",
            "New York York",
            "São Paulo Paulo",
            "“a”—…\u{2028}she",
            "It’s fine—…\u{2028}",
            "",
        ] {
            let expected: Vec<_> = old
                .find_iter(text)
                .map(|m| (m.pattern(), m.start(), m.end()))
                .collect();
            let actual: Vec<_> = compact
                .find_iter(text)
                .map(|m| (m.pattern().as_usize(), m.start(), m.end()))
                .collect();
            assert_eq!(actual, expected, "{text:?}");
        }
    }
}
