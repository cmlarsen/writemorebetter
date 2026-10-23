//! Whole-text rules: paragraph digests, the document resolver, and `EN_CONSISTENT_APOS`.

use bincode::Options;
use lazy_static::lazy_static;
use nlprule::{
    document::{merge_document_suggestions, DIGEST_VERSION},
    profile::Profile,
    types::{Position, Suggestion},
    Error, ParagraphDigest, Rules, Tokenizer,
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

/// Splits `text` after each line break, keeping the line breaks.
fn paragraphs(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// Checks `parts` one by one with `check_paragraph`, resolves the digests and merges the document
/// matches into each part's suggestions, shifted to the coordinates of the text made of the parts
/// joined with `gap`.
fn incremental(
    rules: &Rules,
    parts: &[&str],
    gap: &str,
    ends_paragraph: bool,
) -> (Vec<Suggestion>, Vec<ParagraphDigest>) {
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
        assert_eq!(digest.text_len().byte, part.len());
        assert_eq!(digest.text_len().char, part.chars().count());
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
    (out, digests)
}

type Key = (
    String,
    std::ops::Range<usize>,
    std::ops::Range<usize>,
    Vec<String>,
);

fn key(s: &Suggestion) -> Key {
    (
        s.source().to_owned(),
        s.span().char().clone(),
        s.span().byte().clone(),
        s.replacements().to_vec(),
    )
}

fn apos_keys(suggestions: &[Suggestion]) -> Vec<Key> {
    suggestions
        .iter()
        .filter(|s| s.source() == SOURCE)
        .map(key)
        .collect()
}

const MANUSCRIPT: &str = "\u{201c}Don't go,\u{201d} she said\u{2014}softly.\n\
                          \n\
                          He didn\u{2019}t answer\u{2026} the dogs\u{2019} bowls were empty.\n\
                          It\u{2019}s late, and O'Brien's rock'n'roll was loud.\u{2028}Nothin' doin'.\n\
                          \n\
                          \u{2018}Fine,\u{2019} she said. 'tis true.";

/// `suggest` on the whole text gives the same whole-text matches as checking it paragraph by
/// paragraph and resolving the digests. (Only those: the other rules see one paragraph at a
/// time in `check_paragraph`.)
#[test]
fn suggest_equals_paragraph_digests_and_resolver() {
    let whole = apos_keys(&APOS.suggest(MANUSCRIPT, &TOKENIZER));
    let (split, digests) = incremental(&APOS, &paragraphs(MANUSCRIPT), "", false);
    assert!(whole.len() > 5, "{:?}", whole);
    assert_eq!(whole, apos_keys(&split));
    assert_eq!(digests.len(), paragraphs(MANUSCRIPT).len());

    // The whole text checked as one paragraph has the same matches.
    let (one, _) = incremental(&APOS, &[MANUSCRIPT], "", true);
    assert_eq!(whole, apos_keys(&one));
}

/// The equivalence holds however an app splits the text: with the separators kept in the
/// pieces, or stripped (an editor that stores paragraphs without them), at line breaks, at
/// paragraph or line separators, or mid-paragraph.
#[test]
fn equivalence_does_not_depend_on_the_split() {
    let texts = [
        MANUSCRIPT,
        "It's here.\nIt\u{2019}s there.\nWe'll go.",
        "It's here.\n\nIt\u{2019}s there.\n\nWe'll go.\n",
        "It's here.\r\nIt\u{2019}s there.\r\n\r\nWe'll go.",
        "  It's here.\n\n  It\u{2019}s there.  \n\n   \n\nWe'll go.  ",
        "\n\nIt's here.\n\n\n\nIt\u{2019}s there.",
        "\u{1F600} It's here.\n\u{1F600}\u{1F600} It\u{2019}s there.\n\u{1D11E} We'll see.",
        "It's here.\u{2029}It\u{2019}s there.\u{2029}We'll go.",
        "It's here.\u{2028}It\u{2019}s there.\u{2028}We'll go.",
        "The dogs'\n'Tis true. It\u{2019}s.",
        "The dogs'\n\n'Tis true. It\u{2019}s.",
        "Ross's car O\u{2019}Brien's.\nO\u{2019}Brien\u{2019}s.",
        "He wagged it's tail. The cat\u{2019}s fine.\nIts a dog's life.",
        "'Come here,' she said.\n'I\u{2019}m waiting.'",
    ];
    for text in texts {
        let whole = apos_keys(&APOS.suggest(text, &TOKENIZER));
        for separator in ["\n", "\u{2029}", "\u{2028}", ". "] {
            let parts: Vec<&str> = text.split_inclusive(separator).collect();
            for ends_paragraph in [false, true] {
                let (incremental, _) = incremental(&APOS, &parts, "", ends_paragraph);
                assert_eq!(
                    whole,
                    apos_keys(&incremental),
                    "{:?} split after {:?}",
                    text,
                    separator
                );
            }
            let parts: Vec<&str> = text.split(separator).collect();
            let (incremental, _) = incremental(&APOS, &parts, separator, true);
            assert_eq!(
                whole,
                apos_keys(&incremental),
                "{:?} split at {:?}",
                text,
                separator
            );
        }
    }
}

/// Paragraph ends come from blank lines between tokens, also after trailing spaces, whichever
/// sentence the whitespace ends up in; a line holding only spaces is not blank, as in LT 6.6.
#[test]
fn digests_find_blank_lines() {
    let starts = |text: &str| {
        APOS.check_paragraph(text, false, &TOKENIZER)
            .1
            .paragraph_starts()
            .collect::<Vec<_>>()
    };
    assert_eq!(starts("The dog ran home.\n\nThe dog sat down."), [19]);
    assert_eq!(starts("The dog ran home.  \n\nThe dog sat down."), [21]);
    assert_eq!(starts("The dog ran home.\r\n\r\nThe dog sat down."), [21]);
    // The offset can come before the spaces that start the next paragraph.
    assert_eq!(starts("The dog ran home.\n\n  The dog sat down."), [19]);
    assert_eq!(starts("\u{201c}Go.\u{201d}  \n\n\u{201c}No.\u{201d}"), [9]);
    assert_eq!(starts("The dog ran home\n\nThe dog sat down."), [18]);
    assert!(starts("The dog ran home.\nThe dog sat down.").is_empty());
    assert!(starts("The dog ran home.\n \nThe dog sat down.").is_empty());
    assert!(starts("The dog ran home.\u{2029}The dog sat down.").is_empty());
    assert!(starts("The dog ran home.\u{2028}\u{2028}The dog sat down.").is_empty());

    let (_, digest) = APOS.check_paragraph("No blank line at the end.", false, &TOKENIZER);
    assert!(!digest.ends_paragraph());
    let (_, digest) = APOS.check_paragraph("No blank line at the end.", true, &TOKENIZER);
    assert!(digest.ends_paragraph());
    let (_, digest) = APOS.check_paragraph("A blank line.  \n\n", false, &TOKENIZER);
    assert!(digest.ends_paragraph());
}

/// A whole-text match is kept when it overlaps a match of another rule, as LanguageTool 6.6
/// reports both (`IT_IS_2` on "it's", `EN_UNPAIRED_QUOTES` on a quote).
#[test]
fn whole_text_matches_overlapping_other_rules_are_kept() {
    let text = "The dog wagged it's tail. The cat\u{2019}s fine.";
    assert_eq!(
        flags(text),
        vec![typewriter("'s"), typographic("\u{2019}s")]
    );
    let suggestions = APOS.suggest(text, &TOKENIZER);
    assert!(suggestions
        .iter()
        .any(|s| s.source() != SOURCE && s.span().char().start <= 17 && s.span().char().end >= 19));

    assert_eq!(
        flags("'Come here,' she said. 'I\u{2019}m waiting.'"),
        vec![
            typewriter("'"),
            typewriter("'"),
            typewriter("'"),
            typographic("\u{2019}m"),
            typewriter("'")
        ]
    );
}

/// The other rules' suggestions are unchanged by enabling the whole-text rule.
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
    let first = "It's late. We'll go.";
    let (_, a) = APOS.check_paragraph(first, true, &TOKENIZER);
    let (_, b) = APOS.check_paragraph("He said no.", true, &TOKENIZER);
    assert!(APOS.resolve_document(&[a.clone(), b]).matches.is_empty());

    let (_, b) = APOS.check_paragraph("He didn\u{2019}t say no.", true, &TOKENIZER);
    let matches = APOS.resolve_document(&[a.clone(), b]).matches;
    let in_first: Vec<_> = matches.iter().filter(|m| m.paragraph() == 0).collect();
    assert_eq!(in_first.len(), 2);
    assert_eq!(
        in_first[0].to_suggestion(first).unwrap().replacements(),
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
    let (_, off) = RULES.check_paragraph(text, true, &TOKENIZER);
    let (_, on) = APOS.check_paragraph(text, true, &TOKENIZER);
    assert_eq!(off, on);
    assert!(RULES
        .resolve_document(std::slice::from_ref(&off))
        .matches
        .is_empty());
    assert_eq!(APOS.resolve_document(&[off]).matches.len(), 2);
}

#[test]
fn digests_round_trip_with_their_version() {
    let (_, digest) = APOS.check_paragraph(MANUSCRIPT, true, &TOKENIZER);
    assert!(digest.is_current());
    assert_eq!(digest.version(), DIGEST_VERSION);
    let back = ParagraphDigest::from_bytes(&digest.to_bytes()).unwrap();
    assert_eq!(back, digest);
    assert_eq!(
        APOS.resolve_document(&[back]),
        APOS.resolve_document(std::slice::from_ref(&digest))
    );

    // A stale digest is reported, since it can hide matches in every paragraph.
    let (_, fresh) = APOS.check_paragraph("It\u{2019}s there.", true, &TOKENIZER);
    let mut old = ParagraphDigest::from_bytes(&digest.to_bytes()).unwrap();
    let (_, other) = APOS.check_paragraph("We'll go.", true, &TOKENIZER);
    assert_eq!(
        APOS.resolve_document(&[fresh.clone(), other]).matches.len(),
        2
    );
    // The version is also the first (one-byte) integer of the serialized digest after the four
    // version bytes; a digest of another version could reach the resolver through serde.
    let mut bytes = old.to_bytes();
    assert_eq!(bytes[4] as u32, DIGEST_VERSION);
    bytes[4] += 1;
    old = bincode::DefaultOptions::new()
        .with_varint_encoding()
        .deserialize(&bytes[4..])
        .unwrap();
    let resolved = APOS.resolve_document(&[fresh.clone(), old]);
    assert!(resolved.matches.is_empty());
    assert_eq!(resolved.stale, [1]);

    // Stored digests that cannot be read are stale too.
    let resolved = APOS.resolve_document_bytes(&[fresh.to_bytes(), bytes, vec![1, 2]]);
    assert!(resolved.matches.is_empty());
    assert_eq!(resolved.stale, [1, 2]);
    let (_, other) = APOS.check_paragraph("We'll go.", true, &TOKENIZER);
    assert_eq!(
        APOS.resolve_document_bytes(&[fresh.to_bytes(), other.to_bytes()]),
        APOS.resolve_document(&[fresh, other])
    );

    // A digest from another version is rejected with a format error, not misread.
    let mut bytes = digest.to_bytes();
    bytes[..4].copy_from_slice(&(DIGEST_VERSION + 1).to_le_bytes());
    match ParagraphDigest::from_bytes(&bytes) {
        Err(Error::Format(message)) => assert!(message.contains("check the paragraph again")),
        other => panic!("unexpected {:?}", other),
    }
}

/// A digest applied to an edited paragraph never panics and gives no suggestion where the text
/// visibly changed.
#[test]
fn stale_digests_give_no_wrong_suggestions() {
    let (_, digest) = APOS.check_paragraph("I'm fine. It\u{2019}s ok.", true, &TOKENIZER);
    let matches = APOS.resolve_document(&[digest]).matches;
    assert_eq!(matches.len(), 2);
    // `’s` became `'s`: its match no longer applies, the one on `'m` still does.
    let edited = "I'm fine. It's ok.";
    assert!(matches[0].to_suggestion(edited).is_some());
    assert!(matches[1].to_suggestion(edited).is_none());
    for edited in [
        "I am fine. It\u{2019}s ok.",
        "\u{1F600}'m fine. It\u{2019}s ok.",
        "x",
        "",
        "\u{2014}\u{2026}\u{2028}\u{201c}",
    ] {
        for m in &matches {
            assert!(m.to_suggestion(edited).is_none(), "{:?} {:?}", edited, m);
        }
    }
    // Text after the matches may change.
    assert!(matches
        .iter()
        .all(|m| m.to_suggestion("I'm fine. It\u{2019}s ok. More.").is_some()));
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
        .split(blank_line)
        .map(|part| RULES.check_paragraph(part, true, &TOKENIZER).1)
        .collect();
    let bytes: usize = digests.iter().map(|x| x.to_bytes().len()).sum();
    let apostrophes = APOS.resolve_document(&digests).matches.len();
    println!(
        "{} words, {} paragraphs, {} apostrophe matches: {} bytes ({:.0} per 1,000 words)",
        words,
        digests.len(),
        apostrophes,
        bytes,
        bytes as f64 * 1000.0 / words as f64
    );
}

lazy_static! {
    static ref FICTION: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        assert!(rules.apply_profile(&Profile::fiction()).is_empty());
        rules
    };
}

/// `EN_CONSISTENT_APOS` suggestions under the fiction profile, as `flags` gives them.
fn fiction_flags(text: &str) -> Vec<(String, String, bool)> {
    FICTION
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| s.source() == SOURCE)
        .map(|s| {
            let flagged = text[s.span().byte().clone()].to_owned();
            let typographic = s.message() == TYPOGRAPHIC;
            (flagged, s.replacements()[0].clone(), typographic)
        })
        .collect()
}

