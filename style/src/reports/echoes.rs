use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{Document, Span, Token};

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
    pub key: String,
    pub label: String,
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

fn listed(word: &str) -> bool {
    FUNCTION_WORDS.binary_search(&word).is_ok()
}

fn lemmas(token: &Token) -> BTreeSet<String> {
    token
        .readings
        .iter()
        .map(|reading| fold(&reading.lemma))
        .filter(|lemma| !lemma.is_empty() && !lemma.contains(char::is_whitespace))
        .collect()
}

pub(crate) fn words(document: &Document) -> impl Iterator<Item = &Token> {
    document
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.sentences)
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| token.is_word && !token.is_skipped)
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

fn root(parent: &mut [usize], mut node: usize) -> usize {
    while parent[node] != node {
        parent[node] = parent[parent[node]];
        node = parent[node];
    }
    node
}

impl Lexicon {
    pub(crate) fn new(document: &Document) -> Self {
        let mut spellings: HashMap<String, Spelling> = HashMap::new();
        for token in words(document) {
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
        let mut nodes: HashMap<String, usize> = HashMap::new();
        let mut parent: Vec<usize> = Vec::new();
        let mut node = |name: String, parent: &mut Vec<usize>| {
            *nodes.entry(name).or_insert_with(|| {
                parent.push(parent.len());
                parent.len() - 1
            })
        };
        let mut members = Vec::new();
        for (spelling, entry) in spellings {
            if entry.function || listed(&spelling) || entry.content * 2 < entry.occurrences {
                function.insert(spelling);
                continue;
            }
            let own = node(format!("s:{spelling}"), &mut parent);
            for lemma in entry.lemmas.iter().chain([&spelling]) {
                let other = node(format!("l:{lemma}"), &mut parent);
                let (left, right) = (root(&mut parent, own), root(&mut parent, other));
                parent[left] = right;
            }
            members.push((spelling, own, entry.lemmas));
        }
        let mut names: HashMap<usize, String> = HashMap::new();
        for (spelling, own, lemmas) in &members {
            let family = root(&mut parent, *own);
            let candidate = lemmas.iter().next().unwrap_or(spelling);
            names
                .entry(family)
                .and_modify(|name| {
                    if candidate < name {
                        candidate.clone_into(name);
                    }
                })
                .or_insert_with(|| candidate.clone());
        }
        let families = members
            .into_iter()
            .map(|(spelling, own, _)| {
                let name = names[&root(&mut parent, own)].clone();
                (spelling, name)
            })
            .collect();
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

pub fn echoes(document: &Document, options: &EchoOptions) -> EchoReport {
    let lexicon = Lexicon::new(document);
    let mut last: HashMap<String, (usize, &Token)> = HashMap::new();
    let mut echoes = Vec::new();
    for (index, token) in words(document).enumerate() {
        if lexicon.is_function(token) {
            continue;
        }
        let family = if options.match_forms {
            lexicon.family(token)
        } else {
            fold(&token.text)
        };
        if let Some((earlier, previous)) = last.get(&family) {
            let distance = index - earlier;
            if distance <= options.window {
                echoes.push(Echo {
                    label: if options.match_forms {
                        lexicon.label([*previous, token])
                    } else {
                        family.clone()
                    },
                    key: family.clone(),
                    earlier: previous.span.clone(),
                    later: token.span.clone(),
                    distance,
                });
            }
        }
        last.insert(family, (index, token));
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
    fn function_words_are_sorted_for_binary_search() {
        assert!(super::FUNCTION_WORDS
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
    }
}
