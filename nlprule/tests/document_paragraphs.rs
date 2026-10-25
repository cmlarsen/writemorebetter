//! The paragraph rules among the whole-text builtins: `READABILITY_RULE_SIMPLE`,
//! `READABILITY_RULE_DIFFICULT` and `PARAGRAPH_REPEAT_BEGINNING_RULE`, checked against
//! LanguageTool 6.6 and through paragraph digests.

use lazy_static::lazy_static;
use nlprule::{
    builtin::Builtin,
    document::merge_document_suggestions,
    profile::Profile,
    types::{Position, Suggestion},
    ParagraphDigest, Rules, Tokenizer,
};
use std::ops::Range;

const TOKENIZER_PATH: &str = "../storage/en_tokenizer.bin";
const RULES_PATH: &str = "../storage/en_rules.bin";
const SIMPLE: &str = "READABILITY_RULE_SIMPLE";
const DIFFICULT: &str = "READABILITY_RULE_DIFFICULT";
const REPEAT: &str = "PARAGRAPH_REPEAT_BEGINNING_RULE";
const IDS: [&str; 3] = [SIMPLE, DIFFICULT, REPEAT];
/// writemorebetter's own paragraph rule, which no LanguageTool probe covers.
const OPENER: &str = "PARAGRAPH_OPENER_REPEAT";

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new(TOKENIZER_PATH).unwrap();
    static ref RULES: Rules = Rules::new(RULES_PATH).unwrap();
    static ref ON: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        let selectors: Vec<String> = IDS.iter().map(|x| format!("BUILTIN/{}", x)).collect();
        assert_eq!(rules.enable_rules(&selectors).unwrap(), 3);
        rules
    };
}

fn is_paragraph_rule(suggestion: &Suggestion) -> bool {
    IDS.iter()
        .any(|id| suggestion.source() == format!("BUILTIN/{}", id))
}

/// `(rule id, char span)` of each paragraph-rule suggestion `rules` makes on `text`, checking
/// that byte and char spans agree and that there is no replacement.
fn flags_with(rules: &Rules, text: &str) -> Vec<(String, Range<usize>)> {
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(is_paragraph_rule)
        .map(|s| {
            let flagged: String = text
                .chars()
                .skip(s.span().char().start)
                .take(s.span().char().len())
                .collect();
            assert_eq!(&text[s.span().byte().clone()], flagged, "{:?}", text);
            assert!(s.replacements().is_empty());
            (
                s.source().trim_start_matches("BUILTIN/").to_owned(),
                s.span().char().clone(),
            )
        })
        .collect()
}

fn flags(text: &str) -> Vec<(String, Range<usize>)> {
    flags_with(&ON, text)
}

/// The flags of the rules whose id starts with `prefix`.
fn flags_of(prefix: &str, text: &str) -> Vec<(String, Range<usize>)> {
    flags(text)
        .into_iter()
        .filter(|(id, _)| id.starts_with(prefix))
        .collect()
}

fn expected(matches: &[(&str, Range<usize>)]) -> Vec<(String, Range<usize>)> {
    matches
        .iter()
        .map(|(id, range)| ((*id).to_owned(), range.clone()))
        .collect()
}