/// A curly-typeset fiction draft: curly double and single quotes, curly apostrophes, clipped
/// words, a plural possessive before a closing quote, em dashes, ellipses and U+2028.
const CURLY_DRAFT: &str = "\u{201c}Don\u{2019}t go,\u{201d} she said\u{2014}softly. \u{201c}I\u{2019}m \
                           jus\u{2019} tired o\u{2019} waitin\u{2019}\u{2026}\u{201d}\n\
                           \n\
                           \u{2018}Tell \u{2019}em I said so,\u{2019} he wrote. \u{201c}She said \
                           \u{2018}no\u{2019}.\u{201d}\u{2028}The dogs\u{2019} bowls were \
                           empty\u{2026} \u{2018}Rock \u{2019}n\u{2019} roll\u{2019}\u{2014}that\u{2019}s \
                           all.\n\
                           \n\
                           \u{2018}Where\u{2019}s the boys\u{2019}?\u{2019} she asked. \u{2018}Gone.\u{2019}";

#[test]
fn fiction_profile_enables_consistent_apostrophes() {
    let enabled = |rules: &Rules| {
        rules
            .builtins()
            .iter()
            .find(|x| x.id() == ID)
            .unwrap()
            .enabled()
    };
    assert!(enabled(&FICTION));
    assert!(Profile::fiction().enable.contains(&SOURCE.to_owned()));
    // The default profile leaves it off.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(rules.apply_profile(&Profile::default_profile()).is_empty());
    assert!(!enabled(&rules));
}

