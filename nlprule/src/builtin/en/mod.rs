//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod simple_replace;
mod word_repeat;

pub use a_vs_an::AVsAn;
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use simple_replace::{MatchMode, SimpleReplace};
pub use word_repeat::WordRepeat;
