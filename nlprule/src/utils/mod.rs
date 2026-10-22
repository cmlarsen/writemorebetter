use lazy_static::lazy_static;

pub mod parallelism;
pub mod regex;

use regex::Regex;

// see https://stackoverflow.com/questions/38406793/why-is-capitalizing-the-first-letter-of-a-string-so-convoluted-in-rust
pub fn apply_to_first<F>(string: &str, func: F) -> String
where
    F: Fn(char) -> String,
{
    let mut c = string.chars();
    match c.next() {
        None => String::new(),
        Some(first) => func(first) + c.as_str(),
    }
}

pub fn is_title_case(string: &str) -> bool {
    let mut char_case = string.chars().map(|x| x.is_uppercase());

    char_case.next().unwrap_or(false) && !char_case.any(|x| x)
}

pub fn is_uppercase(string: &str) -> bool {
    !string.chars().any(|x| x.is_lowercase())
}

/// The code points of the Latin script (`Script=Latin` in Unicode 16.0's `Scripts.txt`), as
/// sorted, non-overlapping inclusive ranges.
const LATIN_SCRIPT: &[(char, char)] = &[
    ('\u{0041}', '\u{005A}'),
    ('\u{0061}', '\u{007A}'),
    ('\u{00AA}', '\u{00AA}'),
    ('\u{00BA}', '\u{00BA}'),
    ('\u{00C0}', '\u{00D6}'),
    ('\u{00D8}', '\u{00F6}'),
    ('\u{00F8}', '\u{02B8}'),
    ('\u{02E0}', '\u{02E4}'),
    ('\u{1D00}', '\u{1D25}'),
    ('\u{1D2C}', '\u{1D5C}'),
    ('\u{1D62}', '\u{1D65}'),
    ('\u{1D6B}', '\u{1D77}'),
    ('\u{1D79}', '\u{1DBE}'),
    ('\u{1E00}', '\u{1EFF}'),
    ('\u{2071}', '\u{2071}'),
    ('\u{207F}', '\u{207F}'),
    ('\u{2090}', '\u{209C}'),
    ('\u{212A}', '\u{212B}'),
    ('\u{2132}', '\u{2132}'),
    ('\u{214E}', '\u{214E}'),
    ('\u{2160}', '\u{2188}'),
    ('\u{2C60}', '\u{2C7F}'),
    ('\u{A722}', '\u{A787}'),
    ('\u{A78B}', '\u{A7CD}'),
    ('\u{A7D0}', '\u{A7D1}'),
    ('\u{A7D3}', '\u{A7D3}'),
    ('\u{A7D5}', '\u{A7DC}'),
    ('\u{A7F2}', '\u{A7FF}'),
    ('\u{AB30}', '\u{AB5A}'),
    ('\u{AB5C}', '\u{AB64}'),
    ('\u{AB66}', '\u{AB69}'),
    ('\u{FB00}', '\u{FB06}'),
    ('\u{FF21}', '\u{FF3A}'),
    ('\u{FF41}', '\u{FF5A}'),
    ('\u{10780}', '\u{10785}'),
    ('\u{10787}', '\u{107B0}'),
    ('\u{107B2}', '\u{107BA}'),
    ('\u{1DF00}', '\u{1DF1E}'),
    ('\u{1DF25}', '\u{1DF2A}'),
];

/// Whether `string` contains a character of the Latin script (`\p{script=Latin}`).
pub fn has_latin_script(string: &str) -> bool {
    string
        .chars()
        .any(|c| crate::tokenizer::in_ranges(LATIN_SCRIPT, c))
}

// remove duplicate whitespaces
pub fn normalize_whitespace(string: &str) -> String {
    lazy_static! {
        static ref REGEX: Regex = Regex::new(r"(\s)\s+".into());
    }

    REGEX.replace_all(string, "$1")
}

#[inline]
pub fn splitting_chars() -> &'static str {
    r##"¡¿«»'’`´‘],.:;!?/\()<=>„“”"+#…*"##
}

#[inline]
pub fn no_space_chars() -> &'static str {
    r##","##
}

pub fn fix_nospace_chars(text: &str) -> String {
    text.char_indices()
        .filter(|(i, c)| {
            if c.is_whitespace() {
                !no_space_chars()
                    .chars()
                    .any(|nospace_c| text[(i + c.len_utf8())..].starts_with(nospace_c))
            } else {
                true
            }
        })
        .map(|x| x.1)
        .collect()
}
