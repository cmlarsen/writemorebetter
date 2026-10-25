//! Flags paragraphs that begin with the same word as one of the few paragraphs before them.
//!
//! This is writemorebetter's own rule, not a LanguageTool one. LanguageTool's
//! `PARAGRAPH_REPEAT_BEGINNING_RULE` ([`super::ParagraphRepeatBeginning`]) compares each
//! paragraph only with the one right before it, so a first-person draft whose paragraphs open
//! `I…`, `Trembling, I…`, `I…`, `I…`, `Okay…`, `Before…`, `I…` gets one pair flagged. This rule
//! compares each paragraph with the [`WINDOW`] paragraphs before it, and marks every paragraph
//! of a match once, so that draft gets the first, third, fourth and seventh flagged.
//!
//! Two beginnings are compared as LanguageTool compares them (a shared leading quote or bracket
//! and a shared determiner are skipped, the word is case-sensitive; see
//! [`super::ParagraphRepeatBeginning`]), so dialogue paragraphs that open `“I…` match each other
//! but not a narration paragraph that opens `I…`. Each mark spans the paragraph's first token to
//! the end of the repeated word.

use crate::{
    document::{At, DocumentFix, DocumentMatch, LtParagraph},
    types::Span,
};
use serde::{Deserialize, Serialize};

use super::paragraph_repeat_beginning::equal_beginning;

/// The identifier of this rule.
const ID: &str = "PARAGRAPH_OPENER_REPEAT";

const MESSAGE: &str = "Nearby paragraphs begin with the same word";

/// How many paragraphs before each paragraph its beginning is compared with.
pub const WINDOW: usize = 3;

/// Flags paragraphs that begin with the same word as one of the [`WINDOW`] paragraphs before
/// them.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParagraphOpenerRepeat {}

impl ParagraphOpenerRepeat {
    pub(crate) fn new() -> Self {
        ParagraphOpenerRepeat {}
    }

    /// The rule identifier.
    pub fn id(&self) -> &str {
        ID
    }

    /// The matches in a text made of `paragraphs`, in text order.
    pub(crate) fn resolve(&self, paragraphs: &[LtParagraph]) -> Vec<DocumentMatch> {
        // The index of the repeated word in each paragraph's beginning, if it repeats.
        let mut repeated: Vec<Option<usize>> = vec![None; paragraphs.len()];
        for (i, paragraph) in paragraphs.iter().enumerate() {
            let Some(next) = &paragraph.beginning else {
                continue;
            };
            for j in i.saturating_sub(WINDOW)..i {
                let Some(last) = &paragraphs[j].beginning else {
                    continue;
                };
                if let Some(n) = equal_beginning(last, next) {
                    repeated[j] = Some(n);
                    repeated[i] = Some(n);
                }
            }
        }
        paragraphs
            .iter()
            .zip(repeated)
            .filter_map(|(paragraph, n)| {
                let tokens = &paragraph.beginning.as_ref()?.tokens;
                let (start, end) = (tokens.first()?.start, tokens.get(n?)?.end);
                (start.digest == end.digest && start.offset.char < end.offset.char)
                    .then(|| self.mark(start, end))
            })
            .collect()
    }

    fn mark(&self, start: At, end: At) -> DocumentMatch {
        DocumentMatch::new(
            start.digest,
            format!("BUILTIN/{}", ID),
            MESSAGE.to_owned(),
            Span::from_positions(start.offset.position(), end.offset.position()),
            DocumentFix::Mark,
        )
    }
}
