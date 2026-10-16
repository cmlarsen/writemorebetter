use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::echoes::{fold, Lexicon};
use crate::{Document, Span, Token};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct PhraseOptions {
    #[serde(alias = "minLength")]
    pub min_length: usize,
    #[serde(alias = "maxLength")]
    pub max_length: usize,
    #[serde(alias = "minContentWords")]
    pub min_content_words: usize,
}

impl Default for PhraseOptions {
    fn default() -> Self {
        Self {
            min_length: 2,
            max_length: 5,
            min_content_words: 2,
        }
    }
}

impl PhraseOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_length(mut self, min_length: usize, max_length: usize) -> Self {
        self.min_length = min_length;
        self.max_length = max_length;
        self
    }
    pub fn with_min_content_words(mut self, min_content_words: usize) -> Self {
        self.min_content_words = min_content_words;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RepeatedPhrase {
    pub keys: Vec<String>,
    pub labels: Vec<String>,
    pub count: usize,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PhraseReport {
    pub phrases: Vec<RepeatedPhrase>,
}

struct Unit<'a> {
    token: &'a Token,
    function: bool,
    end: usize,
}

fn close_segment(units: &mut [Unit], segment: &mut usize) {
    let end = units.len();
    for unit in &mut units[*segment..] {
        unit.end = end;
    }
    *segment = end;
}

pub fn repeated_phrases(document: &Document, options: &PhraseOptions) -> PhraseReport {
    let lexicon = Lexicon::new(document);
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut names = Vec::new();
    let mut keys = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut segment = 0;
    for sentence in document
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.sentences)
    {
        for token in sentence.tokens.iter().filter(|token| token.is_word) {
            if token.is_skipped {
                close_segment(&mut units, &mut segment);
                continue;
            }
            let function = lexicon.is_function(token);
            let key = if function {
                fold(&token.text)
            } else {
                lexicon.family(token)
            };
            let id = *ids.entry(key).or_insert_with_key(|key| {
                names.push(key.clone());
                names.len() as u32 - 1
            });
            keys.push(id);
            units.push(Unit {
                token,
                function,
                end: 0,
            });
        }
        close_segment(&mut units, &mut segment);
    }

    let min_length = options.min_length.max(1);
    let longest = (0..units.len())
        .map(|start| units[start].end - start)
        .max()
        .unwrap_or(0);
    let max_length = options.max_length.min(longest);
    let mut phrases: Vec<(usize, RepeatedPhrase)> = Vec::new();
    let mut longer: HashSet<usize> = HashSet::new();
    for length in (min_length..=max_length).rev() {
        let mut grams: HashMap<&[u32], (usize, Vec<usize>)> = HashMap::new();
        for start in 0..units.len() {
            let end = start + length;
            if end <= units[start].end
                && units[start..end]
                    .iter()
                    .filter(|unit| !unit.function)
                    .count()
                    >= options.min_content_words.max(1)
            {
                grams
                    .entry(&keys[start..end])
                    .and_modify(|(_, later)| later.push(start))
                    .or_insert((start, Vec::new()));
            }
        }
        let mut repeated = HashSet::new();
        for (gram, (first, later)) in grams {
            if later.is_empty() {
                continue;
            }
            let starts = [vec![first], later].concat();
            repeated.extend(starts.iter().copied());
            if starts.iter().all(|start| {
                longer.contains(start) || (*start > 0 && longer.contains(&(start - 1)))
            }) {
                continue;
            }
            let spans: Vec<Span> = starts
                .iter()
                .map(|start| {
                    let head = &units[*start].token.span;
                    let tail = &units[start + length - 1].token.span;
                    Span {
                        byte: head.byte.start..tail.byte.end,
                        char: head.char.start..tail.char.end,
                        utf16: head.utf16.start..tail.utf16.end,
                    }
                })
                .collect();
            let labels = (0..length)
                .map(|offset| {
                    if units[first + offset].function {
                        fold(&units[first + offset].token.text)
                    } else {
                        lexicon.label(starts.iter().map(|start| units[start + offset].token))
                    }
                })
                .collect();
            phrases.push((
                first,
                RepeatedPhrase {
                    keys: gram.iter().map(|id| names[*id as usize].clone()).collect(),
                    labels,
                    count: spans.len(),
                    spans,
                },
            ));
        }
        longer = repeated;
    }
    phrases.sort_by(|(left_start, left), (right_start, right)| {
        right
            .count
            .cmp(&left.count)
            .then(right.keys.len().cmp(&left.keys.len()))
            .then(left_start.cmp(right_start))
    });
    PhraseReport {
        phrases: phrases.into_iter().map(|(_, phrase)| phrase).collect(),
    }
}
