pub mod cliches;
pub mod dialogue_tags;
pub mod echoes;
pub mod phrases;
pub mod sentence_length;
pub mod sentence_starts;

use crate::{Document, Sentence, Span};

pub(crate) fn dialogue(document: &Document, sentence: &Sentence) -> bool {
    sentence
        .tokens
        .iter()
        .find(|t| t.is_word)
        .is_some_and(|first| {
            let index = document
                .dialogue
                .partition_point(|s| s.byte.end <= first.span.byte.start);
            document
                .dialogue
                .get(index)
                .is_some_and(|s| s.byte.start <= first.span.byte.start)
        })
}

pub(crate) fn covering(first: &Span, last: &Span) -> Span {
    Span {
        byte: first.byte.start..last.byte.end,
        char: first.char.start..last.char.end,
        utf16: first.utf16.start..last.utf16.end,
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceReportsRequest {
    #[serde(default)]
    pub sentence_length: Option<sentence_length::Options>,
    #[serde(default)]
    pub sentence_starts: Option<sentence_starts::Options>,
    #[serde(default)]
    pub sentence_length_baseline: Option<sentence_length::Baseline>,
    #[serde(default)]
    pub sentence_starts_baseline: Option<sentence_starts::Baseline>,
}
impl Default for SentenceReportsRequest {
    fn default() -> Self {
        Self {
            sentence_length: Some(sentence_length::Options::new()),
            sentence_starts: Some(sentence_starts::Options::new()),
            sentence_length_baseline: None,
            sentence_starts_baseline: None,
        }
    }
}
impl SentenceReportsRequest {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SentenceReports {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentence_length: Option<sentence_length::Report>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sentence_starts: Option<sentence_starts::Report>,
}
pub fn sentence_reports(document: &Document, request: &SentenceReportsRequest) -> SentenceReports {
    SentenceReports {
        sentence_length: request.sentence_length.as_ref().map(|options| {
            sentence_length::report(document, options, request.sentence_length_baseline.as_ref())
        }),
        sentence_starts: request.sentence_starts.as_ref().map(|options| {
            sentence_starts::report(document, options, request.sentence_starts_baseline.as_ref())
        }),
    }
}
