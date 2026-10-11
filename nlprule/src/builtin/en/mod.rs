//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod unpaired;
mod uppercase_sentence_start;
mod word_repeat;

pub use a_vs_an::AVsAn;
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use unpaired::{UnpairedBrackets, UnpairedQuotes};
pub use uppercase_sentence_start::UppercaseSentenceStart;
pub use word_repeat::WordRepeat;
