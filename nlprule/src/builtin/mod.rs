//! Builtin (code-defined) grammatical rules.
//!
//! A few rules are not expressible in the rule XML format and are implemented
//! directly in Rust instead. This module provides the [`Builtin`] enum with one
//! variant per such rule and [`BuiltinRule`], which pairs a [`Builtin`] with an
//! enabled flag. Builtin rules are stored alongside the XML rules in
//! [`Rules`](crate::rules::Rules).

use crate::{
    spell::Speller,
    types::{Sentence, Suggestion},
};
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
    /// Flags two consecutive dots or commas, e.g. `"He left.."`.
    DoublePunctuation(en::DoublePunctuation),
    /// Flags a run of repeated whitespace, e.g. two spaces between words.
    MultipleWhitespace(en::MultipleWhitespace),
    /// Flags a sentence that directly follows the previous one without a space.
    SentenceWhitespace(en::SentenceWhitespace),
    /// Flags more than one empty line between paragraphs.
    EmptyLine(en::EmptyLine),
    /// Flags a multi-word proper noun with the wrong capitalization, e.g.
    /// `"Harry potter"`.
    SpecificCase(en::SpecificCase),
    /// Flags a hyphenated compound written with a dash, e.g. `"T—shirt"`.
    Dash(en::Dash),
    /// Flags a compound written as separate words, e.g. `"part time"`.
    Compounds(en::Compounds),
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
            Builtin::DoublePunctuation(rule) => rule.id(),
            Builtin::MultipleWhitespace(rule) => rule.id(),
            Builtin::SentenceWhitespace(rule) => rule.id(),
            Builtin::EmptyLine(rule) => rule.id(),
            Builtin::SpecificCase(rule) => rule.id(),
            Builtin::Dash(rule) => rule.id(),
            Builtin::Compounds(rule) => rule.id(),
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
        self.check_paragraph(sentences, previous, true, None)
    }

    /// Like [`Builtin::check_text_with_previous`], for a paragraph that is
    /// followed by more text unless `ends_text` is set. Only rules that look at
    /// the end of the text (`EMPTY_LINE`) use `ends_text`. Rules that check
    /// their suggestions against the speller (`EN_COMPOUNDS`) use `speller`;
    /// without one they keep every suggestion.
    pub fn check_paragraph(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
        ends_text: bool,
        speller: Option<&Speller>,
    ) -> Vec<Suggestion> {
        match self {
            Builtin::CommaParenthesisWhitespace(rule) => rule.check_sentences(sentences),
            Builtin::AVsAn(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::WordRepeat(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::SimpleReplace(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::WrongWordInContext(rule) => rule.check_sentences(sentences),
            Builtin::UppercaseSentenceStart(rule) => rule.check_with_previous(sentences, previous),
            Builtin::UnpairedBrackets(rule) => rule.check(sentences),
            Builtin::UnpairedQuotes(rule) => rule.check(sentences),
            Builtin::WordCoherency(rule) => rule.check_text(sentences),
            Builtin::DoublePunctuation(rule) => {
                sentences.iter().flat_map(|x| rule.check(x)).collect()
            }
            Builtin::MultipleWhitespace(rule) => rule.check_with_previous(sentences, previous),
            Builtin::SentenceWhitespace(rule) => rule.check_with_previous(sentences, previous),
            Builtin::EmptyLine(rule) => rule.check_text(sentences, ends_text),
            Builtin::SpecificCase(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::Dash(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::Compounds(rule) => sentences
                .iter()
                .flat_map(|x| rule.check(x, speller))
                .collect(),
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

    /// Stops this rule from reporting one of its cases, named `case`, while it
    /// keeps reporting the others. Returns whether the rule has such a case.
    /// The only case so far is `INDENTATION` of `WHITESPACE_RULE`, see
    /// [`en::MultipleWhitespace::skip_indentation`].
    pub fn disable_case(&mut self, case: &str) -> bool {
        match (&mut self.builtin, case) {
            (Builtin::MultipleWhitespace(rule), "INDENTATION") => {
                rule.skip_indentation();
                true
            }
            _ => false,
        }
    }
}
