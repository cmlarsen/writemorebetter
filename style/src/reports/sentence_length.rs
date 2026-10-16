use super::{covering, dialogue};
use crate::{Document, Span};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Options {
    pub minimum_sentences: usize,
    pub long_sentence_words: usize,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            minimum_sentences: 200,
            long_sentence_words: 40,
        }
    }
}
impl Options {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct Baseline {
    pub mean: Option<f64>,
    pub percentiles: BTreeMap<u8, f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Measurement {
    pub span: Span,
    pub words: usize,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Group {
    pub sentences: Vec<Measurement>,
    pub histogram: BTreeMap<usize, usize>,
    pub mean: Option<f64>,
    pub percentiles: BTreeMap<u8, Option<f64>>,
    pub mean_difference: Option<f64>,
    pub percentile_differences: BTreeMap<u8, Option<f64>>,
    pub long_sentences: Vec<Measurement>,
    pub monotony_runs: Vec<Span>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Report {
    pub all: Group,
    pub narration: Group,
    pub dialogue: Group,
}

fn percentile(sorted: &[usize], percent: u8) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let index = (sorted.len() - 1) as f64 * f64::from(percent.min(100)) / 100.0;
    let low = index.floor() as usize;
    let high = (low + 1).min(sorted.len() - 1);
    Some(sorted[low] as f64 + (sorted[high] as f64 - sorted[low] as f64) * index.fract())
}

fn finish(group: &mut Group, options: &Options, baseline: Option<&Baseline>) {
    let mut lengths: Vec<_> = group.sentences.iter().map(|s| s.words).collect();
    lengths.sort_unstable();
    let measurable = !lengths.is_empty() && lengths.len() >= options.minimum_sentences;
    group.mean = measurable.then(|| lengths.iter().sum::<usize>() as f64 / lengths.len() as f64);
    group.mean_difference = group
        .mean
        .zip(baseline.and_then(|b| b.mean))
        .map(|(a, b)| a - b);
    for p in [5, 25, 50, 75, 95] {
        let value = measurable.then(|| percentile(&lengths, p)).flatten();
        group.percentiles.insert(p, value);
        group.percentile_differences.insert(
            p,
            value
                .zip(baseline.and_then(|b| b.percentiles.get(&p)))
                .map(|(a, b)| a - b),
        );
    }
}

pub fn report(document: &Document, options: &Options, baseline: Option<&Baseline>) -> Report {
    let mut result = Report {
        all: Group::default(),
        narration: Group::default(),
        dialogue: Group::default(),
    };
    for paragraph in &document.paragraphs {
        let mut sequence: Vec<(bool, Measurement)> = Vec::new();
        for sentence in &paragraph.sentences {
            let words = sentence
                .tokens
                .iter()
                .filter(|t| t.is_word && !t.is_skipped)
                .count();
            let measurement = Measurement {
                span: sentence.span.clone(),
                words,
            };
            let speech = dialogue(document, sentence);
            for group in [
                &mut result.all,
                if speech {
                    &mut result.dialogue
                } else {
                    &mut result.narration
                },
            ] {
                *group.histogram.entry(words).or_default() += 1;
                group.sentences.push(measurement.clone());
                if words >= options.long_sentence_words {
                    group.long_sentences.push(measurement.clone());
                }
            }
            sequence.push((speech, measurement));
        }
        for mode in [None, Some(false), Some(true)] {
            let group = match mode {
                None => &mut result.all,
                Some(false) => &mut result.narration,
                Some(true) => &mut result.dialogue,
            };
            let mut start = 0;
            let mut lengths = BTreeMap::<usize, usize>::new();
            for end in 0..=sequence.len() {
                let boundary = end == sequence.len() || mode.is_some_and(|m| m != sequence[end].0);
                let incompatible = !boundary && !lengths.is_empty() && {
                    let words = sequence[end].1.words;
                    lengths.last_key_value().unwrap().0.max(&words)
                        - lengths.first_key_value().unwrap().0.min(&words)
                        > 2
                };
                if (boundary || incompatible) && end - start >= 4 {
                    group
                        .monotony_runs
                        .push(covering(&sequence[start].1.span, &sequence[end - 1].1.span));
                }
                if boundary {
                    lengths.clear();
                    start = end + 1;
                    continue;
                }
                *lengths.entry(sequence[end].1.words).or_default() += 1;
                while lengths.last_key_value().unwrap().0 - lengths.first_key_value().unwrap().0 > 2
                {
                    let words = sequence[start].1.words;
                    let count = lengths.get_mut(&words).unwrap();
                    *count -= 1;
                    if *count == 0 {
                        lengths.remove(&words);
                    }
                    start += 1;
                }
            }
        }
    }
    for group in [&mut result.all, &mut result.narration, &mut result.dialogue] {
        finish(group, options, baseline);
    }
    result
}
