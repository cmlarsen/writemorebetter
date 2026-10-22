//! Rules that need the whole text: per-paragraph digests and the document resolver.
//!
//! A few LanguageTool rules decide from text outside the paragraph they report in, e.g.
//! `EN_CONSISTENT_APOS` flags apostrophes only when the whole text mixes `'` and `’`. An app that
//! checks one paragraph per call can't run them on the paragraph alone, and re-checking the whole
//! manuscript on every edit is too slow. Instead:
//!
//! 1. Checking a paragraph also yields a [`ParagraphDigest`]: the few offsets and flags of that
//!    paragraph the whole-text rules need. It is a pure function of the paragraph's text, so an app
//!    keeps one per paragraph and replaces it when the paragraph changes
//!    ([`Rules::check_paragraph`]).
//! 2. [`Rules::resolve_document`] takes the digests of all paragraphs in order and returns the
//!    whole-text rules' matches ([`DocumentMatch`]), each tagged with its paragraph and with a span
//!    relative to it. It never re-tokenizes, and its result replaces the previous set, since one
//!    edit can add or remove matches in paragraphs that did not change.
//!
//! [`Rules::suggest`] goes through the same two steps internally, building one digest per engine
//! paragraph (the sentences up to a line break) and merging the resolver's matches into its output
//! in text order, so whole-text checks and paragraph-by-paragraph checks share one code path.
//!
//! Digests hold offsets and flags, never text. New whole-text rules add fields with serde
//! defaults, so a digest serialized by an older engine still deserializes; it just carries nothing
//! for the newer rules until its paragraph is checked again.

use crate::{
    builtin::{en, Builtin},
    types::{Position, Sentence, Span, Suggestion},
    Rules, Tokenizer,
};
use serde::{Deserialize, Serialize};

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
/// Serializable with serde, so an app can keep digests alongside its paragraphs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParagraphDigest {
    /// The length of the text this digest covers.
    #[serde(default)]
    len: Offset,
    /// Whether the text starts with a line break: the paragraph before it then ends a LanguageTool
    /// paragraph (`Tools.isParagraphEnd`).
    #[serde(default)]
    starts_with_line_break: bool,
    /// The char offsets after each sentence of this text that ends a LanguageTool paragraph:
    /// one that ends with a blank line, or whose next sentence starts with a line break. The end
    /// of the text is not listed: whether it ends a paragraph depends on what follows.
    #[serde(default)]
    paragraph_ends: Vec<u32>,
    /// The tokens with an apostrophe (`EN_CONSISTENT_APOS`), in text order.
    #[serde(default)]
    apostrophes: Vec<Apostrophe>,
}

impl ParagraphDigest {
    /// The byte and char length of the text this digest covers.
    pub fn text_len(&self) -> Position {
        self.len.position()
    }

    pub(crate) fn apostrophes(&self) -> &[Apostrophe] {
        &self.apostrophes
    }

    /// Builds the digest of the sentences of one engine paragraph, which starts at `start` and
    /// ends at `end` in the text. Only the parts for `rules` are filled in.
    fn from_sentences<'a>(
        sentences: &[Sentence],
        start: Position,
        end: Position,
        rules: impl Iterator<Item = &'a Builtin> + Clone,
    ) -> Self {
        let mut digest = ParagraphDigest {
            len: Offset::new(end - start),
            starts_with_line_break: sentences
                .first()
                .is_some_and(|x| starts_with_line_break(x.text())),
            ..ParagraphDigest::default()
        };

        for (n, sentence) in sentences.iter().enumerate() {
            if n + 1 < sentences.len()
                && (ends_with_blank_line(sentence.text())
                    || starts_with_line_break(sentences[n + 1].text()))
            {
                digest
                    .paragraph_ends
                    .push((sentence.span().end().char - start.char) as u32);
            }
            for rule in rules.clone() {
                rule.digest_sentence(sentence, start, &mut digest);
            }
        }
        // The last sentence ends a paragraph if it ends with a blank line, whatever follows.
        if sentences
            .last()
            .is_some_and(|x| ends_with_blank_line(x.text()))
        {
            digest.paragraph_ends.push(digest.len.char);
        }

        digest
    }

    /// Appends `other`, the digest of the text right after this one's.
    fn append(&mut self, mut other: ParagraphDigest) {
        let by = self.len;
        if other.starts_with_line_break && self.paragraph_ends.last() != Some(&by.char) {
            self.paragraph_ends.push(by.char);
        }
        self.paragraph_ends
            .extend(other.paragraph_ends.iter().map(|x| x + by.char));
        for apostrophe in other.apostrophes.iter_mut() {
            apostrophe.start = apostrophe.start.shift(by);
            apostrophe.end = apostrophe.end.shift(by);
        }
        self.apostrophes.append(&mut other.apostrophes);
        self.len = self.len.shift(other.len);
    }

    /// Whether the end of this text ends a LanguageTool paragraph, given the digest of the text
    /// after it (`None` at the end of the document).
    fn ends_paragraph_before(&self, next: Option<&ParagraphDigest>) -> bool {
        next.is_none_or(|next| next.starts_with_line_break)
            || self.paragraph_ends.last() == Some(&self.len.char)
    }
}

