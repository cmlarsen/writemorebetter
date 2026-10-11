//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod simple_replace;
mod unpaired;
mod uppercase_sentence_start;
mod word_repeat;

pub use a_vs_an::AVsAn;
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use simple_replace::{MatchMode, SimpleReplace};
pub use unpaired::{UnpairedBrackets, UnpairedQuotes};
pub use uppercase_sentence_start::UppercaseSentenceStart;
pub use word_repeat::WordRepeat;
