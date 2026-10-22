//! Flags every apostrophe token once a text mixes typewriter (`'`) and typographic (`’`)
//! apostrophes.
//!
//! The behaviour follows LanguageTool 6.6's `ConsistentApostrophesRule` (`EN_CONSISTENT_APOS`), a
//! text-level rule that is off by default (`setDefaultTempOff`). LanguageTool's English tagger
//! replaces `’` with `'` in every token longer than one UTF-16 unit and remembers that it did; the
//! rule then looks at every token that contains `'`:
//!
//! - a token whose `'` was typed as such is typewriter style, suggestion: each `'` as `’`;
//! - a token whose `’` was normalized is typographic, suggestion: the normalized token.
//!
//! A lone `’` (a closing single quote) is one UTF-16 unit, is not normalized and does not count,
//! while a lone `'` does count as typewriter style. If the text has tokens of both styles, every
//! such token is reported; otherwise none is.
//!
//! The rule needs the whole text before it can report anything, so it works on
//! [`ParagraphDigest`]s: each paragraph records the char spans of its apostrophe tokens and their
//! style while it is checked, and [`ConsistentApostrophes::resolve`] decides from all of them.

use crate::document::{DocumentFix, DocumentMatch, ParagraphDigest};
use serde::{Deserialize, Serialize};

/// The identifier of the LanguageTool rule this builtin reimplements.
pub(crate) const ID: &str = "EN_CONSISTENT_APOS";

const TYPEWRITER_MESSAGE: &str = "You used a typewriter-style apostrophe here, but a typographic \
                                  apostrophe elsewhere in this text. Both are correct, but \
                                  consider using the same type everywhere in your text.";
const TYPOGRAPHIC_MESSAGE: &str = "You used a typographic apostrophe here, but a typewriter-style \
                                   apostrophe elsewhere in this text. Both are correct, but \
                                   consider using the same type everywhere in your text.";

/// Flags apostrophe tokens of both styles in a text that mixes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ConsistentApostrophes;

impl ConsistentApostrophes {
    pub(crate) fn new() -> Self {
        ConsistentApostrophes
    }

    /// The LanguageTool rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// The matches for a text made of `digests`, in text order: none unless the text has
    /// apostrophe tokens of both styles.
    pub(crate) fn resolve(&self, digests: &[ParagraphDigest]) -> Vec<DocumentMatch> {
        let mut typewriter = false;
        let mut typographic = false;
        for apostrophe in digests.iter().flat_map(|digest| digest.apostrophes()) {
            if apostrophe.typographic {
                typographic = true;
            } else {
                typewriter = true;
            }
        }
        if !(typewriter && typographic) {
            return Vec::new();
        }

        digests
            .iter()
            .enumerate()
            .flat_map(|(paragraph, digest)| {
                digest.apostrophes().iter().map(move |apostrophe| {
                    let (message, to_typographic) = if apostrophe.typographic {
                        (TYPOGRAPHIC_MESSAGE, false)
                    } else {
                        (TYPEWRITER_MESSAGE, true)
                    };
                    DocumentMatch::new(
                        paragraph,
                        format!("BUILTIN/{}", ID),
                        message.to_owned(),
                        apostrophe.span(),
                        DocumentFix::SwapApostrophes {
                            to_typographic,
                            first: apostrophe.first,
                        },
                    )
                })
            })
            .collect()
    }
}

/// The style of a token's apostrophes as LanguageTool's English tagger and this rule see them:
/// `Some(true)` for a typographic token, `Some(false)` for a typewriter one, `None` for a token
/// the rule ignores. `token` is the token's original text.
pub(crate) fn apostrophe_style(token: &str) -> Option<bool> {
    // Java's `String.length() > 1` counts UTF-16 units; `’` is one.
    let longer_than_one = token.encode_utf16().nth(1).is_some();
    if longer_than_one && token.contains('\u{2019}') {
        Some(true)
    } else if token.contains('\'') {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles() {
        assert_eq!(apostrophe_style("'ll"), Some(false));
        assert_eq!(apostrophe_style("n\u{2019}t"), Some(true));
        assert_eq!(apostrophe_style("'"), Some(false));
        // A lone closing single quote is not normalized by LanguageTool's tagger.
        assert_eq!(apostrophe_style("\u{2019}"), None);
        // Both kinds in one token: the tagger normalizes it, so it is typographic.
        assert_eq!(apostrophe_style("rock'n\u{2019}roll"), Some(true));
        assert_eq!(apostrophe_style("\u{2019}\u{2019}"), Some(true));
        assert_eq!(apostrophe_style("\u{2018}"), None);
        assert_eq!(apostrophe_style("\u{2014}"), None);
        assert_eq!(apostrophe_style("\u{2026}"), None);
        assert_eq!(apostrophe_style("\u{2028}"), None);
        assert_eq!(apostrophe_style("word"), None);
    }
}