/// Probes run through LanguageTool 6.6 (`-eo -e READABILITY_RULE_SIMPLE,READABILITY_RULE_DIFFICULT`),
/// with its matches as char spans.
/// A probe text and LanguageTool's matches in it, as rule ids and char spans.
type Probe = (&'static str, &'static [(&'static str, Range<usize>)]);

const READABILITY_PROBES: &[Probe] = &[
    ("The cat sat on the mat. The dog ran to the park. We had fun.", &[]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 62..69)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.", &[("READABILITY_RULE_DIFFICULT", 0..38)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.\n\nInterdisciplinary investigations demonstrate considerable methodological heterogeneity, particularly concerning epidemiological classification.", &[("READABILITY_RULE_DIFFICULT", 0..38), ("READABILITY_RULE_DIFFICULT", 182..226)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\nInstitutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.", &[("READABILITY_RULE_DIFFICULT", 62..100)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\nInstitutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_DIFFICULT", 62..100)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.\n\nThe cat sat on the mat. The dog ran to the park. We had fun.\n\nInterdisciplinary investigations demonstrate considerable methodological heterogeneity, particularly concerning epidemiological classification.", &[("READABILITY_RULE_DIFFICULT", 0..38), ("READABILITY_RULE_DIFFICULT", 244..288)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\nI saw a big red bus. It was fast. We got on it and sat down.", &[]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\u{2028}I saw a big red bus. It was fast. We got on it and sat down.\n\nThe cat sat on the mat. The dog ran to the park. We had fun.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 123..134)]),
    ("\u{201c}The cat sat on the mat,\u{201d} she said\u{2014}softly. \u{201c}The dog ran\u{2026} We had fun.\u{201d}\n\n\u{2018}I\u{2019}m fine,\u{2019} he said. It\u{2019}s late. We\u{2019}ll go now and sit.", &[("READABILITY_RULE_SIMPLE", 0..8), ("READABILITY_RULE_SIMPLE", 71..75)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.  \n\nInterdisciplinary investigations demonstrate considerable methodological heterogeneity, particularly concerning epidemiological classification.", &[("READABILITY_RULE_DIFFICULT", 0..38), ("READABILITY_RULE_DIFFICULT", 184..228)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.\n\n\n\nInterdisciplinary investigations demonstrate considerable methodological heterogeneity, particularly concerning epidemiological classification.\n", &[("READABILITY_RULE_DIFFICULT", 0..38), ("READABILITY_RULE_DIFFICULT", 184..228)]),
    ("Short.\n\nThe cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 8..19), ("READABILITY_RULE_SIMPLE", 70..77)]),
    // LanguageTool 6.6 throws on this one.
    ("Yes. No. Yes. No. Yes. No. Yes. No. Yes. No. Yes.\n\nYes. No. Yes. No. Yes. No. Yes. No. Yes. No. Yes.", &[]),
    ("She walked to the store and bought some bread for her family before the evening came.", &[]),
    ("She walked to the store and bought some bread for her family before the evening came.\n\nShe walked to the store and bought some bread for her family before the evening came.", &[("READABILITY_RULE_SIMPLE", 0..13), ("READABILITY_RULE_SIMPLE", 87..100)]),
    ("  The cat sat on the mat. The dog ran to the park. We had fun.\n\n  I saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 2..13), ("READABILITY_RULE_SIMPLE", 66..73)]),
    ("Go. The cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..15), ("READABILITY_RULE_SIMPLE", 66..73)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.\n", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 62..69)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down. \n\n", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 62..69)]),
    ("Cat.  Dog went home. The cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..19), ("READABILITY_RULE_SIMPLE", 83..90)]),
    ("\n\nThe cat sat on the mat. The dog ran to the park. We had fun.", &[("READABILITY_RULE_SIMPLE", 2..13)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\n", &[]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.  \n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 64..71)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun. \r\n\r\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 65..72)]),
    ("She walked slowly to the market and bought some bread. Then she went back home.\n\nShe walked slowly to the market and bought some bread. Then she went back home.", &[("READABILITY_RULE_SIMPLE", 0..17), ("READABILITY_RULE_SIMPLE", 81..98)]),
    ("She walked slowly to the market and bought some bread. Then she went back home.  \n\nShe walked slowly to the market and bought some bread. Then she went back home.  \n\n", &[("READABILITY_RULE_SIMPLE", 0..17), ("READABILITY_RULE_SIMPLE", 83..100)]),
    ("Institutional considerations regarding international collaboration necessitate comprehensive organizational restructuring, notwithstanding substantial administrative complications.\n\nThe cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.\n\nThe cat sat on the mat. The dog ran to the park. We had fun.", &[("READABILITY_RULE_DIFFICULT", 0..38)]),
    ("\u{201c}The cat sat on the mat,\u{201d} she said\u{2014}softly\u{2026} \u{201c}We had fun.\u{201d}\u{2028}\u{2018}It\u{2019}s late,\u{2019} he said. We\u{2019}ll go now.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..8), ("READABILITY_RULE_SIMPLE", 94..101)]),
    ("The cat sat on\nthe mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 62..69)]),
    ("Go.\nThe cat sat on the mat. The dog ran to the park. We had fun.\n\nI saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..15), ("READABILITY_RULE_SIMPLE", 66..73)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\n\tI saw a big red bus. It was fast. We got on it and sat down.", &[]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\n\u{a0}I saw a big red bus. It was fast. We got on it and sat down.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 63..70)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\r\n\r\nI saw a big red bus. It was fast. We got on it and sat down.\r\n", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 64..71)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\n\n\nI saw a big red bus. It was fast. We got on it and sat down.\n\n\n", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 64..71)]),
    // Whitespace-only sentences of a whole text: a no-break space line makes a paragraph of its
    // own, so the text has two; a trailing one before a blank line too.
    ("The cat sat on the mat. The dog ran to the park. We had fun.\n\n\u{a0}\n\n", &[("READABILITY_RULE_SIMPLE", 0..11)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\u{a0}\n\n", &[("READABILITY_RULE_SIMPLE", 0..11)]),
    ("The cat sat on the mat. The dog ran to the park. We had fun.\u{a0}\n\nThe cat sat on the mat. The dog ran to the park. We had fun.", &[("READABILITY_RULE_SIMPLE", 0..11), ("READABILITY_RULE_SIMPLE", 63..74)]),
    ("It\u{2019}s late.\u{2014}She ran.She ran.\n\r\n\u{2014}She ran.\u{2026}", &[]),
];

/// Probes run through LanguageTool 6.6 (`-eo -e PARAGRAPH_REPEAT_BEGINNING_RULE`), with its
/// matches as char spans.
const REPEAT_PROBES: &[Probe] = &[
    (
        "The cat sat.\n\nThe cat ran.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..7),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 14..21),
        ],
    ),
    (
        "\u{201c}Hello,\u{201d} she said.\n\n\u{201c}Hello,\u{201d} he said.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..6),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 20..26),
        ],
    ),
    ("She went.\nShe came.", &[]),
    (
        "She went.\u{2028}She came.\n\nShe left.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 21..24),
        ],
    ),
    ("\u{2014}She went.\n\n\u{2014}She came.", &[]),
    ("\u{2026}and so.\n\n\u{2026}and so.", &[]),
    (
        "She\u{2019}d go.\n\nShe\u{2019}d stay.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..14),
        ],
    ),
    (
        "It's here.\n\nIt\u{2019}s there.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..2),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..14),
        ],
    ),
    (
        "Then she left.\n\n\n\nThen he came.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 18..22),
        ],
    ),
    ("The end.  \n\nThe start.", &[]),
    ("A dog.\n\nA cat.", &[]),
    ("The dog.\n\nThey ran.", &[]),
    ("the dog.\n\nThe dog.", &[]),
    (
        "She ran.\n\nShe hid.\n\nShe slept.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 10..13),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 20..23),
        ],
    ),
    (
        "She ran. He hid.\nShe slept.\n\nShe woke.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 29..32),
        ],
    ),
    (
        "  She ran.\n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 2..5),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..17),
        ],
    ),
    (
        "She ran.\n\n  She hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..13),
        ],
    ),
    (
        "'She ran.'\n\n'She hid.'",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..16),
        ],
    ),
    (
        "\u{2018}She ran.\u{2019}\n\n\u{2018}She hid.\u{2019}",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..16),
        ],
    ),
    (
        "(She ran.)\n\n(She hid.)",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..16),
        ],
    ),
    (
        "\u{201c}The dog ran.\u{201d}\n\n\u{201c}The dog hid.\u{201d}",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..8),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 16..24),
        ],
    ),
    ("\u{201c}\u{201d}\n\n\u{201c}\u{201d}", &[]),
    (
        "The\n\nThe",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 5..8),
        ],
    ),
    (
        "\u{201c}The\n\n\u{201c}The",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 6..10),
        ],
    ),
    (
        "She ran\u{2026}\n\nShe hid\u{2014}",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 10..13),
        ],
    ),
    ("Sheila ran.\n\nShe hid.", &[]),
    (
        "\u{c9}mile ran.\n\n\u{c9}mile hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..5),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..17),
        ],
    ),
    ("1999 was good.\n\n1999 was bad.", &[]),
    (
        "She ran.\r\n\r\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..15),
        ],
    ),
    (
        "She ran.\n\nShe hid.\n",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 10..13),
        ],
    ),
    (
        "She ran.\n\nShe hid.\n\n",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 10..13),
        ],
    ),
    (
        "She ran.\n\n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..14),
        ],
    ),
    (
        "Her dog ran.\n\nHer dog hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 14..17),
        ],
    ),
    (
        "This dog ran.\n\nThis dog hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..8),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 15..23),
        ],
    ),
    ("This dog ran.\n\nThis cat hid.", &[]),
    (
        "An owl.\n\nAn owl.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..6),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 9..15),
        ],
    ),
    (
        "\u{201c}She ran.\n\n\u{201c}She hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..15),
        ],
    ),
    (
        "Mr. Smith ran.\n\nMr. Smith hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..2),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 16..18),
        ],
    ),
    (
        "She ran. She hid.\n\nShe slept. She woke.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 19..22),
        ],
    ),
    // LanguageTool 6.6 throws on this one, as the second match would end before it starts; the
    // builtin reports the first match only.
    (
        "She ran.  \n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..15),
        ],
    ),
    ("She ran. \r\n\r\nShe hid.", &[]),
    (
        "She ran.\n\n\n \n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 14..17),
        ],
    ),
    (
        "\n\nShe ran.\n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 2..5),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..15),
        ],
    ),
    ("She ran.\n \nShe hid.", &[]),
    // LanguageTool 6.6 throws on this one.
    (
        "She ran.\n\n     She hid.",
        &[("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3)],
    ),
    (
        "  She ran.\n\nShe hid.\n\nShe slept.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 2..5),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..17),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 22..25),
        ],
    ),
    ("\u{2019}Tis true.\n\n'Tis false.", &[]),
    (
        "She\u{2014}ran.\n\nShe\u{2026} hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 10..13),
        ],
    ),
    (
        "She ran.\u{2028}She hid.\n\nShe slept.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 19..22),
        ],
    ),
    (
        "\u{201c}She ran,\u{201d} he said.\n\n\u{201c}She hid,\u{201d} he said.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 21..25),
        ],
    ),
    (
        "The\u{a0}cat ran.\n\nThe cat hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..7),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 14..21),
        ],
    ),
    (
        "Next time.\n\nNext time.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..9),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 12..21),
        ],
    ),
    (
        "She ran.\n\n\u{a0}She hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..13),
        ],
    ),
    ("She ran.\n\n\tShe hid.", &[]),
    (
        "She ran. \r\nShe hid.\r\n\r\nShe slept.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..12),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 23..28),
        ],
    ),
    (
        "\n  She ran.\n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 3..6),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 13..19),
        ],
    ),
    (
        "She ran.\n\n\u{2028}She hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..13),
        ],
    ),
    (
        "it\u{2019}s\u{2026}\n\nit\u{2019}s\u{2026} fine.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..2),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 7..9),
        ],
    ),
    (
        "\u{201c}She went.\u{201d}\n\n\u{201c}She went.\u{201d}",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..4),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 13..17),
        ],
    ),
    ("She hid.\n\n\n \r\nShe hid.", &[]),
    // Whitespace-only sentences of a whole text: a line of other whitespace than ASCII is a
    // paragraph of its own; a CRLF after a bare LF, or a line break after a no-break space,
    // starts the next sentence.
    ("She ran.\n\n\u{a0}\n\nShe hid.", &[]),
    ("She ran.\n\n\u{2028}\n\nShe hid.", &[]),
    ("She ran.\n\n\u{3000}\n\nShe hid.", &[]),
    ("She ran.\n\n\u{202f}\n\nShe hid.", &[]),
    (
        "She ran.\n\n \n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 13..16),
        ],
    ),
    (
        "She ran.\u{a0}\n\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..13),
        ],
    ),
    ("She ran.\n\r\n\r\nShe hid.", &[]),
    (
        "She ran.\n\r\nShe hid.",
        &[
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 0..3),
            ("PARAGRAPH_REPEAT_BEGINNING_RULE", 11..12),
        ],
    ),
    ("She ran.\u{a0}\r\n\r\nShe hid.", &[]),
    ("She ran.\u{a0}\r\n\nShe hid.", &[]),
];

