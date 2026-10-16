use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::echoes::{fold, Lexicon};
use crate::error::StyleError;
use crate::{Document, Span, Token};

pub(crate) const REPORT: &str = "repeatedPhrases";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RepeatedPhraseOptions {
    #[serde(alias = "minimum_length")]
    pub minimum_length: usize,
    #[serde(alias = "maximum_length")]
    pub maximum_length: usize,
    #[serde(alias = "minimum_content_words")]
    pub minimum_content_words: usize,
    #[serde(alias = "minimum_occurrences")]
    pub minimum_occurrences: usize,
    #[serde(alias = "nearby_words")]
    pub nearby_words: usize,
}

impl Default for RepeatedPhraseOptions {
    fn default() -> Self {
        Self {
            minimum_length: 2,
            maximum_length: 5,
            minimum_content_words: 2,
            minimum_occurrences: 3,
            nearby_words: 300,
        }
    }
}

impl RepeatedPhraseOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_length(mut self, minimum_length: usize, maximum_length: usize) -> Self {
        self.minimum_length = minimum_length;
        self.maximum_length = maximum_length;
        self
    }
    pub fn with_minimum_content_words(mut self, minimum_content_words: usize) -> Self {
        self.minimum_content_words = minimum_content_words;
        self
    }
    pub fn with_minimum_occurrences(mut self, count: usize) -> Self {
        self.minimum_occurrences = count;
        self
    }
    pub fn with_nearby_words(mut self, words: usize) -> Self {
        self.nearby_words = words;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RepeatedPhrase {
    pub keys: Vec<String>,
    pub labels: Vec<String>,
    pub count: usize,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RepeatedPhraseReport {
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

pub fn report(
    document: &Document,
    options: &RepeatedPhraseOptions,
) -> Result<RepeatedPhraseReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let lexicon = Lexicon::new(document);
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut names = Vec::new();
    let mut keys = Vec::new();
    let mut units: Vec<Unit> = Vec::new();
    let mut segment = 0;
    let tags = super::dialogue_tags::report(
        document,
        &super::dialogue_tags::DialogueTagOptions::new(),
        None,
    )?;
    let tag_verbs: HashSet<usize> = tags.tags.iter().map(|t| t.verb.byte.start).collect();
    for sentence in document.sentences() {
        for token in sentence.tokens.iter().filter(|token| token.is_word) {
            let masked = token.is_skipped || tag_verbs.contains(&token.span.byte.start);
            let function = masked || lexicon.is_function(token);
            let key = if token.is_skipped {
                "<name>".into()
            } else if masked {
                "<tag>".into()
            } else if function {
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

    let min_length = options.minimum_length.max(1);
    let longest = (0..units.len())
        .map(|start| units[start].end - start)
        .max()
        .unwrap_or(0);
    let max_length = options.maximum_length.min(longest);
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
                    >= options.minimum_content_words.max(1)
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
            if starts.len() < options.minimum_occurrences.max(2)
                && !starts
                    .windows(2)
                    .any(|pair| pair[1] - pair[0] <= options.nearby_words)
            {
                continue;
            }
            repeated.extend(starts.iter().copied());
            if starts.iter().all(|start| {
                longer.contains(start) || (*start > 0 && longer.contains(&(start - 1)))
            }) {
                continue;
            }
            let spans: Vec<Span> = starts
                .iter()
                .map(|start| {
                    Span::covering(
                        &units[*start].token.span,
                        &units[start + length - 1].token.span,
                    )
                })
                .collect();
            let labels = (0..length)
                .map(|offset| {
                    if gram[offset] == *ids.get("<name>").unwrap_or(&u32::MAX) {
                        "<name>".into()
                    } else if gram[offset] == *ids.get("<tag>").unwrap_or(&u32::MAX) {
                        "<tag>".into()
                    } else if units[first + offset].function {
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
    phrases.sort_by_key(|(first, phrase)| (*first, std::cmp::Reverse(phrase.keys.len())));
    let mut merged: Vec<(usize, RepeatedPhrase)> = Vec::new();
    for (first, phrase) in phrases {
        if let Some((previous, longer)) = merged.iter_mut().rev().find(|(previous, longer)| {
            *previous <= first
                && first < *previous + longer.keys.len()
                && longer.count == phrase.count
                && longer
                    .spans
                    .iter()
                    .zip(&phrase.spans)
                    .all(|(a, b)| a.byte.start <= b.byte.start && b.byte.start < a.byte.end)
        }) {
            let offset = first - *previous;
            let overlap = longer
                .keys
                .len()
                .saturating_sub(offset)
                .min(phrase.keys.len());
            if longer.keys[offset..offset + overlap] == phrase.keys[..overlap] {
                longer.keys.extend_from_slice(&phrase.keys[overlap..]);
                longer.labels.extend_from_slice(&phrase.labels[overlap..]);
                for (a, b) in longer.spans.iter_mut().zip(phrase.spans) {
                    if b.byte.end > a.byte.end {
                        *a = Span::covering(a, &b);
                    }
                }
                continue;
            }
        }
        merged.push((first, phrase));
    }
    let mut phrases = merged;
    phrases.sort_by(|(left_start, left), (right_start, right)| {
        right
            .count
            .cmp(&left.count)
            .then(right.keys.len().cmp(&left.keys.len()))
            .then(left_start.cmp(right_start))
    });
    Ok(RepeatedPhraseReport {
        phrases: phrases.into_iter().map(|(_, phrase)| phrase).collect(),
    })
}
