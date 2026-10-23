//! The word-statistics rules as whole-text rules: `EN_WORD_COHERENCY`, `EN_REPEATEDWORDS` and
//! `TOO_OFTEN_USED_*` give the same matches on a whole text (`Rules::suggest`) as on its
//! paragraphs checked one at a time (`Rules::check_paragraph` and `Rules::resolve_document`).

use lazy_static::lazy_static;
use nlprule::{types::Suggestion, ParagraphDigest, Rules, Tokenizer};
use std::ops::Range;

const TOKENIZER_PATH: &str = "../storage/en_tokenizer.bin";
const RULES_PATH: &str = "../storage/en_rules.bin";

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new(TOKENIZER_PATH).unwrap();
    static ref RULES: Rules = Rules::new(RULES_PATH).unwrap();
    /// The word-statistics rules on (`EN_WORD_COHERENCY` already is).
    static ref WORDS: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        let selectors = [
            "BUILTIN/EN_REPEATEDWORDS",
            "BUILTIN/TOO_OFTEN_USED_ADJECTIVE_EN",
            "BUILTIN/TOO_OFTEN_USED_NOUN_EN",
            "BUILTIN/TOO_OFTEN_USED_VERB_EN",
        ];
        assert_eq!(rules.enable_rules(&selectors.map(String::from)).unwrap(), 4);
        rules
    };
    /// As `WORDS`, leaving direct speech out of the `TOO_OFTEN_USED_*` counts.
    static ref NARRATION: Rules = {
        let mut rules = WORDS.clone();
        for rule in rules.builtins_mut() {
            if rule.id().starts_with("TOO_OFTEN_USED_") {
                assert!(rule.disable_case("DIRECT_SPEECH"));
            }
        }
        rules
    };
}

fn is_word_statistics(source: &str) -> bool {
    ["EN_WORD_COHERENCY", "EN_REPEATEDWORDS", "TOO_OFTEN_USED_"]
        .iter()
        .any(|id| source.starts_with(&format!("BUILTIN/{}", id)))
}

type Key = (String, Range<usize>, Range<usize>, Vec<String>);

fn keys(suggestions: &[Suggestion]) -> Vec<Key> {
    suggestions
        .iter()
        .filter(|s| is_word_statistics(s.source()))
        .map(|s| {
            (
                s.source().to_owned(),
                s.span().char().clone(),
                s.span().byte().clone(),
                s.replacements().to_vec(),
            )
        })
        .collect()
}

/// Checks `parts` one at a time and returns the suggestions of the joined text, the parts joined
/// with `gap` between them, as `Rules::suggest` would.
fn incremental(rules: &Rules, parts: &[&str], gap: &str, ends_paragraph: bool) -> Vec<Suggestion> {
    let checked: Vec<_> = parts
        .iter()
        .map(|part| rules.check_paragraph(part, ends_paragraph, &TOKENIZER))
        .collect();
    let digests: Vec<ParagraphDigest> = checked.iter().map(|(_, d)| d.clone()).collect();
    let resolved = rules.resolve_document(&digests);
    assert!(resolved.stale.is_empty());
    join(parts, gap, |i, part| {
        resolved.suggestions(i, part, checked[i].0.clone())
    })
}

/// The suggestions of each part, shifted to the joined text.
fn join(
    parts: &[&str],
    gap: &str,
    mut suggestions: impl FnMut(usize, &str) -> Vec<Suggestion>,
) -> Vec<Suggestion> {
    let mut out = Vec::new();
    let mut start = nlprule::types::Position::default();
    for (i, part) in parts.iter().enumerate() {
        out.extend(suggestions(i, part).into_iter().map(|s| s.rshift(start)));
        start.byte += part.len() + gap.len();
        start.char += part.chars().count() + gap.chars().count();
    }
    out
}

