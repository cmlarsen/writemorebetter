//! Rules that need the whole text: per-paragraph digests and the document resolver.
//!
//! A few LanguageTool rules decide from text outside the paragraph they report in, e.g.
//! `EN_CONSISTENT_APOS` flags apostrophes only when the whole text mixes `'` and `’`, and the
//! word-statistics rules count words over the whole text: `TOO_OFTEN_USED_*` (each lemma's share
//! of all adjectives, nouns or verbs), `EN_REPEATEDWORDS` (a word used up to 150 words earlier)
//! and `EN_WORD_COHERENCY` (a spelling variant used anywhere earlier). An app that
//! checks one paragraph per call can't run them on the paragraph alone, and re-checking the whole
//! manuscript on every edit is too slow. Instead:
//!
//! 1. Checking a paragraph also yields a [`ParagraphDigest`]: the few offsets and flags of that
//!    paragraph the whole-text rules need. It is a pure function of the paragraph's text (and of
//!    whether the caller says the paragraph ends there), so an app keeps one per paragraph and
//!    replaces it when the paragraph changes ([`Rules::check_paragraph`]).
//! 2. [`Rules::resolve_document`] takes the digests of all paragraphs in order and returns the
//!    whole-text rules' matches ([`DocumentMatch`]), each tagged with its paragraph and with a span
//!    relative to it. It never re-tokenizes, and its result replaces the previous set, since one
//!    edit can add or remove matches in paragraphs that did not change.
//! 3. [`DocumentMatch::to_suggestion`] turns a match into a [`Suggestion`] against the paragraph's
//!    text, and [`merge_document_suggestions`] merges those into the paragraph's other
//!    suggestions.
//!
//! [`Rules::suggest`] goes through the same steps internally, with one digest per LanguageTool
//! paragraph, so whole-text checks and paragraph-by-paragraph checks share one code path. The
//! equivalence holds for the whole-text rules' matches only: the other rules checked by
//! [`Rules::check_paragraph`] see only that paragraph, while [`Rules::suggest`] lets some of them
//! (`UPPERCASE_SENTENCE_START`, repetition rules, …) look at the text before it.
//!
//! # Paragraph boundaries
//!
//! LanguageTool ends a paragraph at a blank line (`Tools.isParagraphEnd`; a line with only spaces
//! does not count) and at the end of the text. A digest records the blank lines in and around its
//! text from the whitespace between tokens, so it doesn't depend on how the sentencizer attaches
//! whitespace to sentences. An editor that stores paragraphs without their separators says so
//! with the `ends_paragraph` argument of [`Rules::check_paragraph`].
//!
//! # Versions
//!
//! Digests hold offsets, flags and the lemmas the word-statistics rules count (as strings, so a
//! digest does not depend on the tagger's word ids), never the paragraph's text itself. Their
//! layout changes when a whole-text rule needs more from a paragraph; [`DIGEST_VERSION`] changes
//! with it. [`ParagraphDigest::to_bytes`] and
//! [`ParagraphDigest::from_bytes`] store a digest with its version and reject one from another
//! version with [`Error::Format`]. A digest of another version that reaches
//! [`Rules::resolve_document`] some other way (e.g. through serde with a self-describing format)
//! contributes nothing to it, and [`DocumentMatches::stale`] lists it so the app checks that
//! paragraph again.
//!
//! [`DIGEST_VERSION`] covers the layout only. What a digest records also depends on the
//! tokenizer and rules data (how a text is split into tokens, which tokens hold apostrophes, the
//! lemmas and tags of a word), so rebuild all digests after updating the engine or its data
//! binaries, even when the version is unchanged.
//!
//! # Sentences
//!
//! The word-statistics rules also look at sentences: `EN_REPEATEDWORDS` only counts sentences that
//! end in `.`, `!` or `?`, and a word's tags depend on the sentence around it. Split a document
//! between sentences (at line breaks or paragraph ends) for the per-paragraph matches to equal
//! those of the whole text; a piece that ends inside a sentence is analyzed as a sentence of its
//! own.

use crate::{
    builtin::{en, Builtin},
    types::{Position, Sentence, Span, Suggestion},
    Error, Rules, Tokenizer,
};
use bincode::Options;
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// The layout version of [`ParagraphDigest`]. Digests of another version contribute nothing to
/// [`Rules::resolve_document`]; check their paragraphs again.
///
/// Version 2: the word-statistics rules (`TOO_OFTEN_USED_*`, `EN_REPEATEDWORDS`,
/// `EN_WORD_COHERENCY`), and [`ParagraphDigest::to_bytes`] writes variable-length integers and
/// relative offsets.
pub const DIGEST_VERSION: u32 = 2;

/// A compact text position: byte and char offset relative to the start of a digest's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Offset {
    byte: u32,
    char: u32,
}

impl Offset {
    pub(crate) fn new(position: Position) -> Self {
        Offset {
            byte: position.byte as u32,
            char: position.char as u32,
        }
    }

    fn of(text: &str) -> Self {
        Offset {
            byte: text.len() as u32,
            char: text.chars().count() as u32,
        }
    }

    fn position(self) -> Position {
        Position {
            byte: self.byte as usize,
            char: self.char as usize,
        }
    }

    fn shift(self, by: Offset) -> Self {
        Offset {
            byte: self.byte + by.byte,
            char: self.char + by.char,
        }
    }

    fn checked_add(self, by: Offset) -> Option<Self> {
        Some(Offset {
            byte: self.byte.checked_add(by.byte)?,
            char: self.char.checked_add(by.char)?,
        })
    }

    fn checked_sub(self, by: Offset) -> Option<Self> {
        Some(Offset {
            byte: self.byte.checked_sub(by.byte)?,
            char: self.char.checked_sub(by.char)?,
        })
    }
}

/// The span of a token in a digest, relative to the start of the digest's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TokenSpan {
    pub(crate) start: Offset,
    pub(crate) end: Offset,
}

impl TokenSpan {
    /// The span of `span`, a span in the checked text, in a digest whose text starts at `start`.
    pub(crate) fn new(span: &Span, start: Position) -> Self {
        TokenSpan {
            start: Offset::new(span.start() - start),
            end: Offset::new(span.end() - start),
        }
    }

    pub(crate) fn span(&self) -> Span {
        Span::from_positions(self.start.position(), self.end.position())
    }

    fn shift(self, by: Offset) -> Self {
        TokenSpan {
            start: self.start.shift(by),
            end: self.end.shift(by),
        }
    }
}

/// A token that contains an apostrophe, as `EN_CONSISTENT_APOS` sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Apostrophe {
    span: TokenSpan,
    /// Whether the token's apostrophe is typographic (`’`) rather than typewriter (`'`).
    pub(crate) typographic: bool,
    /// The char index of the first apostrophe in the token (at most 255), which
    /// [`DocumentMatch::to_suggestion`] checks to notice a stale digest.
    pub(crate) first: u8,
}

impl Apostrophe {
    pub(crate) fn span(&self) -> Span {
        self.span.span()
    }
}

/// A word one of the `TOO_OFTEN_USED_*` rules counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CountedWord {
    pub(crate) span: TokenSpan,
    /// The lemma the word is counted under.
    pub(crate) lemma: String,
    /// Whether the word is in direct speech: bit 0 if the text starts outside direct speech, bit
    /// 1 if it starts inside. Only the rules that skip direct speech read it.
    pub(crate) speech: u8,
}

