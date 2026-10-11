//! Builtin (code-defined) grammatical rules.
//!
//! A few rules are not expressible in the rule XML format and are implemented
//! directly in Rust instead. This module provides the [`Builtin`] enum with one
//! variant per such rule and [`BuiltinRule`], which pairs a [`Builtin`] with an
//! enabled flag. Builtin rules are stored alongside the XML rules in
//! [`Rules`](crate::rules::Rules).

use crate::types::{Sentence, Suggestion};
use serde::{Deserialize, Serialize};

pub mod en;

/// A rule whose logic is implemented in Rust rather than parsed from rule XML.
///
/// One variant is added per builtin rule; the variant carries the rule's
/// language-specific state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Builtin {
    /// Flags whitespace around commas, full stops, parentheses and quotes.
    CommaParenthesisWhitespace(en::CommaParenthesisWhitespace),
    /// Flags the wrong indefinite article before a word, e.g. `"a apple"`.
    AVsAn(en::AVsAn),
    /// Flags a word that is immediately repeated, e.g. `"the the"`.
    WordRepeat(en::WordRepeat),
    /// Flags a sentence that does not start with an uppercase letter.
    UppercaseSentenceStart(en::UppercaseSentenceStart),
    /// Flags brackets that are never closed within a paragraph.
    UnpairedBrackets(en::UnpairedBrackets),
    /// Flags quotation marks that are never paired within a paragraph.
    UnpairedQuotes(en::UnpairedQuotes),
}

impl Builtin {
    /// The identifier of the corresponding LanguageTool rule, e.g.
    /// `"ENGLISH_WORD_REPEAT_RULE"`.
    pub fn id(&self) -> &str {
        match self {
            Builtin::CommaParenthesisWhitespace(rule) => rule.id(),
            Builtin::AVsAn(rule) => rule.id(),
            Builtin::WordRepeat(rule) => rule.id(),
            Builtin::UppercaseSentenceStart(rule) => rule.id(),
            Builtin::UnpairedBrackets(rule) => rule.id(),
            Builtin::UnpairedQuotes(rule) => rule.id(),
        }
    }

    /// Runs this rule on one sentence and returns the suggestions it produces.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        self.check_text(std::slice::from_ref(sentence))
    }

    /// Runs this rule on consecutive sentences of one paragraph. Rules that need context beyond one sentence
    /// (e.g. quotes spanning sentences) see all of them; sentence-level rules check each one separately.
    pub fn check_text(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        match self {
            Builtin::CommaParenthesisWhitespace(rule) => rule.check_sentences(sentences),
            Builtin::AVsAn(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::WordRepeat(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::UppercaseSentenceStart(rule) => rule.check(sentences),
            Builtin::UnpairedBrackets(rule) => rule.check(sentences),
            Builtin::UnpairedQuotes(rule) => rule.check(sentences),
        }
    }
}

/// A [`Builtin`] rule together with whether it is enabled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuiltinRule {
    builtin: Builtin,
    enabled: bool,
}

impl BuiltinRule {
    /// Creates a new builtin rule. Rules are enabled by default.
    pub fn new(builtin: Builtin) -> Self {
        BuiltinRule {
            builtin,
            enabled: true,
        }
    }

    /// Gets the identifier of this rule (the LanguageTool rule id).
    pub fn id(&self) -> &str {
        self.builtin.id()
    }

    /// Gets the underlying builtin rule.
    pub fn builtin(&self) -> &Builtin {
        &self.builtin
    }

    /// Returns whether this rule is enabled.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Enables this rule.
    pub fn enable(&mut self) {
        self.enabled = true;
    }

    /// Disables this rule.
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}