#[test]
fn off_by_default() {
    for id in IDS {
        let rule = RULES.builtins().iter().find(|x| x.id() == id).unwrap();
        assert!(!rule.enabled(), "{}", id);
    }
    let text = "She ran.\n\nShe hid.";
    assert!(RULES
        .suggest(text, &TOKENIZER)
        .iter()
        .all(|s| !is_paragraph_rule(s)));
    assert_eq!(flags(text).len(), 2);
    // Not in the fiction profile either, as in LanguageTool.
    let mut fiction = Rules::new(RULES_PATH).unwrap();
    assert!(fiction.apply_profile(&Profile::fiction()).is_empty());
    assert!(fiction
        .builtins()
        .iter()
        .filter(|x| IDS.contains(&x.id()))
        .all(|x| !x.enabled()));
}

#[test]
fn readability_matches_languagetool() {
    for (text, matches) in READABILITY_PROBES {
        assert_eq!(
            flags_of("READABILITY", text),
            expected(matches),
            "{:?}",
            text
        );
    }
}

#[test]
fn paragraph_repeat_matches_languagetool() {
    for (text, matches) in REPEAT_PROBES {
        assert_eq!(flags_of(REPEAT, text), expected(matches), "{:?}", text);
    }
}

/// LanguageTool 6.6's messages, which name the paragraph's level.
#[test]
fn messages() {
    let messages = |text: &str| -> Vec<String> {
        ON.suggest(text, &TOKENIZER)
            .into_iter()
            .filter(is_paragraph_rule)
            .map(|s| s.message().to_owned())
            .collect()
    };
    let simple = READABILITY_PROBES[1].0;
    assert_eq!(
        messages(simple)[0],
        "Readability: The text of this paragraph is too simple {Level 6: Very easy}. Too few \
         words per sentence and too few syllables per word."
    );
    let difficult = READABILITY_PROBES[2].0;
    assert_eq!(
        messages(difficult),
        [
            "Readability: The text of this paragraph is too difficult {Level 0: Very difficult}. \
          Too many words per sentence and too many syllables per word."
        ]
    );
    let fairly_easy = "She walked to the store and bought some bread for her family before the \
                       evening came.";
    let readability: Vec<String> = messages(&format!("{0}\n\n{0}", fairly_easy))
        .into_iter()
        .filter(|m| m.starts_with("Readability"))
        .collect();
    assert_eq!(readability.len(), 2);
    assert!(readability
        .iter()
        .all(|m| m.contains("too simple {Level 4: Fairly easy}")));
    assert_eq!(
        messages("She ran.\n\nShe hid."),
        ["Same beginning as last paragraph"; 2]
    );
}