/// Curly single quotes used as quotation marks are not apostrophes: a draft typeset with curly
/// quotes and apostrophes throughout has no flags, and neither does one with straight
/// apostrophes inside curly quotation marks.
#[test]
fn fiction_curly_quotes_are_not_mixed_apostrophes() {
    assert_eq!(fiction_flags(CURLY_DRAFT), vec![]);
    for text in [
        "\u{2018}Don't go,\u{2019} she said\u{2014}softly. \u{2018}I'm tired\u{2026}\u{2019}",
        "\u{201c}She said \u{2018}no\u{2019}.\u{201d}\u{2028}It's late.",
        "\u{2018}Gone.\u{2019} \u{2018}Where?\u{2019} \u{2018}Out!\u{2019}\u{2014}we'll see.",
        "\u{2018}Come here\u{2019}\u{2014}she couldn't\u{2026}",
    ] {
        assert_eq!(fiction_flags(text), vec![], "{text:?}");
    }
}

/// A straight-typeset draft: straight apostrophes and clipped words, curly double quotes, em
/// dashes, ellipses and U+2028.
const STRAIGHT_DRAFT: &str =
    "\u{201c}Don't go,\u{201d} she said\u{2014}softly. \u{201c}I'm jus' tired o' \
                              waitin'\u{2026}\u{201d}\n\
                              \n\
                              He couldn't say\u{2014}wouldn't.\u{2028}It's late\u{2026} we'll see.";