/// `(flagged text, source without "BUILTIN/", replacements)` of the word-statistics suggestions.
fn flags(rules: &Rules, text: &str) -> Vec<(String, String, Vec<String>)> {
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| is_word_statistics(s.source()))
        .map(|s| {
            (
                text[s.span().byte().clone()].to_owned(),
                s.source().trim_start_matches("BUILTIN/").to_owned(),
                s.replacements().to_vec(),
            )
        })
        .collect()
}

fn char_starts(rules: &Rules, text: &str, id: &str) -> Vec<usize> {
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|s| s.source().starts_with(&format!("BUILTIN/{}", id)))
        .map(|s| s.span().char().start)
        .collect()
}

/// Curly quotes and apostrophes, an em dash, an ellipsis, U+2028, a single line break inside a
/// paragraph and a blank line.
const COHERENCY_PROBE: &str = "\u{201c}I\u{2019}ll be the adviser\u{2014}no one else,\u{201d} she \
    said\u{2026}\u{2028}He nodded.\nThe advisor\u{2019}s hat was red.\n\nThe advisers left. \
    \u{201c}Advisors,\u{201d} he muttered.\n";

const REPEATED_PROBE: &str =
    "\u{201c}It\u{2019}s a weird problem,\u{201d} she said\u{2014}quietly.\
    \u{2028}It was weird.\n\nThe problem\u{2026} stayed. She definitely knew.\nHe\u{2019}d \
    definitely go.\n\n\u{201c}Definitely,\u{201d} she said. It\u{2019}s weird weird.\n";

const NOUNS: &str = "dog tree river car book chair garden street city lamp bottle cup plate shirt \
    horse apple kitchen window door table house phone mountain forest island bridge castle candle \
    blanket pillow spoon fork knife basket ladder carpet curtain mirror wallet pocket jacket helmet \
    hammer engine wheel tunnel valley meadow village harbor lantern barrel saddle anchor violin \
    piano guitar trumpet tomato potato carrot onion banana lemon cherry peach pear melon cabbage \
    pepper garlic biscuit pastry sandwich elephant giraffe monkey tiger rabbit turtle squirrel \
    donkey camel dolphin parrot pigeon sparrow eagle owl spider beetle ant wasp nephew niece uncle \
    aunt cousin grandmother teacher doctor farmer";

/// Three paragraphs with 7 "cat" of 101 nouns, which only the whole text has.
fn cats_probe() -> String {
    let cats = [
        "\u{201c}I saw the cat\u{2014}the cat\u{2026}\u{201d}",
        "He\u{2019}d seen the cat.",
        "I saw the cat.",
        "I saw the cat.\u{2028}She left.",
        "I saw the cat.",
    ];
    let others: Vec<String> = NOUNS
        .split_whitespace()
        .take(94)
        .map(|noun| format!("I saw the {}.", noun))
        .collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    let first: Vec<&str> = cats[..2]
        .iter()
        .copied()
        .chain(others[..30].iter().copied())
        .collect();
    let second: Vec<&str> = cats[2..4]
        .iter()
        .copied()
        .chain(others[30..60].iter().copied())
        .collect();
    let third: Vec<&str> = cats[4..]
        .iter()
        .copied()
        .chain(others[60..].iter().copied())
        .collect();
    format!(
        "{}\n\n{}\n\n{} She got a free cat.\n",
        first.join(" "),
        second.join("\n"),
        third.join(" ")
    )
}

fn owned(flags: &[(&str, &str, &[&str])]) -> Vec<(String, String, Vec<String>)> {
    flags
        .iter()
        .map(|(text, id, replacements)| {
            (
                (*text).to_owned(),
                (*id).to_owned(),
                replacements.iter().map(|x| (*x).to_owned()).collect(),
            )
        })
        .collect()
}

