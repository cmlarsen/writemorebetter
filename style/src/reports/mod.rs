pub mod cliches;
pub mod dialogue_tags;
pub mod echoes;
pub mod repeated_phrases;
pub mod sentence_length;
pub mod sentence_starts;

use std::fmt;
use std::ops::Range;
use std::sync::OnceLock;

use nlprule::Tokenizer;
use serde::{Deserialize, Serialize};

use crate::{analyze_document, Document, DocumentOptions, Mode, Span, StyleError};
use cliches::{ClicheList, ClicheOptions, ClicheReport};
use dialogue_tags::{DialogueTagBaseline, DialogueTagOptions, DialogueTagReport};
use echoes::{EchoOptions, EchoReport};
use repeated_phrases::{RepeatedPhraseOptions, RepeatedPhraseReport};
use sentence_length::{SentenceLengthBaseline, SentenceLengthOptions, SentenceLengthReport};
use sentence_starts::{SentenceStartsBaseline, SentenceStartsOptions, SentenceStartsReport};

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

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StyleBaselines {
    pub sentence_length: Option<SentenceLengthBaseline>,
    pub sentence_starts: Option<SentenceStartsBaseline>,
    pub dialogue_tags: Option<DialogueTagBaseline>,
}

impl StyleBaselines {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn bundled() -> Self {
        Self {
            sentence_length: SentenceLengthBaseline::bundled(),
            sentence_starts: SentenceStartsBaseline::bundled(),
            dialogue_tags: DialogueTagBaseline::bundled(),
        }
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        if let Some(baseline) = &self.sentence_length {
            baseline.validate()?;
        }
        if let Some(baseline) = &self.sentence_starts {
            baseline.validate()?;
        }
        if let Some(baseline) = &self.dialogue_tags {
            baseline.validate()?;
        }
        Ok(())
    }
}

impl<'de> Deserialize<'de> for StyleBaselines {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase", deny_unknown_fields)]
        struct Explicit {
            #[serde(default, alias = "sentence_length")]
            sentence_length: Option<SentenceLengthBaseline>,
            #[serde(default, alias = "sentence_starts")]
            sentence_starts: Option<SentenceStartsBaseline>,
            #[serde(default, alias = "dialogue_tags")]
            dialogue_tags: Option<DialogueTagBaseline>,
        }
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = StyleBaselines;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("\"bundled\", null, or an object of report baselines")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "bundled" {
                    Ok(StyleBaselines::bundled())
                } else {
                    Err(E::invalid_value(serde::de::Unexpected::Str(value), &self))
                }
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(StyleBaselines::none())
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                let explicit =
                    Explicit::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
                Ok(StyleBaselines {
                    sentence_length: explicit.sentence_length,
                    sentence_starts: explicit.sentence_starts,
                    dialogue_tags: explicit.dialogue_tags,
                })
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[non_exhaustive]
pub struct StyleRequest {
    #[serde(default)]
    pub echoes: Option<EchoOptions>,
    #[serde(default, alias = "repeated_phrases")]
    pub repeated_phrases: Option<RepeatedPhraseOptions>,
    #[serde(default)]
    pub cliches: Option<ClicheOptions>,
    #[serde(default, alias = "dialogue_tags")]
    pub dialogue_tags: Option<DialogueTagOptions>,
    #[serde(default, alias = "sentence_length")]
    pub sentence_length: Option<SentenceLengthOptions>,
    #[serde(default, alias = "sentence_starts")]
    pub sentence_starts: Option<SentenceStartsOptions>,
    #[serde(default)]
    pub baselines: StyleBaselines,
}

impl Default for StyleRequest {
    fn default() -> Self {
        Self {
            echoes: None,
            repeated_phrases: None,
            cliches: Some(ClicheOptions::new()),
            dialogue_tags: Some(DialogueTagOptions::new()),
            sentence_length: Some(SentenceLengthOptions::new()),
            sentence_starts: Some(SentenceStartsOptions::new()),
            baselines: StyleBaselines::bundled(),
        }
    }
}

impl StyleRequest {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn empty() -> Self {
        Self {
            echoes: None,
            repeated_phrases: None,
            cliches: None,
            dialogue_tags: None,
            sentence_length: None,
            sentence_starts: None,
            baselines: StyleBaselines::none(),
        }
    }

