use std::sync::{Arc, Mutex};

use nlprule::Tokenizer;
use serde::de::DeserializeOwned;

use crate::reports::{
    cliches, dialogue_tags, diction, echoes, overused_words, pacing, repeated_phrases,
    sensory_balance, sentence_length, sentence_starts, DocumentStats, StyleResources,
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

fn parsed_table(text: &str) -> Result<Arc<diction::ConcretenessTable>, StyleError> {
    static LAST: Mutex<Option<(String, Arc<diction::ConcretenessTable>)>> = Mutex::new(None);
    {
        let last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, table)) = last.as_ref().filter(|(cached, _)| cached == text) {
            return Ok(table.clone());
        }
    }
    let table = Arc::new(diction::ConcretenessTable::parse(text)?);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *last = Some((text.to_owned(), table.clone()));
    Ok(table)
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

    pub fn transitions(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::transitions::TransitionsReport, StyleError> {
        crate::reports::transitions::report(&self.document, &options(json)?)
    }

    pub fn pronoun_starts(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::pronoun_starts::PronounStartsReport, StyleError> {
        crate::reports::pronoun_starts::report(&self.document, &options(json)?)
    }

    pub fn adverbs(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::adverbs::AdverbsReport, StyleError> {
        crate::reports::adverbs::report(&self.document, &options(json)?)
    }

    pub fn sensory_balance(
        &self,
        json: Option<&str>,
    ) -> Result<sensory_balance::SensoryBalanceReport, StyleError> {
        sensory_balance::report(
            &self.document,
            sensory_balance::PerceptualNorms::bundled(),
            &options(json)?,
        )
    }

    pub fn pacing(&self, json: Option<&str>) -> Result<pacing::PacingReport, StyleError> {
        pacing::report(&self.document, &options(json)?)
    }

    pub fn diction(
        &self,
        json: Option<&str>,
        table: Option<&str>,
    ) -> Result<diction::DictionReport, StyleError> {
        let options = options(json)?;
        match table {
            Some(text) => diction::report(&self.document, &*parsed_table(text)?, &options),
            None => diction::report(
                &self.document,
                diction::ConcretenessTable::bundled(),
                &options,
            ),
        }
    }

    pub fn overused_words(
        &self,
        json: Option<&str>,
        baseline_selector: Option<&str>,
    ) -> Result<overused_words::OverusedWordsReport, StyleError> {
        let custom = baseline(baseline_selector, || None, overused_words::REPORT)?;
        let baseline = match baseline_selector {
            None | Some("bundled") => overused_words::OverusedWordsBaseline::bundled(),
            _ => custom.as_ref(),
        };
        overused_words::report(&self.document, &options(json)?, baseline)
    }

    pub fn consistency(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::consistency::ConsistencyReport, StyleError> {
        crate::reports::consistency::report(&self.document, &options(json)?)
    }

    pub fn readability(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::readability::ReadabilityReport, StyleError> {
        crate::reports::readability::report(&self.document, &options(json)?)
    }

    pub fn passive(
        &self,
        json: Option<&str>,
        baseline_selector: Option<&str>,
    ) -> Result<crate::reports::passive::PassiveReport, StyleError> {
        let baseline = match baseline_selector {
            Some("publicStandIn") => {
                Some(crate::reports::passive::PassiveBaseline::public_stand_in())
            }
            _ => baseline(
                baseline_selector,
                crate::reports::passive::PassiveBaseline::bundled,
                crate::reports::passive::REPORT,
            )?,
        };
        crate::reports::passive::report(&self.document, &options(json)?, baseline.as_ref())
    }

    pub fn clarity(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::clarity::ClarityReport, StyleError> {
        crate::reports::clarity::report(&self.document, &options(json)?)
    }

    /// Runs caller-supplied house-style rules (YAML or JSON text); `None` is an error.
    pub fn house_style(
        &self,
        json: Option<&str>,
        rules: Option<&str>,
    ) -> Result<crate::reports::house_style::HouseStyleReport, StyleError> {
        use crate::reports::house_style;
        let options = options(json)?;
        let rules = rules.ok_or_else(|| {
            StyleError::rules(house_style::REPORT, None, "no rules were supplied")
        })?;
        house_style::report(&self.document, &*parsed_rules(rules)?, &options)
    }

    pub fn regional_spelling(
        &self,
        json: Option<&str>,
    ) -> Result<crate::reports::regional_spelling::RegionalSpellingReport, StyleError> {
        crate::reports::regional_spelling::report(&self.document, &options(json)?)
    }
}

fn parsed_rules(
    text: &str,
) -> Result<Arc<crate::reports::house_style::HouseStyleRules>, StyleError> {
    use crate::reports::house_style::HouseStyleRules;
    static LAST: Mutex<Option<(String, Arc<HouseStyleRules>)>> = Mutex::new(None);
    {
        let last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((_, rules)) = last.as_ref().filter(|(cached, _)| cached == text) {
            return Ok(rules.clone());
        }
    }
    let rules = Arc::new(HouseStyleRules::parse(text)?);
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    *last = Some((text.to_owned(), rules.clone()));
    Ok(rules)
}
