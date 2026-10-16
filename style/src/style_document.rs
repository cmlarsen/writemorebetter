use std::sync::Arc;

use nlprule::Tokenizer;
use serde::{de::DeserializeOwned, Serialize};

use crate::reports::{
    self, cliches, dialogue_tags, echoes, repeated_phrases, sentence_length, sentence_starts,
    DocumentStats, StyleResources,
};
use crate::{analyze_document, Document, DocumentOptions, StyleError};

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
    json: Option<&str>,
    bundled: impl FnOnce() -> Option<T>,
) -> Result<Option<T>, StyleError> {
    match json {
        None => Ok(bundled()),
        Some(json) => serde_json::from_str(json).map_err(StyleError::options),
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

    pub fn echoes(&self, json: Option<&str>) -> Result<String, StyleError> {
        encode(echoes::report(&self.document, &options(json)?)?)
    }

    pub fn repeated_phrases(&self, json: Option<&str>) -> Result<String, StyleError> {
        encode(repeated_phrases::report(&self.document, &options(json)?)?)
    }

    pub fn cliches(&self, json: Option<&str>) -> Result<String, StyleError> {
        let options = options(json)?;
        encode(cliches::report(
            &self.document,
            self.resources.cliches(&self.tokenizer),
            &options,
        )?)
    }

    pub fn dialogue_tags(
        &self,
        json: Option<&str>,
        baseline_json: Option<&str>,
    ) -> Result<String, StyleError> {
        let baseline = baseline(baseline_json, dialogue_tags::DialogueTagBaseline::bundled)?;
        encode(dialogue_tags::report(
            &self.document,
            &options(json)?,
            baseline.as_ref(),
        )?)
    }

    pub fn sentence_length(
        &self,
        json: Option<&str>,
        baseline_json: Option<&str>,
    ) -> Result<String, StyleError> {
        let baseline = baseline(
            baseline_json,
            sentence_length::SentenceLengthBaseline::bundled,
        )?;
        encode(sentence_length::report(
            &self.document,
            &options(json)?,
            baseline.as_ref(),
        )?)
    }

    pub fn sentence_starts(
        &self,
        json: Option<&str>,
        baseline_json: Option<&str>,
    ) -> Result<String, StyleError> {
        let baseline = baseline(
            baseline_json,
            sentence_starts::SentenceStartsBaseline::bundled,
        )?;
        encode(sentence_starts::report(
            &self.document,
            &options(json)?,
            baseline.as_ref(),
        )?)
    }
}

fn encode(value: impl Serialize) -> Result<String, StyleError> {
    Ok(reports::to_utf16_json(&value))
}
