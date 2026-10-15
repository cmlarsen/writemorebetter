use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::cliches::{
    bundled_phrase_entries, cliches, suppressed_phrases, vendored_phrase_entries, ClicheReport,
    PhraseContext, PhraseEntry, PhraseKind, PhraseList,
};
use writemorebetter_style::{analyze_document, DocumentOptions, Span};

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn bundled() -> &'static PhraseList {
    static LIST: OnceLock<PhraseList> = OnceLock::new();
    LIST.get_or_init(|| PhraseList::bundled(tokenizer()))
}

fn report(text: &str, options: &DocumentOptions) -> ClicheReport {
    cliches(&analyze_document(tokenizer(), text, options), bundled())
}

fn assert_span(text: &str, span: &Span, expected: &str) {
    assert_eq!(text.get(span.byte.clone()), Some(expected));
    assert_eq!(
        text.chars()
            .skip(span.char.start)
            .take(span.char.len())
            .collect::<String>(),
        expected
    );
    assert_eq!(
        String::from_utf16(
            &text
                .encode_utf16()
                .skip(span.utf16.start)
                .take(span.utf16.len())
                .collect::<Vec<_>>()
        )
        .unwrap(),
        expected
    );
}

fn found(text: &str, options: &DocumentOptions) -> Vec<(String, String, PhraseContext)> {
    report(text, options)
        .matches
        .into_iter()
        .map(|found| {
            assert_span(
                text,
                &found.span,
                text.get(found.span.byte.clone()).unwrap(),
            );
            (
                found.phrase,
                text.get(found.span.byte).unwrap().to_owned(),
                found.context,
            )
        })
        .collect()
}

fn phrases(text: &str) -> Vec<(String, String)> {
    found(text, &DocumentOptions::new())
        .into_iter()
        .map(|(phrase, surface, _)| (phrase, surface))
        .collect()
}

fn pair(phrase: &str, surface: &str) -> (String, String) {
    (phrase.to_owned(), surface.to_owned())
}

#[test]
fn inflected_forms_match_by_lemma() {
    assert_eq!(
        phrases("The old sailor kicked the bucket. She bites the dust. They were biting the dust."),
        vec![
            pair("kick the bucket", "kicked the bucket"),
            pair("bite the dust", "bites the dust"),
            pair("bite the dust", "biting the dust"),
        ]
    );
    assert_eq!(
        phrases("Two cock and bull stories."),
        vec![pair("cock and bull story", "cock and bull stories")]
    );
}

#[test]
fn phrase_across_a_line_wrap() {
    let text = "Her refusal only added insult to\ninjury, and he left.\n\nNext.";
    let hard = DocumentOptions::new().with_hard_wrapped(true);
    let matches = found(text, &hard);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1, "added insult to\ninjury");
    assert!(found(text, &DocumentOptions::new()).is_empty());
    assert_eq!(
        phrases("They arrived in the nick\u{2028}of time."),
        vec![pair("in the nick of time", "in the nick\u{2028}of time")]
    );
}