fn ends_with_blank_line(text: &str) -> bool {
    text.ends_with("\n\n") || text.ends_with("\n\r\n\r") || text.ends_with("\r\n\r\n")
}

fn starts_with_line_break(text: &str) -> bool {
    text.starts_with('\n') || text.starts_with("\r\n")
}

/// The digests grouped into LanguageTool paragraphs, as ranges of digest indices. A digest whose
/// text holds several LanguageTool paragraphs is not split; rules that look at paragraph starts
/// use its `paragraph_ends` as well. The paragraph-level rules of the next phases
/// (`PARAGRAPH_REPEAT_BEGINNING_RULE`, `READABILITY_RULE_SIMPLE`) group digests this way.
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn lt_paragraphs(digests: &[ParagraphDigest]) -> Vec<std::ops::Range<usize>> {
    let mut paragraphs = Vec::new();
    let mut start = 0;
    for (i, digest) in digests.iter().enumerate() {
        if digest.ends_paragraph_before(digests.get(i + 1)) {
            paragraphs.push(start..i + 1);
            start = i + 1;
        }
    }
    paragraphs
}

/// How a [`DocumentMatch`] turns its paragraph's text into a replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum DocumentFix {
    /// Replace every `'` with `’` (`to_typographic`) or every `’` with `'`.
    SwapApostrophes { to_typographic: bool },
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

    /// The suggestion for this match in `paragraph`, the text its digest was built from. Its span
    /// stays relative to the paragraph. If the span does not fit `paragraph` (a stale digest), the
    /// suggestion has no replacement.
    pub fn to_suggestion(&self, paragraph: &str) -> Suggestion {
        let original = paragraph.get(self.span.byte().clone());
        let replacements = match self.fix {
            DocumentFix::SwapApostrophes { to_typographic } => original
                .map(|original| {
                    if to_typographic {
                        original.replace('\'', "\u{2019}")
                    } else {
                        original.replace('\u{2019}', "'")
                    }
                })
                .into_iter()
                .collect(),
        };
        Suggestion::new(
            self.source.clone(),
            self.message.clone(),
            self.span.clone(),
            replacements,
        )
    }
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
                if let Some(typographic) = original.and_then(en::apostrophe_style) {
                    digest.apostrophes.push(Apostrophe {
                        start: Offset::new(span.start() - start),
                        end: Offset::new(span.end() - start),
                        typographic,
                    });
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

    /// The digest of one engine paragraph for the whole-text builtins: the enabled ones, or all of
    /// them with `all`.
    pub(crate) fn digest(
        &self,
        sentences: &[Sentence],
        start: Position,
        end: Position,
        all: bool,
    ) -> ParagraphDigest {
        let rules = self
            .builtins
            .iter()
            .filter(move |rule| (all || rule.enabled()) && rule.builtin().is_document_rule())
            .map(|rule| rule.builtin());
        ParagraphDigest::from_sentences(sentences, start, end, rules)
    }

    /// Runs the enabled whole-text rules on a document made of `digests`, one per paragraph in
    /// text order. Matches are sorted by paragraph and then by position, and are relative to their
    /// paragraph. The result replaces any earlier one for the document: one changed paragraph can
    /// add or remove matches in others.
    pub fn resolve_document(&self, digests: &[ParagraphDigest]) -> Vec<DocumentMatch> {
        let mut matches: Vec<DocumentMatch> = self
            .builtins
            .iter()
            .filter(|rule| rule.enabled() && rule.builtin().is_document_rule())
            .flat_map(|rule| rule.builtin().resolve_document(digests))
            .collect();
        matches.sort_by_key(|x| (x.paragraph, x.span.char().start));
        drop_touching(matches, digests)
    }

    /// Checks one paragraph of a larger document on its own and returns its suggestions, without
    /// those of the whole-text rules, and its digest. Spans are relative to `paragraph`.
    ///
    /// The digest covers every whole-text builtin, enabled or not, so it stays valid when rules
    /// are toggled. Pass the digests of all paragraphs to [`Rules::resolve_document`] for the
    /// whole-text matches, then [`merge_document_suggestions`] to combine them with the
    /// paragraph's suggestions as [`Rules::suggest`] does.
    pub fn check_paragraph(
        &self,
        paragraph: &str,
        tokenizer: &Tokenizer,
    ) -> (Vec<Suggestion>, ParagraphDigest) {
        let mut digests = Vec::new();
        let suggestions = self.suggest_paragraphs(paragraph, tokenizer, Some((&mut digests, true)));
        let mut digests = digests.into_iter().map(|(_, digest)| digest);
        let mut digest = digests.next().unwrap_or_default();
        for next in digests {
            digest.append(next);
        }
        // Cover all of `paragraph`, also when it has no sentence (whitespace only), so an app can
        // add up the lengths of its paragraphs.
        digest.len = Offset::new(Position {
            byte: paragraph.len(),
            char: paragraph.chars().count(),
        });
        digest.starts_with_line_break = starts_with_line_break(paragraph);
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
        let (starts, digests): (Vec<_>, Vec<_>) = paragraphs.into_iter().unzip();
        let document: Vec<Suggestion> = self
            .resolve_document(&digests)
            .into_iter()
            .map(|x| {
                let start: Position = starts[x.paragraph];
                let end = start + digests[x.paragraph].text_len();
                let paragraph = text.get(start.byte..end.byte).unwrap_or_default();
                x.to_suggestion(paragraph).rshift(start)
            })
            .collect();
        merge_document_suggestions(suggestions, document)
    }
}

