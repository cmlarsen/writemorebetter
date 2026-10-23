//! Curated suggestions: hand-written answers for common misspellings that go before the
//! speller's candidates, or replace them, as LanguageTool's speller rules do with
//! `getAdditionalTopSuggestions` and `getOnlySuggestions`.
//!
//! The tables are language data (for English, LGPL data extracted from LanguageTool), read from
//! the `speller_suggestions` builtin list at compile time and stored in the data binary.
use serde::{Deserialize, Serialize};

use crate::{types::*, utils::regex::Regex};

/// How an [OnlyRule] builds its suggestions.
#[derive(Debug, Clone, Serialize, Deserialize)]
enum OnlySuggestions {
    /// These suggestions, as written.
    List(Vec<String>),
    /// The word with its first `from` replaced by `to` (`StringUtils.replaceOnce`).
    Replace { from: String, to: String },
}

/// A pattern whose suggestions replace the speller's for every word it matches as a whole.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlyRule {
    pattern: Regex,
    suggestions: OnlySuggestions,
}

/// A word and its curated suggestions.
type Entry = (String, Vec<String>);

/// The curated suggestions of a language.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct CuratedSuggestions {
    /// Patterns whose suggestions replace the speller's; the first that matches wins.
    only: Vec<OnlyRule>,
    /// Suggestions put before the speller's, by the word as written.
    #[serde(serialize_with = "crate::types::sorted::map")]
    top: DefaultHashMap<String, Vec<String>>,
    /// Suggestions put before the speller's, by the lowercased word. They follow those of `top`.
    #[serde(serialize_with = "crate::types::sorted::map")]
    top_ignore_case: DefaultHashMap<String, Vec<String>>,
    /// One variant's suggestions by the word as written. A word found here gets these instead of
    /// the `top`, `top_ignore_case` and `suffixes` suggestions.
    variant_top: Vec<(String, Vec<Entry>)>,
    /// `(ending, replacement)`: a word with none of the suggestions above that has this ending
    /// gets the word with the ending replaced, if that is a correct word.
    suffixes: Vec<(String, String)>,
}

/// What [CuratedSuggestions::lookup] found for a word.
pub(crate) enum Curated {
    /// Suggestions that replace the speller's.
    Only(Vec<String>),
    /// Suggestions that go before the speller's (possibly none).
    Top(Vec<String>),
}

