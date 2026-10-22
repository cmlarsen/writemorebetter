//! Flags every use of an adjective, noun or verb that makes up too large a
//! share of all the adjectives, nouns or verbs of a text.
//!
//! The behaviour follows LanguageTool's `TOO_OFTEN_USED_ADJECTIVE_EN`,
//! `TOO_OFTEN_USED_NOUN_EN` and `TOO_OFTEN_USED_VERB_EN` rules, which are off
//! by default.

use crate::types::{DefaultHashMap, Sentence, Span, Suggestion, Token};
use crate::Error;
use serde::{Deserialize, Serialize};

const MIN_WORD_COUNT: usize = 100;

const DEFAULT_MIN_PERCENT: usize = 5;

/// The range LanguageTool's `RuleOption` allows for the minimum percentage.
const MIN_PERCENT_RANGE: std::ops::RangeInclusive<usize> = 1..=100;

/// The kind of word a [`TooOftenUsed`] rule counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WordClass {
    /// Adjectives (`JJ*`), `TOO_OFTEN_USED_ADJECTIVE_EN`.
    Adjective,
    /// Common nouns (`NN*`), `TOO_OFTEN_USED_NOUN_EN`.
    Noun,
    /// Verbs other than "be", "have" and "do" (`VB*`), `TOO_OFTEN_USED_VERB_EN`.
    Verb,
}

impl WordClass {
    fn id(self) -> &'static str {
        match self {
            WordClass::Adjective => "TOO_OFTEN_USED_ADJECTIVE_EN",
            WordClass::Noun => "TOO_OFTEN_USED_NOUN_EN",
            WordClass::Verb => "TOO_OFTEN_USED_VERB_EN",
        }
    }

    fn name(self) -> &'static str {
        match self {
            WordClass::Adjective => "adjective",
            WordClass::Noun => "noun",
            WordClass::Verb => "verb",
        }
    }

    fn pos_prefix(self) -> &'static str {
        match self {
            WordClass::Adjective => "JJ",
            WordClass::Noun => "NN",
            WordClass::Verb => "VB",
        }
    }

    fn is_exception(self, token: &Token) -> bool {
        let has_pos = |prefix: &str| {
            token
                .word()
                .tags()
                .iter()
                .any(|reading| reading.pos().as_str().starts_with(prefix))
        };

        match self {
            WordClass::Adjective => ["RB", "IN", "CD", "DT", "NN"].iter().any(|x| has_pos(x)),
            WordClass::Noun => ["NNP", "IN", "JJ", "RB", "VB"].iter().any(|x| has_pos(x)),
            WordClass::Verb => {
                token
                    .word()
                    .tags()
                    .iter()
                    .any(|reading| matches!(reading.lemma().as_str(), "be" | "have" | "do"))
                    || has_pos("IN")
                    || has_pos("NN")
            }
        }
    }
}

/// Flags each use of a word that makes up at least 5% (see
/// [`TooOftenUsed::set_min_percent`]) of all the words of its class in a text
/// with at least 100 such words.
///
/// # Scope
///
/// The share is computed over the whole text of one
/// [`Rules::suggest`](crate::Rules::suggest) call, so the matches are only
/// known once the text has been read to its end. An app that checks a
/// document piece by piece gets the shares of each piece.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TooOftenUsed {
    class: WordClass,
    min_percent: usize,
    without_direct_speech: bool,
}

/// What [`TooOftenUsed`] collects from the sentences of one text.
#[derive(Debug, Default)]
pub(crate) struct TooOftenUsedState {
    is_direct_speech: bool,
    lemmas: DefaultHashMap<String, usize>,
    counts: Vec<usize>,
    words: Vec<(usize, Span)>,
}

impl TooOftenUsed {
    pub(crate) fn new(class: WordClass) -> Self {
        TooOftenUsed {
            class,
            min_percent: DEFAULT_MIN_PERCENT,
            without_direct_speech: false,
        }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        self.class.id()
    }

    /// The share of all the words of its class, in percent, from which a word
    /// is reported. The default is 5.
    pub fn min_percent(&self) -> usize {
        self.min_percent
    }

