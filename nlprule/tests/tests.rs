use std::convert::TryInto;

use lazy_static::lazy_static;
use nlprule::{rule::id::Category, types::Position, Error, Rules, Tokenizer};
use quickcheck_macros::quickcheck;

const TOKENIZER_PATH: &str = "../storage/en_tokenizer.bin";
const RULES_PATH: &str = "../storage/en_rules.bin";

lazy_static! {
    static ref TOKENIZER: Tokenizer = Tokenizer::new(TOKENIZER_PATH).unwrap();
    static ref RULES: Rules = Rules::new(RULES_PATH).unwrap();
}

#[test]
fn can_tokenize_empty_text() {
    let sentences: Vec<_> = TOKENIZER.pipe("").collect();
    assert!(sentences.is_empty());
}

#[test]
fn handles_whitespace_correctly() {
    // preceding whitespace has to be included, trailing whitespace behavior is unspecified
    let text = "  hello.\ttest.\t\t";

    let mut sentences = TOKENIZER.pipe(text);
    assert_eq!(
        &text[sentences.next().unwrap().span().byte().clone()],
        "  hello.\t"
    );
    assert_eq!(
        &text[sentences.next().unwrap().span().byte().clone()],
        "test.\t"
    );
    assert_eq!(sentences.next(), None);
}

#[quickcheck]
fn can_tokenize_anything(text: String) -> bool {
    let _: Vec<_> = TOKENIZER.pipe(&text).collect();
    true
}

#[test]
fn suggest_indices_are_relative_to_input_text() {
    let suggestions = RULES.suggest(
        "I can due his homework for 10€. I can due his homework.",
        &*TOKENIZER,
    );

    assert_eq!(*suggestions[0].span().char(), 6..9);
    assert_eq!(*suggestions[0].span().byte(), 6..9);

    assert_eq!(*suggestions[1].span().char(), 38..41);
    assert_eq!(
        *suggestions[1].span().byte(),
        38 + '€'.len_utf8() - 1..41 + '€'.len_utf8() - 1
    );
}

#[test]
fn sentence_spans_correct() {
    let text = "A short test. A test with emoji 😊.";

    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    assert_eq!(sentences.len(), 2);

    assert_eq!(*sentences[0].span().char(), 0..14);
    assert_eq!(*sentences[0].span().byte(), 0..14);

    assert_eq!(*sentences[1].span().char(), 14..34);
    assert_eq!(*sentences[1].span().byte(), 14..37);
}

#[test]
fn token_spans_correct() {
    let text = "A short test. A test with emoji 😊.";

    let tokens: Vec<_> = TOKENIZER
        .pipe(text)
        .map(|x| x.into_iter())
        .flatten()
        .collect();
    assert_eq!(*tokens[0].span().byte(), 0..1);
    assert_eq!(*tokens[0].span().char(), 0..1);

    assert_eq!(*tokens[2].span().char(), 8..12);
    assert_eq!(*tokens[2].span().byte(), 8..12);

    assert_eq!(*tokens[tokens.len() - 2].span().char(), 32..33);
    assert_eq!(*tokens[tokens.len() - 2].span().byte(), 32..36);

    assert_eq!(*tokens[tokens.len() - 1].span().char(), 33..34);
    assert_eq!(*tokens[tokens.len() - 1].span().byte(), 36..37);
}

#[quickcheck]
fn no_gaps_between_sentences(text: String) {
    let mut prev_pos = Position::default();
    let mut contains_sentence = false;

    for sentence in TOKENIZER.pipe(&text) {
        assert_eq!(sentence.span().start(), prev_pos);
        prev_pos += sentence.span().len();

        contains_sentence = true;
    }

    assert_eq!(contains_sentence, !text.trim().is_empty());
}

#[test]
fn rules_can_be_disabled_enabled() {
    let mut rules = Rules::new(RULES_PATH).unwrap();

    // enabled by default
    assert!(!rules
        .suggest("I can due his homework", &*TOKENIZER)
        .is_empty());

    rules
        .select_mut(
            &Category::new("confused_words")
                .join("confusion_due_do")
                .into(),
        )
        .for_each(|x| x.disable());

    // disabled now
    assert!(rules
        .suggest("I can due his homework", &*TOKENIZER)
        .is_empty());

    // disabled by default
    assert!(rules.suggest("I can not go", &*TOKENIZER).is_empty());

    rules
        .select_mut(&"typos/can_not".try_into().unwrap())
        .for_each(|x| x.enable());

    // enabled now
    assert!(!rules.suggest("I can not go", &*TOKENIZER).is_empty());
}

#[test]
fn spellchecker_works() -> Result<(), Error> {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_GB")?);

    assert_eq!(
        rules.correct("color spellhceking", &*TOKENIZER),
        "colour spellchecking"
    );

    Ok(())
}

fn spelling_errors(text: &str) -> Vec<&str> {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());

    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| &text[x.span().byte().clone()])
        .collect()
}

#[test]
fn spellchecker_accepts_fiction_punctuation() {
    for text in &[
        "\u{201c}I didn\u{2019}t know,\u{201d} she said. \u{201c}Weren\u{2019}t you there?\u{201d}",
        "\u{2018}Doesn\u{2019}t matter,\u{2019} he said.",
        "He opened the door\u{2014}then stopped. It was quiet\u{2014}too quiet.",
        "Pages 10\u{2013}12 were missing\u{2013}or torn out.",
        "She waited\u{2026} and waited.",
        "She waited.\u{2028}Nothing came.",
    ] {
        assert_eq!(spelling_errors(text), Vec::<&str>::new(), "{}", text);
    }
}