/// The level option: `READABILITY_RULE_SIMPLE` flags paragraphs above it.
#[test]
fn readability_level_option() {
    let fairly_easy = "She walked to the store and bought some bread for her family before the \
                       evening came.";
    let text = format!("{0}\n\n{0}", fairly_easy);
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules
        .enable_rules(&[format!("BUILTIN/{}", SIMPLE)])
        .unwrap();
    assert_eq!(flags_with(&rules, &text).len(), 2);
    for rule in rules.builtins_mut() {
        if let Builtin::Readability(rule) = rule.builtin_mut() {
            assert!(rule.set_level(7).is_err());
            rule.set_level(4).unwrap();
        }
    }
    assert!(flags_with(&rules, &text).is_empty());
}

/// `Rules::set_readability_level` sets the level of both readability rules, and an
/// out-of-range level changes neither.
#[test]
fn readability_level_for_both_rules() {
    // Level 5 (easy), with curly quotes and apostrophes, an em dash, an ellipsis and a
    // line separator.
    let easy = "\u{201c}She walked to the store\u{2014}and bought some bread for her \
                       family\u{2019}s table,\u{201d} he said\u{2026}\u{2028}It was before the \
                       evening came.";
    let text = format!("{0}\n\n{0}", easy);
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules
        .enable_rules(&[
            format!("BUILTIN/{}", SIMPLE),
            format!("BUILTIN/{}", DIFFICULT),
        ])
        .unwrap();
    let ids = |rules: &Rules| -> Vec<String> {
        flags_with(rules, &text)
            .into_iter()
            .map(|(id, _)| id)
            .collect()
    };
    assert_eq!(ids(&rules), [SIMPLE; 2]);
    rules.set_readability_level(5).unwrap();
    assert!(ids(&rules).is_empty());
    rules.set_readability_level(6).unwrap();
    assert_eq!(ids(&rules), [DIFFICULT; 2]);
    assert!(matches!(
        rules.set_readability_level(7),
        Err(nlprule::Error::OptionOutOfRange { value: 7, .. })
    ));
    assert_eq!(ids(&rules), [DIFFICULT; 2]);
    rules.set_readability_level(0).unwrap();
    assert_eq!(ids(&rules), [SIMPLE; 2]);
}

