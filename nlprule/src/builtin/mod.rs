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
    /// Flags a word that was already used in a preceding sentence and suggests synonyms.
    RepeatedWords(en::RepeatedWords),
    /// Flags an adjective, noun or verb that makes up too large a share of its kind in a text.
    TooOftenUsed(en::TooOftenUsed),
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
            Builtin::RepeatedWords(rule) => rule.id(),
            Builtin::TooOftenUsed(rule) => rule.id(),
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
    /// The rules that count words across a text (`EN_REPEATEDWORDS`,
    /// `TOO_OFTEN_USED_*`) take `sentences` as the whole text.
    pub fn check_text_with_previous(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
    ) -> Vec<Suggestion> {
        match self {
            Builtin::RepeatedWords(_) | Builtin::TooOftenUsed(_) => {
                let mut state = RuleState::default();
                let mut suggestions: Vec<Suggestion> = sentences
                    .iter()
                    .enumerate()
                    .flat_map(|(i, sentence)| {
                        self.check_in_text(sentence, i + 1 == sentences.len(), &mut state)
                    })
                    .collect();
                suggestions.extend(self.finish_text(&state));
                suggestions
            }
            _ => self.check_paragraph(sentences, previous, true),
        }
    }

    /// Like [`Builtin::check_text_with_previous`], for a paragraph that is
    /// followed by more text unless `ends_text` is set. Only rules that look at
    /// the end of the text (`EMPTY_LINE`) use `ends_text`. The rules that count
    /// words across a text return nothing here: [`Rules::suggest`](crate::Rules::suggest)
    /// runs them on the whole text.
    pub fn check_paragraph(
        &self,
        sentences: &[Sentence],
        previous: Option<&Sentence>,
        ends_text: bool,
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
            Builtin::RepeatedWords(_) | Builtin::TooOftenUsed(_) => Vec::new(),
        }
    }

    /// Runs the rules that look at a whole text on one sentence of it, in text order. `state`
    /// keeps what this rule remembers of the earlier sentences, and `ends_text` tells whether no
    /// sentence follows.
    fn check_in_text(
        &self,
        sentence: &Sentence,
        ends_text: bool,
        state: &mut RuleState,
    ) -> Vec<Suggestion> {
        match self {
            Builtin::RepeatedWords(rule) => {
                rule.check(sentence, ends_text, &mut state.repeated_words)
            }
            Builtin::TooOftenUsed(rule) => {
                rule.count(sentence, &mut state.too_often_used);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The suggestions of the rules that need to read a whole text before they can report.
    fn finish_text(&self, state: &RuleState) -> Vec<Suggestion> {
        match self {
            Builtin::TooOftenUsed(rule) => rule.finish(&state.too_often_used),
            _ => Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct RuleState {
    repeated_words: en::RepeatedWordsState,
    too_often_used: en::TooOftenUsedState,
}

/// What the builtins that count words across a text (`EN_REPEATEDWORDS`, `TOO_OFTEN_USED_*`)
/// remember during one check. It starts empty for every text.
#[derive(Debug, Default)]
pub(crate) struct TextState {
    rules: Vec<RuleState>,
}

impl TextState {
    /// Checks the next sentence of the text with every enabled rule of `rules`.
    pub(crate) fn check_sentence(
        &mut self,
        rules: &[BuiltinRule],
        sentence: &Sentence,
        ends_text: bool,
    ) -> Vec<Suggestion> {
        self.rules.resize_with(rules.len(), RuleState::default);
        rules
            .iter()
            .zip(self.rules.iter_mut())
            .filter(|(rule, _)| rule.enabled())
            .flat_map(|(rule, state)| rule.builtin().check_in_text(sentence, ends_text, state))
            .collect()
    }

    /// The suggestions that are only known once every sentence of the text was checked.
    pub(crate) fn finish(&self, rules: &[BuiltinRule]) -> Vec<Suggestion> {
        rules
            .iter()
            .zip(self.rules.iter())
            .filter(|(rule, _)| rule.enabled())
            .flat_map(|(rule, state)| rule.builtin().finish_text(state))
            .collect()
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
    /// The cases are `INDENTATION` of `WHITESPACE_RULE`, see
    /// [`en::MultipleWhitespace::skip_indentation`], and `DIRECT_SPEECH` of the
    /// `TOO_OFTEN_USED_*` rules, see [`en::TooOftenUsed::skip_direct_speech`].
    pub fn disable_case(&mut self, case: &str) -> bool {
        match (&mut self.builtin, case) {
            (Builtin::MultipleWhitespace(rule), "INDENTATION") => {
                rule.skip_indentation();
                true
            }
            (Builtin::TooOftenUsed(rule), "DIRECT_SPEECH") => {
                rule.skip_direct_speech();
                true
            }
            _ => false,
        }
    }
}
