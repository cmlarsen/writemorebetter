//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod simple_replace;
mod unpaired;
mod uppercase_sentence_start;
mod word_coherency;
mod word_repeat;
mod wrong_word_in_context;

pub use a_vs_an::AVsAn;
pub(crate) use a_vs_an::{
    indefinite_article, lowercase_first_char_if_capitalized, Article, ArticleData,
};
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use simple_replace::{MatchMode, SimpleReplace};
pub use unpaired::{UnpairedBrackets, UnpairedQuotes};
pub use uppercase_sentence_start::UppercaseSentenceStart;
pub use word_coherency::WordCoherency;
pub use word_repeat::WordRepeat;
pub use wrong_word_in_context::{Confusion, WrongWordInContext};
