//! Flags a word whose spelling variant differs from one used earlier in the text.

use crate::document::{
    DocumentFix, DocumentMatch, ParagraphDigest, TokenSpan, VariantCandidate, VariantWord,
};
use crate::types::{DefaultHashMap, Position, Sentence, Token};
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
/// the spelling of a word across all paragraphs. So this is a whole-text rule (see
/// [`crate::document`]): [`Rules::suggest`](crate::Rules::suggest) remembers
/// spellings across the text it is given, and an app that checks a document
/// paragraph by paragraph gets the variants used in other paragraphs from
/// [`Rules::resolve_document`](crate::Rules::resolve_document).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WordCoherency {
    /// Maps each admitted spelling to the other spellings of the same word.
    #[serde(serialize_with = "crate::types::sorted::map")]
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

    /// Adds the tokens of one sentence that have a spelling variant to `words`, those of a
    /// digest whose text starts at `start`. Which of them is flagged depends on the spellings
    /// used before, so [`WordCoherency::resolve`] decides that.
    pub(crate) fn digest(
        &self,
        sentence: &Sentence,
        start: Position,
        words: &mut Vec<VariantWord>,
    ) {
        if self.variants.is_empty() {
            return;
        }
        let sentence_start = sentence.span().start().byte;
        for token in sentence.tokens() {
            let span = token.span();
            // The original text, which `DocumentMatch::to_suggestion` sees.
            let Some(original) = sentence
                .text()
                .get(span.byte().start - sentence_start..span.byte().end - sentence_start)
            else {
                continue;
            };
            // Candidates without variants are never flagged and register nothing.
            let candidates: Vec<VariantCandidate> = candidates(token)
                .into_iter()
                .filter_map(|word| {
                    let others = self.variants.get(&word)?;
                    // Past 32 variants a match is assumed to have a replacement; the
                    // suggestion then checks it.
                    let replaceable = others
                        .iter()
                        .enumerate()
                        .map(|(i, other)| {
                            let ok = i >= 32 || replacement(original, &word, other).is_some();
                            u32::from(ok) << i.min(31)
                        })
                        .fold(0, |mask, bit| mask | bit);
                    Some(VariantCandidate { word, replaceable })
                })
                .collect();
            if !candidates.is_empty() {
                words.push(VariantWord {
                    span: TokenSpan::new(span, start),
                    candidates,
                });
            }
        }
    }

    /// The matches in a document made of `digests` (the current ones, with their indices).
    pub(crate) fn resolve<'a>(
        &self,
        digests: impl Iterator<Item = (usize, &'a ParagraphDigest)>,
    ) -> Vec<DocumentMatch> {
        // Other spelling that should not appear -> the spelling already seen.
        let mut should_not_appear: DefaultHashMap<&str, &str> = DefaultHashMap::default();
        let mut matches = Vec::new();

        for (paragraph, digest) in digests {
            for word in &digest.variants {
                for candidate in &word.candidates {
                    let Some(others) = self.variants.get_key_value(candidate.word.as_str()) else {
                        continue;
                    };
                    // A spelling seen earlier in the text is reported and no variant of it is
                    // registered, matching the rule the builtin reimplements.
                    if let Some(other) = should_not_appear.get(candidate.word.as_str()) {
                        let i = others.1.iter().position(|x| x == other).unwrap_or(0);
                        if i >= 32 || candidate.replaceable & (1 << i) != 0 {
                            matches.push(DocumentMatch::new(
                                paragraph,
                                format!("BUILTIN/{}", ID),
                                format!(
                                    "Do not mix variants of the same word ('{}' and '{}') \
                                     within a single text.",
                                    candidate.word, other
                                ),
                                word.span.span(),
                                DocumentFix::Variant {
                                    word: candidate.word.clone(),
                                    other: (*other).to_owned(),
                                },
                            ));
                        }
                        break;
                    }

                    let (spelling, others) = others;
                    for other in others {
                        should_not_appear.insert(other, spelling);
                    }
                }
            }
        }

        matches
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
pub(crate) fn replacement(surface: &str, word: &str, other: &str) -> Option<String> {
    let found = find_case_insensitive(surface, word)?;

    let mut replacement = String::with_capacity(surface.len() + other.len());
    replacement.push_str(&surface[..found.start]);
    replacement.push_str(other);
    replacement.push_str(&surface[found.end..]);

    if replacement.eq_ignore_ascii_case(surface) {
        return None;
    }

    if surface.chars().next().is_some_and(char::is_uppercase) {
        uppercase_first(&mut replacement);
    }

    Some(replacement)
}

/// Returns the byte range of the first case-insensitive occurrence of `needle`
/// in `haystack`. Only whole character comparisons are made, so the range
/// always falls on character boundaries, also where a character and its
/// lowercase form have different lengths.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<std::ops::Range<usize>> {
    if needle.is_empty() {
        return Some(0..0);
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
            let bytes = |chars: &[char]| chars.iter().map(|c| c.len_utf8()).sum::<usize>();
            let start_byte = bytes(&haystack[..start]);
            return Some(start_byte..start_byte + bytes(&haystack[start..start + needle.len()]));
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