#[test]
fn spellchecker_flags_misspellings_next_to_fiction_punctuation() {
    assert_eq!(
        spelling_errors(
            "\u{201c}The recieved letter,\u{201d} she said\u{2026} \u{2018}teh end.\u{2019}"
        ),
        vec!["recieved", "teh"]
    );
    assert_eq!(
        spelling_errors("Nothing.\u{2028}Patenice\u{2014}always."),
        vec!["Patenice\u{2014}always"]
    );
}

/// Runs the English word-repeat builtin directly on a single sentence and
/// returns the flagged text and the first replacement of every suggestion.
fn builtin_word_repeat_flags(text: &str) -> Vec<(String, String)> {
    let sentence = TOKENIZER
        .pipe(text)
        .next()
        .expect("test input contains a sentence");
    let chars: Vec<char> = text.chars().collect();

    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check(&sentence))
        .filter(|suggestion| suggestion.source() == "BUILTIN/ENGLISH_WORD_REPEAT_RULE")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements()[0].clone(),
            )
        })
        .collect()
}

#[test]
fn builtin_word_repeat_flags_simple() {
    assert_eq!(
        builtin_word_repeat_flags("We carried it into into the shed."),
        vec![("into into".to_string(), "into".to_string())]
    );
}

#[test]
fn builtin_word_repeat_flags_curly_quotes() {
    assert_eq!(
        builtin_word_repeat_flags("\u{201c}The the door,\u{201d} she said."),
        vec![("The the".to_string(), "The".to_string())]
    );
}

#[test]
fn builtin_word_repeat_flags_curly_apostrophe() {
    assert_eq!(
        builtin_word_repeat_flags("I don\u{2019}t don\u{2019}t know."),
        vec![(
            "don\u{2019}t don\u{2019}t".to_string(),
            "don\u{2019}t".to_string()
        )]
    );
}

#[test]
fn builtin_word_repeat_flags_before_ellipsis() {
    assert_eq!(
        builtin_word_repeat_flags("It was was late\u{2026}"),
        vec![("was was".to_string(), "was".to_string())]
    );
}

#[test]
fn builtin_word_repeat_flags_across_u2028() {
    // U+2028 is whitespace inside a paragraph, so the words are still repeated.
    assert_eq!(
        builtin_word_repeat_flags("the\u{2028}the end"),
        vec![("the\u{2028}the".to_string(), "the".to_string())]
    );
}