/// The probes, checked against LanguageTool 6.6 (`-l en-US`; `--level PICKY -e EN_REPEATEDWORDS`
/// for the repeated words, `-e TOO_OFTEN_USED_NOUN_EN` for the nouns).
#[test]
fn matches_languagetool_across_paragraphs() {
    assert_eq!(
        flags(&RULES, COHERENCY_PROBE),
        owned(&[
            ("advisor", "EN_WORD_COHERENCY", &["adviser"]),
            ("Advisors", "EN_WORD_COHERENCY", &["Advisers"]),
        ])
    );
    let starts: Vec<usize> = RULES
        .suggest(COHERENCY_PROBE, &TOKENIZER)
        .iter()
        .filter(|s| s.source() == "BUILTIN/EN_WORD_COHERENCY")
        .map(|s| s.span().char().start)
        .collect();
    assert_eq!(starts, [60, 104]);

    let mut repeated = RULES.clone();
    repeated
        .enable_rules(&["BUILTIN/EN_REPEATEDWORDS".to_owned()])
        .unwrap();
    let problem = ["issue", "concern", "difficulty"];
    assert_eq!(
        flags(&repeated, REPEATED_PROBE),
        owned(&[
            ("problem", "EN_REPEATEDWORDS_PROBLEM", &problem),
            (
                "definitely",
                "EN_REPEATEDWORDS_DEFINITELY",
                &["certainly", "absolutely", "undoubtedly"]
            ),
            (
                "Definitely",
                "EN_REPEATEDWORDS_DEFINITELY",
                &["Certainly", "Absolutely", "Undoubtedly"]
            ),
        ])
    );
    assert_eq!(
        char_starts(&repeated, REPEATED_PROBE, "EN_REPEATEDWORDS"),
        [61, 104, 121]
    );
    // `ENGLISH_WORD_REPEAT_RULE` on "weird weird", which LanguageTool also reports.
    assert_eq!(
        char_starts(&repeated, REPEATED_PROBE, "ENGLISH_WORD_REPEAT_RULE"),
        [149]
    );

    let mut nouns = RULES.clone();
    nouns
        .enable_rules(&["BUILTIN/TOO_OFTEN_USED_NOUN_EN".to_owned()])
        .unwrap();
    let cats = flags(&nouns, &cats_probe());
    assert_eq!(cats.len(), 7);
    assert!(cats
        .iter()
        .all(|(text, id, _)| text == "cat" && id == "TOO_OFTEN_USED_NOUN_EN"));
    // Each paragraph alone has fewer than 100 nouns.
    for paragraph in cats_probe().split("\n\n") {
        assert!(flags(&nouns, paragraph).is_empty());
    }
}

/// An overlapping match of another rule group is reported too, for `EN_REPEATEDWORDS` as in
/// LanguageTool 6.6: `ENGLISH_WORD_REPEAT_RULE` 37..56 and `EN_REPEATEDWORDS_EXCELLENT` 37..46,
/// `ENGLISH_WORD_REPEAT_RULE` 93..104 and `EN_REPEATEDWORDS_WEIRD` 93..98, the second across a
/// blank line.
#[test]
fn repeated_words_are_kept_beside_overlapping_matches() {
    let text = "The food was excellent. The view was excellent excellent.\n\nIt was a weird \
                problem.\n\nThat was weird weird, she said. The problem stayed.\n";
    let mut rules = RULES.clone();
    rules
        .enable_rules(&["BUILTIN/EN_REPEATEDWORDS".to_owned()])
        .unwrap();
    let spans = |id: &str| -> Vec<Range<usize>> {
        rules
            .suggest(text, &TOKENIZER)
            .into_iter()
            .filter(|s| s.source().contains(id))
            .map(|s| s.span().char().clone())
            .collect()
    };
    assert_eq!(spans("ENGLISH_WORD_REPEAT_RULE"), [37..56, 93..104]);
    assert_eq!(spans("EN_REPEATEDWORDS"), [37..46, 93..98]);
    assert_eq!(
        keys(&rules.suggest(text, &TOKENIZER)),
        keys(&incremental(
            &rules,
            &text.split("\n\n").collect::<Vec<_>>(),
            "\n\n",
            true
        ))
    );
}

