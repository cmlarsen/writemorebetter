//! English name phonetics. The trait is the boundary for language-specific encoders.
//!
//! This is a fresh implementation of the contextual rules described by Lawrence Philips in
//! "The Double Metaphone Search Algorithm" (C/C++ Users Journal, June 2000):
//! https://jacobfilipp.com/DrDobbs/articles/CUJ/2000/0006/philips/philips.htm
//! It emits the usual four-symbol primary and alternate keys; `0` is the /th/ sound and `X`
//! is /sh/. No rules or name lexicon from the grammar data are included here.
//!
//! `transcript` is not part of Double Metaphone: it is the whole primary code with each vowel
//! letter kept in place, so a caller can measure the vowel and syllable differences the keys drop
//! without counting spellings the keys treat as one sound (*C*/*K*, *PH*/*F*, doubled letters).

use unicode_normalization::{char::is_combining_mark, UnicodeNormalization};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Codes {
    pub primary: String,
    pub alternate: String,
    pub transcript: String,
}

impl Codes {
    pub fn overlaps(&self, other: &Self) -> bool {
        !self.primary.is_empty()
            && (self.primary == other.primary
                || self.primary == other.alternate
                || self.alternate == other.primary
                || self.alternate == other.alternate)
    }
}

pub(crate) trait NamePhonetics {
    fn codes(&self, word: &str) -> Codes;
}

pub(crate) struct EnglishNamePhonetics;

fn at(word: &[u8], i: usize, pattern: &[u8]) -> bool {
    word.get(i..i.saturating_add(pattern.len())) == Some(pattern)
}

fn vowel(byte: Option<&u8>) -> bool {
    byte.is_some_and(|c| b"AEIOUY".contains(c))
}

fn put(codes: &mut Codes, primary: &str, alternate: &str) {
    codes.transcript.push_str(primary);
    for (target, symbol) in [
        (&mut codes.primary, primary),
        (&mut codes.alternate, alternate),
    ] {
        for c in symbol.chars() {
            if target.len() < 4 {
                target.push(c);
            }
        }
    }
}

fn ascii_letters(word: &str) -> Vec<u8> {
    let mut out = Vec::new();
    for c in word.nfd().filter(|c| !is_combining_mark(*c)) {
        match c {
            'ß' | 'ẞ' => out.extend_from_slice(b"SS"),
            'Æ' | 'æ' => out.extend_from_slice(b"AE"),
            'Œ' | 'œ' => out.extend_from_slice(b"OE"),
            'Ø' | 'ø' => out.push(b'O'),
            'Ł' | 'ł' => out.push(b'L'),
            _ => out.extend(
                c.to_uppercase()
                    .filter(|c| c.is_ascii_alphabetic())
                    .map(|c| c as u8),
            ),
        }
    }
    out
}

