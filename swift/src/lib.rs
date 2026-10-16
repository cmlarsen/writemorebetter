//! Swift bindings for writemorebetter.
//!
//! The compiled rule and dictionary binaries are LGPL-2.1 resources and are deliberately *not*
//! embedded in this crate: `en_tokenizer.bin` and `en_rules.bin` ship as application resources and
//! the caller passes their paths to [`Checker::new`], which keeps them as separately loaded data.
//!
//! [`Checker`] is `Send + Sync`. Load it once (constructing it costs a few hundred milliseconds)
//! and call [`Checker::check`] off the main thread — on iOS/macOS the natural shape is one checker
//! per app, kept by a single owner and queried from a background queue.

use std::io::Cursor;
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};

use nlprule::{Rules, Tokenizer};

uniffi::setup_scaffolding!();

/// Everything that can go wrong while constructing a [`Checker`].
///
/// A rule or binary that fails to load is reported here; nothing panics across the FFI boundary.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CheckError {
    /// The tokenizer file could not be read from disk.
    #[error("failed to read tokenizer file: {message}")]
    TokenizerFile { message: String },
    /// The rules file could not be read from disk.
    #[error("failed to read rules file: {message}")]
    RulesFile { message: String },
    /// The tokenizer binary was read but could not be deserialized.
    #[error("failed to load tokenizer: {message}")]
    Tokenizer { message: String },
    /// The rules binary was read but could not be deserialized.
    #[error("failed to load rules: {message}")]
    Rules { message: String },
    /// `profile` named a profile the engine does not know.
    #[error("unknown profile: {name}")]
    UnknownProfile { name: String },
    /// `spell_variant` named a speller dictionary that is not part of the loaded rules.
    #[error("unknown spell variant: {message}")]
    UnknownSpellVariant { message: String },
    #[error("invalid report input: {message}")]
    ReportInput { message: String },
}

/// A single flagged span.
///
/// `start` and `end` are UTF-16 offsets into the checked string, so they can be used as an
/// `NSRange` location and end without further conversion. `end` is exclusive.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Suggestion {
    /// Start of the flagged span, in UTF-16 code units.
    pub start: u32,
    /// End of the flagged span (exclusive), in UTF-16 code units.
    pub end: u32,
    /// The rule that fired, e.g. `"GRAMMAR/WAS_BEEN/1"`.
    pub source: String,
    /// Human-readable explanation.
    pub message: String,
    /// Suggested replacements, possibly empty.
    pub replacements: Vec<String>,
}

/// A paired lemma and POS reading after disambiguation.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AnalysisReading {
    pub lemma: String,
    pub pos: String,
}

/// An original-surface token, with absolute, exclusive-end UTF-16 offsets.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AnalysisToken {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub readings: Vec<AnalysisReading>,
    pub chunks: Vec<String>,
    pub is_whitespace: bool,
    pub is_punctuation: bool,
}

/// An owned sentence, including whitespace, with absolute UTF-16 offsets.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct AnalysisSentence {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub tokens: Vec<AnalysisToken>,
}

