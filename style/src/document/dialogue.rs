use std::collections::BTreeSet;

use nlprule::analysis::AnalysisSpan;

use super::Document;

#[derive(Clone, Copy)]
struct Point {
    byte: usize,
    char: usize,
    utf16: usize,
}

fn span(start: Point, end: Point) -> AnalysisSpan {
    AnalysisSpan {
        byte: start.byte..end.byte,
        char: start.char..end.char,
        utf16: start.utf16..end.utf16,
    }
}

pub(super) fn find(document: &Document) -> Vec<AnalysisSpan> {
    // The tokenizer already identifies clitics, elisions and dropped-g words. Apostrophes
    // within those surface tokens must never close a quote, including at a word's end.
    let protected: BTreeSet<_> = document
        .paragraphs
        .iter()
        .flat_map(|p| &p.sentences)
        .flat_map(|s| &s.tokens)
        .filter(|t| t.text.chars().any(char::is_alphanumeric))
        .flat_map(|t| {
            t.text.char_indices().filter_map(move |(i, ch)| {
                matches!(ch, '\'' | '’').then_some(t.span.byte.start + i)
            })
        })
        .collect();
    let mut quotes = Vec::new();
    let mut stack = Vec::new();
    let mut start = None;
    for paragraph in &document.paragraphs {
        let text = &document.text[paragraph.span.byte.clone()];
        let first = text.len() - text.trim_start().len();
        let mut chars = text.char_indices().peekable();
        let mut previous: Option<char> = None;
        let mut char_offset = paragraph.span.char.start;
        let mut utf16_offset = paragraph.span.utf16.start;
        while let Some((local_byte, ch)) = chars.next() {
            let point = Point {
                byte: paragraph.span.byte.start + local_byte,
                char: char_offset,
                utf16: utf16_offset,
            };
            char_offset += 1;
            utf16_offset += ch.len_utf16();
            let prev = previous;
            previous = Some(ch);
            if !matches!(ch, '“' | '”' | '‘' | '’' | '"' | '\'') || protected.contains(&point.byte)
            {
                continue;
            }
            let next = chars.peek().map(|(_, ch)| *ch);
            if matches!(ch, '\'' | '’') && prev.is_some_and(char::is_alphanumeric) {
                // Internal apostrophes and unpaired word-final possessives aren't quotes.
                if next.is_some_and(char::is_alphanumeric) || stack.last() != Some(&ch) {
                    continue;
                }
            }
            let closing = match ch {
                '“' => '”',
                '‘' => '’',
                other => other,
            };
            let opens = matches!(ch, '“' | '‘' | '"' | '\'');
            // Fiction repeats the opening mark at each paragraph of one speaker's speech.
            if opens && local_byte == first && stack.last() == Some(&closing) {
                continue;
            }
            if stack.last() == Some(&ch) {
                stack.pop();
                if stack.is_empty() {
                    quotes.push(span(
                        start.take().expect("open quote has a start"),
                        Point {
                            byte: point.byte + ch.len_utf8(),
                            char: char_offset,
                            utf16: utf16_offset,
                        },
                    ));
                }
            } else if opens {
                if stack.is_empty() {
                    start = Some(point);
                }
                stack.push(closing);
            }
        }
    }
    if let Some(start) = start {
        quotes.push(span(
            start,
            Point {
                byte: document.text.len(),
                char: document.text.chars().count(),
                utf16: document.text.encode_utf16().count(),
            },
        ));
    }
    quotes
}