/// The LanguageTool probe below, checked with `-e EN_CONSISTENT_APOS`: two curly apostrophe
/// tokens and one straight. The tokenizer splits the `’` off `’em`, `jus’`, `o’` and `dogs’`, so
/// those are lone quotes and do not count, in LanguageTool too.
const PROBE: &str = "\u{2018}Tell \u{2019}em I\u{2019}m jus\u{2019} tired o\u{2019} waitin\u{2019}\u{2026}\u{2019} \
                     The dogs\u{2019} bowls. \u{2018}Come here\u{2019}\u{2014}he said \u{2018}no\u{2019}. \
                     He didn't answer.";

/// LanguageTool flags both styles; the fiction profile only the rarer one, suggesting the other.
#[test]
fn fiction_flags_only_the_minority_style() {
    assert_eq!(
        flags(PROBE),
        vec![
            typographic("\u{2019}m"),
            typographic("waitin\u{2019}"),
            typewriter("n't")
        ]
    );
    assert_eq!(fiction_flags(PROBE), vec![typewriter("n't")]);

    // Majority curly: one straight apostrophe pasted into a curly draft.
    let text = format!("{CURLY_DRAFT}\n\nHe didn't answer.");
    assert!(flags(&text).len() > 5);
    assert_eq!(fiction_flags(&text), vec![typewriter("n't")]);

    // Majority straight: one curly apostrophe in a straight draft.
    let text = format!("{STRAIGHT_DRAFT}\n\nShe\u{2019}d gone.");
    assert!(flags(&text).len() > 5);
    assert_eq!(fiction_flags(&text), vec![typographic("\u{2019}d")]);

    // A tie flags the straight ones, as curly is the fiction norm.
    assert_eq!(
        fiction_flags("\u{201c}It's late,\u{201d} she said\u{2014}it\u{2019}s late\u{2026}"),
        vec![typewriter("'s")]
    );
    assert_eq!(
        fiction_flags("We'll go.\u{2028}\u{2018}Don't.\u{2019} I\u{2019}m\u{2026} you\u{2019}re."),
        vec![typewriter("'ll"), typewriter("n't")]
    );
    // Curly by one: the straight ones.
    assert_eq!(
        fiction_flags("We'll go. I\u{2019}m\u{2014}you\u{2019}re."),
        vec![typewriter("'ll")]
    );
    // Straight by one: the curly one. A lone `’` quotation mark does not count.
    assert_eq!(
        fiction_flags("We'll go. It's \u{2018}fine\u{2019}. I\u{2019}m here."),
        vec![typographic("\u{2019}m")]
    );

    // One style throughout: no flags.
    for text in [CURLY_DRAFT, STRAIGHT_DRAFT] {
        assert_eq!(fiction_flags(text), vec![], "{:?}", text);
    }
}