/// Checks `parts` one by one with `check_paragraph`, resolves the digests and merges the document
/// matches into each part's suggestions, shifted to the coordinates of the text made of the parts
/// joined with `gap`.
fn incremental(rules: &Rules, parts: &[&str], gap: &str, ends_paragraph: bool) -> Vec<Suggestion> {
    let checked: Vec<_> = parts
        .iter()
        .map(|part| rules.check_paragraph(part, ends_paragraph, &TOKENIZER))
        .collect();
    let digests: Vec<ParagraphDigest> = checked.iter().map(|(_, d)| d.clone()).collect();
    let matches = rules.resolve_document(&digests).matches;

    let gap = Position {
        byte: gap.len(),
        char: gap.chars().count(),
    };
    let mut out = Vec::new();
    let mut start = Position::default();
    for (i, (part, (suggestions, digest))) in parts.iter().zip(checked).enumerate() {
        let document: Vec<Suggestion> = matches
            .iter()
            .filter(|m| m.paragraph() == i)
            .map(|m| m.to_suggestion(part).unwrap())
            .collect();
        out.extend(
            merge_document_suggestions(suggestions, document)
                .into_iter()
                .map(|s| s.rshift(start)),
        );
        start += digest.text_len();
        start += gap;
    }
    out
}

type Key = (String, Range<usize>, Range<usize>, String);

fn keys(suggestions: &[Suggestion]) -> Vec<Key> {
    suggestions
        .iter()
        .filter(|s| is_paragraph_rule(s))
        .map(|s| {
            (
                s.source().to_owned(),
                s.span().char().clone(),
                s.span().byte().clone(),
                s.message().to_owned(),
            )
        })
        .collect()
}

/// The text split before each sentence (as the tokenizer splits it, whitespace-only sentences
/// included).
fn sentences(text: &str) -> Vec<&str> {
    let mut starts: Vec<usize> = TOKENIZER
        .pipe_with_whitespace_sentences(text, false)
        .map(|s| s.span().byte().start)
        .collect();
    starts.push(text.len());
    starts.windows(2).map(|x| &text[x[0]..x[1]]).collect()
}

/// Asserts that `split`, the suggestions from checking `pieces` of `text`, has the paragraph-rule
/// matches of `whole`, except that a readability match whose first three words are in more than
/// one piece ends with the first of them.
fn assert_same(whole: &[Key], split: &[Suggestion], pieces: &[&str], text: &str) {
    let split = keys(split);
    assert_eq!(whole.len(), split.len(), "{:?} in {:?}", text, pieces);
    let mut ends = Vec::new();
    let mut end = 0;
    for piece in pieces {
        end += piece.chars().count();
        ends.push(end);
    }
    for (a, b) in whole.iter().zip(&split) {
        if a != b {
            assert!(
                a.0.starts_with("BUILTIN/READABILITY"),
                "{:?}: {:?} {:?}",
                text,
                a,
                b
            );
            assert_eq!((&a.1.start, &a.3), (&b.1.start, &b.3));
            assert!(
                b.1.end < a.1.end && ends.contains(&b.1.end),
                "{:?}: {:?} {:?}",
                text,
                a,
                b
            );
        }
    }
}

/// A fiction manuscript: curly quotes and apostrophes, em dashes, ellipses, U+2028, a single line
/// break inside a paragraph, blank lines, paragraphs that begin alike and very easy ones.
const MANUSCRIPT: &str = "\u{201c}The cat sat on the mat,\u{201d} she said\u{2014}softly. \
                          \u{201c}The dog ran\u{2026} We had fun.\u{201d}\n\
                          \n\
                          \u{201c}The cat is gone,\u{201d} he said. It\u{2019}s late.\u{2028}We\u{2019}ll go \
                          now and sit\nby the fire for a bit.\n\
                          \n\
                          She ran. She hid. She slept for a long time in the old red barn.\n\
                          \n\n\
                          She woke.  \n\
                          \n\
                          Institutional considerations regarding international collaboration \
                          necessitate comprehensive organizational restructuring.";

/// Whether `text` has whitespace where a split can change the paragraph rules' matches (see the
/// `document` module): a tab after a blank line, a bare LF followed by a CRLF, or other
/// whitespace than ASCII before a line break.
fn split_gap(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    text.contains("\n\n\t")
        || text.contains("\n\r\n")
        || chars
            .windows(2)
            .any(|x| x[0].is_whitespace() && !x[0].is_ascii() && matches!(x[1], '\r' | '\n'))
}

/// A blank line holding spaces adds nothing, also as a paragraph of its own with the separators
/// stripped; one holding a no-break space is a paragraph of its own (LanguageTool 6.6).
#[test]
fn whitespace_only_paragraphs() {
    for (paragraphs, expected) in [
        (["She ran.", " ", "She hid."], 2),
        (["She ran.", "\t", "She hid."], 2),
        (["She ran.", "\u{a0}", "She hid."], 0),
    ] {
        let whole = keys(&ON.suggest(&paragraphs.join("\n\n"), &TOKENIZER));
        assert_eq!(whole.len(), expected, "{:?}", paragraphs);
        assert_eq!(
            whole,
            keys(&incremental(&ON, &paragraphs, "\n\n", true)),
            "{:?}",
            paragraphs
        );
    }
}

