use std::collections::BTreeMap;
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::reports::{word_split, Split};
use crate::{Document, Mode, Paragraph, Span, StyleError, Token};

pub const REPORT: &str = "drafting";
const FILLERS: &str = include_str!("../../data/drafting/fillers.txt");
const PLACEHOLDERS: &str = include_str!("../../data/drafting/placeholders.tsv");

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DraftingCategory {
    Placeholder,
    SpokenCommand,
    FilledPause,
    Restart,
    SelfCorrection,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct DraftingOptions {
    pub placeholders: bool,
    pub filled_pauses: bool,
    pub er: bool,
    pub include_dialogue: bool,
}
impl Default for DraftingOptions {
    fn default() -> Self {
        Self {
            placeholders: true,
            filled_pauses: true,
            er: false,
            include_dialogue: false,
        }
    }
}
impl DraftingOptions {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftingEdit {
    pub span: Span,
    pub replacement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftingMatch {
    pub span: Span,
    pub sentence: usize,
    pub category: DraftingCategory,
    pub term: String,
    pub mode: Mode,
    pub edits: Vec<DraftingEdit>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftingTerm {
    pub term: String,
    pub category: DraftingCategory,
    pub count: usize,
    pub narration: usize,
    pub dialogue: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftingGroup {
    pub matches: usize,
    pub edits: usize,
    pub words: usize,
    pub per_thousand_words: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftingReport {
    pub matches: Vec<DraftingMatch>,
    pub terms: Vec<DraftingTerm>,
    pub groups: Split<DraftingGroup>,
}

/// Apply any subset of nonoverlapping edits. Invalid and overlapping spans are ignored.
pub fn apply(text: &str, edits: &[&DraftingEdit]) -> String {
    let mut ordered = edits.to_vec();
    ordered.sort_by_key(|edit| (edit.span.byte.start, edit.span.byte.end));
    let mut result = String::with_capacity(text.len());
    let mut cursor = 0;
    for edit in ordered {
        let range = &edit.span.byte;
        if range.start < cursor
            || range.start > range.end
            || range.end > text.len()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        result.push_str(&text[cursor..range.start]);
        result.push_str(&edit.replacement);
        cursor = range.end;
    }
    result.push_str(&text[cursor..]);
    result
}

fn span(paragraph: &Paragraph, start: usize, end: usize) -> Span {
    let text = &paragraph.text;
    Span {
        byte: paragraph.span.byte.start + start..paragraph.span.byte.start + end,
        char: paragraph.span.char.start + text[..start].chars().count()
            ..paragraph.span.char.start + text[..end].chars().count(),
        utf16: paragraph.span.utf16.start + text[..start].encode_utf16().count()
            ..paragraph.span.utf16.start + text[..end].encode_utf16().count(),
    }
}
fn rel(paragraph: &Paragraph, byte: usize) -> usize {
    byte - paragraph.span.byte.start
}
fn prev(text: &str, at: usize) -> Option<(usize, char)> {
    text[..at].char_indices().next_back()
}
fn next(text: &str, at: usize) -> Option<char> {
    text[at..].chars().next()
}
fn quoted_alone(text: &str, start: usize, end: usize) -> bool {
    if !matches!(
        prev(text, start).map(|(_, c)| c),
        Some('“' | '‘' | '"' | '\'')
    ) {
        return false;
    }
    let after = text[end..].trim_start_matches(['.', '!', '?']);
    matches!(after.chars().next(), Some('”' | '’' | '"' | '\''))
}
fn listed(kind: &str, word: &str) -> bool {
    PLACEHOLDERS.lines().any(|line| {
        line.split_once('\t')
            .is_some_and(|(k, w)| k == kind && w == word)
    })
}
fn filler(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    if FILLERS.lines().any(|line| line == lower) {
        return true;
    }
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    PATTERNS
        .get_or_init(|| {
            ["^u+m+$", "^u+h+$", "^u+h+m+$", "^e+r+m+$", "^e+h+m+$"]
                .into_iter()
                .map(|pattern| Regex::new(pattern).expect("filler pattern"))
                .collect()
        })
        .iter()
        .any(|pattern| pattern.is_match(&lower))
}
fn pause_term(word: &str) -> &'static str {
    let lower = word.to_ascii_lowercase();
    if lower == "er" {
        "er"
    } else if lower.starts_with('u') && lower.contains('h') && lower.ends_with('m') {
        "uhm"
    } else if lower.starts_with('u') && lower.contains('h') {
        "uh"
    } else if lower.starts_with('u') {
        "um"
    } else if lower.contains('r') {
        "erm"
    } else {
        "ehm"
    }
}
fn mark_allowed(tokens: &[&Token], at: usize) -> bool {
    let text = tokens[at].text.as_str();
    if !matches!(text, "XX" | "XXXX") {
        return true;
    }
    let before = tokens[..at]
        .iter()
        .rev()
        .find(|t| t.is_word)
        .map(|t| t.text.as_str());
    let after = tokens[at + 1..]
        .iter()
        .find(|t| t.is_word)
        .map(|t| t.text.as_str());
    !matches!(before, Some("Chapter" | "Book" | "Part" | "Act" | "Pius"))
        && !matches!(after, Some("chromosome" | "Chromosome"))
}
fn note(text: &str) -> bool {
    let inner = text.trim();
    let words: Vec<&str> = inner.split_whitespace().collect();
    if words.is_empty() || words.len() > 5 {
        return false;
    }
    if inner.ends_with('?') {
        return true;
    }
    let first = words[0].trim_matches(|c: char| !c.is_alphanumeric());
    (words.len() <= 4 && listed("verb", &first.to_ascii_lowercase()))
        || words
            .iter()
            .all(|word| listed("word", word.trim_matches(|c: char| !c.is_alphanumeric())))
        || inner == "TK"
}
fn placeholders(
    paragraph: &Paragraph,
    sentence_at: impl Fn(usize) -> usize,
    document: &Document,
    out: &mut Vec<DraftingMatch>,
) {
    let text = &paragraph.text;
    let mut found: Vec<(usize, usize, String)> = Vec::new();
    for sentence in &paragraph.sentences {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|t| !t.is_whitespace)
            .collect();
        for (at, token) in tokens.iter().enumerate() {
            if !token.is_word
                || token.is_skipped
                || !listed("mark", &token.text)
                || !mark_allowed(&tokens, at)
            {
                continue;
            }
            let start = rel(paragraph, token.span.byte.start);
            let end = rel(paragraph, token.span.byte.end);
            if !quoted_alone(text, start, end) {
                found.push((start, end, token.text.clone()));
            }
        }
    }
    static BRACKET: OnceLock<Regex> = OnceLock::new();
    let bracket = BRACKET.get_or_init(|| {
        Regex::new(r"\[[^\]\n]{1,80}\]|\{\{[^}\n]{1,80}\}\}|<<[^>\n]{1,80}>>|\([^()\n]{1,80}\)")
            .unwrap()
    });
    for m in bracket.find_iter(text) {
        let inner = if m.as_str().starts_with("{{") || m.as_str().starts_with("<<") {
            &m.as_str()[2..m.as_str().len() - 2]
        } else {
            &m.as_str()[1..m.as_str().len() - 1]
        };
        if note(inner) {
            found.push((m.start(), m.end(), m.as_str().to_owned()));
        }
    }
    static QUESTION: OnceLock<Regex> = OnceLock::new();
    let question = QUESTION.get_or_init(|| Regex::new(r"\?{3,}").unwrap());
    for m in question.find_iter(text) {
        if prev(text, m.start()).is_none_or(|(_, c)| !c.is_alphanumeric()) {
            found.push((m.start(), m.end(), "???".into()));
        }
    }
    static PREFIX: OnceLock<Regex> = OnceLock::new();
    let prefix =
        PREFIX.get_or_init(|| Regex::new(r"(?i)^(?:note(?: to self)?|nts|todo):").unwrap());
    if let Some(m) = prefix.find(text) {
        found.push((m.start(), m.end(), m.as_str().to_owned()));
    }
    found.sort_by_key(|(start, end, _)| (*start, std::cmp::Reverse(*end)));
    let mut last = 0;
    for (start, end, term) in found {
        if start < last {
            continue;
        }
        last = end;
        let at = paragraph.span.byte.start + start;
        out.push(DraftingMatch {
            span: span(paragraph, start, end),
            sentence: sentence_at(at),
            category: DraftingCategory::Placeholder,
            term,
            mode: document.mode_at(at),
            edits: Vec::new(),
        });
    }
}

fn pause_edit(paragraph: &Paragraph, core_start: usize, core_end: usize) -> DraftingEdit {
    let text = &paragraph.text;
    let mut start = core_start;
    let mut end = core_end;
    let mut right = end;
    while next(text, right).is_some_and(|c| c == ' ' || c == '\t') {
        right += next(text, right).unwrap().len_utf8();
    }
    let trailing = [",", "…", "...", "—", "–", "--"]
        .into_iter()
        .find(|mark| text[right..].starts_with(mark));
    let mut trailing_kind = None;
    let closing_comma =
        trailing == Some(",") && next(text, right + 1).is_some_and(|c| "”’\")]}".contains(c));
    if let Some(mark) = trailing {
        let after = right + mark.len();
        if !(mark == "," && next(text, after).is_some_and(|c| "”’\")]}".contains(c))) {
            end = after;
            trailing_kind = Some(mark);
        }
    }
    let mut left = start;
    while prev(text, left).is_some_and(|(_, c)| c == ' ' || c == '\t') {
        left = prev(text, left).unwrap().0;
    }
    let before = prev(text, left);
    let paired_dash = trailing_kind.is_some_and(|m| matches!(m, "—" | "–" | "--"))
        && before.is_some_and(|(_, c)| matches!(c, '—' | '–' | '-'));
    if paired_dash {
        start = before.unwrap().0;
        if text[..start].ends_with('-') {
            start -= 1;
        }
    } else if closing_comma && before.is_some_and(|(_, c)| c == ',') {
        start = before.unwrap().0;
    } else if trailing_kind == Some(",") && before.is_some_and(|(_, c)| c == ',') {
        let lead = text[..before.unwrap().0].trim();
        let discourse = ["Well", "Oh", "So", "Yes", "No", "Okay"].contains(&lead)
            || (lead.split_whitespace().count() == 1
                && lead.chars().next().is_some_and(char::is_uppercase));
        if !discourse {
            start = before.unwrap().0;
        }
    }
    while prev(text, start).is_some_and(|(_, c)| c == ' ' || c == '\t') {
        start = prev(text, start).unwrap().0;
    }
    while next(text, end).is_some_and(|c| c == ' ' || c == '\t') {
        end += next(text, end).unwrap().len_utf8();
    }
    let left_context = prev(text, start).map(|(_, c)| c);
    let right_context = next(text, end);
    let separator = if left_context.is_none_or(|c| "“‘\"([\u{2028}".contains(c))
        || right_context.is_none_or(|c| ".,;:!?…”’\")]}\u{2028}".contains(c))
    {
        ""
    } else {
        " "
    };
    let mut replacement = separator.to_owned();
    let at_sentence_start = left_context.is_none_or(|c| ".?!“‘\"".contains(c))
        && !text[..start].ends_with("...")
        && !text[..start].ends_with('…');
    let next_word = text[end..]
        .split(|c: char| !c.is_alphabetic())
        .next()
        .unwrap_or("");
    if at_sentence_start && !filler(next_word) {
        if let Some(c) = right_context.filter(|c| c.is_lowercase()) {
            let mut upper = c.to_uppercase();
            if let (Some(first), None) = (upper.next(), upper.next()) {
                replacement.push(first);
                end += c.len_utf8();
            }
        }
    }
    // Minimize the edit while preserving its exact output.
    let original = &text[start..end];
    let mut head = 0;
    while let (Some(a), Some(b)) = (next(original, head), next(&replacement, head)) {
        if a != b {
            break;
        }
        head += a.len_utf8();
        if head >= original.len() || head >= replacement.len() {
            break;
        }
    }
    start += head;
    replacement = replacement[head..].to_owned();
    let mut original_end = end;
    while let (Some((i, a)), Some((j, b))) = (
        prev(text, original_end),
        prev(&replacement, replacement.len()),
    ) {
        if i < start || a != b {
            break;
        }
        original_end = i;
        replacement.truncate(j);
    }
    DraftingEdit {
        span: span(paragraph, start, original_end),
        replacement,
    }
}

fn pauses(
    paragraph: &Paragraph,
    sentence_offset: usize,
    document: &Document,
    options: &DraftingOptions,
    out: &mut Vec<DraftingMatch>,
) {
    let text = &paragraph.text;
    for (sentence_index, sentence) in paragraph.sentences.iter().enumerate() {
        let tokens: Vec<&Token> = sentence
            .tokens
            .iter()
            .filter(|t| !t.is_whitespace)
            .collect();
        for token in tokens {
            if !token.is_word || token.is_skipped || token.text.chars().all(char::is_uppercase) {
                continue;
            }
            let start = rel(paragraph, token.span.byte.start);
            let end = rel(paragraph, token.span.byte.end);
            let er = token.text.eq_ignore_ascii_case("er");
            if !filler(&token.text) && !(options.er && er) {
                continue;
            }
            if er {
                let before = text[..start].trim_end_matches([' ', '\t']);
                let after = text[end..].trim_start_matches([' ', '\t']);
                let paired = [",", "—", "–", "--"]
                    .into_iter()
                    .any(|mark| before.ends_with(mark))
                    && [",", "—", "–", "--"]
                        .into_iter()
                        .any(|mark| after.starts_with(mark));
                if !(paired || after.starts_with('…') || after.starts_with("...")) {
                    continue;
                }
            }
            if (text[..start].ends_with('-') && !text[..start].ends_with("--"))
                || (text[end..].starts_with('-') && !text[end..].starts_with("--"))
                || quoted_alone(text, start, end)
            {
                continue;
            }
            let mode = document.mode_at(token.span.byte.start);
            if mode == Mode::Dialogue && !options.include_dialogue {
                continue;
            }
            out.push(DraftingMatch {
                span: token.span.clone(),
                sentence: sentence_offset + sentence_index,
                category: DraftingCategory::FilledPause,
                term: pause_term(&token.text).to_owned(),
                mode,
                edits: vec![pause_edit(paragraph, start, end)],
            });
        }
    }
    // The tokenizer can keep a double-dash phrase in one token. Its middle word still has
    // char-boundary offsets, so recover the pause at that punctuation seam.
    static DASHED: OnceLock<Regex> = OnceLock::new();
    let dashed =
        DASHED.get_or_init(|| Regex::new(r"(?i)--(u+m+|u+h+m+|u+h+|e+r+m+|e+h+m+|er)--").unwrap());
    for capture in dashed.captures_iter(text) {
        let middle = capture.get(1).expect("pause capture");
        if middle.as_str().eq_ignore_ascii_case("er") && !options.er {
            continue;
        }
        if middle.as_str().chars().all(char::is_uppercase) {
            continue;
        }
        let at = paragraph.span.byte.start + middle.start();
        let mode = document.mode_at(at);
        if mode == Mode::Dialogue && !options.include_dialogue {
            continue;
        }
        if out.iter().any(|found| found.span.byte.start == at) {
            continue;
        }
        let sentence = paragraph
            .sentences
            .partition_point(|s| s.span.byte.start <= at)
            .saturating_sub(1);
        out.push(DraftingMatch {
            span: span(paragraph, middle.start(), middle.end()),
            sentence: sentence_offset + sentence,
            category: DraftingCategory::FilledPause,
            term: pause_term(middle.as_str()).to_owned(),
            mode,
            edits: vec![pause_edit(paragraph, middle.start(), middle.end())],
        });
    }
}

pub fn report(
    document: &Document,
    options: &DraftingOptions,
) -> Result<DraftingReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let mut matches = Vec::new();
    let mut sentence_offset = 0;
    for paragraph in &document.paragraphs {
        if options.placeholders {
            let starts: Vec<usize> = paragraph
                .sentences
                .iter()
                .map(|s| s.span.byte.start)
                .collect();
            placeholders(
                paragraph,
                |at| {
                    sentence_offset
                        + starts
                            .partition_point(|start| *start <= at)
                            .saturating_sub(1)
                },
                document,
                &mut matches,
            );
        }
        if options.filled_pauses {
            pauses(paragraph, sentence_offset, document, options, &mut matches);
        }
        sentence_offset += paragraph.sentences.len();
    }
    matches.sort_by_key(|m| (m.span.byte.start, m.span.byte.end));
    let mut used_end = 0;
    let mut empty_lead = false;
    for found in &mut matches {
        for edit in &mut found.edits {
            if edit.span.byte.start < used_end && used_end <= found.span.byte.start {
                if let Some(paragraph) = document.paragraphs.iter().find(|p| {
                    p.span.byte.start <= used_end && edit.span.byte.end <= p.span.byte.end
                }) {
                    edit.span = span(
                        paragraph,
                        rel(paragraph, used_end),
                        rel(paragraph, edit.span.byte.end),
                    );
                    if empty_lead && edit.span.byte.start == used_end {
                        edit.replacement.clear();
                        let mut end = rel(paragraph, edit.span.byte.end);
                        while next(&paragraph.text, end).is_some_and(|c| c == ' ' || c == '\t') {
                            end += next(&paragraph.text, end).unwrap().len_utf8();
                        }
                        edit.span = span(paragraph, rel(paragraph, used_end), end);
                    }
                }
            }
        }
        found.edits.retain(|edit| edit.span.byte.start >= used_end);
        if let Some(last) = found.edits.last() {
            used_end = last.span.byte.end;
            empty_lead = last.replacement.is_empty()
                && document
                    .paragraphs
                    .iter()
                    .any(|p| p.span.byte.start == last.span.byte.start);
        }
    }
    let mut terms: BTreeMap<(DraftingCategory, String), DraftingTerm> = BTreeMap::new();
    let mut groups = word_split(document).map(|words| DraftingGroup {
        words,
        ..Default::default()
    });
    for found in &matches {
        let term = terms
            .entry((found.category, found.term.clone()))
            .or_insert_with(|| DraftingTerm {
                term: found.term.clone(),
                category: found.category,
                count: 0,
                narration: 0,
                dialogue: 0,
            });
        term.count += 1;
        match found.mode {
            Mode::Narration => term.narration += 1,
            Mode::Dialogue => term.dialogue += 1,
        }
        for group in groups.all_and_mode_mut(found.mode) {
            group.matches += 1;
            group.edits += found.edits.len();
        }
    }
    for group in [&mut groups.all, &mut groups.narration, &mut groups.dialogue] {
        group.per_thousand_words =
            (group.words > 0).then(|| 1000.0 * group.matches as f64 / group.words as f64);
    }
    let mut terms: Vec<_> = terms.into_values().collect();
    terms.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.term.cmp(&b.term)));
    Ok(DraftingReport {
        matches,
        terms,
        groups,
    })
}
