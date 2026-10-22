//! The voices report (#353): the document's resolved non-narration spans and their counts. Opt-in
//! and informational; it lists what the voice layer decided so a caller can check it and lenses
//! can rely on it. See docs/STYLE-VOICES.md.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{Document, StyleError, Voice, VoiceSpan};

/// The report name in errors and bindings. No option or input of this report can fail yet.
#[allow(dead_code)]
pub(crate) const REPORT: &str = "voices";

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct VoicesOptions {}

impl VoicesOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VoiceCount {
    pub spans: usize,
    pub words: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct VoicesReport {
    pub spans: Vec<VoiceSpan>,
    /// Spans and words per non-narration voice; a voice with no span is absent.
    pub counts: BTreeMap<Voice, VoiceCount>,
    /// Words outside every voice span.
    pub narration_words: usize,
    /// Sentences whose first word is narration, the voice counterpart of narration mode.
    pub narration_sentences: usize,
    pub sentences: usize,
    pub ignored_marked_spans: Vec<usize>,
}

/// Works on compact documents too: voices are resolved before readings are dropped.
pub fn report(document: &Document, _options: &VoicesOptions) -> Result<VoicesReport, StyleError> {
    let mut counts: BTreeMap<Voice, VoiceCount> = BTreeMap::new();
    for span in &document.voices {
        counts.entry(span.voice).or_default().spans += 1;
    }
    let mut narration_words = 0;
    for token in document.words() {
        match document.voice_at(token.span.byte.start) {
            Voice::Narration => narration_words += 1,
            voice => counts.entry(voice).or_default().words += 1,
        }
    }
    let mut sentences = 0;
    let mut narration_sentences = 0;
    for sentence in document.sentences() {
        sentences += 1;
        narration_sentences += usize::from(document.sentence_is_narration(sentence));
    }
    Ok(VoicesReport {
        spans: document.voices.clone(),
        counts,
        narration_words,
        narration_sentences,
        sentences,
        ignored_marked_spans: document.ignored_marked_spans.clone(),
    })
}
