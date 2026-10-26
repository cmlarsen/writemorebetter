use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::reports::{consistency, word_split, Split};
use crate::{Document, Mode, Paragraph, Span, StyleError, Token};

pub const REPORT: &str = "drafting";
const FILLERS: &str = include_str!("../../data/drafting/fillers.txt");
const PLACEHOLDERS: &str = include_str!("../../data/drafting/placeholders.tsv");
const COMMANDS: &str = include_str!("../../data/drafting/commands.tsv");

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
    pub spoken_commands: bool,
    pub filled_pauses: bool,
    pub er: bool,
    pub include_dialogue: bool,
}
impl Default for DraftingOptions {
    fn default() -> Self {
        Self {
            placeholders: true,
            spoken_commands: false,
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
fn previous_word(text: &str, at: usize) -> &str {
    let mut end = at;
    while let Some((start, ch)) = prev(text, end) {
        if ch.is_alphabetic() {
            break;
        }
        end = start;
    }
    let mut start = end;
    while let Some((at, ch)) = prev(text, start) {
        if !ch.is_alphabetic() {
            break;
        }
        start = at;
    }
    &text[start..end]
}
fn quoted_alone(text: &str, start: usize, end: usize) -> bool {
    if !matches!(
        prev(text, start).map(|(_, c)| c),
        Some('“' | '‘' | '"' | '\'')
    ) {
        return false;
    }
    let after = text[end..].trim_start_matches(['.', '!', '?', ',']);
    matches!(after.chars().next(), Some('”' | '’' | '"' | '\''))
}
struct PlaceholderLists {
    marks: HashSet<&'static str>,
    words: HashSet<String>,
    verbs: HashSet<String>,
}
fn placeholder_lists() -> &'static PlaceholderLists {
    static LISTS: OnceLock<PlaceholderLists> = OnceLock::new();
    LISTS.get_or_init(|| {
        let mut lists = PlaceholderLists {
            marks: HashSet::new(),
            words: HashSet::new(),
            verbs: HashSet::new(),
        };
        for line in PLACEHOLDERS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
        {
            let (kind, value) = line.split_once('\t').expect("placeholder TSV");
            match kind {
                "mark" => {
                    lists.marks.insert(value);
                }
                "word" => {
                    lists.words.insert(value.to_ascii_lowercase());
                }
                "verb" => {
                    lists.verbs.insert(value.to_ascii_lowercase());
                }
                _ => unreachable!("placeholder kind"),
            }
        }
        lists
    })
}
fn listed(kind: &str, word: &str) -> bool {
    let lists = placeholder_lists();
    match kind {
        "mark" => lists.marks.contains(word),
        "word" => lists.words.contains(&word.to_ascii_lowercase()),
        "verb" => lists.verbs.contains(&word.to_ascii_lowercase()),
        _ => false,
    }
}
fn filler(word: &str) -> bool {
    let lower = word.to_ascii_lowercase();
    static LIST: OnceLock<HashSet<&'static str>> = OnceLock::new();
    if LIST
        .get_or_init(|| {
            FILLERS
                .lines()
                .filter(|line| !line.starts_with('#') && !line.is_empty())
                .collect()
        })
        .contains(lower.as_str())
    {
        return true;
    }
    filler_pattern().is_match(&lower)
}
fn filler_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"^(?:u+m+|u+h+m+|u+h+|e+r+m+|e+h+m+)$").expect("filled-pause pattern")
    })
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
    if !matches!(text, "XX" | "XXX" | "XXXX") {
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
    !matches!(
        before,
        Some("Chapter" | "Book" | "Part" | "Act" | "Pius" | "Volume")
    ) && !(text == "XXX"
        && tokens[..at]
            .iter()
            .rev()
            .find(|token| token.is_word)
            .is_some_and(|token| has_pos(token, "NNP")))
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

struct CommandEntry {
    phrase: &'static str,
    words: Vec<&'static str>,
    mark: &'static str,
    guard: &'static str,
}

fn command_entries() -> &'static [CommandEntry] {
    static ENTRIES: OnceLock<Vec<CommandEntry>> = OnceLock::new();
    ENTRIES.get_or_init(|| {
        let mut entries: Vec<_> = COMMANDS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.is_empty())
            .map(|line| {
                let mut fields = line.split('\t');
                let phrase = fields.next().expect("command phrase");
                CommandEntry {
                    phrase,
                    words: phrase.split_whitespace().collect(),
                    mark: fields.next().expect("command mark"),
                    guard: fields.next().expect("command guard"),
                }
            })
            .collect();
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.words.len()));
        entries
    })
}
fn command_index() -> &'static HashMap<&'static str, Vec<&'static CommandEntry>> {
    static INDEX: OnceLock<HashMap<&'static str, Vec<&'static CommandEntry>>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let mut index: HashMap<&'static str, Vec<&'static CommandEntry>> = HashMap::new();
        for entry in command_entries() {
            index.entry(entry.words[0]).or_default().push(entry);
        }
        index
    })
}
struct CommandCandidate {
    entry: &'static CommandEntry,
    start: usize,
    end: usize,
    sentence: usize,
    first_word: usize,
    last_word: usize,
}
struct Marks {
    double_open: &'static str,
    double_close: &'static str,
    single_open: &'static str,
    single_close: &'static str,
    apostrophe: &'static str,
    ellipsis: &'static str,
}

