use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{PercentileBand, SentenceRun, Split};
use crate::error::StyleError;
use crate::{Document, Mode, Span};

pub(crate) const REPORT: &str = "sentenceLength";
pub const PERCENTILES: [u8; 5] = [5, 25, 50, 75, 95];
pub const MONOTONY_MIN_SENTENCES: usize = 4;
pub const MONOTONY_MAX_SPREAD: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthOptions {
    #[serde(alias = "minimum_sentences")]
    pub minimum_sentences: usize,
    #[serde(alias = "long_sentence_words")]
    pub long_sentence_words: usize,
    #[serde(alias = "longest_per_chapter")]
    pub longest_per_chapter: usize,
}

impl Default for SentenceLengthOptions {
    fn default() -> Self {
        Self {
            minimum_sentences: 50,
            long_sentence_words: 40,
            longest_per_chapter: 3,
        }
    }
}

impl SentenceLengthOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_sentences(mut self, sentences: usize) -> Self {
        self.minimum_sentences = sentences;
        self
    }
    pub fn with_long_sentence_words(mut self, words: usize) -> Self {
        self.long_sentence_words = words;
        self
    }
    pub fn with_longest_per_chapter(mut self, count: usize) -> Self {
        self.longest_per_chapter = count;
        self
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthBands {
    pub mean: Option<PercentileBand>,
    pub percentiles: BTreeMap<u8, PercentileBand>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthBaseline {
    pub mean: Option<f64>,
    pub percentiles: BTreeMap<u8, f64>,
    pub bands: Split<SentenceLengthBands>,
    pub narration: Option<Box<SentenceLengthBaseline>>,
    pub dialogue: Option<Box<SentenceLengthBaseline>>,
}

impl SentenceLengthBaseline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_mean(mut self, mean: f64) -> Self {
        self.mean = Some(mean);
        self
    }

    pub fn with_percentile(mut self, percentile: u8, words: f64) -> Self {
        self.percentiles.insert(percentile, words);
        self
    }

    pub fn bundled() -> Option<Self> {
        static BUNDLED: OnceLock<SentenceLengthBaseline> = OnceLock::new();
        Some(
            BUNDLED
                .get_or_init(|| {
                    super::bundled_baseline(include_str!(
                        "../../data/baselines/sentence-length.json"
                    ))
                })
                .clone(),
        )
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        let words = |name: &str, value: f64| {
            if value.is_finite() && value >= 0.0 {
                Ok(())
            } else {
                Err(StyleError::baseline(
                    REPORT,
                    format!("{name} must be a finite, non-negative word count, got {value}"),
                ))
            }
        };
        if let Some(mean) = self.mean {
            words("mean", mean)?;
        }
        for (percentile, value) in &self.percentiles {
            if !PERCENTILES.contains(percentile) {
                return Err(StyleError::baseline(
                    REPORT,
                    format!("percentile keys must be one of 5, 25, 50, 75, 95, got {percentile}"),
                ));
            }
            words("percentile", *value)?;
        }
        for bands in [&self.bands.all, &self.bands.narration, &self.bands.dialogue] {
            if let Some(band) = &bands.mean {
                band.validate(REPORT, f64::MAX)?;
            }
            for (p, band) in &bands.percentiles {
                if !PERCENTILES.contains(p) {
                    return Err(StyleError::baseline(REPORT, "unknown band percentile"));
                }
                band.validate(REPORT, f64::MAX)?;
            }
        }
        for baseline in [&self.narration, &self.dialogue].into_iter().flatten() {
            baseline.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthBucket {
    pub words: usize,
    pub sentences: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthPercentile {
    pub percentile: u8,
    pub value: Option<f64>,
    pub difference: Option<f64>,
    pub highlighted: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthGroup {
    pub sentences: usize,
    pub zero_word_sentences: usize,
    pub histogram: Vec<SentenceLengthBucket>,
    pub mean: Option<f64>,
    pub mean_difference: Option<f64>,
    pub mean_highlighted: bool,
    pub low_confidence: bool,
    pub percentiles: Vec<SentenceLengthPercentile>,
    pub long_sentences: Vec<usize>,
    pub monotony_runs: Vec<SentenceRun>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceLengthReport {
    pub groups: Split<SentenceLengthGroup>,
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

#[derive(Default)]
struct Builder {
    lengths: Vec<usize>,
    histogram: BTreeMap<usize, usize>,
    long_sentences: Vec<usize>,
    monotony_runs: Vec<SentenceRun>,
}

impl Builder {
    fn finish(
        mut self,
        options: &SentenceLengthOptions,
        baseline: Option<&SentenceLengthBaseline>,
        bands: Option<&SentenceLengthBands>,
    ) -> SentenceLengthGroup {
        let sentences = self.lengths.len();
        self.lengths.sort_unstable();
        let measurable = sentences != 0 && sentences >= options.minimum_sentences;
        let mean = measurable.then(|| self.lengths.iter().sum::<usize>() as f64 / sentences as f64);
        SentenceLengthGroup {
            sentences,
            zero_word_sentences: self.histogram.get(&0).copied().unwrap_or(0),
            histogram: self
                .histogram
                .into_iter()
                .map(|(words, sentences)| SentenceLengthBucket { words, sentences })
                .collect(),
            mean,
            mean_difference: mean.zip(baseline.and_then(|b| b.mean)).map(|(a, b)| a - b),
            mean_highlighted: PercentileBand::highlights(bands.and_then(|b| b.mean.as_ref()), mean),
            low_confidence: sentences < 200,
            percentiles: PERCENTILES
                .into_iter()
                .map(|p| {
                    let value = measurable.then(|| percentile(&self.lengths, p)).flatten();
                    SentenceLengthPercentile {
                        percentile: p,
                        value,
                        difference: value
                            .zip(baseline.and_then(|b| b.percentiles.get(&p)))
                            .map(|(a, b)| a - b),
                        highlighted: PercentileBand::highlights(
                            bands.and_then(|b| b.percentiles.get(&p)),
                            value,
                        ),
                    }
                })
                .collect(),
            long_sentences: self.long_sentences,
            monotony_runs: self.monotony_runs,
        }
    }
}

struct Measured {
    index: usize,
    mode: Mode,
    span: Span,
    words: usize,
}

fn monotony_runs(sequence: &[Measured], mode: Option<Mode>, runs: &mut Vec<SentenceRun>) {
    let mut start = 0;
    let mut lengths = BTreeMap::<usize, usize>::new();
    for end in 0..=sequence.len() {
        let boundary = end == sequence.len() || mode.is_some_and(|m| m != sequence[end].mode);
        let incompatible = !boundary && !lengths.is_empty() && {
            let words = sequence[end].words;
            lengths.last_key_value().unwrap().0.max(&words)
                - lengths.first_key_value().unwrap().0.min(&words)
                > MONOTONY_MAX_SPREAD
        };
        if (boundary || incompatible) && end - start >= MONOTONY_MIN_SENTENCES {
            runs.push(SentenceRun {
                span: Span::covering(&sequence[start].span, &sequence[end - 1].span),
                sentences: sequence[start].index..sequence[end - 1].index + 1,
            });
        }
        if boundary {
            lengths.clear();
            start = end + 1;
            continue;
        }
        *lengths.entry(sequence[end].words).or_default() += 1;
        while lengths.last_key_value().unwrap().0 - lengths.first_key_value().unwrap().0
            > MONOTONY_MAX_SPREAD
        {
            let words = sequence[start].words;
            let count = lengths.get_mut(&words).unwrap();
            *count -= 1;
            if *count == 0 {
                lengths.remove(&words);
            }
            start += 1;
        }
    }
}

pub fn report(
    document: &Document,
    options: &SentenceLengthOptions,
    baseline: Option<&SentenceLengthBaseline>,
) -> Result<SentenceLengthReport, StyleError> {
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let mut groups: Split<Builder> = Split::default();
    let mut index = 0;
    let mut sequence = Vec::new();
    for paragraph in &document.paragraphs {
        if paragraph.scene_break_before {
            monotony_runs(
                &sequence,
                Some(Mode::Narration),
                &mut groups.all.monotony_runs,
            );
            sequence.clear();
        }
        for sentence in &paragraph.sentences {
            let words = sentence.word_count();
            let mode = document.sentence_mode(sentence);
            for group in groups.all_and_mode_mut(mode) {
                group.lengths.push(words);
                *group.histogram.entry(words).or_default() += 1;
            }
            sequence.push(Measured {
                index,
                mode: if document.sentence_has_speech(sentence) {
                    Mode::Dialogue
                } else {
                    mode
                },
                span: sentence.span.clone(),
                words,
            });
            index += 1;
        }
    }
    monotony_runs(
        &sequence,
        Some(Mode::Narration),
        &mut groups.all.monotony_runs,
    );
    groups.narration.monotony_runs = groups.all.monotony_runs.clone();
    let mut chapters: BTreeMap<usize, Vec<(usize, usize, Mode)>> = BTreeMap::new();
    let mut chapter = 0;
    let mut index = 0;
    for paragraph in &document.paragraphs {
        if paragraph.chapter_start {
            chapter += 1;
        }
        for sentence in &paragraph.sentences {
            chapters.entry(chapter).or_default().push((
                index,
                sentence.word_count(),
                document.sentence_mode(sentence),
            ));
            index += 1;
        }
    }
    for sentences in chapters.values_mut() {
        sentences.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let mut lengths: Vec<_> = sentences.iter().map(|s| s.1).collect();
        lengths.sort_unstable();
        let p95 = percentile(&lengths, 95).unwrap_or(0.0);
        for (rank, &(index, words, mode)) in sentences.iter().enumerate() {
            if words >= options.long_sentence_words
                && (rank < options.longest_per_chapter || words as f64 > p95)
            {
                for group in groups.all_and_mode_mut(mode) {
                    group.long_sentences.push(index);
                }
            }
        }
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        group.long_sentences.sort_unstable();
    }
    Ok(SentenceLengthReport {
        groups: Split {
            all: groups
                .all
                .finish(options, baseline, baseline.map(|b| &b.bands.all)),
            narration: groups.narration.finish(
                options,
                baseline.and_then(|b| b.narration.as_deref()),
                baseline.map(|b| &b.bands.narration),
            ),
            dialogue: groups.dialogue.finish(
                options,
                baseline.and_then(|b| b.dialogue.as_deref()),
                baseline.map(|b| &b.bands.dialogue),
            ),
        },
    })
}
