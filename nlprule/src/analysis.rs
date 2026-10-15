//! Owned sentences and tagged tokens from the tokenizer's existing analysis pipeline.
//!
//! Spans are absolute, half-open ranges into the original input. Token surface text is
//! preserved even when tagging normalizes it (for example a curly apostrophe).

use std::ops::Range;

use serde::{Deserialize, Serialize};
use unicode_categories::UnicodeCategories;

use crate::{tokenizer::tag::SpecialPos, Tokenizer};

/// A range into the original input in three offset conventions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalysisSpan {
    /// UTF-8 byte offsets, suitable for slicing a Rust string.
    pub byte: Range<usize>,
    /// Unicode scalar offsets, matching the rule engine's character spans.
    pub char: Range<usize>,
    /// UTF-16 code-unit offsets, suitable for JavaScript strings and `NSRange`.
    pub utf16: Range<usize>,
}

/// One possible lemma and POS reading after disambiguation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalysisReading {
    /// Lemma, which can differ from the surface text.
    pub lemma: String,
    /// Language-specific POS tag (internal matcher sentinel tags are omitted).
    pub pos: String,
}

/// An owned token, including whitespace reconstructed between pipeline tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalysisToken {
    /// Original surface text, without normalization.
    pub text: String,
    /// Absolute input span.
    pub span: AnalysisSpan,
    /// Paired lemma/POS readings after disambiguation; empty for whitespace and unknowns.
    /// Readings can repeat and include internal linguistic tags; count tokens, not readings.
    pub readings: Vec<AnalysisReading>,
    /// Chunk tags assigned by the tokenizer.
    pub chunks: Vec<String>,
    /// True when every character is Unicode whitespace.
    pub is_whitespace: bool,
    /// True when every character is Unicode punctuation (symbols such as emoji are not).
    pub is_punctuation: bool,
}

/// An owned sentence from the tokenizer's full analysis pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct AnalysisSentence {
    /// Original sentence text, including whitespace retained by the sentence splitter.
    pub text: String,
    /// Absolute input span. Trailing whitespace inclusion follows [`Tokenizer::pipe`].
    pub span: AnalysisSpan,
    /// Tokens in input order, including whitespace, without synthetic sentence markers.
    pub tokens: Vec<AnalysisToken>,
}

impl Tokenizer {
    /// Analyzes text once with sentence splitting, tagging, chunking and disambiguation.
    ///
    /// The result owns all its strings and outlives both the input and tokenizer. It uses
    /// the same tokenizer as grammar checking, but does not run grammar or spelling rules.
    /// Empty or whitespace-only input returns no sentences. Within each returned sentence,
    /// token spans partition its original text, with each token counted once.
    pub fn analyze(&self, text: &str) -> Vec<AnalysisSentence> {
        // Tokens (including reconstructed gaps) are visited in input order. Count offsets
        // as we emit them, without rescanning prefixes or allocating a document-sized map.
        let mut char_offset = 0;
        let mut utf16_offset = 0;
        let mut token = |byte: Range<usize>, readings, chunks| {
            let surface = text
                .get(byte.clone())
                .expect("token span is on UTF-8 boundaries");
            let char_start = char_offset;
            let utf16_start = utf16_offset;
            for ch in surface.chars() {
                char_offset += 1;
                utf16_offset += ch.len_utf16();
            }
            AnalysisToken {
                text: surface.to_owned(),
                span: AnalysisSpan {
                    byte,
                    char: char_start..char_offset,
                    utf16: utf16_start..utf16_offset,
                },
                readings,
                chunks,
                is_whitespace: surface.chars().all(char::is_whitespace),
                is_punctuation: surface.chars().all(|ch| ch.is_punctuation()),
            }
        };
        self.pipe_with_raw(text, false)
            .map(|sentence| {
                let mut tokens = Vec::new();
                let mut cursor = sentence.span().byte().start;
                for item in sentence.tokens() {
                    let byte = item.span().byte().clone();
                    if cursor < byte.start {
                        tokens.push(token(cursor..byte.start, Vec::new(), Vec::new()));
                    }
                    let readings = item
                        .word()
                        .tags()
                        .iter()
                        .filter(|reading| {
                            !SpecialPos::iter().any(|pos| pos == reading.pos().as_str())
                        })
                        .map(|reading| AnalysisReading {
                            lemma: reading.lemma().as_str().to_owned(),
                            pos: reading.pos().as_str().to_owned(),
                        })
                        .collect();
                    cursor = byte.end;
                    tokens.push(token(byte, readings, item.chunks().to_vec()));
                }
                let byte = sentence.span().byte().clone();
                if cursor < byte.end {
                    tokens.push(token(cursor..byte.end, Vec::new(), Vec::new()));
                }
                // A nonempty pipeline sentence always contains at least one real token.
                let first = &tokens.first().expect("sentence has tokens").span;
                let last = &tokens.last().expect("sentence has tokens").span;
                let span = AnalysisSpan {
                    byte: byte.clone(),
                    char: first.char.start..last.char.end,
                    utf16: first.utf16.start..last.utf16.end,
                };
                AnalysisSentence {
                    text: text
                        .get(byte.clone())
                        .expect("sentence span is on UTF-8 boundaries")
                        .to_owned(),
                    span,
                    tokens,
                }
            })
            .collect()
    }
}
