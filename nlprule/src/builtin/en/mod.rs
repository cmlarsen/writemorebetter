//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod word_coherency;
mod word_repeat;

pub use a_vs_an::AVsAn;
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use word_coherency::WordCoherency;
pub use word_repeat::WordRepeat;
