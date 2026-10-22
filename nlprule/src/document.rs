//! Rules that need the whole text: per-paragraph digests and the document resolver.
//!
//! A few LanguageTool rules decide from text outside the paragraph they report in, e.g.
//! `EN_CONSISTENT_APOS` flags apostrophes only when the whole text mixes `'` and `’`. An app that
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
//! Digests hold offsets and flags, never text. Their layout changes when a whole-text rule needs
//! more from a paragraph; [`DIGEST_VERSION`] changes with it. [`ParagraphDigest::to_bytes`] and
//! [`ParagraphDigest::from_bytes`] store a digest with its version and reject one from another
//! version with [`Error::Format`]. A digest of another version that reaches
//! [`Rules::resolve_document`] some other way (e.g. through serde with a self-describing format)
//! contributes nothing to it, and [`DocumentMatches::stale`] lists it so the app checks that
//! paragraph again.
//!
//! [`DIGEST_VERSION`] covers the layout only. What a digest records also depends on the
//! tokenizer and rules data (how a text is split into tokens, which tokens hold apostrophes), so
//! rebuild all digests after updating the engine or its data binaries, even when the version is
//! unchanged.

use crate::{
    builtin::{en, Builtin},
    types::{Position, Sentence, Span, Suggestion},
    Error, Rules, Tokenizer,
};
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// The layout version of [`ParagraphDigest`]. Digests of another version contribute nothing to
/// [`Rules::resolve_document`]; check their paragraphs again.
pub const DIGEST_VERSION: u32 = 1;

/// A compact text position: byte and char offset relative to the start of a digest's text.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Offset {
    byte: u32,
    char: u32,
}

impl Offset {
    fn new(position: Position) -> Self {
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
}

/// A token that contains an apostrophe, as `EN_CONSISTENT_APOS` sees it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Apostrophe {
    start: Offset,
    end: Offset,
    /// Whether the token's apostrophe is typographic (`’`) rather than typewriter (`'`).
    pub(crate) typographic: bool,
    /// The char index of the first apostrophe in the token (at most 255), which
    /// [`DocumentMatch::to_suggestion`] checks to notice a stale digest.
    pub(crate) first: u8,
}

impl Apostrophe {
    pub(crate) fn span(&self) -> Span {
        Span::from_positions(self.start.position(), self.end.position())
    }
}

