use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::echoes::{fold, is_stopword, Keys};
use crate::{Document, Span, Token};

const MIN_LENGTH: usize = 2;
const MAX_LENGTH: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct RepeatedPhrase {
    pub lemmas: Vec<String>,
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
    stopword: bool,
    end: usize,
}

fn close_segment(units: &mut [Unit], segment: &mut usize) {
    let end = units.len();
    for unit in &mut units[*segment..] {
        unit.end = end;
    }
    *segment = end;
}

pub fn repeated_phrases(document: &Document) -> PhraseReport {
    let surface_keys = Keys::new(document);
    let mut ids: HashMap<String, u32> = HashMap::new();
    let mut lemmas = Vec::new();
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
            let stopword = is_stopword(token);
            let surface = fold(&token.text);
            let key = if stopword {
                surface
            } else {
                surface_keys.key(surface)
            };
            let id = *ids.entry(key).or_insert_with_key(|key| {
                lemmas.push(key.clone());
                lemmas.len() as u32 - 1
            });
            keys.push(id);
            units.push(Unit {
                token,
                stopword,
                end: 0,
            });
        }
        close_segment(&mut units, &mut segment);
    }

    let mut phrases: Vec<(usize, RepeatedPhrase)> = Vec::new();
    let mut longer: HashSet<usize> = HashSet::new();
    for length in (MIN_LENGTH..=MAX_LENGTH).rev() {
        let mut grams: HashMap<&[u32], (usize, Vec<usize>)> = HashMap::new();
        for start in 0..units.len() {
            let end = start + length;
            if end <= units[start].end && units[start..end].iter().any(|unit| !unit.stopword) {
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
            phrases.push((
                first,
                RepeatedPhrase {
                    lemmas: gram.iter().map(|id| lemmas[*id as usize].clone()).collect(),
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
            .then(right.lemmas.len().cmp(&left.lemmas.len()))
            .then(left_start.cmp(right_start))
    });
    PhraseReport {
        phrases: phrases.into_iter().map(|(_, phrase)| phrase).collect(),
    }
}
