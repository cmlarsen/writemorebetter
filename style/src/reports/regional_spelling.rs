use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::{part_span, parts, Split};
use crate::{Document, Mode, Span, StyleError, Token};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpellingSystem {
    American,
    British,
    Oxford,
}
impl SpellingSystem {
    pub const ALL: [Self; 3] = [Self::American, Self::British, Self::Oxford];
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegionalSpellingReason {
    System,
    Form,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct RegionalSpellingOptions {
    #[serde(alias = "minimum_markers")]
    pub minimum_markers: usize,
    pub system: Option<SpellingSystem>,
}
impl Default for RegionalSpellingOptions {
    fn default() -> Self {
        Self {
            minimum_markers: 3,
            system: None,
        }
    }
}
impl RegionalSpellingOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_minimum_markers(mut self, value: usize) -> Self {
        self.minimum_markers = value;
        self
    }
    pub fn with_system(mut self, value: Option<SpellingSystem>) -> Self {
        self.system = value;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct SpellingSystemCount {
    pub system: SpellingSystem,
    pub count: usize,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RegionalSpellingForm {
    pub form: String,
    pub systems: Vec<SpellingSystem>,
    pub counts: Split<usize>,
    pub spans: Vec<Span>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RegionalSpellingWord {
    pub word: String,
    pub mixed: bool,
    pub forms: Vec<RegionalSpellingForm>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RegionalSpellingFlag {
    pub span: Span,
    pub sentence: usize,
    pub form: String,
    pub word: String,
    pub systems: Vec<SpellingSystem>,
    pub reason: RegionalSpellingReason,
    pub dominant_form: Option<String>,
    pub mode: Mode,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct RegionalSpellingReport {
    pub markers: usize,
    pub systems: Vec<SpellingSystemCount>,
    pub dominant: Vec<SpellingSystem>,
    pub words: Vec<RegionalSpellingWord>,
    pub flags: Vec<RegionalSpellingFlag>,
}

struct Spelling {
    systems: u8,
    sense_dependent: bool,
    row: usize,
}
struct Table {
    rows: Vec<Vec<&'static str>>,
    spellings: HashMap<&'static str, Spelling>,
}
const ALL_SYSTEMS: u8 = 0b111;

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Table {
            rows: Vec::new(),
            spellings: HashMap::new(),
        };
        for line in include_str!("../../data/spelling/regional.txt").lines() {
            if line.starts_with('#') {
                continue;
            }
            let row = table.rows.len();
            let mut forms = Vec::new();
            for entry in line.split(' ') {
                let (form, flags) = entry.split_once('/').expect("regional spelling row");
                let systems = flags.bytes().fold(0, |bits, flag| {
                    bits | match flag {
                        b'a' => SpellingSystem::American.bit(),
                        b'b' => SpellingSystem::British.bit(),
                        b'z' => SpellingSystem::Oxford.bit(),
                        _ => 0,
                    }
                });
                // A spelling in several rows keeps the first; every row gives it the same systems.
                table.spellings.entry(form).or_insert(Spelling {
                    systems,
                    sense_dependent: flags.ends_with('~'),
                    row,
                });
                forms.push(form);
            }
            table.rows.push(forms);
        }
        table
    })
}

fn systems(bits: u8) -> Vec<SpellingSystem> {
    SpellingSystem::ALL
        .into_iter()
        .filter(|system| bits & system.bit() != 0)
        .collect()
}

fn capitalized(text: &str) -> bool {
    text.chars().next().is_some_and(char::is_uppercase)
}

/// A capitalized word counts only where any word is capitalized: the first word of its sentence
/// (after any dash, ellipsis or punctuation), after an opening parenthesis, or after an opening
/// quote inside dialogue, and not followed by another capitalized word (`Labour Party`,
/// `Harbor Street`). All-caps words count. Elsewhere a capital marks a name (`Pearl Harbor`),
/// and a capital after a quote in narration marks a title (`the “Endeavour”`).
fn probable_name(document: &Document, tokens: &[&Token], at: usize) -> bool {
    let token = tokens[at];
    if !capitalized(&token.text) || token.text.chars().all(|c| !c.is_lowercase()) {
        return false;
    }
    let previous = at.checked_sub(1).map(|i| tokens[i].text.as_str());
    let opening = if previous.is_some_and(|p| ["\"", "“", "‘", "'"].contains(&p)) {
        document.mode_at(token.span.byte.start) == Mode::Dialogue
    } else {
        previous == Some("(") || tokens[..at].iter().all(|t| !t.is_word)
    };
    !opening
        || tokens
            .get(at + 1)
            .is_some_and(|next| next.is_word && capitalized(&next.text))
}

struct Occurrence {
    span: Span,
    sentence: usize,
    form: &'static str,
    mode: Mode,
}

pub fn report(
    document: &Document,
    options: &RegionalSpellingOptions,
) -> Result<RegionalSpellingReport, StyleError> {
    if options.minimum_markers == 0 {
        return Err(StyleError::options("minimumMarkers must be at least 1"));
    }
    let table = table();
    let mut occurrences = Vec::new();
    for (index, sentence) in document.sentences().enumerate() {
        let tokens: Vec<_> = sentence
            .tokens
            .iter()
            .filter(|t| !t.is_whitespace)
            .collect();
        for at in 0..tokens.len() {
            let token = tokens[at];
            if !token.is_word || token.is_skipped || probable_name(document, &tokens, at) {
                continue;
            }
            for (start, part) in parts(&token.text, &['-', '\u{2010}']) {
                let folded = part.to_lowercase();
                if let Some((&form, _)) = table.spellings.get_key_value(folded.as_str()) {
                    occurrences.push(Occurrence {
                        span: part_span(&token.span, &token.text, start, part),
                        sentence: index,
                        form,
                        mode: document.mode_at(token.span.byte.start),
                    });
                }
            }
        }
    }
    let spelling = |form: &str| &table.spellings[form];
    let markers: Vec<_> = occurrences
        .iter()
        .filter(|o| spelling(o.form).systems != ALL_SYSTEMS)
        .collect();
    let counts: Vec<_> = SpellingSystem::ALL
        .into_iter()
        .map(|system| SpellingSystemCount {
            system,
            count: markers
                .iter()
                .filter(|o| spelling(o.form).systems & system.bit() != 0)
                .count(),
        })
        .collect();
    let dominant: Vec<_> = match options.system {
        Some(system) => vec![system],
        None if markers.len() >= options.minimum_markers => {
            let top = counts.iter().map(|c| c.count).max().unwrap_or(0);
            counts
                .iter()
                .filter(|c| c.count == top && top > 0)
                .map(|c| c.system)
                .collect()
        }
        None => Vec::new(),
    };
    let dominant_bits = dominant.iter().fold(0, |bits, system| bits | system.bit());

    // Words: forms of one variant row seen in the document. A sense-dependent spelling (`check`,
    // `tire`) cannot show which sense was meant, so it never makes a word mixed.
    let mut rows: BTreeMap<usize, BTreeMap<&'static str, Vec<&Occurrence>>> = BTreeMap::new();
    for occurrence in &occurrences {
        let entry = spelling(occurrence.form);
        if !entry.sense_dependent {
            rows.entry(entry.row)
                .or_default()
                .entry(occurrence.form)
                .or_default()
                .push(occurrence);
        }
    }
    let word_of = |row: usize| {
        let forms = &table.rows[row];
        forms
            .iter()
            .find(|form| spelling(form).systems & SpellingSystem::American.bit() != 0)
            .unwrap_or(&forms[0])
            .to_string()
    };
    let mut words = Vec::new();
    let mut minority: HashMap<(usize, usize), &'static str> = HashMap::new();
    for (&row, forms) in &rows {
        if forms
            .keys()
            .all(|form| spelling(form).systems == ALL_SYSTEMS)
        {
            continue;
        }
        let mixed = forms.len() > 1;
        if mixed && dominant_bits != 0 {
            let top = forms.values().map(Vec::len).max().unwrap_or(0);
            let leaders: Vec<_> = forms
                .iter()
                .filter(|(_, list)| list.len() == top)
                .map(|(&form, _)| form)
                .collect();
            // A leader the dominant system rejects is flagged itself; its rivals are not minorities.
            if let Some(&leader) = leaders.first().filter(|&&leader| {
                leaders.len() == 1 && spelling(leader).systems & dominant_bits != 0
            }) {
                for (_, list) in forms.iter().filter(|(&form, _)| form != leader) {
                    for occurrence in list {
                        minority.insert(
                            (occurrence.span.byte.start, occurrence.span.byte.end),
                            leader,
                        );
                    }
                }
            }
        }
        words.push(RegionalSpellingWord {
            word: word_of(row),
            mixed,
            forms: forms
                .iter()
                .map(|(&form, list)| {
                    let mut counts = Split::default();
                    for occurrence in list {
                        for count in counts.all_and_mode_mut(occurrence.mode) {
                            *count += 1;
                        }
                    }
                    RegionalSpellingForm {
                        form: form.to_owned(),
                        systems: systems(spelling(form).systems),
                        counts,
                        spans: list.iter().map(|o| o.span.clone()).collect(),
                    }
                })
                .collect(),
        });
    }

    // Flags: a spelling no dominant system accepts, or the minority form of a mixed word.
    let mut flags = Vec::new();
    for occurrence in &occurrences {
        let entry = spelling(occurrence.form);
        let key = (occurrence.span.byte.start, occurrence.span.byte.end);
        let (reason, dominant_form) = if dominant_bits != 0 && entry.systems & dominant_bits == 0 {
            let counterpart = table.rows[entry.row]
                .iter()
                .find(|form| spelling(form).systems & dominant_bits == dominant_bits)
                .map(|form| (*form).to_owned());
            (RegionalSpellingReason::System, counterpart)
        } else if let Some(leader) = minority.get(&key) {
            (RegionalSpellingReason::Form, Some((*leader).to_owned()))
        } else {
            continue;
        };
        flags.push(RegionalSpellingFlag {
            span: occurrence.span.clone(),
            sentence: occurrence.sentence,
            form: occurrence.form.to_owned(),
            word: word_of(entry.row),
            systems: systems(entry.systems),
            reason,
            dominant_form,
            mode: occurrence.mode,
        });
    }
    Ok(RegionalSpellingReport {
        markers: markers.len(),
        systems: counts,
        dominant,
        words,
        flags,
    })
}