fn texts() -> Vec<String> {
    let cats = cats_probe();
    vec![
        COHERENCY_PROBE.to_owned(),
        REPEATED_PROBE.to_owned(),
        cats.clone(),
        // Dialogue that a paragraph leaves open, closed two paragraphs later.
        cats.replacen("\n\n", "\n\n\u{201c}I saw the cat.\n\n", 1)
            .replacen("\n\n", "\n\nI saw the cat,\u{201d} she said.\n\n", 2),
        format!(
            "{}\n\nThe advisor\u{2019}s hat. The adviser\u{2019}s coat.",
            cats
        ),
        "He definitely ran.\n\n\n\nShe definitely left.  \n\nThe adviser came.\r\n\r\nThe \
         advisor went. \u{201c}Definitely!\u{201d}\n"
            .to_owned(),
        // The text ends in whitespace after a sentence, which EN_REPEATEDWORDS then skips.
        "He definitely ran.\n\nShe definitely left. ".to_owned(),
        "He definitely ran.\n\nShe definitely left.".to_owned(),
        // An editor's document after Enter at its end: an empty last paragraph.
        "\u{1f600} It was weird.\n\nIt was a weird day and weird.\n\n".to_owned(),
        "It was weird.\u{2028}It was a weird day. It was weird.\n".to_owned(),
        // Spaces after a blank line are a sentence of their own, which ends the text instead.
        "It was weird. It was a weird day\u{2026} It was weird.\n\n  ".to_owned(),
    ]
}

/// The whole text and its paragraphs checked one at a time give the same word-statistics
/// matches, whether the pieces keep their separators or not, and when split at line breaks.
#[test]
fn whole_text_equals_paragraphs() {
    for rules in [&*WORDS, &*NARRATION] {
        for text in texts() {
            let whole = keys(&rules.suggest(&text, &TOKENIZER));
            for separator in ["\n\n", "\n"] {
                let parts: Vec<&str> = text.split_inclusive(separator).collect();
                for ends_paragraph in [false, true] {
                    assert_eq!(
                        whole,
                        keys(&incremental(rules, &parts, "", ends_paragraph)),
                        "{:?} split after {:?}",
                        text,
                        separator
                    );
                }
                // Without the separators, kept by the editor between its paragraphs, also when
                // the last paragraph is empty.
                let parts: Vec<&str> = text.split(separator).collect();
                assert_eq!(
                    whole,
                    keys(&incremental(rules, &parts, separator, true)),
                    "{:?} split at {:?}",
                    text,
                    separator
                );
                // Without the separators, the pieces do not hold the whitespace that ends the
                // text, after which `EN_REPEATEDWORDS` skips the last sentence (as LanguageTool
                // does): the editor's document ends with its last paragraph.
                let stripped = text.trim_end();
                let parts: Vec<&str> = stripped.split(separator).collect();
                assert_eq!(
                    keys(&rules.suggest(stripped, &TOKENIZER)),
                    keys(&incremental(rules, &parts, separator, true)),
                    "{:?} split at {:?}",
                    text,
                    separator
                );
            }
            let (one, _) = rules.check_paragraph(&text, true, &TOKENIZER);
            let (_, digest) = rules.check_paragraph(&text, true, &TOKENIZER);
            let resolved = rules.resolve_document(&[digest]);
            assert_eq!(whole, keys(&resolved.suggestions(0, &text, one)));
        }
    }
    // The probes have matches of every rule.
    let all: Vec<Key> = texts()
        .iter()
        .flat_map(|text| keys(&NARRATION.suggest(text, &TOKENIZER)))
        .collect();
    for id in [
        "EN_WORD_COHERENCY",
        "EN_REPEATEDWORDS_",
        "TOO_OFTEN_USED_NOUN_EN",
    ] {
        assert!(all.iter().any(|(source, ..)| source.contains(id)), "{}", id);
    }
}

