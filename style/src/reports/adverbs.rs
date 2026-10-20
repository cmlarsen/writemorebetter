use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::{dialogue_tags, word_split, Split};
use crate::{Document, Mode, Span, StyleError};

// These can have an RB reading, but express frequency/time or are lexical adjectives.
const NON_MANNER: &[&str] = &[
    "daily", "weekly", "monthly", "yearly", "hourly", "nightly", "early", "only", "friendly",
    "holy",
];

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct AdverbsOptions {}

impl AdverbsOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AdverbMatch {
    pub span: Span,
    pub lemma: String,
    pub mode: Mode,
    pub tag_adverb: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AdverbLemma {
    pub lemma: String,
    pub count: usize,
    pub per_thousand_words: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AdverbGroup {
    pub words: usize,
    pub count: usize,
    pub per_thousand_words: f64,
    pub lemmas: Vec<AdverbLemma>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct AdverbsReport {
    pub matches: Vec<AdverbMatch>,
    pub groups: Split<AdverbGroup>,
}

pub fn report(document: &Document, _options: &AdverbsOptions) -> Result<AdverbsReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details("adverbs"));
    }
    let tags = dialogue_tags::report(document, &Default::default(), None)?;
    let tag_spans: BTreeSet<_> = tags
        .tags
        .iter()
        .filter_map(|tag| tag.adverb.as_ref())
        .map(|span| (span.byte.start, span.byte.end))
        .collect();
    let mut matches = Vec::new();
    for sentence in document.sentences() {
        for (index, token) in sentence.tokens.iter().enumerate() {
            if !dialogue_tags::is_adverb(token)
                || NON_MANNER.contains(&token.text.to_lowercase().as_str())
            {
                continue;
            }
            let adjectival = token
                .readings
                .iter()
                .any(|reading| super::sentence_starts::normalize_pos(&reading.pos) == "JJ");
            let next_is_noun = sentence.tokens[index + 1..]
                .iter()
                .find(|next| next.is_word)
                .is_some_and(|next| {
                    next.readings
                        .iter()
                        .any(|reading| super::sentence_starts::normalize_pos(&reading.pos) == "NN")
                });
            if adjectival && next_is_noun {
                continue;
            }
            let reading = token
                .readings
                .iter()
                .find(|reading| reading.pos.starts_with("RB"));
            let Some(reading) = reading else {
                continue;
            };
            matches.push(AdverbMatch {
                span: token.span.clone(),
                lemma: reading.lemma.to_lowercase(),
                mode: document.mode_at(token.span.byte.start),
                tag_adverb: tag_spans.contains(&(token.span.byte.start, token.span.byte.end)),
            });
        }
    }
    let words = word_split(document);
    let groups = Split {
        all: group(&matches, words.all, None),
        narration: group(&matches, words.narration, Some(Mode::Narration)),
        dialogue: group(&matches, words.dialogue, Some(Mode::Dialogue)),
    };
    Ok(AdverbsReport { matches, groups })
}

fn group(matches: &[AdverbMatch], words: usize, mode: Option<Mode>) -> AdverbGroup {
    let mut counts = BTreeMap::<String, usize>::new();
    for item in matches
        .iter()
        .filter(|item| mode.is_none_or(|mode| item.mode == mode))
    {
        *counts.entry(item.lemma.clone()).or_default() += 1;
    }
    let count = counts.values().sum();
    let rate = |count: usize| {
        if words == 0 {
            0.0
        } else {
            1000.0 * count as f64 / words as f64
        }
    };
    AdverbGroup {
        words,
        count,
        per_thousand_words: rate(count),
        lemmas: counts
            .into_iter()
            .map(|(lemma, count)| AdverbLemma {
                lemma,
                count,
                per_thousand_words: rate(count),
            })
            .collect(),
    }
}
