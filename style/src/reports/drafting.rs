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
    /// Reported with no edit: a mark outside a dictated pair, a mention after a verb of saying, or
    /// *full stop* without a positive cue. A typed mark the edit can't absorb makes it flag only
    /// later, in `command_edit`.
    flag_only: bool,
    /// The other member of this command's pair from `mark_pairs`, as an index into the
    /// candidates.
    pair: Option<usize>,
    /// The command this dialogue-tag mark goes with, as an index into the candidates: the opening
    /// after *said comma*, the closing before *he said period* at the paragraph end, or the break
    /// or opening right after *he said period*.
    tag_of: Option<usize>,
    /// For a quote, paren or bracket command, the index of the next command phrase of its kind,
    /// found before any guard runs, or `None` once that phrase is dropped.
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
    // Each kind follows the document's own marks of that kind, and the apostrophes when it has
    // none, so a dictated quote matches the typed dialogue beside it and never sits curly beside
    // a straight apostrophe in a document without quotes (*"I don't know"*). An apostrophe
    // follows the document's apostrophes, then its single quotes.
    let curly = |check: &consistency::ConsistencyCheck| {
        let [curly, straight] = [0, 1].map(|form| check.variants[form].counts.all);
        (curly + straight > 0).then_some(curly >= straight)
    };
    let apostrophes = curly(&report.apostrophes);
    let fallback = apostrophes.unwrap_or(true);
    let double = curly(&report.double_quotes).unwrap_or(fallback);
    let single = curly(&report.single_quotes).unwrap_or(fallback);
    let apostrophe = apostrophes.unwrap_or(single);
    let character = report.ellipses.variants[0].counts.all
        >= report.ellipses.variants[1].counts.all + report.ellipses.variants[2].counts.all;
    Marks {
        double_open: if double { "“" } else { "\"" },
        double_close: if double { "”" } else { "\"" },
        single_open: if single { "‘" } else { "'" },
        single_close: if single { "’" } else { "'" },
        apostrophe: if apostrophe { "’" } else { "'" },
        ellipsis: if character { "…" } else { "..." },
    }
}

