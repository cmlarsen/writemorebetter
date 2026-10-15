use std::collections::{BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

use crate::{Document, Span, Token};

const CONTENT_TAGS: [&str; 4] = ["NN", "VB", "JJ", "RB"];

#[rustfmt::skip]
const STOPWORDS: &[&str] = &[
    "a", "about", "after", "again", "ah", "all", "almost", "also", "although", "am", "an", "and",
    "any", "are", "as", "at", "be", "because", "been", "before", "being", "both", "but", "by",
    "can", "could", "did", "do", "does", "doing", "done", "each", "either", "else", "even", "ever",
    "every", "for", "from", "had", "has", "have", "having", "he", "her", "here", "hers", "herself",
    "him", "himself", "his", "how", "i", "if", "in", "into", "is", "it", "its", "itself", "just",
    "least", "less", "many", "may", "me", "might", "mine", "more", "most", "much", "must", "my",
    "myself", "n't", "neither", "never", "no", "nor", "not", "now", "o", "of", "off", "oh", "on",
    "once", "one", "only", "onto", "or", "other", "ought", "our", "ours", "ourselves", "quite",
    "rather", "same", "shall", "she", "should", "so", "some", "still", "such", "than", "that",
    "the", "thee", "their", "theirs", "them", "themselves", "then", "there", "these", "they",
    "thine", "this", "those", "thou", "though", "thus", "thy", "to", "too", "upon", "us", "very",
    "was", "we", "were", "what", "when", "where", "whether", "which", "while", "who", "whom",
    "whose", "why", "will", "with", "would", "ye", "yes", "yet", "you", "your", "yours", "yourself",
    "yourselves",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
#[non_exhaustive]
pub struct EchoOptions {
    pub window: usize,
    #[serde(alias = "matchForms")]
    pub match_forms: bool,
}

impl Default for EchoOptions {
    fn default() -> Self {
        Self {
            window: 50,
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
#[non_exhaustive]
pub struct Echo {
    pub lemma: String,
    pub earlier: Span,
    pub later: Span,
    pub distance: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct EchoReport {
    pub echoes: Vec<Echo>,
    pub word_count: usize,
    pub density_per_1000: f64,
}

pub(crate) fn fold(text: &str) -> String {
    text.replace('’', "'").to_lowercase()
}

pub(crate) fn is_stopword(token: &Token) -> bool {
    token.readings.iter().any(|reading| {
        !reading.pos.is_empty() && !CONTENT_TAGS.iter().any(|tag| reading.pos.starts_with(tag))
    }) || STOPWORDS.binary_search(&fold(&token.text).as_str()).is_ok()
        || token.readings.iter().any(|reading| {
            STOPWORDS
                .binary_search(&fold(&reading.lemma).as_str())
                .is_ok()
        })
}

pub(crate) struct Keys(HashMap<String, String>);

impl Keys {
    pub(crate) fn new(document: &Document) -> Self {
        let mut lemmas: HashMap<String, BTreeSet<String>> = HashMap::new();
        for token in words(document).filter(|token| !is_stopword(token)) {
            lemmas.entry(fold(&token.text)).or_default().extend(
                token
                    .readings
                    .iter()
                    .map(|reading| fold(&reading.lemma))
                    .filter(|lemma| !lemma.is_empty() && !lemma.contains(char::is_whitespace)),
            );
        }
        Self(
            lemmas
                .into_iter()
                .map(|(surface, lemmas)| {
                    let key = lemmas
                        .into_iter()
                        .find(|lemma| *lemma != surface)
                        .unwrap_or_else(|| surface.clone());
                    (surface, key)
                })
                .collect(),
        )
    }

    pub(crate) fn key(&self, surface: String) -> String {
        self.0.get(&surface).cloned().unwrap_or(surface)
    }
}

pub(crate) fn words(document: &Document) -> impl Iterator<Item = &Token> {
    document
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.sentences)
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| token.is_word && !token.is_skipped)
}

pub fn echoes(document: &Document, options: &EchoOptions) -> EchoReport {
    let keys = Keys::new(document);
    let mut last: HashMap<String, (usize, &Token)> = HashMap::new();
    let mut echoes = Vec::new();
    for (index, token) in words(document).enumerate() {
        if is_stopword(token) {
            continue;
        }
        let surface = fold(&token.text);
        let key = if options.match_forms {
            keys.key(surface)
        } else {
            surface
        };
        if let Some((earlier, previous)) = last.get(&key) {
            let distance = index - earlier;
            if distance <= options.window {
                echoes.push(Echo {
                    lemma: key.clone(),
                    earlier: previous.span.clone(),
                    later: token.span.clone(),
                    distance,
                });
            }
        }
        last.insert(key, (index, token));
    }
    let word_count = document.word_count;
    let density_per_1000 = if word_count == 0 {
        0.0
    } else {
        echoes.len() as f64 * 1000.0 / word_count as f64
    };
    EchoReport {
        echoes,
        word_count,
        density_per_1000,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn stopwords_are_sorted_for_binary_search() {
        assert!(super::STOPWORDS.windows(2).all(|pair| pair[0] < pair[1]));
    }
}
