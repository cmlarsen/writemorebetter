//! Split suggestions: a misspelling that is two words run together (`alot` -> `a lot`), as
//! Morfologik's `Speller.replaceRunOnWordCandidates` finds them, and the language's list of
//! splits not worth suggesting.
use serde::{Deserialize, Serialize};

/// The split suggestions a language drops, because one part is a prefix, suffix or stray letter
/// rather than a word of its own (`re view`, `use full`, `timezone s`). Read from the
/// `speller_split_filter` builtin list (LGPL data) and stored in the data binary.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub(crate) struct SplitFilter {
    /// Lowercase first words dropped in any case.
    first_ignore_case: Vec<String>,
    /// First words dropped as written.
    first: Vec<String>,
    /// Last words dropped as written.
    last: Vec<String>,
}

impl SplitFilter {
    /// Builds the filter from list lines `first-ignore-case <word>`, `first <word>` and
    /// `last <word>`. Other lines are ignored.
    pub fn new(lines: impl IntoIterator<Item = String>) -> Self {
        let mut filter = SplitFilter::default();
        for line in lines {
            match line.trim().split_once(' ') {
                Some(("first-ignore-case", word)) => {
                    filter.first_ignore_case.push(word.trim().to_lowercase())
                }
                Some(("first", word)) => filter.first.push(word.trim().to_owned()),
                Some(("last", word)) => filter.last.push(word.trim().to_owned()),
                _ => log::warn!("Ignoring speller split filter line {:?}", line),
            }
        }
        filter
    }

    /// Whether to keep `suggestion`. Only a suggestion of several words can be dropped.
    pub fn keeps(&self, suggestion: &str) -> bool {
        let (first, last) = match (suggestion.split_once(' '), suggestion.rsplit_once(' ')) {
            (Some((first, _)), Some((_, last))) => (first, last),
            _ => return true,
        };
        let first_lower = first.to_lowercase();
        !(self.first_ignore_case.iter().any(|x| *x == first_lower)
            || self.first.iter().any(|x| x == first)
            || self.last.iter().any(|x| x == last))
    }
}

/// Every split of `word` into two parts that `is_word` accepts, first part shortest first,
/// joined by a space. As in Morfologik, a capitalized part may also be a lowercase word (`Aduke`
/// -> `A duke`, `GreatElephant` -> `Great Elephant`).
pub(crate) fn run_ons(word: &str, is_word: impl Fn(&str) -> bool) -> Vec<String> {
    let starts_upper = |part: &str| part.chars().next().is_some_and(char::is_uppercase);
    word.char_indices()
        .skip(1)
        .filter_map(|(i, _)| {
            let (first, last) = word.split_at(i);
            let last_ok = is_word(last)
                || (crate::utils::is_title_case(last) && is_word(&last.to_lowercase()));
            let first_ok =
                is_word(first) || (starts_upper(first) && is_word(&first.to_lowercase()));
            if last_ok && first_ok {
                Some(format!("{} {}", first, last))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter() -> SplitFilter {
        SplitFilter::new(
            ["first-ignore-case re", "first i", "last s", "last ING", "bogus"]
                .iter()
                .map(|x| x.to_string()),
        )
    }

    #[test]
    fn filter_drops_splits_by_their_first_and_last_word() {
        let filter = filter();
        assert!(!filter.keeps("re view"));
        assert!(!filter.keeps("Re view"));
        assert!(!filter.keeps("i phone"));
        assert!(filter.keeps("I phone"));
        assert!(!filter.keeps("timezone s"));
        assert!(!filter.keeps("RUNN ING"));
        assert!(filter.keeps("runn ing"));
        assert!(filter.keeps("a lot"));
        assert!(filter.keeps("review"));
        assert!(filter.keeps("Zo\u{eb}\u{2019}s caf\u{e9}"));
    }

    #[test]
    fn run_ons_split_where_both_parts_are_words() {
        let words = ["a", "lot", "A", "duke", "Rhys", "and", "Zo\u{eb}", "caf\u{e9}", "great"];
        let is_word = |x: &str| words.contains(&x);
        assert_eq!(run_ons("alot", is_word), vec!["a lot"]);
        assert_eq!(run_ons("Aduke", is_word), vec!["A duke"]);
        assert_eq!(run_ons("Rhysand", is_word), vec!["Rhys and"]);
        // A capitalized part may be a lowercase word; a mixed-case one may not.
        assert_eq!(run_ons("GreatLot", is_word), vec!["Great Lot"]);
        assert_eq!(run_ons("GreatLOT", is_word), Vec::<String>::new());
        // Splits fall on character boundaries.
        assert_eq!(run_ons("Zo\u{eb}caf\u{e9}", is_word), vec!["Zo\u{eb} caf\u{e9}"]);
        assert_eq!(
            run_ons("\u{201c}\u{2014}\u{2026}\u{2028}", is_word),
            Vec::<String>::new()
        );
        assert_eq!(run_ons("a", is_word), Vec::<String>::new());
    }
}