#[test]
fn false_positive_guards() {
    for text in [
        "The pin number was painted on the skittle.",
        "He kicked the old bucket across the yard.",
        "That was the insult. To injury he added nothing.",
        "She was three years younger than her sister.",
        "Sailors spoke of it in their dreams.",
        "He sat on the rocking chair.",
        "She walked around the bush in the garden.",
        "He put all his eggs in her basket.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
    assert_eq!(
        phrases("She put all her eggs in one basket."),
        vec![pair(
            "all your eggs in one basket",
            "all her eggs in one basket"
        )]
    );
    assert_eq!(
        phrases("In your dreams, he thought."),
        vec![pair("in your dreams", "In your dreams")]
    );
}

#[test]
fn possessive_personal_and_reflexive_slots() {
    assert_eq!(
        phrases("She aired her dirty laundry. He aired the captain’s dirty laundry."),
        vec![
            pair("air your dirty laundry", "aired her dirty laundry"),
            pair(
                "air your dirty laundry",
                "aired the captain’s dirty laundry"
            ),
        ]
    );
    assert_eq!(
        phrases("We will cross that bridge when we come to it."),
        vec![pair(
            "cross that bridge when you come to it",
            "cross that bridge when we come to it"
        )]
    );
    assert_eq!(
        phrases("By then she was at her wits’ end."),
        vec![pair("at my wits end", "at her wits’ end")]
    );
}

#[test]
fn skipped_names_fill_slots_but_never_words() {
    let options = DocumentOptions::new().with_skip_names(vec!["Mara".into(), "Jack".into()]);
    let text =
        "He aired Mara’s dirty laundry. Jack of all trades, they called him; a jack of all trades.";
    let matches: Vec<_> = found(text, &options)
        .into_iter()
        .map(|(phrase, surface, _)| (phrase, surface))
        .collect();
    assert!(matches.contains(&pair(
        "air your dirty laundry",
        "aired Mara’s dirty laundry"
    )));
    assert!(!matches.iter().any(|(_, surface)| surface.contains("Jack")));
    assert!(matches.contains(&pair("Jack of all trades", "jack of all trades")));
}

#[test]
fn optional_leading_article() {
    assert_eq!(
        phrases("It was a far cry from home. It was far cry. Such a clean slate."),
        vec![
            pair("a far cry", "a far cry"),
            pair("a far cry", "far cry"),
            pair("a clean slate", "a clean slate"),
        ]
    );
}

#[test]
fn hyphens_and_punctuation_inside_phrases() {
    assert_eq!(
        phrases("He was caught red handed. A bird’s-eye view. Cool, calm and collected."),
        vec![
            pair("caught red-handed", "caught red handed"),
            pair("bird's eye view", "bird’s-eye view"),
            pair("cool, calm, and collected", "Cool, calm and collected"),
        ]
    );
}

#[test]
fn typography() {
    let text = "“Don’t count your chickens,” she said—under the weather—and \
                ‘better safe than sorry,’ he said. It was the tip of the iceberg…\u{2028}\
                Then: “ATM teller machines… ABS braking systems.”";
    let matches = found(text, &DocumentOptions::new());
    let expected = [
        (
            "don't count your chickens",
            "Don’t count your chickens",
            PhraseContext::Dialogue,
        ),
        (
            "under the weather",
            "under the weather",
            PhraseContext::Narration,
        ),
        (
            "better safe than sorry",
            "better safe than sorry",
            PhraseContext::Dialogue,
        ),
        (
            "tip of the iceberg",
            "tip of the iceberg",
            PhraseContext::Narration,
        ),
        (
            "ATM teller machine",
            "ATM teller machines",
            PhraseContext::Dialogue,
        ),
        (
            "ABS braking system",
            "ABS braking systems",
            PhraseContext::Dialogue,
        ),
    ];
    assert_eq!(
        matches,
        expected
            .iter()
            .map(|(phrase, surface, context)| (phrase.to_string(), surface.to_string(), *context))
            .collect::<Vec<_>>()
    );
}

#[test]
fn narration_and_dialogue_are_counted_separately() {
    let text = "He kicked the bucket. “Better safe than sorry,” she said. The GPS system failed.";
    let result = report(text, &DocumentOptions::new());
    assert_eq!(result.narration.cliches, 1);
    assert_eq!(result.narration.redundancies, 1);
    assert_eq!(result.dialogue.cliches, 1);
    assert_eq!(result.dialogue.redundancies, 0);
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    assert_eq!(
        result.narration.words + result.dialogue.words,
        document.word_count
    );
    assert_eq!(result.dialogue.words, 4);
}

#[test]
fn compact_documents_give_the_same_matches() {
    let text = "“She aired her dirty laundry,” he said. They kicked the bucket.";
    let full = report(text, &DocumentOptions::new());
    let compact = report(text, &DocumentOptions::new().with_token_details(false));
    assert_eq!(full, compact);
    assert_eq!(full.matches.len(), 2);
}

#[test]
fn acronyms_must_be_capitalized() {
    assert_eq!(
        phrases("The UPC codes and the upc codes."),
        vec![pair("UPC code", "UPC codes")]
    );
}

#[test]
fn longer_matches_absorb_contained_ones() {
    assert_eq!(
        phrases("He was a chip off the old block."),
        vec![pair("a chip off the old block", "a chip off the old block")]
    );
}

#[test]
fn entries_already_covered_by_grammar_rules_are_dropped() {
    let suppressed = suppressed_phrases();
    assert!(suppressed.contains(&"ATM machine"));
    let vendored = vendored_phrase_entries();
    let bundled = bundled_phrase_entries();
    assert!(vendored.iter().any(|entry| entry.phrase == "ATM machine"));
    assert!(!bundled
        .iter()
        .any(|entry| suppressed.contains(&entry.phrase.as_str())));
    assert_eq!(vendored.len(), bundled.len() + suppressed.len());
    assert!(phrases("The ATM machines were empty.").is_empty());
}

#[test]
fn bundled_lists_are_complete() {
    let entries = bundled_phrase_entries();
    let count = |source: &str, kind: PhraseKind| {
        entries
            .iter()
            .filter(|entry| entry.source == source && entry.kind == kind)
            .count()
    };
    assert_eq!(count("no-cliches", PhraseKind::Cliche), 697);
    assert_eq!(count("proselint", PhraseKind::Cliche), 1);
    assert!(count("retext-redundant-acronyms", PhraseKind::Redundancy) > 100);
    assert_eq!(vendored_phrase_entries().len(), 889);
    assert_eq!(entries.len(), 882);
    assert_eq!(bundled().len(), entries.len());
}

#[test]
fn guard_and_slot_lists_name_vendored_entries() {
    let normalize = |text: &str| text.replace(['’', '‘'], "'").to_lowercase();
    let vendored: Vec<_> = vendored_phrase_entries()
        .iter()
        .map(|entry| normalize(&entry.phrase))
        .collect();
    for data in [
        include_str!("../data/fixed-wording.txt"),
        include_str!("../data/literal-guards.txt"),
        include_str!("../data/open-slot.txt"),
        include_str!("../data/suppressed.txt"),
    ] {
        for line in data.lines().map(str::trim) {
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let phrase = normalize(line.split('\t').next().unwrap());
            assert!(vendored.contains(&phrase), "{phrase}");
        }
    }
}

#[test]
fn callers_can_supply_their_own_list() {
    let list = PhraseList::new(
        tokenizer(),
        vec![PhraseEntry::new(
            PhraseKind::Redundancy,
            "past history",
            "custom",
        )],
    );
    let text = "Their past histories were long.";
    let result = cliches(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        &list,
    );
    assert_eq!(result.matches.len(), 1);
    assert_eq!(result.matches[0].source, "custom");
    assert_span(text, &result.matches[0].span, "past histories");
}

#[test]
fn irregular_and_suppletive_lemmas_do_not_match() {
    for text in [
        "A good half of the crowd left.",
        "They waited a good half hour.",
        "She woke from a bad nightmare.",
        "Every fiber of her was screaming.",
        "He was worse to the bone.",
        "Well safe than sorry, she thought.",
        "Good late than never.",
        "They were shooting in the dark at shadows.",
        "He kept his ear to the grinding stone.",
        "It was easier said than did.",
        "It was better as gold. Best as gold.",
        "A chip off the older block. As older as time.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
    assert_eq!(
        phrases("His better half stayed. It was her worst nightmare. Every fiber of my being."),
        vec![
            pair("better half", "better half"),
            pair("worst nightmare", "worst nightmare"),
            pair("every fiber of my being", "Every fiber of my being"),
        ]
    );
    assert_eq!(
        phrases("It went without saying. It goes without saying."),
        vec![pair("it goes without saying", "It goes without saying")]
    );
}

#[test]
fn open_slot_entries_accept_the_or_nothing() {
    assert_eq!(
        phrases(
            "She aired the dirty laundry. She aired dirty laundry. They aired the family's dirty \
             laundry. He aired her own dirty laundry. They aired the Joneses’ dirty laundry."
        ),
        vec![
            pair("air your dirty laundry", "aired the dirty laundry"),
            pair("air your dirty laundry", "aired dirty laundry"),
            pair("air your dirty laundry", "aired the family's dirty laundry"),
            pair("air your dirty laundry", "aired her own dirty laundry"),
            pair("air your dirty laundry", "aired the Joneses’ dirty laundry"),
        ]
    );
    let options = DocumentOptions::new().with_skip_names(vec!["Mara".into(), "Quinn".into()]);
    let text = "He would go Mara Quinn one better.";
    assert_eq!(
        found(text, &options),
        vec![(
            "go him one better".to_owned(),
            "go Mara Quinn one better".to_owned(),
            PhraseContext::Narration
        )]
    );
}

#[test]
fn other_slots_need_a_possessive() {
    for text in [
        "Hold the horses while I check the trail.",
        "He held the horses at the ford.",
        "She flipped the lid of the box open.",
        "The dog was not in the backyard.",
        "Two spinning wheels stood by the hearth.",
        "He was yanking the chain of the anchor.",
        "Every evening she gets the goat in from the field.",
        "Don't burn the bridges until the army has crossed.",
        "She put a foot down to steady herself.",
        "The nurse licked the wounds clean.",
        "He was pulling the leg of the table.",
        "She burst the bubble with a pin.",
        "The birds feather the nest with down.",
        "Don't count the chickens until the fox is gone.",
        "Eat that words.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
    assert_eq!(
        phrases("Hold your horses. She flipped the captain's lid. He was spinning his wheels."),
        vec![
            pair("hold your horses", "Hold your horses"),
            pair("flip your lid", "flipped the captain's lid"),
            pair("spinning your wheels", "spinning his wheels"),
        ]
    );
}

#[test]
fn own_in_the_entry_is_not_taken_by_the_slot() {
    assert_eq!(
        phrases(
            "He liked to blow his own horn. Blow your own horn. She held her own. He could hold \
             his own in a fight. To each his own. To each their own, she said."
        ),
        vec![
            pair("blow your own horn", "blow his own horn"),
            pair("blow your own horn", "Blow your own horn"),
            pair("hold your own", "held her own"),
            pair("hold your own", "hold his own"),
            pair("to each his own", "To each his own"),
            pair("to each his own", "To each their own"),
        ]
    );
    for text in [
        "He blew own horn.",
        "He blew the own horn.",
        "She held the own.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
}

#[test]
fn wits_end_accepts_the_singular_possessive() {
    assert_eq!(
        phrases("He was at his wit's end. At my wit’s end. At their wits’ end. At wits end."),
        vec![
            pair("at my wits end", "at his wit's end"),
            pair("at my wits end", "At my wit’s end"),
            pair("at my wits end", "At their wits’ end"),
            pair("at my wits end", "At wits end"),
        ]
    );
}

#[test]
fn literal_body_part_uses_are_guarded() {
    for text in [
        "She let her hair down from its pins and brushed it.",
        "She put her foot down on the brake.",
        "The dog lay in the straw and licked its wounds.",
        "The cat landed on its feet.",
        "He kicked the bucket of water across the yard.",
        "She put her foot down on the brakes.",
        "He put his foot down onto the deck.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
    assert_eq!(
        phrases("At last she let her hair down. He put his foot down. He landed on his feet."),
        vec![
            pair("let your hair down", "let her hair down"),
            pair("put your foot down", "put his foot down"),
            pair("land on your feet", "landed on his feet"),
        ]
    );
    assert_eq!(
        phrases(
            "Old Tom kicked the bucket over Christmas. She put her foot down on that plan. He put \
             his foot down on the matter of the dog."
        ),
        vec![
            pair("kick the bucket", "kicked the bucket"),
            pair("put your foot down", "put her foot down"),
            pair("put your foot down", "put his foot down"),
        ]
    );
}

#[test]
fn caller_entries_naming_its_keep_it() {
    let list = PhraseList::new(
        tokenizer(),
        vec![PhraseEntry::new(
            PhraseKind::Cliche,
            "rear its ugly head",
            "custom",
        )],
    );
    let text = "The problem reared its ugly head.";
    let result = cliches(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        &list,
    );
    assert_eq!(result.matches.len(), 1);
    assert_span(text, &result.matches[0].span, "reared its ugly head");
}

#[test]
fn matches_stay_inside_one_sentence() {
    for text in [
        "It was all talk… No action came.",
        "It was all talk. No action came.",
        "He kicked the… Bucket was full.",
        "He was caught. Red handed, she said.",
    ] {
        assert!(phrases(text).is_empty(), "{text}");
    }
    assert_eq!(
        phrases("It was all talk, no action."),
        vec![pair("all talk, no action", "all talk, no action")]
    );
}

#[test]
fn lookback_takes_only_articles_and_possessives() {
    assert_eq!(
        phrases("He said that far cry was nothing. It was his far cry."),
        vec![
            pair("a far cry", "far cry"),
            pair("a far cry", "his far cry")
        ]
    );
}

type Expected = (&'static str, &'static str, PhraseContext);

#[test]
fn review_adversarial_cases() {
    let plain = DocumentOptions::new();
    let hard = DocumentOptions::new().with_hard_wrapped(true);
    let names = DocumentOptions::new().with_skip_names(vec!["Bucket".into()]);
    let cases: &[(&str, &DocumentOptions, &[Expected])] = &[
        (
            "He kicks the bucket. They are kicking the bucket. He has kicked the bucket.",
            &plain,
            &[
                (
                    "kick the bucket",
                    "kicks the bucket",
                    PhraseContext::Narration,
                ),
                (
                    "kick the bucket",
                    "kicking the bucket",
                    PhraseContext::Narration,
                ),
                (
                    "kick the bucket",
                    "kicked the bucket",
                    PhraseContext::Narration,
                ),
            ],
        ),
        (
            "KICKED THE BUCKET. Kicked The Bucket.",
            &plain,
            &[
                (
                    "kick the bucket",
                    "KICKED THE BUCKET",
                    PhraseContext::Narration,
                ),
                (
                    "kick the bucket",
                    "Kicked The Bucket",
                    PhraseContext::Narration,
                ),
            ],
        ),
        ("He kicked the\nbucket.", &plain, &[]),
        (
            "He kicked the\nbucket.",
            &hard,
            &[(
                "kick the bucket",
                "kicked the\nbucket",
                PhraseContext::Narration,
            )],
        ),
        (
            "He kicked the\r\nbucket.",
            &hard,
            &[(
                "kick the bucket",
                "kicked the\r\nbucket",
                PhraseContext::Narration,
            )],
        ),
        ("He kicked the\u{2029}bucket.", &plain, &[]),
        ("He kicked the\n\nbucket.", &hard, &[]),
        (
            "He kicked the\u{a0}bucket.",
            &plain,
            &[(
                "kick the bucket",
                "kicked the\u{a0}bucket",
                PhraseContext::Narration,
            )],
        ),
        (
            "He kicked\tthe  bucket.",
            &plain,
            &[(
                "kick the bucket",
                "kicked\tthe  bucket",
                PhraseContext::Narration,
            )],
        ),
        (
            "He kicked the—bucket. He was under the — weather.",
            &plain,
            &[],
        ),
        (
            "He kicked the - bucket.",
            &plain,
            &[(
                "kick the bucket",
                "kicked the - bucket",
                PhraseContext::Narration,
            )],
        ),
        (
            "All in a day’s work. All in a day's work. All in a days work.",
            &plain,
            &[
                (
                    "all in a day's work",
                    "All in a day’s work",
                    PhraseContext::Narration,
                ),
                (
                    "all in a day's work",
                    "All in a day's work",
                    PhraseContext::Narration,
                ),
            ],
        ),
        (
            "The Gps system. The GPS System. THE GPS SYSTEM FAILED. The GPSs system.",
            &plain,
            &[
                ("GPS system", "GPS System", PhraseContext::Narration),
                ("GPS system", "GPS SYSTEM", PhraseContext::Narration),
            ],
        ),
        (
            "“The PIN numbers,” she said. “ATM teller machines!”",
            &plain,
            &[(
                "ATM teller machine",
                "ATM teller machines",
                PhraseContext::Dialogue,
            )],
        ),
        (
            "He had, as she put it, “kicked the bucket.” Then he “kicked” the bucket.",
            &plain,
            &[(
                "kick the bucket",
                "kicked the bucket",
                PhraseContext::Dialogue,
            )],
        ),
        (
            "“Better safe\u{2028}than sorry,” she said.",
            &plain,
            &[(
                "better safe than sorry",
                "Better safe\u{2028}than sorry",
                PhraseContext::Dialogue,
            )],
        ),
        ("They kicked the Bucket.", &names, &[]),
        (
            "It was a kick-the-bucket moment.",
            &plain,
            &[(
                "kick the bucket",
                "kick-the-bucket",
                PhraseContext::Narration,
            )],
        ),
        (
            "It was the calm before the storm before the storm.",
            &plain,
            &[(
                "calm before the storm",
                "calm before the storm",
                PhraseContext::Narration,
            )],
        ),
        (
            "Every dog has its days. Busier than a bee. As busier as a bee. As busy as a bee.",
            &plain,
            &[
                (
                    "every dog has its day",
                    "Every dog has its days",
                    PhraseContext::Narration,
                ),
                ("busy as a bee", "busy as a bee", PhraseContext::Narration),
            ],
        ),
        ("He kicked “the bucket”.", &plain, &[]),
    ];
    for (text, options, expected) in cases {
        let expected: Vec<_> = expected
            .iter()
            .map(|(phrase, surface, context)| (phrase.to_string(), surface.to_string(), *context))
            .collect();
        assert_eq!(found(text, options), expected, "{text:?}");
    }
}
