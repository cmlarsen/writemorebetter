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

/// LanguageTool's `StringTools.uppercaseFirstChar`: uppercases the first letter or
/// decimal digit, keeping the characters before it (`(the)` becomes `(The)`,
/// `‘tis` becomes `‘Tis`). Text without one changes only its last character. A
/// one-character text is uppercased in full (`ß` becomes `SS`); otherwise only a
/// one-to-one case mapping applies, as with Java's `Character.toUpperCase`.
pub fn uppercase_first_letter_or_digit(text: &str) -> String {
    use unicode_categories::UnicodeCategories;

    if text.chars().nth(1).is_none() {
        return text.to_uppercase();
    }
    let last = text.char_indices().last().map_or(0, |(i, _)| i);
    let (pos, first) = text
        .char_indices()
        .find(|&(i, c)| c.is_letter() || c.is_number_decimal_digit() || i == last)
        .expect("text has at least two chars");
    let mut upper = first.to_uppercase();
    let upper = match (upper.next(), upper.next()) {
        (Some(c), None) => c,
        _ => first,
    };
    let mut out = String::with_capacity(text.len());
    out.push_str(&text[..pos]);
    out.push(upper);
    out.push_str(&text[pos + first.len_utf8()..]);
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uppercase_first_letter_or_digit_skips_leading_punctuation() {
        assert_eq!(
            uppercase_first_letter_or_digit("(the) latest"),
            "(The) latest"
        );
        assert_eq!(uppercase_first_letter_or_digit("ß"), "SS");
        assert_eq!(uppercase_first_letter_or_digit("ßa"), "ßa");
        assert_eq!(uppercase_first_letter_or_digit("‘tis"), "‘Tis");
        assert_eq!(uppercase_first_letter_or_digit("’tis"), "’Tis");
        assert_eq!(uppercase_first_letter_or_digit("“well”"), "“Well”");
        assert_eq!(uppercase_first_letter_or_digit("—…"), "—…");
        assert_eq!(uppercase_first_letter_or_digit("…and"), "…And");
        assert_eq!(uppercase_first_letter_or_digit("\u{2028}x"), "\u{2028}X");
        // `½` is a number but not a decimal digit, so Java skips it.
        assert_eq!(uppercase_first_letter_or_digit("\"½ frob"), "\"½ Frob");
        assert_eq!(uppercase_first_letter_or_digit("1st"), "1st");
        assert_eq!(
            uppercase_first_letter_or_digit("\u{e9}clair"),
            "\u{c9}clair"
        );
        assert_eq!(uppercase_first_letter_or_digit(""), "");
    }
}