impl CuratedSuggestions {
    /// Builds the tables from list lines (see `build/builtin-data/en/README.md`):
    /// `top`, `top-ignore-case` and `top:<variant>` lines `kind<TAB>word<TAB>suggestion...`,
    /// `suffix<TAB>ending<TAB>replacement`, and `only<TAB>regex<TAB>list<TAB>suggestion...` or
    /// `only<TAB>regex<TAB>replace<TAB>from<TAB>to`. `compile` turns an `only` pattern into a
    /// regex matching the whole word. Malformed lines are skipped with a warning.
    pub fn new(
        lines: impl IntoIterator<Item = String>,
        compile: impl Fn(&str) -> Result<Regex, String>,
    ) -> Self {
        let mut curated = CuratedSuggestions::default();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            let owned = |x: &[&str]| x.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
            match fields.as_slice() {
                ["top", word, suggestions @ ..] if !suggestions.is_empty() => {
                    curated.top.insert((*word).to_owned(), owned(suggestions));
                }
                ["top-ignore-case", word, suggestions @ ..] if !suggestions.is_empty() => {
                    // Looked up by the lowercased word, so a key with a capital never matches; kept
                    // as written to behave like LanguageTool's table.
                    curated
                        .top_ignore_case
                        .insert((*word).to_owned(), owned(suggestions));
                }
                [kind, word, suggestions @ ..]
                    if kind.starts_with("top:") && !suggestions.is_empty() =>
                {
                    let variant = &kind["top:".len()..];
                    let entry = (word.to_string(), owned(suggestions));
                    match curated.variant_top.iter_mut().find(|(v, _)| v == variant) {
                        Some((_, entries)) => entries.push(entry),
                        None => curated.variant_top.push((variant.to_owned(), vec![entry])),
                    }
                }
                ["suffix", ending, replacement] if !ending.is_empty() => curated
                    .suffixes
                    .push(((*ending).to_owned(), (*replacement).to_owned())),
                ["only", pattern, action @ ..] => {
                    let suggestions = match action {
                        ["list", list @ ..] if !list.is_empty() => {
                            OnlySuggestions::List(owned(list))
                        }
                        ["replace", from, to] if !from.is_empty() => OnlySuggestions::Replace {
                            from: (*from).to_owned(),
                            to: (*to).to_owned(),
                        },
                        _ => {
                            log::warn!("Ignoring curated suggestion line {:?}", line);
                            continue;
                        }
                    };
                    match compile(pattern) {
                        Ok(pattern) => curated.only.push(OnlyRule {
                            pattern,
                            suggestions,
                        }),
                        Err(error) => {
                            log::warn!("Ignoring curated suggestion line {:?}: {}", line, error)
                        }
                    }
                }
                _ => log::warn!("Ignoring curated suggestion line {:?}", line),
            }
        }
        curated
    }

    /// The suggestions that replace the speller's for `word`, if a pattern matches it.
    pub fn only(&self, word: &str) -> Option<Vec<String>> {
        let rule = self.only.iter().find(|rule| rule.pattern.is_match(word))?;
        Some(match &rule.suggestions {
            OnlySuggestions::List(list) => list.clone(),
            OnlySuggestions::Replace { from, to } => vec![word.replacen(from.as_str(), to, 1)],
        })
    }

    /// The curated suggestions for `word` in `variant`: those of a matching `only` pattern, or
    /// those to put before the speller's. `is_correct` decides whether a suffix rewrite is a
    /// word.
    pub fn lookup(&self, word: &str, variant: &str, is_correct: impl Fn(&str) -> bool) -> Curated {
        if let Some(only) = self.only(word) {
            return Curated::Only(only);
        }
        if let Some((_, suggestions)) = self
            .variant_top
            .iter()
            .filter(|(v, _)| v == variant)
            .flat_map(|(_, entries)| entries)
            .find(|(w, _)| w == word)
        {
            return Curated::Top(suggestions.clone());
        }
        let mut top: Vec<String> = self.top.get(word).cloned().unwrap_or_default();
        if let Some(more) = self.top_ignore_case.get(&word.to_lowercase()) {
            top.extend(more.iter().cloned());
        }
        if top.is_empty() {
            if let Some(rewrite) = self.suffixes.iter().find_map(|(ending, replacement)| {
                let stem = word.strip_suffix(ending.as_str())?;
                let rewrite = format!("{}{}", stem, replacement);
                is_correct(&rewrite).then_some(rewrite)
            }) {
                top.push(rewrite);
            }
        }
        Curated::Top(top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curated() -> CuratedSuggestions {
        CuratedSuggestions::new(
            [
                "top\tteh\tthe",
                "top\tTeh\tThe",
                "top-ignore-case\tteh\tthe\tten",
                "top-ignore-case\tqwy\tquay",
                "top-ignore-case\tQwz\tquartz",
                "top:en_US\tcolour\tcolor",
                "suffix\tys\ties",
                "only\t[Ff]reinds?\treplace\treind\triend",
                "only\tzz\tlist\tbuzz\tfuzz",
                "only\tcaf[eé]e\treplace\te\t\u{e9}",
                "bogus\tline",
                "only\tx\tlist",
            ]
            .iter()
            .map(|x| x.to_string()),
            |pattern| Ok(Regex::new(format!("^(?:{})$", pattern))),
        )
    }

    fn top(curated: &CuratedSuggestions, word: &str, variant: &str) -> Vec<String> {
        match curated.lookup(word, variant, |w| ["babies", "Babies"].contains(&w)) {
            Curated::Top(top) => top,
            Curated::Only(only) => panic!("{} got only {:?}", word, only),
        }
    }

    fn only(curated: &CuratedSuggestions, word: &str) -> Vec<String> {
        match curated.lookup(word, "en_US", |_| false) {
            Curated::Only(only) => only,
            Curated::Top(top) => panic!("{} got top {:?}", word, top),
        }
    }

    #[test]
    fn top_matches_as_written_then_lowercased() {
        let curated = curated();
        assert_eq!(top(&curated, "teh", "en_US"), vec!["the", "the", "ten"]);
        assert_eq!(top(&curated, "Teh", "en_US"), vec!["The", "the", "ten"]);
        assert_eq!(top(&curated, "TEH", "en_US"), vec!["the", "ten"]);
        assert_eq!(top(&curated, "Qwy", "en_US"), vec!["quay"]);
        assert!(top(&curated, "tehs", "en_US").is_empty());
        // A key with a capital can never equal a lowercased word.
        assert!(top(&curated, "Qwz", "en_US").is_empty());
        assert!(top(&curated, "qwz", "en_US").is_empty());
    }

    #[test]
    fn variant_entries_replace_the_shared_ones() {
        let curated = curated();
        assert_eq!(top(&curated, "colour", "en_US"), vec!["color"]);
        assert!(top(&curated, "colour", "en_GB").is_empty());
        assert!(top(&curated, "Colour", "en_US").is_empty());
    }

    #[test]
    fn suffix_rewrites_only_to_correct_words() {
        let curated = curated();
        assert_eq!(top(&curated, "babys", "en_US"), vec!["babies"]);
        assert_eq!(top(&curated, "Babys", "en_US"), vec!["Babies"]);
        assert!(top(&curated, "ladys", "en_US").is_empty());
        assert!(top(&curated, "ys", "en_GB").is_empty());
    }

    #[test]
    fn only_patterns_match_the_whole_word() {
        let curated = curated();
        assert_eq!(only(&curated, "freind"), vec!["friend"]);
        assert_eq!(only(&curated, "Freinds"), vec!["Friends"]);
        assert_eq!(only(&curated, "zz"), vec!["buzz", "fuzz"]);
        assert!(curated.only("FREIND").is_none());
        assert!(curated.only("freindly").is_none());
        assert!(curated.only("x").is_none());
    }

    #[test]
    fn only_replaces_on_char_boundaries() {
        let curated = curated();
        assert_eq!(only(&curated, "cafee"), vec!["caf\u{e9}e"]);
        assert_eq!(only(&curated, "caf\u{e9}e"), vec!["caf\u{e9}\u{e9}"]);
        // Text around a word is never part of it: punctuation does not match.
        for word in [
            "\u{201c}freind",
            "freind\u{2019}s",
            "freind\u{2014}",
            "freind\u{2026}",
            "freind\u{2028}",
        ] {
            assert!(curated.only(word).is_none(), "{:?}", word);
        }
    }

    #[test]
    fn malformed_lines_are_skipped() {
        let curated = curated();
        assert_eq!(curated.only.len(), 3);
        assert_eq!(curated.suffixes.len(), 1);
        assert!(curated.only("x").is_none());
        let empty = CuratedSuggestions::new(Vec::new(), |_| unreachable!());
        assert!(
            matches!(empty.lookup("teh", "en_US", |_| true), Curated::Top(top) if top.is_empty())
        );
    }
}