impl From<nlprule::analysis::AnalysisSentence> for AnalysisSentence {
    fn from(sentence: nlprule::analysis::AnalysisSentence) -> Self {
        Self {
            text: sentence.text,
            start: sentence.span.utf16.start as u32,
            end: sentence.span.utf16.end as u32,
            tokens: sentence
                .tokens
                .into_iter()
                .map(|token| AnalysisToken {
                    text: token.text,
                    start: token.span.utf16.start as u32,
                    end: token.span.utf16.end as u32,
                    readings: token
                        .readings
                        .into_iter()
                        .map(|reading| AnalysisReading {
                            lemma: reading.lemma,
                            pos: reading.pos,
                        })
                        .collect(),
                    chunks: token.chunks,
                    is_whitespace: token.is_whitespace,
                    is_punctuation: token.is_punctuation,
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct DocumentOptions {
    #[uniffi(default)]
    pub skip_names: Vec<String>,
    #[uniffi(default = true)]
    pub include_token_details: bool,
    #[uniffi(default)]
    pub hard_wrapped: bool,
}

impl Default for DocumentOptions {
    fn default() -> Self {
        Self {
            skip_names: Vec::new(),
            include_token_details: true,
            hard_wrapped: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocumentSpan {
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocumentToken {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub readings: Vec<AnalysisReading>,
    pub chunks: Vec<String>,
    pub is_whitespace: bool,
    pub is_punctuation: bool,
    pub is_word: bool,
    pub is_skipped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocumentSentence {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub tokens: Vec<DocumentToken>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct DocumentParagraph {
    pub text: String,
    pub start: u32,
    pub end: u32,
    pub sentences: Vec<DocumentSentence>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TokenPosition {
    pub paragraph: u32,
    pub sentence: u32,
    pub token: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Document {
    pub paragraphs: Vec<DocumentParagraph>,
    pub dialogue: Vec<DocumentSpan>,
    pub lemma_index: std::collections::HashMap<String, Vec<TokenPosition>>,
    pub word_count: u32,
}

impl From<writemorebetter_style::Document> for Document {
    fn from(document: writemorebetter_style::Document) -> Self {
        Self {
            paragraphs: document
                .paragraphs
                .into_iter()
                .map(|paragraph| DocumentParagraph {
                    text: paragraph.text,
                    start: paragraph.span.utf16.start as u32,
                    end: paragraph.span.utf16.end as u32,
                    sentences: paragraph
                        .sentences
                        .into_iter()
                        .map(|sentence| DocumentSentence {
                            text: sentence.text,
                            start: sentence.span.utf16.start as u32,
                            end: sentence.span.utf16.end as u32,
                            tokens: sentence
                                .tokens
                                .into_iter()
                                .map(|token| DocumentToken {
                                    text: token.text,
                                    start: token.span.utf16.start as u32,
                                    end: token.span.utf16.end as u32,
                                    readings: token
                                        .readings
                                        .into_iter()
                                        .map(|reading| AnalysisReading {
                                            lemma: reading.lemma,
                                            pos: reading.pos,
                                        })
                                        .collect(),
                                    chunks: token.chunks,
                                    is_whitespace: token.is_whitespace,
                                    is_punctuation: token.is_punctuation,
                                    is_word: token.is_word,
                                    is_skipped: token.is_skipped,
                                })
                                .collect(),
                        })
                        .collect(),
                })
                .collect(),
            dialogue: document
                .dialogue
                .into_iter()
                .map(|span| DocumentSpan {
                    start: span.utf16.start as u32,
                    end: span.utf16.end as u32,
                })
                .collect(),
            lemma_index: document
                .lemma_index
                .into_iter()
                .map(|(lemma, positions)| {
                    (
                        lemma,
                        positions
                            .into_iter()
                            .map(|position| TokenPosition {
                                paragraph: position.paragraph as u32,
                                sentence: position.sentence as u32,
                                token: position.token as u32,
                            })
                            .collect(),
                    )
                })
                .collect(),
            word_count: document.word_count as u32,
        }
    }
}

/// A grammar and spelling checker over a pre-loaded tokenizer and rule set.
///
/// The checker is thread-safe (`Send + Sync`), but loading it deserializes two multi-megabyte
/// binaries, so load it once — typically at app startup, off the main thread — and share the one
/// instance. Calls to [`Checker::check`] are also safe to make off the main thread.
///
/// The rule set sits behind a read/write lock so that the user-dictionary methods below can mutate
/// it while other threads check text: `check` takes the read lock, a dictionary edit takes the
/// write lock, and both are visible to the next `check`.
#[derive(uniffi::Object)]
pub struct Checker {
    tokenizer: Arc<Tokenizer>,
    rules: RwLock<Rules>,
    style: Arc<writemorebetter_style::reports::StyleResources>,
}

const _: fn() = || {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Checker>();
};

impl Checker {
    fn read_rules(&self) -> RwLockReadGuard<'_, Rules> {
        self.rules
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn write_rules(&self) -> RwLockWriteGuard<'_, Rules> {
        self.rules
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

#[uniffi::export]
impl Checker {
    /// Loads a checker from the compiled tokenizer and rules files on disk.
    ///
    /// `spellVariant` selects a speller dictionary when non-nil (for example `"en_US"`); pass
    /// `nil` (or an empty string) to leave spelling off. `profile` selects a rule profile by name;
    /// `"fiction"` (the default) applies the fiction profile described in `docs/PROFILE.md`, an
    /// empty string disables nothing, and an unknown name throws.
    #[uniffi::constructor(default(profile = "fiction", spell_variant))]
    pub fn new(
        tokenizer_path: String,
        rules_path: String,
        spell_variant: Option<String>,
        profile: String,
    ) -> Result<Arc<Self>, CheckError> {
        let tokenizer_bytes =
            std::fs::read(&tokenizer_path).map_err(|err| CheckError::TokenizerFile {
                message: err.to_string(),
            })?;
        let rules_bytes = std::fs::read(&rules_path).map_err(|err| CheckError::RulesFile {
            message: err.to_string(),
        })?;

        let tokenizer = Tokenizer::from_reader(Cursor::new(tokenizer_bytes)).map_err(|err| {
            CheckError::Tokenizer {
                message: err.to_string(),
            }
        })?;

        let mut rules =
            Rules::from_reader(Cursor::new(rules_bytes)).map_err(|err| CheckError::Rules {
                message: err.to_string(),
            })?;

        if !profile.is_empty() {
            let profile = nlprule::profile::Profile::by_name(&profile).ok_or_else(|| {
                CheckError::UnknownProfile {
                    name: profile.clone(),
                }
            })?;
            rules.apply_profile(&profile);
        }

        if let Some(variant) = spell_variant.filter(|v| !v.is_empty()) {
            let variant =
                rules
                    .spell()
                    .variant(&variant)
                    .map_err(|err| CheckError::UnknownSpellVariant {
                        message: err.to_string(),
                    })?;
            rules.spell_mut().options_mut().variant = Some(variant);
        }

        Ok(Arc::new(Checker {
            tokenizer: Arc::new(tokenizer),
            rules: RwLock::new(rules),
            style: Arc::new(writemorebetter_style::reports::StyleResources::new()),
        }))
    }

    /// Checks `text` and returns every flagged span. See [`Suggestion`] for the offset units.
    pub fn check(&self, text: String) -> Vec<Suggestion> {
        let utf16 = Utf16Offsets::new(&text);
        self.read_rules()
            .suggest(&text, &self.tokenizer)
            .iter()
            .map(|suggestion| {
                let span = suggestion.span().char();
                Suggestion {
                    start: utf16.at(span.start),
                    end: utf16.at(span.end),
                    source: suggestion.source().to_string(),
                    message: suggestion.message().to_string(),
                    replacements: suggestion.replacements().to_vec(),
                }
            })
            .collect()
    }

    /// Analyzes text with the loaded tokenizer, without grammar or spelling checks.
    /// Tokens preserve original surface text and include whitespace. Offsets count UTF-16
    /// code units into the input, as in [`Suggestion`]. No rules lock is needed.
    pub fn analyze(&self, text: String) -> Vec<AnalysisSentence> {
        self.tokenizer
            .analyze(&text)
            .into_iter()
            .map(AnalysisSentence::from)
            .collect()
    }

    pub fn analyze_document(&self, text: String, options: Option<DocumentOptions>) -> Document {
        let options = options.unwrap_or_default();
        let options = writemorebetter_style::DocumentOptions::new()
            .with_skip_names(options.skip_names)
            .with_token_details(options.include_token_details)
            .with_hard_wrapped(options.hard_wrapped);
        writemorebetter_style::analyze_document(&self.tokenizer, &text, &options).into()
    }

    /// Retains one analysis. Compact documents support only clichés and sentence length.
    #[uniffi::method(default(options = None))]
    pub fn style_document(
        &self,
        text: String,
        options: Option<DocumentOptions>,
    ) -> Arc<StyleDocument> {
        let input = options.unwrap_or_default();
        let options = writemorebetter_style::DocumentOptions::new()
            .with_skip_names(input.skip_names)
            .with_token_details(input.include_token_details)
            .with_hard_wrapped(input.hard_wrapped);
        Arc::new(StyleDocument {
            inner: writemorebetter_style::StyleDocument::new(
                self.tokenizer.clone(),
                self.style.clone(),
                &text,
                &options,
            ),
        })
    }

    /// Adds words to the user dictionary and returns how many were new.
    ///
    /// Use it for character names, invented places and coinages so the speller stops flagging
    /// them. Entries may be single words, whitespace-separated phrases (`"Port Royal"`), or words
    /// with an internal apostrophe; curly and straight apostrophes match each other. Matching is
    /// case-sensitive except that a lowercase entry also accepts its sentence-initial capitalized
    /// form (an added capitalized name does not make the lowercase form valid), and every entry
    /// accepts a possessive. The caller owns persistence: nothing is written into a binary, and
    /// changes apply to the next `check`.
    pub fn add_words(&self, words: Vec<String>) -> u32 {
        self.write_rules().add_words(words) as u32
    }

    /// Removes words from the user dictionary and returns how many were present.
    pub fn remove_words(&self, words: Vec<String>) -> u32 {
        self.write_rules().remove_words(words) as u32
    }

    /// Replaces the whole user dictionary and returns the number of entries now set.
    pub fn set_words(&self, words: Vec<String>) -> u32 {
        self.write_rules().set_words(words) as u32
    }

    /// Removes every word from the user dictionary.
    pub fn clear_words(&self) {
        self.write_rules().clear_words();
    }

    /// Lists the user dictionary, canonicalized and sorted.
    pub fn words(&self) -> Vec<String> {
        self.read_rules().words()
    }

    /// Selects the writer's mother tongue (e.g. `"de"`) so the false-friend rules for speakers of
    /// that language run; pass `nil` (or an empty string) to turn them off again (the default).
    /// Changes apply to the next `check`.
    pub fn set_mother_tongue(&self, mother_tongue: Option<String>) {
        self.write_rules()
            .set_mother_tongue(mother_tongue.as_deref().filter(|code| !code.is_empty()));
    }

    /// Lists the mother tongues the rules binary has false-friend rules for, as sorted language
    /// codes.
    pub fn mother_tongues(&self) -> Vec<String> {
        self.read_rules()
            .mother_tongues()
            .into_iter()
            .map(str::to_string)
            .collect()
    }
}

/// Maps character indices in a string to UTF-16 code-unit indices.
///
/// Rules report spans as Unicode scalar values; Swift's `NSRange` counts UTF-16 code units, so a
/// span after an astral character (an emoji, say) would be off by one per surrogate pair without
/// this.
struct Utf16Offsets {
    starts: Vec<u32>,
    total: u32,
}

impl Utf16Offsets {
    fn new(text: &str) -> Self {
        let mut starts = Vec::with_capacity(text.chars().count() + 1);
        let mut offset = 0u32;
        starts.push(0);
        for ch in text.chars() {
            offset += ch.len_utf16() as u32;
            starts.push(offset);
        }
        Utf16Offsets {
            starts,
            total: offset,
        }
    }

    fn at(&self, char_index: usize) -> u32 {
        self.starts.get(char_index).copied().unwrap_or(self.total)
    }
}

#[cfg(test)]
mod tests {
    use super::{Checker, Utf16Offsets};

    const FALSE_FRIEND_TEXT: &str =
        "\u{201c}My handy is broken\u{2014}again\u{2026}\u{201d}\u{2028}It\u{2019}s the actual problem.";

    fn checker() -> std::sync::Arc<Checker> {
        Checker::new(
            "../storage/en_tokenizer.bin".to_string(),
            "../storage/en_rules.bin".to_string(),
            None,
            "fiction".to_string(),
        )
        .unwrap()
    }

    fn false_friends(checker: &Checker) -> Vec<(String, u32, u32)> {
        checker
            .check(FALSE_FRIEND_TEXT.to_string())
            .into_iter()
            .filter(|suggestion| suggestion.source.starts_with("FALSE_FRIENDS/"))
            .map(|suggestion| (suggestion.source, suggestion.start, suggestion.end))
            .collect()
    }

    #[test]
    fn mother_tongue_selects_false_friends() {
        let checker = checker();
        assert!(checker.mother_tongues().contains(&"de".to_string()));
        assert!(false_friends(&checker).is_empty());

        checker.set_mother_tongue(Some("de".to_string()));
        assert_eq!(
            false_friends(&checker),
            vec![
                ("FALSE_FRIENDS/HANDY/0".to_string(), 4, 9),
                ("FALSE_FRIENDS/ACTUAL/0".to_string(), 37, 43),
            ]
        );

        checker.set_mother_tongue(Some(String::new()));
        assert!(false_friends(&checker).is_empty());

        checker.set_mother_tongue(Some("de-DE".to_string()));
        assert_eq!(false_friends(&checker).len(), 2);

        checker.set_mother_tongue(Some("xx".to_string()));
        assert!(false_friends(&checker).is_empty());

        checker.set_mother_tongue(Some("DE".to_string()));
        assert_eq!(false_friends(&checker).len(), 2);

        checker.set_mother_tongue(None);
        assert!(false_friends(&checker).is_empty());
    }

    #[test]
    fn offsets_count_utf16_code_units() {
        let offsets = Utf16Offsets::new("a\u{1F600}b");
        assert_eq!(offsets.at(0), 0);
        assert_eq!(offsets.at(1), 1);
        assert_eq!(offsets.at(2), 3);
        assert_eq!(offsets.at(3), 4);
    }

    #[test]
    fn offsets_handle_combining_marks_and_separators() {
        let offsets = Utf16Offsets::new("e\u{301}\u{2014}\u{2028}x");
        assert_eq!(offsets.at(0), 0);
        assert_eq!(offsets.at(1), 1);
        assert_eq!(offsets.at(2), 2);
        assert_eq!(offsets.at(3), 3);
        assert_eq!(offsets.at(4), 4);
    }

    #[test]
    fn offsets_clamp_past_the_end() {
        let offsets = Utf16Offsets::new("\u{1F600}");
        assert_eq!(offsets.at(99), 2);
    }
}

#[derive(uniffi::Object)]
pub struct StyleDocument {
    inner: writemorebetter_style::StyleDocument,
}

#[uniffi::export]
impl StyleDocument {
    /// Counts for the retained analysis, returned as DocumentStats JSON.
    pub fn stats_json(&self) -> String {
        writemorebetter_style::reports::to_utf16_json(&self.inner.stats())
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn echoes_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .echoes(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn repeated_phrases_json(
        &self,
        options_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .repeated_phrases(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn cliches_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .cliches(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    #[uniffi::method(default(options_json = None, baseline_json = None))]
    pub fn dialogue_tags_json(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .dialogue_tags(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    #[uniffi::method(default(options_json = None, baseline_json = None))]
    pub fn sentence_length_json(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .sentence_length(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    #[uniffi::method(default(options_json = None, baseline_json = None))]
    pub fn sentence_starts_json(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .sentence_starts(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn transitions_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .transitions(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn pronoun_starts_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .pronoun_starts(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn sensory_balance_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .sensory_balance(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns the opt-in pacing lens on the retained analysis.
    #[uniffi::method(default(options_json = None))]
    pub fn pacing_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .pacing(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    /// Table: a word-to-concreteness JSON object or TSV text; nil throws a missing-table error.
    #[uniffi::method(default(options_json = None, table = None))]
    pub fn diction_json(
        &self,
        options_json: Option<String>,
        table: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .diction(options_json.as_deref(), table.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null means bundled; "none" disables comparisons; otherwise pass a JSON object.
    #[uniffi::method(default(options_json = None, baseline_json = None))]
    pub fn overused_words_json(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .overused_words(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}
