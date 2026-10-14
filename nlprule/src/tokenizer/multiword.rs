//! Checks if the input text contains multi-token phrases from a finite list (might contain e. g. city names) and assigns lemmas and part-of-speech tags accordingly.

use crate::types::*;
use serde::{Deserialize, Serialize};

const MAX_TOKENS_IN_MULTIWORD: usize = 20;

#[derive(Serialize, Deserialize)]
pub(crate) struct MultiwordTaggerFields {
    pub(crate) multiwords: Vec<(String, String, owned::PosId)>,
}

impl From<MultiwordTaggerFields> for MultiwordTagger {
    fn from(data: MultiwordTaggerFields) -> Self {
        let mut full = DefaultHashMap::default();
        let mut start_space: DefaultHashMap<String, usize> = DefaultHashMap::default();
        let mut start_no_space: DefaultHashMap<char, usize> = DefaultHashMap::default();

        for (i, (phrase, _, _)) in data.multiwords.iter().enumerate() {
            full.insert(phrase.clone(), i);
            if phrase.contains(' ') {
                let mut words = phrase.split(' ');
                let first = words.next().unwrap_or_default().to_owned();
                let n_words = 1 + words.count();
                let max = start_space.entry(first).or_default();
                *max = (*max).max(n_words);
            } else if let Some(first) = phrase.chars().next() {
                let max = start_no_space.entry(first).or_default();
                *max = (*max).max(phrase.len());
            }
        }

        MultiwordTagger {
            full,
            start_space,
            start_no_space,
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
    full: DefaultHashMap<String, usize>,
    #[serde(skip)]
    start_space: DefaultHashMap<String, usize>,
    #[serde(skip)]
    start_no_space: DefaultHashMap<char, usize>,
    multiwords: Vec<(String, String, owned::PosId)>,
}

struct Hit {
    start: usize,
    end: usize,
    index: usize,
}

impl MultiwordTagger {
    fn find_hits(&self, texts: &[&str], space_before: &[bool]) -> Vec<Hit> {
        let mut hits = Vec::new();

        for i in 0..texts.len() {
            if texts[i].is_empty() {
                continue;
            }
            let mut chunk = texts[i].to_owned();
            for k in i + 1..texts.len() {
                if space_before[k] {
                    break;
                }
                chunk.push_str(texts[k]);
            }

            if let Some(&max_words) = self.start_space.get(&chunk) {
                let mut key = String::new();
                let mut n_spaces = 0;
                let mut steps = 0;
                for j in i..texts.len() {
                    if j > i && space_before[j] {
                        key.push(' ');
                        n_spaces += 1;
                        steps += 1;
                        if n_spaces == max_words || steps >= MAX_TOKENS_IN_MULTIWORD {
                            break;
                        }
                    }
                    key.push_str(texts[j]);
                    steps += 1;
                    if let Some(&index) = self.full.get(&key) {
                        if key.contains(' ') {
                            hits.push(Hit {
                                start: i,
                                end: j,
                                index,
                            });
                        }
                    }
                    if steps >= MAX_TOKENS_IN_MULTIWORD {
                        break;
                    }
                }
            }

            let max_len = chunk
                .chars()
                .next()
                .and_then(|first| self.start_no_space.get(&first));
            if let Some(&max_len) = max_len {
                let mut key = String::new();
                for j in i..texts.len().min(i + MAX_TOKENS_IN_MULTIWORD) {
                    if j > i && space_before[j] {
                        break;
                    }
                    key.push_str(texts[j]);
                    if key.len() > max_len {
                        break;
                    }
                    if let Some(&index) = self.full.get(&key) {
                        hits.push(Hit {
                            start: i,
                            end: j,
                            index,
                        });
                    }
                }
            }
        }

        hits
    }

    /// Populates the `.multiword_data` field of the passed tokens by checking if any known phrases are contained.
    ///
    /// If `remove_previous_tags` is set, every token of a phrase spanning more than one token keeps only the
    /// phrase's tag. Where such phrases overlap, the longest one starting first wins.
    pub fn apply<'t>(&'t self, sentence: &mut IncompleteSentence<'t>, remove_previous_tags: bool) {
        let tagger = sentence.tagger();

        let hits = {
            let texts: Vec<&str> = sentence.iter().map(|x| x.word().text().as_str()).collect();
            let space_before: Vec<bool> = sentence.iter().map(|x| x.has_space_before()).collect();
            self.find_hits(&texts, &space_before)
        };
        if hits.is_empty() {
            return;
        }

        let data_for = |index: usize| {
            let (_, lemma, pos) = &self.multiwords[index];
            WordData::new(tagger.id_word(lemma.as_str().into()), pos.as_ref_id())
        };

        let mut tokens: Vec<_> = sentence.iter_mut().collect();
        for hit in &hits {
            for token in &mut tokens[hit.start..=hit.end] {
                *token.ignore_spelling_mut() = true;
            }
        }

        if !remove_previous_tags {
            for hit in &hits {
                for token in &mut tokens[hit.start..=hit.end] {
                    token.word_mut().push(data_for(hit.index));
                }
            }
            return;
        }

        let mut covered = vec![false; tokens.len()];
        let mut i = 0;
        while i < tokens.len() {
            let longest = hits
                .iter()
                .filter(|hit| hit.start == i && hit.end > i)
                .max_by_key(|hit| hit.end);
            if let Some(hit) = longest {
                for k in hit.start..=hit.end {
                    tokens[k].word_mut().clear();
                    tokens[k].word_mut().push(data_for(hit.index));
                    covered[k] = true;
                }
                i = hit.end + 1;
            } else {
                i += 1;
            }
        }

        for hit in hits.iter().filter(|hit| hit.start == hit.end) {
            if !covered[hit.start] {
                tokens[hit.start].word_mut().push(data_for(hit.index));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagger(phrases: &[(&str, &str)]) -> MultiwordTagger {
        let pos = owned::PosId("NNP".into(), PosIdInt::from_value_unchecked(0));
        MultiwordTaggerFields {
            multiwords: phrases
                .iter()
                .map(|(phrase, lemma)| ((*phrase).into(), (*lemma).into(), pos.clone()))
                .collect(),
        }
        .into()
    }

    fn hits(tagger: &MultiwordTagger, tokens: &[(&str, bool)]) -> Vec<(usize, usize, String)> {
        let texts: Vec<&str> = tokens.iter().map(|(text, _)| *text).collect();
        let space_before: Vec<bool> = tokens.iter().map(|(_, space)| *space).collect();
        tagger
            .find_hits(&texts, &space_before)
            .into_iter()
            .map(|hit| (hit.start, hit.end, tagger.multiwords[hit.index].1.clone()))
            .collect()
    }

    #[test]
    fn matches_phrases_split_into_adjacent_tokens() {
        let tagger = tagger(&[
            ("O'Brien", "O'Brien"),
            ("O'BRIEN", "O'Brien"),
            ("O’BRIEN", "O’Brien"),
            ("L’ORÉAL", "L’Oréal"),
        ]);
        for (o, apostrophe, rest, lemma) in [
            ("O", "'", "BRIEN", "O'Brien"),
            ("O", "’", "BRIEN", "O’Brien"),
            ("L", "’", "ORÉAL", "L’Oréal"),
        ] {
            let tokens = [
                ("“", true),
                (o, false),
                (apostrophe, false),
                (rest, false),
                ("—", false),
                ("…", false),
                ("\u{2028}", true),
            ];
            assert_eq!(hits(&tagger, &tokens), vec![(1, 3, lemma.to_owned())]);
        }
    }

    #[test]
    fn phrases_do_not_match_across_whitespace_or_mid_word() {
        let tagger = tagger(&[("O'BRIEN", "O'Brien")]);
        assert!(hits(&tagger, &[("O", true), ("'", true), ("BRIEN", false)]).is_empty());
        assert!(hits(&tagger, &[("O", true), ("'", false), ("BRIENS", false)]).is_empty());
    }

    #[test]
    fn spaced_phrases_match_at_token_boundaries() {
        let tagger = tagger(&[
            ("Samuel L. Jackson", "Samuel L. Jackson"),
            ("STANLEY KUBRICK", "Stanley Kubrick"),
        ]);
        let tokens = [
            ("Samuel", true),
            ("L", true),
            (".", false),
            ("Jackson", true),
            ("’s", false),
        ];
        assert_eq!(
            hits(&tagger, &tokens),
            vec![(0, 3, "Samuel L. Jackson".to_owned())]
        );
        let tokens = [
            ("‘", true),
            ("STANLEY", false),
            ("KUBRICK", true),
            ("’", false),
        ];
        assert_eq!(
            hits(&tagger, &tokens),
            vec![(1, 2, "Stanley Kubrick".to_owned())]
        );
        let tokens = [("STANLEY", true), ("-", false), ("KUBRICK", false)];
        assert!(hits(&tagger, &tokens).is_empty());
        let tokens = [("STANLEY", true), ("KUBRICK", true), ("…", false)];
        assert_eq!(
            hits(&tagger, &tokens),
            vec![(0, 1, "Stanley Kubrick".to_owned())]
        );
    }
}