/// Direct speech left open at the end of a paragraph goes on in the next, as in LanguageTool,
/// so its words are left out of the counts wherever the text is split.
#[test]
fn direct_speech_continues_across_paragraphs() {
    let open = cats_probe().replacen("\n\n", "\n\n\u{201c}I saw the cat. I saw the cat.\n\n", 1);
    let closed = open.replacen("\n\n", "\n\nI saw the cat,\u{201d} she said.\n\n", 2);
    for text in [open, closed] {
        let parts: Vec<&str> = text.split("\n\n").collect();
        for rules in [&*WORDS, &*NARRATION] {
            assert_eq!(
                keys(&rules.suggest(&text, &TOKENIZER)),
                keys(&incremental(rules, &parts, "\n\n", true))
            );
        }
    }
    // Without the closing quote, everything after the opening one is speech: the nouns of the
    // rest of the text are not counted, so fewer than 100 remain.
    let text = cats_probe().replacen("\n\n", "\n\n\u{201c}I saw the cat.\n\n", 1);
    assert!(char_starts(&NARRATION, &text, "TOO_OFTEN_USED_NOUN_EN").is_empty());
    assert_eq!(
        char_starts(&WORDS, &text, "TOO_OFTEN_USED_NOUN_EN").len(),
        8
    );
}

/// An app keeps one digest per paragraph: after an edit it checks only the edited paragraph and
/// resolves again, which can add or remove matches in paragraphs that did not change.
#[test]
fn an_edit_rechecks_one_paragraph() {
    let text = cats_probe();
    let mut paragraphs: Vec<String> = text.split("\n\n").map(str::to_owned).collect();
    let mut checked: Vec<(Vec<Suggestion>, ParagraphDigest)> = paragraphs
        .iter()
        .map(|p| WORDS.check_paragraph(p, true, &TOKENIZER))
        .collect();

    let resolve = |paragraphs: &[String], checked: &[(Vec<Suggestion>, ParagraphDigest)]| {
        let digests: Vec<ParagraphDigest> = checked.iter().map(|(_, d)| d.clone()).collect();
        let resolved = WORDS.resolve_document(&digests);
        let parts: Vec<&str> = paragraphs.iter().map(String::as_str).collect();
        join(&parts, "\n\n", |i, part| {
            resolved.suggestions(i, part, checked[i].0.clone())
        })
    };
    let cats = |suggestions: &[Suggestion]| {
        suggestions
            .iter()
            .filter(|s| s.source() == "BUILTIN/TOO_OFTEN_USED_NOUN_EN")
            .count()
    };
    let before = resolve(&paragraphs, &checked);
    assert_eq!(keys(&before), keys(&WORDS.suggest(&text, &TOKENIZER)));
    assert_eq!(cats(&before), 7);

    // Two "rat" for "cat" in the last paragraph put "cat" under 5% (5 of 101): the matches in the
    // first two paragraphs go away although they did not change.
    paragraphs[2] = paragraphs[2].replace("cat", "rat");
    checked[2] = WORDS.check_paragraph(&paragraphs[2], true, &TOKENIZER);
    let after = resolve(&paragraphs, &checked);
    let joined = paragraphs.join("\n\n");
    assert_eq!(keys(&after), keys(&WORDS.suggest(&joined, &TOKENIZER)));
    assert_eq!(cats(&after), 0);

    // A spelling variant in the first paragraph flags the other spelling in the last.
    paragraphs[0].insert_str(0, "The adviser came. ");
    paragraphs[2].push_str(" The advisor left.");
    for i in [0, 2] {
        checked[i] = WORDS.check_paragraph(&paragraphs[i], true, &TOKENIZER);
    }
    let edited = resolve(&paragraphs, &checked);
    let joined = paragraphs.join("\n\n");
    assert_eq!(keys(&edited), keys(&WORDS.suggest(&joined, &TOKENIZER)));
    assert!(edited
        .iter()
        .any(|s| s.source() == "BUILTIN/EN_WORD_COHERENCY"
            && joined[s.span().byte().clone()] == *"advisor"));
}

