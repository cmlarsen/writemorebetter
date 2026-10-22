//! Whole-text rules: paragraph digests, the document resolver, and `EN_CONSISTENT_APOS`.

use lazy_static::lazy_static;
use nlprule::{
    document::merge_document_suggestions,
    types::{Position, Suggestion},
    ParagraphDigest, Rules, Tokenizer,
};

const TOKENIZER_PATH: &str = "../storage/en_tokenizer.bin";
const RULES_PATH: &str = "../storage/en_rules.bin";
const ID: &str = "EN_CONSISTENT_APOS";
const SOURCE: &str = "BUILTIN/EN_CONSISTENT_APOS";

const TYPEWRITER: &str = "You used a typewriter-style apostrophe here, but a typographic \
                          apostrophe elsewhere in this text. Both are correct, but consider using \
                          the same type everywhere in your text.";
const TYPOGRAPHIC: &str = "You used a typographic apostrophe here, but a typewriter-style \
                           apostrophe elsewhere in this text. Both are correct, but consider using \
                           the same type everywhere in your text.";

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new(TOKENIZER_PATH).unwrap();
    static ref RULES: Rules = Rules::new(RULES_PATH).unwrap();
    static ref APOS: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        assert_eq!(rules.enable_rules(&[format!("BUILTIN/{}", ID)]).unwrap(), 1);
        rules
    };
}

/// `(flagged text, replacement, typographic)` for each `EN_CONSISTENT_APOS` suggestion on `text`,
/// checking that the message matches the style.
fn flags(text: &str) -> Vec<(String, String, bool)> {
    APOS.suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| s.source() == SOURCE)
        .map(|s| {
            let flagged: String = text
                .chars()
                .skip(s.span().char().start)
                .take(s.span().char().len())
                .collect();
            assert_eq!(&text[s.span().byte().clone()], flagged);
            assert_eq!(s.replacements().len(), 1);
            let typographic = s.message() == TYPOGRAPHIC;
            assert!(typographic || s.message() == TYPEWRITER, "{}", s.message());
            (flagged, s.replacements()[0].clone(), typographic)
        })
        .collect()
}

fn typewriter(flagged: &str) -> (String, String, bool) {
    (flagged.into(), flagged.replace('\'', "\u{2019}"), false)
}

fn typographic(flagged: &str) -> (String, String, bool) {
    (flagged.into(), flagged.replace('\u{2019}', "'"), true)
}

#[test]
fn off_by_default() {
    let rule = RULES.builtins().iter().find(|x| x.id() == ID).unwrap();
    assert!(!rule.enabled());
    let text = "I'll buy it. It's nice, but it doesn\u{2019}t work.";
    assert!(RULES
        .suggest(text, &TOKENIZER)
        .iter()
        .all(|s| s.source() != SOURCE));
}

/// The probe from #228, with LanguageTool 6.6's spans, messages and suggestions.
#[test]
fn flags_both_styles_in_a_mixed_text() {
    let text = "I'll buy it. It's nice, but it doesn\u{2019}t work.";
    let suggestions: Vec<Suggestion> = APOS
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| s.source() == SOURCE)
        .collect();
    let spans: Vec<_> = suggestions
        .iter()
        .map(|s| s.span().char().clone())
        .collect();
    assert_eq!(spans, vec![1..4, 15..17, 35..38]);
    assert_eq!(suggestions[0].replacements(), ["\u{2019}ll"]);
    assert_eq!(suggestions[0].message(), TYPEWRITER);
    assert_eq!(suggestions[2].replacements(), ["n't"]);
    assert_eq!(suggestions[2].message(), TYPOGRAPHIC);
}

#[test]
fn one_style_has_no_flags() {
    for text in [
        "I'll buy it. It's nice, but it doesn't work.",
        "I\u{2019}ll buy it. It\u{2019}s nice, but it doesn\u{2019}t work.",
        // Curly single quotes around straight-free text: a lone `’` does not count.
        "\u{2018}Hello,\u{2019} she said. It\u{2019}s fine.",
        // Straight single quotes and straight apostrophes are all typewriter style.
        "'Hello,' she said. It's fine.",
        // A lone closing `’` is not an apostrophe token, so it does not mix with `'`.
        "\u{2018}Hello,\u{2019} she said. It's fine.",
        "No apostrophes at all \u{2014} just dashes\u{2026}\u{2028}and breaks.",
        "",
    ] {
        assert_eq!(flags(text), vec![], "{text:?}");
    }
}

