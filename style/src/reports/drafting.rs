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
    /// The host knows the text came from speech-to-text. Edits are offered only then; typed text
    /// is flag only.
    pub dictated: bool,
    /// Spoken commands, which follow `dictated` when unset. `Some(true)` without `dictated` is
    /// `invalidOptions`, so `dictated` is the one way to turn edits on.
    #[serde(alias = "spoken_commands", skip_serializing_if = "Option::is_none")]
    pub spoken_commands: Option<bool>,
    #[serde(alias = "filled_pauses")]
    pub filled_pauses: bool,
    pub er: bool,
    #[serde(alias = "include_dialogue")]
    pub include_dialogue: bool,
}
impl Default for DraftingOptions {
    fn default() -> Self {
        Self {
            placeholders: true,
            dictated: false,
            spoken_commands: None,
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
    /// Options for text the host knows came from dictation: edits and spoken commands on.
    pub fn dictated() -> Self {
        Self {
            dictated: true,
            ..Self::default()
        }
    }
    /// Whether spoken commands are detected: `spokenCommands`, or `dictated` when it is unset.
    pub fn commands(&self) -> bool {
        self.spoken_commands.unwrap_or(self.dictated)
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
/// A verb that reports or quotes words (*She said um*, *I said question mark to myself*).
fn reporting_verb(word: &str) -> bool {
    matches!(
        word,
        "say"
            | "says"
            | "said"
            | "saying"
            | "answer"
            | "answers"
            | "answered"
            | "reply"
            | "replies"
            | "replied"
            | "went"
            | "goes"
            | "type"
            | "types"
            | "typed"
            | "typing"
            | "write"
            | "writes"
            | "wrote"
            | "writing"
            | "mutter"
            | "muttered"
            | "mumbled"
            | "murmured"
            | "whispered"
            | "uttered"
            | "repeated"
            | "added"
    )
}
/// An article or possessive determiner, whose noun a following pause word may be (*his um*,
/// *an um*). Words that also stand alone are left out, because dictated pauses follow them too:
/// *her* as an object (*I told her um the*), demonstratives and quantifiers as pronouns (*Make
/// this um bold*, *I knew that um the*), and *no* as an answer (*No um I*).
fn determiner(word: &str) -> bool {
    matches!(
        word,
        "a" | "an"
            | "the"
            | "my"
            | "your"
            | "his"
            | "its"
            | "our"
            | "their"
            | "whose"
            | "every"
            | "each"
            | "another"
    ) || word.ends_with("'s")
        || word.ends_with("’s")
}
/// The word ending right before `at` with only spaces or tabs between, folded to lower case, or
/// empty when punctuation or nothing comes first.
fn spaced_word_before(text: &str, at: usize) -> String {
    let lead = text[..at].trim_end_matches(horizontal);
    if lead.len() == at {
        return String::new();
    }
    let start = lead
        .trim_end_matches(|ch: char| ch.is_alphabetic() || matches!(ch, '\'' | '’'))
        .len();
    lead[start..].to_lowercase()
}
/// Closed quotes, speech or not, and the report's own speech: an opening double quote that never
/// closes in the paragraph (to the paragraph end), and a paragraph that opens with an em dash
/// before a word (dialogue set with dashes). Byte ranges in the paragraph.
fn quoted(text: &str) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    let mut closed = Vec::new();
    let mut speech = Vec::new();
    let mut double = None;
    let mut single = None;
    for (at, ch) in text.char_indices() {
        let before = prev(text, at).map(|(_, ch)| ch);
        let after = next(text, at + ch.len_utf8());
        match ch {
            '“' | '"' if double.is_none() && (ch == '“' || opening_straight_quote(text, at)) => {
                double = Some(at);
            }
            '”' | '"' => {
                if let Some(start) = double.take() {
                    closed.push(start..at + ch.len_utf8());
                }
            }
            '‘' | '\''
                if single.is_none()
                    && before.is_none_or(|ch| !ch.is_alphanumeric())
                    && after.is_some_and(char::is_alphanumeric) =>
            {
                single = Some(at);
            }
            '’' | '\''
                if before.is_some_and(|ch| ch.is_alphanumeric() || ".,!?…".contains(ch))
                    && after.is_none_or(|ch| !ch.is_alphanumeric()) =>
            {
                if let Some(start) = single.take() {
                    closed.push(start..at + ch.len_utf8());
                }
            }
            _ => {}
        }
    }
    if let Some(start) = double {
        speech.push(start..text.len());
    }
    let body = text.trim_start();
    if let Some(rest) = body.strip_prefix('—').or_else(|| body.strip_prefix("--")) {
        if rest.trim_start().starts_with(char::is_alphabetic) {
            speech.push(0..text.len());
        }
    }
    (closed, speech)
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
    /// Reported with no edit: a mark outside a dictated pair, a typed mark beside it, or *full
    /// stop* without a positive cue.
    flag_only: bool,
    /// The dictated pair member this dialogue-tag mark goes with, as an index into the final
    /// candidates: the opening after *said comma*, or the closing before *he said period*.
    tag_of: Option<usize>,
    /// For a quote, paren or bracket command, where the next command phrase of its kind starts,
    /// found before any guard runs.
    next_same: Option<usize>,
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
    // One style for every inserted quote and apostrophe, counted over all three, so a dictated
    // quote never sits curly beside a straight apostrophe (*“I don't know”*).
    let checks = [
        &report.double_quotes,
        &report.single_quotes,
        &report.apostrophes,
    ];
    let count = |form: usize| -> usize {
        checks
            .iter()
            .map(|check| check.variants[form].counts.all)
            .sum()
    };
    let curly = count(0) >= count(1);
    let (double_curly, single_curly, apostrophe_curly) = (curly, curly, curly);
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
    // A mention of the command word (*the word comma*, *The words open quote appeared*).
    if matches!(
        previous.as_deref(),
        Some("word" | "words" | "term" | "terms" | "phrase" | "phrases")
    ) {
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
        || after_terminal(text, candidate.start)
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
    evidence(candidate, words, paragraph, other)
}

/// After a verb of saying or writing, the command word is usually a mention (*I said question
/// mark to myself*, *She typed new paragraph and hit enter*); only a following clause, a pause
/// before one, or a dictated pair of marks makes it dictation (*she said comma he left*, *she said
/// open quote come in close quote*).
fn mention(candidate: &CommandCandidate, words: &[(&Token, usize)], paired: bool) -> bool {
    if !candidate
        .first_word
        .checked_sub(1)
        .is_some_and(|index| reporting_verb(&word(&words[index].0.text)))
    {
        return false;
    }
    let after_pause = words
        .get(candidate.last_word + 1)
        .is_some_and(|(token, _)| filler(&token.text))
        && words
            .get(candidate.last_word + 2)
            .is_some_and(|(token, _)| {
                has_pos(token, "PRP") || token.text.starts_with(char::is_uppercase)
            });
    !(paired || after_pause || opens_clause(candidate, words))
}

/// The words after the command open a clause: a pronoun, determiner, proper noun or capitalized
/// word, or *and*/*but*/*so* plus a clause.
fn opens_clause(candidate: &CommandCandidate, words: &[(&Token, usize)]) -> bool {
    let after = words.get(candidate.last_word + 1).map(|(token, _)| *token);
    let next_word = after.map(|token| word(&token.text));
    after.is_some_and(|token| {
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
            })
}

/// Evidence beyond the spelling that a mid-sentence command was dictated.
fn evidence(
    candidate: &CommandCandidate,
    words: &[(&Token, usize)],
    paragraph: &Paragraph,
    other: bool,
) -> bool {
    let text = &paragraph.text;
    let after = words.get(candidate.last_word + 1).map(|(token, _)| *token);
    let next_word = after.map(|token| word(&token.text));
    let clause = opens_clause(candidate, words);
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
                    flag_only: false,
                    tag_of: None,
                    next_same: None,
                });
            }
            at = last + 1;
        } else {
            at += 1;
        }
    }
    // Pairs are adjacent phrases of one kind, whether or not the guards keep them, so a rejected
    // phrase between two commands (*open paren a open paren b close paren*) leaves them unpaired.
    for index in 0..candidates.len() {
        let kind = mark_kind(candidates[index].entry.mark);
        candidates[index].next_same = kind.and_then(|kind| {
            candidates[index + 1..]
                .iter()
                .find(|later| mark_kind(later.entry.mark) == Some(kind))
                .map(|later| later.start)
        });
    }
    // A closing command right after an opening one may follow any word (*open paren see chapter
    // two close paren*), so it skips its own guards, but only for an opening that passed them.
    let guarded: Vec<bool> = candidates
        .iter()
        .map(|candidate| command_guard(candidate, &words, paragraph))
        .collect();
    let openings: Vec<(&str, usize)> = candidates
        .iter()
        .zip(&guarded)
        .filter(|(candidate, kept)| **kept && candidate.entry.mark.starts_with("open"))
        .map(|(candidate, _)| (candidate.entry.mark, candidate.start))
        .collect();
    let mut guarded = guarded.into_iter();
    candidates.retain(|candidate| {
        guarded.next().unwrap_or(false)
            || candidate.entry.mark.starts_with("close")
                && openings.iter().any(|(mark, start)| {
                    *start < candidate.start
                        && candidate.entry.mark.strip_prefix("close") == mark.strip_prefix("open")
                })
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
    let paired = pair_members(&candidates);
    // The dictated dialogue tag: *said comma* before a paired opening command, and *he said
    // period* at the paragraph end after a paired closing one (*open quote fine close quote he
    // said period*).
    let tag: Vec<bool> = (0..candidates.len())
        .map(|index| dialogue_tag(paragraph, &candidates, &paired, index).is_some())
        .collect();
    // A mention isn't a command, so it isn't evidence for the others either.
    let mut kept: Vec<(CommandCandidate, bool)> = candidates
        .into_iter()
        .zip(paired.iter().zip(tag))
        .filter(|(candidate, (paired, tag))| *tag || !mention(candidate, &words, **paired))
        .map(|(candidate, (paired, _))| (candidate, *paired))
        .collect();
    let count = kept.len();
    kept.retain(|(candidate, paired)| {
        positive_cue(candidate, &words, paragraph, count > 1, *paired)
    });
    let mut candidates: Vec<CommandCandidate> =
        kept.into_iter().map(|(candidate, _)| candidate).collect();
    // A quote, paren or bracket command outside a pair from `mark_pairs` would insert an
    // unmatched mark, a mark the engine typed beside a command would meet the command's own
    // (*late period, she*), and *full stop* is ordinary English (*I love you full stop.*) without
    // other evidence: all flag only. A dialogue-tag mark goes with its pair.
    let paired = pair_members(&candidates);
    let count = candidates.len();
    let decided: Vec<(bool, Option<usize>)> = candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| {
            let mark = candidate.entry.mark;
            let unpaired =
                (mark.starts_with("open") || mark.starts_with("close")) && !paired[index];
            let tag_of = dialogue_tag(paragraph, &candidates, &paired, index);
            let lone_tag = tag_of.is_none()
                && candidate
                    .first_word
                    .checked_sub(1)
                    .is_some_and(|before| reporting_verb(&word(&words[before].0.text)))
                && mention(candidate, &words, paired[index]);
            let flag_only = unpaired
                || lone_tag
                || typed_neighbor(&paragraph.text, candidate)
                || candidate.entry.phrase == "full stop"
                    && !evidence(candidate, &words, paragraph, count > 1);
            (flag_only, tag_of)
        })
        .collect();
    for (candidate, (flag_only, tag_of)) in candidates.iter_mut().zip(decided) {
        candidate.flag_only = flag_only;
        candidate.tag_of = tag_of;
    }
    candidates
}