fn marks(document: &Document) -> Marks {
    let report = consistency::report(document, &consistency::ConsistencyOptions::new())
        .expect("default consistency options");
    let curly = |check: &consistency::ConsistencyCheck| {
        check.variants[0].counts.all >= check.variants[1].counts.all
    };
    let double_curly = curly(&report.double_quotes);
    let single_curly = curly(&report.single_quotes);
    let apostrophe_curly = curly(&report.apostrophes);
    let character = report.ellipses.variants[0].counts.all
        >= report.ellipses.variants[1].counts.all + report.ellipses.variants[2].counts.all;
    Marks {
        double_open: if double_curly { "“" } else { "\"" },
        double_close: if double_curly { "”" } else { "\"" },
        single_open: if single_curly { "‘" } else { "'" },
        single_close: if single_curly { "’" } else { "'" },
        apostrophe: if apostrophe_curly { "’" } else { "'" },
        ellipsis: if character { "…" } else { "..." },
    }
}

impl Marks {
    fn replacement(&self, mark: &str) -> Option<&'static str> {
        match mark {
            "comma" => Some(","),
            "period" => Some("."),
            "question" => Some("?"),
            "exclamation" => Some("!"),
            "colon" => Some(":"),
            "semicolon" => Some(";"),
            "dash" => Some("—"),
            "hyphen" => Some("-"),
            "ellipsis" => Some(self.ellipsis),
            "openDouble" => Some(self.double_open),
            "closeDouble" => Some(self.double_close),
            "openSingle" => Some(self.single_open),
            "closeSingle" => Some(self.single_close),
            "openParen" => Some("("),
            "closeParen" => Some(")"),
            "openBracket" => Some("["),
            "closeBracket" => Some("]"),
            "apostrophe" => Some(self.apostrophe),
            "newline" => Some("\n"),
            _ => None,
        }
    }
}

fn has_pos(token: &Token, prefix: &str) -> bool {
    token
        .readings
        .iter()
        .any(|reading| reading.pos.starts_with(prefix))
}
fn word(text: &str) -> String {
    text.to_ascii_lowercase()
}

fn command_guard(
    candidate: &CommandCandidate,
    words: &[(&Token, usize)],
    paragraph: &Paragraph,
) -> bool {
    let entry = candidate.entry;
    let before = candidate
        .first_word
        .checked_sub(1)
        .map(|index| words[index].0);
    let after = words.get(candidate.last_word + 1).map(|(token, _)| *token);
    let previous = before.map(|token| word(&token.text));
    let next_word = after.map(|token| word(&token.text));
    let next_content = if next_word.as_deref().is_some_and(filler) {
        words
            .get(candidate.last_word + 2)
            .map(|(token, _)| word(&token.text))
    } else {
        next_word.clone()
    };
    if matches!(previous.as_deref(), Some("word" | "term")) {
        return false;
    }
    if entry.words.len() == 1
        && paragraph.text[..candidate.start].trim_end().ends_with(',')
        && previous
            .as_deref()
            .is_some_and(|value| command_index().contains_key(value))
        && next_word
            .as_deref()
            .is_some_and(|value| command_index().contains_key(value))
    {
        return false;
    }
    if entry.words.len() > 1
        && candidate.start
            == rel(
                paragraph,
                paragraph.sentences[candidate.sentence].span.byte.start,
            )
        && matches!(next_word.as_deref(), Some("is" | "was" | "means" | "were"))
    {
        return false;
    }
    if entry.mark == "apostrophe"
        && next_word.as_deref() != Some("s")
        && !paragraph.text[candidate.end..].trim().is_empty()
    {
        return false;
    }
    // "in all caps" and "in no caps" describe lettering; the command precedes the words it caps.
    if matches!(entry.phrase, "all caps" | "no caps")
        && matches!(previous.as_deref(), Some("in" | "of"))
    {
        return false;
    }
    if entry.phrase == "all caps"
        && (candidate.start
            == rel(
                paragraph,
                paragraph.sentences[candidate.sentence].span.byte.start,
            )
            || next_word.as_deref() == Some("on"))
    {
        return false;
    }
    if entry.mark == "dash" {
        let lead = paragraph.text[..candidate.start].trim_end();
        if lead.is_empty()
            || lead.ends_with(['.', '!', '?', '“', '”', '"'])
            || matches!(
                next_word.as_deref(),
                Some("my" | "your" | "himself" | "herself" | "themselves")
            )
        {
            return false;
        }
    }
    if entry.mark == "delete"
        && (paragraph.text[candidate.end..]
            .trim_start_matches([' ', '\t'])
            .chars()
            .next()
            .is_some_and(char::is_alphabetic)
            || before.is_some_and(|token| has_pos(token, "MD"))
            || matches!(
                previous.as_deref(),
                Some("i" | "you" | "he" | "she" | "we" | "they")
            )
            || ["can't", "can’t", "don't", "don’t"]
                .iter()
                .any(|end| paragraph.text[..candidate.start].trim_end().ends_with(end)))
    {
        return false;
    }
    // A verb use: an infinitive or coordinated verb (*likes to scratch that*, *and delete that*),
    // a question (*Did Mary delete that?*), or an auxiliary or modal earlier in the sentence.
    if entry.mark == "delete"
        && (matches!(
            previous.as_deref(),
            Some("to" | "and" | "or" | "but" | "nor")
        ) || paragraph.text[candidate.end..]
            .trim_start_matches(horizontal)
            .starts_with(['?', '!'])
            || words[..candidate.first_word]
                .iter()
                .rev()
                .take_while(|(_, sentence)| *sentence == candidate.sentence)
                .any(|(token, _)| {
                    matches!(word(&token.text).as_str(), "do" | "does" | "did")
                        || has_pos(token, "MD")
                }))
    {
        return false;
    }
    if entry.phrase == "no caps"
        && matches!(
            next_word.as_deref(),
            Some("at" | "on" | "in" | "of" | "for" | "with")
        )
    {
        return false;
    }
    if entry.guard == "unquote" {
        let prefix = paragraph.text[..candidate.start].trim_end();
        if prefix.ends_with("quote,") || prefix.ends_with("quote-") {
            return false;
        }
    }
    if matches!(entry.phrase, "period" | "full stop") {
        let sentence = &paragraph.sentences[candidate.sentence];
        if matches!(
            sentence.text.trim().to_ascii_lowercase().as_str(),
            "period." | "full stop."
        ) {
            return false;
        }
    }
    {
        let prior_typed_mark = match entry.mark {
            "comma" => paragraph.text[..candidate.start].trim_end().ends_with(','),
            "period" => paragraph.text[..candidate.start].trim_end().ends_with('.'),
            "dash" => paragraph.text[..candidate.start].trim_end().ends_with('—'),
            _ => false,
        };
        let prior_other_mark = paragraph.text[..candidate.start]
            .trim_end_matches([' ', '\t'])
            .chars()
            .next_back()
            .is_some_and(|ch| ",.;:!?—–".contains(ch));
        if entry.words.len() == 1 && prior_other_mark && !prior_typed_mark {
            return false;
        }
        let preceding_verb = candidate.first_word >= 2
            && has_pos(words[candidate.first_word - 2].0, "VB")
            && !has_pos(words[candidate.first_word - 2].0, "DT");
        let adjective_modifier = before.is_some_and(|token| has_pos(token, "JJ"))
            && !before.is_some_and(|token| has_pos(token, "VB") || has_pos(token, "VBG"))
            && !preceding_verb;
        let prev_guard = !prior_typed_mark
            && (before.is_some_and(|token| {
                has_pos(token, "DT")
                    || (has_pos(token, "PRP$") && token.text != "mine")
                    || has_pos(token, "CD")
                    || token.text.chars().all(|ch| ch.is_ascii_digit())
                    || (adjective_modifier && entry.mark != "flag")
            }) || (candidate.first_word >= 2
                && entry.mark != "flag"
                && before.is_some_and(|token| has_pos(token, "JJ") || has_pos(token, "NNP"))
                && has_pos(words[candidate.first_word - 2].0, "DT"))
                || matches!(previous.as_deref(), Some("of" | "by"))
                || matches!(
                    previous.as_deref(),
                    Some("a" | "an" | "the" | "her" | "his" | "their" | "my" | "our")
                ));
        let next_guard = matches!(
            next_content.as_deref(),
            Some("of" | "in" | "for" | "between" | "by" | "near")
        );
        let object_guard = matches!(previous.as_deref(), Some("made" | "took" | "put" | "used"));
        let dash_guard = entry.guard == "dash"
            && (previous.as_deref() == Some("to")
                || before.is_some_and(|token| has_pos(token, "MD")));
        if prev_guard || next_guard || object_guard || dash_guard {
            return false;
        }
    }
    true
}