impl CountedWord {
    /// Whether the word is in direct speech when the text starts inside it (`in_speech`) or not.
    pub(crate) fn in_speech(&self, in_speech: bool) -> bool {
        self.speech & (1 << u8::from(in_speech)) != 0
    }
}

/// The words of one class (adjectives, nouns or verbs) that `TOO_OFTEN_USED_*` counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassWords {
    pub(crate) words: Vec<CountedWord>,
    /// Whether the text ends in direct speech if it starts outside it (`[0]`) or inside (`[1]`).
    /// While the digest is built, the state after the last sentence so far.
    pub(crate) speech_after: [bool; 2],
}

impl Default for ClassWords {
    fn default() -> Self {
        ClassWords {
            words: Vec::new(),
            speech_after: [false, true],
        }
    }
}

impl ClassWords {
    /// Appends `other`, the words of the text right after this one's, shifted by `by`.
    fn append(&mut self, other: ClassWords, by: Offset) {
        let after = self.speech_after;
        self.words.extend(other.words.into_iter().map(|word| {
            // Express the state at `other`'s start through the state at this text's start.
            let speech =
                u8::from(word.in_speech(after[0])) | u8::from(word.in_speech(after[1])) << 1;
            CountedWord {
                span: word.span.shift(by),
                speech,
                ..word
            }
        }));
        self.speech_after = [
            other.speech_after[usize::from(after[0])],
            other.speech_after[usize::from(after[1])],
        ];
    }
}

/// One reading of a word that `EN_REPEATEDWORDS` has synonyms for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepeatedReading {
    pub(crate) lemma: String,
    /// The suggestions when this reading's tag and chunk allow a match, `None` otherwise.
    pub(crate) replacements: Option<Vec<String>>,
}

/// A word that `EN_REPEATEDWORDS` remembers and may flag: one with a reading whose lemma has
/// synonyms, in a sentence the rule checks, and not exempt (an antipattern, a proper noun, …).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RepeatedWord {
    pub(crate) span: TokenSpan,
    /// The number of the word among the words the rule counts in the digest's text, from 1.
    pub(crate) number: u32,
    /// Whether it is the first such word of its sentence.
    pub(crate) starts_sentence: bool,
    /// The readings with synonyms, in the token's order.
    pub(crate) readings: Vec<RepeatedReading>,
}

/// What `EN_REPEATEDWORDS` needs from a digest's text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatedWords {
    /// The number of words the rule counts in the text.
    pub(crate) count: u32,
    pub(crate) words: Vec<RepeatedWord>,
    /// The index in `words` of the first word of the text's last sentence, if that sentence is
    /// one the rule skips when it ends the whole text (it ends with whitespace).
    pub(crate) last_sentence: Option<u32>,
}

impl RepeatedWords {
    fn append(&mut self, other: RepeatedWords, by: Offset) {
        let (count, words) = (self.count, self.words.len() as u32);
        self.words
            .extend(other.words.into_iter().map(|word| RepeatedWord {
                span: word.span.shift(by),
                number: word.number + count,
                ..word
            }));
        self.count += other.count;
        self.last_sentence = other.last_sentence.map(|x| x + words);
    }
}

/// A spelling that `EN_WORD_COHERENCY` knows a variant of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariantCandidate {
    /// The lemma (or the word, if it has none) the rule looks up.
    pub(crate) word: String,
    /// Bit `i` is set when the token can be rewritten with the `i`-th variant of `word`.
    pub(crate) replaceable: u32,
}

/// A token whose lemma or text `EN_WORD_COHERENCY` knows a spelling variant of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VariantWord {
    pub(crate) span: TokenSpan,
    /// The candidates in the rule's order.
    pub(crate) candidates: Vec<VariantCandidate>,
}

/// What the whole-text rules need to know about one paragraph: offsets, flags and the lemmas the
/// word-statistics rules count, no text.
///
/// Built while the paragraph is checked, from its already tokenized and tagged sentences
/// ([`Rules::check_paragraph`]). Offsets are relative to the start of the paragraph's text.
/// Serializable with serde; see the [module docs](self) for how versions are handled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(into = "wire::Digest", try_from = "wire::Digest")]
pub struct ParagraphDigest {
    /// [`DIGEST_VERSION`] when the digest was made.
    version: u32,
    /// The length of the text this digest covers.
    len: Offset,
    /// Whether the text starts with a line break.
    starts_with_line_break: bool,
    /// Whether the text ends with a line break.
    ends_with_line_break: bool,
    /// Whether the whitespace before the first token holds a blank line: the text before this one
    /// ends a paragraph.
    starts_paragraph: bool,
    /// Whether the end of this text ends a paragraph: the whitespace after its last token holds a
    /// blank line, or the caller said so.
    ends_paragraph: bool,
    /// The char offsets inside the text where a paragraph starts after a blank line, in order.
    /// The paragraph's first token is the first one at or after the offset.
    paragraph_starts: Vec<u32>,
    /// The tokens with an apostrophe (`EN_CONSISTENT_APOS`), in text order.
    apostrophes: Vec<Apostrophe>,
    /// The words `TOO_OFTEN_USED_ADJECTIVE_EN`, `_NOUN_EN` and `_VERB_EN` count, in that order.
    pub(crate) too_often_used: [ClassWords; 3],
    /// What `EN_REPEATEDWORDS` remembers and may flag.
    pub(crate) repeated_words: RepeatedWords,
    /// The tokens `EN_WORD_COHERENCY` looks up, in text order.
    pub(crate) variants: Vec<VariantWord>,
}

impl Default for ParagraphDigest {
    fn default() -> Self {
        ParagraphDigest {
            version: DIGEST_VERSION,
            len: Offset::default(),
            starts_with_line_break: false,
            ends_with_line_break: false,
            starts_paragraph: false,
            ends_paragraph: false,
            paragraph_starts: Vec::new(),
            apostrophes: Vec::new(),
            too_often_used: Default::default(),
            repeated_words: RepeatedWords::default(),
            variants: Vec::new(),
        }
    }
}

impl ParagraphDigest {
    /// The byte and char length of the text this digest covers.
    pub fn text_len(&self) -> Position {
        self.len.position()
    }

