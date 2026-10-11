//! English builtin rules.

mod unpaired;
mod word_repeat;

pub use unpaired::{UnpairedBrackets, UnpairedQuotes};
pub use word_repeat::WordRepeat;