fn positive_cue(
    candidate: &CommandCandidate,
    words: &[(&Token, usize)],
    paragraph: &Paragraph,
    other: bool,
    paired: bool,
) -> bool {
    let text = &paragraph.text;
    let begin = rel(
        paragraph,
        paragraph.sentences[candidate.sentence].span.byte.start,
    );
    // A command that starts a sentence, after any quotes, brackets or filled pauses, is ordinary
    // prose (*Full stop signs*, *Um comma the dog*), except one of a dictated pair of marks
    // (*open quote … close quote*) and a line break before a capital.
    if text[begin..candidate.start]
        .split(|ch: char| !ch.is_alphanumeric())
        .all(|lead| lead.is_empty() || filler(lead))
        || sentence_start(text, candidate.start)
    {
        return match candidate.entry.mark {
            "newline" => words
                .get(candidate.last_word + 1)
                .is_none_or(|(token, _)| token.text.starts_with(char::is_uppercase)),
            _ => paired,
        };
    }
    // Mid-sentence, a capitalized command word is a name (*His brother Colon then left*).
    if text[candidate.start..].starts_with(char::is_uppercase) {
        return false;
    }
    if candidate.entry.words.len() != 1 {
        return true;
    }
    let after = words.get(candidate.last_word + 1).map(|(token, _)| *token);
    let next_word = after.map(|token| word(&token.text));
    let clause = after.is_some_and(|token| {
        (has_pos(token, "PRP") && !has_pos(token, "PRP$"))
            || has_pos(token, "DT")
            || has_pos(token, "NNP")
            || token.text.chars().next().is_some_and(char::is_uppercase)
    }) || matches!(next_word.as_deref(), Some("and" | "but" | "so"))
        && words
            .get(candidate.last_word + 2)
            .is_some_and(|(token, _)| {
                has_pos(token, "PRP")
                    || candidate.entry.mark == "comma"
                        && (has_pos(token, "JJ") || has_pos(token, "DT"))
                    || candidate.entry.mark == "dash"
                        && (has_pos(token, "VB") || has_pos(token, "RB"))
            });
    let then_clause = matches!(next_word.as_deref(), Some("then"))
        && words
            .get(candidate.last_word + 2)
            .is_some_and(|(token, _)| has_pos(token, "VB"));
    let conjunction_then = matches!(next_word.as_deref(), Some("and" | "but" | "so"))
        && words
            .get(candidate.last_word + 2)
            .is_some_and(|(token, _)| token.text.eq_ignore_ascii_case("then"))
        && words
            .get(candidate.last_word + 3)
            .is_some_and(|(token, _)| has_pos(token, "PRP") || has_pos(token, "DT"));
    let after_pause = next_word.as_deref().is_some_and(filler)
        && words
            .get(candidate.last_word + 2)
            .is_some_and(|(token, _)| {
                has_pos(token, "PRP") || token.text.chars().next().is_some_and(char::is_uppercase)
            });
    // A list or compound joins two items. The word before must be a noun or adjective that isn't
    // the verb after a subject (*Teachers mark comma errors*); after a verb such as *has* or
    // *hated*, the command word modifies the next noun (*has colon cancer*, *hated hyphen words*).
    let before = candidate
        .first_word
        .checked_sub(1)
        .map(|index| words[index].0);
    let subject = candidate.first_word >= 2 && {
        let token = words[candidate.first_word - 2].0;
        has_pos(token, "NN") || has_pos(token, "PRP") && !has_pos(token, "PRP$")
    };
    let list = matches!(candidate.entry.mark, "comma" | "colon" | "semicolon")
        && before.is_some_and(|token| has_pos(token, "NN") || has_pos(token, "JJ"))
        && !subject
        && after.is_some_and(|token| token.is_word && !has_pos(token, "RB"))
        && words
            .get(candidate.last_word + 2)
            .is_none_or(|(token, _)| has_pos(token, "NN") || token.text == "and");
    let after_filler =
        candidate.entry.mark == "comma" && before.is_some_and(|token| filler(&token.text));
    let compound = candidate.entry.mark == "hyphen"
        && before.is_some_and(|token| has_pos(token, "NN") || has_pos(token, "JJ"))
        && !subject
        && after.is_some_and(|token| has_pos(token, "NN"));
    let apostrophe = candidate.entry.mark == "apostrophe" && next_word.as_deref() == Some("s");
    // The dictation engine typed the mark on both sides too (*again, comma, and*).
    let doubled = candidate.entry.mark == "comma"
        && text[..candidate.start].trim_end().ends_with(',')
        && text[candidate.end..].starts_with(',');
    clause
        || after_pause
        || then_clause
        || conjunction_then
        || list
        || compound
        || apostrophe
        || after_filler
        || doubled
        || paragraph.text[candidate.end..].trim().is_empty()
        || other
}