    /// The char offsets inside the text where a paragraph starts after a blank line, in order.
    /// The paragraph's first token is the first one at or after the offset.
    pub fn paragraph_starts(&self) -> impl Iterator<Item = usize> + '_ {
        self.paragraph_starts.iter().map(|x| *x as usize)
    }

    /// Whether the end of this text ends a paragraph: the whitespace after its last token holds
    /// a blank line, or [`Rules::check_paragraph`] was told so.
    pub fn ends_paragraph(&self) -> bool {
        self.ends_paragraph
    }

    /// The [`DIGEST_VERSION`] this digest was made with.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Whether this digest has the current [`DIGEST_VERSION`]. A digest that is not current
    /// contributes nothing to [`Rules::resolve_document`]; check its paragraph again.
    pub fn is_current(&self) -> bool {
        self.version == DIGEST_VERSION
    }

    /// The bincode options of [`ParagraphDigest::to_bytes`] after the version.
    fn bincode() -> impl bincode::Options {
        bincode::DefaultOptions::new()
            .with_varint_encoding()
            .allow_trailing_bytes()
    }

    /// Serializes this digest compactly: its version as four little-endian bytes, then the digest
    /// with bincode's variable-length integers.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.version.to_le_bytes().to_vec();
        Self::bincode()
            .serialize_into(&mut bytes, self)
            .expect("a digest always serializes");
        bytes
    }

    /// Deserializes a digest written by [`ParagraphDigest::to_bytes`].
    ///
    /// # Errors
    /// [`Error::Format`] if the digest has another [`DIGEST_VERSION`] (check the paragraph again)
    /// or is followed by trailing bytes, [`Error::Serialization`] if the bytes are not a digest.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        let version = bytes
            .get(..4)
            .map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
            .ok_or_else(|| Error::Format("paragraph digest is truncated".to_owned()))?;
        if version != DIGEST_VERSION {
            return Err(Error::Format(format!(
                "paragraph digest version {} is not supported by this engine (expected {}); \
                 check the paragraph again",
                version, DIGEST_VERSION
            )));
        }
        let rest = &bytes[4..];
        let digest: ParagraphDigest = Self::bincode().deserialize(rest)?;
        if digest.version != version {
            return Err(Error::Format(
                "paragraph digest has two different versions".to_owned(),
            ));
        }
        if Self::bincode().serialized_size(&digest)? != rest.len() as u64 {
            return Err(Error::Format(
                "paragraph digest has trailing bytes".to_owned(),
            ));
        }
        Ok(digest)
    }

    /// A digest that is not [current](ParagraphDigest::is_current), standing in for bytes that
    /// are not a digest of this version.
    fn stale() -> Self {
        ParagraphDigest {
            version: 0,
            ..ParagraphDigest::default()
        }
    }

    /// The apostrophe tokens, or none for a digest of another version.
    pub(crate) fn apostrophes(&self) -> &[Apostrophe] {
        if self.is_current() {
            &self.apostrophes
        } else {
            &[]
        }
    }

    /// Sets the flags that depend only on the ends of `text`, the text this digest covers.
    fn set_ends(&mut self, text: &str) {
        let trimmed = text.trim_start();
        let leading = &text[..text.len() - trimmed.len()];
        let trailing = &text[text.trim_end().len()..];
        self.len = Offset::of(text);
        self.starts_with_line_break = text.starts_with('\n') || text.starts_with("\r\n");
        self.ends_with_line_break = text.ends_with('\n');
        self.starts_paragraph = has_blank_line(leading);
        self.ends_paragraph = has_blank_line(trailing);
    }

    /// Builds the digest of `text`, which starts at `start` in the checked text and holds
    /// `sentences`. Only the parts for `rules` are filled in.
    fn from_sentences<'a>(
        text: &str,
        sentences: &[Sentence],
        start: Position,
        rules: impl Iterator<Item = &'a Builtin> + Clone,
    ) -> Self {
        let mut digest = ParagraphDigest::default();
        digest.set_ends(text);

        let mut previous_end: Option<usize> = None;
        for sentence in sentences {
            for token in sentence.tokens() {
                let span = token.span().clone().lshift(start);
                if let Some(end) = previous_end {
                    let gap = text.get(end..span.byte().start).unwrap_or_default();
                    if has_blank_line(gap) {
                        digest.paragraph_starts.push(span.char().start as u32);
                    }
                }
                previous_end = Some(span.byte().end);
            }
            for rule in rules.clone() {
                rule.digest_sentence(sentence, start, &mut digest);
            }
        }
        digest
    }

    /// Appends `other`, the digest of the text right after this one's.
    fn append(&mut self, mut other: ParagraphDigest) {
        if other.len.char == 0 {
            self.ends_paragraph |= other.ends_paragraph;
            return;
        }
        let by = self.len;
        if self.len.char == 0 {
            other.starts_paragraph |= self.ends_paragraph;
            *self = other;
            return;
        }
        if breaks_between(self, &other) {
            self.paragraph_starts.push(by.char);
        }
        self.paragraph_starts
            .extend(other.paragraph_starts.iter().map(|x| x + by.char));
        for apostrophe in other.apostrophes.iter_mut() {
            apostrophe.span = apostrophe.span.shift(by);
        }
        self.apostrophes.append(&mut other.apostrophes);
        for (class, other) in self.too_often_used.iter_mut().zip(other.too_often_used) {
            class.append(other, by);
        }
        self.repeated_words.append(other.repeated_words, by);
        self.variants
            .extend(other.variants.into_iter().map(|word| VariantWord {
                span: word.span.shift(by),
                ..word
            }));
        self.len = self.len.shift(other.len);
        self.ends_with_line_break = other.ends_with_line_break;
        self.ends_paragraph = other.ends_paragraph;
    }
}

/// The serialized form of [`ParagraphDigest`]: lemmas are stored once per digest, and each span
/// is stored relative to the start of the one before it in its list, so that the variable-length
/// integers of [`ParagraphDigest::to_bytes`] are mostly one byte long.
mod wire {
    use super::Offset;
    use crate::types::DefaultHashMap;
    use serde::{Deserialize, Serialize};
    use std::convert::TryFrom;

    /// A span: its start relative to the start of the previous span in its list (or of the text),
    /// and its length.
    #[derive(Serialize, Deserialize)]
    pub(super) struct Span {
        gap: Offset,
        len: Offset,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct Apostrophe {
        span: Span,
        typographic: bool,
        first: u8,
    }

