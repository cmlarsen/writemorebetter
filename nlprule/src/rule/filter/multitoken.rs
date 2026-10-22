//! `org.languagetool.rules.spelling.multitoken.MultitokenSpellerFilter` and the search of
//! `MultitokenSpeller.getSuggestions` it runs, as in LanguageTool 6.6.

use unicode_categories::UnicodeCategories;

use super::{group, is_capitalized_word, is_misspelled, FilterMatch};
use crate::rule::engine::composition::{MatchGraph, MatchSentence};
use crate::spell::multitoken::{normalize_key, MultitokenSpeller};
use crate::spell::Speller;
use crate::types::{Span, Token};
use crate::utils::uppercase_first_letter_or_digit;

const MAX_LENGTH_DIFF: usize = 3;

/// Suggests the known multi-word name or phrase closest to the matched text, and vetoes the
/// match when there is none.
///
/// `speller` is the selected variant's. The run-on check asks `default_speller` instead, as
/// LanguageTool's `EnglishMultitokenSpeller` is built once with the `en_US` spelling rule.
pub(super) fn accept<'t>(
    mut m: FilterMatch,
    sentence: &'t MatchSentence<'t>,
    graph: &MatchGraph<'t>,
    speller: Option<&Speller>,
    default_speller: Option<&Speller>,
) -> Option<FilterMatch> {
    let speller = speller?;
    let run_on_speller = default_speller.unwrap_or(speller);
    let mut tokens: Vec<&Token> = Vec::new();
    let mut n = 1;
    while let Some(g) = group(graph, n) {
        tokens.extend(g.tokens(sentence));
        n += 1;
    }
    if tokens.iter().all(|token| token.ignore_spelling()) {
        return None;
    }

    let underlined = sentence.slice(Span::from_positions(m.from, m.to));
    let accepted_by_speller = !is_misspelled(speller, underlined);
    let mut replacements = suggestions(
        speller.multitoken(),
        underlined,
        accepted_by_speller,
        |word| !word.is_empty() && !run_on_speller.is_correct(word),
    );
    if replacements.is_empty() {
        return None;
    }

    if utf16_len(underlined) > 4 && is_all_uppercase(underlined) {
        replacements = dedup_without(replacements.iter().map(|x| x.to_uppercase()), underlined);
    } else {
        let first_word = sentence
            .iter()
            .skip(1)
            .find(|token| token.word().as_str().chars().any(char::is_alphabetic));
        if first_word.is_some_and(|word| word.span().char().start == tokens[0].span().char().start)
        {
            replacements = dedup_without(
                replacements.iter().map(|x| {
                    if *x == x.to_lowercase() {
                        uppercase_first_letter_or_digit(x)
                    } else {
                        x.clone()
                    }
                }),
                underlined,
            );
        }
    }
    if replacements.is_empty() {
        return None;
    }
    m.replacements = replacements;
    Some(m)
}

fn dedup_without(replacements: impl Iterator<Item = String>, underlined: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for replacement in replacements {
        if !out.contains(&replacement) && replacement != underlined {
            out.push(replacement);
        }
    }
    out
}