#[test]
fn builtin_word_repeat_ignores_had_had_exception() {
    assert!(builtin_word_repeat_flags("He had had enough.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_that_that_exception() {
    assert!(builtin_word_repeat_flags("I know that that is true.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_numbers() {
    // The English rule skips tokens that are numeric, so "10 10" is allowed.
    assert!(builtin_word_repeat_flags("It cost 10 10 dollars.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_em_dash() {
    // "the—the" is a single token, so it is not a repetition of two words.
    assert!(builtin_word_repeat_flags("the\u{2014}the").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_punctuation_between() {
    // The comma is a token between the two occurrences of "bye".
    assert!(builtin_word_repeat_flags("Bye, bye.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_quotes_between() {
    // Following LanguageTool, "“no” “no”" is not flagged: the closing and
    // opening quotes are tokens between the two occurrences of "no".
    assert!(builtin_word_repeat_flags("\u{201c}no\u{201d} \u{201c}no\u{201d}").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_three_or_more() {
    // LanguageTool only reports the first two of a longer run.
    assert!(builtin_word_repeat_flags("It was was was late.").is_empty());
    assert!(builtin_word_repeat_flags("I had had had enough.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_spelled_out_letters() {
    // "b a s i c a l l y" must not be flagged; here the pair "c c" followed by
    // another letter is suppressed by the spelled-out-letters exception.
    assert!(builtin_word_repeat_flags("b a c c a").is_empty());
}

#[test]
fn builtin_word_repeat_is_wired_into_suggest() {
    let suggestions = RULES.suggest("We carried it into into the shed.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/ENGLISH_WORD_REPEAT_RULE")
        .expect("repeated word is flagged");
    assert_eq!(flagged.replacements(), &["into".to_string()]);
}

#[test]
fn builtin_word_repeat_span_covers_case_folded_second_word() {
    assert_eq!(
        builtin_word_repeat_flags("They lived on the STRASSE straße for years."),
        vec![("STRASSE straße".to_string(), "STRASSE".to_string())]
    );
}

/// Runs the English a/an builtin through the full suggestion pipeline and
/// returns the flagged text and the first replacement of every suggestion.
fn builtin_a_vs_an_flags(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();

    RULES
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/EN_A_VS_AN")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements()[0].clone(),
            )
        })
        .collect()
}

#[test]
fn builtin_a_vs_an_flags_a_apple() {
    assert_eq!(
        builtin_a_vs_an_flags("She ate a apple."),
        vec![("a apple".to_string(), "an".to_string())]
    );
}

#[test]
fn builtin_a_vs_an_flags_a_before_honest() {
    assert_eq!(
        builtin_a_vs_an_flags("He is a honest man."),
        vec![("a honest".to_string(), "an".to_string())]
    );
    assert!(builtin_a_vs_an_flags("He is an honest man.").is_empty());
}

#[test]
fn builtin_a_vs_an_flags_an_before_consonant() {
    assert_eq!(
        builtin_a_vs_an_flags("They met an year ago."),
        vec![("an year".to_string(), "a".to_string())]
    );
}

#[test]
fn builtin_a_vs_an_accepts_an_hour() {
    assert!(builtin_a_vs_an_flags("We waited an hour.").is_empty());
}

#[test]
fn builtin_a_vs_an_accepts_a_university() {
    // "university" starts with a vowel letter but a consonant sound.
    assert!(builtin_a_vs_an_flags("He attends a university.").is_empty());
}

#[test]
fn builtin_a_vs_an_accepts_an_mba() {
    // "MBA" is an abbreviation listed as requiring "an".
    assert!(builtin_a_vs_an_flags("She has an MBA.").is_empty());
    assert_eq!(
        builtin_a_vs_an_flags("She has a MBA."),
        vec![("a MBA".to_string(), "an".to_string())]
    );
}

#[test]
fn builtin_a_vs_an_accepts_a_one_time() {
    // Only the part before the hyphen counts; "one" takes "a".
    assert!(builtin_a_vs_an_flags("It was a one-time thing.").is_empty());
}

#[test]
fn builtin_a_vs_an_ignores_unknown_abbreviations() {
    // All-uppercase words are assumed to be abbreviations with unknown
    // pronunciation, so no article is enforced.
    assert!(builtin_a_vs_an_flags("He plugged in a USB cable.").is_empty());
}

#[test]
fn builtin_a_vs_an_flags_capitalized_article_at_sentence_start() {
    assert_eq!(
        builtin_a_vs_an_flags("A apple fell."),
        vec![("A apple".to_string(), "An".to_string())]
    );
}

#[test]
fn builtin_a_vs_an_accepts_capitalized_an() {
    assert!(builtin_a_vs_an_flags("An honest man arrived.").is_empty());
}

#[test]
fn builtin_a_vs_an_flags_inside_curly_quotes() {
    assert_eq!(
        builtin_a_vs_an_flags("He was a \u{201c}honest\u{201d} man."),
        vec![("a \u{201c}honest".to_string(), "an".to_string())]
    );
    // A correct article inside the quotes is left alone.
    assert!(builtin_a_vs_an_flags("It took an \u{201c}hour\u{201d} or so\u{2026}").is_empty());
}

#[test]
fn builtin_a_vs_an_flags_before_em_dash_word() {
    // The em dash joins two words into one token; the article still refers to
    // the "a"-initial word.
    assert_eq!(
        builtin_a_vs_an_flags("He gave me a apple\u{2014}then left."),
        vec![("a apple\u{2014}then".to_string(), "an".to_string())]
    );
}

#[test]
fn builtin_a_vs_an_ignores_correct_prose_with_fiction_punctuation() {
    // Curly double quotes, curly apostrophes, an em dash and an ellipsis, all
    // correctly punctuated: no flags.
    assert!(builtin_a_vs_an_flags(
        "Marta\u{2019}s an honest friend\u{2014}then she left\u{2026} \u{201c}An hour,\u{201d} she said."
    )
    .is_empty());
}

#[test]
fn builtin_a_vs_an_checks_across_u2028() {
    // U+2028 separates two sentences inside one paragraph; each is checked.
    assert!(builtin_a_vs_an_flags("It was a one-time thing.\u{2028}An honest mistake.").is_empty());
    assert_eq!(
        builtin_a_vs_an_flags("It was a apple.\u{2028}An year passed."),
        vec![
            ("a apple".to_string(), "an".to_string()),
            ("An year".to_string(), "A".to_string()),
        ]
    );
}

/// Runs the English comma/parenthesis-whitespace builtin over a paragraph and
/// returns the flagged text and all replacements of every suggestion.
fn comma_flags(text: &str) -> Vec<(String, Vec<String>)> {
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let chars: Vec<char> = text.chars().collect();

    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check_text(&sentences))
        .filter(|suggestion| suggestion.source() == "BUILTIN/COMMA_PARENTHESIS_WHITESPACE")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

#[test]
fn builtin_comma_flags_whitespace_before_comma() {
    assert_eq!(
        comma_flags("She said , and left."),
        vec![(" ,".to_string(), vec![",".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_missing_space_after_comma() {
    assert_eq!(
        comma_flags("It began to snow,and then stopped."),
        vec![(",and".to_string(), vec![", and".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_whitespace_after_opening_parenthesis() {
    assert_eq!(
        comma_flags("We waited ( for hours."),
        vec![("( ".to_string(), vec!["(".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_whitespace_before_closing_parenthesis() {
    assert_eq!(
        comma_flags("We waited ) for hours."),
        vec![(" )".to_string(), vec![")".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_whitespace_before_full_stop() {
    assert_eq!(
        comma_flags("It ended ."),
        vec![(" .".to_string(), vec![".".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_all_planted_public_errors() {
    // The four punctuation errors planted in the public corpus.
    assert_eq!(
        comma_flags("said , and"),
        vec![(" ,".to_string(), vec![",".to_string()])]
    );
    assert_eq!(
        comma_flags("snow,and"),
        vec![(",and".to_string(), vec![", and".to_string()])]
    );
    assert_eq!(
        comma_flags("rose , then"),
        vec![(" ,".to_string(), vec![",".to_string()])]
    );
    assert_eq!(
        comma_flags("the door,and"),
        vec![(",and".to_string(), vec![", and".to_string()])]
    );
}

#[test]
fn builtin_comma_flags_whitespace_on_both_sides_of_quote() {
    assert_eq!(
        comma_flags("You \" fixed\" it."),
        vec![(
            " \" ".to_string(),
            vec!["\" ".to_string(), " \"".to_string()]
        )]
    );
}

#[test]
fn builtin_comma_replacements_differ_from_flagged_text() {
    // A previous port flagged ordinary commas with a replacement identical to
    // the flagged text, so every suggestion must offer an actual change.
    for text in &[
        "She said , and left.",
        "It began to snow,and then stopped.",
        "rose , then",
        "the door,and",
        "We waited ( for hours.",
        "We waited ) for hours.",
        "It ended .",
        "It was quiet\u{2028},and then.",
        "You \" fixed\" it.",
    ] {
        let flags = comma_flags(text);
        for (flagged, replacements) in flags {
            assert!(
                replacements
                    .iter()
                    .any(|replacement| replacement != &flagged),
                "no change suggested for {:?} in {:?}",
                flagged,
                text
            );
        }
    }
}

#[test]
fn builtin_comma_ignores_ordinary_comma() {
    assert!(comma_flags("It was late, and she left.").is_empty());
}

#[test]
fn builtin_comma_ignores_curly_quote_dialogue() {
    assert!(comma_flags("\u{201c}Wait,\u{201d} she said, and left.").is_empty());
}

#[test]
fn builtin_comma_ignores_fiction_prose() {
    for text in &[
        "\u{201c}I didn\u{2019}t know,\u{201d} she said. \u{201c}Weren\u{2019}t you there?\u{201d}",
        "\u{2018}Doesn\u{2019}t matter,\u{2019} he said.",
        "He opened the door\u{2014}then stopped. It was quiet\u{2014}too quiet.",
        "It wasn\u{2019}t Marta\u{2019}s coat, and it wasn\u{2019}t her hat.",
        "She waited\u{2026} and waited.",
        "It cost 1,000 dollars, or 3.5 pounds.",
        "Visit foo.org, or read the .Net docs.",
        "Run ./validate.sh, then stop.",
        "He waited (patiently) for her.",
        "She waited.\u{2028}Nothing came.",
        "The sign read \u{201c}no entry\u{201d}, and she obeyed.",
    ] {
        assert!(
            comma_flags(text).is_empty(),
            "unexpected flags for {:?}: {:?}",
            text,
            comma_flags(text)
        );
    }
}

#[test]
fn builtin_comma_ignores_punctuation_after_comma() {
    // An em dash, asterisk or closing parenthesis after a comma is not a word;
    // these occur throughout Moby-Dick. LanguageTool flags them, but the rule
    // is about whitespace between words, so they are left alone here.
    for text in &[
        "The sea,\u{2014}though calm\u{2014}was dark.",
        "and,* the rest",
        "the, (aside) and more",
    ] {
        assert!(comma_flags(text).is_empty(), "{:?}", text);
    }
}

#[test]
fn builtin_comma_handles_line_separator() {
    assert_eq!(
        comma_flags("It was quiet\u{2028},and then."),
        vec![
            ("\u{2028},".to_string(), vec![", ".to_string()]),
            (",and".to_string(), vec![", and".to_string()]),
        ]
    );
    assert!(comma_flags("\u{201c}Wait,\u{201d}\u{2028}she said.").is_empty());
}

#[test]
fn builtin_comma_ignores_numbers_ellipsis_and_abbreviations() {
    for text in &[
        "The town had 1,000 people.",
        "It grew 3.5 inches.",
        "This is 1,5.",
        "This is $1,000,000.",
        "He paused... and left.",
        ".Net and .MP3 are file names.",
        "I live in .Los Angeles",
    ] {
        let flags = comma_flags(text);
        // ".Los Angeles" is a genuine error in LanguageTool's test suite.
        if text == &"I live in .Los Angeles" {
            assert_eq!(flags.len(), 1, "{:?}", text);
        } else {
            assert!(
                flags.is_empty(),
                "unexpected flags for {:?}: {:?}",
                text,
                flags
            );
        }
    }
}

#[test]
fn builtin_comma_is_wired_into_suggest() {
    let suggestions = RULES.suggest("She said , and left.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/COMMA_PARENTHESIS_WHITESPACE")
        .expect("whitespace before comma is flagged");
    assert_eq!(flagged.replacements(), &[",".to_string()]);
}

/// Runs the English word-coherency builtin through the full suggestion
/// pipeline and returns the flagged text and the first replacement of every
/// suggestion.
fn builtin_word_coherency_flags(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();

    RULES
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/EN_WORD_COHERENCY")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements()[0].clone(),
            )
        })
        .collect()
}

#[test]
fn builtin_word_coherency_flags_second_spelling() {
    // The example from the rule this builtin reimplements.
    assert_eq!(
        builtin_word_coherency_flags("He likes archaeology. Really? She likes archeology, too."),
        vec![("archeology".to_string(), "archaeology".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_adviser_after_advisor() {
    assert_eq!(
        builtin_word_coherency_flags("She was an adviser. Later the advisor arrived."),
        vec![("advisor".to_string(), "adviser".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_each_occurrence_of_the_second_spelling() {
    assert_eq!(
        builtin_word_coherency_flags(
            "The advisor left. Then the adviser returned. Another adviser waved."
        ),
        vec![
            ("adviser".to_string(), "advisor".to_string()),
            ("adviser".to_string(), "advisor".to_string()),
        ],
    );
}

#[test]
fn builtin_word_coherency_replaces_only_the_stem() {
    // The plural keeps its ending: only the stem changes.
    assert_eq!(
        builtin_word_coherency_flags("The advisor arrived. Then two advisers came."),
        vec![("advisers".to_string(), "advisors".to_string())]
    );
}

#[test]
fn builtin_word_coherency_capitalizes_the_replacement() {
    assert_eq!(
        builtin_word_coherency_flags("The advisors met. Then the Adviser left."),
        vec![("Adviser".to_string(), "Advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_inside_curly_quotes() {
    assert_eq!(
        builtin_word_coherency_flags(
            "\u{201c}advisor,\u{201d} she said, and later \u{201c}adviser.\u{201d}"
        ),
        vec![("adviser".to_string(), "advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_after_curly_apostrophe() {
    assert_eq!(
        builtin_word_coherency_flags("Marta\u{2019}s advisor and Marta\u{2019}s adviser."),
        vec![("adviser".to_string(), "advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_across_em_dash() {
    // The em dash joins neighbouring words into one token, which must not stop
    // the other variant from being recognised.
    assert_eq!(
        builtin_word_coherency_flags(
            "The advisor waited\u{2014}impatiently\u{2014}for the adviser."
        ),
        vec![("adviser".to_string(), "advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_across_u2028() {
    // U+2028 is whitespace inside a paragraph, so both sentences share state.
    assert_eq!(
        builtin_word_coherency_flags("The advisor waited.\u{2028}Then the adviser came."),
        vec![("adviser".to_string(), "advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_flags_across_sentences() {
    assert_eq!(
        builtin_word_coherency_flags("The advisor waited. Then the adviser came."),
        vec![("adviser".to_string(), "advisor".to_string())]
    );
}

#[test]
fn builtin_word_coherency_ignores_a_single_spelling() {
    assert!(builtin_word_coherency_flags("The advisor advised the advisors.").is_empty());
}

#[test]
fn builtin_word_coherency_ignores_variants_across_paragraphs() {
    // Documented limitation: the engine checks one paragraph at a time, so
    // variants used in different paragraphs are not compared.
    assert!(builtin_word_coherency_flags("The advisor left.\n\nThe adviser arrived.").is_empty());
}

#[test]
fn builtin_word_coherency_ignores_fiction_prose() {
    for text in &[
        "\u{201c}I didn\u{2019}t know,\u{201d} she said. \u{201c}Weren\u{2019}t you there?\u{201d}",
        "\u{2018}Doesn\u{2019}t matter,\u{2019} he said.",
        "He opened the door\u{2014}then stopped. It was quiet\u{2014}too quiet.",
        "It wasn\u{2019}t Marta\u{2019}s coat, and it wasn\u{2019}t her hat.",
        "She waited\u{2026} and waited.",
        "She waited.\u{2028}Nothing came.",
        "The sign read \u{201c}no entry\u{201d}, and she obeyed.",
        "The adviser nodded. She was a good adviser, the best adviser in town.",
    ] {
        assert!(
            builtin_word_coherency_flags(text).is_empty(),
            "unexpected flags for {:?}: {:?}",
            text,
            builtin_word_coherency_flags(text)
        );
    }
}

#[test]
fn builtin_word_coherency_is_wired_into_suggest() {
    let suggestions = RULES.suggest(
        "She was an adviser. Later the advisor arrived.",
        &*TOKENIZER,
    );
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/EN_WORD_COHERENCY")
        .expect("the mixed spelling is flagged");
    assert_eq!(flagged.replacements(), &["adviser".to_string()]);
}
/// Runs the English unpaired-bracket and unpaired-quote builtins on one
/// paragraph and returns the rule id and flagged character of every suggestion.
fn builtin_unpaired_flags(text: &str) -> Vec<(String, String)> {
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let chars: Vec<char> = text.chars().collect();

    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check_text(&sentences))
        .filter(|suggestion| {
            suggestion.source() == "BUILTIN/EN_UNPAIRED_BRACKETS"
                || suggestion.source() == "BUILTIN/EN_UNPAIRED_QUOTES"
        })
        .map(|suggestion| {
            (
                suggestion.source().to_owned(),
                chars[suggestion.span().char().clone()].iter().collect(),
            )
        })
        .collect()
}

fn bracket_flag(sym: &str) -> (String, String) {
    ("BUILTIN/EN_UNPAIRED_BRACKETS".to_string(), sym.to_string())
}

fn quote_flag(sym: &str) -> (String, String) {
    ("BUILTIN/EN_UNPAIRED_QUOTES".to_string(), sym.to_string())
}

#[test]
fn builtin_unpaired_brackets_flags_unclosed_open() {
    assert_eq!(
        builtin_unpaired_flags("He lived in a (large house."),
        vec![bracket_flag("(")]
    );
}

#[test]
fn builtin_unpaired_brackets_flags_stray_close() {
    assert_eq!(
        builtin_unpaired_flags("The list was ] empty."),
        vec![bracket_flag("]")]
    );
}

#[test]
fn builtin_unpaired_brackets_ignores_balanced() {
    assert!(builtin_unpaired_flags("The list ( a, b ) and [ c ] and { d } is done.").is_empty());
}

#[test]
fn builtin_unpaired_brackets_ignores_list_markers() {
    assert!(builtin_unpaired_flags("a) first item. b) second item.").is_empty());
}

#[test]
fn builtin_unpaired_brackets_ignores_smileys() {
    assert!(builtin_unpaired_flags("That is funny :) and this is sad :(").is_empty());
}

#[test]
fn builtin_unpaired_brackets_pair_across_sentences() {
    assert!(builtin_unpaired_flags("He came in (the door was open. He sat down).").is_empty());
}

#[test]
fn builtin_unpaired_quotes_ignores_balanced_dialogue() {
    assert!(builtin_unpaired_flags(
        "\u{201c}Hello,\u{201d} she said. \u{201c}Goodbye,\u{201d} he said."
    )
    .is_empty());
}

#[test]
fn builtin_unpaired_quotes_ignores_curly_apostrophes() {
    // don’t, Marta’s, the Joneses’, ’90s and rock ’n’ roll are apostrophes.
    assert!(builtin_unpaired_flags(
        "\u{201c}I don\u{2019}t know Marta\u{2019}s name,\u{201d} she said. \
         The Joneses\u{2019} house. In the \u{2019}90s, rock \u{2019}n\u{2019} roll."
    )
    .is_empty());
}

#[test]
fn builtin_unpaired_quotes_ignores_nested_single_quotes() {
    // The single quotes pair inside the enclosing double quote.
    assert!(builtin_unpaired_flags(
        "\u{201c}Everyone is \u{2018}reasonable\u{2019}, and the water keeps rising.\u{201d}"
    )
    .is_empty());
}

#[test]
fn builtin_unpaired_quotes_pair_across_sentences() {
    assert!(
        builtin_unpaired_flags("\u{201c}I went home. Then I slept,\u{201d} she said.").is_empty()
    );
}

#[test]
fn builtin_unpaired_quotes_ignores_unclosed_opening() {
    // An opening mark still open at the end of the paragraph may continue in
    // the next paragraph, so it is left alone (conservative choice).
    assert!(builtin_unpaired_flags("\u{201c}I went home. Then I slept.").is_empty());
}

#[test]
fn builtin_unpaired_quotes_flags_stray_closing_double() {
    assert_eq!(
        builtin_unpaired_flags("\u{201c}Hello,\u{201d} she said.\u{201d}"),
        vec![quote_flag("\u{201d}")]
    );
}

#[test]
fn builtin_unpaired_quotes_flags_stray_closing_single() {
    assert_eq!(
        builtin_unpaired_flags("\u{2018}Hello,\u{2019} she said.\u{2019}"),
        vec![quote_flag("\u{2019}")]
    );
}

#[test]
fn builtin_unpaired_ignores_fiction_punctuation() {
    assert!(builtin_unpaired_flags(
        "The door\u{2014}then the room\u{2026} \u{201c}Done,\u{201d} he said.\u{2028}\
         \u{201c}Really?\u{201d} she asked. He (finally) left [ and ] and { stayed }."
    )
    .is_empty());
}

#[test]
fn builtin_unpaired_pairs_across_u2028() {
    assert!(builtin_unpaired_flags("He said (hello\u{2028}world).").is_empty());
    assert!(builtin_unpaired_flags("\u{201c}one\u{2028}two\u{201d}").is_empty());
}

#[test]
fn builtin_unpaired_is_wired_into_suggest() {
    let suggestions = RULES.suggest("He lived in a (large house.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/EN_UNPAIRED_BRACKETS")
        .expect("unclosed bracket is flagged");
    assert!(flagged.replacements().is_empty());
}

#[test]
fn builtin_unpaired_brackets_sees_symbols_merged_into_words() {
    // The tokenizer produces "[eBook", "_quietly_." and "_)" etc. as single
    // tokens; the rule still sees the bracket characters inside them.
    assert!(builtin_unpaired_flags("See [eBook #2701] here.").is_empty());
    assert!(builtin_unpaired_flags("He left (_quietly_.) and came back.").is_empty());
}

#[test]
fn builtin_unpaired_quotes_sees_mark_merged_into_word() {
    // "_x“" and "y”_z" are single tokens; the marks still pair.
    assert!(builtin_unpaired_flags("The marks _x\u{201c} and y\u{201d}_z balance.").is_empty());
}

#[test]
fn builtin_unpaired_brackets_flags_unclosed_open_after_word() {
    assert_eq!(
        builtin_unpaired_flags("He left (quietly and came back."),
        vec![bracket_flag("(")]
    );
}

/// Runs the uppercase-sentence-start builtin over the paragraphs of `text` and
/// returns the flagged text and the first replacement of every suggestion.
fn builtin_uppercase_sentence_start_flags(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut flags = Vec::new();
    let mut paragraph: Vec<nlprule::types::Sentence<'_>> = Vec::new();

    for sentence in TOKENIZER.pipe(text) {
        let ends_paragraph = sentence.text().contains('\n');
        paragraph.push(sentence);

        if ends_paragraph {
            flags.extend(uppercase_sentence_start_flags(&paragraph, &chars));
            paragraph.clear();
        }
    }
    flags.extend(uppercase_sentence_start_flags(&paragraph, &chars));
    flags
}

fn uppercase_sentence_start_flags(
    paragraph: &[nlprule::types::Sentence<'_>],
    chars: &[char],
) -> Vec<(String, String)> {
    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check_text(paragraph))
        .filter(|suggestion| suggestion.source() == "BUILTIN/UPPERCASE_SENTENCE_START")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements()[0].clone(),
            )
        })
        .collect()
}

#[test]
fn builtin_uppercase_sentence_start_flags_simple() {
    assert_eq!(
        builtin_uppercase_sentence_start_flags("this is wrong. Also fine."),
        vec![("this".to_string(), "This".to_string())]
    );
}

#[test]
fn builtin_uppercase_sentence_start_flags_curly_apostrophe() {
    assert_eq!(
        builtin_uppercase_sentence_start_flags("don\u{2019}t go there."),
        vec![("don".to_string(), "Don".to_string())]
    );
}

#[test]
fn builtin_uppercase_sentence_start_flags_across_u2028() {
    // U+2028 is whitespace inside a paragraph, so the second sentence is still
    // checked against the first one.
    assert_eq!(
        builtin_uppercase_sentence_start_flags("Good.\u{2028}bad start here."),
        vec![("bad".to_string(), "Bad".to_string())]
    );
}

#[test]
fn builtin_uppercase_sentence_start_ignores_dialogue_tag_after_comma() {
    // "she" follows a closing quote after a comma inside the quote and does not
    // start a new sentence.
    assert!(builtin_uppercase_sentence_start_flags(
        "\u{201c}I don\u{2019}t know,\u{201d} she said."
    )
    .is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_dialogue_tag_after_question() {
    assert!(builtin_uppercase_sentence_start_flags("\u{201c}Where?\u{201d} he asked.").is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_dialogue_tag_after_exclamation() {
    assert!(
        builtin_uppercase_sentence_start_flags("\u{201c}Stop!\u{201d} she shouted.").is_empty()
    );
}

#[test]
fn builtin_uppercase_sentence_start_ignores_ellipsis_continuation() {
    // The sentence splitter treats the ellipsis as a boundary, but "maybe"
    // continues the sentence.
    assert!(builtin_uppercase_sentence_start_flags(
        "\u{201c}I thought\u{2026} maybe not.\u{201d} She shrugged."
    )
    .is_empty());
    assert!(builtin_uppercase_sentence_start_flags("She waited\u{2026} and waited.").is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_em_dash() {
    // The em dash joins words into a single token, so "then" is not a sentence
    // start.
    assert!(
        builtin_uppercase_sentence_start_flags("He opened the door\u{2014}then stopped.")
            .is_empty()
    );
}

#[test]
fn builtin_uppercase_sentence_start_ignores_number_start() {
    assert!(
        builtin_uppercase_sentence_start_flags("42 apples were on the table. 3.14 is pi.")
            .is_empty()
    );
}

#[test]
fn builtin_uppercase_sentence_start_ignores_quoted_capital() {
    assert!(builtin_uppercase_sentence_start_flags("\u{201c}Hello,\u{201d} she said.").is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_camel_case() {
    // "iPhone" and "eBay" start lowercase but are not errors.
    assert!(builtin_uppercase_sentence_start_flags("iPhone sales rose. eBay is fine.").is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_exceptions() {
    assert!(builtin_uppercase_sentence_start_flags("pH matters here. x86 is old.").is_empty());
}

#[test]
fn builtin_uppercase_sentence_start_ignores_correct_fiction_prose() {
    for text in &[
        "\u{201c}I didn\u{2019}t know,\u{201d} she said. \u{201c}Weren\u{2019}t you there?\u{201d}",
        "\u{2018}Doesn\u{2019}t matter,\u{2019} he said.",
        "He opened the door\u{2014}then stopped. It was quiet\u{2014}too quiet.",
        "Pages 10\u{2013}12 were missing\u{2013}or torn out.",
        "She waited\u{2026} and waited.",
        "Marta\u{2019}s letter arrived. She read it slowly.",
        "She waited.\u{2028}Nothing came.",
        "The lighthouse blinked\u{2026} then went dark.",
    ] {
        assert!(
            builtin_uppercase_sentence_start_flags(text).is_empty(),
            "{}",
            text
        );
    }
}

#[test]
fn builtin_uppercase_sentence_start_is_wired_into_suggest() {
    let suggestions = RULES.suggest("tomorrow we sail.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/UPPERCASE_SENTENCE_START")
        .expect("lowercase sentence start is flagged");
    assert_eq!(flagged.replacements(), &["Tomorrow".to_string()]);
}
/// Runs the English wrong-word-in-context builtin through the full suggestion
/// pipeline and returns the flagged text and the first replacement of every
/// suggestion.
fn builtin_wrong_word_flags(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();

    RULES
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/ENGLISH_WRONG_WORD_IN_CONTEXT")
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements()[0].clone(),
            )
        })
        .collect()
}

#[test]
fn builtin_wrong_word_flags_proscribed() {
    assert_eq!(
        builtin_wrong_word_flags("I have proscribed you a course of antibiotics."),
        vec![("proscribed".to_string(), "prescribed".to_string())]
    );
}

#[test]
fn builtin_wrong_word_ignores_correct_prescribed() {
    assert!(builtin_wrong_word_flags("I have prescribed you a course of antibiotics.").is_empty());
}

#[test]
fn builtin_wrong_word_flags_gorilla_for_soldiers() {
    assert_eq!(
        builtin_wrong_word_flags("The gorilla charged at the soldiers."),
        vec![("gorilla".to_string(), "guerilla".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_sing_for_contract() {
    assert_eq!(
        builtin_wrong_word_flags("Please sing the contract before the meeting."),
        vec![("sing".to_string(), "sign".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_statues_for_government() {
    assert_eq!(
        builtin_wrong_word_flags("The statues were passed by the government."),
        vec![("statues".to_string(), "statutes".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_hangers_for_plane() {
    // "massage"/"message" is already handled by an n-gram rule, so this uses a
    // pair that only this builtin knows about.
    assert_eq!(
        builtin_wrong_word_flags("The plane was kept in the hangers."),
        vec![("hangers".to_string(), "hangars".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_heroin_for_novel() {
    assert_eq!(
        builtin_wrong_word_flags("She wrote about heroin in her novels."),
        vec![("heroin".to_string(), "heroine".to_string())]
    );
}

#[test]
fn builtin_wrong_word_ignores_correct_heroin_context() {
    // "addiction" is a context of "heroin", so the word is used correctly.
    assert!(builtin_wrong_word_flags("He was treated for heroin addiction.").is_empty());
}

#[test]
fn builtin_wrong_word_ignores_correct_heroine_context() {
    // "novel" is a context of "heroine", so the word is used correctly.
    assert!(builtin_wrong_word_flags("The heroine of the novel was brave.").is_empty());
}

#[test]
fn builtin_wrong_word_ignores_when_both_words_occur() {
    assert!(
        builtin_wrong_word_flags("The statutes and the statues were both mentioned.").is_empty()
    );
}

#[test]
fn builtin_wrong_word_flags_inside_curly_quotes() {
    assert_eq!(
        builtin_wrong_word_flags("\u{201c}Please sing the contract,\u{201d} she said."),
        vec![("sing".to_string(), "sign".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_after_curly_apostrophe() {
    assert_eq!(
        builtin_wrong_word_flags("Marta\u{2019}s statues were passed by the government."),
        vec![("statues".to_string(), "statutes".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_around_em_dashes() {
    // The em dashes join "door—then" into a single token; the flagged word is
    // still its own token.
    assert_eq!(
        builtin_wrong_word_flags(
            "She opened the door\u{2014}then the gorilla charged the \
                                  soldiers."
        ),
        vec![("gorilla".to_string(), "guerilla".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_before_ellipsis() {
    assert_eq!(
        builtin_wrong_word_flags("Please sing the contract\u{2026} and then leave."),
        vec![("sing".to_string(), "sign".to_string())]
    );
}

#[test]
fn builtin_wrong_word_flags_across_u2028() {
    // U+2028 separates two sentences inside one paragraph; each is checked on
    // its own.
    assert_eq!(
        builtin_wrong_word_flags(
            "Please sing the contract.\u{2028}The statues were passed by the government."
        ),
        vec![
            ("sing".to_string(), "sign".to_string()),
            ("statues".to_string(), "statutes".to_string()),
        ]
    );
}

#[test]
fn builtin_wrong_word_ignores_correct_prose_with_fiction_punctuation() {
    // Curly double and single quotes, curly apostrophes, em dashes and an
    // ellipsis, all correctly punctuated: no flags.
    for text in &[
        "\u{201c}I\u{2019}ve prescribed the medication,\u{201d} she said\u{2026}",
        "The \u{2018}heroine\u{2019} of the novel was brave\u{2014}and the crowd \
         cheered\u{2026}",
        "Marta\u{2019}s statues stood in the garden\u{2014}then the rain came\u{2026}",
        "The \u{2018}heroine\u{2019} of the novel was brave.",
    ] {
        assert!(
            builtin_wrong_word_flags(text).is_empty(),
            "unexpected flags for {:?}: {:?}",
            text,
            builtin_wrong_word_flags(text)
        );
    }
}

#[test]
fn builtin_wrong_word_ignores_correct_prose_across_u2028() {
    // A correct context for "heroine" in the second sentence, an ordinary
    // sentence in the first one.
    assert!(builtin_wrong_word_flags(
        "It was quiet\u{2028}The \u{2018}heroine\u{2019} of the novel was brave\u{2026}"
    )
    .is_empty());
}

#[test]
fn builtin_wrong_word_ignores_words_without_a_confusion() {
    // "song" is not one of the confused words, so it is never flagged.
    assert!(builtin_wrong_word_flags("The song was over\u{2026} and everyone left.").is_empty());
}

#[test]
fn builtin_wrong_word_is_wired_into_suggest() {
    let suggestions = RULES.suggest(
        "I have proscribed you a course of antibiotics.",
        &*TOKENIZER,
    );
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/ENGLISH_WRONG_WORD_IN_CONTEXT")
        .expect("the confused word is flagged");
    assert_eq!(flagged.replacements(), &["prescribed".to_string()]);
}
