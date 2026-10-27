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
    /// `enable_rules` named rule selectors that match no rule.
    #[error("unknown rule selectors: {selectors:?}")]
    UnknownSelectors { selectors: Vec<String> },
    /// A rule option was set to a value outside the range it allows.
    #[error("invalid rule option: {message}")]
    InvalidOption { message: String },
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
        to_records(&text, &self.read_rules().suggest(&text, &self.tokenizer))
    }

    /// Checks one paragraph of a larger document on its own, for an editor that rechecks only
    /// the paragraphs that changed.
    ///
    /// Returns the paragraph's suggestions (UTF-16 offsets into `text`) without those of the
    /// rules that need the whole document (`EN_CONSISTENT_APOS`, `EN_WORD_COHERENCY`,
    /// `EN_REPEATEDWORDS`, `TOO_OFTEN_USED_*`), and its digest: the bytes those rules need from
    /// the paragraph, a pure function of `text`. Keep the digest with the paragraph, replace both
    /// when the paragraph changes, and pass the digests of all paragraphs to `resolveDocument`.
    ///
    /// `endsParagraph` (the default) says the paragraph ends at the end of `text`, for an editor
    /// that stores paragraphs without the blank line after them; pass `false` for slices that
    /// keep their line breaks, where a paragraph ends only at a blank line.
    #[uniffi::method(default(ends_paragraph = true))]
    pub fn check_paragraph(&self, text: String, ends_paragraph: bool) -> ParagraphCheck {
        let (suggestions, digest) =
            self.read_rules()
                .check_paragraph(&text, ends_paragraph, &self.tokenizer);
        ParagraphCheck {
            suggestions: to_records(&text, &suggestions),
            digest: digest.to_bytes(),
        }
    }

    /// Runs the whole-document rules over the stored digests of every paragraph, in document
    /// order. Resolve again after every edit: one change can add or remove matches in paragraphs
    /// that did not change. A digest from another engine version, or bytes that are not a
    /// digest, are listed in `stale()`.
    pub fn resolve_document(&self, digests: Vec<Vec<u8>>) -> Arc<DocumentResolution> {
        Arc::new(DocumentResolution {
            matches: self.read_rules().resolve_document_bytes(&digests),
        })
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

    /// Enables rules that are off by default, such as LanguageTool's `picky` style rules, by the
    /// selector syntax profiles use (`"PLAIN_ENGLISH"`, `"STYLE/PASSIVE_VOICE"`,
    /// `"BUILTIN/EN_REDUNDANCY_REPLACE"`; see `docs/PROFILE.md`). It applies on top of the
    /// constructor's profile, so it can turn back on a rule the profile disabled. Returns how
    /// many rules changed from off to on. A selector that matches no rule throws and nothing is
    /// enabled. Changes apply to the next `check`.
    pub fn enable_rules(&self, selectors: Vec<String>) -> Result<u32, CheckError> {
        self.write_rules()
            .enable_rules(&selectors)
            .map(|enabled| enabled as u32)
            .map_err(|err| match err {
                nlprule::Error::UnknownSelectors(selectors) => {
                    CheckError::UnknownSelectors { selectors }
                }
                other => CheckError::Rules {
                    message: other.to_string(),
                },
            })
    }

    /// Sets the minimum percentage of the `TOO_OFTEN_USED_ADJECTIVE_EN`, `TOO_OFTEN_USED_NOUN_EN`
    /// and `TOO_OFTEN_USED_VERB_EN` rules: a word is flagged once it makes up at least `percent`
    /// of all the adjectives, nouns or verbs of the checked text. The default is 5. A value
    /// outside 1...100 throws and nothing changes. It does not enable the rules, which are off by
    /// default; turn them on with `enableRules`. Changes apply to the next `check`.
    pub fn set_too_often_used_percent(&self, percent: u32) -> Result<(), CheckError> {
        self.write_rules()
            .set_too_often_used_percent(percent as usize)
            .map_err(|err| CheckError::InvalidOption {
                message: err.to_string(),
            })
    }

    /// Sets the level of the `READABILITY_RULE_SIMPLE` and `READABILITY_RULE_DIFFICULT` rules,
    /// from 0 (very difficult) to 6 (very easy): `READABILITY_RULE_SIMPLE` flags a paragraph whose
    /// Flesch reading level is above it, `READABILITY_RULE_DIFFICULT` one whose level is below
    /// it. The default is 3. A value above 6 throws and nothing changes. It does not enable the
    /// rules, which are off by default; turn them on with `enableRules`. Changes apply to the next
    /// `check` and `resolveDocument`.
    pub fn set_readability_level(&self, level: u32) -> Result<(), CheckError> {
        self.write_rules()
            .set_readability_level(level as usize)
            .map_err(|err| CheckError::InvalidOption {
                message: err.to_string(),
            })
    }
}