/// Whether each candidate belongs to a pair from `mark_pairs`.
fn pair_members(candidates: &[CommandCandidate]) -> Vec<bool> {
    let mut paired = vec![false; candidates.len()];
    for (open, close) in mark_pairs(candidates) {
        paired[open] = true;
        paired[close] = true;
    }
    paired
}

/// For a comma or period dictated as part of a dialogue tag, the pair member it goes with:
/// *said comma* right before a paired opening quote command, or *said period* at the paragraph
/// end after a paired closing one. After a verb of saying these are otherwise mentions.
fn dialogue_tag(
    paragraph: &Paragraph,
    candidates: &[CommandCandidate],
    paired: &[bool],
    index: usize,
) -> Option<usize> {
    let candidate = &candidates[index];
    let quote = |mark: &str, side: &str| {
        mark.strip_prefix(side)
            .is_some_and(|kind| matches!(kind, "Double" | "Single"))
    };
    match candidate.entry.phrase {
        "comma" => candidates
            .get(index + 1)
            .filter(|next| {
                paired[index + 1]
                    && quote(next.entry.mark, "open")
                    && next.first_word == candidate.last_word + 1
            })
            .map(|_| index + 1),
        "period" if paragraph.text[candidate.end..].trim().is_empty() => (0..index)
            .rev()
            .find(|&before| paired[before] && quote(candidates[before].entry.mark, "close")),
        _ => None,
    }
}