    /// Sets the share of all the words of its class, in percent, from which a
    /// word is reported. This is the option LanguageTool's rule offers in its
    /// user configuration, which allows 1 to 100.
    ///
    /// # Errors
    /// [`Error::OptionOutOfRange`] if `percent` is outside 1..=100; the rule
    /// keeps its previous value.
    pub fn set_min_percent(&mut self, percent: usize) -> Result<(), Error> {
        Self::check_min_percent(percent)?;
        self.min_percent = percent;
        Ok(())
    }

    /// Checks that `percent` is a minimum percentage [`TooOftenUsed::set_min_percent`] accepts.
    pub(crate) fn check_min_percent(percent: usize) -> Result<(), Error> {
        if MIN_PERCENT_RANGE.contains(&percent) {
            Ok(())
        } else {
            Err(Error::OptionOutOfRange {
                option: "minimum percentage",
                value: percent,
                min: *MIN_PERCENT_RANGE.start(),
                max: *MIN_PERCENT_RANGE.end(),
            })
        }
    }

    /// Stops counting and reporting words in direct speech, i.e. between an
    /// opening quote that is followed directly by a word and a closing quote
    /// that follows directly after one. This is LanguageTool's
    /// `setWithoutDirectSpeech` option.
    pub fn skip_direct_speech(&mut self) {
        self.without_direct_speech = true;
    }

    /// Counts the words of one sentence of a text.
    pub(crate) fn count(&self, sentence: &Sentence, state: &mut TooOftenUsedState) {
        let tokens = sentence.tokens();

        for (index, token) in tokens.iter().enumerate() {
            let text = token.word().text().as_str();

            if self.without_direct_speech
                && !state.is_direct_speech
                && matches!(text, "\"" | "“" | "„" | "»" | "«")
                && tokens
                    .get(index + 1)
                    .is_some_and(|next| !next.has_space_before())
            {
                state.is_direct_speech = true;
            } else if self.without_direct_speech
                && state.is_direct_speech
                && matches!(text, "\"" | "“" | "”" | "»" | "«")
                && index > 0
                && !token.has_space_before()
            {
                state.is_direct_speech = false;
            } else if !state.is_direct_speech && !is_non_word(text) && self.is_counted(token) {
                if let Some(lemma) = self.lemma(token) {
                    let next = state.counts.len();
                    let id = *state.lemmas.entry(lemma.to_owned()).or_insert(next);
                    if id == next {
                        state.counts.push(0);
                    }
                    state.counts[id] += 1;
                    state.words.push((id, token.span().clone()));
                }
            }
        }
    }

    /// Reports the counted words that are used too often, once the whole text
    /// has been counted.
    pub(crate) fn finish(&self, state: &TooOftenUsedState) -> Vec<Suggestion> {
        let total: usize = state.counts.iter().sum();
        if total < MIN_WORD_COUNT {
            return Vec::new();
        }

        let too_often: Vec<bool> = state
            .counts
            .iter()
            .map(|count| (*count as f64 * 100.0 / total as f64) as usize >= self.min_percent)
            .collect();

        let name = self.class.name();
        let message = format!(
            "The {} is used more than {}% times of all {}s. It may be better to replace it with a \
             synonym.",
            name, self.min_percent, name
        );

        state
            .words
            .iter()
            .filter(|(id, _)| too_often[*id])
            .map(|(_, span)| {
                Suggestion::new(
                    format!("BUILTIN/{}", self.id()),
                    message.clone(),
                    span.clone(),
                    Vec::new(),
                )
            })
            .collect()
    }

    fn is_counted(&self, token: &Token) -> bool {
        let prefix = self.class.pos_prefix();
        token
            .word()
            .tags()
            .iter()
            .any(|reading| reading.pos().as_str().starts_with(prefix))
            && !self.class.is_exception(token)
    }

    fn lemma<'a>(&self, token: &'a Token) -> Option<&'a str> {
        let prefix = self.class.pos_prefix();
        token
            .word()
            .tags()
            .iter()
            .find(|reading| reading.pos().as_str().starts_with(prefix))
            .map(|reading| reading.lemma().as_str())
            .filter(|lemma| !lemma.is_empty())
    }
}

/// Whether `text` is a single punctuation character that LanguageTool does not
/// count as a word.
fn is_non_word(text: &str) -> bool {
    let mut chars = text.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => ".?!…:;,~’'\"„“”»«‚‘›‹()[]-–—*×∗·+÷/=".contains(c),
        _ => false,
    }
}