/// `suggest` on the whole text gives the same paragraph-rule matches as checking it in pieces
/// and resolving the digests, however it is split between sentences: line by line with the line
/// breaks kept, sentence by sentence, as one piece, or at blank lines with the separators
/// stripped and `ends_paragraph`.
#[test]
fn suggest_equals_paragraph_digests_and_resolver() {
    let mut texts: Vec<&str> = READABILITY_PROBES
        .iter()
        .chain(REPEAT_PROBES)
        .map(|(text, _)| *text)
        .collect();
    texts.push(MANUSCRIPT);
    for text in texts {
        let whole = keys(&ON.suggest(text, &TOKENIZER));
        assert_eq!(
            whole,
            keys(&incremental(&ON, &[text], "", true)),
            "{:?}",
            text
        );
        if split_gap(text) {
            // Not equivalent when split in that whitespace, see `equivalence_on_random_texts`.
            continue;
        }
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        assert_same(&whole, &incremental(&ON, &lines, "", false), &lines, text);
        let sentences = sentences(text);
        assert_same(
            &whole,
            &incremental(&ON, &sentences, "", false),
            &sentences,
            text,
        );

        // Stripping the separators drops whitespace at the paragraphs' ends, which
        // LanguageTool can count as a sentence: only for paragraphs without it.
        let paragraphs: Vec<&str> = text.split("\n\n").collect();
        if paragraphs.iter().all(|x| x.trim() == *x && !x.is_empty()) {
            assert_eq!(
                whole,
                keys(&incremental(&ON, &paragraphs, "\n\n", true)),
                "{:?} by paragraph",
                text
            );
        }
    }
    let whole = flags(MANUSCRIPT);
    assert!(
        whole.iter().filter(|(id, _)| id == REPEAT).count() >= 4,
        "{:?}",
        whole
    );
    assert!(whole.iter().any(|(id, _)| id == SIMPLE), "{:?}", whole);
}

/// A paragraph checked one line at a time whose first three words are not on one line: the
/// readability match ends with the first line's piece.
#[test]
fn readability_mark_ends_with_its_piece() {
    let text = "The cat\nsat on the mat. The dog ran to the park. We had fun.\n\n\
                I saw a big red bus. It was fast. We got on it and sat down.";
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let flagged: Vec<_> = incremental(&ON, &lines, "", false)
        .iter()
        .filter(|s| is_paragraph_rule(s))
        .map(|s| s.span().char().clone())
        .collect();
    assert_eq!(flagged, [0..8, 62..69]);
}

/// Editing one paragraph changes the paragraph rules' matches in the paragraphs that did not
/// change: the paragraph before it for `PARAGRAPH_REPEAT_BEGINNING_RULE`, and every paragraph for
/// the readability rules' whole-text condition.
#[test]
fn an_edit_changes_matches_in_other_paragraphs() {
    let digest = |text: &str| ON.check_paragraph(text, true, &TOKENIZER).1;
    let in_paragraphs = |digests: &[ParagraphDigest], id: &str| -> Vec<usize> {
        ON.resolve_document(digests)
            .matches
            .iter()
            .filter(|m| m.source() == format!("BUILTIN/{}", id))
            .map(|m| m.paragraph())
            .collect()
    };
    let first = digest("She ran to the barn.");
    assert_eq!(
        in_paragraphs(&[first.clone(), digest("She hid in the hay.")], REPEAT),
        [0, 1]
    );
    assert!(in_paragraphs(&[first.clone(), digest("He hid in the hay.")], REPEAT).is_empty());

    let easy = digest("The cat sat on the mat. The dog ran to the park. We had fun.");
    let easy_too = digest("I saw a big red bus. It was fast. We got on it and sat down.");
    let hard = digest(
        "Institutional considerations regarding international collaboration necessitate \
         comprehensive organizational restructuring, notwithstanding substantial \
         administrative complications.",
    );
    assert_eq!(
        in_paragraphs(&[easy.clone(), easy_too.clone()], SIMPLE),
        [0, 1]
    );
    // The hard paragraph brings the whole text's level down to 3 or below.
    assert!(in_paragraphs(&[easy.clone(), easy_too, hard.clone()], SIMPLE).is_empty());
    // One paragraph alone is never too simple, but can be too difficult.
    assert!(in_paragraphs(&[easy], SIMPLE).is_empty());
    assert_eq!(in_paragraphs(&[hard], DIFFICULT), [0]);
}

/// The paragraph rules add their own suggestions and change no other.
#[test]
fn paragraph_rules_only_add_their_own_suggestions() {
    let without: Vec<_> = RULES
        .suggest(MANUSCRIPT, &TOKENIZER)
        .iter()
        .map(|s| (s.source().to_owned(), s.span().char().clone()))
        .collect();
    let with: Vec<_> = ON
        .suggest(MANUSCRIPT, &TOKENIZER)
        .iter()
        .filter(|s| !is_paragraph_rule(s))
        .map(|s| (s.source().to_owned(), s.span().char().clone()))
        .collect();
    assert_eq!(without, with);
}

/// A digest built with the paragraph rules round-trips and resolves the same.
#[test]
fn digests_round_trip() {
    let (_, digest) = ON.check_paragraph(MANUSCRIPT, true, &TOKENIZER);
    let back = ParagraphDigest::from_bytes(&digest.to_bytes()).unwrap();
    assert_eq!(back, digest);
    assert_eq!(
        ON.resolve_document(&[back]),
        ON.resolve_document(std::slice::from_ref(&digest))
    );
    // Digests cover the paragraph rules while they are disabled.
    let (_, off) = RULES.check_paragraph(MANUSCRIPT, true, &TOKENIZER);
    assert_eq!(off, digest);
}