/// Drops each match that overlaps or touches an earlier kept match of the same rule, as
/// LanguageTool's `SameRuleGroupFilter` does: e.g. `O'Brien` and `’s` in `O'Brien’s` give one
/// match. `matches` are sorted by position.
fn drop_touching(matches: Vec<DocumentMatch>, digests: &[ParagraphDigest]) -> Vec<DocumentMatch> {
    let starts: Vec<usize> = digests
        .iter()
        .scan(0, |start, digest| {
            let this = *start;
            *start += digest.len.char as usize;
            Some(this)
        })
        .collect();
    let absolute = |x: &DocumentMatch| {
        let start = starts.get(x.paragraph).copied().unwrap_or_default();
        start + x.span.char().start..start + x.span.char().end
    };

    let mut kept: Vec<DocumentMatch> = Vec::with_capacity(matches.len());
    let mut last: Vec<(&str, std::ops::Range<usize>)> = Vec::new();
    for x in &matches {
        let span = absolute(x);
        let touches = last.iter().any(|(source, kept)| {
            *source == x.source && kept.start <= span.end && kept.end >= span.start
        });
        if !touches {
            last.retain(|(source, _)| *source != x.source);
            last.push((&x.source, span));
            kept.push(x.clone());
        }
    }
    kept
}

/// Merges whole-text suggestions into a text's other suggestions the way [`Rules::suggest`] does:
/// in text order, leaving out each one that overlaps one of `suggestions`. Both lists must use the
/// same coordinates (e.g. both relative to one paragraph) and `suggestions` must be sorted by
/// position, as [`Rules::suggest`] and [`Rules::check_paragraph`] return them.
pub fn merge_document_suggestions(
    mut suggestions: Vec<Suggestion>,
    document: Vec<Suggestion>,
) -> Vec<Suggestion> {
    let kept: Vec<Suggestion> = document
        .into_iter()
        .filter(|suggestion| {
            let span = suggestion.span().char();
            let next = suggestions.partition_point(|other| other.span().char().end <= span.start);
            suggestions
                .get(next)
                .is_none_or(|other| other.span().char().start >= span.end)
        })
        .collect();

    if !kept.is_empty() {
        suggestions.extend(kept);
        suggestions.sort_by_key(|suggestion| suggestion.span().char().start);
    }
    suggestions
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(len: u32, starts: bool, ends: &[u32]) -> ParagraphDigest {
        ParagraphDigest {
            len: Offset {
                byte: len,
                char: len,
            },
            starts_with_line_break: starts,
            paragraph_ends: ends.to_vec(),
            apostrophes: Vec::new(),
        }
    }

    #[test]
    fn groups_lt_paragraphs() {
        // A blank line ends the first paragraph; a single line break does not end the second;
        // the third starts with a line break, which ends the second; the text end ends the last.
        let digests = [
            digest(10, false, &[10]),
            digest(5, false, &[]),
            digest(5, true, &[]),
            digest(5, false, &[]),
        ];
        assert_eq!(lt_paragraphs(&digests), vec![0..1, 1..2, 2..4]);
        assert!(lt_paragraphs(&[]).is_empty());
    }

    #[test]
    fn appends_digests() {
        let mut a = digest(10, false, &[]);
        a.apostrophes.push(Apostrophe {
            start: Offset { byte: 1, char: 1 },
            end: Offset { byte: 4, char: 3 },
            typographic: true,
        });
        let mut b = digest(6, true, &[6]);
        b.apostrophes.push(Apostrophe {
            start: Offset { byte: 2, char: 2 },
            end: Offset { byte: 4, char: 4 },
            typographic: false,
        });
        a.append(b);
        assert_eq!(a.text_len(), Position { byte: 16, char: 16 });
        assert_eq!(a.paragraph_ends, vec![10, 16]);
        assert_eq!(a.apostrophes[1].span(), Span::new(12..14, 12..14));
    }

    #[test]
    fn suggestion_from_stale_digest_has_no_replacement() {
        let m = DocumentMatch::new(
            0,
            "BUILTIN/EN_CONSISTENT_APOS".into(),
            "m".into(),
            // `n’t` in "a n’t"; in "n’t" the bytes end inside nothing and are out of range.
            Span::new(2..7, 2..5),
            DocumentFix::SwapApostrophes {
                to_typographic: false,
            },
        );
        assert!(m.to_suggestion("n\u{2019}t").replacements().is_empty());
        assert!(m.to_suggestion("").replacements().is_empty());
        assert_eq!(m.to_suggestion("a n\u{2019}t").replacements(), ["n't"]);
    }
}
