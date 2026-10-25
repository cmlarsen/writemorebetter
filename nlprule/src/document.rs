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
//! The paragraph rules (`READABILITY_RULE_SIMPLE`, `READABILITY_RULE_DIFFICULT`,
//! `PARAGRAPH_REPEAT_BEGINNING_RULE`) follow LanguageTool's sentence-level test instead: a
//! paragraph ends after a sentence whose text ends with a blank line or before one whose text
//! starts with a line break (`Tools.isParagraphEnd`). A digest groups its sentences into such
//! paragraphs, with the counts and first words those rules need, and leaves the last one open
//! for the resolver to join with the next digest's. Their results equal the whole text's for a
//! split between sentences (by line, by sentence, at blank lines) of ordinary text. Where the
//! split falls in whitespace, the resolver has to guess how the sentencizer would have split
//! the joined text (see [`Lead`]), and some rare runs come out differently: a tab right after a
//! blank line (kept with the sentence before it in a whole text), line breaks that mix a bare LF
//! with CRLF, and whitespace before the line break. A readability match whose first three words
//! are not in one piece ends with the first piece; with the separators stripped
//! (`ends_paragraph`), whitespace at the end of a paragraph, which LanguageTool can count as a
//! sentence, must stay in the piece. A piece of only ASCII whitespace adds nothing, as a blank
//! line holding spaces adds nothing to a whole text.
//!
//! # Versions
//! # Versions
//!
//! Digests hold offsets, counts, flags, the lemmas the word-statistics rules count (as strings,
//! so a digest does not depend on the tagger's word ids) and hashes of the first words of
//! paragraphs, never the paragraph's text itself. Their
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
///
/// Version 3: the paragraph rules (`READABILITY_RULE_SIMPLE`, `READABILITY_RULE_DIFFICULT`,
/// `PARAGRAPH_REPEAT_BEGINNING_RULE`).
pub const DIGEST_VERSION: u32 = 3;

/// A compact text position: byte and char offset relative to the start of a digest's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Offset {
    pub(crate) byte: u32,
    pub(crate) char: u32,
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

    pub(crate) fn position(self) -> Position {
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
    /// The index in `words` of the first word of the text's last sentence, if the rule counts
    /// that sentence (it ends in `.`, `!` or `?`). The rule skips it when it ends the whole text
    /// and whitespace follows it.
    pub(crate) last_sentence: Option<u32>,
    /// Whether whitespace follows the last sentence within the text.
    pub(crate) trailing_whitespace: bool,
    /// Whether the text is only line breaks, which LanguageTool adds to the sentence before them.
    /// Other whitespace after a blank line, by contrast, makes a sentence of its own.
    pub(crate) line_breaks_only: bool,
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
        if other.line_breaks_only {
            self.trailing_whitespace = true;
        } else {
            self.last_sentence = other.last_sentence.map(|x| x + words);
            self.trailing_whitespace = other.trailing_whitespace;
        }
        self.line_breaks_only &= other.line_breaks_only;
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

/// An offset in the digest at index `digest` of a document, as [`Rules::resolve_document`] sees
/// it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct At {
    pub(crate) digest: usize,
    pub(crate) offset: Offset,
}

/// The run of CR and LF chars that ends a sentence's text, its last four at most: enough to tell
/// whether the text ends with a blank line as LanguageTool's `Tools.isParagraphEnd` sees it. The
/// low three bits hold the length, bit `3 + i` whether the `i`th char (oldest first) is a CR.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LineEnd(u8);

impl LineEnd {
    const MAX: usize = 4;

    fn from_chars(chars: &[char]) -> Self {
        let chars = &chars[chars.len().saturating_sub(Self::MAX)..];
        let mut bits = chars.len() as u8;
        for (i, c) in chars.iter().enumerate() {
            if *c == '\r' {
                bits |= 1 << (3 + i);
            }
        }
        LineEnd(bits)
    }

    /// The run of CR and LF chars at the end of `text`.
    fn of(text: &str) -> Self {
        let mut chars: Vec<char> = text
            .chars()
            .rev()
            .take_while(|c| matches!(c, '\r' | '\n'))
            .take(Self::MAX)
            .collect();
        chars.reverse();
        Self::from_chars(&chars)
    }

    fn chars(self) -> Vec<char> {
        (0..self.len().min(Self::MAX))
            .map(|i| {
                if self.0 & (1 << (3 + i)) != 0 {
                    '\r'
                } else {
                    '\n'
                }
            })
            .collect()
    }

    /// The run of `self` followed by that of `other`.
    fn then(self, other: LineEnd) -> Self {
        let mut chars = self.chars();
        chars.extend(other.chars());
        Self::from_chars(&chars)
    }

    /// The number of chars in the run.
    fn len(self) -> usize {
        (self.0 & 7) as usize
    }

    fn ends_with_lf(self) -> bool {
        self.chars().last() == Some(&'\n')
    }

    /// Whether the text ends with a blank line, `\n\n`, `\n\r\n\r` or `\r\n\r\n`, as LanguageTool
    /// checks.
    fn is_blank(self) -> bool {
        let text: String = self.chars().into_iter().collect();
        text.ends_with("\n\n") || text.ends_with("\n\r\n\r") || text.ends_with("\r\n\r\n")
    }
}

/// One of the first tokens of a sentence, as `PARAGRAPH_REPEAT_BEGINNING_RULE` compares them:
/// its span, a hash of its text (digests hold no text) and what kind of token it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct BeginningToken<P> {
    pub(crate) start: P,
    pub(crate) end: P,
    /// The 64-bit FNV-1a hash of the token's text, with `’` normalized to `'` as the tagger does.
    pub(crate) hash: u64,
    /// The flags [`BeginningToken::QUOTE`], [`BeginningToken::LETTER`] and
    /// [`BeginningToken::DETERMINER`].
    kind: u8,
}

impl<P> BeginningToken<P> {
    const QUOTE: u8 = 1;
    const LETTER: u8 = 2;
    const DETERMINER: u8 = 4;