/// `MultitokenSpeller.getSuggestions`. `is_misspelled` answers for single words, for the run-on
/// check.
pub(super) fn suggestions(
    speller: &MultitokenSpeller,
    original: &str,
    accepted_by_speller: bool,
    is_misspelled: impl Fn(&str) -> bool,
) -> Vec<String> {
    let original = collapse_space_separators(original);
    let word = original.replace("- ", "-").replace(" -", "-");
    if discard_run_on_words(&word, &is_misspelled) {
        return Vec::new();
    }
    let normalized = normalize_key(&word);
    let mut weighted: Vec<(&str, usize)> = Vec::new();
    if let Some(candidates) = speller.spellings_without_spaces(&normalized.replace(' ', "")) {
        if stop_searching(candidates.clone(), &original) {
            return Vec::new();
        }
        weighted.extend(candidates.map(|candidate| (candidate, 0)));
    }

    if let (true, Some(first)) = (weighted.is_empty(), normalized.chars().next()) {
        let word_len = utf16_len(&word);
        let word_parts = split_by_space(&normalized);
        for (key, candidates) in speller.phrases_starting_with(first) {
            if stop_searching(candidates.clone(), &original) {
                return Vec::new();
            }
            let key_len = utf16_len(key);
            if key_len.abs_diff(word_len) > MAX_LENGTH_DIFF {
                continue;
            }
            let key_parts = split_by_space(key);
            let distances = distances_per_word(&key_parts, &word_parts, key, &normalized);
            let total: usize = distances.iter().sum();
            if total < 1 {
                weighted.extend(candidates.map(|candidate| (candidate, 0)));
                if weighted.len() == 2 {
                    break;
                }
                continue;
            }
            if key_len < 7 {
                continue;
            }
            let part_len = |parts: &[&str], i: usize| parts.get(i).map_or(0, |x| utf16_len(x));
            let exceeds_max_distance_per_token = distances.iter().enumerate().any(|(i, &d)| {
                let max_distance = if part_len(&word_parts, i) > 5 && part_len(&key_parts, i) > 4 {
                    2
                } else {
                    1
                };
                d > max_distance
            });
            if exceeds_max_distance_per_token {
                continue;
            }
            if total as i64 <= max_edit_distance(key, &normalized) {
                weighted.extend(candidates.map(|candidate| (candidate, total)));
            }
        }
    }

    if weighted.is_empty() {
        return Vec::new();
    }
    weighted.sort_by_key(|(_, weight)| *weight);
    let (first, first_weight) = weighted[0];
    if accepted_by_speller && (first.to_uppercase() == original || first_weight > 1) {
        return Vec::new();
    }
    weighted
        .into_iter()
        .filter(|(_, weight)| *weight == first_weight)
        .map(|(candidate, _)| candidate.to_owned())
        .collect()
}

fn utf16_len(text: &str) -> usize {
    text.encode_utf16().count()
}

/// `StringTools.isAllUppercase`: no lowercase letter.
fn is_all_uppercase(text: &str) -> bool {
    !text.chars().any(|c| c.is_alphabetic() && c.is_lowercase())
}