fn command_candidates(paragraph: &Paragraph) -> Vec<CommandCandidate> {
    let words: Vec<(&Token, usize)> = paragraph
        .sentences
        .iter()
        .enumerate()
        .flat_map(|(sentence, item)| {
            item.tokens
                .iter()
                .filter(|token| token.is_word)
                .map(move |token| (token, sentence))
        })
        .collect();
    let mut candidates = Vec::new();
    let mut at = 0;
    while at < words.len() {
        let folded = word(&words[at].0.text);
        let found = command_index()
            .get(folded.as_str())
            .into_iter()
            .flatten()
            .copied()
            .find(|entry| {
                if at + entry.words.len() > words.len() {
                    return false;
                }
                entry.words.iter().enumerate().all(|(offset, expected)| {
                    let token = words[at + offset].0;
                    if token.is_skipped || !token.text.eq_ignore_ascii_case(expected) {
                        return false;
                    }
                    offset == 0
                        || paragraph.text[rel(paragraph, words[at + offset - 1].0.span.byte.end)
                            ..rel(paragraph, token.span.byte.start)]
                            .chars()
                            .all(char::is_whitespace)
                })
            });
        if let Some(entry) = found {
            let last = at + entry.words.len() - 1;
            let start = rel(paragraph, words[at].0.span.byte.start);
            let end = rel(paragraph, words[last].0.span.byte.end);
            if !quoted_alone(&paragraph.text, start, end) {
                candidates.push(CommandCandidate {
                    entry,
                    start,
                    end,
                    sentence: words[at].1,
                    first_word: at,
                    last_word: last,
                });
            }
            at = last + 1;
        } else {
            at += 1;
        }
    }
    let openings: Vec<(&str, usize)> = candidates
        .iter()
        .filter(|candidate| candidate.entry.mark.starts_with("open"))
        .map(|candidate| (candidate.entry.mark, candidate.start))
        .collect();
    candidates.retain(|candidate| {
        (matches!(
            candidate.entry.mark,
            "closeBracket" | "closeParen" | "closeDouble" | "closeSingle"
        ) && openings.iter().any(|(mark, start)| {
            *start < candidate.start
                && candidate.entry.mark.strip_prefix("close") == mark.strip_prefix("open")
        })) || command_guard(candidate, &words, paragraph)
    });
    let paired_opens: Vec<usize> = candidates
        .iter()
        .filter(|candidate| candidate.entry.mark == "openDouble")
        .map(|candidate| candidate.start)
        .collect();
    candidates.retain(|candidate| {
        candidate.entry.guard != "unquote"
            || paired_opens.iter().any(|start| *start < candidate.start)
    });
    let count = candidates.len();
    let marks: Vec<(&str, usize)> = candidates
        .iter()
        .map(|candidate| (candidate.entry.mark, candidate.start))
        .collect();
    candidates.retain(|candidate| {
        // An opening mark with a later closing command, or a closing mark after an opening one.
        let paired = marks.iter().any(|(mark, start)| {
            match (
                candidate.entry.mark.strip_prefix("open"),
                candidate.entry.mark.strip_prefix("close"),
            ) {
                (Some(kind), _) => {
                    mark.strip_prefix("close") == Some(kind) && *start > candidate.start
                }
                (_, Some(kind)) => {
                    mark.strip_prefix("open") == Some(kind) && *start < candidate.start
                }
                _ => false,
            }
        });
        positive_cue(candidate, &words, paragraph, count > 1, paired)
    });
    candidates
}