    pub(crate) fn new(
        start: P,
        end: P,
        text: &str,
        is_quote: bool,
        starts_with_letter: bool,
        is_determiner: bool,
    ) -> Self {
        let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
        });
        let kind = (u8::from(is_quote) * Self::QUOTE)
            | (u8::from(starts_with_letter) * Self::LETTER)
            | (u8::from(is_determiner) * Self::DETERMINER);
        BeginningToken {
            start,
            end,
            hash,
            kind,
        }
    }

    /// Whether the token is a single quote or bracket.
    pub(crate) fn is_quote(&self) -> bool {
        self.kind & Self::QUOTE != 0
    }

    /// Whether the token's first UTF-16 unit is a letter.
    pub(crate) fn starts_with_letter(&self) -> bool {
        self.kind & Self::LETTER != 0
    }

    /// Whether a reading of the token is tagged `DT…`.
    pub(crate) fn is_determiner(&self) -> bool {
        self.kind & Self::DETERMINER != 0
    }

    fn map<Q>(self, f: impl Fn(P) -> Q) -> BeginningToken<Q> {
        BeginningToken {
            start: f(self.start),
            end: f(self.end),
            hash: self.hash,
            kind: self.kind,
        }
    }
}

/// The first tokens of a sentence (at most three), for `PARAGRAPH_REPEAT_BEGINNING_RULE`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Beginning<P> {
    /// The length of the sentence's text before its first token.
    pub(crate) lead: Offset,
    pub(crate) tokens: Vec<BeginningToken<P>>,
}

impl<P> Beginning<P> {
    fn map<Q>(self, f: impl Fn(P) -> Q + Copy) -> Beginning<Q> {
        Beginning {
            lead: self.lead,
            tokens: self.tokens.into_iter().map(|x| x.map(f)).collect(),
        }
    }
}

/// Consecutive sentences of one LanguageTool paragraph, as the paragraph rules
/// (`READABILITY_RULE_*`, `PARAGRAPH_REPEAT_BEGINNING_RULE`) see them. A part that is not
/// `closed` may continue in the text after it. Positions are `P`: [`Offset`]s in a digest, [`At`]
/// in the resolver.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Part<P> {
    /// The sentences, counting whitespace-only ones as LanguageTool does.
    sentences: u32,
    /// The words and their syllables, as `READABILITY_RULE_*` counts them.
    words: u32,
    syllables: u32,
    /// The start of the first token of the first sentence with a token.
    mark_start: Option<P>,
    /// The end of the third token of the first sentence with at least three.
    mark_end: Option<P>,
    /// The first sentence's first tokens; `None` if it has no token.
    beginning: Option<Beginning<P>>,
    /// Whether the first sentence's text starts with a line break, which ends the paragraph
    /// before it.
    starts_with_line_break: bool,
    /// The line breaks that end the last sentence's text.
    line_end: LineEnd,
    /// Whether a paragraph ends after the part.
    closed: bool,
    /// The length of the part's text, and how many LF chars it holds (at most 2), if it is a
    /// single whitespace-only sentence that may still turn out to be the leading whitespace of
    /// the next sentence, see [`Lead`].
    whitespace: Option<(Offset, u8)>,
}

impl<P> Part<P> {
    fn map<Q>(self, f: impl Fn(P) -> Q + Copy) -> Part<Q> {
        Part {
            sentences: self.sentences,
            words: self.words,
            syllables: self.syllables,
            mark_start: self.mark_start.map(f),
            mark_end: self.mark_end.map(f),
            beginning: self.beginning.map(|x| x.map(f)),
            starts_with_line_break: self.starts_with_line_break,
            line_end: self.line_end,
            closed: self.closed,
            whitespace: self.whitespace,
        }
    }

    /// Adds `next`, the sentences right after this part's, to this part.
    fn join(&mut self, next: Part<P>) {
        self.sentences += next.sentences;
        self.words += next.words;
        self.syllables += next.syllables;
        if self.mark_start.is_none() {
            self.mark_start = next.mark_start;
        }
        if self.mark_end.is_none() {
            self.mark_end = next.mark_end;
        }
        self.line_end = next.line_end;
        self.closed = next.closed;
        self.whitespace = None;
    }

    /// Makes `whitespace`, a whitespace-only part right before this part, the leading whitespace
    /// of this part's first sentence.
    fn absorb(&mut self, whitespace: &Part<P>) {
        let Some((by, by_line_feeds)) = whitespace.whitespace else {
            return;
        };
        self.starts_with_line_break = whitespace.starts_with_line_break;
        if let Some(beginning) = &mut self.beginning {
            beginning.lead = beginning.lead.shift(by);
        }
        if let Some((len, line_feeds)) = &mut self.whitespace {
            // Two whitespace-only sentences make one: `"\r\n"` and `"\r\n"` end a paragraph.
            if len.char as usize == self.line_end.len() {
                self.line_end = whitespace.line_end.then(self.line_end);
                self.closed = self.line_end.is_blank();
            }
            *len = len.shift(by);
            *line_feeds = (*line_feeds + by_line_feeds).min(2);
        }
    }
}

impl Part<Offset> {
    /// The part made of `sentence` alone, in a digest whose text starts at `start` in the checked
    /// text.
    fn of(sentence: &Sentence, start: Position) -> Self {
        let offset = |position: Position| Offset::new(position - start);
        let tokens = sentence.tokens();
        let (words, syllables) = en::readability_count(sentence);
        let text = sentence.text();
        let line_end = LineEnd::of(text);
        Part {
            sentences: 1,
            words,
            syllables,
            mark_start: tokens.first().map(|x| offset(x.span().start())),
            mark_end: tokens.get(2).map(|x| offset(x.span().end())),
            beginning: en::paragraph_beginning(sentence, offset),
            starts_with_line_break: starts_with_line_break(text),
            line_end,
            closed: line_end.is_blank(),
            whitespace: None,
        }
    }
}

fn starts_with_line_break(text: &str) -> bool {
    text.starts_with('\n') || text.starts_with("\r\n")
}

/// The LF chars in `text`, at most 2.
fn line_feeds(text: &str) -> u8 {
    text.chars().filter(|c| *c == '\n').take(2).count() as u8
}