/// The counts are over the whole document, so checking it whole and checking it paragraph by
/// paragraph (however it is split) flag the same apostrophes.
#[test]
fn fiction_whole_text_equals_paragraphs() {
    let texts = [
        format!("{CURLY_DRAFT}\n\nHe didn't answer."),
        format!("{STRAIGHT_DRAFT}\n\nShe\u{2019}d gone."),
        format!("{CURLY_DRAFT}\n\n{STRAIGHT_DRAFT}"),
        format!("{STRAIGHT_DRAFT}\u{2028}{CURLY_DRAFT}"),
        "It's here.\n\nIt\u{2019}s there.\n\nWe'll go.\n".to_owned(),
        "It\u{2019}s here.\u{2029}It's there.\u{2029}We\u{2019}ll go.".to_owned(),
        "\u{201c}It's late,\u{201d}\n\nshe said\u{2014}it\u{2019}s late\u{2026}".to_owned(),
    ];
    for text in &texts {
        let whole = apos_keys(&FICTION.suggest(text, &TOKENIZER));
        assert!(!whole.is_empty(), "{:?}", text);
        for separator in ["\n", "\u{2029}", "\u{2028}", ". "] {
            let parts: Vec<&str> = text.split_inclusive(separator).collect();
            let (split, _) = incremental(&FICTION, &parts, "", false);
            assert_eq!(whole, apos_keys(&split), "{:?} {:?}", text, separator);
            let parts: Vec<&str> = text.split(separator).collect();
            let (split, _) = incremental(&FICTION, &parts, separator, true);
            assert_eq!(whole, apos_keys(&split), "{:?} {:?}", text, separator);
        }
    }
}

