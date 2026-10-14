//! Spells out a number in English words, e.g. `"12"` as `"twelve"`.
//!
//! This is the English synthesis behind LanguageTool's `<match postag="_spell_number_"/>`.
//! It reproduces LanguageTool 6.6's output for its English number speller: American style
//! without "and" after "hundred", "and" before a final part below one hundred, and a comma
//! before any other part after a scale word ("one million, two hundred").

const ONES: [&str; 20] = [
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];

const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];

/// The scale word for each group of three digits, starting with thousands.
const SCALES: [&str; 14] = [
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
    "sextillion",
    "septillion",
    "octillion",
    "nonillion",
    "decillion",
    "undecillion",
    "duodecillion",
    "tredecillion",
];

/// The most significant digits a number can have to be spelled out.
const MAX_DIGITS: usize = 3 * (SCALES.len() + 1);

/// Spells out `number`, a string of ASCII digits, in English words.
///
/// Leading zeros are ignored (`"007"` is "seven"). Returns `None` for anything
/// else, as LanguageTool spells nothing for it: an empty string, a sign, a
/// decimal or thousands separator, non-ASCII digits, other characters, or more
/// than 45 significant digits.
pub(crate) fn spell_number(number: &str) -> Option<String> {
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }

    let digits = number.trim_start_matches('0');
    if digits.is_empty() {
        return Some("zero".to_string());
    }
    if digits.len() > MAX_DIGITS {
        return None;
    }

    Some(spell_digits(digits))
}

/// Spells a string of ASCII digits; zero (or only zeros) is spelled as an empty string.
fn spell_digits(digits: &str) -> String {
    let digits = digits.trim_start_matches('0');
    let len = digits.len();

    match len {
        0 => String::new(),
        1 | 2 => spell_below_hundred(digit_value(digits)),
        3 => {
            let mut out = format!("{} hundred", spell_digits(&digits[..1]));
            let rest = spell_digits(&digits[1..]);
            if !rest.is_empty() {
                out.push(' ');
                out.push_str(&rest);
            }
            out
        }
        _ if len <= 5 && digits.as_bytes()[len - 3] != b'0' => format!(
            "{} thousand {}",
            spell_digits(&digits[..len - 3]),
            spell_digits(&digits[len - 3..])
        ),
        _ => {
            let groups = (len - 1) / 3;
            let (head, tail) = digits.split_at(len - 3 * groups);
            let tail_digits = tail.trim_start_matches('0');
            let separator = match tail_digits.len() {
                0 => "",
                1 | 2 => " and ",
                _ => ", ",
            };
            format!(
                "{} {}{}{}",
                spell_digits(head),
                SCALES[groups - 1],
                separator,
                spell_digits(tail_digits)
            )
        }
    }
}

/// The value of one or two ASCII digits.
fn digit_value(digits: &str) -> usize {
    digits
        .bytes()
        .fold(0, |value, b| value * 10 + usize::from(b - b'0'))
}