/// Whether `text` is made only of the whitespace Java's `\s` matches.
fn is_ascii_space(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{b}' | '\u{c}'))
}

/// How the sentences added by [`extend_parts`] follow those before them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seam {
    /// Both come from one run of the sentencizer over the text (one digest, or the engine
    /// paragraphs of [`Rules::suggest`]): every whitespace-only sentence is one LanguageTool
    /// has too.
    Exact,
    /// The text was split there and the pieces checked on their own: a whitespace-only sentence
    /// at the seam may be an artifact of the split, see [`Lead`].
    Split,
    /// Split, and the caller said a paragraph ends before the added sentences
    /// (`ends_paragraph`), so their separator was stripped.
    Sealed,
}

/// A whitespace-only sentence, which may belong to the sentence before it.
///
/// LanguageTool keeps a whitespace-only split of its sentence splitter as a sentence: it counts
/// for `READABILITY_RULE_*`, and one that starts a paragraph makes `PARAGRAPH_REPEAT_BEGINNING_RULE`
/// see no first word. Its splitter makes one only where a line-break rule matches, so it always
/// holds two line feeds (`"She ran.\n"`, `"\r\n\r\n"`, `"She hid."`; `"She ran.\n\n"`,
/// `"\u{a0}\n\n"`, `"She hid."`). Ours finds overlapping matches of a rule (see the `srx` crate)
/// and also splits off whitespace with fewer line feeds, which LanguageTool's next sentence
/// starts with (`"She ran.\u{a0}\n"`, `"\nShe hid."`); such a sentence becomes that leading
/// whitespace, see [`push_part`]. At the end of the text it stays a sentence, as in
/// LanguageTool.
///
/// A text split by the app adds whitespace-only sentences the whole text does not have. Where a
/// piece ends a sentence with a line break and the next piece starts with whitespace that holds
/// one (`"She ran.\n"`, `"\n"`), the whole text keeps both in one sentence (`"She ran.\n\n"`),
/// so the piece's first sentence is merged into the sentence before it, see [`Lead::merges_into`].
/// After a stripped separator (`ends_paragraph`) ASCII whitespace adds no sentence either: the
/// sentencizer keeps whitespace after a blank line with the sentence before it
/// (`"She ran.\n\n \n\n"`). Other whitespace, a no-break space, U+2028 or U+3000, is a sentence
/// of its own in a whole text too and is never merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Lead {
    len: Offset,
    line_end: LineEnd,
    /// Whether the sentence is made only of CR and LF chars.
    all_breaks: bool,
    /// Whether the sentence is made only of the whitespace Java's `\s` matches (space, tab, CR,
    /// LF, form feed, vertical tab), which the sentencizer keeps with the sentence before it
    /// after a blank line. Other whitespace (a no-break space, U+2028, U+3000) makes a sentence
    /// of its own in a whole text too.
    ascii: bool,
    starts_with_line_break: bool,
    /// Whether the sentence starts with an LF rather than a CRLF.
    starts_with_lf: bool,
    has_line_break: bool,
    /// The LF chars in the sentence, at most 2.
    line_feeds: u8,
}

impl Lead {
    /// The lead of a whitespace-only sentence, `None` for one with tokens.
    fn of(sentence: &Sentence) -> Option<Self> {
        let text = sentence.text();
        sentence.tokens().is_empty().then(|| Lead {
            len: Offset::of(text),
            line_end: LineEnd::of(text),
            all_breaks: text.chars().all(|c| matches!(c, '\r' | '\n')),
            ascii: is_ascii_space(text),
            starts_with_line_break: starts_with_line_break(text),
            starts_with_lf: text.starts_with('\n'),
            has_line_break: text.contains(['\r', '\n']),
            line_feeds: line_feeds(text),
        })
    }

    /// Whether this sentence belongs to `last`, the sentences before it. The splitter keeps an
    /// LF right after an LF, and a CRLF right after a CRLF, with the sentence before it
    /// (`"She ran.\n\n"`), but starts the next sentence with a CRLF after a bare LF
    /// (`"She ran.\n"`, `"\r\nShe hid."`).
    fn merges_into<P>(&self, last: &Part<P>) -> bool {
        let crlf = last.line_end.chars().ends_with(&['\r', '\n']);
        self.ascii
            && last.line_end.ends_with_lf()
            && (self.starts_with_lf
                || self.starts_with_line_break && crlf
                || self.has_line_break && last.line_end.is_blank())
    }

    fn into_part<P>(self) -> Part<P> {
        Part {
            sentences: 1,
            words: 0,
            syllables: 0,
            mark_start: None,
            mark_end: None,
            beginning: None,
            starts_with_line_break: self.starts_with_line_break,
            line_end: self.line_end,
            closed: self.line_end.is_blank(),
            whitespace: Some((self.len, self.line_feeds)),
        }
    }
}

/// Adds `lead` (if any) and then `more`, the sentences after those of `parts`, to `parts`,
/// joining the parts that belong to one LanguageTool paragraph: a paragraph ends after a sentence
/// that ends with a blank line or before one that starts with a line break
/// (`Tools.isParagraphEnd`), and at the end of the text. The result does not depend on where the
/// text was split, as long as each split is between sentences.
///
/// `seam` says how the text was split between `parts` and the added sentences. With
/// [`Seam::Exact`] nothing is merged. With [`Seam::Sealed`] a paragraph ends after `parts`, and
/// a `lead` made only of ASCII whitespace is dropped: in the whole text the stripped separator
/// and that whitespace both end the sentence before it, so it adds no sentence.
fn extend_parts<P>(parts: &mut Vec<Part<P>>, lead: Option<Lead>, more: Vec<Part<P>>, seam: Seam) {
    let mut more = more.into_iter().peekable();
    // Whether the next part is the first one after the split.
    let mut boundary = seam != Seam::Exact;
    if let Some(lead) = lead {
        match parts.last_mut() {
            Some(_) if seam == Seam::Sealed && lead.ascii => {}
            Some(last) if seam == Seam::Split && lead.merges_into(last) => {
                last.line_end = if lead.all_breaks {
                    last.line_end.then(lead.line_end)
                } else {
                    lead.line_end
                };
                // Whether the merged sentence ends a paragraph is decided anew: `"\n\n"` and
                // `" \r\n"` make `"…\n\n \r\n"`, which does not end with a blank line.
                last.closed = last.line_end.is_blank()
                    || more.peek().is_some_and(|x| x.starts_with_line_break);
                if let Some((len, line_feeds)) = &mut last.whitespace {
                    *len = len.shift(lead.len);
                    *line_feeds = (*line_feeds + lead.line_feeds).min(2);
                }
            }
            _ => {
                push_part(parts, lead.into_part(), boundary);
                boundary = false;
            }
        }
    }
    for part in more {
        push_part(parts, part, boundary);
        boundary = false;
    }
}