/// Probes checked against LanguageTool 6.6 (`-e EN_CONSISTENT_APOS`).
#[test]
fn matches_languagetool_on_fiction_probes() {
    assert_eq!(
        flags("I'll buy it.\n\nIt\u{2019}s nice."),
        vec![typewriter("'ll"), typographic("\u{2019}s")]
    );
    assert_eq!(
        flags("I'll buy it.\nIt\u{2019}s nice."),
        vec![typewriter("'ll"), typographic("\u{2019}s")]
    );
    // Straight single quotes are typewriter tokens.
    assert_eq!(
        flags("'Hello,' she said. It\u{2019}s fine."),
        vec![typewriter("'"), typewriter("'"), typographic("\u{2019}s")]
    );
    // Curly double quotes, an em dash and an ellipsis.
    assert_eq!(
        flags("\u{201c}Don't,\u{201d} she said\u{2014}it wasn\u{2019}t\u{2026} the dogs\u{2019} bowls."),
        vec![typewriter("n't"), typographic("n\u{2019}t")]
    );
    assert_eq!(
        flags("Nothin' doin'. It\u{2019}s fine."),
        vec![
            typewriter("Nothin'"),
            typewriter("doin'"),
            typographic("\u{2019}s")
        ]
    );
    // U+2028 inside the text.
    assert_eq!(
        flags("It's\u{2028}fine. He\u{2019}d go\u{2026} 'tis true."),
        vec![typewriter("'s"), typographic("\u{2019}d"), typewriter("'")]
    );
    // Apostrophes inside curly quotes vs. the closing curly quote.
    assert_eq!(
        flags("He said \u{2018}it\u{2019}s fine\u{2019}. She said 'it's not'."),
        vec![
            typographic("\u{2019}s"),
            typewriter("'"),
            typewriter("'s"),
            typewriter("'")
        ]
    );
    // Mixed styles in different paragraphs.
    assert_eq!(
        flags("First paragraph isn't curly.\n\nSecond one\u{2019}s curly.\n\nThird: \u{201c}We'll see.\u{201d}"),
        vec![typewriter("n't"), typographic("\u{2019}s"), typewriter("'ll")]
    );
    // Touching matches give one, as LanguageTool's `SameRuleGroupFilter`: `’s` right after
    // `O’Brien` is not flagged.
    assert_eq!(
        flags("O\u{2019}Brien\u{2019}s dog. The Joneses' house."),
        vec![typographic("O\u{2019}Brien"), typewriter("'")]
    );
    assert_eq!(
        flags("The dogs' bowls. It\u{2019}s O'Brien\u{2019}s rock'n'roll."),
        vec![
            typewriter("'"),
            typographic("\u{2019}s"),
            typewriter("O'Brien"),
            typewriter("'"),
            typewriter("'")
        ]
    );
    assert_eq!(
        flags("Line one.\n\n\n\nShe\u{2019}s here. He's there."),
        vec![typographic("\u{2019}s"), typewriter("'s")]
    );
}

