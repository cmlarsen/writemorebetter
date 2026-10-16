use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::echoes::{fold, listed};
use crate::error::StyleError;
use crate::{Document, Span, Token};

pub(crate) const REPORT: &str = "overusedWords";

pub const FILLER_WORDS: [&str; 9] = [
    "actually", "just", "quite", "really", "simply", "somehow", "suddenly", "that", "very",
];

const CONTENT_TAGS: [&str; 4] = ["NN", "VB", "JJ", "RB"];

pub const EXCLUDED_LEMMAS: [&str; 9] = [
    "around", "away", "back", "down", "like", "off", "out", "round", "up",
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsOptions {
    #[serde(alias = "minimum_ratio")]
    pub minimum_ratio: f64,
    #[serde(alias = "minimum_count")]
    pub minimum_count: usize,
    #[serde(alias = "minimum_reference_per_million")]
    pub minimum_reference_per_million: f64,
    #[serde(alias = "include_nouns")]
    pub include_nouns: bool,
}

impl Default for OverusedWordsOptions {
    fn default() -> Self {
        Self {
            minimum_ratio: 4.0,
            minimum_count: 5,
            minimum_reference_per_million: 10.0,
            include_nouns: false,
        }
    }
}

impl OverusedWordsOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_ratio(mut self, ratio: f64) -> Self {
        self.minimum_ratio = ratio;
        self
    }
    pub fn with_minimum_count(mut self, count: usize) -> Self {
        self.minimum_count = count;
        self
    }
    pub fn with_minimum_reference_per_million(mut self, rate: f64) -> Self {
        self.minimum_reference_per_million = rate;
        self
    }
    pub fn with_include_nouns(mut self, include: bool) -> Self {
        self.include_nouns = include;
        self
    }

    fn validate(&self) -> Result<(), StyleError> {
        if !(self.minimum_ratio.is_finite() && self.minimum_ratio > 0.0) {
            return Err(StyleError::options(
                "minimumRatio must be a positive number",
            ));
        }
        if !(self.minimum_reference_per_million.is_finite()
            && self.minimum_reference_per_million >= 0.0)
        {
            return Err(StyleError::options(
                "minimumReferencePerMillion must be a finite, non-negative rate",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsBaseline {
    pub lemmas: BTreeMap<String, f64>,
    pub fillers: BTreeMap<String, f64>,
}

impl OverusedWordsBaseline {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_lemma(mut self, lemma: &str, per_million: f64) -> Self {
        self.lemmas.insert(lemma.to_owned(), per_million);
        self
    }

    pub fn with_filler(mut self, word: &str, per_million: f64) -> Self {
        self.fillers.insert(word.to_owned(), per_million);
        self
    }

    pub fn bundled() -> Option<&'static Self> {
        static BUNDLED: OnceLock<OverusedWordsBaseline> = OnceLock::new();
        Some(BUNDLED.get_or_init(|| {
            super::bundled_baseline(include_str!("../../data/baselines/overused-words.json"))
        }))
    }

    pub fn validate(&self) -> Result<(), StyleError> {
        for (key, rate) in self.lemmas.iter().chain(&self.fillers) {
            if !(rate.is_finite() && *rate >= 0.0) {
                return Err(StyleError::baseline(
                    REPORT,
                    format!("{key} must have a finite, non-negative rate per million, got {rate}"),
                ));
            }
        }
        if let Some(word) = self
            .fillers
            .keys()
            .find(|w| !FILLER_WORDS.contains(&w.as_str()))
        {
            return Err(StyleError::baseline(
                REPORT,
                format!("{word} is not a filler word; use one of {FILLER_WORDS:?}"),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWord {
    pub lemma: String,
    pub count: usize,
    pub per_ten_thousand_words: f64,
    pub reference_per_ten_thousand_words: f64,
    pub ratio: f64,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsFiller {
    pub word: String,
    pub count: usize,
    pub per_ten_thousand_words: f64,
    pub reference_per_ten_thousand_words: Option<f64>,
    pub ratio: Option<f64>,
    pub spans: Vec<Span>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct OverusedWordsReport {
    pub words: usize,
    pub overused: Vec<OverusedWord>,
    pub fillers: Vec<OverusedWordsFiller>,
}

fn filler(token: &Token) -> Option<&'static str> {
    let word = fold(&token.text);
    let word = FILLER_WORDS.iter().find(|filler| **filler == word)?;
    let qualifies = if *word == "that" {
        token.chunks.iter().any(|chunk| chunk == "B-SBAR")
    } else {
        token.readings.iter().any(|r| r.pos.starts_with("RB"))
    };
    qualifies.then_some(*word)
}

fn capitalized(token: &Token) -> bool {
    token.text.chars().next().is_some_and(char::is_uppercase)
}

fn proper(token: &Token, initial: bool, names: &HashSet<&str>) -> bool {
    capitalized(token)
        && (!initial
            || names.contains(token.text.as_str())
            || token
                .readings
                .iter()
                .any(|r| matches!(r.pos.as_str(), "NNP" | "NNPS")))
}

fn words_with_position(document: &Document) -> impl Iterator<Item = (&Token, bool)> {
    document.sentences().flat_map(|sentence| {
        let mut initial = true;
        sentence.tokens.iter().filter_map(move |token| {
            if token.is_whitespace {
                return None;
            }
            let opens = initial;
            initial =
                token.is_punctuation && (initial || token.text.contains(['“', '‘', '"', ':', '(']));
            (token.is_word && !token.is_skipped).then_some((token, opens))
        })
    })
}

fn excluded(lemma: &str) -> bool {
    !lemma.chars().any(char::is_alphabetic)
        || lemma.contains(char::is_whitespace)
        || listed(lemma)
        || FILLER_WORDS.contains(&lemma)
        || EXCLUDED_LEMMAS.contains(&lemma)
}

fn content_lemma(token: &Token, baseline: &OverusedWordsBaseline) -> Option<(String, bool)> {
    if token.readings.iter().any(|r| r.pos.starts_with("CD")) {
        return None;
    }
    let content: Vec<(String, &str)> = token
        .readings
        .iter()
        .filter_map(|r| {
            if matches!(r.pos.as_str(), "NNP" | "NNPS") {
                return None;
            }
            let tag = CONTENT_TAGS.iter().find(|tag| r.pos.starts_with(**tag))?;
            Some((fold(&r.lemma), *tag))
        })
        .filter(|(lemma, _)| !excluded(lemma))
        .collect();
    let chunk = |kind: &str| token.chunks.iter().any(|c| c.contains(kind));
    let family = |tag: &str| content.iter().any(|(_, t)| *t == tag);
    let content: Vec<(String, &str)> = if chunk("-VP") && family("VB") {
        content.into_iter().filter(|(_, t)| *t == "VB").collect()
    } else if chunk("E-NP") && family("NN") {
        content.into_iter().filter(|(_, t)| *t == "NN").collect()
    } else {
        content
    };
    let rate = |lemma: &str| baseline.lemmas.get(lemma).copied().unwrap_or(0.0);
    let lemma = content
        .iter()
        .map(|(lemma, _)| lemma)
        .max_by(|a, b| rate(a).total_cmp(&rate(b)).then_with(|| b.cmp(a)))?
        .clone();
    let noun = content
        .iter()
        .filter(|(other, _)| *other == lemma)
        .all(|(_, tag)| *tag == "NN");
    Some((lemma, noun))
}

fn per_ten_thousand(count: usize, words: usize) -> f64 {
    if words == 0 {
        0.0
    } else {
        count as f64 * 10_000.0 / words as f64
    }
}

pub fn report(
    document: &Document,
    options: &OverusedWordsOptions,
    baseline: Option<&OverusedWordsBaseline>,
) -> Result<OverusedWordsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    options.validate()?;
    if let Some(baseline) = baseline {
        baseline.validate()?;
    }
    let words = document.word_count;
    let mut lemmas: HashMap<String, Vec<Span>> = HashMap::new();
    let mut fillers: BTreeMap<&str, Vec<Span>> = BTreeMap::new();
    let names: HashSet<&str> = words_with_position(document)
        .filter(|(token, initial)| !initial && capitalized(token))
        .map(|(token, _)| token.text.as_str())
        .collect();
    for (token, initial) in words_with_position(document) {
        if let Some(word) = filler(token) {
            fillers.entry(word).or_default().push(token.span.clone());
        } else if let Some(baseline) = baseline.filter(|_| !proper(token, initial, &names)) {
            match content_lemma(token, baseline) {
                Some((lemma, noun)) if options.include_nouns || !noun => {
                    lemmas.entry(lemma).or_default().push(token.span.clone());
                }
                _ => {}
            }
        }
    }
    let mut overused: Vec<OverusedWord> = baseline
        .map(|baseline| {
            lemmas.into_iter().filter_map(|(lemma, spans)| {
                let reference = *baseline.lemmas.get(&lemma)?;
                if spans.len() < options.minimum_count.max(1)
                    || reference <= 0.0
                    || reference < options.minimum_reference_per_million
                {
                    return None;
                }
                let rate = per_ten_thousand(spans.len(), words);
                let reference = reference / 100.0;
                let ratio = rate / reference;
                (ratio >= options.minimum_ratio).then_some(OverusedWord {
                    lemma,
                    count: spans.len(),
                    per_ten_thousand_words: rate,
                    reference_per_ten_thousand_words: reference,
                    ratio,
                    spans,
                })
            })
        })
        .into_iter()
        .flatten()
        .collect();
    overused.sort_by(|a, b| {
        b.ratio
            .total_cmp(&a.ratio)
            .then(b.count.cmp(&a.count))
            .then_with(|| a.lemma.cmp(&b.lemma))
    });
    let mut fillers: Vec<OverusedWordsFiller> = fillers
        .into_iter()
        .map(|(word, spans)| {
            let rate = per_ten_thousand(spans.len(), words);
            let reference = baseline
                .and_then(|b| b.fillers.get(word))
                .map(|per_million| per_million / 100.0);
            OverusedWordsFiller {
                word: word.to_owned(),
                count: spans.len(),
                per_ten_thousand_words: rate,
                reference_per_ten_thousand_words: reference,
                ratio: reference.filter(|r| *r > 0.0).map(|r| rate / r),
                spans,
            }
        })
        .collect();
    fillers.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.word.cmp(&b.word)));
    Ok(OverusedWordsReport {
        words,
        overused,
        fillers,
    })
}