/// Adds `next`, the sentences after those of `parts`, to `parts`. With `boundary`, the text was
/// split between them.
fn push_part<P>(parts: &mut Vec<Part<P>>, mut next: Part<P>, boundary: bool) {
    let whitespace = parts
        .last()
        .filter(|x| !x.closed)
        .and_then(|x| x.whitespace);
    if whitespace.is_some_and(|(_, line_feeds)| boundary || line_feeds < 2) {
        // A whitespace-only sentence that ends no paragraph and that LanguageTool's splitter
        // does not make starts the next sentence, see [`Lead`]. At a split, any that ends no
        // paragraph does: the whole text has it at the start of the next sentence.
        if let Some(whitespace) = parts.pop() {
            next.absorb(&whitespace);
        }
    }
    match parts.last_mut() {
        Some(last) if !last.closed && !next.starts_with_line_break => last.join(next),
        Some(last) => {
            last.closed = true;
            parts.push(next);
        }
        None => parts.push(next),
    }
}

/// A LanguageTool paragraph of a document, from its [`ParagraphDigest`]s: what the paragraph
/// rules need of it.
#[derive(Debug, Clone)]
pub(crate) struct LtParagraph {
    pub(crate) sentences: u32,
    pub(crate) words: u32,
    pub(crate) syllables: u32,
    pub(crate) mark_start: Option<At>,
    pub(crate) mark_end: Option<At>,
    pub(crate) beginning: Option<Beginning<At>>,
}

/// The LanguageTool paragraphs of the document made of `digests`, in order. Digests of another
/// version contribute nothing. With `exact`, the digests' texts were split from one run of the
/// sentencizer ([`Rules::suggest`]), else each was checked on its own.
pub(crate) fn document_paragraphs(digests: &[ParagraphDigest], exact: bool) -> Vec<LtParagraph> {
    let mut parts: Vec<Part<At>> = Vec::new();
    let mut sealed = false;
    for (digest, x) in digests.iter().enumerate().filter(|(_, x)| x.is_usable()) {
        let more = x
            .parts
            .iter()
            .map(|part| part.clone().map(|offset| At { digest, offset }))
            .collect();
        let seam = match (exact, sealed) {
            (true, _) => Seam::Exact,
            (false, true) => Seam::Sealed,
            (false, false) => Seam::Split,
        };
        extend_parts(&mut parts, x.lead, more, seam);
        sealed = x.closes;
        if x.closes {
            if let Some(last) = parts.last_mut() {
                last.closed = true;
            }
        }
    }
    parts
        .into_iter()
        .map(|part| LtParagraph {
            sentences: part.sentences,
            words: part.words,
            syllables: part.syllables,
            mark_start: part.mark_start,
            mark_end: part.mark_end,
            beginning: part.beginning,
        })
        .collect()
}