/// The dictation engine typed a mark right before or after a mark command, with only whitespace
/// between (*late period, she*, *she left, full stop she*, *said, open quote, hi*), other than the
/// same mark that the command's edit already absorbs (*said comma, he*, *milk, comma and*).
fn typed_neighbor(text: &str, candidate: &CommandCandidate) -> bool {
    let mark = candidate.entry.mark;
    if matches!(mark, "delete" | "flag") {
        return false;
    }
    let sentence_mark = match mark {
        "comma" => Some(','),
        "period" => Some('.'),
        "question" => Some('?'),
        "exclamation" => Some('!'),
        "colon" => Some(':'),
        "semicolon" => Some(';'),
        "dash" => Some('—'),
        "hyphen" => Some('-'),
        _ => None,
    };
    let same: &[char] = match mark {
        "ellipsis" => &['…', '.'],
        "openDouble" => &['“', '"'],
        "closeDouble" => &['”', '"'],
        "openSingle" => &['‘', '\''],
        "closeSingle" | "apostrophe" => &['’', '\''],
        "openParen" => &['('],
        "closeParen" => &[')'],
        "openBracket" => &['['],
        "closeBracket" => &[']'],
        _ => sentence_mark.as_slice(),
    };
    let typed = |ch: &char| !ch.is_alphanumeric() && !ch.is_whitespace();
    // Before the command only a single-character sentence mark is absorbed, and after it only a
    // mark typed with no space between.
    let before = text[..candidate.start]
        .trim_end()
        .chars()
        .next_back()
        .filter(typed)
        .is_some_and(|ch| sentence_mark != Some(ch));
    let after = text[candidate.end..]
        .trim_start()
        .chars()
        .next()
        .filter(typed)
        .is_some_and(|ch| !(same.contains(&ch) && text[candidate.end..].starts_with(ch)));
    before || after
}