    pub fn from_json(json: &str) -> Result<Self, StyleError> {
        serde_json::from_str(json).map_err(StyleError::options)
    }

    pub fn validate(&self, document: &Document) -> Result<(), StyleError> {
        self.baselines.validate()?;
        if !document.token_details {
            for (report, selected) in [
                (echoes::REPORT, self.echoes.is_some()),
                (repeated_phrases::REPORT, self.repeated_phrases.is_some()),
                (dialogue_tags::REPORT, self.dialogue_tags.is_some()),
                (sentence_starts::REPORT, self.sentence_starts.is_some()),
            ] {
                if selected {
                    return Err(StyleError::needs_token_details(report));
                }
            }
        }
        Ok(())
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct StyleReports {
    pub stats: DocumentStats,
    pub sentences: Vec<SentenceRow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub echoes: Option<EchoReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repeated_phrases: Option<RepeatedPhraseReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cliches: Option<ClicheReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialogue_tags: Option<DialogueTagReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentence_length: Option<SentenceLengthReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sentence_starts: Option<SentenceStartsReport>,
}

pub fn run(
    document: &Document,
    request: &StyleRequest,
    resources: &StyleResources,
    tokenizer: &Tokenizer,
) -> Result<StyleReports, StyleError> {
    request.validate(document)?;
    let baselines = &request.baselines;
    let sentences = if request.sentence_length.is_some() || request.sentence_starts.is_some() {
        sentence_table(document)
    } else {
        Vec::new()
    };
    Ok(StyleReports {
        stats: DocumentStats::new(document),
        sentences,
        echoes: request
            .echoes
            .as_ref()
            .map(|options| echoes::report(document, options))
            .transpose()?,
        repeated_phrases: request
            .repeated_phrases
            .as_ref()
            .map(|options| repeated_phrases::report(document, options))
            .transpose()?,
        cliches: request
            .cliches
            .as_ref()
            .map(|options| cliches::report(document, resources.cliches(tokenizer), options))
            .transpose()?,
        dialogue_tags: request
            .dialogue_tags
            .as_ref()
            .map(|options| {
                dialogue_tags::report(document, options, baselines.dialogue_tags.as_ref())
            })
            .transpose()?,
        sentence_length: request
            .sentence_length
            .as_ref()
            .map(|options| {
                sentence_length::report(document, options, baselines.sentence_length.as_ref())
            })
            .transpose()?,
        sentence_starts: request
            .sentence_starts
            .as_ref()
            .map(|options| {
                sentence_starts::report(document, options, baselines.sentence_starts.as_ref())
            })
            .transpose()?,
    })
}

pub fn parse_document_options(json: Option<&str>) -> Result<DocumentOptions, StyleError> {
    json.map(serde_json::from_str)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(StyleError::options)
}

pub fn run_json(
    tokenizer: &Tokenizer,
    resources: &StyleResources,
    text: &str,
    request_json: Option<&str>,
    document_options: &DocumentOptions,
) -> Result<String, StyleError> {
    let request = request_json
        .map(StyleRequest::from_json)
        .transpose()?
        .unwrap_or_default();
    request.baselines.validate()?;
    let options = document_options.clone().with_token_details(true);
    let document = analyze_document(tokenizer, text, &options);
    Ok(to_utf16_json(&run(
        &document, &request, resources, tokenizer,
    )?))
}

pub fn to_utf16_json<T: Serialize>(value: &T) -> String {
    crate::document::with_utf16_spans(|| {
        serde_json::to_string(value).expect("style records serialize to JSON")
    })
}