/// Corrupted digests are rejected or resolve without panicking: a stored digest that was
/// damaged must not crash the app.
#[test]
fn corrupted_digests_never_panic() {
    let texts = [
        "\u{201c}She ran,\u{201d} he said. The cat sat.\n\nShe hid in the barn.\n",
        "\nShe hid. It\u{2019}s late. We go.\u{a0}\n\n",
        "The dog ran. \r\n",
    ];
    let digests: Vec<Vec<u8>> = texts
        .iter()
        .map(|text| ON.check_paragraph(text, false, &TOKENIZER).1.to_bytes())
        .collect();
    let mut seed: u64 = 99;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    for _ in 0..5000 {
        let mut decoded = Vec::new();
        for bytes in &digests {
            let mut bytes = bytes.clone();
            for _ in 0..next() % 3 {
                let i = 4 + next() % (bytes.len() - 4);
                bytes[i] = (next() % 256) as u8;
            }
            if next() % 10 == 0 {
                bytes.truncate(4 + next() % (bytes.len() - 4));
            }
            if let Ok(digest) = ParagraphDigest::from_bytes(&bytes) {
                decoded.push(digest);
            }
        }
        for m in ON.resolve_document(&decoded).matches {
            for text in texts {
                let _ = m.to_suggestion(text);
            }
        }
    }
}

/// A match of the paragraph rules applied to an edited paragraph never panics.
#[test]
fn stale_digests_never_panic() {
    let text = "\u{201c}She ran,\u{201d} he said.\n\n\u{201c}She hid,\u{201d} he said.";
    let (_, digest) = ON.check_paragraph(text, true, &TOKENIZER);
    let matches = ON.resolve_document(&[digest]).matches;
    assert_eq!(matches.len(), 2);
    for edited in [
        "",
        "x",
        "\u{1F600}\u{1F600}\u{1F600}\u{1F600}\u{1F600}",
        "\u{2014}\u{2026}\u{2028}",
    ] {
        for m in &matches {
            let _ = m.to_suggestion(edited);
        }
    }
    assert!(matches[0].to_suggestion("\u{1F600}").is_none());
}

/// Random texts made of sentences, whitespace runs, line breaks, curly quotes, em dashes,
/// ellipses and U+2028 give the same paragraph-rule matches checked whole, line by line and
/// sentence by sentence.
///
/// Not covered: a tab or a no-break space right after a blank line. In a whole text the
/// sentencizer keeps a tab with the sentence before the blank line (so LanguageTool sees no
/// paragraph end there) and makes a no-break space a sentence of its own, which a text split at
/// the blank line can't know.
#[test]
fn equivalence_on_random_texts() {
    const PIECES: [&str; 17] = [
        "\u{a0}",
        "She ran.",
        "She hid.",
        "The cat sat on the mat.",
        "  ",
        " ",
        "\n",
        "\n\n",
        "\r\n",
        "\u{2028}",
        "\u{201c}She went.\u{201d}",
        "it\u{2019}s\u{2026}",
        "\u{2014}",
        "She hid.\n",
        "Institutional considerations regarding collaboration.",
        "We had fun. ",
        "Go.",
    ];
    let mut seed: u64 = 12345;
    let mut next = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as usize
    };
    for _ in 0..200 {
        let text: String = (0..next() % 12)
            .map(|_| PIECES[next() % PIECES.len()])
            .collect();
        let whole = keys(&ON.suggest(&text, &TOKENIZER));
        let lines: Vec<&str> = text.split_inclusive('\n').collect();
        assert_same(&whole, &incremental(&ON, &lines, "", false), &lines, &text);
        let sentences = sentences(&text);
        assert_same(
            &whole,
            &incremental(&ON, &sentences, "", false),
            &sentences,
            &text,
        );
    }
}

lazy_static! {
    /// Every whole-text builtin on: the paragraph rules and the word-statistics ones share each
    /// digest and one resolver.
    static ref ALL: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        let selectors: Vec<String> = IDS
            .iter()
            .chain(&[
                "EN_CONSISTENT_APOS",
                "EN_REPEATEDWORDS",
                "EN_WORD_COHERENCY",
                "TOO_OFTEN_USED_ADJECTIVE_EN",
                "TOO_OFTEN_USED_NOUN_EN",
                "TOO_OFTEN_USED_VERB_EN",
                OPENER,
            ])
            .map(|x| format!("BUILTIN/{}", x))
            .collect();
        rules.enable_rules(&selectors).unwrap();
        rules
    };
}

/// `(source, char start, char end, message)` of the suggestions of the whole-text rules, without
/// the end of a readability match (it may end with the first piece of a split).
fn whole_text_keys(suggestions: &[Suggestion]) -> Vec<(String, usize, Option<usize>, String)> {
    let sources = [
        "EN_CONSISTENT_APOS",
        "EN_REPEATEDWORDS",
        "EN_WORD_COHERENCY",
        "TOO_OFTEN_USED",
        SIMPLE,
        DIFFICULT,
        REPEAT,
        OPENER,
    ];
    suggestions
        .iter()
        .filter(|s| {
            sources
                .iter()
                .any(|x| s.source().starts_with(&format!("BUILTIN/{}", x)))
        })
        .map(|s| {
            let end = (!s.source().starts_with("BUILTIN/READABILITY")).then(|| s.span().char().end);
            (
                s.source().to_owned(),
                s.span().char().start,
                end,
                s.message().to_owned(),
            )
        })
        .collect()
}

