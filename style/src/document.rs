use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use nlprule::{analysis::AnalysisSpan, Tokenizer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub byte: Range<usize>,
    pub char: Range<usize>,
    pub utf16: Range<usize>,
}

impl From<AnalysisSpan> for Span {
    fn from(span: AnalysisSpan) -> Self {
        Self {
            byte: span.byte,
            char: span.char,
            utf16: span.utf16,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct DocumentOptions {
    pub skip_names: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenPosition {
    pub paragraph: usize,
    pub sentence: usize,
    pub token: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub span: Span,
    pub readings: Vec<nlprule::analysis::AnalysisReading>,
    pub chunks: Vec<String>,
    pub is_whitespace: bool,
    pub is_punctuation: bool,
    pub is_word: bool,
    pub is_skipped: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sentence {
    pub text: String,
    pub span: Span,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paragraph {
    pub text: String,
    pub span: Span,
    pub sentences: Vec<Sentence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub paragraphs: Vec<Paragraph>,
    pub dialogue: Vec<Span>,
    pub lemma_index: BTreeMap<String, Vec<TokenPosition>>,
    pub word_count: usize,
}

pub fn is_word(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

fn name_key(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('’', "'")
        .to_lowercase()
}

fn skipped_tokens(
    tokens: &[nlprule::analysis::AnalysisToken],
    names: &BTreeSet<String>,
) -> Vec<bool> {
    let mut skipped = vec![false; tokens.len()];
    if names.is_empty() {
        return skipped;
    }
    let phrase_length = names
        .iter()
        .filter(|name| name.contains(' '))
        .map(String::len)
        .max()
        .unwrap_or(0);
    for (start, token) in tokens.iter().enumerate() {
        if token.is_whitespace {
            continue;
        }
        skipped[start] |= names.contains(&name_key(&token.text));
        if phrase_length == 0 {
            continue;
        }
        let mut surface = String::new();
        for (end, token) in tokens.iter().enumerate().skip(start) {
            surface.push_str(&token.text);
            let key = name_key(&surface);
            if key.len() > phrase_length {
                break;
            }
            if names.contains(&key) {
                skipped[start..=end].fill(true);
            }
        }
    }
    skipped
}

fn shift(span: AnalysisSpan, base: &Span) -> Span {
    Span {
        byte: span.byte.start + base.byte.start..span.byte.end + base.byte.start,
        char: span.char.start + base.char.start..span.char.end + base.char.start,
        utf16: span.utf16.start + base.utf16.start..span.utf16.end + base.utf16.start,
    }
}

pub fn paragraph_spans(text: &str) -> Vec<Span> {
    let mut spans = Vec::new();
    let (mut byte_start, mut char_start, mut utf16_start) = (0, 0, 0);
    let (mut scalar, mut utf16) = (0, 0);
    for (byte, ch) in text.char_indices() {
        if matches!(ch, '\r' | '\n' | '\u{2029}') {
            if byte_start < byte && !text.get(byte_start..byte).unwrap().trim().is_empty() {
                spans.push(Span {
                    byte: byte_start..byte,
                    char: char_start..scalar,
                    utf16: utf16_start..utf16,
                });
            }
            byte_start = byte + ch.len_utf8();
            char_start = scalar + 1;
            utf16_start = utf16 + ch.len_utf16();
        }
        scalar += 1;
        utf16 += ch.len_utf16();
    }
    if byte_start < text.len() && !text.get(byte_start..).unwrap().trim().is_empty() {
        spans.push(Span {
            byte: byte_start..text.len(),
            char: char_start..scalar,
            utf16: utf16_start..utf16,
        });
    }
    spans
}

pub fn analyze_document(tokenizer: &Tokenizer, text: &str, options: &DocumentOptions) -> Document {
    let skipped: BTreeSet<_> = options
        .skip_names
        .iter()
        .map(|name| name_key(name))
        .collect();
    let mut document = Document {
        paragraphs: Vec::new(),
        dialogue: dialogue_spans(text),
        lemma_index: BTreeMap::new(),
        word_count: 0,
    };
    for span in paragraph_spans(text) {
        let paragraph_text = text.get(span.byte.clone()).unwrap();
        let paragraph_index = document.paragraphs.len();
        let mut sentences = Vec::new();
        for (sentence_index, sentence) in tokenizer.analyze(paragraph_text).into_iter().enumerate()
        {
            let mut tokens = Vec::new();
            let skipped = skipped_tokens(&sentence.tokens, &skipped);
            for (token_index, token) in sentence.tokens.into_iter().enumerate() {
                let word = is_word(&token.text);
                let is_skipped = skipped[token_index];
                if word && !is_skipped {
                    document.word_count += 1;
                    let lemmas: BTreeSet<_> = token
                        .readings
                        .iter()
                        .map(|reading| reading.lemma.as_str())
                        .collect();
                    for lemma in lemmas {
                        document
                            .lemma_index
                            .entry(lemma.to_owned())
                            .or_default()
                            .push(TokenPosition {
                                paragraph: paragraph_index,
                                sentence: sentence_index,
                                token: token_index,
                            });
                    }
                }
                tokens.push(Token {
                    text: token.text,
                    span: shift(token.span, &span),
                    readings: token.readings,
                    chunks: token.chunks,
                    is_whitespace: token.is_whitespace,
                    is_punctuation: token.is_punctuation,
                    is_word: word,
                    is_skipped,
                });
            }
            sentences.push(Sentence {
                text: sentence.text,
                span: shift(sentence.span, &span),
                tokens,
            });
        }
        document.paragraphs.push(Paragraph {
            text: paragraph_text.to_owned(),
            span,
            sentences,
        });
    }
    document
}

pub fn dialogue_spans(text: &str) -> Vec<Span> {
    let chars: Vec<_> = text.char_indices().collect();
    let mut spans = Vec::new();
    let mut open: Option<(char, usize, usize, usize)> = None;
    let mut utf16 = 0;
    let mut line_start = true;
    for (scalar, &(byte, ch)) in chars.iter().enumerate() {
        let previous = scalar.checked_sub(1).map(|i| chars[i].1);
        let next = chars.get(scalar + 1).map(|(_, ch)| *ch);
        let single = matches!(ch, '\'' | '‘' | '’');
        let apostrophe = single
            && (previous.is_some_and(char::is_alphanumeric)
                && (next.is_some_and(char::is_alphanumeric)
                    || open
                        .as_ref()
                        .is_none_or(|(kind, ..)| *kind != '\'' && *kind != '‘')));
        if !apostrophe {
            if let Some((kind, start_byte, start_char, start_utf16)) = open {
                let closer = match kind {
                    '“' => '”',
                    '‘' => '’',
                    other => other,
                };
                if ch == kind && line_start {
                    // A repeated opener continues an unfinished speech across paragraphs.
                } else if ch == closer
                    && (!single
                        || previous.is_some_and(|ch| {
                            matches!(ch, '.' | '!' | '?' | ',' | ':' | ';' | '—' | '…')
                        }))
                {
                    spans.push(Span {
                        byte: start_byte..byte + ch.len_utf8(),
                        char: start_char..scalar + 1,
                        utf16: start_utf16..utf16 + ch.len_utf16(),
                    });
                    open = None;
                }
            } else if matches!(ch, '“' | '"' | '‘' | '\'')
                && (!single
                    || (!previous.is_some_and(char::is_alphanumeric)
                        && next.is_some_and(char::is_alphanumeric)))
            {
                open = Some((ch, byte, scalar, utf16));
            }
        }
        if matches!(ch, '\n' | '\r' | '\u{2029}') {
            line_start = true;
        } else if !ch.is_whitespace() {
            line_start = false;
        }
        utf16 += ch.len_utf16();
    }
    spans
}
