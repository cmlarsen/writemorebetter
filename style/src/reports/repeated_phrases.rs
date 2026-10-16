use std::collections::HashMap;

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
    #[serde(alias = "automatic_names")]
    pub automatic_names: bool,
}

impl Default for RepeatedPhraseOptions {
    fn default() -> Self {
        Self {
            minimum_length: 2,
            maximum_length: 5,
            minimum_content_words: 2,
            minimum_occurrences: 3,
            nearby_words: 300,
            automatic_names: false,
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
    pub fn with_automatic_names(mut self, enabled: bool) -> Self {
        self.automatic_names = enabled;
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
    let mut capitals: HashMap<String, usize> = HashMap::new();
    if options.automatic_names && !document.skip_names_supplied {
        for sentence in document.sentences() {
            for token in sentence.words().skip(1) {
                if token.text.chars().next().is_some_and(char::is_uppercase)
                    && token.readings.iter().any(|r| r.pos.starts_with("NNP"))
                {
                    *capitals.entry(fold(&token.text)).or_default() += 1;
                }
            }
        }
    }
    for sentence in document.sentences() {
        for (index, token) in sentence.tokens.iter().enumerate() {
            if !token.is_word {
                if token
                    .text
                    .contains([',', ';', '—', '–', '-', '…', '”', '"'])
                    || token.text.contains("...")
                    || (token.text == "."
                        && (index.checked_sub(1).is_some_and(|i| {
                            sentence.tokens[i].text == "."
                                && sentence.tokens[i].span.byte.end == token.span.byte.start
                        }) || sentence.tokens.get(index + 1).is_some_and(|next| {
                            next.text == "." && token.span.byte.end == next.span.byte.start
                        })))
                {
                    close_segment(&mut units, &mut segment);
                }
                continue;
            }
            let masked = token.is_skipped
                || token.text.chars().next().is_some_and(char::is_uppercase)
                    && capitals
                        .get(&fold(&token.text))
                        .is_some_and(|count| *count >= 2);
            let title = crate::document::honorific(&fold(&token.text))
                && sentence.tokens[index + 1..]
                    .iter()
                    .find(|t| t.is_word)
                    .is_some_and(|t| t.text.chars().next().is_some_and(char::is_uppercase));
            let function = masked || title || lexicon.is_function(token);
            let key = if masked {
                "<name>".into()
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
    merged.sort_by_key(|(first, phrase)| (std::cmp::Reverse(phrase.keys.len()), *first));
    let mut selected: Vec<(usize, RepeatedPhrase)> = Vec::new();
    for entry in merged {
        if selected.iter().any(|(_, longer)| {
            longer.keys.len() > entry.1.keys.len()
                && entry.1.count < longer.count.saturating_mul(2)
                && entry.1.spans.iter().any(|short| {
                    longer.spans.iter().any(|long| {
                        long.byte.start < short.byte.end && short.byte.start < long.byte.end
                    })
                })
        }) {
            continue;
        }
        selected.push(entry);
    }
    let mut phrases = selected;
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