impl NamePhonetics for EnglishNamePhonetics {
    fn codes(&self, word: &str) -> Codes {
        let word = ascii_letters(word);
        let mut result = Codes {
            primary: String::new(),
            alternate: String::new(),
            transcript: String::new(),
        };
        if word.is_empty() {
            return result;
        }
        // Philips' silent initial pairs; an initial vowel, as in *Aerin*, keeps its `A` key.
        let mut i = usize::from(
            [b"GN", b"KN", b"PN", b"WR", b"PS"]
                .iter()
                .any(|p| at(&word, 0, *p)),
        );
        if at(&word, 0, b"X") {
            put(&mut result, "S", "S");
            i = 1;
        }
        // The keys stop at four symbols; the transcript continues to the end of the word.
        while i < word.len() {
            let c = word[i];
            let prev = i.checked_sub(1).and_then(|j| word.get(j)).copied();
            let next = word.get(i + 1).copied();
            let mut step = 1;
            match c {
                b'A' | b'E' | b'I' | b'O' | b'U' | b'Y' => {
                    if i == 0 {
                        put(&mut result, "A", "A");
                        result.transcript.pop();
                    }
                    result.transcript.push(char::from(c));
                }
                b'B' => {
                    if !(prev == Some(b'P') || prev == Some(b'M') && i + 1 == word.len()) {
                        put(&mut result, "P", "P");
                    }
                    if next == Some(b'B') {
                        step = 2;
                    }
                }
                b'C' => {
                    if i == 0 && at(&word, i, b"CZ") {
                        put(&mut result, "S", "X");
                        step = 2;
                    } else if at(&word, i, b"CH") {
                        let greek = i == 0
                            && [b"CHOR", b"CHEM", b"CHIA", b"CHAE"]
                                .iter()
                                .any(|p| at(&word, i, *p))
                            || i > 0 && (at(&word, i + 2, b"OR") || at(&word, i + 2, b"YM"));
                        // Philips' Germanic and Greek CH: before T or S, or at the start or
                        // after A, O, U or E when a consonant or the word end follows
                        // (*Christopher*, *Bach*).
                        let hard = matches!(word.get(i + 2), Some(b'T' | b'S'))
                            || (i == 0 || matches!(prev, Some(b'A' | b'O' | b'U' | b'E')))
                                && matches!(
                                    word.get(i + 2),
                                    None | Some(
                                        b'L' | b'R'
                                            | b'N'
                                            | b'M'
                                            | b'B'
                                            | b'H'
                                            | b'F'
                                            | b'V'
                                            | b'W'
                                    )
                                );
                        if greek || hard || i > 0 && prev == Some(b'S') {
                            put(&mut result, "K", "K");
                        } else {
                            put(&mut result, "X", "K");
                        }
                        step = 2;
                    } else if at(&word, i, b"CIA")
                        || at(&word, i, b"CC")
                            && matches!(word.get(i + 2), Some(b'I' | b'E' | b'H'))
                    {
                        put(&mut result, "X", "X");
                        step = 3;
                    } else if [b"CK", b"CG", b"CQ"].iter().any(|p| at(&word, i, *p)) {
                        put(&mut result, "K", "K");
                        step = 2;
                    } else if matches!(next, Some(b'I' | b'E' | b'Y')) {
                        put(&mut result, "S", "S");
                        step = 2;
                    } else {
                        put(&mut result, "K", "K");
                        if next == Some(b'C') {
                            step = 2;
                        }
                    }
                }
                b'D' => {
                    if at(&word, i, b"DG") && matches!(word.get(i + 2), Some(b'I' | b'E' | b'Y')) {
                        put(&mut result, "J", "J");
                        step = 3;
                    } else {
                        put(&mut result, "T", "T");
                        if matches!(next, Some(b'D' | b'T')) {
                            step = 2;
                        }
                    }
                }
                b'F' => {
                    put(&mut result, "F", "F");
                    if next == Some(b'F') {
                        step = 2;
                    }
                }
                b'G' => {
                    if at(&word, i, b"GLI") {
                        put(&mut result, "K", "");
                    } else if at(&word, i, b"GH") {
                        if i == 0 && vowel(word.get(i + 2)) {
                            put(&mut result, "J", "J");
                        } else if i > 0
                            && !vowel(word.get(i + 2))
                            && matches!(prev, Some(b'C' | b'G' | b'L' | b'R' | b'T'))
                        {
                            put(&mut result, "F", "F");
                        }
                        step = 2;
                    } else if at(&word, i, b"GN") {
                        put(&mut result, "N", "KN");
                        step = 2;
                    } else if matches!(next, Some(b'E' | b'I' | b'Y')) && prev != Some(b'G') {
                        put(&mut result, "J", "K");
                        step = 2;
                    } else {
                        put(&mut result, "K", "K");
                        if next == Some(b'G') {
                            step = 2;
                        }
                    }
                }
                b'H' => {
                    if (i == 0 || vowel(prev.as_ref())) && vowel(word.get(i + 1)) {
                        put(&mut result, "H", "H");
                    }
                }
                b'J' => {
                    if i == 0 && at(&word, 0, b"JOSE") {
                        put(&mut result, "H", "H");
                    } else {
                        put(&mut result, "J", "J");
                    }
                    if next == Some(b'J') {
                        step = 2;
                    }
                }
                b'K' => {
                    if prev != Some(b'C') {
                        put(&mut result, "K", "K");
                    }
                    if next == Some(b'K') {
                        step = 2;
                    }
                }
                b'L' => {
                    put(&mut result, "L", "L");
                    if next == Some(b'L') {
                        step = 2;
                    }
                }
                b'M' => {
                    put(&mut result, "M", "M");
                    if next == Some(b'M') {
                        step = 2;
                    }
                }
                b'N' => {
                    put(&mut result, "N", "N");
                    if next == Some(b'N') {
                        step = 2;
                    }
                }
                b'P' => {
                    if next == Some(b'H') {
                        put(&mut result, "F", "F");
                        step = 2;
                    } else {
                        put(&mut result, "P", "P");
                        if next == Some(b'P') {
                            step = 2;
                        }
                    }
                }
                b'Q' => {
                    put(&mut result, "K", "K");
                    if next == Some(b'Q') {
                        step = 2;
                    }
                }
                b'R' => {
                    put(&mut result, "R", "R");
                    if next == Some(b'R') {
                        step = 2;
                    }
                }
                b'S' => {
                    if i == 0 && at(&word, i, b"SCH") {
                        put(&mut result, "X", "S");
                        step = 3;
                    } else if i == 0 && at(&word, i, b"SM") {
                        put(&mut result, "S", "X");
                    } else if at(&word, i, b"SH") {
                        put(&mut result, "X", "X");
                        step = 2;
                    } else if [b"SIO", b"SIA"].iter().any(|p| at(&word, i, *p)) {
                        put(&mut result, "X", "S");
                        step = 3;
                    } else if at(&word, i, b"SC")
                        && matches!(word.get(i + 2), Some(b'I' | b'E' | b'Y'))
                    {
                        put(&mut result, "S", "S");
                        step = 3;
                    } else {
                        put(&mut result, "S", "S");
                        if next == Some(b'S') {
                            step = 2;
                        }
                    }
                }
                b'T' => {
                    if at(&word, i, b"TH") {
                        if at(&word, 0, b"THOM") || at(&word, 0, b"THAM") {
                            put(&mut result, "T", "T");
                        } else {
                            put(&mut result, "0", "T");
                        }
                        step = 2;
                    } else if [b"TIA", b"TIO"].iter().any(|p| at(&word, i, *p)) {
                        put(&mut result, "X", "X");
                        step = 3;
                    } else if at(&word, i, b"TCH") {
                        step = 1;
                    } else {
                        put(&mut result, "T", "T");
                        if next == Some(b'T') {
                            step = 2;
                        }
                    }
                }
                b'V' => {
                    put(&mut result, "F", "F");
                    if next == Some(b'V') {
                        step = 2;
                    }
                }
                // W is otherwise silent: Stewart and Stuart share STRT.
                b'W' => {
                    if next == Some(b'R') {
                        put(&mut result, "R", "R");
                        step = 2;
                    } else {
                        if i == 0 && vowel(word.get(i + 1)) {
                            put(&mut result, "A", "F");
                        } else if i == 0 && next == Some(b'H') {
                            put(&mut result, "A", "A");
                        }
                        if i + 1 == word.len() && vowel(prev.as_ref())
                            || i > 0
                                && [b"EWSKI", b"EWSKY", b"OWSKI", b"OWSKY"]
                                    .iter()
                                    .any(|p| at(&word, i - 1, *p))
                            || at(&word, 0, b"SCH")
                        {
                            put(&mut result, "", "F");
                        } else if at(&word, i, b"WICZ") || at(&word, i, b"WITZ") {
                            put(&mut result, "TS", "FX");
                            step = 4;
                        }
                    }
                }
                b'X' => {
                    put(&mut result, "KS", "KS");
                    if next == Some(b'X') {
                        step = 2;
                    }
                }
                b'Z' => {
                    if next == Some(b'H') {
                        put(&mut result, "J", "J");
                        step = 2;
                    } else {
                        put(&mut result, "S", "S");
                        if next == Some(b'Z') {
                            step = 2;
                        }
                    }
                }
                _ => {}
            }
            // A rule that consumes a following vowel (soft C and G, -TIO) keeps it here.
            for &skipped in word.get(i + 1..i + step).unwrap_or_default() {
                if vowel(Some(&skipped)) {
                    result.transcript.push(char::from(skipped));
                }
            }
            i += step;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{EnglishNamePhonetics, NamePhonetics};

    // Expected four-symbol pairs are from Apache Commons Codec's published
    // DoubleMetaphone2Test reference data (derived from Stephen Woodbridge's surnames list):
    // https://commons.apache.org/proper/commons-codec/xref-test/org/apache/commons/codec/language/DoubleMetaphone2Test.html
    #[test]
    fn published_name_codes() {
        let encoder = EnglishNamePhonetics;
        let mut errors = Vec::new();
        for (name, primary, alternate) in [
            ("Adams", "ATMS", "ATMS"),
            ("Allen", "ALN", "ALN"),
            ("Benjamin", "PNJM", "PNJM"),
            ("Bishop", "PXP", "PXP"),
            ("Campbell", "KMPL", "KMPL"),
            ("Smith", "SM0", "XMT"),
            ("Schmidt", "XMT", "SMT"),
            ("Czerny", "SRN", "XRN"),
            ("Tagliaro", "TKLR", "TLR"),
            ("Jose", "HS", "HS"),
        ] {
            let actual = encoder.codes(name);
            if actual.primary != primary || actual.alternate != alternate {
                errors.push(format!(
                    "{name}: ({}, {}) != ({primary}, {alternate})",
                    actual.primary, actual.alternate
                ));
            }
        }
        assert!(errors.is_empty(), "{}", errors.join("; "));
    }

    // The transcript is the whole primary code with vowel letters in place.
    #[test]
    fn transcripts_keep_vowels() {
        let encoder = EnglishNamePhonetics;
        for (name, transcript) in [
            ("Catherine", "KA0ERINE"),
            ("Kathryn", "KA0RYN"),
            ("Cecily", "SESILY"),
            ("Haleigh", "HALEI"),
            ("Tiphanie", "TIFANIE"),
            ("Ellen", "ELEN"),
        ] {
            assert_eq!(encoder.codes(name).transcript, transcript, "{name}");
        }
    }

    // Philips codes W only at a word start or in the Polish and final-W alternates.
    #[test]
    fn w_rules() {
        let encoder = EnglishNamePhonetics;
        for (name, primary, alternate) in [
            ("Stewart", "STRT", "STRT"),
            ("Stuart", "STRT", "STRT"),
            ("Arnow", "ARN", "ARNF"),
            ("Walker", "ALKR", "FLKR"),
            ("Filipowicz", "FLPT", "FLPF"),
        ] {
            let actual = encoder.codes(name);
            assert_eq!(
                (actual.primary.as_str(), actual.alternate.as_str()),
                (primary, alternate),
                "{name}"
            );
        }
    }

    // Philips skips only GN, KN, PN, WR and PS at the start, and codes CH as K before a
    // consonant at the start (*Christopher*).
    #[test]
    fn initial_rules() {
        let encoder = EnglishNamePhonetics;
        for (name, primary, alternate) in [
            ("Aerin", "ARN", "ARN"),
            ("Erin", "ARN", "ARN"),
            ("Psmith", "SM0", "SMT"),
            ("Smith", "SM0", "XMT"),
            ("Christopher", "KRST", "KRST"),
            ("Chris", "KRS", "KRS"),
            ("Bach", "PK", "PK"),
        ] {
            let actual = encoder.codes(name);
            assert_eq!(
                (actual.primary.as_str(), actual.alternate.as_str()),
                (primary, alternate),
                "{name}"
            );
        }
    }
}