/// What the whole-text rules need to know about one paragraph: offsets, counts, flags and the
/// lemmas the word-statistics rules count, no text.
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
    /// A whitespace-only first sentence that may belong to the sentence before the text, see
    /// [`Lead`] (`READABILITY_RULE_*`, `PARAGRAPH_REPEAT_BEGINNING_RULE`).
    lead: Option<Lead>,
    /// The text's sentences grouped into LanguageTool paragraphs, or the parts of them that are
    /// in this text, in order (`READABILITY_RULE_*`, `PARAGRAPH_REPEAT_BEGINNING_RULE`).
    parts: Vec<Part<Offset>>,
    /// Whether the caller of [`Rules::check_paragraph`] said that a paragraph ends after the text.
    closes: bool,
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
            lead: None,
            parts: Vec::new(),
            closes: false,
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
        if !digest.is_well_formed() {
            return Err(Error::Format(
                "paragraph digest is malformed; check the paragraph again".to_owned(),
            ));
        }
        Ok(digest)
    }

    /// Whether the digest's offsets are ordered and within its text, as every digest this
    /// engine makes is. One that is not (corrupted or tampered with) is not used, so that it
    /// can't make [`Rules::resolve_document`] build an inverted span or one past its text.
    fn is_well_formed(&self) -> bool {
        let len = self.len;
        // At least one and at most four bytes per char.
        let fits = |x: Offset| x.byte <= len.byte && x.char <= len.char && x.char <= x.byte;
        let ordered =
            |a: Offset, b: Offset| fits(a) && fits(b) && a.byte <= b.byte && a.char <= b.char;
        let span = |x: &TokenSpan| ordered(x.start, x.end);
        let line_end = |x: LineEnd| x.len() <= LineEnd::MAX;
        len.char <= len.byte
            && len.byte <= len.char.saturating_mul(4)
            && self.paragraph_starts.iter().all(|x| *x <= len.char)
            && self.apostrophes.iter().all(|x| span(&x.span))
            && self
                .too_often_used
                .iter()
                .all(|class| class.words.iter().all(|x| span(&x.span)))
            && self.repeated_words.words.iter().all(|x| span(&x.span))
            && self.variants.iter().all(|x| span(&x.span))
            && self
                .lead
                .is_none_or(|x| fits(x.len) && line_end(x.line_end))
            && self.parts.iter().all(|part| {
                part.mark_start.is_none_or(fits)
                    && part.mark_end.is_none_or(fits)
                    && match (part.mark_start, part.mark_end) {
                        (Some(start), Some(end)) => ordered(start, end),
                        _ => true,
                    }
                    && part.beginning.as_ref().is_none_or(|x| {
                        fits(x.lead)
                            && x.tokens.iter().all(|token| ordered(token.start, token.end))
                            && x.tokens
                                .windows(2)
                                .all(|pair| ordered(pair[0].end, pair[1].start))
                    })
                    && line_end(part.line_end)
                    && part.whitespace.is_none_or(|(x, _)| fits(x))
            })
    }

    /// Whether the digest is [current](ParagraphDigest::is_current) and well formed, and so
    /// contributes to [`Rules::resolve_document`].
    pub(crate) fn is_usable(&self) -> bool {
        self.is_current() && self.is_well_formed()
    }

    /// A digest that is not [current](ParagraphDigest::is_current), standing in for bytes that
    /// are not a digest of this version.
    fn stale() -> Self {
        ParagraphDigest {
            version: 0,
            ..ParagraphDigest::default()
        }
    }

    /// The apostrophe tokens, or none for a digest that is not usable.
    pub(crate) fn apostrophes(&self) -> &[Apostrophe] {
        if self.is_usable() {
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
        self.repeated_words.line_breaks_only =
            !text.is_empty() && text.chars().all(|c| c == '\n' || c == '\r');
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
        let paragraphs = rules.clone().any(Builtin::is_paragraph_rule);

        let mut previous_end: Option<usize> = None;
        for sentence in sentences {
            if paragraphs {
                match Lead::of(sentence) {
                    Some(lead) if digest.parts.is_empty() && digest.lead.is_none() => {
                        digest.lead = Some(lead)
                    }
                    // A whitespace-only sentence inside the text is one LanguageTool has too.
                    Some(lead) => push_part(&mut digest.parts, lead.into_part(), false),
                    None => push_part(&mut digest.parts, Part::of(sentence, start), false),
                }
            }
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
        let parts = std::mem::take(&mut other.parts)
            .into_iter()
            .map(|part| part.map(|offset| offset.shift(by)))
            .collect();
        extend_parts(&mut self.parts, other.lead, parts, Seam::Exact);
        self.closes = other.closes;
        self.len = self.len.shift(other.len);
        self.ends_with_line_break = other.ends_with_line_break;
        self.ends_paragraph = other.ends_paragraph;
    }
}

/// The serialized form of [`ParagraphDigest`]: lemmas are stored once per digest, and each span
/// is stored relative to the start of the one before it in its list, so that the variable-length
/// integers of [`ParagraphDigest::to_bytes`] are mostly one byte long.
mod wire {
    use super::{Lead, Offset, Part};
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
        /// `starts_with_line_break`, `ends_with_line_break`, `starts_paragraph`,
        /// `ends_paragraph`, `repeated_words.trailing_whitespace` and `.line_breaks_only`, and
        /// `closes`, as bits 0 to 6.
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
        lead: Option<Lead>,
        parts: Vec<Part<Offset>>,
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
            if !valid_offset(span.gap) || !valid_offset(span.len) {
                return Err("paragraph digest span is not a span of UTF-8 text".to_owned());
            }
            let overflow = || "paragraph digest span overflows".to_owned();
            let start = self.0.checked_add(span.gap).ok_or_else(overflow)?;
            let end = start.checked_add(span.len).ok_or_else(overflow)?;
            self.0 = start;
            Ok(super::TokenSpan { start, end })
        }
    }

    /// Whether `offset` can be the length of UTF-8 text: each char takes one to four bytes. The
    /// resolver relies on this, e.g. to order positions.
    fn valid_offset(offset: Offset) -> bool {
        offset.char <= offset.byte && u64::from(offset.byte) <= u64::from(offset.char) * 4
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
                    | u8::from(digest.ends_paragraph) << 3
                    | u8::from(digest.repeated_words.trailing_whitespace) << 4
                    | u8::from(digest.repeated_words.line_breaks_only) << 5
                    | u8::from(digest.closes) << 6,
                paragraph_starts: digest.paragraph_starts,
                apostrophes,
                lemmas: lemmas.list,
                too_often_used,
                repeated_count: digest.repeated_words.count,
                repeated_words,
                repeated_last_sentence: digest.repeated_words.last_sentence,
                variants,
                lead: digest.lead,
                parts: digest.parts,
            }
        }
    }

    impl TryFrom<Digest> for super::ParagraphDigest {
        type Error = String;

        fn try_from(digest: Digest) -> Result<Self, String> {
            if !valid_offset(digest.len) {
                return Err("paragraph digest length is not that of UTF-8 text".to_owned());
            }
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
            let count = digest.repeated_count;
            let last_sentence = digest.repeated_last_sentence;
            let words = digest
                .repeated_words
                .into_iter()
                .map(|word| {
                    number = number
                        .checked_add(word.number)
                        .filter(|&x| x <= count)
                        .ok_or_else(|| "paragraph digest word number is out of range".to_owned())?;
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
                .collect::<Result<Vec<_>, String>>()?;
            if last_sentence.is_some_and(|x| x as usize > words.len()) {
                return Err("paragraph digest sentence start is out of range".to_owned());
            }
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
                    trailing_whitespace: digest.flags & 16 != 0,
                    line_breaks_only: digest.flags & 32 != 0,
                },
                variants,
                lead: digest.lead,
                parts: digest.parts,
                closes: digest.flags & 64 != 0,
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
    /// No replacement: the match only marks the span (`READABILITY_RULE_*`,
    /// `PARAGRAPH_REPEAT_BEGINNING_RULE`). Its byte end is recomputed from the char end, see
    /// [`DocumentMatch::to_suggestion`].
    Mark,
}

/// The result of [`Rules::resolve_document`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct DocumentMatches {
    /// The matches, sorted by paragraph and then by position.
    pub matches: Vec<DocumentMatch>,
    /// The indices of the digests that are not [current](ParagraphDigest::is_current), or whose
    /// offsets don't fit their text (a corrupted digest), and so contributed nothing: check those paragraphs again and resolve again. While any are listed,
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

    /// The span of the match, relative to the start of its paragraph's text. For the rare
    /// `PARAGRAPH_REPEAT_BEGINNING_RULE` match that ends inside a word (see
    /// [`DocumentMatch::to_suggestion`]) the byte end is estimated; the suggestion's is exact.
    pub fn span(&self) -> &Span {
        &self.span
    }

    /// The suggestion for this match in `paragraph`, the text its digest was built from, with the
    /// span still relative to the paragraph. `None` if `paragraph` visibly isn't that text: the
    /// span doesn't fit it (in bytes or chars), or doesn't hold a token like the one the match is
    /// about (for `EN_CONSISTENT_APOS`: the same apostrophe style, with the first apostrophe at
    /// the same place; for `EN_WORD_COHERENCY`: a spelling it can replace). Replace a
    /// paragraph's digest whenever its text changes; this check is a guard, not a substitute.
    ///
    /// The span's byte end is not checked for the matches that only mark a span (the paragraph
    /// rules): it is taken from the char end, which `PARAGRAPH_REPEAT_BEGINNING_RULE` knows
    /// exactly even where LanguageTool's quirky later match ends inside a word the digest has no
    /// byte offsets for.
    pub fn to_suggestion(&self, paragraph: &str) -> Option<Suggestion> {
        let mut span = self.span.clone();
        if self.fix == DocumentFix::Mark {
            let start = span.byte().start;
            let rest = paragraph.get(start..)?;
            let len = rest
                .char_indices()
                .map(|(i, _)| i)
                .chain(std::iter::once(rest.len()))
                .nth(span.char().len())?;
            span = Span::new(start..start + len, span.char().clone());
        }
        let bytes = span.byte().clone();
        let original = paragraph.get(bytes.clone())?;
        if original.chars().count() != span.char().len()
            || paragraph[..bytes.start].chars().count() != span.char().start
        {
            return None;
        }
        self.suggestion_for(original, span)
    }

    /// The suggestion for this match, given `original`, the text of `span`.
    fn suggestion_for(&self, original: &str, span: Span) -> Option<Suggestion> {
        if original.is_empty() {
            return None;
        }
        let replacements = match &self.fix {
            DocumentFix::Mark => Vec::new(),
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
            span,
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
        ) || self.is_paragraph_rule()
    }

    /// Whether this whole-text rule works on LanguageTool paragraphs ([`Part`]s of digests).
    pub(crate) fn is_paragraph_rule(&self) -> bool {
        matches!(
            self,
            Builtin::Readability(_)
                | Builtin::ParagraphRepeatBeginning(_)
                | Builtin::ParagraphOpenerRepeat(_)
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
    /// usable ones count. `paragraphs` caches the document's LanguageTool paragraphs for the
    /// paragraph rules; with `exact`, the digests were made from one run of the sentencizer over
    /// the whole text, see [`document_paragraphs`].
    pub(crate) fn resolve_document(
        &self,
        digests: &[ParagraphDigest],
        paragraphs: &mut Option<Vec<LtParagraph>>,
        exact: bool,
    ) -> Vec<DocumentMatch> {
        let current = || {
            digests
                .iter()
                .enumerate()
                .filter(|(_, digest)| digest.is_usable())
        };
        match self {
            Builtin::ConsistentApostrophes(rule) => rule.resolve(digests),
            Builtin::TooOftenUsed(rule) => rule.resolve(current()),
            Builtin::RepeatedWords(rule) => {
                // The last sentence of the document is skipped when whitespace follows it: inside
                // its digest's text, in line breaks after it, or as the separator after a text
                // that ends a paragraph and is not the last.
                let current: Vec<_> = current().collect();
                let skipped = current
                    .iter()
                    .rposition(|(_, d)| d.len.char > 0 && !d.repeated_words.line_breaks_only)
                    .filter(|&last| {
                        let (rest, after) = (&current[last..], &current[last + 1..]);
                        rest[0].1.repeated_words.trailing_whitespace
                            || after.iter().any(|(_, d)| d.len.char > 0)
                            || rest[..rest.len() - 1].iter().any(|(_, d)| d.ends_paragraph)
                    })
                    .map(|last| current[last].0);
                rule.resolve(current.into_iter(), skipped)
            }
            Builtin::WordCoherency(rule) => rule.resolve(current()),
            Builtin::Readability(rule) => {
                let lengths: Vec<Position> = digests.iter().map(|x| x.text_len()).collect();
                rule.resolve(
                    paragraphs.get_or_insert_with(|| document_paragraphs(digests, exact)),
                    &lengths,
                )
            }
            Builtin::ParagraphRepeatBeginning(rule) => {
                rule.resolve(paragraphs.get_or_insert_with(|| document_paragraphs(digests, exact)))
            }
            Builtin::ParagraphOpenerRepeat(rule) => {
                rule.resolve(paragraphs.get_or_insert_with(|| document_paragraphs(digests, exact)))
            }
            _ => Vec::new(),
        }
    }

    /// The suggestions of this whole-text rule for `sentences`, taken as the whole text.
    pub(crate) fn check_document(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        let Some(first) = sentences.first() else {
            return Vec::new();
        };
        let start = first.span().start();
        // Consecutive sentences of one text cover it, whitespace included.
        let text: String = sentences.iter().map(|x| x.text()).collect();
        let digest =
            ParagraphDigest::from_sentences(&text, sentences, start, std::iter::once(self));

        let document = self
            .resolve_document(std::slice::from_ref(&digest), &mut None, true)
            .into_iter()
            .filter_map(|x| Some(x.to_suggestion(&text)?.rshift(start)))
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
    /// or are malformed contribute nothing and are listed in [`DocumentMatches::stale`].
    ///
    /// Turn each match into a suggestion with [`DocumentMatch::to_suggestion`] and merge those
    /// into the paragraph's suggestions with [`merge_document_suggestions`], which also drops a
    /// match that touches another of the same rule, as LanguageTool does;
    /// [`DocumentMatches::suggestions`] does both for one paragraph.
    pub fn resolve_document(&self, digests: &[ParagraphDigest]) -> DocumentMatches {
        self.resolve_digests(digests, false)
    }

    /// [`Rules::resolve_document`]; with `exact`, the digests were made from one run of the
    /// sentencizer over the whole text, see [`document_paragraphs`].
    fn resolve_digests(&self, digests: &[ParagraphDigest], exact: bool) -> DocumentMatches {
        // Grouped into LanguageTool paragraphs once, for the rules that need them.
        let mut paragraphs = None;
        let mut matches: Vec<DocumentMatch> = self
            .builtins
            .iter()
            .filter(|rule| rule.enabled() && rule.builtin().is_document_rule())
            .flat_map(|rule| {
                rule.builtin()
                    .resolve_document(digests, &mut paragraphs, exact)
            })
            .collect();
        matches.sort_by_key(|x| (x.paragraph, x.span.char().start));
        let stale = digests
            .iter()
            .enumerate()
            .filter(|(_, digest)| !digest.is_usable())
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
        digest.closes = ends_paragraph;
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
            .resolve_digests(&digests, true)
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
            trailing_whitespace: true,
            line_breaks_only: false,
        };
        d.variants.push(VariantWord {
            span: span(17..20, 11..14),
            candidates: vec![VariantCandidate {
                word: "cat".into(),
                replaceable: 0b101,
            }],
        });
        d.lead = Some(Lead {
            len: Offset { byte: 1, char: 1 },
            line_end: LineEnd::of("\n"),
            all_breaks: true,
            ascii: true,
            starts_with_line_break: true,
            starts_with_lf: true,
            has_line_break: true,
            line_feeds: 1,
        });
        d.parts.push(Part {
            mark_start: Some(Offset { byte: 3, char: 1 }),
            mark_end: Some(Offset { byte: 14, char: 10 }),
            beginning: Some(Beginning {
                lead: Offset { byte: 3, char: 1 },
                tokens: vec![BeginningToken::new(
                    Offset { byte: 3, char: 1 },
                    Offset { byte: 6, char: 4 },
                    "Cat",
                    false,
                    true,
                    false,
                )],
            }),
            whitespace: Some((Offset { byte: 1, char: 1 }, 1)),
            ..part(false, "\n", 3)
        });
        d.closes = true;
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
    fn inconsistent_digests_are_errors() {
        let fails = |change: &dyn Fn(&mut ParagraphDigest)| {
            let mut d = full_digest();
            change(&mut d);
            ParagraphDigest::from_bytes(&d.to_bytes()).is_err()
        };
        assert!(!fails(&|_| {}));
        // Bytes without chars, or more than four bytes a char: not a span of UTF-8 text.
        assert!(fails(&|d| d.apostrophes[0].span = span(1..4, 1..1)));
        assert!(fails(&|d| d.variants[0].span = span(17..30, 11..13)));
        assert!(fails(&|d| d.len = Offset { byte: 3, char: 0 }));
        // A sentence start past the words, a word numbered past the count.
        assert!(fails(&|d| d.repeated_words.last_sentence = Some(2)));
        assert!(!fails(&|d| d.repeated_words.last_sentence = Some(1)));
        assert!(fails(&|d| d.repeated_words.words[0].number = 5));
        // Spans past the text, inverted or overlapping paragraph-rule offsets, a too long run of
        // line breaks.
        assert!(fails(&|d| d.variants[0].span = span(17..30, 11..20)));
        assert!(fails(
            &|d| d.parts[0].mark_end = Some(Offset { byte: 2, char: 1 })
        ));
        assert!(fails(
            &|d| d.parts[0].mark_start = Some(Offset { byte: 40, char: 20 })
        ));
        assert!(fails(&|d| d.parts[0].line_end = LineEnd(5)));
        assert!(fails(
            &|d| d.parts[0].whitespace = Some((Offset { byte: 30, char: 20 }, 0))
        ));
        assert!(fails(
            &|d| d.lead.as_mut().unwrap().len = Offset { byte: 30, char: 20 }
        ));
        assert!(fails(&|d| {
            let tokens = &mut d.parts[0].beginning.as_mut().unwrap().tokens;
            tokens.push(tokens[0].clone());
        }));
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
            trailing_whitespace: true,
            line_breaks_only: false,
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
            trailing_whitespace: false,
            line_breaks_only: false,
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
        assert!(!a.repeated_words.trailing_whitespace);
        // Line breaks join the last sentence; other whitespace ends it.
        a.repeated_words.last_sentence = Some(1);
        a.append(digest("\n\r\n"));
        assert!(a.repeated_words.trailing_whitespace);
        assert_eq!(a.repeated_words.last_sentence, Some(1));
        assert!(!a.repeated_words.line_breaks_only);
        a.append(digest("\n "));
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
            ..digest.clone()
        };
        assert!(!stale.is_current());
        assert!(stale.apostrophes().is_empty());

        // Offsets outside the text or out of order: rejected, and unused if they arrive another
        // way.
        let mut outside_apostrophe = digest.clone();
        outside_apostrophe.apostrophes.push(Apostrophe {
            span: span(3..9, 3..9),
            typographic: false,
            first: 0,
        });
        let mut outside = digest.clone();
        outside.parts.push(Part {
            mark_start: Some(Offset { byte: 9, char: 9 }),
            ..part(false, "", 1)
        });
        let mut inverted = digest.clone();
        inverted.parts.push(Part {
            mark_start: Some(Offset { byte: 3, char: 3 }),
            mark_end: Some(Offset { byte: 1, char: 1 }),
            ..part(false, "", 1)
        });
        let mut line_end = digest.clone();
        line_end.parts.push(Part {
            line_end: LineEnd(7),
            ..part(false, "", 1)
        });
        for bad in [outside_apostrophe, outside, inverted, line_end] {
            assert!(matches!(
                ParagraphDigest::from_bytes(&bad.to_bytes()),
                Err(Error::Format(_))
            ));
            assert!(!bad.is_usable());
            assert!(bad.apostrophes().is_empty());
        }
        assert!(digest.is_usable());
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

    #[test]
    fn line_ends() {
        let blank = |text: &str| LineEnd::of(text).is_blank();
        assert!(blank("End.\n\n"));
        assert!(blank("End.  \n\n"));
        assert!(blank("End.\r\n\r\n"));
        assert!(blank("End.\n\r\n\r"));
        assert!(blank("End.\n\n\n\n\n"));
        assert!(!blank("End.\n"));
        assert!(!blank("End.\r\n"));
        assert!(!blank("End.\n\r\n"));
        assert!(!blank("End.\n\n "));
        assert!(!blank("End.\n\n\u{2028}"));
        assert!(!blank(""));
        assert!(LineEnd::of("A.\r\n").ends_with_lf());
        assert!(!LineEnd::of("A.\n\r").ends_with_lf());
        // Joined runs keep their last four chars.
        let joined = LineEnd::of("A.\r\n").then(LineEnd::of("\r\n"));
        assert!(joined.is_blank());
        assert_eq!(joined.chars(), ['\r', '\n', '\r', '\n']);
        assert_eq!(
            LineEnd::of("\n\n\n").then(LineEnd::of("\r\n")).chars(),
            ['\n', '\n', '\r', '\n']
        );
    }

    fn part(starts_with_line_break: bool, text_end: &str, words: u32) -> Part<Offset> {
        let line_end = LineEnd::of(text_end);
        Part {
            sentences: 1,
            words,
            syllables: words,
            mark_start: None,
            mark_end: None,
            beginning: None,
            starts_with_line_break,
            line_end,
            closed: line_end.is_blank(),
            whitespace: None,
        }
    }

    #[test]
    fn parts_join_into_paragraphs() {
        let lead = |text: &str| Lead {
            len: Offset::of(text),
            line_end: LineEnd::of(text),
            all_breaks: text.chars().all(|c| matches!(c, '\r' | '\n')),
            ascii: is_ascii_space(text),
            starts_with_line_break: starts_with_line_break(text),
            starts_with_lf: text.starts_with('\n'),
            has_line_break: text.contains(['\r', '\n']),
            line_feeds: line_feeds(text),
        };
        let counts = |parts: &[Part<Offset>]| -> Vec<(u32, u32)> {
            parts.iter().map(|x| (x.sentences, x.words)).collect()
        };

        // "One.\n" + "Two.\n\n" + "\nThree.": a blank line, then a sentence starting with a
        // line break, end paragraphs.
        let mut parts = Vec::new();
        extend_parts(
            &mut parts,
            None,
            vec![
                part(false, "\n", 1),
                part(false, "\n\n", 2),
                part(true, "", 3),
            ],
            Seam::Split,
        );
        assert_eq!(counts(&parts), [(2, 3), (1, 3)]);

        // "One.\n" split from "\n" + "Two.": the lead completes the blank line.
        let mut parts = vec![part(false, "\n", 1)];
        extend_parts(
            &mut parts,
            Some(lead("\n")),
            vec![part(false, "", 2)],
            Seam::Split,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 2)]);
        // In a whole text, LanguageTool's splitter makes a whitespace-only sentence only of two
        // line feeds ("One.\n" + "\r\n\r\n"); ours also splits off one line feed
        // ("One.\u{a0}\n" + "\n"), which LanguageTool's next sentence starts with.
        let mut parts = vec![part(false, "\n", 1)];
        extend_parts(
            &mut parts,
            Some(lead("\r\n\r\n")),
            vec![part(false, "", 2)],
            Seam::Exact,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 0), (1, 2)]);
        let mut parts = vec![part(false, "\n", 1)];
        extend_parts(
            &mut parts,
            Some(lead("\n")),
            vec![part(false, "", 2)],
            Seam::Exact,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 2)]);
        assert!(parts[1].starts_with_line_break);
        // Two such sentences make one: "One.\u{a0}" + "\r\n" + "\r\n" ends a paragraph.
        let mut parts = vec![part(false, "", 1)];
        extend_parts(
            &mut parts,
            Some(lead("\r\n")),
            vec![lead("\r\n").into_part(), part(false, "", 2)],
            Seam::Exact,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 0), (1, 2)]);
        // A no-break space is a sentence of its own in a whole text too.
        let mut parts = vec![part(false, "\n\n", 1)];
        extend_parts(
            &mut parts,
            Some(lead("\u{a0}\n\n")),
            vec![part(false, "", 2)],
            Seam::Split,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 0), (1, 2)]);
        // After a stripped separator, ASCII whitespace ends the sentence before it: it adds no
        // sentence. Other whitespace is a sentence of its own, in the next paragraph.
        let mut parts = vec![part(false, "", 1)];
        parts[0].closed = true;
        extend_parts(
            &mut parts,
            Some(lead("\n")),
            vec![part(false, "", 2)],
            Seam::Sealed,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 2)]);
        let mut parts = vec![part(false, "", 1)];
        parts[0].closed = true;
        extend_parts(&mut parts, Some(lead(" ")), Vec::new(), Seam::Sealed);
        assert_eq!(counts(&parts), [(1, 1)]);
        let mut parts = vec![part(false, "", 1)];
        parts[0].closed = true;
        extend_parts(
            &mut parts,
            Some(lead("\u{a0}\n\n")),
            vec![part(false, "", 2)],
            Seam::Sealed,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 0), (1, 2)]);

        // "End. " + "\r\n" at the end of a piece, then "Next.": the whitespace starts "Next.".
        let mut parts = vec![part(false, "", 1)];
        extend_parts(
            &mut parts,
            None,
            vec![lead("\r\n").into_part()],
            Seam::Split,
        );
        assert_eq!(counts(&parts), [(1, 1), (1, 0)]);
        extend_parts(&mut parts, None, vec![part(false, "", 2)], Seam::Split);
        assert_eq!(counts(&parts), [(1, 1), (1, 2)]);
        assert!(parts[1].starts_with_line_break);
        // A whitespace-only sentence that ends with a blank line stays a sentence.
        let mut parts = vec![part(false, "", 1)];
        extend_parts(
            &mut parts,
            None,
            vec![lead(" \n\n").into_part()],
            Seam::Split,
        );
        extend_parts(&mut parts, None, vec![part(false, "", 2)], Seam::Split);
        assert_eq!(counts(&parts), [(2, 1), (1, 2)]);
    }
}