fn command_edit(
    paragraph: &Paragraph,
    candidate: &CommandCandidate,
    marks: &Marks,
) -> Vec<DraftingEdit> {
    let mark = marks
        .replacement(candidate.entry.mark)
        .expect("command mark");
    let text = &paragraph.text;
    let is_open_mark = candidate.entry.mark.starts_with("open");
    let mut start = candidate.start;
    if !is_open_mark {
        while prev(text, start).is_some_and(|(_, ch)| ch == ' ' || ch == '\t') {
            start = prev(text, start).unwrap().0;
        }
    }
    let mut end = candidate.end;
    let consume_after = is_open_mark
        || matches!(candidate.entry.mark, "apostrophe" | "dash" | "newline")
        || candidate.entry.mark == "hyphen"
            && text[candidate.end..]
                .trim_start_matches([' ', '\t'])
                .chars()
                .next()
                .is_some_and(char::is_lowercase);
    if consume_after {
        while next(text, end).is_some_and(|ch| ch == ' ' || ch == '\t') {
            end += next(text, end).unwrap().len_utf8();
        }
    }
    let already = if mark == "…" || mark == "..." {
        text[candidate.end..].starts_with('…') || text[candidate.end..].starts_with("...")
    } else if matches!(candidate.entry.mark, "openDouble" | "closeDouble") {
        text[candidate.end..].starts_with(['“', '”', '"'])
    } else if matches!(
        candidate.entry.mark,
        "openSingle" | "closeSingle" | "apostrophe"
    ) {
        text[candidate.end..].starts_with(['‘', '’', '\''])
    } else {
        text[candidate.end..].starts_with(mark)
    };
    let paired_mark = matches!(mark, "," | "." | "?" | "!" | ":" | ";" | "—" | "-")
        && text[..start].ends_with(mark);
    if paired_mark && already {
        end = candidate.end + mark.len();
    }
    let mut replacement = if already || paired_mark {
        String::new()
    } else {
        mark.to_owned()
    };
    let terminal = matches!(candidate.entry.mark, "period" | "question" | "exclamation");
    if terminal && !already {
        let remaining = &text[end..];
        let gap = remaining.len() - remaining.trim_start_matches([' ', '\t']).len();
        let next_word = remaining[gap..]
            .split(|ch: char| !ch.is_alphabetic())
            .next()
            .unwrap_or("");
        if !filler(next_word) {
            if let Some(letter) = next(text, end + gap).filter(|ch| ch.is_lowercase()) {
                let mut upper = letter.to_uppercase();
                if let (Some(upper), None) = (upper.next(), upper.next()) {
                    replacement.push_str(&remaining[..gap]);
                    replacement.push(upper);
                    end += gap + letter.len_utf8();
                }
            }
        }
    }
    let mut edits = vec![DraftingEdit {
        span: span(paragraph, start, end),
        replacement,
    }];
    if terminal && already {
        let mut after_mark = candidate.end
            + if text[candidate.end..].starts_with("...") {
                3
            } else {
                mark.len()
            };
        while next(text, after_mark).is_some_and(|ch| ch == ' ' || ch == '\t') {
            after_mark += next(text, after_mark).unwrap().len_utf8();
        }
        if let Some(letter) = next(text, after_mark).filter(|ch| ch.is_lowercase()) {
            let mut upper = letter.to_uppercase();
            if let (Some(upper), None) = (upper.next(), upper.next()) {
                edits.push(DraftingEdit {
                    span: span(paragraph, after_mark, after_mark + letter.len_utf8()),
                    replacement: upper.to_string(),
                });
            }
        }
    }
    edits
}

fn dictated_speech(
    paragraph: &Paragraph,
    candidates: &[CommandCandidate],
) -> Vec<std::ops::Range<usize>> {
    let mut speech = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if matches!(candidate.entry.mark, "openDouble") {
            if let Some(close) = candidates[index + 1..]
                .iter()
                .find(|item| item.entry.mark == "closeDouble")
            {
                speech.push(
                    paragraph.span.byte.start + candidate.end
                        ..paragraph.span.byte.start + close.start,
                );
            }
        }
    }
    speech
}

fn horizontal(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}
fn opening_straight_quote(text: &str, quote: usize) -> bool {
    prev(text, quote).is_none_or(|(_, ch)| !ch.is_alphanumeric() && !".,;:!?)]}”’".contains(ch))
}
/// The paragraph start, after terminal punctuation (and any closing quotes) plus a space, or
/// right after an opening quote.
fn sentence_start(text: &str, at: usize) -> bool {
    let before = text[..at].trim_end_matches(horizontal);
    let Some((last_at, last)) = prev(before, before.len()) else {
        return true;
    };
    if before.len() == at {
        return matches!(last, '“' | '‘')
            || matches!(last, '"' | '\'') && opening_straight_quote(text, last_at);
    }
    let core = before.trim_end_matches(['”', '’', '"', '\'', ')', ']']);
    core.ends_with(['.', '!', '?']) && !core.ends_with("..")
}
fn next_word(text: &str, at: usize) -> &str {
    text[at..]
        .split(|ch: char| !ch.is_alphabetic())
        .next()
        .unwrap_or("")
}
/// The upper case of the lowercase letter at `at` and its length, when it is exactly one char.
fn capital(text: &str, at: usize) -> Option<(char, usize)> {
    let letter = next(text, at).filter(|ch| ch.is_lowercase())?;
    let mut upper = letter.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(upper), None) => Some((upper, letter.len_utf8())),
        _ => None,
    }
}
fn word_token(paragraph: &Paragraph, at: usize, ending: bool) -> Option<&Token> {
    paragraph
        .sentences
        .iter()
        .flat_map(|sentence| &sentence.tokens)
        .find(|token| {
            let edge = if ending {
                token.span.byte.end
            } else {
                token.span.byte.start
            };
            token.is_word && rel(paragraph, edge) == at
        })
}
/// An edit replacing `start..end`, narrowed past any text the replacement keeps at either end.
fn trimmed_edit(
    paragraph: &Paragraph,
    mut start: usize,
    mut end: usize,
    mut replacement: String,
) -> DraftingEdit {
    let text = &paragraph.text;
    while let (Some(kept), Some(new)) = (next(&text[..end], start), replacement.chars().next()) {
        if kept != new {
            break;
        }
        start += kept.len_utf8();
        replacement.remove(0);
    }
    while let (Some((at, kept)), Some(new)) = (
        prev(text, end).filter(|(at, _)| *at >= start),
        replacement.chars().next_back(),
    ) {
        if kept != new {
            break;
        }
        end = at;
        replacement.pop();
    }
    DraftingEdit {
        span: span(paragraph, start, end),
        replacement,
    }
}