fn spell_below_hundred(value: usize) -> String {
    if value < ONES.len() {
        return ONES[value].to_string();
    }
    let (tens, ones) = (TENS[value / 10], ONES[value % 10]);
    if ones.is_empty() {
        tens.to_string()
    } else {
        format!("{}-{}", tens, ones)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spell(number: &str) -> String {
        spell_number(number).unwrap_or_else(|| panic!("{:?} was not spelled", number))
    }

    #[test]
    fn spells_numbers_below_one_hundred() {
        let cases = [
            ("0", "zero"),
            ("7", "seven"),
            ("10", "ten"),
            ("12", "twelve"),
            ("13", "thirteen"),
            ("14", "fourteen"),
            ("15", "fifteen"),
            ("18", "eighteen"),
            ("19", "nineteen"),
            ("20", "twenty"),
            ("21", "twenty-one"),
            ("40", "forty"),
            ("42", "forty-two"),
            ("50", "fifty"),
            ("80", "eighty"),
            ("99", "ninety-nine"),
        ];
        for (number, words) in cases {
            assert_eq!(spell(number), words, "{}", number);
        }
    }

    #[test]
    fn spells_hundreds_and_thousands_like_languagetool() {
        let cases = [
            ("100", "one hundred"),
            ("101", "one hundred one"),
            ("110", "one hundred ten"),
            ("999", "nine hundred ninety-nine"),
            ("1000", "one thousand"),
            ("1001", "one thousand and one"),
            ("1005", "one thousand and five"),
            ("1010", "one thousand and ten"),
            ("1100", "one thousand one hundred"),
            ("1243", "one thousand two hundred forty-three"),
            ("10200", "ten thousand two hundred"),
            ("12345", "twelve thousand three hundred forty-five"),
            ("100000", "one hundred thousand"),
            ("100005", "one hundred thousand and five"),
            ("100200", "one hundred thousand, two hundred"),
            (
                "999999",
                "nine hundred ninety-nine thousand, nine hundred ninety-nine",
            ),
        ];
        for (number, words) in cases {
            assert_eq!(spell(number), words, "{}", number);
        }
    }

    #[test]
    fn spells_large_numbers_like_languagetool() {
        let cases = [
            ("1000000", "one million"),
            ("1000001", "one million and one"),
            ("1000099", "one million and ninety-nine"),
            ("1000100", "one million, one hundred"),
            ("1001000", "one million, one thousand"),
            ("1010000", "one million, ten thousand"),
            (
                "1234567",
                "one million, two hundred thirty-four thousand, five hundred sixty-seven",
            ),
            ("100000001", "one hundred million and one"),
            ("1000000000", "one billion"),
            ("1000100000", "one billion, one hundred thousand"),
            ("1000000000000", "one trillion"),
            (
                "123456789012345678901234567890123456789012345",
                "one hundred twenty-three tredecillion, four hundred fifty-six duodecillion, \
                 seven hundred eighty-nine undecillion, twelve decillion, three hundred forty-five \
                 nonillion, six hundred seventy-eight octillion, nine hundred one septillion, two \
                 hundred thirty-four sextillion, five hundred sixty-seven quintillion, eight \
                 hundred ninety quadrillion, one hundred twenty-three trillion, four hundred \
                 fifty-six billion, seven hundred eighty-nine million, twelve thousand three \
                 hundred forty-five",
            ),
            (
                "100000000000000000000000000000000000000000001",
                "one hundred tredecillion and one",
            ),
        ];
        for (number, words) in cases {
            assert_eq!(spell(number), words, "{}", number);
        }
    }

    #[test]
    fn spells_the_largest_number_languagetool_spells() {
        let words = spell(&"9".repeat(45));
        assert!(words.starts_with("nine hundred ninety-nine tredecillion, "));
        assert!(words.ends_with(" thousand, nine hundred ninety-nine"));
        assert_eq!(spell_number(&format!("1{}", "0".repeat(45))), None);
    }

    #[test]
    fn ignores_leading_zeros() {
        assert_eq!(spell("00"), "zero");
        assert_eq!(spell("0000000"), "zero");
        assert_eq!(spell("01"), "one");
        assert_eq!(spell("007"), "seven");
        assert_eq!(spell("0100"), "one hundred");
        assert_eq!(spell("010000000"), "ten million");
        assert!(spell(&format!("00{}", "9".repeat(45))).starts_with("nine hundred"));
    }

    #[test]
    fn rejects_anything_but_ascii_digits() {
        for number in [
            "",
            "1.5",
            "1,000",
            "-5",
            "\u{2212}5",
            "1st",
            "12th",
            "3a",
            "1e3",
            "12\u{2019}",
            "\u{2014}12",
            "12\u{2026}",
            "12\u{2028}",
            "\u{201c}12\u{201d}",
            "\u{0663}",
            "\u{ff11}\u{ff12}",
            "\u{bd}",
        ] {
            assert_eq!(spell_number(number), None, "{:?}", number);
        }
    }
}