/// The kind of a quote, paren or bracket mark (`Double` for `openDouble` and `closeDouble`).
fn mark_kind(mark: &str) -> Option<&str> {
    mark.strip_prefix("open")
        .or_else(|| mark.strip_prefix("close"))
}

/// Each opening command whose next command phrase of its kind, kept or not, is a closing command
/// that is still a candidate, as indexes into `candidates`. Another opening first (*open quote
/// wait open quote stop close quote*) leaves the earlier one unpaired, and so does a closing
/// command right after the opening one, with no word to enclose. A pair is edited together or not
/// at all.
fn mark_pairs(candidates: &[CommandCandidate]) -> Vec<(usize, usize)> {
    let mut pairs = Vec::new();
    for (open, candidate) in candidates.iter().enumerate() {
        if !candidate.entry.mark.starts_with("open") {
            continue;
        }
        if let Some(close) = (open + 1..candidates.len()).find(|&close| {
            candidates[close].entry.mark.starts_with("close")
                && Some(candidates[close].start) == candidate.next_same
                && candidates[close].first_word > candidate.last_word + 1
        }) {
            pairs.push((open, close));
        }
    }
    pairs
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
        let word = text.len() - text[end..].trim_start_matches(horizontal).len();
        if !filler(next_word(text, word)) {
            if let Some((upper, len)) = capital(text, word) {
                replacement.push_str(&text[end..word]);
                replacement.push(upper);
                end = word + len;
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
        while next(text, after_mark).is_some_and(horizontal) {
            after_mark += 1;
        }
        // The typed mark is already in the text, so the tokenizer says whether a sentence starts.
        if let Some((upper, len)) =
            capital(text, after_mark).filter(|_| sentence_start(paragraph, after_mark))
        {
            edits.push(DraftingEdit {
                span: span(paragraph, after_mark, after_mark + len),
                replacement: upper.to_string(),
            });
        }
    }
    edits
}

fn dictated_speech(
    paragraph: &Paragraph,
    candidates: &[CommandCandidate],
) -> Vec<std::ops::Range<usize>> {
    mark_pairs(candidates)
        .into_iter()
        .filter(|(open, _)| candidates[*open].entry.mark == "openDouble")
        .map(|(open, close)| {
            paragraph.span.byte.start + candidates[open].end
                ..paragraph.span.byte.start + candidates[close].start
        })
        .collect()
}

fn horizontal(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}
fn opening_straight_quote(text: &str, quote: usize) -> bool {
    prev(text, quote).is_none_or(|(_, ch)| !ch.is_alphanumeric() && !".,;:!?)]}”’".contains(ch))
}
/// The paragraph start, after terminal punctuation (and any closing quotes) plus a space, or
/// right after an opening quote. Only a guard that keeps commands there as prose; edits use
/// `sentence_start`.
fn after_terminal(text: &str, at: usize) -> bool {
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
/// The index of the tokenizer sentence whose first word starts at `at`.
fn first_word(paragraph: &Paragraph, at: usize) -> Option<usize> {
    paragraph.sentences.iter().position(|sentence| {
        sentence
            .tokens
            .iter()
            .find(|token| token.is_word)
            .is_some_and(|token| rel(paragraph, token.span.byte.start) == at)
    })
}
/// Whether the word at `at` starts a tokenizer sentence that doesn't follow a closing quote. A
/// lowercase dialogue tag may follow a quote (*“Why?” um, she asked*), so nothing there is
/// capitalized or edited.
fn sentence_start(paragraph: &Paragraph, at: usize) -> bool {
    first_word(paragraph, at).is_some_and(|index| {
        index == 0
            || !paragraph.sentences[index - 1]
                .text
                .trim_end()
                .ends_with(['”', '’', '"', '\''])
    })
}
fn next_word(text: &str, at: usize) -> &str {
    text[at..]
        .split(|ch: char| !ch.is_alphabetic())
        .next()
        .unwrap_or("")
}
/// A lowercase-initial word with a capital later in it (*iPhone*, *eBay*), which is never
/// capitalized.
fn camel(text: &str, at: usize) -> bool {
    let word = next_word(text, at);
    word.starts_with(char::is_lowercase) && word.chars().skip(1).any(char::is_uppercase)
}
/// The upper case of the lowercase letter at `at` and its length, when it is exactly one char
/// and the word isn't camel-cased.
fn capital(text: &str, at: usize) -> Option<(char, usize)> {
    if camel(text, at) {
        return None;
    }
    let letter = next(text, at).filter(|ch| ch.is_lowercase())?;
    let mut upper = letter.to_uppercase();
    match (upper.next(), upper.next()) {
        (Some(upper), None) => Some((upper, letter.len_utf8())),
        _ => None,
    }
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

/// Sentence-initial words that are always discourse words before a comma pair: none of them can
/// start the phrase after the pause.
const INTERJECTIONS: &[&str] = &["Oh", "Yes", "Yeah", "Okay", "OK", "Ok"];
/// Discourse words that can also start the next phrase (*No one*, *So much*, *Now is*, *Look
/// at*, *Listen to*, *Right here*, *Well enough*, *Sure enough*). They count only when a subject
/// pronoun or a capitalized word follows the pause, which that phrase can't continue with.
const DISCOURSE: &[&str] = &["Well", "So", "No", "Now", "Right", "Sure", "Look", "Listen"];

/// The edit that deletes the filled pause or `scratch that` at `start..end`, or `None` where the
/// punctuation around it is ambiguous and the leftover is flag only. A `run` of several pauses
/// is deleted only when plain spaces separate them and surround it.
fn deletion(paragraph: &Paragraph, start: usize, end: usize, run: bool) -> Option<DraftingEdit> {
    let text = &paragraph.text;
    if run
        && !text[start..end]
            .chars()
            .all(|ch| ch.is_alphabetic() || horizontal(ch))
    {
        return None;
    }
    let left = text[..start].trim_end_matches(horizontal).len();
    let right = text.len() - text[end..].trim_start_matches(horizontal).len();
    let before = prev(text, left).map(|(_, ch)| ch);
    let after = next(text, right);
    // (a) Spaces on both sides, or a space before and terminal punctuation right after. Between
    // spaces, the word before may end in a period that doesn't end a tokenizer sentence
    // (*3 p.m. um the*).
    let abbreviation = before == Some('.')
        && !text[..left].ends_with("..")
        && prev(text, left - 1).is_some_and(|(_, ch)| ch.is_alphabetic())
        && first_word(paragraph, start).is_none();
    if left < start && (before.is_some_and(char::is_alphanumeric) || abbreviation) {
        let spaced = right > end && after.is_some_and(char::is_alphanumeric);
        let terminal = right == end
            && !abbreviation
            && after.is_some_and(|ch| matches!(ch, '.' | '!' | '?'))
            && !text[end..].starts_with("..");
        if spaced || terminal {
            return Some(trimmed_edit(paragraph, left, end, String::new()));
        }
    }
    if run {
        return None;
    }
    // (b) The first word of a tokenizer sentence, not after U+2028, then one comma, an ellipsis
    // or nothing, then a word to capitalize.
    if sentence_start(paragraph, start) && before != Some('\u{2028}') {
        // After an unquoted ? or ! the tokenizer starts a sentence, but a lowercase pause there
        // continues the author's sentence (*He yelled Fire! um and ran*, *Why? um, she wondered*).
        let index = first_word(paragraph, start).unwrap_or(0);
        if index > 0
            && paragraph.sentences[index - 1]
                .text
                .trim_end()
                .ends_with(['?', '!'])
            && text[start..].starts_with(char::is_lowercase)
        {
            return None;
        }
        let mark = [",", "…", "..."]
            .into_iter()
            .find(|mark| text[end..].starts_with(mark))
            .map_or(0, str::len);
        let word = text.len() - text[end + mark..].trim_start_matches(horizontal).len();
        if (mark > 0 || word > end)
            && next(text, word).is_some_and(char::is_alphabetic)
            && !camel(text, word)
        {
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
    // (c) A comma pair, `Well, um, I`, after a sentence-initial discourse word keeps its comma.
    // Any other comma pair is flag only, because the first comma may be the author's.
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
    let lead = &text[first..comma];
    let following = next_word(text, word);
    let subject = following.starts_with(char::is_uppercase)
        || matches!(
            following.to_ascii_lowercase().as_str(),
            "i" | "you" | "he" | "she" | "we" | "they" | "it"
        );
    ((INTERJECTIONS.contains(&lead) || DISCOURSE.contains(&lead) && subject)
        && sentence_start(paragraph, first))
    .then(|| trimmed_edit(paragraph, start, word, String::new()))
}

/// A filled-pause run or an accepted command, with the index of its match.
struct Leftover<'a> {
    start: usize,
    end: usize,
    kind: LeftoverKind<'a>,
    found: usize,
    /// Reported with no edit, which makes its whole cluster flag only.
    flag_only: bool,
}
#[derive(Clone, Copy)]
enum LeftoverKind<'a> {
    /// One filled pause, or a `run` of several in one match.
    Pause {
        run: bool,
    },
    Command(&'a CommandCandidate),
}
/// A U+2028 gap joins a cluster too, and its members' separation isn't plain spaces, so the whole
/// cluster is flag only (*left period* U+2028 *period the*).
fn joined(gap: &str) -> bool {
    !gap.chars().any(char::is_alphanumeric)
}

/// The edits for one cluster of leftovers, or `None` when the cluster is flag only. The caller
/// attaches them to the cluster's first command match, or to its pause when it has no command.
fn cluster_edit(
    paragraph: &Paragraph,
    members: &[Leftover],
    marks: Option<&Marks>,
) -> Option<Vec<DraftingEdit>> {
    if members.iter().any(|member| member.flag_only) {
        return None;
    }
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
/// marks would meet, even across closing quotes or brackets (*left period um comma then*,
/// *fine period close quote comma he*).
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
            .iter()
            .filter(|key| !key.starts_with("close"))
            .collect::<Vec<_>>()
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
    verbatim: bool,
}
/// Where a paragraph's pauses count as speech or quoted words. `dictated` holds document byte
/// ranges inside dictated quote pairs; `quotes` and `own` are paragraph byte ranges from
/// `quoted`.
struct Speech {
    dictated: Vec<std::ops::Range<usize>>,
    quotes: Vec<std::ops::Range<usize>>,
    own: Vec<std::ops::Range<usize>>,
}
fn adjacent_pauses(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|ch| horizontal(ch) || matches!(ch, ',' | '—' | '–' | '-' | '…' | '.'))
}
/// A pause word as the subject of a sentence that names it (*and um was her favorite word*,
/// *Um, she thought, was not a word.*): at a sentence start or after *and*, *but* or *or*, then
/// *is* or *was*, optionally after one comma-enclosed aside, then an article, a possessive, *one*,
/// or *not* or *never* before one. *I um was going* has a subject before the pause, *and um was
/// going* no article after the verb, and a question (*Um, he said, was the door open?*) is
/// inverted, so all three stay pauses.
fn named_subject(paragraph: &Paragraph, start: usize, end: usize, before_word: &str) -> bool {
    let text = &paragraph.text;
    if !(matches!(before_word, "and" | "but" | "or") || first_word(paragraph, start).is_some()) {
        return false;
    }
    let mut rest = &text[end..];
    if let Some(aside) = rest.strip_prefix(',') {
        let Some((inside, after)) = aside.split_once(',') else {
            return false;
        };
        let count = inside.split_whitespace().count();
        if !(1..=3).contains(&count)
            || !inside
                .chars()
                .all(|ch| ch.is_alphabetic() || horizontal(ch))
        {
            return false;
        }
        rest = after;
    }
    let mut words = rest
        .split(|ch: char| !(ch.is_alphabetic() || ch == '\'' || ch == '’'))
        .filter(|word| !word.is_empty())
        .map(str::to_lowercase);
    let sentence = paragraph
        .sentences
        .iter()
        .rfind(|sentence| rel(paragraph, sentence.span.byte.start) <= start);
    if !rest.starts_with(horizontal)
        || sentence.is_some_and(|sentence| sentence.text.trim_end().ends_with('?'))
        || !matches!(words.next().as_deref(), Some("is" | "was"))
    {
        return false;
    }
    let mut next = words.next();
    if matches!(next.as_deref(), Some("not" | "never")) {
        next = words.next();
    }
    next.is_some_and(|word| word == "one" || word == "her" || determiner(&word))
}

fn pauses<'a>(
    paragraph: &Paragraph,
    sentence_offset: usize,
    document: &Document,
    options: &DraftingOptions,
    speech: &Speech,
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
    let mut reported: Option<usize> = None;
    let mut candidates = Vec::new();
    for found in scan.find_iter(text) {
        let start = found.start();
        let end = found.end();
        let before = prev(text, start).map(|(_, ch)| ch);
        let after = next(text, end);
        let prior_word = previous_word(text, start);
        // Two-word interjections written with a space, comma or dash (*uh oh*, *Uh, oh*, *uh huh*,
        // *um hm*, *uh—huh*) are words, not pauses.
        let follow = text[end..]
            .strip_prefix([',', '-', '–', '—'])
            .unwrap_or(&text[end..])
            .trim_start_matches(horizontal);
        let partner = next_word(follow, 0).to_ascii_lowercase();
        let lower = found.as_str().to_ascii_lowercase();
        let uh = lower
            .trim_start_matches('u')
            .trim_start_matches('h')
            .is_empty();
        let um = lower
            .trim_start_matches('u')
            .trim_start_matches('m')
            .is_empty();
        let interjection = follow.len() < text.len() - end
            && (text[end..].starts_with(['-', '–', '—'])
                && matches!(partner.as_str(), "huh" | "hm")
                || uh && matches!(partner.as_str(), "oh" | "huh")
                || um && matches!(partner.as_str(), "hm" | "hmm"));
        let domain = text[end..].strip_prefix('.').is_some_and(|rest| {
            ["com", "edu", "org", "net", "gov"]
                .iter()
                .any(|tld| rest.starts_with(tld))
        });
        if before.is_some_and(char::is_alphanumeric)
            || after.is_some_and(char::is_alphanumeric)
            || (before == Some('-') && !text[..start].ends_with("--"))
            || (after == Some('-') && !text[end..].starts_with("--"))
            || interjection
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
        // A pause word reported or used as a noun is a word (*She said um and looked away*, *with
        // an um and a shrug*, *his um*), and so is every pause right after it, with only spaces
        // and pause punctuation between (*The baby went uh uh uh*, *He said um--uh--no*). After a determiner other than *an*, the pause is still a pause when a
        // noun or adjective follows it, because the determiner then has its own head
        // (*to the um store*).
        let before_word = spaced_word_before(text, start);
        let head = tokens
            .iter()
            .find(|token| token.is_word && token.span.byte.start >= paragraph.span.byte.start + end)
            .is_some_and(|token| {
                has_pos(token, "NN") || token.readings.iter().any(|reading| reading.pos == "JJ")
            });
        // A determiner and an adjective before the pause make it their noun too (*a long uhhhh*).
        let adjective_word = tokens
            .iter()
            .rev()
            .find(|token| token.is_word && token.span.byte.end <= paragraph.span.byte.start + start)
            .is_some_and(|token| {
                token.readings.iter().any(|reading| reading.pos == "JJ")
                    && !before_word.is_empty()
                    && determiner(&spaced_word_before(
                        text,
                        rel(paragraph, token.span.byte.start),
                    ))
            });
        if reported.is_some_and(|stop| stop < start && adjacent_pauses(&text[stop..start]))
            || reporting_verb(&before_word)
            || before_word == "an"
            || (determiner(&before_word) || adjective_word) && !head
            || named_subject(paragraph, start, end, &before_word)
        {
            reported = Some(end);
            continue;
        }
        // A capitalized pause word right before another capitalized word other than *I* is a
        // name, whatever its spelling (*Um Kulthum*, *Umm Qasr*), and so is one mid-sentence
        // unless it follows another pause (*Um, Umm, the door* is two pauses).
        let rest = text[end..].trim_start_matches(horizontal);
        let named = rest.len() < text.len() - end
            && rest.starts_with(char::is_uppercase)
            && next_word(rest, 0) != "I";
        let lead = text[..start].trim_end_matches(|ch: char| {
            horizontal(ch) || matches!(ch, ',' | '—' | '–' | '-' | '…' | '.')
        });
        let after_pause =
            lead.len() < start && filler(&lead[lead.trim_end_matches(char::is_alphabetic).len()..]);
        if found.as_str().starts_with(char::is_uppercase)
            && (named || first_word(paragraph, start).is_none() && !after_pause)
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
        let paired = speech.dictated.iter().any(|range| range.contains(&at));
        let own = speech.own.iter().any(|range| range.contains(&start));
        let quoted = speech.quotes.iter().any(|range| range.contains(&start));
        let mode = if paired || own {
            Mode::Dialogue
        } else {
            document.mode_at(at)
        };
        if (mode == Mode::Dialogue || quoted) && !options.include_dialogue {
            continue;
        }
        // Quoted text other than detected speech is the author's verbatim wording (*They called
        // it “the um thing”*), and the report's own speech rule is a heuristic: flag only.
        let verbatim = (quoted || own) && !paired && document.mode_at(at) == Mode::Narration;
        candidates.push(PauseCandidate {
            start,
            end,
            sentence: sentence_offset + sentence,
            mode,
            term: term.to_owned(),
            verbatim,
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
            flag_only: candidates[at..=last].iter().any(|item| item.verbatim),
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
    if options.spoken_commands == Some(true) && !options.dictated {
        return Err(StyleError::options(
            "spokenCommands needs dictated: true; edits are offered only for dictated text",
        ));
    }
    let mut matches = Vec::new();
    let mut sentence_offset = 0;
    let mut mark_style = None;
    let mut dictated = Vec::new();
    for paragraph in &document.paragraphs {
        let candidates = if options.commands() {
            command_candidates(paragraph)
        } else {
            Vec::new()
        };
        let (quotes, own) = quoted(&paragraph.text);
        let speech = Speech {
            dictated: dictated_speech(paragraph, &candidates),
            quotes,
            own,
        };
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
                    flag_only: candidate.flag_only,
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
        // Pauses and commands separated only by spaces, punctuation and U+2028 form one cluster
        // with one edit, computed from the text on both sides, so no two edits share a mark or a
        // space. Typed text gets no edits at all.
        if options.dictated {
            leftovers.sort_by_key(|leftover| leftover.start);
            let mut clusters = Vec::new();
            let mut at = 0;
            while at < leftovers.len() {
                let mut last = at;
                while last + 1 < leftovers.len()
                    && leftovers[last].end <= leftovers[last + 1].start
                    && joined(&paragraph.text[leftovers[last].end..leftovers[last + 1].start])
                {
                    last += 1;
                }
                clusters.push((
                    at..=last,
                    cluster_edit(paragraph, &leftovers[at..=last], style),
                ));
                at = last + 1;
            }
            // A dictated pair of marks is edited together or not at all: if either side's cluster
            // is flag only, so is the other's, until nothing changes.
            let cluster_of = |candidate: &CommandCandidate| {
                clusters.iter().position(|(members, _)| {
                    leftovers[members.clone()].iter().any(|leftover| {
                        matches!(leftover.kind, LeftoverKind::Command(item) if std::ptr::eq(item, candidate))
                    })
                })
            };
            // A dialogue-tag mark goes with its pair member the same way.
            let tags = candidates
                .iter()
                .enumerate()
                .filter_map(|(index, candidate)| candidate.tag_of.map(|other| (index, other)));
            let pairs: Vec<(Option<usize>, Option<usize>)> = mark_pairs(&candidates)
                .into_iter()
                .chain(tags)
                .map(|(open, close)| {
                    (
                        cluster_of(&candidates[open]),
                        cluster_of(&candidates[close]),
                    )
                })
                .collect();
            let mut changed = true;
            while changed {
                changed = false;
                for (open, close) in &pairs {
                    let (Some(open), Some(close)) = (*open, *close) else {
                        continue;
                    };
                    if clusters[open].1.is_some() != clusters[close].1.is_some() {
                        clusters[open].1 = None;
                        clusters[close].1 = None;
                        changed = true;
                    }
                }
            }
            for (members, edits) in clusters {
                if let Some(edits) = edits {
                    let members = &leftovers[members];
                    let carrier = members
                        .iter()
                        .find(|leftover| matches!(leftover.kind, LeftoverKind::Command(_)))
                        .unwrap_or(&members[0]);
                    found[carrier.found].edits = edits;
                }
            }
        }
        matches.extend(found);
        dictated.extend(speech.dictated);
        dictated.extend(speech.own.iter().map(|range| {
            paragraph.span.byte.start + range.start..paragraph.span.byte.start + range.end
        }));
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