const DISCOURSE: &[&str] = &[
    "Well", "Oh", "So", "Yes", "Yeah", "No", "Okay", "OK", "Ok", "Now", "Right", "Sure", "Look",
    "Listen",
];

/// The edit that deletes the filled pause or `scratch that` at `start..end`, or `None` where the
/// punctuation around it is ambiguous and the leftover is flag only. A `run` of several pauses
/// is deleted only between plain spaces.
fn deletion(paragraph: &Paragraph, start: usize, end: usize, run: bool) -> Option<DraftingEdit> {
    let text = &paragraph.text;
    let left = text[..start].trim_end_matches(horizontal).len();
    let right = text.len() - text[end..].trim_start_matches(horizontal).len();
    let before = prev(text, left).map(|(_, ch)| ch);
    let after = next(text, right);
    // (a) Spaces on both sides, or a space before and terminal punctuation right after.
    if left < start && before.is_some_and(char::is_alphanumeric) {
        let spaced = right > end && after.is_some_and(char::is_alphanumeric);
        let terminal = right == end
            && after.is_some_and(|ch| matches!(ch, '.' | '!' | '?'))
            && !text[end..].starts_with("..");
        if spaced || terminal {
            return Some(trimmed_edit(paragraph, left, end, String::new()));
        }
    }
    if run {
        return None;
    }
    // (b) A sentence start, then one comma, an ellipsis or nothing, then a word to capitalize.
    if sentence_start(text, start) {
        let mark = [",", "…", "..."]
            .into_iter()
            .find(|mark| text[end..].starts_with(mark))
            .map_or(0, str::len);
        let word = text.len() - text[end + mark..].trim_start_matches(horizontal).len();
        if (mark > 0 || word > end) && next(text, word).is_some_and(char::is_alphabetic) {
            let mut replacement = String::new();
            let mut stop = word;
            if !filler(next_word(text, word)) {
                if let Some((upper, len)) = capital(text, word) {
                    replacement.push(upper);
                    stop += len;
                }
            }
            return Some(trimmed_edit(paragraph, start, stop, replacement));
        }
        return None;
    }
    // (c) and (d): a comma pair, `X, um, y`.
    if before != Some(',') || left == start || !text[end..].starts_with(',') {
        return None;
    }
    let comma = left - 1;
    let word = text.len() - text[end + 1..].trim_start_matches(horizontal).len();
    if word == end + 1 || !next(text, word).is_some_and(char::is_alphabetic) {
        return None;
    }
    let first = text[..comma]
        .trim_end_matches(|ch: char| ch.is_alphanumeric() || matches!(ch, '\'' | '’'))
        .len();
    if first == comma {
        return None;
    }
    let left_word = &text[first..comma];
    // (c) After a sentence-initial discourse word, keep its comma.
    if DISCOURSE.contains(&left_word) && sentence_start(text, first) {
        return Some(trimmed_edit(paragraph, start, word, String::new()));
    }
    // (d) Elsewhere both commas go, unless a noun or name before or a conjunction or preposition
    // after suggests the first comma is the author's (*bread, um, and milk*, *Mary, um, I*).
    let noun = word_token(paragraph, comma, true).is_none_or(|token| has_pos(token, "NN"))
        || left_word.starts_with(char::is_uppercase)
            && left_word != "I"
            && !sentence_start(text, first);
    let joins = word_token(paragraph, word, false).is_none_or(|token| {
        ["CC", "IN", "TO", "RP"]
            .into_iter()
            .any(|tag| has_pos(token, tag))
    });
    if noun
        || joins
        || !text[word..].starts_with(char::is_lowercase)
        || filler(next_word(text, word))
    {
        return None;
    }
    Some(trimmed_edit(paragraph, comma, end + 1, String::new()))
}

/// A filled-pause run or an accepted command, with the index of its match.
struct Leftover<'a> {
    start: usize,
    end: usize,
    kind: LeftoverKind<'a>,
    found: usize,
}
#[derive(Clone, Copy)]
enum LeftoverKind<'a> {
    /// One filled pause, or a `run` of several in one match.
    Pause {
        run: bool,
    },
    Command(&'a CommandCandidate),
}
fn joined(gap: &str) -> bool {
    !gap.chars()
        .any(|ch| ch.is_alphanumeric() || ch == '\u{2028}')
}

