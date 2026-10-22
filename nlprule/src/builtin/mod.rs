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
    /// Flags two consecutive dots or commas, e.g. `"He left.."`.
    DoublePunctuation(en::DoublePunctuation),
    /// Flags a run of repeated whitespace, e.g. two spaces between words.
    MultipleWhitespace(en::MultipleWhitespace),
    /// Flags a sentence that directly follows the previous one without a space.
    SentenceWhitespace(en::SentenceWhitespace),
    /// Flags more than one empty line between paragraphs.
    EmptyLine(en::EmptyLine),
    /// Flags a sentence with more than a maximum number of words.
    LongSentence(en::LongSentence),
    /// Flags a paragraph with more than a maximum number of words.
    LongParagraph(en::LongParagraph),
    /// Flags successive sentences that begin with the same word.
    WordRepeatBeginning(en::WordRepeatBeginning),
    /// Flags a word that was already used in a preceding sentence and suggests synonyms.
    RepeatedWords(en::RepeatedWords),
    /// Flags an adjective, noun or verb that makes up too large a share of its kind in a text.
    TooOftenUsed(en::TooOftenUsed),
    /// Flags a multi-word proper noun with the wrong capitalization, e.g.
    /// `"Harry potter"`.
    SpecificCase(en::SpecificCase),
    /// Flags a hyphenated compound written with a dash, e.g. `"T—shirt"`.
    Dash(en::Dash),
    /// Flags a compound written as separate words, e.g. `"part time"`.
    Compounds(en::Compounds),
    /// A narrow English word confusion adapted from Harper.
    HarperConfusion(en::HarperConfusion),
    /// Flags every apostrophe token in a text that mixes `'` and `’`. A whole-text rule: see
    /// [`crate::document`].
    ConsistentApostrophes(en::ConsistentApostrophes),
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
            Builtin::LongSentence(rule) => rule.id(),
            Builtin::LongParagraph(rule) => rule.id(),
            Builtin::WordRepeatBeginning(rule) => rule.id(),
            Builtin::RepeatedWords(rule) => rule.id(),
            Builtin::TooOftenUsed(rule) => rule.id(),
            Builtin::SpecificCase(rule) => rule.id(),
            Builtin::Dash(rule) => rule.id(),
            Builtin::Compounds(rule) => rule.id(),
            Builtin::HarperConfusion(rule) => rule.id(),
            Builtin::ConsistentApostrophes(rule) => rule.id(),
        }
    }

    /// Runs this rule on one sentence and returns the suggestions it produces. `speller` is
    /// used as by [`Builtin::check_text`].
    pub fn check(&self, sentence: &Sentence, speller: Option<&Speller>) -> Vec<Suggestion> {
        self.check_text(std::slice::from_ref(sentence), speller)
    }

    /// Runs this rule on consecutive sentences of one paragraph. Rules that need context beyond one sentence
    /// (e.g. quotes spanning sentences) see all of them; sentence-level rules check each one separately.
    /// The rules that count words across a text (`EN_REPEATEDWORDS`, `TOO_OFTEN_USED_*`) take
    /// `sentences` as the whole text. Rules that check their suggestions against the speller
    /// (`EN_COMPOUNDS`) use `speller`; pass [`Spell::speller`](crate::spell::Spell::speller) of
    /// the [`Rules`](crate::Rules) to get the suggestions [`Rules::suggest`](crate::Rules::suggest)
    /// reports. Without one they keep every suggestion.
    pub fn check_text(&self, sentences: &[Sentence], speller: Option<&Speller>) -> Vec<Suggestion> {
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
            _ => self.check_paragraph(sentences, &[], true, speller),
        }
    }

    /// Like [`Builtin::check_text`], for a paragraph that follows `previous`,
    /// the sentences before it, oldest first, and that is followed by more
    /// text unless `ends_text` is set. Only rules that are text-level in
    /// LanguageTool look at `previous`: `ENGLISH_WORD_REPEAT_BEGINNING_RULE`
    /// uses the last two sentences, the others at most the last one. Only
    /// rules that look at the end of the text (`EMPTY_LINE`,
    /// `TOO_LONG_PARAGRAPH`, `ENGLISH_WORD_REPEAT_BEGINNING_RULE`) use
    /// `ends_text`. `speller` is used as by [`Builtin::check_text`].
    /// The rules that count words across a text return nothing here:
    /// [`Rules::suggest`](crate::Rules::suggest) runs them on the whole text.
    pub fn check_paragraph(
        &self,
        sentences: &[Sentence],
        previous: &[Sentence],
        ends_text: bool,
        speller: Option<&Speller>,
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
            Builtin::DoublePunctuation(rule) => {
                sentences.iter().flat_map(|x| rule.check(x)).collect()
            }
            Builtin::MultipleWhitespace(rule) => {
                rule.check_with_previous(sentences, previous.last())
            }
            Builtin::SentenceWhitespace(rule) => {
                rule.check_with_previous(sentences, previous.last())
            }
            Builtin::EmptyLine(rule) => rule.check_text(sentences, ends_text),
            Builtin::RepeatedWords(_) | Builtin::TooOftenUsed(_) => Vec::new(),
            Builtin::SpecificCase(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::Dash(rule) => sentences.iter().flat_map(|x| rule.check(x)).collect(),
            Builtin::Compounds(rule) => sentences
                .iter()
                .flat_map(|x| rule.check(x, speller))
                .collect(),
            Builtin::HarperConfusion(rule) => {
                sentences.iter().flat_map(|x| rule.check(x)).collect()
            }
            // Runs in `Rules::resolve_document` instead.
            Builtin::ConsistentApostrophes(_) => Vec::new(),
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

    /// Gets the underlying builtin rule (mutable), e.g. to set an option such as
    /// [`en::TooOftenUsed::set_min_percent`].
    pub fn builtin_mut(&mut self) -> &mut Builtin {
        &mut self.builtin
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

    /// Whether this rule has a case named `case` that [`BuiltinRule::disable_case`] can turn off.
    pub fn has_case(&self, case: &str) -> bool {
        matches!(
            (&self.builtin, case),
            (Builtin::MultipleWhitespace(_), "INDENTATION")
                | (Builtin::TooOftenUsed(_), "DIRECT_SPEECH")
        )
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
