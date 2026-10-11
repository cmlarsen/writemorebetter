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