impl Marks {
    fn replacement(&self, mark: &str) -> Option<&'static str> {
        match mark {
            "ellipsis" => Some(self.ellipsis),
            "openDouble" => Some(self.double_open),
            "closeDouble" => Some(self.double_close),
            "openSingle" => Some(self.single_open),
            "closeSingle" => Some(self.single_close),
            "apostrophe" => Some(self.apostrophe),
            _ => fixed_mark(mark),
        }
    }
}
/// The mark a command inserts whatever the document's style.
fn fixed_mark(mark: &str) -> Option<&'static str> {
    match mark {
        "comma" => Some(","),
        "period" => Some("."),
        "question" => Some("?"),
        "exclamation" => Some("!"),
        "colon" => Some(":"),
        "semicolon" => Some(";"),
        "dash" => Some("—"),
        "hyphen" => Some("-"),
        "openParen" => Some("("),
        "closeParen" => Some(")"),
        "openBracket" => Some("["),
        "closeBracket" => Some("]"),
        "newline" => Some("\n"),
        _ => None,
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
    // A multi-word command with a later word capitalized is a name or title (*New Line Cinema*).
    if words[candidate.first_word + 1..=candidate.last_word]
        .iter()
        .any(|(token, _)| token.text.starts_with(char::is_uppercase))
    {
        return false;
    }
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
        // A single-word command after a different typed mark is prose (*No more, period.*). The
        // same mark typed before it separates it from the word before, so the noun guard doesn't
        // apply; whether the edit can absorb that mark is `absorbed_before`'s decision.
        let typed_before = mark_before(&paragraph.text, candidate);
        let prior_typed_mark =
            typed_before.is_some_and(|(_, ch)| sentence_mark(entry.mark) == Some(ch));
        if entry.words.len() == 1 && typed_before.is_some() && !prior_typed_mark {
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
    // (*open quote … close quote*) and a line break before a capital, which may also follow a
    // typed ellipsis or closing quote (*He left… New paragraph The*).
    if text[begin..candidate.start]
        .split(|ch: char| !ch.is_alphanumeric())
        .all(|lead| lead.is_empty() || filler(lead))
        || after_terminal(text, candidate.start)
        || candidate.entry.mark == "newline" && break_after(text, candidate.start)
    {
        // A name after the break may continue the command words as prose (*She read the first
        // line aloud. Next line Tom read.*).
        let name = |token: &Token| {
            !token.readings.is_empty()
                && token
                    .readings
                    .iter()
                    .all(|reading| reading.pos.starts_with("NNP"))
        };
        return match candidate.entry.mark {
            "newline" => words.get(candidate.last_word + 1).is_none_or(|(token, _)| {
                token.text.starts_with(char::is_uppercase) && !name(token)
            }),
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
        && absorbed_before(text, candidate).is_some_and(|(_, typed)| typed)
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
                    pair: None,
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
            (index + 1..candidates.len())
                .find(|&later| mark_kind(candidates[later].entry.mark) == Some(kind))
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
    let keep: Vec<bool> = candidates
        .iter()
        .zip(&guarded)
        .map(|(candidate, kept)| {
            *kept
                || candidate.entry.mark.starts_with("close")
                    && openings.iter().any(|(mark, start)| {
                        *start < candidate.start
                            && candidate.entry.mark.strip_prefix("close")
                                == mark.strip_prefix("open")
                    })
        })
        .collect();
    let candidates = compact(candidates, &keep);
    let keep: Vec<bool> = candidates
        .iter()
        .map(|candidate| {
            candidate.entry.guard != "unquote"
                || candidates
                    .iter()
                    .any(|open| open.entry.mark == "openDouble" && open.start < candidate.start)
        })
        .collect();
    let mut candidates = compact(candidates, &keep);
    mark_pairs(&mut candidates, &words);
    // The dictated dialogue tag: *said comma* before a paired opening command, and *he said
    // period* after a closing quote (*open quote fine close quote he said period*).
    for index in 0..candidates.len() {
        candidates[index].tag_of = dialogue_tag(paragraph, &candidates, index);
    }
    // A mention isn't a command, so it isn't evidence for the others either.
    let keep: Vec<bool> = candidates
        .iter()
        .map(|candidate| {
            candidate.tag_of.is_some() || !mention(candidate, &words, candidate.pair.is_some())
        })
        .collect();
    let mut candidates = compact(candidates, &keep);
    let count = candidates.len();
    let keep: Vec<bool> = candidates
        .iter_mut()
        .map(|candidate| {
            let cue = positive_cue(
                candidate,
                &words,
                paragraph,
                count > 1,
                candidate.pair.is_some(),
            );
            // A lowercase *ellipsis* mid-sentence with nothing after it but typed marks (*She
            // trailed off ellipsis.*): the engine typed the end of the sentence the spoken mark
            // ends. Without a positive cue it is a match but flag only; what an edit could absorb
            // is `command_edit`'s question alone.
            let text = &paragraph.text;
            let rest = text[candidate.end..].trim();
            candidate.flag_only = !cue
                && candidate.entry.phrase == "ellipsis"
                && text[candidate.start..].starts_with(char::is_lowercase)
                && !after_terminal(text, candidate.start)
                && !rest.is_empty()
                && rest.chars().all(typed_mark);
            cue || candidate.flag_only
        })
        .collect();
    let mut candidates = compact(candidates, &keep);
    // A quote, paren or bracket command outside a pair from `mark_pairs` would insert an
    // unmatched mark, and *full stop* is ordinary English (*I love you full stop.*) without other
    // evidence: both flag only. A dialogue-tag mark goes with its pair. Where the edit can't
    // absorb a typed mark beside a command, `command_edit` makes it flag only.
    let count = candidates.len();
    let flag_only: Vec<bool> = candidates
        .iter()
        .map(|candidate| {
            let mark = candidate.entry.mark;
            let unpaired =
                (mark.starts_with("open") || mark.starts_with("close")) && candidate.pair.is_none();
            let lone_tag = candidate.tag_of.is_none()
                && candidate
                    .first_word
                    .checked_sub(1)
                    .is_some_and(|before| reporting_verb(&word(&words[before].0.text)))
                && mention(candidate, &words, candidate.pair.is_some());
            candidate.flag_only
                || unpaired
                || lone_tag
                || candidate.entry.phrase == "full stop"
                    && !evidence(candidate, &words, paragraph, count > 1)
        })
        .collect();
    for (candidate, flag_only) in candidates.iter_mut().zip(flag_only) {
        candidate.flag_only = flag_only;
    }
    candidates
}

/// The candidates that `keep` marks, with the indexes they hold into each other renumbered. A
/// link to a dropped candidate becomes `None`: its pair is broken, and a dropped next phrase of
/// the same kind still leaves its opening unpaired.
fn compact(candidates: Vec<CommandCandidate>, keep: &[bool]) -> Vec<CommandCandidate> {
    let mut kept = 0;
    let index: Vec<Option<usize>> = keep
        .iter()
        .map(|keep| {
            keep.then(|| {
                kept += 1;
                kept - 1
            })
        })
        .collect();
    let renumber = |link: Option<usize>| link.and_then(|old| index[old]);
    candidates
        .into_iter()
        .zip(keep)
        .filter(|(_, keep)| **keep)
        .map(|(mut candidate, _)| {
            candidate.pair = renumber(candidate.pair);
            candidate.tag_of = renumber(candidate.tag_of);
            candidate.next_same = renumber(candidate.next_same);
            candidate
        })
        .collect()
}

/// For a comma or period dictated as part of a dialogue tag, the command it goes with: *said
/// comma* right before a paired opening quote command; *said period* at the paragraph end after a
/// paired closing one; and *said period* right before a paragraph-break command or a paired
/// opening quote command, after a closing quote command or a typed closing quote (*“hello” she
/// said period open quote*). After a verb of saying these are otherwise mentions.
fn dialogue_tag(
    paragraph: &Paragraph,
    candidates: &[CommandCandidate],
    index: usize,
) -> Option<usize> {
    let candidate = &candidates[index];
    let quote = |mark: &str, side: &str| {
        mark.strip_prefix(side)
            .is_some_and(|kind| matches!(kind, "Double" | "Single"))
    };
    let next = candidates
        .get(index + 1)
        .filter(|next| next.first_word == candidate.last_word + 1);
    let opening = |next: &&CommandCandidate| next.pair.is_some() && quote(next.entry.mark, "open");
    match candidate.entry.phrase {
        "comma" => next.filter(opening).map(|_| index + 1),
        "period" => {
            let closed = (0..index).rev().find(|&before| {
                candidates[before].pair.is_some() && quote(candidates[before].entry.mark, "close")
            });
            if paragraph.text[candidate.end..].trim().is_empty() {
                return closed;
            }
            next.filter(|next| next.entry.mark == "newline" || opening(next))
                .filter(|_| {
                    closed.is_some() || typed_quote_before(&paragraph.text, candidate.start)
                })
                .map(|_| index + 1)
        }
        _ => None,
    }
}

/// A typed closing double quote, then at most one word and a verb, before `at` (*“hello” she
/// said*).
fn typed_quote_before(text: &str, at: usize) -> bool {
    let word = |lead: &str| -> usize {
        lead.trim_end_matches(char::is_alphabetic)
            .trim_end_matches(horizontal)
            .len()
    };
    let verb = word(text[..at].trim_end_matches(horizontal));
    [verb, word(&text[..verb])].into_iter().any(|end| {
        prev(text, end).is_some_and(|(quote, ch)| {
            ch == '”' || ch == '"' && !opening_straight_quote(text, quote)
        })
    })
}

/// The kind of a quote, paren or bracket mark (`Double` for `openDouble` and `closeDouble`).
fn mark_kind(mark: &str) -> Option<&str> {
    mark.strip_prefix("open")
        .or_else(|| mark.strip_prefix("close"))
}

/// Pairs each opening command with its next command phrase of its kind, kept or not, when that
/// is a closing command that is still a candidate, and stores each side's index in the other's
/// `pair`. Another opening first (*open quote wait open quote stop close quote*) leaves the
/// earlier one unpaired, and so does a pair with nothing but filled pauses and other commands
/// between, which would leave empty or punctuation-only marks (*open paren uh close paren*). A
/// pair is edited together or not at all.
fn mark_pairs(candidates: &mut [CommandCandidate], words: &[(&Token, usize)]) {
    for open in 0..candidates.len() {
        let candidate = &candidates[open];
        let Some(close) = candidate.next_same.filter(|&close| {
            candidate.entry.mark.starts_with("open")
                && candidates[close].entry.mark.starts_with("close")
        }) else {
            continue;
        };
        let inner = &candidates[open + 1..close];
        let content = (candidate.last_word + 1..candidates[close].first_word).any(|at| {
            !filler(&words[at].0.text)
                && !inner
                    .iter()
                    .any(|command| (command.first_word..=command.last_word).contains(&at))
        });
        if content {
            candidates[open].pair = Some(close);
            candidates[close].pair = Some(open);
        }
    }
}

/// A mark the author or the engine typed: anything but a letter, digit or whitespace.
fn typed_mark(ch: char) -> bool {
    !ch.is_alphanumeric() && !ch.is_whitespace()
}
/// The single sentence mark a command inserts, which the same mark typed before it can stand in
/// for.
fn sentence_mark(mark: &str) -> Option<char> {
    match mark {
        "comma" | "period" | "question" | "exclamation" | "colon" | "semicolon" | "dash"
        | "hyphen" => fixed_mark(mark)?.chars().next(),
        _ => None,
    }
}
/// Typed terminal punctuation or a closing double quote before `at`, across spaces, which a
/// dictated line break may follow (*He left. New paragraph The*).
fn break_after(text: &str, at: usize) -> bool {
    let lead = text[..at].trim_end_matches(horizontal).len();
    prev(text, lead).is_some_and(|(quote, ch)| {
        matches!(ch, '.' | '?' | '!' | '…' | '”')
            || ch == '"' && !opening_straight_quote(text, quote)
    })
}

/// The offset of the command's own sentence mark typed right before it across spaces or tabs
/// (*said, comma he*), which separates the command from the word before.
fn same_mark_before(text: &str, candidate: &CommandCandidate) -> Option<usize> {
    mark_before(text, candidate)
        .filter(|(_, ch)| sentence_mark(candidate.entry.mark) == Some(*ch))
        .map(|(at, _)| at)
}
/// The offset and char of the typed mark right before the command across spaces or tabs.
fn mark_before(text: &str, candidate: &CommandCandidate) -> Option<(usize, char)> {
    let start = text[..candidate.start].trim_end_matches(horizontal).len();
    prev(text, start).filter(|(_, ch)| typed_mark(*ch))
}

/// Where a command's edit starts, and whether the same sentence mark typed right before it stands
/// in for the command's own (*said, comma he*), or `None` where the edit can't absorb what is
/// typed before it. An opening mark keeps the whitespace before it, so any typed mark across that
/// whitespace would meet it (*said, open quote*). Any other mark consumes the spaces and tabs
/// before it, so other whitespace stays (*paused,* U+2028 *comma*), and so does any other typed
/// mark, except terminal punctuation or a closing quote before a line break. A spaced mark isn't
/// absorbed either, because the edit would close it on one side only (*day — dash the*, *well -
/// hyphen known*).
fn absorbed_before(text: &str, candidate: &CommandCandidate) -> Option<(usize, bool)> {
    let mark = candidate.entry.mark;
    if mark.starts_with("open") {
        let typed = text[..candidate.start]
            .trim_end()
            .chars()
            .next_back()
            .is_some_and(typed_mark);
        return (!typed).then_some((candidate.start, false));
    }
    let start = text[..candidate.start].trim_end_matches(horizontal).len();
    let Some((_, before)) = prev(text, start) else {
        return Some((start, false));
    };
    if before.is_alphanumeric() || mark == "newline" && break_after(text, candidate.start) {
        return Some((start, false));
    }
    let at = same_mark_before(text, candidate)?;
    prev(text, at)
        .is_none_or(|(_, ch)| !ch.is_whitespace())
        .then_some((start, true))
}

/// Whether the command's own mark is typed right after it with no space between, which its edit
/// absorbs (*late period. she*, *tired comma, and*), or `None` where another typed mark follows
/// it across any whitespace, which the edit can't absorb.
fn absorbed_after(text: &str, candidate: &CommandCandidate) -> Option<bool> {
    let rest = &text[candidate.end..];
    let already = match candidate.entry.mark {
        "ellipsis" => rest.starts_with('…') || rest.starts_with("..."),
        "openDouble" | "closeDouble" => rest.starts_with(['“', '”', '"']),
        "openSingle" | "closeSingle" | "apostrophe" => rest.starts_with(['‘', '’', '\'']),
        mark => fixed_mark(mark).is_some_and(|mark| rest.starts_with(mark)),
    };
    (already || !rest.trim_start().starts_with(typed_mark)).then_some(already)
}

/// The edit for a command alone, or `None` where it can't absorb a mark typed beside it, which
/// makes the command flag only, and its pair and dialogue tag with it. Before the command,
/// `absorbed_before` decides; after it, `absorbed_after`. A line break with nothing after it in
/// the paragraph would sit beside the paragraph separator, and an apostrophe beside one the
/// engine typed would double it (*the dog's apostrophe s*, *the dog apostrophe s'*): both are
/// flag only too.
fn command_edit(
    paragraph: &Paragraph,
    candidate: &CommandCandidate,
    marks: &Marks,
) -> Option<Vec<DraftingEdit>> {
    let mark = marks
        .replacement(candidate.entry.mark)
        .expect("command mark");
    let text = &paragraph.text;
    let (start, paired_mark) = absorbed_before(text, candidate)?;
    let is_open_mark = candidate.entry.mark.starts_with("open");
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
    let already = absorbed_after(text, candidate)?;
    if candidate.entry.mark == "newline"
        && text[end..].chars().next().is_none_or(char::is_whitespace)
    {
        return None;
    }
    let apostrophe = |piece: &str| piece.ends_with(['\'', '’']);
    if candidate.entry.mark == "apostrophe" {
        let lead = &text[..start];
        if apostrophe(lead)
            || lead.strip_suffix('s').is_some_and(apostrophe)
            || text[end..]
                .strip_prefix('s')
                .is_some_and(|rest| rest.starts_with(['\'', '’']))
        {
            return None;
        }
    }
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
    Some(edits)
}

fn dictated_speech(
    paragraph: &Paragraph,
    candidates: &[CommandCandidate],
) -> Vec<std::ops::Range<usize>> {
    candidates
        .iter()
        .filter(|open| open.entry.mark == "openDouble")
        .filter_map(|open| {
            open.pair.map(|close| {
                paragraph.span.byte.start + open.end
                    ..paragraph.span.byte.start + candidates[close].start
            })
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
                command_edit(paragraph, candidate, marks?)
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
    let separating_mark = |key: &str| {
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
        || keys.contains(&"newline") && after.is_none()
        || keys
            .iter()
            .filter(|key| !key.starts_with("close"))
            .collect::<Vec<_>>()
            .windows(2)
            .any(|pair| separating_mark(pair[0]) && separating_mark(pair[1]))
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
    flag_only: bool,
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
        // A determiner and an adjective before the pause make it their noun too (*a long uhhhh
        // and sat down*), and v1 treats a restart the same way (*the red um the blue one*). With a
        // noun or adjective after it, the pause may still be a noun (*a long um. Long enough.*),
        // so it is flag only.
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
            flag_only: verbatim || adjective_word,
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
            flag_only: candidates[at..=last].iter().any(|item| item.flag_only),
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
            // A dialogue-tag mark goes with its command the same way.
            let pairs: Vec<(Option<usize>, Option<usize>)> = candidates
                .iter()
                .enumerate()
                .flat_map(|(index, candidate)| {
                    [
                        candidate.pair.filter(|close| index < *close),
                        candidate.tag_of,
                    ]
                    .map(|other| other.map(|other| (index, other)))
                })
                .flatten()
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