/// The edits for one cluster of leftovers, carried by its first match, or `None` when the
/// cluster is flag only.
fn cluster_edit(
    paragraph: &Paragraph,
    members: &[Leftover],
    marks: Option<&Marks>,
) -> Option<Vec<DraftingEdit>> {
    let deletable = |member: &Leftover| match member.kind {
        LeftoverKind::Pause { run } => deletion(paragraph, member.start, member.end, run),
        LeftoverKind::Command(_) => deletion(paragraph, member.start, member.end, false),
    };
    if let [only] = members {
        return match only.kind {
            LeftoverKind::Command(candidate) if candidate.entry.mark != "delete" => {
                Some(command_edit(paragraph, candidate, marks?))
            }
            _ => deletable(only).map(|edit| vec![edit]),
        };
    }
    let text = &paragraph.text;
    if members.windows(2).any(|pair| {
        let gap = &text[pair[0].end..pair[1].start];
        gap.is_empty() || !gap.chars().all(horizontal)
    }) {
        return None;
    }
    let mut keys: Vec<&str> = Vec::new();
    for member in members {
        match member.kind {
            LeftoverKind::Command(candidate) if candidate.entry.mark != "delete" => {
                if keys.last() != Some(&candidate.entry.mark) {
                    keys.push(candidate.entry.mark);
                }
            }
            _ => {
                deletable(member)?;
            }
        }
    }
    let first = members[0].start;
    let last = members[members.len() - 1].end;
    if keys.is_empty() {
        return deletion(paragraph, first, last, true).map(|edit| vec![edit]);
    }
    compose(paragraph, first, last, &keys, marks?).map(|edit| vec![edit])
}

/// One edit for commands with deletable leftovers between them: the marks in order, each spaced
/// as it would be alone. `None` unless the cluster sits between words, or where two sentence
/// marks would meet (*left period um comma then*).
fn compose(
    paragraph: &Paragraph,
    first: usize,
    last: usize,
    keys: &[&str],
    marks: &Marks,
) -> Option<DraftingEdit> {
    let text = &paragraph.text;
    let left = text[..first].trim_end_matches(horizontal).len();
    let right = text.len() - text[last..].trim_start_matches(horizontal).len();
    let before = prev(text, left).map(|(_, ch)| ch);
    let after = next(text, right);
    let sentence_mark = |key: &str| {
        matches!(
            key,
            "comma"
                | "period"
                | "question"
                | "exclamation"
                | "colon"
                | "semicolon"
                | "ellipsis"
                | "dash"
                | "hyphen"
        )
    };
    if before.is_some_and(|ch| !ch.is_alphanumeric())
        || after.is_some_and(|ch| !ch.is_alphanumeric())
        || before.is_none() && !keys[0].starts_with("open")
        || keys.contains(&"apostrophe")
        || keys
            .windows(2)
            .any(|pair| sentence_mark(pair[0]) && sentence_mark(pair[1]))
    {
        return None;
    }
    let mut replacement = String::new();
    let mut space = left < first;
    for key in keys {
        if key.starts_with("open") {
            if space {
                replacement.push(' ');
            }
            space = false;
        } else {
            space = !matches!(*key, "dash" | "hyphen" | "newline");
        }
        replacement.push_str(marks.replacement(key)?);
    }
    let mut end = right;
    if space && after.is_some() {
        replacement.push(' ');
    }
    let terminal = keys
        .iter()
        .rfind(|key| **key != "newline")
        .is_some_and(|key| matches!(*key, "period" | "question" | "exclamation"));
    if terminal && after.is_some() && !filler(next_word(text, right)) {
        if let Some((upper, len)) = capital(text, right) {
            replacement.push(upper);
            end += len;
        }
    }
    Some(trimmed_edit(paragraph, left, end, replacement))
}