/// What the whole-text rules need to know about one paragraph: offsets and flags only, no text.
///
/// Built while the paragraph is checked, from its already tokenized and tagged sentences
/// ([`Rules::check_paragraph`]). Offsets are relative to the start of the paragraph's text.
/// Serializable with serde; see the [module docs](self) for how versions are handled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphDigest {
    /// [`DIGEST_VERSION`] when the digest was made. First, so [`ParagraphDigest::from_bytes`] can
    /// read it before the rest.
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

    /// Serializes this digest with bincode. The first four bytes are its version, little-endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        bincode::serialize(self).expect("a digest always serializes")
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
        let digest: ParagraphDigest = bincode::deserialize(bytes)?;
        if bincode::serialized_size(&digest)? != bytes.len() as u64 {
            return Err(Error::Format(
                "paragraph digest has trailing bytes".to_owned(),
            ));
        }
        Ok(digest)
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
            apostrophe.start = apostrophe.start.shift(by);
            apostrophe.end = apostrophe.end.shift(by);
        }
        self.apostrophes.append(&mut other.apostrophes);
        self.len = self.len.shift(other.len);
        self.ends_with_line_break = other.ends_with_line_break;
        self.ends_paragraph = other.ends_paragraph;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DocumentFix {
    /// Replace every `'` with `’` (`to_typographic`) or every `’` with `'`. `first` is the char
    /// index of the first apostrophe in the token.
    SwapApostrophes { to_typographic: bool, first: u8 },
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
    /// the same place). Replace a paragraph's digest whenever its text changes; this check is a
    /// guard, not a substitute.
    pub fn to_suggestion(&self, paragraph: &str) -> Option<Suggestion> {
        let bytes = self.span.byte().clone();
        let original = paragraph.get(bytes.clone())?;
        if original.chars().count() != self.span.char().len()
            || paragraph[..bytes.start].chars().count() != self.span.char().start
        {
            return None;
        }
        let replacement = match self.fix {
            DocumentFix::SwapApostrophes {
                to_typographic,
                first,
            } => {
                if en::apostrophe_style(original) != Some(!to_typographic)
                    || first_apostrophe(original) != first
                {
                    return None;
                }
                if to_typographic {
                    original.replace('\'', "\u{2019}")
                } else {
                    original.replace('\u{2019}', "'")
                }
            }
        };
        Some(Suggestion::new(
            self.source.clone(),
            self.message.clone(),
            self.span.clone(),
            vec![replacement],
        ))
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
        matches!(self, Builtin::ConsistentApostrophes(_))
    }

    /// Adds what this whole-text rule needs from `sentence` to `digest`, whose text starts at
    /// `start`.
    fn digest_sentence(&self, sentence: &Sentence, start: Position, digest: &mut ParagraphDigest) {
        if let Builtin::ConsistentApostrophes(_) = self {
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
                            start: Offset::new(span.start() - start),
                            end: Offset::new(span.end() - start),
                            typographic,
                            first: first_apostrophe(original),
                        });
                    }
                }
            }
        }
    }

    fn resolve_document(&self, digests: &[ParagraphDigest]) -> Vec<DocumentMatch> {
        match self {
            Builtin::ConsistentApostrophes(rule) => rule.resolve(digests),
            _ => Vec::new(),
        }
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
    /// match that touches another of the same rule, as LanguageTool does.
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
    /// are toggled. Pass the digests of all paragraphs to [`Rules::resolve_document`]. The
    /// whole-text matches equal those [`Rules::suggest`] finds in the joined text; the other
    /// suggestions only see this paragraph.
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
        let (paragraph_starts, apostrophes) = (digest.paragraph_starts, digest.apostrophes);
        let mut digest = ParagraphDigest {
            paragraph_starts,
            apostrophes,
            ..ParagraphDigest::default()
        };
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

/// Answers whether a span overlaps any suggestion of a list sorted by start, without assuming
/// that ends grow with starts (overlapping suggestions are allowed).
pub(crate) struct Overlaps {
    starts: Vec<usize>,
    /// The largest end among the suggestions up to each index.
    max_ends: Vec<usize>,
}

impl Overlaps {
    pub(crate) fn new(sorted: &[Suggestion]) -> Self {
        let starts = sorted.iter().map(|x| x.span().char().start).collect();
        let max_ends = sorted
            .iter()
            .scan(0, |max, x| {
                *max = (*max).max(x.span().char().end);
                Some(*max)
            })
            .collect();
        Overlaps { starts, max_ends }
    }

    /// Whether `span` shares at least one char with a suggestion.
    pub(crate) fn any(&self, span: &Range<usize>) -> bool {
        let before_end = self.starts.partition_point(|start| *start < span.end);
        before_end > 0 && self.max_ends[before_end - 1] > span.start
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
            start: Offset { byte: 1, char: 1 },
            end: Offset { byte: 4, char: 3 },
            typographic: true,
            first: 0,
        });
        let mut b = digest("\n\n2345");
        b.apostrophes.push(Apostrophe {
            start: Offset { byte: 2, char: 2 },
            end: Offset { byte: 4, char: 4 },
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
                start: Offset::default(),
                end: Offset { byte: 1, char: 1 },
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

    #[test]
    fn overlap_index_allows_nested_spans() {
        let sorted = [
            suggestion("A", 0..20),
            suggestion("B", 2..3),
            suggestion("C", 30..31),
        ];
        let overlaps = Overlaps::new(&sorted);
        assert!(overlaps.any(&(10..12)));
        assert!(!overlaps.any(&(20..30)));
        assert!(overlaps.any(&(29..31)));
        assert!(!overlaps.any(&(31..40)));
    }
}
