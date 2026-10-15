//! The phrase list behind LanguageTool's `MultitokenSpeller`: known multi-word names and phrases,
//! looked up by a normalized key (lowercase, without diacritics, hyphens read as spaces).

use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

use crate::types::DefaultHashMap;

/// Known multi-word phrases, read from the build directory's `spell/multitoken.txt`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct MultitokenSpeller {
    /// The phrases by the first character of their key, each with its spellings. Within a
    /// character the keys are in the order LanguageTool's `HashMap` iterates them, because the
    /// search stops early on some matches.
    #[serde(serialize_with = "crate::types::sorted::map")]
    pub(crate) by_first_char: DefaultHashMap<char, Vec<(String, Vec<String>)>>,
    /// The spellings by their key with the spaces removed.
    #[serde(serialize_with = "crate::types::sorted::map")]
    pub(crate) by_key_without_spaces: DefaultHashMap<String, Vec<String>>,
}

/// `MultitokenSpeller.getNormalizeKey`: lowercase, without diacritics, hyphens as spaces.
pub(crate) fn normalize_key(text: &str) -> String {
    remove_diacritics(&text.to_lowercase()).replace('-', " ")
}

/// `StringTools.removeDiacritics`: the canonical decomposition without its combining
/// diacritical marks (U+0300 to U+036F).
fn remove_diacritics(text: &str) -> String {
    text.nfd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_key_strips_case_diacritics_and_hyphens() {
        assert_eq!(
            normalize_key("Antoine de Saint-Exupéry"),
            "antoine de saint exupery"
        );
        assert_eq!(normalize_key("Thé Dákk"), "the dakk");
        assert_eq!(
            normalize_key("Zoë\u{2019}s\u{2014}Quill"),
            "zoe\u{2019}s\u{2014}quill"
        );
        assert_eq!(
            normalize_key("Ka\u{2026}Dor\u{2028}Ash"),
            "ka\u{2026}dor\u{2028}ash"
        );
    }
}
