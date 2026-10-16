pub mod cliches;
pub mod dialogue_tags;
pub mod diction;
pub mod echoes;
mod norms;
pub mod repeated_phrases;
pub mod sensory_balance;
pub mod sentence_length;
pub mod sentence_starts;

use std::ops::Range;
use std::sync::OnceLock;

use cliches::ClicheList;
use nlprule::Tokenizer;
use serde::{Deserialize, Serialize};

use crate::{Document, DocumentOptions, Mode, Span, StyleError};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, bound(deserialize = "T: Deserialize<'de> + Default"))]
#[non_exhaustive]
pub struct Split<T> {
    pub all: T,
    pub narration: T,
    pub dialogue: T,
}

impl<T> Split<T> {
    pub fn mode(&self, mode: Mode) -> &T {
        match mode {
            Mode::Narration => &self.narration,
            Mode::Dialogue => &self.dialogue,
        }
    }

    pub fn mode_mut(&mut self, mode: Mode) -> &mut T {
        match mode {
            Mode::Narration => &mut self.narration,
            Mode::Dialogue => &mut self.dialogue,
        }
    }

    pub(crate) fn all_and_mode_mut(&mut self, mode: Mode) -> [&mut T; 2] {
        match mode {
            Mode::Narration => [&mut self.all, &mut self.narration],
            Mode::Dialogue => [&mut self.all, &mut self.dialogue],
        }
    }

    pub(crate) fn map<U>(self, mut f: impl FnMut(T) -> U) -> Split<U> {
        Split {
            all: f(self.all),
            narration: f(self.narration),
            dialogue: f(self.dialogue),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Share {
    pub count: usize,
    pub percent: Option<f64>,
    pub difference: Option<f64>,
    pub highlighted: bool,
}

impl Share {
    pub(crate) fn new(count: usize, total: usize, enough: bool, baseline: Option<f64>) -> Self {
        let percent = (enough && total > 0).then(|| 100.0 * count as f64 / total as f64);
        Self {
            count,
            percent,
            difference: percent.zip(baseline).map(|(a, b)| a - b),
            highlighted: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct PercentileBand {
    pub p5: f64,
    pub p95: f64,
}

impl PercentileBand {
    pub fn new(p5: f64, p95: f64) -> Self {
        Self { p5, p95 }
    }

    pub(crate) fn validate(&self, report: &str, maximum: f64) -> Result<(), StyleError> {
        if self.p5.is_finite()
            && self.p95.is_finite()
            && self.p5 >= 0.0
            && self.p5 <= self.p95
            && self.p95 <= maximum
        {
            Ok(())
        } else {
            Err(StyleError::baseline(
                report,
                "bands require 0 <= p5 <= p95 within the measure's range",
            ))
        }
    }

    pub(crate) fn highlights(band: Option<&Self>, value: Option<f64>) -> bool {
        band.zip(value)
            .is_some_and(|(band, value)| value < band.p5 || value > band.p95)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceRow {
    pub span: Span,
    pub mode: Mode,
    pub words: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceRun {
    pub span: Span,
    pub sentences: Range<usize>,
}

pub fn sentence_table(document: &Document) -> Vec<SentenceRow> {
    document
        .sentences()
        .map(|sentence| SentenceRow {
            span: sentence.span.clone(),
            mode: document.sentence_mode(sentence),
            words: sentence.word_count(),
        })
        .collect()
}

pub(crate) fn bundled_baseline<T: serde::de::DeserializeOwned>(json: &str) -> T {
    let mut file: serde_json::Value = serde_json::from_str(json).expect("bundled baseline is JSON");
    serde_json::from_value(file["baseline"].take()).expect("bundled baseline matches its record")
}

pub(crate) fn word_split(document: &Document) -> Split<usize> {
    let mut words = Split::default();
    for token in document.words() {
        words.all += 1;
        *words.mode_mut(document.mode_at(token.span.byte.start)) += 1;
    }
    words
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct DocumentStats {
    pub paragraphs: usize,
    pub hard_wrapped: bool,
    pub token_details: bool,
    pub words: Split<usize>,
    pub sentences: Split<usize>,
    pub dialogue_spans: usize,
    pub speech_spans: usize,
}

impl DocumentStats {
    pub fn new(document: &Document) -> Self {
        let mut sentences = Split::default();
        for sentence in document.sentences() {
            sentences.all += 1;
            *sentences.mode_mut(document.sentence_mode(sentence)) += 1;
        }
        Self {
            paragraphs: document.paragraphs.len(),
            hard_wrapped: document.hard_wrapped,
            token_details: document.token_details,
            words: word_split(document),
            sentences,
            dialogue_spans: document.dialogue.len(),
            speech_spans: (0..document.dialogue.len())
                .filter(|&index| document.is_speech(index))
                .count(),
        }
    }
}

#[derive(Debug, Default)]
pub struct StyleResources {
    cliches: OnceLock<ClicheList>,
}

impl StyleResources {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cliches(&self, tokenizer: &Tokenizer) -> &ClicheList {
        self.cliches.get_or_init(|| ClicheList::bundled(tokenizer))
    }
}

pub fn parse_document_options(json: Option<&str>) -> Result<DocumentOptions, StyleError> {
    json.map(serde_json::from_str)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(StyleError::options)
}

pub fn to_utf16_json<T: Serialize>(value: &T) -> String {
    crate::document::with_utf16_spans(|| {
        serde_json::to_string(value).expect("style records serialize to JSON")
    })
}
