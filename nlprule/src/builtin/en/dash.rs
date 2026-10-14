//! Flags a hyphenated compound written with a dash instead of a hyphen.
//!
//! The behaviour follows the LanguageTool `EN_DASH_RULE`: a compound from the
//! compounds list (e.g. `T-shirt`) is searched for in the sentence text with
//! every hyphen replaced by the same one of `–`, `—`, ` – ` or ` — `. A match
//! must not be preceded or followed by an ASCII letter; at each start the
//! longest match wins. The suggestion turns each dash, with a space on either
//! side, back into a hyphen.
//!
//! The compounds are LanguageTool data (LGPL-2.1), copied into the build
//! directory as `builtin/compounds.txt` and serialized into the rules binary;
//! only the matching logic lives in this source file.

use crate::types::{DefaultHashSet, Position, Sentence, Span, Suggestion};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
const ID: &str = "EN_DASH_RULE";

const MESSAGE: &str = "A dash was used instead of a hyphen.";

/// The strings that may stand in for every hyphen of a compound.
const DASH_VARIANTS: [&str; 4] = [" \u{2013} ", " \u{2014} ", "\u{2013}", "\u{2014}"];

/// Flags a compound written with a dash, e.g. `"T—shirt"`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Dash {
    /// The hyphenated compounds.
    #[serde(serialize_with = "crate::types::sorted::set")]
    compounds: DefaultHashSet<String>,
    /// The length in chars of the longest dashed variant of a compound.
    max_chars: usize,
}

impl Dash {
    /// Builds the rule from the raw lines of the compounds list. As in
    /// LanguageTool, compounds that are only written joined (`+` or `?`) are
    /// skipped and a trailing `*` or `$` is removed; lines are not trimmed.
    pub(crate) fn new(lines: Vec<String>) -> Self {
        let mut rule = Dash::default();
        for line in lines {
            if line.is_empty() || line.starts_with('#') || line.ends_with(['+', '?']) {
                continue;
            }
            let compound = line.strip_suffix(['*', '$']).unwrap_or(&line);
            let hyphens = compound.matches('-').count();
            rule.max_chars = rule.max_chars.max(compound.chars().count() + 2 * hyphens);
            rule.compounds.insert(compound.to_owned());
        }
        rule
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// Whether `candidate` is a compound with each hyphen replaced by the same
    /// dash variant.
    fn is_dashed_compound(&self, candidate: &str) -> bool {
        DASH_VARIANTS.iter().any(|variant| {
            let compound = candidate.replace(variant, "-");
            self.compounds.contains(&compound) && compound.replace('-', variant) == candidate
        })
    }

    /// Checks a sentence and returns the resulting suggestions.
    pub fn check(&self, sentence: &Sentence) -> Vec<Suggestion> {
        let text = sentence.text();
        if !text.contains(['\u{2013}', '\u{2014}']) {
            return Vec::new();
        }

        let base = sentence.span().start();
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let byte_at = |i: usize| chars.get(i).map_or(text.len(), |(byte, _)| *byte);
        let is_letter = |i: usize| chars.get(i).is_some_and(|(_, c)| c.is_ascii_alphabetic());
        let position = |i: usize| {
            base + Position {
                byte: byte_at(i),
                char: i,
            }
        };

        let mut next_dash = vec![chars.len(); chars.len() + 1];
        for i in (0..chars.len()).rev() {
            next_dash[i] = if matches!(chars[i].1, '\u{2013}' | '\u{2014}') {
                i
            } else {
                next_dash[i + 1]
            };
        }

        let mut suggestions = Vec::new();
        for start in 0..chars.len() {
            if start > 0 && is_letter(start - 1) {
                continue;
            }
            let last = chars.len().min(start + self.max_chars);
            for end in (next_dash[start] + 1..=last).rev() {
                if is_letter(end) {
                    continue;
                }
                let candidate = &text[byte_at(start)..byte_at(end)];
                if self.is_dashed_compound(candidate) {
                    suggestions.push(Suggestion::new(
                        format!("BUILTIN/{}", ID),
                        MESSAGE.to_owned(),
                        Span::from_positions(position(start), position(end)),
                        vec![to_hyphens(candidate)],
                    ));
                    break;
                }
            }
        }

        suggestions
    }
}

/// Replaces each dash, with an optional space on either side, by a hyphen
/// (LanguageTool's `replaceAll(" ?[–—] ?", "-")`).
fn to_hyphens(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut result = String::with_capacity(text.len());
    let mut i = 0;
    while i < chars.len() {
        let dash_at = if matches!(chars[i], '\u{2013}' | '\u{2014}') {
            Some(i)
        } else if chars[i] == ' ' && matches!(chars.get(i + 1), Some('\u{2013}' | '\u{2014}')) {
            Some(i + 1)
        } else {
            None
        };
        match dash_at {
            Some(dash) => {
                result.push('-');
                i = dash + 1;
                if chars.get(i) == Some(&' ') {
                    i += 1;
                }
            }
            None => {
                result.push(chars[i]);
                i += 1;
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_hyphens_replaces_dashes_with_their_spaces() {
        assert_eq!(to_hyphens("T\u{2014}shirt"), "T-shirt");
        assert_eq!(to_hyphens("T \u{2013} shirt"), "T-shirt");
        assert_eq!(
            to_hyphens("mother \u{2014} in \u{2014} law"),
            "mother-in-law"
        );
    }

    #[test]
    fn new_skips_joined_only_compounds_and_strips_suffixes() {
        let rule = Dash::new(
            [
                "# comment",
                "",
                "T-shirt",
                "web-cam+",
                "e-mail?",
                "well-being*",
                "x-ray$",
            ]
            .iter()
            .map(|x| x.to_string())
            .collect(),
        );
        assert!(rule.is_dashed_compound("T\u{2014}shirt"));
        assert!(rule.is_dashed_compound("well \u{2013} being"));
        assert!(rule.is_dashed_compound("x\u{2013}ray"));
        assert!(!rule.is_dashed_compound("web\u{2014}cam"));
        assert!(!rule.is_dashed_compound("e\u{2014}mail"));
    }
}
