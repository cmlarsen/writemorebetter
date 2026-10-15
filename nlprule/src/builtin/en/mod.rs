//! English builtin rules.

mod a_vs_an;
mod comma_parenthesis_whitespace;
mod compounds;
mod dash;
mod double_punctuation;
mod empty_line;
mod long_paragraph;
mod long_sentence;
mod multiple_whitespace;
mod sentence_whitespace;
mod simple_replace;
mod specific_case;
mod spell_number;
mod unpaired;
mod uppercase_sentence_start;
mod word_coherency;
mod word_repeat;
mod word_repeat_beginning;
mod wrong_word_in_context;

pub use a_vs_an::AVsAn;
pub(crate) use a_vs_an::{
    indefinite_article, lowercase_first_char_if_capitalized, Article, ArticleData,
};
pub use comma_parenthesis_whitespace::CommaParenthesisWhitespace;
pub use compounds::Compounds;
#[cfg(feature = "compile")]
pub(crate) use compounds::{CompoundData, PatternToken};
pub use dash::Dash;
pub use double_punctuation::DoublePunctuation;
pub use empty_line::EmptyLine;
pub use long_paragraph::LongParagraph;
pub use long_sentence::LongSentence;
pub use multiple_whitespace::MultipleWhitespace;
pub use sentence_whitespace::SentenceWhitespace;
pub use simple_replace::{MatchMode, SimpleReplace};
pub use specific_case::SpecificCase;
pub(crate) use spell_number::spell_number;
pub use unpaired::{UnpairedBrackets, UnpairedQuotes};
pub use uppercase_sentence_start::UppercaseSentenceStart;
pub use word_coherency::WordCoherency;
pub use word_repeat::WordRepeat;
pub use word_repeat_beginning::WordRepeatBeginning;
pub use wrong_word_in_context::{Confusion, WrongWordInContext};