/// Splits `text` after each line break, as an app would split it into paragraphs.
fn paragraphs(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Checks `text` paragraph by paragraph with `check_paragraph`, resolves the digests and merges
/// the document matches into each paragraph's suggestions, shifted to `text`'s coordinates.
fn incremental(rules: &Rules, text: &str) -> (Vec<Suggestion>, Vec<ParagraphDigest>) {
    let parts = paragraphs(text);
    let checked: Vec<_> = parts
        .iter()
        .map(|part| rules.check_paragraph(part, &TOKENIZER))
        .collect();
    let digests: Vec<ParagraphDigest> = checked.iter().map(|(_, d)| d.clone()).collect();
    let matches = rules.resolve_document(&digests);

    let mut out = Vec::new();
    let mut start = Position::default();
    for (i, (part, (suggestions, digest))) in parts.iter().zip(checked).enumerate() {
        assert_eq!(digest.text_len().byte, part.len());
        assert_eq!(digest.text_len().char, part.chars().count());
        let document: Vec<Suggestion> = matches
            .iter()
            .filter(|m| m.paragraph() == i)
            .map(|m| m.to_suggestion(part))
            .collect();
        out.extend(
            merge_document_suggestions(suggestions, document)
                .into_iter()
                .map(|s| s.rshift(start)),
        );
        start += digest.text_len();
    }
    (out, digests)
}

fn key(
    s: &Suggestion,
) -> (
    String,
    std::ops::Range<usize>,
    std::ops::Range<usize>,
    Vec<String>,
) {
    (
        s.source().to_owned(),
        s.span().char().clone(),
        s.span().byte().clone(),
        s.replacements().to_vec(),
    )
}

const MANUSCRIPT: &str = "\u{201c}Don't go,\u{201d} she said\u{2014}softly.\n\
                          \n\
                          He didn\u{2019}t answer\u{2026} the dogs\u{2019} bowls were empty.\n\
                          It\u{2019}s late, and O'Brien's rock'n'roll was loud.\u{2028}Nothin' doin'.\n\
                          \n\
                          \u{2018}Fine,\u{2019} she said. 'tis true.";

/// `suggest` on the whole text gives the same whole-text matches as checking it paragraph by
/// paragraph and resolving the digests.
#[test]
fn suggest_equals_paragraph_digests_and_resolver() {
    let whole: Vec<_> = APOS
        .suggest(MANUSCRIPT, &TOKENIZER)
        .iter()
        .filter(|s| s.source() == SOURCE)
        .map(key)
        .collect();
    let (incremental, digests) = incremental(&APOS, MANUSCRIPT);
    let incremental: Vec<_> = incremental
        .iter()
        .filter(|s| s.source() == SOURCE)
        .map(key)
        .collect();
    assert!(whole.len() > 5, "{:?}", whole);
    assert_eq!(whole, incremental);
    assert_eq!(digests.len(), paragraphs(MANUSCRIPT).len());

    // One paragraph checked as a whole gives the same digest as the concatenation of its engine
    // paragraphs' digests.
    let (_, one) = APOS.check_paragraph(MANUSCRIPT, &TOKENIZER);
    assert_eq!(APOS.resolve_document(&[one]).len(), whole.len());
}

/// The other rules' suggestions are unchanged by enabling the whole-text rule, and when nothing
/// overlaps, enabling it only adds its own suggestions.
#[test]
fn whole_text_rule_only_adds_its_own_suggestions() {
    let without: Vec<_> = RULES
        .suggest(MANUSCRIPT, &TOKENIZER)
        .iter()
        .map(key)
        .collect();
    let with: Vec<_> = APOS
        .suggest(MANUSCRIPT, &TOKENIZER)
        .iter()
        .filter(|s| s.source() != SOURCE)
        .map(key)
        .collect();
    assert_eq!(without, with);
}

/// One edit can add matches to paragraphs that did not change: the digests of unchanged
/// paragraphs are reused and the resolver's output replaces the old set.
#[test]
fn an_edit_changes_matches_in_other_paragraphs() {
    let first = "It's late. We'll go.\n";
    let (_, a) = APOS.check_paragraph(first, &TOKENIZER);
    let (_, b) = APOS.check_paragraph("He said no.\n", &TOKENIZER);
    assert!(APOS.resolve_document(&[a.clone(), b]).is_empty());

    let (_, b) = APOS.check_paragraph("He didn\u{2019}t say no.\n", &TOKENIZER);
    let matches = APOS.resolve_document(&[a.clone(), b]);
    let in_first: Vec<_> = matches.iter().filter(|m| m.paragraph() == 0).collect();
    assert_eq!(in_first.len(), 2);
    assert_eq!(
        in_first[0].to_suggestion(first).replacements(),
        ["\u{2019}s"]
    );
    assert_eq!(*in_first[1].span().char(), 13..16);
    assert_eq!(matches.last().unwrap().paragraph(), 1);
}

/// Digests are computed for every whole-text builtin even while it is disabled, so an app can
/// keep them when the author toggles the rule; the resolver only runs enabled rules.
#[test]
fn digests_do_not_depend_on_enabled_rules() {
    let text = "It's here. It\u{2019}s there.";
    let (_, off) = RULES.check_paragraph(text, &TOKENIZER);
    let (_, on) = APOS.check_paragraph(text, &TOKENIZER);
    assert_eq!(off, on);
    assert!(RULES
        .resolve_document(std::slice::from_ref(&off))
        .is_empty());
    assert_eq!(APOS.resolve_document(&[off]).len(), 2);
}

#[test]
fn digests_round_trip_through_serde() {
    let (_, digest) = APOS.check_paragraph(MANUSCRIPT, &TOKENIZER);
    let bytes = bincode::serialize(&digest).unwrap();
    let back: ParagraphDigest = bincode::deserialize(&bytes).unwrap();
    assert_eq!(back, digest);
    assert_eq!(
        APOS.resolve_document(&[back]),
        APOS.resolve_document(&[digest])
    );
}

/// A stale digest (one built from different text) never panics: matches whose span does not fit
/// get no replacement.
#[test]
fn stale_digests_are_utf8_safe() {
    let (_, digest) = APOS.check_paragraph("It's here. It\u{2019}s there.", &TOKENIZER);
    for m in APOS.resolve_document(&[digest]) {
        for text in ["", "\u{2014}\u{2026}\u{2028}\u{201c}", "short"] {
            let _ = m.to_suggestion(text);
        }
    }
}

/// Prints the size of the digests of `bench/public/moby150k.txt`, checked paragraph by paragraph
/// (split at blank lines), per 1,000 words. Run with `--ignored --nocapture`.
#[test]
#[ignore]
fn digest_size_per_thousand_words() {
    let text = std::fs::read_to_string("../bench/public/moby150k.txt").unwrap();
    let words = text.split_whitespace().count();
    let blank_line = if text.contains("\r\n") {
        "\r\n\r\n"
    } else {
        "\n\n"
    };
    let digests: Vec<ParagraphDigest> = text
        .split_inclusive(blank_line)
        .map(|part| RULES.check_paragraph(part, &TOKENIZER).1)
        .collect();
    let bincode = bincode::serialize(&digests).unwrap().len();
    let apostrophes = APOS.resolve_document(&digests).len();
    println!(
        "{} words, {} paragraphs, {} apostrophe matches: {} bytes bincode ({:.0} per 1,000 words)",
        words,
        digests.len(),
        apostrophes,
        bincode,
        bincode as f64 * 1000.0 / words as f64
    );
}
