use std::collections::{BTreeMap, BTreeSet};
use std::ops::Range;

use nlprule::{analysis::AnalysisSpan, Tokenizer};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct DocumentOptions {
    #[serde(alias = "skipNames")]
    pub skip_names: Vec<String>,
    #[serde(alias = "includeTokenDetails")]
    pub include_token_details: bool,
    #[serde(alias = "hardWrapped")]
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

impl DocumentOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_skip_names(mut self, names: Vec<String>) -> Self {
        self.skip_names = names;
        self
    }
    pub fn with_token_details(mut self, include: bool) -> Self {
        self.include_token_details = include;
        self
    }
    pub fn with_hard_wrapped(mut self, hard_wrapped: bool) -> Self {
        self.hard_wrapped = hard_wrapped;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct TokenPosition {
    pub paragraph: usize,
    pub sentence: usize,
    pub token: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
pub struct Sentence {
    pub text: String,
    pub span: Span,
    pub tokens: Vec<Token>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Paragraph {
    pub text: String,
    pub span: Span,
    pub sentences: Vec<Sentence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
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
}

fn name_forms(names: &[String]) -> BTreeSet<String> {
    let mut forms = BTreeSet::new();
    for name in names {
        let name = name_key(name);
        if name.is_empty() {
            continue;
        }
        forms.insert(name.to_uppercase());
        if let Some(first) = name.chars().next().filter(|ch| ch.is_lowercase()) {
            forms.insert(
                first.to_uppercase().collect::<String>() + name.get(first.len_utf8()..).unwrap(),
            );
        }
        forms.insert(name);
    }
    forms
}

fn skipped_tokens(
    tokens: &[nlprule::analysis::AnalysisToken],
    names: &BTreeSet<String>,
) -> Vec<bool> {
    let mut skipped = vec![false; tokens.len()];
    let Some(max_length) = names.iter().map(String::len).max() else {
        return skipped;
    };
    for (start, token) in tokens.iter().enumerate() {
        if token.is_whitespace {
            continue;
        }
        let mut surface = String::new();
        for (end, token) in tokens.iter().enumerate().skip(start) {
            surface.push_str(&token.text);
            let key = name_key(&surface);
            if key.len() > max_length {
                break;
            }
            if !token.is_whitespace && names.contains(&key) {
                skipped[start..=end].fill(true);
                if let Some(clitic) = tokens.get(end + 1) {
                    if clitic.span.byte.start == token.span.byte.end
                        && matches!(name_key(&clitic.text).as_str(), "'s" | "'S" | "'")
                    {
                        skipped[end + 1] = true;
                    }
                }
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
    let skipped = name_forms(&options.skip_names);
    let paragraphs = paragraph_spans_mode(text, options.hard_wrapped);
    let mut document = Document {
        paragraphs: Vec::new(),
        dialogue: detect_dialogue(text, &paragraphs),
        lemma_index: BTreeMap::new(),
        word_count: 0,
    };
    for span in paragraphs {
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
                    let mut lemmas: BTreeSet<_> = token
                        .readings
                        .iter()
                        .map(|reading| reading.lemma.clone())
                        .collect();
                    if lemmas.is_empty() {
                        lemmas.insert(token.text.to_lowercase());
                    }
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
                    readings: if options.include_token_details {
                        token.readings
                    } else {
                        Vec::new()
                    },
                    chunks: if options.include_token_details {
                        token.chunks
                    } else {
                        Vec::new()
                    },
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

fn paragraph_spans_mode(text: &str, hard_wrapped: bool) -> Vec<Span> {
    let lines = paragraph_spans(text);
    if !hard_wrapped {
        return lines;
    }
    let mut paragraphs: Vec<Span> = Vec::new();
    for line in lines {
        if let Some(previous) = paragraphs.last_mut() {
            let gap = text.get(previous.byte.end..line.byte.start).unwrap();
            let breaks =
                gap.matches('\n').count() + gap.matches('\r').count() - gap.matches("\r\n").count();
            if breaks < 2 && !gap.contains('\u{2029}') {
                previous.byte.end = line.byte.end;
                previous.char.end = line.char.end;
                previous.utf16.end = line.utf16.end;
                continue;
            }
        }
        paragraphs.push(line);
    }
    paragraphs
}

pub fn dialogue_spans(text: &str) -> Vec<Span> {
    dialogue_spans_with_options(text, &DocumentOptions::new())
}

pub fn dialogue_spans_with_options(text: &str, options: &DocumentOptions) -> Vec<Span> {
    detect_dialogue(text, &paragraph_spans_mode(text, options.hard_wrapped))
}

fn leading_elision(after: &str) -> bool {
    let word = after
        .chars()
        .take_while(|ch| ch.is_alphanumeric())
        .collect::<String>()
        .to_lowercase();
    matches!(
        word.as_str(),
        "em" | "tis"
            | "twas"
            | "twere"
            | "twill"
            | "twould"
            | "n"
            | "cause"
            | "bout"
            | "round"
            | "til"
            | "till"
            | "ere"
            | "neath"
            | "nother"
            | "cept"
            | "scuse"
            | "twenties"
            | "thirties"
            | "forties"
            | "fifties"
            | "sixties"
            | "seventies"
            | "eighties"
            | "nineties"
    ) || word
        .strip_suffix('s')
        .is_some_and(|digits| !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit()))
}

fn detect_dialogue(text: &str, paragraphs: &[Span]) -> Vec<Span> {
    let mut spans = Vec::new();
    let mut open: Option<(char, usize, usize, usize)> = None;
    for (paragraph_index, paragraph) in paragraphs.iter().enumerate() {
        let surface = text.get(paragraph.byte.clone()).unwrap();
        let chars: Vec<_> = surface.char_indices().collect();
        let mut utf16 = paragraph.utf16.start;
        let mut at_start = true;
        for (index, &(local_byte, ch)) in chars.iter().enumerate() {
            let byte = paragraph.byte.start + local_byte;
            let scalar = paragraph.char.start + index;
            let previous = index.checked_sub(1).map(|i| chars[i].1);
            let next = chars.get(index + 1).map(|(_, ch)| *ch);
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
                    if !(ch == kind && at_start)
                        && ch == closer
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
                            && next.is_some_and(char::is_alphanumeric)
                            && !leading_elision(
                                surface.get(local_byte + ch.len_utf8()..).unwrap(),
                            )))
                {
                    open = Some((ch, byte, scalar, utf16));
                }
            }
            if !ch.is_whitespace() {
                at_start = false;
            }
            utf16 += ch.len_utf16();
        }
        if let Some((kind, ..)) = open {
            let continued = paragraphs
                .get(paragraph_index + 1)
                .and_then(|next| text.get(next.byte.clone()))
                .is_some_and(|next| next.trim_start().starts_with(kind));
            if !continued {
                open = None;
            }
        }
    }
    spans
}