/// Digests cover the word-statistics rules whether they are on or not, and whatever their
/// options, so an app keeps them when the author changes the settings.
#[test]
fn digests_do_not_depend_on_rule_settings() {
    let mut percent = WORDS.clone();
    percent.set_too_often_used_percent(1).unwrap();
    for text in texts() {
        let (_, digest) = RULES.check_paragraph(&text, true, &TOKENIZER);
        for rules in [&*WORDS, &*NARRATION, &percent] {
            assert_eq!(rules.check_paragraph(&text, true, &TOKENIZER).1, digest);
        }
        assert_eq!(
            ParagraphDigest::from_bytes(&digest.to_bytes()).unwrap(),
            digest
        );
    }
}

/// `Rules::apply` and `Builtin::check_text` take their sentences as the whole text.
#[test]
fn one_sentence_is_a_whole_text() {
    let text = "The advisor met the adviser\u{2014}twice\u{2026}";
    let sentence = TOKENIZER.pipe(text).next().unwrap();
    let coherency: Vec<_> = RULES
        .apply(&sentence)
        .into_iter()
        .filter(|s| s.source() == "BUILTIN/EN_WORD_COHERENCY")
        .map(|s| (s.span().char().clone(), s.replacements().to_vec()))
        .collect();
    assert_eq!(coherency, [(20..27, vec!["advisor".to_owned()])]);

    let rule = RULES
        .builtins()
        .iter()
        .find(|rule| rule.id() == "EN_WORD_COHERENCY")
        .unwrap();
    let sentences: Vec<_> = TOKENIZER
        .pipe("He met an adviser.\n\nShe met an advisor.")
        .collect();
    let starts: Vec<usize> = rule
        .builtin()
        .check_text(&sentences, None)
        .iter()
        .map(|s| s.span().char().start)
        .collect();
    assert_eq!(starts, [31]);
}

/// Prints the size of the word-statistics digests of `bench/public/moby150k.txt`, checked
/// paragraph by paragraph, per 1,000 words. Run with `--ignored --nocapture`.
#[test]
#[ignore]
fn word_digest_size_per_thousand_words() {
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
    let matches = NARRATION.resolve_document(&digests).matches.len();
    let start = std::time::Instant::now();
    let resolved = NARRATION.resolve_document_bytes(
        &digests
            .iter()
            .map(ParagraphDigest::to_bytes)
            .collect::<Vec<_>>(),
    );
    let elapsed = start.elapsed();
    assert_eq!(resolved.matches.len(), matches);
    println!(
        "{} words, {} paragraphs, {} matches: {} bytes ({:.0} per 1,000 words); resolving the \
         bytes takes {:.1} ms",
        words,
        digests.len(),
        matches,
        bytes,
        bytes as f64 * 1000.0 / words as f64,
        elapsed.as_secs_f64() * 1000.0
    );
}

/// Changed digest bytes count as stale or resolve to some matches, and never panic: the resolver
/// trusts the spans, word numbers and sentence starts that `ParagraphDigest::from_bytes` lets
/// through.
#[test]
fn changed_digest_bytes_never_panic() {
    let texts = [
        "\u{1f600} It was weird. It\u{2019}s a weird problem\u{2014}she said \u{201c}weird\u{201d}\u{2026}\n\n",
        "The advisor\u{2019}s problem was weird.\u{2028}The adviser came. ",
        "Problem.",
    ];
    let digests: Vec<Vec<u8>> = texts
        .iter()
        .map(|text| WORDS.check_paragraph(text, true, &TOKENIZER).1.to_bytes())
        .collect();
    for which in 0..digests.len() {
        for i in 4..digests[which].len() {
            for value in [0u8, 1, 2, 3, 9, 0x7f, 0x80, 0xfb, 0xff] {
                let mut changed = digests.clone();
                changed[which][i] = value;
                WORDS.resolve_document_bytes(&changed);
            }
        }
    }
}
