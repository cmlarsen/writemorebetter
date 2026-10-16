use std::sync::Arc;

use nlprule::Tokenizer;
use serde::de::DeserializeOwned;

use crate::reports::{
    cliches, dialogue_tags, echoes, repeated_phrases, sentence_length, sentence_starts,
    DocumentStats, StyleResources,
};
use crate::{analyze_document, Document, DocumentOptions, StyleError};

/// Retained analysis with typed native reports; bindings serialize results with UTF-16 spans.
pub struct StyleDocument {
    document: Document,
    tokenizer: Arc<Tokenizer>,
    resources: Arc<StyleResources>,
}

fn options<T: DeserializeOwned + Default>(json: Option<&str>) -> Result<T, StyleError> {
    json.map(serde_json::from_str)
        .transpose()
        .map(Option::unwrap_or_default)
        .map_err(StyleError::options)
}

fn baseline<T: DeserializeOwned>(
    selector: Option<&str>,
    bundled: impl FnOnce() -> Option<T>,
    report: &str,
) -> Result<Option<T>, StyleError> {
    match selector {
        None | Some("bundled") => Ok(bundled()),
        Some("none") => Ok(None),
        Some(json) => serde_json::from_str::<T>(json)
            .map(Some)
            .map_err(|error| StyleError::baseline(report, error)),
    }
}

impl StyleDocument {
    pub fn new(
        tokenizer: Arc<Tokenizer>,
        resources: Arc<StyleResources>,
        text: &str,
        options: &DocumentOptions,
    ) -> Self {
        Self {
            document: analyze_document(&tokenizer, text, options),
            tokenizer,
            resources,
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn stats(&self) -> DocumentStats {
        DocumentStats::new(&self.document)
    }

    pub fn echoes(&self, json: Option<&str>) -> Result<echoes::EchoReport, StyleError> {
        echoes::report(&self.document, &options(json)?)
    }

    pub fn repeated_phrases(
        &self,
        json: Option<&str>,
    ) -> Result<repeated_phrases::RepeatedPhraseReport, StyleError> {
        repeated_phrases::report(&self.document, &options(json)?)
    }

    pub fn cliches(&self, json: Option<&str>) -> Result<cliches::ClicheReport, StyleError> {
        cliches::report(
            &self.document,
            self.resources.cliches(&self.tokenizer),
            &options(json)?,
        )
    }

    pub fn dialogue_tags(
        &self,
        json: Option<&str>,
        baseline_selector: Option<&str>,
    ) -> Result<dialogue_tags::DialogueTagReport, StyleError> {
        let baseline = baseline(
            baseline_selector,
            dialogue_tags::DialogueTagBaseline::bundled,
            dialogue_tags::REPORT,
        )?;
        dialogue_tags::report(&self.document, &options(json)?, baseline.as_ref())
    }

    pub fn sentence_length(
        &self,
        json: Option<&str>,
        baseline_selector: Option<&str>,
    ) -> Result<sentence_length::SentenceLengthReport, StyleError> {
        let baseline = baseline(
            baseline_selector,
            sentence_length::SentenceLengthBaseline::bundled,
            sentence_length::REPORT,
        )?;
        sentence_length::report(&self.document, &options(json)?, baseline.as_ref())
    }

    pub fn sentence_starts(
        &self,
        json: Option<&str>,
        baseline_selector: Option<&str>,
    ) -> Result<sentence_starts::SentenceStartsReport, StyleError> {
        let baseline = baseline(
            baseline_selector,
            sentence_starts::SentenceStartsBaseline::bundled,
            sentence_starts::REPORT,
        )?;
        sentence_starts::report(&self.document, &options(json)?, baseline.as_ref())
    }
}