/// Replaces each run of space separators (`\p{Zs}+`) with one space.
fn collapse_space_separators(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_run = false;
    for c in text.chars() {
        if c.is_separator_space() {
            if !in_run {
                out.push(' ');
            }
            in_run = true;
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

/// `StringUtils.split(text, ' ')`: the non-empty parts between spaces.
fn split_by_space(text: &str) -> Vec<&str> {
    text.split(' ').filter(|x| !x.is_empty()).collect()
}

/// Two words that are a known word each once a letter moves across the space: a run-on or
/// split typo for the single-word speller, not a misspelled name.
fn discard_run_on_words(word: &str, is_misspelled: &impl Fn(&str) -> bool) -> bool {
    let parts = split_by_space(word);
    let [first, second] = parts.as_slice() else {
        return false;
    };
    if is_capitalized_word(second) {
        return false;
    }
    let (last_pos, _) = first.char_indices().last().expect("parts are not empty");
    let (head, last) = first.split_at(last_pos);
    if !is_misspelled(head) && !is_misspelled(&format!("{}{}", last, second)) {
        return true;
    }
    let second_first_len = second.chars().next().map_or(0, char::len_utf8);
    let (second_first, second_rest) = second.split_at(second_first_len);
    !is_misspelled(&format!("{}{}", first, second_first)) && !is_misspelled(second_rest)
}

fn stop_searching<'a>(
    mut candidates: impl Iterator<Item = &'a str> + Clone,
    original: &str,
) -> bool {
    candidates.clone().any(|candidate| candidate == original)
        || candidates.any(|candidate| {
            candidate == candidate.to_lowercase() && title_case(candidate) == original
        })
}

/// `StringTools.convertToTitleCaseIteratingChars`.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut convert_next = true;
    for c in text.chars() {
        if c.is_separator() || c == '-' {
            convert_next = true;
            out.push(c);
        } else if convert_next {
            out.extend(c.to_uppercase());
            convert_next = false;
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

fn max_edit_distance(candidate: &str, word: &str) -> i64 {
    let correct_length = (utf16_len(word) - number_of_correct_chars(candidate, word)) as i64;
    let first_char_wrong: f32 = first_character_distances(candidate, word).iter().sum();
    if correct_length <= 7 {
        (2.0 - first_char_wrong) as i64
    } else {
        (2.0 + 0.25 * (correct_length - 7) as f64 - 0.6 * f64::from(first_char_wrong)) as i64
    }
}

fn distances_per_word(parts1: &[&str], parts2: &[&str], s1: &str, s2: &str) -> Vec<usize> {
    if parts1.len() == parts2.len() && parts1.len() > 1 {
        parts1
            .iter()
            .zip(parts2)
            .map(|(a, b)| levenshtein_distance(a, b))
            .collect()
    } else {
        vec![levenshtein_distance(s1, s2)]
    }
}

fn first_character_distances(s1: &str, s2: &str) -> Vec<f32> {
    let parts1 = split_by_space(s1);
    let parts2 = split_by_space(s2);
    if parts1.len() == parts2.len() && parts1.len() == 2 {
        parts1
            .iter()
            .zip(&parts2)
            .map(|(a, b)| {
                char_distance(
                    a.chars().next().expect("parts are not empty"),
                    b.chars().next().expect("parts are not empty"),
                )
            })
            .collect()
    } else {
        vec![0.0]
    }
}

fn char_distance(a: char, b: char) -> f32 {
    match (a, b) {
        _ if a == b => 0.0,
        ('s', 'z') | ('z', 's') | ('b', 'v') | ('v', 'b') => 0.2,
        ('i', 'y') | ('y', 'i') => 0.0,
        _ => 1.0,
    }
}

fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    if s1.replace(' ', "") == s2.replace(' ', "") {
        return 0;
    }
    let mut distance = levenshtein(s1, s2);
    let ns1 = normalize_similar_chars(s1);
    let ns2 = normalize_similar_chars(s2);
    if s1 != ns1 || s2 != ns2 {
        distance = distance.min(levenshtein(&ns1, &ns2));
    }
    let anagram = is_anagram(s1, s2);
    if distance > 1 && anagram {
        distance -= 1;
    }
    if distance > 0 && utf16_len(s1) == utf16_len(s2) && anagram {
        distance = 1;
    }
    distance
}

fn normalize_similar_chars(text: &str) -> String {
    text.replace('y', "i")
        .replace("ko", "co")
        .replace("ka", "ca")
}

fn is_anagram(s1: &str, s2: &str) -> bool {
    let mut a: Vec<u16> = s1.encode_utf16().collect();
    let mut b: Vec<u16> = s2.encode_utf16().collect();
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// The plain edit distance over UTF-16 code units, like commons-text's `LevenshteinDistance`.
fn levenshtein(s1: &str, s2: &str) -> usize {
    let a: Vec<u16> = s1.encode_utf16().collect();
    let b: Vec<u16> = s2.encode_utf16().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let above = row[j + 1];
            row[j + 1] = if ca == cb {
                diagonal
            } else {
                1 + diagonal.min(above).min(row[j])
            };
            diagonal = above;
        }
    }
    row[b.len()]
}

/// `MultitokenSpeller.numberOfCorrectChars`: the length of the words that match exactly, when
/// both have the same number of words (more than one). Splits like Java's `String.split(" ")`.
fn number_of_correct_chars(s1: &str, s2: &str) -> usize {
    fn java_split(text: &str) -> Vec<&str> {
        let mut parts: Vec<&str> = text.split(' ').collect();
        while parts.len() > 1 && parts.last() == Some(&"") {
            parts.pop();
        }
        parts
    }
    let parts1 = java_split(s1);
    let parts2 = java_split(s2);
    if parts1.len() == parts2.len() && parts1.len() > 1 {
        parts1
            .iter()
            .zip(&parts2)
            .filter(|(a, b)| a == b)
            .map(|(a, _)| utf16_len(a))
            .sum()
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn speller(lines: &[&str]) -> MultitokenSpeller {
        MultitokenSpeller::from_lines(lines.iter().map(|line| line.to_string()))
    }

    fn suggest(speller: &MultitokenSpeller, text: &str) -> Vec<String> {
        suggestions(speller, text, false, |_| true)
    }

    #[test]
    fn suggests_the_close_phrase() {
        let speller = speller(&["Rocky Mountain National Park", "Yuval Noah Harari"]);
        assert_eq!(
            suggest(&speller, "Rocki Mountains National Park"),
            vec!["Rocky Mountain National Park"]
        );
        assert_eq!(
            suggest(&speller, "Yuval Noha Hariri"),
            vec!["Yuval Noah Harari"]
        );
        assert!(suggest(&speller, "Yuval Noah Harari").is_empty());
        assert!(suggest(&speller, "Zorvath Quill").is_empty());
    }

    #[test]
    fn ignores_diacritics_and_hyphens_in_the_key() {
        let speller = speller(&["Antoine de Saint-Exupéry", "The Dark Knight Rises"]);
        assert_eq!(
            suggest(&speller, "Antoin de Saint-Exupéry"),
            vec!["Antoine de Saint-Exupéry"]
        );
        assert_eq!(
            suggest(&speller, "Thé Dákk Kínght Ríses"),
            vec!["The Dark Knight Rises"]
        );
    }

    #[test]
    fn keeps_typographic_characters_and_collapses_space_separators() {
        let speller = speller(&["Yuval Noah Harari", "O\u{2019}Brien Hall"]);
        assert_eq!(
            suggest(&speller, "Yuval\u{a0}\u{2009}Noha Harari"),
            vec!["Yuval Noah Harari"]
        );
        assert!(suggest(&speller, "O\u{2019}Brien Hall").is_empty());
        assert!(suggest(&speller, "Yuval\u{2014}Noha Harari").is_empty());
        assert!(suggest(&speller, "\u{201c}Yuval Noha\u{2026}").is_empty());
        assert!(suggest(&speller, "Yuval\u{2028}Noha Harari").is_empty());
    }

    #[test]
    fn a_phrase_the_speller_accepts_needs_a_close_match() {
        let speller = speller(&["Michael Jordan"]);
        assert_eq!(
            suggestions(&speller, "Michael Jordon", true, |_| false),
            vec!["Michael Jordan"]
        );
        assert!(suggestions(&speller, "Michel Jordon", true, |_| false).is_empty());
        assert!(suggestions(&speller, "MICHAEL JORDAN", true, |_| false).is_empty());
    }

    #[test]
    fn run_on_words_are_left_to_the_speller() {
        let speller = speller(&["gentle queue"]);
        let known = ["gent", "que", "ue", "gentq"];
        assert!(suggestions(&speller, "gentq ue", false, |w| !known.contains(&w)).is_empty());
    }

    #[test]
    fn edit_distance_counts_utf16_units_and_transpositions() {
        assert_eq!(levenshtein("kitten", "sitting"), 3);
        assert_eq!(levenshtein("zo\u{eb}", "zoe"), 1);
        assert_eq!(levenshtein_distance("harari", "hraari"), 1);
        assert_eq!(levenshtein_distance("rocky", "rocki"), 0);
    }

    #[test]
    fn title_case_follows_spaces_and_hyphens() {
        assert_eq!(title_case("saint-exupery de la"), "Saint-Exupery De La");
        assert_eq!(
            title_case("o\u{2019}brien\u{2028}hall"),
            "O\u{2019}brien\u{2028}Hall"
        );
    }
}