/// Editing one paragraph can change which style is the minority, and so which apostrophes are
/// flagged in the paragraphs that did not change.
#[test]
fn fiction_an_edit_flips_the_minority_elsewhere() {
    let first = "It's late. We'll go.";
    let (_, a) = FICTION.check_paragraph(first, true, &TOKENIZER);
    let flagged = |second: &str| {
        let (_, b) = FICTION.check_paragraph(second, true, &TOKENIZER);
        FICTION
            .resolve_document(&[a.clone(), b])
            .matches
            .iter()
            .map(|m| m.paragraph())
            .collect::<Vec<_>>()
    };
    assert_eq!(flagged("He didn\u{2019}t\u{2014}no."), vec![1]);
    // Two curly against two straight: a tie flags the straight ones in the first paragraph.
    assert_eq!(
        flagged("He didn\u{2019}t\u{2014}I\u{2019}m sure."),
        vec![0, 0]
    );
    assert_eq!(
        flagged("He didn\u{2019}t\u{2026} I\u{2019}m sure you\u{2019}re."),
        vec![0, 0]
    );
    assert_eq!(flagged("He said no."), Vec::<usize>::new());
}

/// The fiction profile's option is a case of the rule: without it the rule flags both styles.
#[test]
fn minority_only_is_a_case_of_the_rule() {
    let rule = |rules: &Rules| match rules
        .builtins()
        .iter()
        .find(|x| x.id() == ID)
        .unwrap()
        .builtin()
    {
        nlprule::builtin::Builtin::ConsistentApostrophes(rule) => rule.flags_minority_only(),
        _ => unreachable!(),
    };
    assert!(rule(&FICTION));
    assert!(!rule(&APOS));
    assert!(Profile::fiction()
        .disable
        .contains(&format!("{}/MAJORITY", SOURCE)));
}