/// With every whole-text rule on, checking the paragraphs one by one and resolving their digests
/// gives the matches of the whole text for the paragraph rules and the word-statistics rules
/// alike, and damaged digest bytes never panic.
#[test]
fn all_whole_text_rules_share_the_digests() {
    let text = format!(
        "{0}\n\nThe advisor\u{2019}s weird problem. The adviser said it was weird. It's \
         weird.\n\n{0}",
        // Not split inside the run of three line breaks, where a split is in whitespace.
        MANUSCRIPT.replace("\n\n\n", "\n\n")
    );
    let whole = whole_text_keys(&ALL.suggest(&text, &TOKENIZER));
    let sources: Vec<&str> = whole.iter().map(|x| x.0.as_str()).collect();
    for id in [REPEAT, OPENER, "EN_WORD_COHERENCY", "EN_CONSISTENT_APOS"] {
        assert!(
            sources.iter().any(|x| x.contains(id)),
            "{}: {:?}",
            id,
            sources
        );
    }
    let blocks: Vec<&str> = text.split("\n\n").collect();
    assert_eq!(
        whole_text_keys(&incremental(&ALL, &blocks, "\n\n", true)),
        whole
    );
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    assert_eq!(
        whole_text_keys(&incremental(&ALL, &lines, "", false)),
        whole
    );

    let digests: Vec<Vec<u8>> = blocks
        .iter()
        .map(|block| ALL.check_paragraph(block, true, &TOKENIZER).1.to_bytes())
        .collect();
    let mut seed: u64 = 7;
    let mut next = move || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 33) as usize
    };
    for _ in 0..3000 {
        let mut changed = digests.clone();
        for bytes in changed.iter_mut() {
            for _ in 0..next() % 3 {
                let i = 4 + next() % (bytes.len() - 4);
                bytes[i] = (next() % 256) as u8;
            }
        }
        for m in ALL.resolve_document_bytes(&changed).matches {
            let _ = m.to_suggestion(blocks[m.paragraph()]);
        }
    }
}

lazy_static! {
    static ref OPENER_ON: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        assert_eq!(
            rules
                .enable_rules(&[format!("BUILTIN/{}", OPENER)])
                .unwrap(),
            1
        );
        rules
    };
}

/// The text each `PARAGRAPH_OPENER_REPEAT` match marks, with the index of its paragraph in
/// `text` split at blank lines.
fn opener_flags(rules: &Rules, text: &str) -> Vec<(usize, String)> {
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| s.source() == format!("BUILTIN/{}", OPENER))
        .map(|s| {
            let byte = s.span().byte().clone();
            assert!(s.replacements().is_empty());
            assert_eq!(s.message(), "Nearby paragraphs begin with the same word");
            (
                text[..byte.start].matches("\n\n").count(),
                text[byte].to_owned(),
            )
        })
        .collect()
}

fn owned(flags: &[(usize, &str)]) -> Vec<(usize, String)> {
    flags.iter().map(|(i, x)| (*i, (*x).to_owned())).collect()
}

/// Each paragraph is compared with the three before it, and every paragraph of a match is
/// marked once, from its first token to the repeated word.
#[test]
fn paragraph_opener_repeat_looks_three_paragraphs_back() {
    let text = "I opened the door. \u{201c}Who\u{2019}s there?\u{201d} I asked.\n\n\
                Shaking, I stepped inside\u{2014}slowly.\n\n\
                I lit the lamp\u{2026} nothing moved.\n\n\
                I sat down.\u{2028}The clock ticked.\n\n\
                Okay, so it was empty.\n\n\
                Before dawn, I heard it again.\n\n\
                I stood up.";
    assert_eq!(
        opener_flags(&OPENER_ON, text),
        owned(&[(0, "I"), (2, "I"), (3, "I"), (6, "I")])
    );
    // Four paragraphs apart is outside the window; LanguageTool's rule flags neither.
    let far = "She ran.\n\nHe hid.\n\nThey waited.\n\nWe left.\n\nShe stopped.";
    assert!(opener_flags(&OPENER_ON, far).is_empty());
    let near = "She ran.\n\nHe hid.\n\nThey waited.\n\nShe stopped.";
    assert_eq!(
        opener_flags(&OPENER_ON, near),
        owned(&[(0, "She"), (3, "She")])
    );
    // A line break inside a paragraph is not a paragraph break.
    assert!(opener_flags(&OPENER_ON, "She ran.\nShe hid.").is_empty());
}

/// Beginnings compare as in LanguageTool's rule: a shared quote and a shared determiner are
/// skipped, the word is case-sensitive, and a dash or ellipsis lead never matches.
#[test]
fn paragraph_opener_repeat_compares_like_languagetool() {
    let cases: &[(&str, &[(usize, &str)])] = &[
        (
            "\u{201c}I\u{2019}m cold,\u{201d} she said.\n\nShe nodded.\n\n\u{201c}I know.\u{201d}",
            &[(0, "\u{201c}I"), (2, "\u{201c}I")],
        ),
        ("\u{201c}I\u{2019}m cold.\u{201d}\n\nI know.", &[]),
        ("The dog barked.\n\nThe cat hissed.", &[]),
        (
            "The dog barked.\n\nA bird sang.\n\nThe dog slept.",
            &[(0, "The dog"), (2, "The dog")],
        ),
        ("\u{2014}She ran.\n\n\u{2014}She hid.", &[]),
        ("\u{2026}and then.\n\n\u{2026}and again.", &[]),
        ("i ran.\n\nI hid.", &[]),
    ];
    for (text, flags) in cases {
        assert_eq!(opener_flags(&OPENER_ON, text), owned(flags), "{:?}", text);
    }
}

/// Off by default, on in the fiction profile.
#[test]
fn paragraph_opener_repeat_is_on_in_fiction() {
    let text = "She ran.\n\nHe hid.\n\nShe stopped.";
    assert!(opener_flags(&RULES, text).is_empty());
    let mut fiction = Rules::new(RULES_PATH).unwrap();
    assert!(fiction.apply_profile(&Profile::fiction()).is_empty());
    assert_eq!(
        opener_flags(&fiction, text),
        owned(&[(0, "She"), (2, "She")])
    );
}
