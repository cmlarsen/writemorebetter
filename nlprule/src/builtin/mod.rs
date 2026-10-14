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

/// Whether a suggestion from `source` is dropped when it overlaps another
/// suggestion. LanguageTool tags `TOO_LONG_SENTENCE` and `TOO_LONG_PARAGRAPH`
/// as picky, which gives them the lowest priority when matches overlap.
pub(crate) fn yields_to_overlaps(source: &str) -> bool {
    matches!(
        source,
        "BUILTIN/TOO_LONG_SENTENCE" | "BUILTIN/TOO_LONG_PARAGRAPH"
    )
}

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
    /// Flags words or phrases from a replacement table, e.g. `"crisps"` in
    /// American English.
    SimpleReplace(en::SimpleReplace),
    /// Flags a word that is easily confused with another one, based on context.
    WrongWordInContext(en::WrongWordInContext),
    /// Flags a sentence that does not start with an uppercase letter.
    UppercaseSentenceStart(en::UppercaseSentenceStart),
    /// Flags brackets that are never closed within a paragraph.
    UnpairedBrackets(en::UnpairedBrackets),
    /// Flags quotation marks that are never paired within a paragraph.
    UnpairedQuotes(en::UnpairedQuotes),
    /// Flags a word whose spelling variant was already used earlier in the text.
    WordCoherency(en::WordCoherency),
    /// Flags a sentence with more than a maximum number of words.
    LongSentence(en::LongSentence),
    /// Flags a paragraph with more than a maximum number of words.
    LongParagraph(en::LongParagraph),
    /// Flags successive sentences that begin with the same word.
    WordRepeatBeginning(en::WordRepeatBeginning),
}

impl Builtin {
    /// The identifier of the corresponding LanguageTool rule, e.g.
    /// `"ENGLISH_WORD_REPEAT_RULE"`.
    pub fn id(&self) -> &str {
        match self {
            Builtin::CommaParenthesisWhitespace(rule) => rule.id(),
            Builtin::AVsAn(rule) => rule.id(),
            Builtin::WordRepeat(rule) => rule.id(),
            Builtin::SimpleReplace(rule) => rule.id(),
            Builtin::WrongWordInContext(rule) => rule.id(),
            Builtin::UppercaseSentenceStart(rule) => rule.id(),
            Builtin::UnpairedBrackets(rule) => rule.id(),
            Builtin::UnpairedQuotes(rule) => rule.id(),
            Builtin::WordCoherency(rule) => rule.id(),
            Builtin::LongSentence(rule) => rule.id(),
            Builtin::LongParagraph(rule) => rule.id(),
            Builtin::WordRepeatBeginning(rule) => rule.id(),
        }
    }

    /// Runs this rule on one sentence and returns the suggestions it produces.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        self.check_text(std::slice::from_ref(sentence))
    }

    /// Runs this rule on consecutive sentences of one paragraph. Rules that need context beyond one sentence
    /// (e.g. quotes spanning sentences) see all of them; sentence-level rules check each one separately.
    pub fn check_text(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        self.check_text_with_previous(sentences, None)
    }

    /// Like [`Builtin::check_text`], but for a paragraph that follows `previous`,
    /// the last sentence of the preceding paragraph. Only rules that are
    /// text-level in LanguageTool (and therefore look across paragraphs) use it.
    pub fn check_text_with_previous(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
    ) -> Vec<Suggestion> {
        self.check_paragraph(sentences, previous.map_or(&[], std::slice::from_ref), true)
    }

    /// Like [`Builtin::check_text_with_previous`], with `previous` holding the
    /// sentences before the paragraph, oldest first, and `ends_text` telling
    /// whether the paragraph is the end of the text. Rules that compare
    /// successive sentences (`ENGLISH_WORD_REPEAT_BEGINNING_RULE`) use the last
    /// two previous sentences; the others use at most the last one.
    pub fn check_paragraph(
        &self,
        sentences: &[Sentence],
        previous: &[Sentence],
        ends_text: bool,
    ) -> Vec<Suggestion> {
        match self {
            Builtin::CommaParenthesisWhitespace(rule) => rule.check_sentences(sentences),
            Builtin::AVsAn(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::WordRepeat(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::SimpleReplace(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::WrongWordInContext(rule) => rule.check_sentences(sentences),
            Builtin::UppercaseSentenceStart(rule) => {
                rule.check_with_previous(sentences, previous.last())
            }
            Builtin::UnpairedBrackets(rule) => rule.check(sentences),
            Builtin::UnpairedQuotes(rule) => rule.check(sentences),
            Builtin::WordCoherency(rule) => rule.check_text(sentences),
            Builtin::LongSentence(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::LongParagraph(rule) => {
                rule.check_with_previous(sentences, previous, ends_text)
            }
            Builtin::WordRepeatBeginning(rule) => {
                rule.check_with_previous(sentences, previous, ends_text)
            }
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

    /// Creates a new builtin rule that is disabled until enabled, like a
    /// LanguageTool rule that is off by default.
    pub fn new_disabled(builtin: Builtin) -> Self {
        BuiltinRule {
            builtin,
            enabled: false,
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
