//! Flags a word whose spelling variant differs from one used earlier in the text.

use crate::types::{DefaultHashMap, Sentence, Suggestion, Token};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_WORD_COHERENCY";

/// Flags words for which two spellings are admitted when the other spelling was
/// already used earlier in the text.
///
/// The data lists pairs such as `adviser`/`advisor` in both directions. As soon
/// as one of the two spellings occurs, the other one is remembered as a
/// spelling that should not appear; every later occurrence of it is flagged and
/// the spelling seen first is offered as the replacement. A word is matched by
/// its base form (lemma) when the tagger knows it and by its surface form
/// otherwise, mirroring the corresponding LanguageTool rule.
///
/// # Scope
///
/// The corresponding LanguageTool rule runs over a whole document: it remembers
/// the spelling of a word across all paragraphs. This builtin only sees the
/// sentences of a single paragraph at a time, so a variant used in one paragraph
/// is not remembered in the next one and mixing variants across paragraphs is
/// not reported. Within one paragraph the behavior matches LanguageTool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordCoherency {
    /// Maps each admitted spelling to the other spellings of the same word.
    variants: DefaultHashMap<String, Vec<String>>,
}

impl WordCoherency {
    /// Creates a new rule from the `other;spelling` pairs of the data file.
    pub(crate) fn new(pairs: Vec<(String, String)>) -> Self {
        let mut variants: DefaultHashMap<String, Vec<String>> = DefaultHashMap::default();
        for (first, second) in pairs {
            add_variant(&mut variants, &first, &second);
            add_variant(&mut variants, &second, &first);
        }
        WordCoherency { variants }
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Checks all sentences of one paragraph and returns the resulting
    /// suggestions.
    pub fn check_text(&self, sentences: &[Sentence]) -> Vec<Suggestion> {
        if self.variants.is_empty() {
            return Vec::new();
        }

        // Other spelling that should not appear -> the spelling already seen.
        let mut should_not_appear: DefaultHashMap<String, String> = DefaultHashMap::default();
        let mut suggestions = Vec::new();

        for sentence in sentences {
            for token in sentence.tokens() {
                let surface = token.word().text().as_str();

                for candidate in candidates(token) {
                    // A spelling seen earlier in the paragraph is reported and
                    // no variant of it is registered, matching the rule the
                    // builtin reimplements.
                    if let Some(other_spelling) = should_not_appear.get(candidate.as_str()) {
                        if let Some(replacement) = replacement(surface, &candidate, other_spelling)
                        {
                            suggestions.push(Suggestion::new(
                                format!("BUILTIN/{}", ID),
                                format!(
                                    "Do not mix variants of the same word ('{}' and '{}') \
                                     within a single text.",
                                    candidate, other_spelling
                                ),
                                token.span().clone(),
                                vec![replacement],
                            ));
                        }
                        break;
                    }

                    if let Some(others) = self.variants.get(candidate.as_str()) {
                        for other in others {
                            should_not_appear.insert(other.clone(), candidate.clone());
                        }
                    }
                }
            }
        }

        suggestions
    }
}

/// Adds `other` to the set of spellings of `word` if it is not there yet.
fn add_variant(variants: &mut DefaultHashMap<String, Vec<String>>, word: &str, other: &str) {
    let entry = variants.entry(word.to_owned()).or_default();
    if !entry.iter().any(|existing| existing == other) {
        entry.push(other.to_owned());
    }
}

/// The keys under which a token is looked up: the lemmas of all its readings, or
/// the surface form when the tagger does not know the token. Duplicates (which
/// are common because several readings share a lemma) are removed.
fn candidates(token: &Token) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    for tag in token.word().tags() {
        let lemma = tag.lemma().as_str();
        if !lemma.is_empty() && !candidates.iter().any(|candidate| candidate == lemma) {
            candidates.push(lemma.to_owned());
        }
    }

    if candidates.is_empty() {
        candidates.push(token.word().text().as_str().to_owned());
    }

    candidates
}

/// Builds the replacement for `surface` by replacing the first occurrence of
/// `word` (case-insensitively) with `other`. Returns `None` when `word` does not
/// occur in `surface` or when the result would only differ in its capitalization,
/// i.e. exactly the cases in which the rule the builtin reimplements emits no
/// suggestion.
fn replacement(surface: &str, word: &str, other: &str) -> Option<String> {
    let start = find_case_insensitive(surface, word)?;

    let mut replacement = String::with_capacity(surface.len() + other.len());
    replacement.push_str(&surface[..start]);
    replacement.push_str(other);
    replacement.push_str(&surface[start + word.len()..]);

    if replacement.eq_ignore_ascii_case(surface) {
        return None;
    }

    if surface.chars().next().is_some_and(char::is_uppercase) {
        uppercase_first(&mut replacement);
    }

    Some(replacement)
}

/// Returns the byte offset of the first case-insensitive occurrence of `needle`
/// in `haystack`. Only whole character comparisons are made, so the returned
/// offset is always a character boundary.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }

    let haystack: Vec<char> = haystack.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.len() > haystack.len() {
        return None;
    }

    for start in 0..=haystack.len() - needle.len() {
        let matches = haystack[start..start + needle.len()]
            .iter()
            .zip(&needle)
            .all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()));
        if matches {
            return Some(haystack[..start].iter().map(|c| c.len_utf8()).sum());
        }
    }

    None
}

/// Replaces the first character of `text` with its uppercase version.
fn uppercase_first(text: &mut String) {
    let mut chars = text.chars();
    if let Some(first) = chars.next() {
        let rest = chars.as_str();
        *text = first.to_uppercase().collect::<String>() + rest;
    }
}
