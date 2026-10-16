use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::error::StyleError;
use crate::{Document, Span, Token};

pub(crate) const REPORT: &str = "echoes";

const CONTENT_TAGS: [&str; 4] = ["NN", "VB", "JJ", "RB"];

#[rustfmt::skip]
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "across", "after", "again", "against", "ah", "all", "almost", "along", "also",
    "although", "am", "amid", "amidst", "among", "amongst", "an", "and", "any", "anybody", "anyone",
    "anything", "are", "around", "as", "at", "be", "because", "been", "before", "behind", "being",
    "below", "beneath", "beside", "besides", "between", "beyond", "both", "but", "by", "can",
    "could", "despite", "did", "do", "does", "doing", "done", "during", "each", "either", "else",
    "even", "ever", "every", "everybody", "everyone", "everything", "except", "for", "from", "had",
    "has", "have", "having", "he", "her", "here", "hers", "herself", "him", "himself", "his", "how",
    "i", "if", "in", "into", "is", "it", "its", "itself", "just", "least", "less", "many", "may",
    "me", "might", "mine", "more", "most", "much", "must", "my", "myself", "n't", "neither",
    "never", "no", "nobody", "none", "nor", "not", "nothing", "now", "o", "of", "oh", "on", "once",
    "one", "only", "onto", "or", "other", "ought", "our", "ours", "ourselves", "over", "per",
    "quite", "rather", "same", "shall", "she", "should", "since", "so", "some", "somebody",
    "someone", "something", "still", "such", "than", "that", "the", "thee", "their", "theirs",
    "them", "themselves", "then", "there", "these", "they", "thine", "this", "those", "thou",
    "though", "through", "throughout", "thus", "thy", "till", "to", "too", "toward", "towards",
    "under", "underneath", "until", "unto", "upon", "us", "very", "via", "was", "we", "were",
    "what", "when", "where", "whether", "which", "while", "who", "whom", "whose", "why", "will",
    "with", "within", "without", "would", "ye", "yes", "yet", "you", "your", "yours", "yourself",
    "yourselves",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "camelCase")]
#[non_exhaustive]
pub struct EchoOptions {
    pub window: usize,
    #[serde(alias = "match_forms")]
    pub match_forms: bool,
}

impl Default for EchoOptions {
    fn default() -> Self {
        Self {
            window: 30,
            match_forms: true,
        }
    }
}

