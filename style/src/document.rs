//! Paragraph structure, quoted dialogue and a document-wide lemma index.

use std::collections::{BTreeMap, BTreeSet};

use nlprule::{
    analysis::{AnalysisSentence, AnalysisSpan, AnalysisToken},
    Tokenizer,
};
use serde::{Deserialize, Serialize};

mod dialogue;

/// A token's stable position within this document snapshot (all indices are zero-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TokenPosition {
    /// Index into [`Document::paragraphs`].
    pub paragraph: usize,
    /// Index into [`Paragraph::sentences`].
    pub sentence: usize,
    /// Index into the sentence's tokens, including whitespace.
    pub token: usize,
}

/// One caller-supplied paragraph. An empty paragraph still has a span and no sentences.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Paragraph {
    /// Absolute range excluding the inserted paragraph separator.
    pub span: AnalysisSpan,
    /// Analyzed sentences, with every span shifted into document coordinates.
    pub sentences: Vec<AnalysisSentence>,
}

/// An owned snapshot shared by prose reports. All spans index [`Document::text`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Document {
    /// Caller paragraphs joined with a single LF, with no added terminal LF.
    pub text: String,
    /// Caller paragraph boundaries, including empty paragraphs.
    pub paragraphs: Vec<Paragraph>,
    /// Ordered, non-overlapping outer quoted spans, including quotation marks.
    /// Nested quotes are covered by their outer span. Unclosed quotes extend to EOF.
    pub dialogue_spans: Vec<AnalysisSpan>,
    /// Exact, case-sensitive lemma → positions, in document order. Ambiguous readings
    /// can index the same token under several lemmas, but never twice under one lemma.
    /// Whitespace and punctuation are excluded; this is not a disambiguated word count.
    pub lemma_index: BTreeMap<String, Vec<TokenPosition>>,
}

impl Document {
    /// Returns the token at a document position, or `None` for an invalid position.
    pub fn token(&self, position: TokenPosition) -> Option<&AnalysisToken> {
        self.paragraphs
            .get(position.paragraph)?
            .sentences
            .get(position.sentence)?
            .tokens
            .get(position.token)
    }

    /// Whether a span overlaps quoted dialogue. Empty spans never overlap.
    pub fn is_dialogue(&self, span: &AnalysisSpan) -> bool {
        let index = self
            .dialogue_spans
            .partition_point(|quote| quote.byte.end <= span.byte.start);
        span.byte.start < span.byte.end
            && self
                .dialogue_spans
                .get(index)
                .is_some_and(|quote| quote.byte.start < span.byte.end)
    }
}

/// Analyzes each paragraph once using the caller's tokenizer, then builds document statistics.
///
/// Input strings define paragraph boundaries even when they have no terminal punctuation.
/// They are preserved verbatim; internal line breaks do not create extra paragraphs. The
/// document is a fresh snapshot on every call, suitable for idle/save analysis after edits
/// (the whole-document strategy in issue #234). Grammar suggestions remain a separate call.
pub fn analyze_document(tokenizer: &Tokenizer, paragraphs: &[&str]) -> Document {
    let text = paragraphs.join("\n");
    let mut document = Document {
        text,
        paragraphs: Vec::with_capacity(paragraphs.len()),
        dialogue_spans: Vec::new(),
        lemma_index: BTreeMap::new(),
    };
    let mut offset = AnalysisSpan {
        byte: 0..0,
        char: 0..0,
        utf16: 0..0,
    };
    for (paragraph_index, text) in paragraphs.iter().enumerate() {
        let mut sentences = tokenizer.analyze(text);
        for (sentence_index, sentence) in sentences.iter_mut().enumerate() {
            shift(&mut sentence.span, &offset);
            for (token_index, token) in sentence.tokens.iter_mut().enumerate() {
                shift(&mut token.span, &offset);
                if token.is_whitespace || token.is_punctuation {
                    continue;
                }
                let lemmas: BTreeSet<_> = token
                    .readings
                    .iter()
                    .map(|r| &r.lemma)
                    .filter(|lemma| !lemma.is_empty())
                    .collect();
                for lemma in lemmas {
                    document
                        .lemma_index
                        .entry(lemma.clone())
                        .or_default()
                        .push(TokenPosition {
                            paragraph: paragraph_index,
                            sentence: sentence_index,
                            token: token_index,
                        });
                }
            }
        }
        let span = AnalysisSpan {
            byte: offset.byte.start..offset.byte.start + text.len(),
            char: offset.char.start..offset.char.start + text.chars().count(),
            utf16: offset.utf16.start..offset.utf16.start + text.encode_utf16().count(),
        };
        offset = AnalysisSpan {
            byte: span.byte.end + 1..span.byte.end + 1,
            char: span.char.end + 1..span.char.end + 1,
            utf16: span.utf16.end + 1..span.utf16.end + 1,
        };
        document.paragraphs.push(Paragraph { span, sentences });
    }
    document.dialogue_spans = dialogue::find(&document);
    document
}

fn shift(span: &mut AnalysisSpan, offset: &AnalysisSpan) {
    span.byte = span.byte.start + offset.byte.start..span.byte.end + offset.byte.start;
    span.char = span.char.start + offset.char.start..span.char.end + offset.char.start;
    span.utf16 = span.utf16.start + offset.utf16.start..span.utf16.end + offset.utf16.start;
}