    /// The counted words, the longest list, flattened to save a few bytes each: per word the
    /// char gap of its span, `char length << 3 | speech << 1 | b` and the lemma's index, where
    /// `b` says that the byte gap and byte length follow because they differ from the char ones.
    /// A word then mostly takes three bytes.
    #[derive(Serialize, Deserialize)]
    pub(super) struct ClassWords {
        words: Vec<u32>,
        speech_after: [bool; 2],
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct RepeatedReading {
        lemma: u32,
        replacements: Option<Vec<String>>,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct RepeatedWord {
        span: Span,
        /// Relative to the number of the previous word in the list.
        number: u32,
        starts_sentence: bool,
        readings: Vec<RepeatedReading>,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct VariantCandidate {
        word: u32,
        replaceable: u32,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct VariantWord {
        span: Span,
        candidates: Vec<VariantCandidate>,
    }

    #[derive(Serialize, Deserialize)]
    pub(super) struct Digest {
        version: u32,
        len: Offset,
        /// `starts_with_line_break`, `ends_with_line_break`, `starts_paragraph` and
        /// `ends_paragraph` as bits 0 to 3.
        flags: u8,
        paragraph_starts: Vec<u32>,
        apostrophes: Vec<Apostrophe>,
        /// Every distinct lemma (or word) the lists below refer to by index.
        lemmas: Vec<String>,
        too_often_used: [ClassWords; 3],
        repeated_count: u32,
        repeated_words: Vec<RepeatedWord>,
        repeated_last_sentence: Option<u32>,
        variants: Vec<VariantWord>,
    }

    /// Writes spans relative to the previous one.
    #[derive(Default)]
    struct Spans(Offset);

    impl Spans {
        fn put(&mut self, span: super::TokenSpan) -> Span {
            let gap = span.start.checked_sub(self.0).unwrap_or_default();
            self.0 = span.start;
            Span {
                gap,
                len: span.end.checked_sub(span.start).unwrap_or_default(),
            }
        }

        fn get(&mut self, span: &Span) -> Result<super::TokenSpan, String> {
            let overflow = || "paragraph digest span overflows".to_owned();
            let start = self.0.checked_add(span.gap).ok_or_else(overflow)?;
            let end = start.checked_add(span.len).ok_or_else(overflow)?;
            self.0 = start;
            Ok(super::TokenSpan { start, end })
        }
    }

    #[derive(Default)]
    struct Lemmas {
        list: Vec<String>,
        index: DefaultHashMap<String, u32>,
    }

    impl Lemmas {
        fn put(&mut self, lemma: String) -> u32 {
            let Lemmas { list, index } = self;
            *index.entry(lemma).or_insert_with_key(|lemma| {
                list.push(lemma.clone());
                list.len() as u32 - 1
            })
        }
    }

    fn lemma(lemmas: &[String], index: u32) -> Result<String, String> {
        lemmas
            .get(index as usize)
            .cloned()
            .ok_or_else(|| "paragraph digest refers to a missing lemma".to_owned())
    }

    impl From<super::ParagraphDigest> for Digest {
        fn from(digest: super::ParagraphDigest) -> Self {
            let mut lemmas = Lemmas::default();
            let mut spans = Spans::default();
            let apostrophes = digest
                .apostrophes
                .into_iter()
                .map(|x| Apostrophe {
                    span: spans.put(x.span),
                    typographic: x.typographic,
                    first: x.first,
                })
                .collect();
            let too_often_used = digest.too_often_used.map(|class| {
                let mut spans = Spans::default();
                let mut words = Vec::with_capacity(class.words.len() * 3);
                for word in class.words {
                    let span = spans.put(word.span);
                    let bytes = span.gap.byte != span.gap.char || span.len.byte != span.len.char;
                    words.extend([
                        span.gap.char,
                        span.len.char << 3 | u32::from(word.speech) << 1 | u32::from(bytes),
                        lemmas.put(word.lemma),
                    ]);
                    if bytes {
                        words.extend([span.gap.byte, span.len.byte]);
                    }
                }
                ClassWords {
                    words,
                    speech_after: class.speech_after,
                }
            });
            let mut spans = Spans::default();
            let mut number = 0;
            let repeated_words = digest
                .repeated_words
                .words
                .into_iter()
                .map(|word| {
                    let relative = word.number.saturating_sub(number);
                    number = word.number;
                    RepeatedWord {
                        span: spans.put(word.span),
                        number: relative,
                        starts_sentence: word.starts_sentence,
                        readings: word
                            .readings
                            .into_iter()
                            .map(|reading| RepeatedReading {
                                lemma: lemmas.put(reading.lemma),
                                replacements: reading.replacements,
                            })
                            .collect(),
                    }
                })
                .collect();
            let mut spans = Spans::default();
            let variants = digest
                .variants
                .into_iter()
                .map(|word| VariantWord {
                    span: spans.put(word.span),
                    candidates: word
                        .candidates
                        .into_iter()
                        .map(|candidate| VariantCandidate {
                            word: lemmas.put(candidate.word),
                            replaceable: candidate.replaceable,
                        })
                        .collect(),
                })
                .collect();
            Digest {
                version: digest.version,
                len: digest.len,
                flags: u8::from(digest.starts_with_line_break)
                    | u8::from(digest.ends_with_line_break) << 1
                    | u8::from(digest.starts_paragraph) << 2
                    | u8::from(digest.ends_paragraph) << 3,
                paragraph_starts: digest.paragraph_starts,
                apostrophes,
                lemmas: lemmas.list,
                too_often_used,
                repeated_count: digest.repeated_words.count,
                repeated_words,
                repeated_last_sentence: digest.repeated_words.last_sentence,
                variants,
            }
        }
    }

    impl TryFrom<Digest> for super::ParagraphDigest {
        type Error = String;

        fn try_from(digest: Digest) -> Result<Self, String> {
            let lemmas = &digest.lemmas;
            let mut spans = Spans::default();
            let apostrophes = digest
                .apostrophes
                .into_iter()
                .map(|x| {
                    Ok(super::Apostrophe {
                        span: spans.get(&x.span)?,
                        typographic: x.typographic,
                        first: x.first,
                    })
                })
                .collect::<Result<_, String>>()?;
            let mut too_often_used: [super::ClassWords; 3] = Default::default();
            for (class, wire) in too_often_used.iter_mut().zip(digest.too_often_used) {
                let mut spans = Spans::default();
                let mut values = wire.words.into_iter();
                let truncated = || "paragraph digest word list is truncated".to_owned();
                while let Some(gap) = values.next() {
                    let packed = values.next().ok_or_else(truncated)?;
                    let len = packed >> 3;
                    let lemma_index = values.next().ok_or_else(truncated)?;
                    let (gap_byte, len_byte) = if packed & 1 != 0 {
                        let gap = values.next().ok_or_else(truncated)?;
                        (gap, values.next().ok_or_else(truncated)?)
                    } else {
                        (gap, len)
                    };
                    let span = Span {
                        gap: Offset {
                            byte: gap_byte,
                            char: gap,
                        },
                        len: Offset {
                            byte: len_byte,
                            char: len,
                        },
                    };
                    class.words.push(super::CountedWord {
                        span: spans.get(&span)?,
                        lemma: lemma(lemmas, lemma_index)?,
                        speech: (packed >> 1 & 0b11) as u8,
                    });
                }
                class.speech_after = wire.speech_after;
            }
            let mut spans = Spans::default();
            let mut number = 0u32;
            let words = digest
                .repeated_words
                .into_iter()
                .map(|word| {
                    number = number
                        .checked_add(word.number)
                        .ok_or_else(|| "paragraph digest word number overflows".to_owned())?;
                    Ok(super::RepeatedWord {
                        span: spans.get(&word.span)?,
                        number,
                        starts_sentence: word.starts_sentence,
                        readings: word
                            .readings
                            .into_iter()
                            .map(|reading| {
                                Ok(super::RepeatedReading {
                                    lemma: lemma(lemmas, reading.lemma)?,
                                    replacements: reading.replacements,
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    })
                })
                .collect::<Result<_, String>>()?;
            let mut spans = Spans::default();
            let variants = digest
                .variants
                .into_iter()
                .map(|word| {
                    Ok(super::VariantWord {
                        span: spans.get(&word.span)?,
                        candidates: word
                            .candidates
                            .into_iter()
                            .map(|candidate| {
                                Ok(super::VariantCandidate {
                                    word: lemma(lemmas, candidate.word)?,
                                    replaceable: candidate.replaceable,
                                })
                            })
                            .collect::<Result<_, String>>()?,
                    })
                })
                .collect::<Result<_, String>>()?;
            Ok(super::ParagraphDigest {
                version: digest.version,
                len: digest.len,
                starts_with_line_break: digest.flags & 1 != 0,
                ends_with_line_break: digest.flags & 2 != 0,
                starts_paragraph: digest.flags & 4 != 0,
                ends_paragraph: digest.flags & 8 != 0,
                paragraph_starts: digest.paragraph_starts,
                apostrophes,
                too_often_used,
                repeated_words: super::RepeatedWords {
                    count: digest.repeated_count,
                    words,
                    last_sentence: digest.repeated_last_sentence,
                },
                variants,
            })
        }
    }
}

/// Whether `text` holds a blank line, which ends a LanguageTool paragraph.
fn has_blank_line(text: &str) -> bool {
    text.contains("\n\n") || text.contains("\n\r\n")
}

/// Whether a paragraph ends between the text of `a` and that of `b`, which follows it.
fn breaks_between(a: &ParagraphDigest, b: &ParagraphDigest) -> bool {
    a.ends_paragraph || b.starts_paragraph || (a.ends_with_line_break && b.starts_with_line_break)
}

/// The digests grouped into LanguageTool paragraphs, as ranges of digest indices. Empty digests
/// don't separate the digests around them. A digest whose text holds several paragraphs is not
/// split: rules that look at paragraph starts also read its `paragraph_starts`.
pub(crate) fn lt_paragraphs(digests: &[ParagraphDigest]) -> Vec<Range<usize>> {
    let mut paragraphs = Vec::new();
    let mut start = 0;
    // The last digest with text, standing for the text before the next one.
    let mut last: Option<ParagraphDigest> = None;
    for (i, digest) in digests.iter().enumerate() {
        if let Some(previous) = &last {
            if i > start && digest.len.char > 0 && breaks_between(previous, digest) {
                paragraphs.push(start..i);
                start = i;
            }
        }
        match &mut last {
            Some(previous) if digest.len.char == 0 => {
                previous.ends_paragraph |= digest.ends_paragraph
            }
            _ => last = Some(digest.clone()),
        }
    }
    if start < digests.len() {
        paragraphs.push(start..digests.len());
    }
    paragraphs
}

/// How a [`DocumentMatch`] turns its paragraph's text into a replacement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DocumentFix {
    /// Replace every `'` with `’` (`to_typographic`) or every `’` with `'`. `first` is the char
    /// index of the first apostrophe in the token.
    SwapApostrophes { to_typographic: bool, first: u8 },
    /// No replacement (`TOO_OFTEN_USED_*`).
    Nothing,
    /// These replacements (`EN_REPEATEDWORDS`'s synonyms).
    Replacements(Vec<String>),
    /// Replace `word`, the spelling the token holds, with `other` (`EN_WORD_COHERENCY`).
    Variant { word: String, other: String },
}

/// The result of [`Rules::resolve_document`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DocumentMatches {
    /// The matches, sorted by paragraph and then by position.
    pub matches: Vec<DocumentMatch>,
    /// The indices of the digests that are not [current](ParagraphDigest::is_current) and so
    /// contributed nothing: check those paragraphs again and resolve again. While any are listed,
    /// `matches` can miss matches in every paragraph, not only in those.
    pub stale: Vec<usize>,
}

impl DocumentMatches {
    /// The matches in paragraph `paragraph`, in position order.
    pub fn in_paragraph(&self, paragraph: usize) -> &[DocumentMatch] {
        let start = self.matches.partition_point(|x| x.paragraph < paragraph);
        let end = self.matches.partition_point(|x| x.paragraph <= paragraph);
        &self.matches[start..end]
    }

    /// The suggestions of paragraph `paragraph`, whose text is `text`: `checked`, the suggestions
    /// [`Rules::check_paragraph`] returned for it, merged with the whole-text matches in it
    /// ([`merge_document_suggestions`]). Spans are relative to `text`. A match whose span no
    /// longer fits `text` ([`DocumentMatch::to_suggestion`]) is left out.
    pub fn suggestions(
        &self,
        paragraph: usize,
        text: &str,
        checked: Vec<Suggestion>,
    ) -> Vec<Suggestion> {
        let document = self
            .in_paragraph(paragraph)
            .iter()
            .filter_map(|x| x.to_suggestion(text))
            .collect();
        merge_document_suggestions(checked, document)
    }
}

/// A match of a whole-text rule, from [`Rules::resolve_document`].
///
/// Its span is relative to the start of its paragraph's text. The replacements depend on that
/// text, which the digests leave out: [`DocumentMatch::to_suggestion`] computes them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMatch {
    paragraph: usize,
    source: String,
    message: String,
    span: Span,
    fix: DocumentFix,
}

impl DocumentMatch {
    pub(crate) fn new(
        paragraph: usize,
        source: String,
        message: String,
        span: Span,
        fix: DocumentFix,
    ) -> Self {
        DocumentMatch {
            paragraph,
            source,
            message,
            span,
            fix,
        }
    }

    /// The index of the paragraph (the digest) this match is in.
    pub fn paragraph(&self) -> usize {
        self.paragraph
    }

    /// The source of the match, e.g. `"BUILTIN/EN_CONSISTENT_APOS"`, as [`Suggestion::source`].
    pub fn source(&self) -> &str {
        &self.source
    }

    /// The human-readable message.
    pub fn message(&self) -> &str {
        &self.message
    }

    /// The span of the match, relative to the start of its paragraph's text.
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// The suggestion for this match in `paragraph`, the text its digest was built from, with the
    /// span still relative to the paragraph. `None` if `paragraph` visibly isn't that text: the
    /// span doesn't fit it (in bytes or chars), or doesn't hold a token like the one the match is
    /// about (for `EN_CONSISTENT_APOS`: the same apostrophe style, with the first apostrophe at
    /// the same place; for `EN_WORD_COHERENCY`: a spelling it can replace). Replace a
    /// paragraph's digest whenever its text changes; this check is a guard, not a substitute.
    pub fn to_suggestion(&self, paragraph: &str) -> Option<Suggestion> {
        let bytes = self.span.byte().clone();
        let original = paragraph.get(bytes.clone())?;
        if original.chars().count() != self.span.char().len()
            || paragraph[..bytes.start].chars().count() != self.span.char().start
        {
            return None;
        }
        self.suggestion_for(original)
    }

    /// The suggestion for this match, given `original`, the text of its span.
    pub(crate) fn suggestion_for(&self, original: &str) -> Option<Suggestion> {
        if original.is_empty() {
            return None;
        }
        let replacements = match &self.fix {
            DocumentFix::SwapApostrophes {
                to_typographic,
                first,
            } => {
                if en::apostrophe_style(original) != Some(!to_typographic)
                    || first_apostrophe(original) != *first
                {
                    return None;
                }
                vec![if *to_typographic {
                    original.replace('\'', "\u{2019}")
                } else {
                    original.replace('\u{2019}', "'")
                }]
            }
            DocumentFix::Nothing => Vec::new(),
            DocumentFix::Replacements(replacements) => replacements.clone(),
            DocumentFix::Variant { word, other } => {
                vec![en::word_coherency::replacement(original, word, other)?]
            }
        };
        let suggestion = Suggestion::new(
            self.source.clone(),
            self.message.clone(),
            self.span.clone(),
            replacements,
        );
        Some(match self.fix {
            // As other rules' suggestions in `Rules::suggest`.
            DocumentFix::Replacements(_) | DocumentFix::Variant { .. } => {
                suggestion.match_apostrophes(original)
            }
            _ => suggestion,
        })
    }
}

/// The char index of the first `'` or `’` in `token`, at most 255.
pub(crate) fn first_apostrophe(token: &str) -> u8 {
    token
        .chars()
        .position(|c| c == '\'' || c == '\u{2019}')
        .map_or(u8::MAX, |x| x.min(u8::MAX as usize) as u8)
}

impl Builtin {
    /// Whether this rule needs the whole text and runs in [`Rules::resolve_document`].
    pub(crate) fn is_document_rule(&self) -> bool {
        matches!(
            self,
            Builtin::ConsistentApostrophes(_)
                | Builtin::TooOftenUsed(_)
                | Builtin::RepeatedWords(_)
                | Builtin::WordCoherency(_)
        )
    }

    /// Adds what this whole-text rule needs from `sentence` to `digest`, whose text starts at
    /// `start`.
    pub(crate) fn digest_sentence(
        &self,
        sentence: &Sentence,
        start: Position,
        digest: &mut ParagraphDigest,
    ) {
        match self {
            Builtin::ConsistentApostrophes(_) => {
                let sentence_start = sentence.span().start().byte;
                for token in sentence.tokens() {
                    let span = token.span();
                    // The token's original text: its word may hold the normalized form.
                    let original = sentence
                        .text()
                        .get(span.byte().start - sentence_start..span.byte().end - sentence_start);
                    if let Some(original) = original {
                        if let Some(typographic) = en::apostrophe_style(original) {
                            digest.apostrophes.push(Apostrophe {
                                span: TokenSpan::new(span, start),
                                typographic,
                                first: first_apostrophe(original),
                            });
                        }
                    }
                }
            }
            Builtin::TooOftenUsed(rule) => rule.digest(
                sentence,
                start,
                &mut digest.too_often_used[rule.class_index()],
            ),
            Builtin::RepeatedWords(rule) => {
                rule.digest(sentence, start, &mut digest.repeated_words)
            }
            Builtin::WordCoherency(rule) => rule.digest(sentence, start, &mut digest.variants),
            _ => {}
        }
    }

    /// The matches of this whole-text rule in the document made of `digests`, of which only the
    /// current ones count.
    pub(crate) fn resolve_document(&self, digests: &[ParagraphDigest]) -> Vec<DocumentMatch> {
        let current = || {
            digests
                .iter()
                .enumerate()
                .filter(|(_, digest)| digest.is_current())
        };
        match self {
            Builtin::ConsistentApostrophes(rule) => rule.resolve(digests),
            Builtin::TooOftenUsed(rule) => rule.resolve(current()),
            Builtin::RepeatedWords(rule) => {
                // The last sentence of the document is skipped when it ends with whitespace.
                let last = current()
                    .rev()
                    .find(|(_, d)| d.len.char > 0)
                    .map(|(i, _)| i);
                rule.resolve(current(), last)
            }
            Builtin::WordCoherency(rule) => rule.resolve(current()),
            _ => Vec::new(),
        }
    }

    /// The suggestions of this whole-text rule for `sentences`, taken as the whole text.
    pub(crate) fn check_document(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let (Some(first), Some(last)) = (sentences.first(), sentences.last()) else {
            return Vec::new();
        };
        let start = first.span().start();
        let mut digest = ParagraphDigest::default();
        for sentence in sentences {
            self.digest_sentence(sentence, start, &mut digest);
        }
        digest.len = Offset::new(last.span().end() - start);

        let document = self
            .resolve_document(std::slice::from_ref(&digest))
            .into_iter()
            .filter_map(|x| {
                let span = x.span().clone().rshift(start);
                let sentence = sentences.iter().find(|sentence| {
                    sentence.span().byte().start <= span.byte().start
                        && span.byte().end <= sentence.span().byte().end
                })?;
                let offset = sentence.span().byte().start;
                let original = sentence
                    .text()
                    .get(span.byte().start - offset..span.byte().end - offset)?;
                Some(x.suggestion_for(original)?.rshift(start))
            })
            .collect();
        merge_document_suggestions(Vec::new(), document)
    }
}

impl Rules {
    /// Whether any enabled builtin needs the whole text. Without one, [`Rules::suggest`] builds
    /// no digests.
    pub(crate) fn document_rules_enabled(&self) -> bool {
        self.builtins
            .iter()
            .any(|rule| rule.enabled() && rule.builtin().is_document_rule())
    }

    /// The digest of `text`, one engine paragraph starting at `start` in the checked text, for the
    /// whole-text builtins: the enabled ones, or all of them with `all`.
    pub(crate) fn digest(
        &self,
        text: &str,
        sentences: &[Sentence],
        start: Position,
        all: bool,
    ) -> ParagraphDigest {
        let rules = self
            .builtins
            .iter()
            .filter(move |rule| (all || rule.enabled()) && rule.builtin().is_document_rule())
            .map(|rule| rule.builtin());
        ParagraphDigest::from_sentences(text, sentences, start, rules)
    }

    /// Runs the enabled whole-text rules on a document made of `digests`, one per paragraph in
    /// text order. Matches are sorted by paragraph and then by position, and are relative to their
    /// paragraph. The result replaces any earlier one for the document: one changed paragraph can
    /// add or remove matches in others. Digests that are not [current](ParagraphDigest::is_current)
    /// contribute nothing and are listed in [`DocumentMatches::stale`].
    ///
    /// Turn each match into a suggestion with [`DocumentMatch::to_suggestion`] and merge those
    /// into the paragraph's suggestions with [`merge_document_suggestions`], which also drops a
    /// match that touches another of the same rule, as LanguageTool does;
    /// [`DocumentMatches::suggestions`] does both for one paragraph.
    pub fn resolve_document(&self, digests: &[ParagraphDigest]) -> DocumentMatches {
        let mut matches: Vec<DocumentMatch> = self
            .builtins
            .iter()
            .filter(|rule| rule.enabled() && rule.builtin().is_document_rule())
            .flat_map(|rule| rule.builtin().resolve_document(digests))
            .collect();
        matches.sort_by_key(|x| (x.paragraph, x.span.char().start));
        let stale = digests
            .iter()
            .enumerate()
            .filter(|(_, digest)| !digest.is_current())
            .map(|(i, _)| i)
            .collect();
        DocumentMatches { matches, stale }
    }

    /// [`Rules::resolve_document`] over digests stored with [`ParagraphDigest::to_bytes`]. Bytes
    /// that [`ParagraphDigest::from_bytes`] rejects (another [`DIGEST_VERSION`], or not a digest)
    /// count as a digest that is not current: listed in [`DocumentMatches::stale`], so the app
    /// checks those paragraphs again.
    pub fn resolve_document_bytes<B: AsRef<[u8]>>(&self, digests: &[B]) -> DocumentMatches {
        let digests: Vec<ParagraphDigest> = digests
            .iter()
            .map(|bytes| {
                ParagraphDigest::from_bytes(bytes.as_ref())
                    .unwrap_or_else(|_| ParagraphDigest::stale())
            })
            .collect();
        self.resolve_document(&digests)
    }

    /// Checks one paragraph of a larger document on its own and returns its suggestions, without
    /// those of the whole-text rules, and its digest. Spans are relative to `paragraph`.
    ///
    /// `ends_paragraph` says that the end of `paragraph` ends a paragraph, whatever its text ends
    /// with. An editor that keeps each paragraph without the separator after it passes `true`.
    /// With `false`, a paragraph ends only where the text has a blank line (as in LanguageTool),
    /// which suits an app that passes slices of a document with their line breaks, e.g. one line
    /// at a time.
    ///
    /// The digest covers every whole-text builtin, enabled or not, so it stays valid when rules
    /// are toggled or their options change. Pass the digests of all paragraphs to
    /// [`Rules::resolve_document`]. The whole-text matches equal those [`Rules::suggest`] finds in
    /// the joined text, when the pieces end between sentences (see the [module docs](self)); the
    /// other suggestions only see this paragraph.
    pub fn check_paragraph(
        &self,
        paragraph: &str,
        ends_paragraph: bool,
        tokenizer: &Tokenizer,
    ) -> (Vec<Suggestion>, ParagraphDigest) {
        let mut digests = Vec::new();
        let suggestions = self.suggest_paragraphs(paragraph, tokenizer, Some((&mut digests, true)));
        let mut digest = ParagraphDigest::default();
        for (_, next) in digests {
            digest.append(next);
        }
        // The ends of `paragraph` decide these, also when it has no sentence (whitespace only)
        // or text after its last sentence.
        digest.set_ends(paragraph);
        digest.ends_paragraph |= ends_paragraph;
        (suggestions, digest)
    }

    /// Adds the matches of the enabled whole-text rules on `text`, whose engine paragraphs start
    /// at the given positions and have the given digests, to `suggestions`, the other rules'
    /// suggestions on it.
    pub(crate) fn add_document_suggestions(
        &self,
        text: &str,
        suggestions: Vec<Suggestion>,
        paragraphs: Vec<(Position, ParagraphDigest)>,
    ) -> Vec<Suggestion> {
        // One digest per LanguageTool paragraph.
        let groups = lt_paragraphs(
            &paragraphs
                .iter()
                .map(|(_, digest)| digest.clone())
                .collect::<Vec<_>>(),
        );
        let mut starts = Vec::with_capacity(groups.len());
        let mut digests = Vec::with_capacity(groups.len());
        let mut paragraphs = paragraphs.into_iter();
        for group in groups {
            let mut digest = ParagraphDigest::default();
            let mut start = None;
            for (position, next) in paragraphs.by_ref().take(group.len()) {
                start.get_or_insert(position);
                digest.append(next);
            }
            starts.push(start.unwrap_or_default());
            digests.push(digest);
        }

        let document: Vec<Suggestion> = self
            .resolve_document(&digests)
            .matches
            .into_iter()
            .filter_map(|x| {
                let start: Position = starts[x.paragraph];
                let end = start + digests[x.paragraph].text_len();
                let paragraph = text.get(start.byte..end.byte)?;
                Some(x.to_suggestion(paragraph)?.rshift(start))
            })
            .collect();
        merge_document_suggestions(suggestions, document)
    }
}

/// Merges whole-text suggestions into a text's other suggestions the way [`Rules::suggest`] does.
///
/// Both lists must use the same coordinates (e.g. both relative to one paragraph). The result is
/// sorted by start. Like LanguageTool, it keeps a whole-text suggestion that overlaps one of
/// another rule, and applies `SameRuleGroupFilter` to the whole-text suggestions: in the list of
/// all suggestions sorted by start, one that overlaps or touches the kept one right before it, of
/// the same rule, is dropped (`O’Brien` and `’s` in `O’Brien’s` give one match).
pub fn merge_document_suggestions(
    suggestions: Vec<Suggestion>,
    document: Vec<Suggestion>,
) -> Vec<Suggestion> {
    if document.is_empty() {
        return suggestions;
    }
    let mut all: Vec<(Suggestion, bool)> = suggestions
        .into_iter()
        .map(|x| (x, false))
        .chain(document.into_iter().map(|x| (x, true)))
        .collect();
    all.sort_by_key(|(x, _)| x.span().char().start);

    let mut out = Vec::with_capacity(all.len());
    let mut all = all.into_iter().peekable();
    while let Some((kept, _)) = all.next() {
        while all.peek().is_some_and(|(next, is_document)| {
            *is_document
                && next.source() == kept.source()
                && kept.span().char().start <= next.span().char().end
                && kept.span().char().end >= next.span().char().start
        }) {
            all.next();
        }
        out.push(kept);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(text: &str) -> ParagraphDigest {
        let mut digest = ParagraphDigest::default();
        digest.set_ends(text);
        digest
    }

    fn span(byte: Range<u32>, char: Range<u32>) -> TokenSpan {
        TokenSpan {
            start: Offset {
                byte: byte.start,
                char: char.start,
            },
            end: Offset {
                byte: byte.end,
                char: char.end,
            },
        }
    }

    fn counted(span: TokenSpan, lemma: &str, speech: u8) -> CountedWord {
        CountedWord {
            span,
            lemma: lemma.into(),
            speech,
        }
    }

    /// A digest with something in every list, with a multi-byte char before its spans.
    fn full_digest() -> ParagraphDigest {
        let mut d = digest("\u{201c}Cat\u{2019}s cat\u{2014}cat\u{2026}\n");
        d.paragraph_starts.push(3);
        d.apostrophes.push(Apostrophe {
            span: span(6..10, 4..6),
            typographic: true,
            first: 0,
        });
        d.too_often_used[1].words = vec![
            counted(span(3..6, 1..4), "cat", 0b10),
            counted(span(11..14, 7..10), "cat", 0b10),
            counted(span(17..20, 11..14), "cat", 0),
        ];
        d.too_often_used[1].speech_after = [false, false];
        d.repeated_words = RepeatedWords {
            count: 4,
            words: vec![RepeatedWord {
                span: span(11..14, 7..10),
                number: 3,
                starts_sentence: true,
                readings: vec![
                    RepeatedReading {
                        lemma: "cat".into(),
                        replacements: None,
                    },
                    RepeatedReading {
                        lemma: "cats".into(),
                        replacements: Some(vec!["dog".into()]),
                    },
                ],
            }],
            last_sentence: Some(0),
        };
        d.variants.push(VariantWord {
            span: span(17..20, 11..14),
            candidates: vec![VariantCandidate {
                word: "cat".into(),
                replaceable: 0b101,
            }],
        });
        d
    }

    #[test]
    fn digests_round_trip_through_bytes() {
        let d = full_digest();
        let bytes = d.to_bytes();
        assert_eq!(ParagraphDigest::from_bytes(&bytes).unwrap(), d);
        // A lemma is stored once, so a longer one adds its length once.
        let mut longer = d.clone();
        for word in longer.too_often_used[1].words.iter_mut() {
            word.lemma = "catamaran".into();
        }
        longer.repeated_words.words[0].readings[0].lemma = "catamaran".into();
        longer.variants[0].candidates[0].word = "catamaran".into();
        assert_eq!(longer.to_bytes().len(), bytes.len() + 6);
        // Serde with another format sees the same digest.
        let options = bincode::DefaultOptions::new();
        let back: ParagraphDigest = options
            .deserialize(&options.serialize(&d).unwrap())
            .unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn malformed_digest_bytes_are_errors() {
        let bytes = full_digest().to_bytes();
        for end in 4..bytes.len() {
            assert!(
                ParagraphDigest::from_bytes(&bytes[..end]).is_err(),
                "{}",
                end
            );
        }
        // Every one-byte change either fails or reads a digest; none panics.
        for i in 4..bytes.len() {
            for value in [0u8, 1, 0x7f, 0xfb, 0xff] {
                let mut changed = bytes.clone();
                changed[i] = value;
                let _ = ParagraphDigest::from_bytes(&changed);
            }
        }
    }

    #[test]
    fn appends_word_statistics() {
        let mut a = digest("0123456789");
        a.too_often_used[0].words = vec![counted(span(0..3, 0..3), "red", 0b10)];
        // `a` opens direct speech and leaves it open.
        a.too_often_used[0].speech_after = [true, true];
        a.repeated_words = RepeatedWords {
            count: 2,
            words: Vec::new(),
            last_sentence: Some(0),
        };
        let mut b = digest("0123");
        // Outside speech if `b` starts outside it, inside if it starts inside.
        b.too_often_used[0].words = vec![counted(span(0..3, 0..3), "red", 0b10)];
        b.too_often_used[0].speech_after = [false, true];
        b.repeated_words = RepeatedWords {
            count: 3,
            words: vec![RepeatedWord {
                span: span(1..2, 1..2),
                number: 1,
                starts_sentence: true,
                readings: Vec::new(),
            }],
            last_sentence: None,
        };
        a.append(b);
        let words = &a.too_often_used[0];
        assert_eq!(words.words[1].span, span(10..13, 10..13));
        // Wherever `a` starts, `b`'s word is in speech now.
        assert_eq!(words.words[1].speech, 0b11);
        assert_eq!(words.speech_after, [true, true]);
        assert_eq!(a.repeated_words.count, 5);
        assert_eq!(a.repeated_words.words[0].number, 3);
        assert_eq!(a.repeated_words.words[0].span, span(11..12, 11..12));
        assert_eq!(a.repeated_words.last_sentence, None);
    }

    #[test]
    fn groups_lt_paragraphs() {
        // A blank line ends the first paragraph, also with spaces before it; a single line break
        // does not end the second; a line break on each side of a boundary makes a blank line;
        // a line holding only spaces is not blank.
        let digests = [
            digest("One.  \n\n"),
            digest("Two.\n"),
            digest("Three.\n"),
            digest("\nFour.\n"),
            digest(" \nFive."),
        ];
        assert_eq!(lt_paragraphs(&digests), vec![0..1, 1..3, 3..5]);
        assert!(lt_paragraphs(&[]).is_empty());
        // An empty digest between two halves of a blank line does not hide it.
        let digests = [digest("One.\n"), digest(""), digest("\nTwo.")];
        assert_eq!(lt_paragraphs(&digests), vec![0..2, 2..3]);
        // Leading blank lines end the paragraph before.
        let digests = [digest("One."), digest("  \n\n  Two.")];
        assert_eq!(lt_paragraphs(&digests), vec![0..1, 1..2]);
    }

    #[test]
    fn appends_digests() {
        let mut a = digest("0123456789");
        a.apostrophes.push(Apostrophe {
            span: span(1..4, 1..3),
            typographic: true,
            first: 0,
        });
        let mut b = digest("\n\n2345");
        b.apostrophes.push(Apostrophe {
            span: span(2..4, 2..4),
            typographic: false,
            first: 0,
        });
        a.append(b);
        assert_eq!(a.text_len(), Position { byte: 16, char: 16 });
        assert_eq!(a.paragraph_starts, vec![10]);
        assert_eq!(a.apostrophes[1].span(), Span::new(12..14, 12..14));
    }

    #[test]
    fn versions() {
        let digest = digest("It's");
        let bytes = digest.to_bytes();
        assert_eq!(bytes[..4], DIGEST_VERSION.to_le_bytes());
        assert_eq!(ParagraphDigest::from_bytes(&bytes).unwrap(), digest);

        let mut old = bytes.clone();
        old[..4].copy_from_slice(&0u32.to_le_bytes());
        assert!(matches!(
            ParagraphDigest::from_bytes(&old),
            Err(Error::Format(_))
        ));
        assert!(matches!(
            ParagraphDigest::from_bytes(&[1]),
            Err(Error::Format(_))
        ));
        assert!(ParagraphDigest::from_bytes(&bytes[..6]).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(matches!(
            ParagraphDigest::from_bytes(&trailing),
            Err(Error::Format(_))
        ));

        let stale = ParagraphDigest {
            version: 0,
            apostrophes: vec![Apostrophe {
                span: span(0..1, 0..1),
                typographic: false,
                first: 0,
            }],
            ..digest
        };
        assert!(!stale.is_current());
        assert!(stale.apostrophes().is_empty());
    }

    #[test]
    fn suggestion_needs_the_digested_text() {
        let m = |span: Span, to_typographic, first| {
            DocumentMatch::new(
                0,
                "BUILTIN/EN_CONSISTENT_APOS".into(),
                "m".into(),
                span,
                DocumentFix::SwapApostrophes {
                    to_typographic,
                    first,
                },
            )
        };
        // `n’t` in "a n’t".
        let nt = m(Span::new(2..7, 2..5), false, 1);
        assert_eq!(
            nt.to_suggestion("a n\u{2019}t").unwrap().replacements(),
            ["n't"]
        );
        assert!(nt.to_suggestion("n\u{2019}t").is_none());
        assert!(nt.to_suggestion("").is_none());
        // The same bytes now hold a straight apostrophe, or a shifted token.
        assert!(nt.to_suggestion("a n't!!").is_none());
        assert!(nt.to_suggestion("an\u{2019}t").is_none());
        assert!(nt.to_suggestion("xx\u{2019}st").is_none());
        // A token split inside a word, as `does` + `n’t`.
        assert!(nt.to_suggestion("don\u{2019}t").is_some());
        // `’s` of "I'm fine. It’s ok." after the paragraph became "I am fine. It’s ok.": the
        // span now starts inside "It".
        let s = m(Span::new(12..16, 12..14), false, 0);
        assert!(s.to_suggestion("I'm fine. It\u{2019}s ok.").is_some());
        assert!(s.to_suggestion("I am fine. It\u{2019}s ok.").is_none());
        // Not on a char boundary, or a different char count before the span.
        assert!(m(Span::new(3..5, 3..5), false, 0)
            .to_suggestion("a n\u{2019}t")
            .is_none());
        assert!(m(Span::new(5..9, 3..5), false, 0)
            .to_suggestion("\u{1F600}a\u{2019}s")
            .is_none());
        // A typewriter match needs a straight apostrophe.
        let ll = m(Span::new(1..4, 1..4), true, 0);
        assert_eq!(
            ll.to_suggestion("I'll").unwrap().replacements(),
            ["\u{2019}ll"]
        );
        assert!(ll.to_suggestion("I\u{2019}l").is_none());
    }

    fn suggestion(source: &str, range: Range<usize>) -> Suggestion {
        Suggestion::new(
            source.into(),
            String::new(),
            Span::new(range.clone(), range),
            Vec::new(),
        )
    }

    #[test]
    fn merging_keeps_overlaps_of_other_rules() {
        let apos = "BUILTIN/EN_CONSISTENT_APOS";
        let merged = merge_document_suggestions(
            vec![
                suggestion("GRAMMAR/IT_IS_2", 15..19),
                suggestion("X", 29..40),
            ],
            vec![
                // Overlaps IT_IS_2: kept.
                suggestion(apos, 17..19),
                // Touches the one before: dropped.
                suggestion(apos, 19..21),
                suggestion(apos, 22..29),
                // Touches 22..29, but "X" comes between them in the sorted list: kept.
                suggestion(apos, 29..31),
            ],
        );
        let spans: Vec<_> = merged
            .iter()
            .map(|x| (x.source(), x.span().char().clone()))
            .collect();
        assert_eq!(
            spans,
            vec![
                ("GRAMMAR/IT_IS_2", 15..19),
                (apos, 17..19),
                (apos, 22..29),
                ("X", 29..40),
                (apos, 29..31)
            ]
        );
    }
}