struct PauseCandidate {
    start: usize,
    end: usize,
    sentence: usize,
    mode: Mode,
    term: String,
}
fn adjacent_pauses(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| horizontal(ch) || matches!(ch, ',' | '—' | '–' | '-' | '…' | '.'))
}
fn pauses<'a>(
    paragraph: &Paragraph,
    sentence_offset: usize,
    document: &Document,
    options: &DraftingOptions,
    speech: &[std::ops::Range<usize>],
    out: &mut Vec<DraftingMatch>,
    leftovers: &mut Vec<Leftover<'a>>,
) {
    static SCAN: OnceLock<Regex> = OnceLock::new();
    let scan = SCAN.get_or_init(|| Regex::new(r"(?i)u+m+|u+h+m+|u+h+|e+r+m+|e+h+m+|er").unwrap());
    let text = &paragraph.text;
    let tokens: Vec<&Token> = paragraph
        .sentences
        .iter()
        .flat_map(|sentence| &sentence.tokens)
        .collect();
    let mut candidates = Vec::new();
    for found in scan.find_iter(text) {
        let start = found.start();
        let end = found.end();
        let before = prev(text, start).map(|(_, ch)| ch);
        let after = next(text, end);
        let prior_word = previous_word(text, start);
        let after_dash = text[end..]
            .strip_prefix(['-', '–', '—'])
            .is_some_and(|rest| {
                rest.starts_with("huh") || rest.starts_with("hm") || rest.starts_with("Huh")
            });
        let domain = text[end..].strip_prefix('.').is_some_and(|rest| {
            ["com", "edu", "org", "net", "gov"]
                .iter()
                .any(|tld| rest.starts_with(tld))
        });
        if before.is_some_and(char::is_alphanumeric)
            || after.is_some_and(char::is_alphanumeric)
            || (before == Some('-') && !text[..start].ends_with("--"))
            || (after == Some('-') && !text[end..].starts_with("--"))
            || after_dash
            || domain
            || (before == Some('*') && after == Some('*'))
            || (before == Some('`') && after == Some('`'))
            || ["italicized", "word", "term"]
                .iter()
                .any(|word| prior_word.eq_ignore_ascii_case(word))
            || found.as_str().chars().all(char::is_uppercase)
            || quoted_alone(text, start, end)
        {
            continue;
        }
        let at = paragraph.span.byte.start + start;
        let token = tokens.get(tokens.partition_point(|token| token.span.byte.end <= at));
        if token.is_some_and(|token| token.is_skipped && token.span.byte.start <= at) {
            continue;
        }
        let term = pause_term(found.as_str());
        if term == "er" {
            if !options.er {
                continue;
            }
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
        let bare = text.trim();
        if bare.eq_ignore_ascii_case(found.as_str())
            || [".", "!", "?", ","]
                .into_iter()
                .any(|mark| bare.eq_ignore_ascii_case(&format!("{}{mark}", found.as_str())))
        {
            continue;
        }
        let sentence = paragraph
            .sentences
            .partition_point(|item| item.span.byte.start <= at)
            .saturating_sub(1);
        let mode = if speech.iter().any(|range| range.contains(&at)) {
            Mode::Dialogue
        } else {
            document.mode_at(at)
        };
        if mode == Mode::Dialogue && !options.include_dialogue {
            continue;
        }
        candidates.push(PauseCandidate {
            start,
            end,
            sentence: sentence_offset + sentence,
            mode,
            term: term.to_owned(),
        });
    }
    let mut at = 0;
    while at < candidates.len() {
        let first = &candidates[at];
        let mut last = at;
        while last + 1 < candidates.len()
            && candidates[last + 1].sentence == first.sentence
            && candidates[last + 1].mode == first.mode
            && adjacent_pauses(&text[candidates[last].end..candidates[last + 1].start])
        {
            last += 1;
        }
        let end = candidates[last].end;
        leftovers.push(Leftover {
            start: first.start,
            end,
            kind: LeftoverKind::Pause { run: last > at },
            found: out.len(),
        });
        out.push(DraftingMatch {
            span: span(paragraph, first.start, end),
            sentence: first.sentence,
            category: DraftingCategory::FilledPause,
            term: first.term.clone(),
            mode: first.mode,
            edits: Vec::new(),
        });
        at = last + 1;
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
    let mut mark_style = None;
    let mut dictated = Vec::new();
    for paragraph in &document.paragraphs {
        let candidates = if options.spoken_commands {
            command_candidates(paragraph)
        } else {
            Vec::new()
        };
        let speech = dictated_speech(paragraph, &candidates);
        let style = if candidates.is_empty() {
            None
        } else {
            Some(&*mark_style.get_or_insert_with(|| marks(document)))
        };
        let mut found = Vec::new();
        let mut leftovers = Vec::new();
        for candidate in &candidates {
            if candidate.entry.mark != "flag" {
                leftovers.push(Leftover {
                    start: candidate.start,
                    end: candidate.end,
                    kind: LeftoverKind::Command(candidate),
                    found: found.len(),
                });
            }
            found.push(DraftingMatch {
                span: span(paragraph, candidate.start, candidate.end),
                sentence: sentence_offset + candidate.sentence,
                category: DraftingCategory::SpokenCommand,
                term: candidate.entry.phrase.to_owned(),
                mode: document.mode_at(paragraph.span.byte.start + candidate.start),
                edits: Vec::new(),
            });
        }
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
            pauses(
                paragraph,
                sentence_offset,
                document,
                options,
                &speech,
                &mut found,
                &mut leftovers,
            );
        }
        // Pauses and commands separated only by spaces and punctuation form one cluster with one
        // edit, computed from the text on both sides, so no two edits share a mark or a space.
        leftovers.sort_by_key(|leftover| leftover.start);
        let mut at = 0;
        while at < leftovers.len() {
            let mut last = at;
            while last + 1 < leftovers.len()
                && leftovers[last].end <= leftovers[last + 1].start
                && joined(&paragraph.text[leftovers[last].end..leftovers[last + 1].start])
            {
                last += 1;
            }
            if let Some(edits) = cluster_edit(paragraph, &leftovers[at..=last], style) {
                found[leftovers[at].found].edits = edits;
            }
            at = last + 1;
        }
        matches.extend(found);
        dictated.extend(speech);
        sentence_offset += paragraph.sentences.len();
    }
    for found in &mut matches {
        if dictated
            .iter()
            .any(|range| range.contains(&found.span.byte.start))
        {
            found.mode = Mode::Dialogue;
        }
    }
    matches.sort_by_key(|m| (m.span.byte.start, m.span.byte.end));
    // Clusters are separated by words, so edits never overlap; this keeps the invariant if a
    // future change breaks that.
    let mut used = 0;
    for found in &mut matches {
        found.edits.retain(|edit| {
            let keep = edit.span.byte.start >= used;
            if keep {
                used = edit.span.byte.end;
            }
            keep
        });
    }
    let mut terms: BTreeMap<(DraftingCategory, String), DraftingTerm> = BTreeMap::new();
    let mut groups = word_split(document).map(|words| DraftingGroup {
        words,
        ..Default::default()
    });
    for token in document.words() {
        if document.mode_at(token.span.byte.start) == Mode::Narration
            && dictated
                .iter()
                .any(|range| range.contains(&token.span.byte.start))
        {
            groups.narration.words -= 1;
            groups.dialogue.words += 1;
        }
    }
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