/// One paragraph checked by [`Checker::check_paragraph`].
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ParagraphCheck {
    /// The paragraph's suggestions without the whole-document rules', UTF-16 offsets into it.
    pub suggestions: Vec<Suggestion>,
    /// The paragraph's digest: no text, only offsets, flags and lemmas. Store it with the
    /// paragraph.
    pub digest: Vec<u8>,
}

/// The whole-document rules' matches over a document's digests, from
/// [`Checker::resolve_document`]. It replaces the document's previous resolution.
#[derive(uniffi::Object)]
pub struct DocumentResolution {
    matches: nlprule::DocumentMatches,
}

#[uniffi::export]
impl DocumentResolution {
    /// The indices of the digests that are from another engine version, or not digests: check
    /// those paragraphs again and resolve again. While any are listed, matches can be missing in
    /// every paragraph.
    pub fn stale(&self) -> Vec<u32> {
        self.matches.stale.iter().map(|x| *x as u32).collect()
    }

    /// The indices of the paragraphs that have whole-document matches, ascending.
    pub fn paragraphs(&self) -> Vec<u32> {
        let mut paragraphs: Vec<u32> = self
            .matches
            .matches
            .iter()
            .map(|x| x.paragraph() as u32)
            .collect();
        paragraphs.dedup();
        paragraphs
    }

    /// The suggestions of paragraph `paragraph`, whose text is `text`: `checked`, the suggestions
    /// `checkParagraph` returned for it, merged with its whole-document matches, in UTF-16
    /// offsets into `text`. A match whose span no longer fits `text` is left out, and so is a
    /// `checked` suggestion whose span does not.
    pub fn suggestions(
        &self,
        paragraph: u32,
        text: String,
        checked: Vec<Suggestion>,
    ) -> Vec<Suggestion> {
        let utf16 = Utf16Offsets::new(&text);
        let checked = checked
            .into_iter()
            .filter_map(|suggestion| {
                let chars = utf16.char_at(suggestion.start)?..utf16.char_at(suggestion.end)?;
                Some(nlprule::types::Suggestion::new(
                    suggestion.source,
                    suggestion.message,
                    nlprule::types::Span::from_chars(&text, chars)?,
                    suggestion.replacements,
                ))
            })
            .collect();
        to_records(
            &text,
            &self.matches.suggestions(paragraph as usize, &text, checked),
        )
    }
}