impl EchoOptions {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_window(mut self, window: usize) -> Self {
        self.window = window;
        self
    }
    pub fn with_match_forms(mut self, match_forms: bool) -> Self {
        self.match_forms = match_forms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct Echo {
    pub key: String,
    pub label: String,
    pub earlier: Span,
    pub later: Span,
    pub distance: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub struct EchoReport {
    pub echoes: Vec<Echo>,
    pub words: usize,
    pub per_thousand_words: f64,
}

pub(crate) fn fold(text: &str) -> String {
    text.replace('’', "'").to_lowercase()
}

fn listed(word: &str) -> bool {
    FUNCTION_WORDS.binary_search(&word).is_ok()
}

fn echo_families(token: &Token) -> Vec<&'static str> {
    let families: Vec<_> = CONTENT_TAGS
        .iter()
        .copied()
        .filter(|tag| token.readings.iter().any(|r| r.pos.starts_with(tag)))
        .collect();
    if families.len() <= 1 {
        return families;
    }
    if families.contains(&"VB") && token.chunks.iter().any(|c| c.contains("-VP")) {
        return vec!["VB"];
    }
    if token.chunks.iter().any(|c| c.contains("-NP")) {
        if families.contains(&"NN") {
            return vec!["NN"];
        }
        if families.contains(&"JJ") {
            return vec!["JJ"];
        }
    }
    if fold(&token.text).ends_with("ing") && token.readings.iter().any(|r| r.pos == "VBG") {
        return vec!["VB"];
    }
    if families.iter().all(|tag| matches!(*tag, "NN" | "VB")) {
        vec!["NN"]
    } else {
        Vec::new()
    }
}

fn echo_lemmas(token: &Token, families: &[&str]) -> BTreeSet<String> {
    let candidates: BTreeSet<_> = token
        .readings
        .iter()
        .filter(|r| families.iter().any(|tag| r.pos.starts_with(tag)))
        .map(|r| fold(&r.lemma))
        .filter(|lemma| !lemma.is_empty() && !lemma.contains(char::is_whitespace))
        .collect();
    if candidates.len() == 1
        && !token
            .readings
            .iter()
            .any(|r| matches!(r.pos.as_str(), "JJR" | "JJS" | "RBR" | "RBS"))
    {
        candidates
    } else {
        BTreeSet::new()
    }
}

fn lemmas(token: &Token) -> BTreeSet<String> {
    token
        .readings
        .iter()
        .map(|r| fold(&r.lemma))
        .filter(|l| !l.is_empty() && !l.contains(char::is_whitespace))
        .collect()
}

fn suppressed(token: &Token, next: Option<&&Token>) -> bool {
    let word = fold(&token.text);
    matches!(
        word.as_str(),
        "say"
            | "said"
            | "says"
            | "saying"
            | "ask"
            | "asked"
            | "asks"
            | "asking"
            | "up"
            | "down"
            | "out"
            | "back"
            | "off"
            | "over"
            | "away"
    ) || (crate::document::honorific(&word)
        && next.is_some_and(|t| t.text.chars().next().is_some_and(char::is_uppercase)))
}

#[derive(Default)]
struct Spelling {
    occurrences: usize,
    content: usize,
    function: bool,
    lemmas: BTreeSet<String>,
}

pub(crate) struct Lexicon {
    function: HashSet<String>,
    families: HashMap<String, String>,
}

impl Lexicon {
    pub(crate) fn new(document: &Document) -> Self {
        let mut spellings: HashMap<String, Spelling> = HashMap::new();
        for token in document.words() {
            let spelling = fold(&token.text);
            let entry = spellings.entry(spelling).or_default();
            entry.occurrences += 1;
            let mut content = token.readings.is_empty();
            for reading in &token.readings {
                if CONTENT_TAGS.iter().any(|tag| reading.pos.starts_with(tag))
                    && !listed(&fold(&reading.lemma))
                {
                    content = true;
                }
                if reading.pos.starts_with("CD") {
                    entry.function = true;
                }
            }
            entry.content += usize::from(content);
            entry.lemmas.extend(lemmas(token));
        }
        let mut function = HashSet::new();
        let mut families = HashMap::new();
        for (spelling, entry) in spellings {
            if entry.function || listed(&spelling) || entry.content * 2 < entry.occurrences {
                function.insert(spelling);
                continue;
            }
            let key = if entry.lemmas.len() == 1 {
                entry.lemmas.into_iter().next().unwrap()
            } else {
                spelling.clone()
            };
            families.insert(spelling, key);
        }
        Self { function, families }
    }

    pub(crate) fn is_function(&self, token: &Token) -> bool {
        self.function.contains(&fold(&token.text))
    }

    pub(crate) fn family(&self, token: &Token) -> String {
        let spelling = fold(&token.text);
        self.families.get(&spelling).cloned().unwrap_or(spelling)
    }

    pub(crate) fn label<'a>(&self, tokens: impl IntoIterator<Item = &'a Token>) -> String {
        let tokens: Vec<&Token> = tokens.into_iter().collect();
        let spellings: Vec<String> = tokens.iter().map(|token| fold(&token.text)).collect();
        let shared = |with_spelling: bool| {
            tokens
                .iter()
                .map(|token| {
                    let mut lemmas = lemmas(token);
                    if with_spelling {
                        lemmas.insert(fold(&token.text));
                    }
                    lemmas
                })
                .reduce(|left, right| left.intersection(&right).cloned().collect())
                .unwrap_or_default()
        };
        [false, true]
            .into_iter()
            .map(shared)
            .find_map(|shared| {
                shared
                    .iter()
                    .find(|lemma| spellings.contains(lemma))
                    .or_else(|| shared.iter().next())
                    .cloned()
            })
            .unwrap_or_else(|| spellings[0].clone())
    }
}

pub fn report(document: &Document, options: &EchoOptions) -> Result<EchoReport, StyleError> {
    if !document.token_details {
        return Err(StyleError::needs_token_details(REPORT));
    }
    let lexicon = Lexicon::new(document);
    let mut last: HashMap<String, (usize, &Token)> = HashMap::new();
    let mut echoes = Vec::new();
    let tokens: Vec<_> = document.words().collect();
    for (index, token) in tokens.iter().copied().enumerate() {
        if lexicon.is_function(token) || suppressed(token, tokens.get(index + 1)) {
            continue;
        }
        let mut candidates = if options.match_forms {
            let families = echo_families(token);
            let lemmas = echo_lemmas(token, &families);
            families
                .into_iter()
                .flat_map(|pos| {
                    lemmas
                        .iter()
                        .map(move |lemma| format!("lemma:{pos}:{lemma}"))
                })
                .collect()
        } else {
            BTreeSet::new()
        };
        candidates.insert(format!("surface:{}", fold(&token.text)));
        let nearest = candidates
            .iter()
            .filter_map(|key| last.get(key).map(|previous| (key, previous)))
            .max_by_key(|(_, (index, _))| *index);
        if let Some((family, (earlier, previous))) = nearest {
            let distance = index - earlier;
            let same_sentence = document.sentences().any(|s| {
                s.span.byte.start <= previous.span.byte.start
                    && token.span.byte.end <= s.span.byte.end
            });
            let turn = |t: &Token| {
                document.paragraphs.iter().position(|p| {
                    p.span.byte.start <= t.span.byte.start && t.span.byte.end <= p.span.byte.end
                })
            };
            let quoted = |t: &Token| {
                document
                    .dialogue
                    .iter()
                    .any(|s| s.byte.start <= t.span.byte.start && t.span.byte.end <= s.byte.end)
            };
            if distance <= options.window.min(30)
                && !(same_sentence && distance <= 10)
                && !(quoted(previous) && quoted(token) && turn(previous) != turn(token))
            {
                echoes.push(Echo {
                    label: if options.match_forms {
                        lexicon.label([*previous, token])
                    } else {
                        family.rsplit(':').next().unwrap().to_owned()
                    },
                    key: family.rsplit(':').next().unwrap().to_owned(),
                    earlier: previous.span.clone(),
                    later: token.span.clone(),
                    distance,
                });
            }
        }
        for key in candidates {
            last.insert(key, (index, token));
        }
    }
    let words = document.word_count;
    let per_thousand_words = if words == 0 {
        0.0
    } else {
        echoes.len() as f64 * 1000.0 / words as f64
    };
    Ok(EchoReport {
        echoes,
        words,
        per_thousand_words,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn function_words_are_sorted_for_binary_search() {
        assert!(super::FUNCTION_WORDS
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
    }
}