/// `suggestions` of `text` as records with UTF-16 offsets.
fn to_records(text: &str, suggestions: &[nlprule::types::Suggestion]) -> Vec<Suggestion> {
    let utf16 = Utf16Offsets::new(text);
    suggestions
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

    /// The character index at UTF-16 offset `utf16`, or `None` if it is past the end or inside
    /// a surrogate pair.
    fn char_at(&self, utf16: u32) -> Option<usize> {
        self.starts.binary_search(&utf16).ok()
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
    fn enable_rules_applies_after_the_profile() {
        let checker = checker();
        let text = "\u{201c}It\u{2019}s, in my opinion, wrong\u{2026}\u{201d} she said\u{2014}quietly.\u{2028}It was a free gift.";
        let fired = |prefix: &str| {
            checker
                .check(text.to_string())
                .into_iter()
                .filter(|suggestion| suggestion.source.starts_with(prefix))
                .map(|suggestion| (suggestion.start, suggestion.end))
                .collect::<Vec<_>>()
        };
        assert!(fired("PLAIN_ENGLISH/").is_empty());
        assert!(fired("REDUNDANCY/").is_empty());

        assert!(
            checker
                .enable_rules(vec![
                    "PLAIN_ENGLISH/IN_MY_OPINION".into(),
                    "REDUNDANCY".into()
                ])
                .unwrap()
                > 1
        );
        // UTF-16 offsets of ", in my opinion," / "in my opinion" after the curly quote and apostrophe.
        let opinion = fired("PLAIN_ENGLISH/IN_MY_OPINION");
        assert_eq!(opinion.len(), 1);
        assert!(opinion[0].0 >= 5 && opinion[0].1 <= 21, "{opinion:?}");
        assert!(!fired("REDUNDANCY/").is_empty());

        match checker.enable_rules(vec![
            "NO_SUCH_CATEGORY".into(),
            "STYLE/PASSIVE_VOICE".into(),
        ]) {
            Err(super::CheckError::UnknownSelectors { selectors }) => {
                assert_eq!(selectors, ["NO_SUCH_CATEGORY"])
            }
            other => panic!("unexpected result: {other:?}"),
        }
        assert_eq!(checker.enable_rules(Vec::new()).unwrap(), 0);
    }

    #[test]
    fn readability_level_sets_both_rules() {
        let checker = checker();
        checker
            .enable_rules(vec![
                "BUILTIN/READABILITY_RULE_SIMPLE".into(),
                "BUILTIN/READABILITY_RULE_DIFFICULT".into(),
            ])
            .unwrap();
        // Two easy paragraphs (level 5) with curly quotes and apostrophes, an em dash, an
        // ellipsis and a line separator.
        let paragraph = "\u{201c}She walked to the store\u{2014}and bought some bread for her \
                         family\u{2019}s table,\u{201d} he said\u{2026}\u{2028}It was before \
                         the evening came.";
        let text = format!("{paragraph}\n\n{paragraph}");
        let flags = || {
            checker
                .check(text.clone())
                .into_iter()
                .filter(|s| s.source.starts_with("BUILTIN/READABILITY_RULE_"))
                .map(|s| (s.source.trim_start_matches("BUILTIN/").to_owned(), s.start))
                .collect::<Vec<_>>()
        };
        // The UTF-16 start of the second paragraph's opening quote.
        let second = paragraph.encode_utf16().count() as u32 + 2;
        let simple = "READABILITY_RULE_SIMPLE".to_owned();
        let difficult = "READABILITY_RULE_DIFFICULT".to_owned();
        assert_eq!(flags(), [(simple.clone(), 0), (simple.clone(), second)]);
        checker.set_readability_level(5).unwrap();
        assert!(flags().is_empty());
        checker.set_readability_level(6).unwrap();
        assert_eq!(flags(), [(difficult.clone(), 0), (difficult, second)]);
        for level in [7, u32::MAX] {
            match checker.set_readability_level(level) {
                Err(super::CheckError::InvalidOption { message }) => {
                    assert!(message.contains("between 0 and 6"), "{message}")
                }
                other => panic!("unexpected result: {other:?}"),
            }
        }
        assert_eq!(flags().len(), 2, "an out-of-range level changes nothing");
        checker.set_readability_level(0).unwrap();
        assert_eq!(flags(), [(simple.clone(), 0), (simple, second)]);
    }

    #[test]
    fn too_often_used_percent_sets_the_threshold() {
        let checker = checker();
        checker
            .enable_rules(vec!["BUILTIN/TOO_OFTEN_USED_NOUN_EN".into()])
            .unwrap();
        // 10 "cat"s among 105 nouns (9.5%) in narration (the fiction profile leaves dialogue
        // out), with a curly apostrophe, an em dash, an ellipsis, a line separator and curly
        // quotes.
        let mut sentences = vec![
            "He\u{2019}d seen the cat\u{2014}the cat\u{2026}\u{2028}\u{201c}So?\u{201d} she said.";
            5
        ]
        .join(" ");
        for noun in "dog tree river car book chair garden street city lamp bottle cup plate shirt \
            horse apple kitchen window door table house phone mountain forest island bridge castle \
            candle blanket pillow spoon fork knife basket ladder carpet curtain mirror wallet pocket \
            jacket helmet hammer engine wheel tunnel valley meadow village harbor lantern barrel \
            saddle anchor violin piano guitar trumpet tomato potato carrot onion banana lemon cherry \
            peach pear melon cabbage pepper garlic biscuit pastry sandwich elephant giraffe monkey \
            tiger rabbit turtle squirrel donkey camel dolphin parrot pigeon sparrow eagle owl spider \
            beetle ant wasp nephew niece uncle aunt cousin grandmother teacher doctor farmer"
            .split_whitespace()
            .take(95)
        {
            sentences.push_str(&format!(" I saw the {noun}."));
        }
        let cats = || {
            checker
                .check(sentences.clone())
                .into_iter()
                .filter(|suggestion| suggestion.source == "BUILTIN/TOO_OFTEN_USED_NOUN_EN")
                .map(|suggestion| (suggestion.start, suggestion.end))
                .collect::<Vec<_>>()
        };

        let flagged = cats();
        assert_eq!(flagged.len(), 10);
        // UTF-16 offsets of "cat" after the curly apostrophe and the em dash.
        assert_eq!(flagged[0], (14, 17));
        assert_eq!(flagged[1], (22, 25));

        checker.set_too_often_used_percent(10).unwrap();
        assert!(cats().is_empty());
        checker.set_too_often_used_percent(9).unwrap();
        assert_eq!(cats().len(), 10);

        for percent in [0, 101] {
            match checker.set_too_often_used_percent(percent) {
                Err(super::CheckError::InvalidOption { message }) => {
                    assert!(message.contains("between 1 and 100"), "{message}")
                }
                other => panic!("unexpected result: {other:?}"),
            }
        }
        assert_eq!(cats().len(), 10);
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

    #[test]
    fn utf16_offsets_map_back_to_chars() {
        let offsets = Utf16Offsets::new("a\u{1F600}b");
        assert_eq!(offsets.char_at(0), Some(0));
        assert_eq!(offsets.char_at(1), Some(1));
        assert_eq!(offsets.char_at(2), None);
        assert_eq!(offsets.char_at(3), Some(2));
        assert_eq!(offsets.char_at(4), Some(3));
        assert_eq!(offsets.char_at(5), None);
    }

    /// Paragraphs with curly quotes and apostrophes, an em dash, an ellipsis, U+2028, an emoji
    /// (chars and UTF-16 units disagree after it) and a single line break; straight apostrophes
    /// in the last one mix the styles.
    const PARAGRAPHS: [&str; 3] = [
        "\u{201c}I\u{2019}ll be the adviser\u{2014}no one else,\u{201d} she said\u{2026}\u{2028}He \
         nodded. \u{1F600} She was definitely tired.\nThe dog\u{2019}s bowl was empty.",
        "It\u{2019}s late. \u{1F600} The advisor left. She definitely knew.",
        "'Tis true, he said. It's done.",
    ];

    fn is_whole_document(suggestion: &super::Suggestion) -> bool {
        [
            "EN_CONSISTENT_APOS",
            "EN_WORD_COHERENCY",
            "EN_REPEATEDWORDS",
        ]
        .iter()
        .any(|rule| suggestion.source.contains(rule))
    }

    fn incremental(checker: &Checker, paragraphs: &[&str]) -> Vec<super::Suggestion> {
        let checks: Vec<_> = paragraphs
            .iter()
            .map(|text| checker.check_paragraph(text.to_string(), true))
            .collect();
        let resolution =
            checker.resolve_document(checks.iter().map(|check| check.digest.clone()).collect());
        assert!(resolution.stale().is_empty());
        let mut out = Vec::new();
        let mut start = 0;
        for (i, (text, check)) in paragraphs.iter().zip(checks).enumerate() {
            for mut suggestion in
                resolution.suggestions(i as u32, text.to_string(), check.suggestions)
            {
                suggestion.start += start;
                suggestion.end += start;
                out.push(suggestion);
            }
            start += text.encode_utf16().count() as u32 + 2;
        }
        out
    }

    #[test]
    fn check_paragraph_and_resolve_document_equal_check() {
        let checker = checker();
        checker
            .enable_rules(vec!["BUILTIN/EN_REPEATEDWORDS".to_string()])
            .unwrap();
        let document = PARAGRAPHS.join("\n\n");
        let whole: Vec<_> = checker
            .check(document.clone())
            .into_iter()
            .filter(is_whole_document)
            .collect();
        let split: Vec<_> = incremental(&checker, &PARAGRAPHS)
            .into_iter()
            .filter(is_whole_document)
            .collect();
        assert_eq!(split, whole);
        let units: Vec<u16> = document.encode_utf16().collect();
        let flagged = |source: &str| -> Vec<String> {
            whole
                .iter()
                .filter(|s| s.source.contains(source))
                .map(|s| String::from_utf16(&units[s.start as usize..s.end as usize]).unwrap())
                .collect()
        };
        assert_eq!(flagged("EN_WORD_COHERENCY"), ["advisor"]);
        assert_eq!(flagged("EN_REPEATEDWORDS"), ["definitely"]);
        // The fiction profile flags only the rarer style.
        assert_eq!(flagged("EN_CONSISTENT_APOS"), ["'", "'s"]);

        // An edit rechecks one paragraph; the variant in an unchanged paragraph goes away.
        let edited = PARAGRAPHS[0].replace("adviser", "counselor");
        let paragraphs = [edited.as_str(), PARAGRAPHS[1], PARAGRAPHS[2]];
        let whole: Vec<_> = checker
            .check(paragraphs.join("\n\n"))
            .into_iter()
            .filter(is_whole_document)
            .collect();
        let split: Vec<_> = incremental(&checker, &paragraphs)
            .into_iter()
            .filter(is_whole_document)
            .collect();
        assert_eq!(split, whole);
        assert!(split
            .iter()
            .all(|s| !s.source.contains("EN_WORD_COHERENCY")));
    }

    #[test]
    fn unreadable_digests_are_stale() {
        let checker = checker();
        let check = checker.check_paragraph(PARAGRAPHS[1].to_string(), true);
        assert_eq!(check.digest[..4], [3, 0, 0, 0]);
        let resolution = checker.resolve_document(vec![check.digest, vec![1, 0, 0, 0, 7], vec![]]);
        assert_eq!(resolution.stale(), [1, 2]);
        // A checked span inside a surrogate pair is left out rather than misplaced.
        let bad = super::Suggestion {
            start: 1,
            end: 2,
            source: "X".into(),
            message: String::new(),
            replacements: Vec::new(),
        };
        assert!(resolution
            .suggestions(0, "\u{1F600}".to_string(), vec![bad])
            .is_empty());
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
    /// Returns manner adverbs in narration and speech.
    #[uniffi::method(default(options_json = None))]
    pub fn adverbs_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .adverbs(options_json.as_deref())
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
    /// Table: a word-to-concreteness JSON object or TSV text; nil uses the bundled table.
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
    /// Returns the opt-in consistency lens on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn consistency_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .consistency(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns the opt-in readability lens on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn readability_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .readability(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns this report on the retained analysis; options default when omitted.
    /// Baseline: omitted/null, "bundled" and "none" disable comparisons (no published band yet);
    /// "publicStandIn" selects the provisional public stand-in band; otherwise pass a JSON object.
    #[uniffi::method(default(options_json = None, baseline_json = None))]
    pub fn passive_json(
        &self,
        options_json: Option<String>,
        baseline_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .passive(options_json.as_deref(), baseline_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns the opt-in clarity lens (sticky sentences, long subordinate clauses).
    #[uniffi::method(default(options_json = None))]
    pub fn clarity_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .clarity(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Runs caller-supplied house-style rules on the retained analysis; options default when omitted.
    /// Rules: Vale-format YAML or JSON text; nil throws an invalid-rules error.
    #[uniffi::method(default(options_json = None, rules = None))]
    pub fn house_style_json(
        &self,
        options_json: Option<String>,
        rules: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .house_style(options_json.as_deref(), rules.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns this report on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn regional_spelling_json(
        &self,
        options_json: Option<String>,
    ) -> Result<String, CheckError> {
        self.inner
            .regional_spelling(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    #[uniffi::method(default(options_json = None))]
    pub fn name_spelling_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .name_spelling(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns the opt-in emotion-tells lens on the retained analysis; narration only by default.
    #[uniffi::method(default(options_json = None))]
    pub fn emotion_tells_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .emotion_tells(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}

/// The built-in style presets' names; the first, `"chicago-fiction"`, is the default.
#[uniffi::export]
pub fn style_preset_names() -> Vec<String> {
    writemorebetter_style::preset::StylePreset::names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// A built-in style preset by name, resolved to self-contained preset JSON.
#[uniffi::export]
pub fn load_style_preset_json(name: String) -> Result<String, CheckError> {
    writemorebetter_style::preset::StylePreset::builtin(&name)
        .map(|preset| preset.to_json())
        .map_err(|error| CheckError::ReportInput {
            message: error.to_string(),
        })
}

/// Validates a preset file (JSON), resolves its `extends`, and returns self-contained preset JSON.
#[uniffi::export]
pub fn parse_style_preset_json(text: String) -> Result<String, CheckError> {
    writemorebetter_style::preset::StylePreset::parse(&text)
        .map(|preset| preset.to_json())
        .map_err(|error| CheckError::ReportInput {
            message: error.to_string(),
        })
}

#[uniffi::export]
impl Checker {
    /// Applies a style preset's grammar settings: its profile, the rules it turns off and on, and its
    /// speller variant. `preset` is a built-in preset's name or a preset file (JSON). Every selector
    /// is checked first, so an unknown one throws and changes nothing. Returns how many rules went
    /// from off to on. Apply a preset to a checker constructed with `profile: ""`; to switch presets,
    /// construct another checker (profiles cannot be undone).
    pub fn apply_preset(&self, preset: String) -> Result<u32, CheckError> {
        writemorebetter_style::preset::StylePreset::load(&preset)
            .and_then(|preset| preset.apply_to_rules(&mut self.write_rules()))
            .map(|enabled| enabled as u32)
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}

#[uniffi::export]
impl StyleDocument {
    /// Runs the built-in preset checks; options are `PresetChecks` JSON (every check off by default).
    #[uniffi::method(default(options_json = None))]
    pub fn preset_checks_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .preset_checks(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Runs every report a style preset drives (consistency, house style, preset checks).
    /// `preset` is a built-in preset's name or a preset file (JSON); nil selects "chicago-fiction".
    #[uniffi::method(default(preset = None))]
    pub fn preset_json(&self, preset: Option<String>) -> Result<String, CheckError> {
        self.inner
            .preset(preset.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}

#[uniffi::export]
impl StyleDocument {
    /// Returns the opt-in alliteration lens on the retained analysis; options default when omitted.
    #[uniffi::method(default(options_json = None))]
    pub fn alliteration_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .alliteration(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}

/// Checks headings such as chapter titles against a title-case style; options are
/// `TitleCaseOptions` JSON (Chicago-style headline case when nil). Returns `TitleCaseReport` JSON
/// with UTF-16 spans into each heading.
#[uniffi::export(default(options_json = None))]
pub fn title_case_json(
    headings: Vec<String>,
    options_json: Option<String>,
) -> Result<String, CheckError> {
    use writemorebetter_style::reports::title_case;
    title_case::parse_options(options_json.as_deref())
        .and_then(|options| title_case::report(&headings, &options))
        .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
        .map_err(|error| CheckError::ReportInput {
            message: error.to_string(),
        })
}

#[uniffi::export]
impl StyleDocument {
    /// Returns drafting leftovers and their mechanical edits on the retained analysis.
    #[uniffi::method(default(options_json = None))]
    pub fn drafting_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .drafting(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns the opt-in vague-words lens on the retained analysis; narration and dialogue are
    /// counted separately.
    #[uniffi::method(default(options_json = None))]
    pub fn vague_words_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .vague_words(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }

    /// Returns the opt-in hidden-verbs lens on the retained analysis; each match names the related
    /// verb but never rewrites.
    #[uniffi::method(default(options_json = None))]
    pub fn hidden_verbs_json(&self, options_json: Option<String>) -> Result<String, CheckError> {
        self.inner
            .hidden_verbs(options_json.as_deref())
            .map(|report| writemorebetter_style::reports::to_utf16_json(&report))
            .map_err(|error| CheckError::ReportInput {
                message: error.to_string(),
            })
    }
}
