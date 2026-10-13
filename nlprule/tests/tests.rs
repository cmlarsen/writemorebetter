use std::convert::{TryFrom, TryInto};

use lazy_static::lazy_static;
use nlprule::{
    profile::Profile, rule::id::Category, rule::id::Selector, types::Position, Error, Rules,
    Tokenizer,
};
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

fn token_texts(text: &str) -> Vec<String> {
    TOKENIZER
        .pipe(text)
        .flat_map(|sentence| {
            sentence
                .tokens()
                .iter()
                .map(|token| token.word().text().as_str().to_string())
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn tokenizer_splits_fiction_punctuation() {
    assert_eq!(
        token_texts("\u{201c}Wait\u{2014}no,\u{201d} she said\u{2026} \u{2018}It\u{2019}s fine\u{2014}isn\u{2019}t it?"),
        vec![
            "\u{201c}", "Wait", "\u{2014}", "no", ",", "\u{201d}", "she", "said", "\u{2026}",
            "\u{2018}", "It", "'s", "fine", "\u{2014}", "is", "n't", "it", "?",
        ]
    );
    assert_eq!(
        token_texts("Pages 10\u{2013}12\u{2015}gone.\u{2028}Then\u{2012}nothing."),
        vec![
            "Pages", "10", "\u{2013}", "12", "\u{2015}", "gone", ".", "Then", "\u{2012}",
            "nothing", ".",
        ]
    );
}

#[test]
fn tokenizer_splits_on_every_non_word_character() {
    assert_eq!(
        token_texts("{{cite |first=S}} x_y 3\u{d7}4 a\u{b7}b TM\u{2122} \u{2192}go"),
        vec![
            "{", "{", "cite", "|", "first", "=", "S", "}", "}", "x", "_", "y", "3", "\u{d7}", "4",
            "a", "\u{b7}", "b", "TM", "\u{2122}", "\u{2192}", "go",
        ]
    );
}

#[test]
fn tokenizer_keeps_word_characters_together() {
    assert_eq!(
        token_texts("well-known caf\u{e9} cafe\u{301} 5\u{b0} 10% $5 \u{a3}3 AT&T me@x"),
        vec![
            "well-known",
            "caf\u{e9}",
            "cafe\u{301}",
            "5\u{b0}",
            "10%",
            "$5",
            "\u{a3}3",
            "AT&T",
            "me@x",
        ]
    );
}

#[test]
fn tokenizer_attaches_variation_selectors() {
    assert_eq!(
        token_texts("I \u{2764}\u{fe0f}it\u{2014}really."),
        vec!["I", "\u{2764}\u{fe0f}", "it", "\u{2014}", "really", "."]
    );
}

#[test]
fn tokenizer_attaches_variation_selectors_to_normalized_clitics() {
    for text in [
        "He said isn\u{2019}t\u{fe0f} so.",
        "He said isn't\u{fe0f} so.",
    ] {
        assert_eq!(
            token_texts(text),
            vec!["He", "said", "is", "n't\u{fe0f}", "so", "."],
            "{}",
            text
        );
    }
}

#[quickcheck]
fn can_tokenize_anything(text: String) -> bool {
    let _: Vec<_> = TOKENIZER.pipe(&text).collect();
    true
}

#[test]
fn sentencizer_breaks_at_a_blank_line() {
    // LanguageTool segments text with the SRX rules of its default line-break
    // mode, where a blank line ends a paragraph (and its last sentence).
    let count = |text: &str| TOKENIZER.pipe(text).count();
    assert_eq!(count("First. Second.\n\nThird. Fourth."), 4);
    assert_eq!(count("A.\n\nB."), 2);
}

#[test]
fn batch_size_does_not_change_suggestions() {
    let text = "I can due his homework.\n\n“She dont know”—he said… \u{2028}It’s fine. A apple fell.\n\nThey was late.";
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let expected = format!("{:?}", rules.suggest(text, &*TOKENIZER));
    for size in [0, 1, 2, 3, 128] {
        rules.set_batch_size(size);
        assert_eq!(format!("{:?}", rules.suggest(text, &*TOKENIZER)), expected);
    }
}

#[test]
fn suggest_indices_are_relative_to_input_text() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    // A newly compiled rule without a suggestion (GRAMMAR/PRP_MD_NN/4) also
    // matches "can due" and would mask the due/do suggestion, so turn it off.
    rules
        .select_mut(&Category::new("grammar").join("prp_md_nn").into())
        .for_each(|x| x.disable());

    let suggestions = rules.suggest(
        "I can due his homework for 10€. I can due his homework.",
        &*TOKENIZER,
    );

    // Newly compiled rules can add suggestions (e.g. TYPOGRAPHY/CURRENCY on
    // "10€"), so look the two "due" matches up by position instead of index.
    let first = suggestions
        .iter()
        .find(|suggestion| suggestion.span().char().start == 6)
        .expect("a suggestion for the first \"due\"");
    assert_eq!(*first.span().char(), 6..9);
    assert_eq!(*first.span().byte(), 6..9);

    let second = suggestions
        .iter()
        .find(|suggestion| suggestion.span().char().start == 38)
        .expect("a suggestion for the second \"due\"");
    assert_eq!(*second.span().char(), 38..41);
    assert_eq!(
        *second.span().byte(),
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

    // A newly compiled no-suggestion rule (GRAMMAR/PRP_MD_NN/4) also matches
    // "I can due his homework" and would otherwise hide the due/do suggestion.
    rules
        .select_mut(&Category::new("grammar").join("prp_md_nn").into())
        .for_each(|x| x.disable());

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

/// The fiction profile is only useful if every selector in it names a rule that exists in
/// the compiled English rule set. A selector that matches nothing is almost always a typo,
/// or a rule that was renamed upstream, so it is a hard failure here.
#[test]
fn fiction_profile_selectors_all_match() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let unmatched = rules.apply_profile(&Profile::fiction());
    assert!(
        unmatched.is_empty(),
        "fiction profile has selectors that match no rules: {:?}",
        unmatched
    );
}

/// The default profile disables nothing, and applying an empty profile is a no-op.
#[test]
fn default_profile_disables_nothing() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let before = rules.rules().iter().filter(|r| r.enabled()).count();
    assert!(rules
        .apply_profile(&Profile::by_name("default").unwrap())
        .is_empty());
    let after = rules.rules().iter().filter(|r| r.enabled()).count();
    assert_eq!(before, after);
}

/// Selectors that name nothing are reported, for both XML selectors and builtins, so a
/// typo in a profile is visible rather than silently ignored.
#[test]
fn apply_profile_reports_unmatched_selectors() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let profile = Profile::new(
        "test",
        vec![
            "NO_SUCH_CATEGORY".to_owned(),
            "BUILTIN/NO_SUCH_BUILTIN".to_owned(),
        ],
    );
    let mut unmatched = rules.apply_profile(&profile);
    unmatched.sort();
    assert_eq!(unmatched, ["BUILTIN/NO_SUCH_BUILTIN", "NO_SUCH_CATEGORY"]);
}

/// The `BUILTIN/` selector path disables a builtin rule by its id.
#[test]
fn apply_profile_disables_builtin_by_id() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let enabled = |rules: &Rules| {
        rules
            .builtins()
            .iter()
            .find(|rule| rule.id() == "UPPERCASE_SENTENCE_START")
            .unwrap()
            .enabled()
    };
    assert!(enabled(&rules));

    let profile = Profile::new("test", vec!["BUILTIN/UPPERCASE_SENTENCE_START".to_owned()]);
    assert!(rules.apply_profile(&profile).is_empty());
    assert!(!enabled(&rules));
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
fn spellchecker_ignores_lone_format_characters() {
    for text in &[
        "\u{201c}Yes\u{200f}\u{2014}no,\u{201d} she said\u{2026}",
        "He paused\u{200f}\u{2014}then went on.\u{2028}It\u{2019}s done\u{200e}.",
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
        vec!["Patenice"]
    );
}

/// Spelling-only flags after adding `words` to a fresh rule set's user dictionary.
fn spelling_errors_with_words(words: &[&str], text: &str) -> Vec<String> {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    rules.add_words(words.iter().copied());

    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| text[x.span().byte().clone()].to_string())
        .collect()
}

#[test]
fn user_dictionary_suppresses_added_word() {
    assert_eq!(
        spelling_errors("The Xylophonix arrived."),
        vec!["Xylophonix"]
    );
    assert!(spelling_errors_with_words(&["Xylophonix"], "The Xylophonix arrived.").is_empty());
}

#[test]
fn user_dictionary_case_handling() {
    // A lowercase entry also accepts its sentence-initial capitalized form.
    assert!(spelling_errors_with_words(&["xylophonix"], "xylophonix arrived.").is_empty());
    assert!(spelling_errors_with_words(&["xylophonix"], "Xylophonix arrived.").is_empty());
    // A capitalized entry does not make the lowercase form valid.
    assert_eq!(
        spelling_errors_with_words(&["Xylophonix"], "the xylophonix"),
        vec!["xylophonix"]
    );
    // Nor does it accept an all-caps form.
    assert_eq!(
        spelling_errors_with_words(&["Xylophonix"], "the XYLOPHONIX"),
        vec!["XYLOPHONIX"]
    );
}

#[test]
fn user_dictionary_possessives_and_apostrophes() {
    for text in [
        "The Xylophonix's wand glowed.",
        "The Xylophonix\u{2019}s wand glowed.",
        "James' coat. James\u{2019} coat.",
    ] {
        assert!(
            spelling_errors_with_words(&["Xylophonix", "James"], text).is_empty(),
            "{}",
            text
        );
    }

    // Curly and straight internal apostrophes match each other.
    for text in [
        "Zar'khan spoke.",
        "Zar\u{2019}khan spoke.",
        "Nothing.\u{2028}Zar\u{2019}khan spoke.",
    ] {
        assert!(
            spelling_errors_with_words(&["Zar'khan"], text).is_empty(),
            "{}",
            text
        );
    }
}

#[test]
fn user_dictionary_accented_and_em_dash_names() {
    for text in [
        "Zo\u{00eb} and Ng\u{0169}g\u{0129} met.",
        "Zo\u{00eb} met Ng\u{0169}g\u{0129} at the harbor.",
        "Ng\u{0169}g\u{0129}'s book. Ng\u{0169}g\u{0129}\u{2019}s book.",
        "Xylophonix\u{2014}then he left.",
        "Krell-Tarn nodded. Marie-Claire nodded.",
    ] {
        assert!(
            spelling_errors_with_words(
                &[
                    "Zo\u{00eb}",
                    "Ng\u{0169}g\u{0129}",
                    "Xylophonix",
                    "Krell-Tarn",
                    "Marie-Claire"
                ],
                text
            )
            .is_empty(),
            "{}",
            text
        );
    }
}

#[test]
fn user_dictionary_entries_with_split_characters() {
    let words = [
        "Ka\u{b7}lel",
        "Zorvath\u{2014}Quill",
        "Vel\u{2019}dara\u{2014}Quinn",
    ];
    assert!(!spelling_errors("Ka\u{b7}lel smiled at Zorvath\u{2014}Quill.").is_empty());

    for text in [
        "Ka\u{b7}lel smiled at Zorvath\u{2014}Quill.",
        "\u{201c}Ka\u{b7}lel\u{2026}\u{201d} Zorvath\u{2014}Quill\u{2019}s voice trailed off.",
        "Vel\u{2019}dara\u{2014}Quinn left. Vel'dara\u{2014}Quinn returned.",
        "Nothing.\u{2028}Ka\u{b7}lel and Vel\u{2019}dara\u{2014}Quinn waited.",
    ] {
        assert!(
            spelling_errors_with_words(&words, text).is_empty(),
            "{}",
            text
        );
    }
}

#[test]
fn user_dictionary_multiword_entry() {
    assert_eq!(
        spelling_errors("They reached Xyzzy Plugh at dusk."),
        vec!["Xyzzy", "Plugh"]
    );
    assert!(
        spelling_errors_with_words(&["Xyzzy Plugh"], "They reached Xyzzy Plugh at dusk.")
            .is_empty()
    );
    // The entry does not leak into unrelated words.
    assert_eq!(
        spelling_errors_with_words(&["Xyzzy Plugh"], "The Xyzzy alone."),
        vec!["Xyzzy"]
    );
}

#[test]
fn user_dictionary_words_are_suggestion_candidates() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    rules.set_words(["Xylophonix"]);

    let suggestions = rules.suggest("the Xylophoni arrived", &*TOKENIZER);
    let spell = suggestions
        .iter()
        .find(|s| s.source().starts_with("SPELLCHECK"))
        .expect("the typo is flagged");
    assert!(
        spell.replacements().iter().any(|x| x == "Xylophonix"),
        "{:?}",
        spell.replacements()
    );
}

#[test]
fn user_dictionary_remove_set_and_list() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());

    assert_eq!(rules.add_words(["Xylophonix", "O\u{2019}Brien", "  "]), 2);
    assert_eq!(rules.words(), vec!["O'Brien", "Xylophonix"]);
    assert_eq!(rules.add_words(["Xylophonix"]), 0);

    assert_eq!(rules.remove_words(["Xylophonix", "Absent"]), 1);
    assert_eq!(rules.words(), vec!["O'Brien"]);

    assert_eq!(rules.set_words(["Zo\u{00eb}", "Ng\u{0169}g\u{0129}"]), 2);
    assert_eq!(rules.words(), vec!["Ng\u{0169}g\u{0129}", "Zo\u{00eb}"]);
    rules.clear_words();
    assert!(rules.words().is_empty());
}

#[test]
fn user_dictionary_does_not_change_output_without_words() {
    let text = "The Xylophonix arrived\u{2014}befuddled. Zo\u{00eb} waved.";
    assert_eq!(spelling_errors(text), spelling_errors_with_words(&[], text));
}

#[test]
fn spellchecker_does_not_search_very_long_tokens() {
    // A pasted run with no word break must not let the finite-state search dominate the
    // check. The Levenshtein search is skipped for tokens longer than a word, so this
    // completes quickly even though the token is still flagged.
    let text = "a".repeat(100_000);
    let start = std::time::Instant::now();
    assert_eq!(spelling_errors(&text), vec![text.as_str()]);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(10),
        "checking a 100k-char token took {:?}",
        start.elapsed()
    );
}

#[test]
fn suggestions_at_a_second_sentence_start_apply_cleanly() {
    // U+2029 starts a new sentence. A rule matching the first token of a later
    // sentence used to report a span from position zero because the previous
    // (zero-width) SENT_START token contributed its default end position; applying
    // the suggestions then underflowed the character index.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());

    let text = "jW\u{2029},<d";
    let suggestions = rules.suggest(text, &*TOKENIZER);
    let char_count = text.chars().count();
    for suggestion in &suggestions {
        assert!(
            suggestion.span().char().start <= suggestion.span().char().end,
            "{:?}",
            suggestion
        );
        assert!(
            suggestion.span().char().end <= char_count,
            "{:?}",
            suggestion
        );
    }
    let _ = nlprule::rules::apply_suggestions(text, &suggestions);
}

#[test]
fn suggestion_span_with_a_sentence_start_group() {
    // `ANOTHER_WORDS` starts with a `SENT_START` group. Its suggestion is the marked
    // "Another words"; leading whitespace must not move the span, and the zero-width
    // SENT_START group must not be reported as a matched token.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    // A second rule matches the same text; disable it so the SENT_START rule is reported.
    rules
        .select_mut(&Selector::try_from("CONFUSED_WORDS/CONFUSION_OF_ANOTHER_IN_OTHER").unwrap())
        .for_each(|rule| rule.disable());

    for text in [
        "Another words, he left.",
        "  Another words, he left.",
        "\n\tAnother words, he left.",
    ] {
        let suggestions = rules.suggest(text, &*TOKENIZER);
        let suggestion = suggestions
            .iter()
            .find(|suggestion| suggestion.source().contains("ANOTHER_WORDS"))
            .unwrap_or_else(|| panic!("no ANOTHER_WORDS suggestion for {:?}", text));
        assert_eq!(&text[suggestion.span().byte().clone()], "Another words");
    }
}

#[test]
fn repeated_token_run_does_not_scan_quadratically() {
    // A run of a token that rules match at every position used to make
    // `MatchGraph::fill_empty` walk the sentence once per group per match, which is
    // quadratic in the run length. This run took ~21s before the fix and ~0.6s after
    // with the app's profile and speller; the bound is generous so the test stays
    // reliable on slow machines.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let _ = rules.apply_profile(&Profile::fiction());
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());

    let text = "I ".repeat(2000);
    let start = std::time::Instant::now();
    let _ = rules.suggest(&text, &*TOKENIZER);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(15),
        "checking a 2000-token run took {:?}",
        start.elapsed()
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
fn builtin_word_repeat_flags_straight_split_contraction() {
    assert_eq!(
        builtin_word_repeat_flags("I don't don't know."),
        vec![("don't don't".to_string(), "don't".to_string())]
    );
}

#[test]
fn builtin_word_repeat_flags_split_clitic() {
    // The `’s` clitic is a token of its own under 6.6 tokenization.
    assert_eq!(
        builtin_word_repeat_flags("It\u{2019}s it\u{2019}s fine."),
        vec![(
            "It\u{2019}s it\u{2019}s".to_string(),
            "It\u{2019}s".to_string()
        )]
    );
}

#[test]
fn builtin_word_repeat_ignores_words_that_only_look_split() {
    // A space before the clitic makes `do` and `n't` two words, so `do` is not
    // immediately repeated.
    assert!(builtin_word_repeat_flags("I do n't do n't know.").is_empty());
    // A sentence boundary separates `do` from the next `Don\u{2019}t`.
    assert!(builtin_word_repeat_flags("I do. Don\u{2019}t go.").is_empty());
}

#[test]
fn builtin_word_repeat_ignores_contraction_after_verb() {
    // The negation clitic is one token under 6.6, but `do do n't` stays allowed.
    assert!(builtin_word_repeat_flags("I do do n't know.").is_empty());
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
    // The em dash is a token between the two occurrences of "the".
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
    // The em dash is a token of its own, so the flag covers just the article
    // and the "a"-initial word.
    assert_eq!(
        builtin_a_vs_an_flags("He gave me a apple\u{2014}then left."),
        vec![("a apple".to_string(), "an".to_string())]
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
    // The em dash is a token of its own, which must not stop the other variant
    // from being recognised.
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
fn builtin_unpaired_quotes_ignores_straight_pairs() {
    assert!(builtin_unpaired_flags("\"Hello,\" she said. \"Goodbye,\" he said.").is_empty());
}

#[test]
fn builtin_unpaired_quotes_ignores_contraction_apostrophe() {
    // The apostrophe inside the 6.6-split negation contraction is a word part,
    // not a quotation mark.
    assert!(builtin_unpaired_flags("\u{201c}I don\u{2019}t know,\u{201d} she said.").is_empty());
}

#[test]
fn builtin_unpaired_quotes_span_points_into_the_original_text() {
    // The tokenizer normalizes both curly marks to `'`, so the reported span
    // must still address the original characters.
    let text = "\u{2018}Hello,\u{2019} she said.\u{2019}";
    let suggestions = RULES.suggest(text, &*TOKENIZER);
    let flagged: Vec<_> = suggestions
        .iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/EN_UNPAIRED_QUOTES")
        .collect();
    assert_eq!(flagged.len(), 1, "{:?}", suggestions);
    let flagged = flagged[0];
    assert_eq!(&text[flagged.span().byte().clone()], "\u{2019}");
    let chars: Vec<char> = text.chars().collect();
    assert_eq!(
        chars[flagged.span().char().clone()]
            .iter()
            .collect::<String>(),
        "\u{2019}"
    );
}

#[test]
fn builtin_unpaired_quotes_ignores_paragraph_continuation() {
    // A closing mark opening a paragraph may continue the previous paragraph's
    // quotation, so on its own it is not reported.
    assert!(builtin_unpaired_flags("\u{201d} she said.").is_empty());
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
fn builtin_unpaired_brackets_pairs_brackets_against_words() {
    // The rule pairs brackets that sit directly against words, underscores and
    // full stops.
    assert!(builtin_unpaired_flags("See [eBook #2701] here.").is_empty());
    assert!(builtin_unpaired_flags("He left (_quietly_.) and came back.").is_empty());
}

#[test]
fn builtin_unpaired_quotes_pairs_marks_between_underscores() {
    // The underscores and curly quotes are tokens of their own; the marks still pair.
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
    // LanguageTool 6.6 splits a negation contraction into the stem and the
    // clitic (`do` + `n’t`), so the flagged token is `do`; replacing it with
    // `Do` still corrects the surface form to `Don’t`.
    assert_eq!(
        builtin_uppercase_sentence_start_flags("don\u{2019}t go there."),
        vec![("do".to_string(), "Do".to_string())]
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
    // The em dash does not end a sentence, so "then" is not a sentence start.
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
    // The em dash is a token of its own and does not hide the flagged word.
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

fn tags_of(text: &str, word: &str) -> Vec<(String, String)> {
    TOKENIZER
        .pipe(text)
        .flat_map(|sentence| sentence.into_iter())
        .filter(|token| token.word().text().as_str() == word)
        .flat_map(|token| {
            token
                .word()
                .tags()
                .iter()
                .map(|data| {
                    (
                        data.lemma().as_str().to_string(),
                        data.pos().as_str().to_string(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn all_uppercase_proper_noun_is_tagged_in_title_case() {
    for text in [
        "I visited SCHWARZWALD yesterday.",
        "“We drove to SCHWARZWALD — again…”\u{2028}He shrugged.",
        "‘SCHWARZWALD’ isn’t far, she’d said.",
    ] {
        assert!(
            tags_of(text, "SCHWARZWALD").contains(&("Schwarzwald".to_string(), "NNP".to_string())),
            "{:?}: {:?}",
            text,
            tags_of(text, "SCHWARZWALD")
        );
    }

    assert!(!tags_of("“HELLO,” he said.", "HELLO")
        .iter()
        .any(|(lemma, _)| lemma == "Hello"));
}

#[test]
fn all_uppercase_im_between_proper_nouns_is_flagged() {
    let text = "THE FAMILY MOVED TO KÖNIGSFELD IM SCHWARZWALD, BADEN-WÜRTTEMBERG, WHERE HE’D STAY…";
    let sources: Vec<String> = RULES
        .suggest(text, &*TOKENIZER)
        .iter()
        .map(|suggestion| suggestion.source().to_string())
        .collect();
    assert!(
        sources.iter().any(|source| source == "TYPOS/IM_I_M/0"),
        "{:?}",
        sources
    );
}

#[test]
fn rule_synthesis_produces_inflected_forms() {
    // HASNT_IRREGULAR_VERB's suggestion is `<match no="4" postag="VBN"/>`, i.e. it must inflect
    // the lemma of "bit" ("bite") into its past participle.
    let suggestions = RULES.suggest("They haven't bit their tongue.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "TYPOS/HASNT_IRREGULAR_VERB/0")
        .expect("the irregular verb rule should fire");
    assert!(
        flagged
            .replacements()
            .iter()
            .any(|replacement| replacement == "haven't bitten"),
        "expected a synthesized \"haven't bitten\", got {:?}",
        flagged.replacements()
    );
}

/// The chunker applies the LanguageTool 5.9+ rule from the 6.6 build directory:
/// a noun phrase whose last token is singular is singular even if it contains a
/// plural (the 5.2 behavior marked it plural if any token was). The 5.2 rule is
/// exercised by the `tokenizer/chunk.rs` unit tests because it is only enabled
/// for build directories generated before 5.9.
#[test]
fn chunker_marks_noun_phrase_from_last_token() {
    let text = "But when I write, I like to use a pens and paper.";
    let tokens: Vec<_> = TOKENIZER
        .pipe(text)
        .map(|sentence| sentence.into_iter())
        .flatten()
        .collect();
    let chunks = |word: &str| {
        tokens
            .iter()
            .find(|token| token.word().as_str() == word)
            .unwrap_or_else(|| panic!("token {:?} not found", word))
            .chunks()
            .to_vec()
    };
    assert_eq!(chunks("a"), vec!["B-NP-singular".to_string()]);
    assert_eq!(chunks("paper"), vec!["E-NP-singular".to_string()]);
}

/// A suggestion that references a token with `min="0"` which did not take part
/// in the match must not keep the space that separated it from the next token.
/// 5.2's A_NNS rule suggests `\2 \3`, where token 2 is an optional adjective,
/// so the old engine produced " teachers"; LanguageTool's own correction for
/// "an animals" is "animals", not " animals".
#[test]
fn suggestion_drops_space_of_absent_optional_token() {
    let suggestions = RULES.suggest("He is a teachers.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source().starts_with("GRAMMAR/A_NNS/"))
        .expect("the agreement error is flagged");
    assert!(
        !flagged.replacements().is_empty(),
        "the rule should offer a correction"
    );
    assert!(
        flagged
            .replacements()
            .iter()
            .all(|r| !r.starts_with(' ') && !r.ends_with(' ')),
        "no replacement should keep the space of the absent token: {:?}",
        flagged.replacements()
    );
}

/// PHRASE_REPETITION uses `<match case_conversion="startupper" no="1"/>`
/// inside one of its tokens. That used to make the whole rule fail to compile,
/// so the rule never fired. Its other antipatterns must keep working, too.
#[test]
fn phrase_repetition_flags_repeated_two_word_phrase() {
    let suggestions = RULES.suggest("She said it, and this is this is the problem.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "GRAMMAR/PHRASE_REPETITION/0")
        .expect("the repeated phrase is flagged");
    assert_eq!(flagged.replacements(), &["this is".to_string()]);

    // The `case_conversion="startupper"` antipattern suppresses "as big as Big
    // Ben" (where the fourth token is the start-uppercased form of the second).
    assert!(!RULES
        .suggest("He needed a bell as big as Big Ben.", &*TOKENIZER)
        .iter()
        .any(|suggestion| suggestion.source() == "GRAMMAR/PHRASE_REPETITION/0"));
}

/// Runs the simple-replace builtins over a paragraph and returns the flagged
/// text and all replacements of every suggestion from `id`.
fn builtin_simple_replace_flags(text: &str, id: &str) -> Vec<(String, Vec<String>)> {
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let chars: Vec<char> = text.chars().collect();
    let source = format!("BUILTIN/{}", id);

    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check_text(&sentences))
        .filter(|suggestion| suggestion.source() == source)
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

/// Runs every simple-replace builtin over a paragraph and returns the flagged
/// words; used for negative tests.
fn all_simple_replace_flags(text: &str) -> Vec<(String, String)> {
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let chars: Vec<char> = text.chars().collect();

    RULES
        .builtins()
        .iter()
        .flat_map(|rule| rule.builtin().check_text(&sentences))
        .filter(|suggestion| {
            matches!(
                suggestion.source(),
                "BUILTIN/EN_SIMPLE_REPLACE"
                    | "BUILTIN/EN_US_SIMPLE_REPLACE"
                    | "BUILTIN/EN_DIACRITICS_REPLACE_ORTHOGRAPHY"
                    | "BUILTIN/EN_CONTRACTION_SPELLING"
            )
        })
        .map(|suggestion| {
            (
                suggestion.source().to_owned(),
                chars[suggestion.span().char().clone()].iter().collect(),
            )
        })
        .collect()
}

#[test]
fn builtin_simple_replace_flags_us_word() {
    assert_eq!(
        builtin_simple_replace_flags("Are baked crisps healthy?", "EN_US_SIMPLE_REPLACE"),
        vec![("crisps".to_string(), vec!["chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_general_phrase() {
    assert_eq!(
        builtin_simple_replace_flags("We met at Sankt Moritz.", "EN_SIMPLE_REPLACE"),
        vec![("Sankt Moritz".to_string(), vec!["Saint Moritz".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_diacritics() {
    assert_eq!(
        builtin_simple_replace_flags(
            "He used the word blase.",
            "EN_DIACRITICS_REPLACE_ORTHOGRAPHY"
        ),
        vec![("blase".to_string(), vec!["blasé".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_contraction() {
    assert_eq!(
        builtin_simple_replace_flags("I dont know.", "EN_CONTRACTION_SPELLING"),
        vec![("dont".to_string(), vec!["don't".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_contraction_is_case_sensitive() {
    assert_eq!(
        builtin_simple_replace_flags("I DONT know.", "EN_CONTRACTION_SPELLING"),
        vec![("DONT".to_string(), vec!["DON'T".to_string()])]
    );
    // There is no table entry for this mixed casing.
    assert!(builtin_simple_replace_flags("I DoNT know.", "EN_CONTRACTION_SPELLING").is_empty());
}

#[test]
fn builtin_simple_replace_capitalizes_phrase_suggestion() {
    assert_eq!(
        builtin_simple_replace_flags("Crisps are baked.", "EN_US_SIMPLE_REPLACE"),
        vec![("Crisps".to_string(), vec!["Chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_inside_curly_quotes() {
    // The opening quote is skipped when finding the sentence start, so the
    // suggestion is capitalized as the first word of the sentence.
    assert_eq!(
        builtin_simple_replace_flags("\u{201c}crisps\u{201d} are baked.", "EN_US_SIMPLE_REPLACE"),
        vec![("crisps".to_string(), vec!["Chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_before_em_dash() {
    assert_eq!(
        builtin_simple_replace_flags("crisps\u{2014}then", "EN_US_SIMPLE_REPLACE"),
        vec![("crisps".to_string(), vec!["Chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_before_ellipsis() {
    assert_eq!(
        builtin_simple_replace_flags("crisps\u{2026}then", "EN_US_SIMPLE_REPLACE"),
        vec![("crisps".to_string(), vec!["Chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_flags_across_u2028() {
    assert_eq!(
        builtin_simple_replace_flags("crisps\u{2028}are stale", "EN_US_SIMPLE_REPLACE"),
        vec![("crisps".to_string(), vec!["Chips".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_phrase_spans_curly_quoted_dialogue() {
    assert_eq!(
        builtin_simple_replace_flags(
            "\u{201c}We met at Sankt Moritz,\u{201d} she said.",
            "EN_SIMPLE_REPLACE"
        ),
        vec![("Sankt Moritz".to_string(), vec!["Saint Moritz".to_string()])]
    );
}

#[test]
fn builtin_simple_replace_contraction_is_wired_into_suggest() {
    let suggestions = RULES.suggest("I dont know.", &*TOKENIZER);
    let flagged = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "BUILTIN/EN_CONTRACTION_SPELLING")
        .expect("a missing apostrophe is flagged");
    assert_eq!(flagged.replacements(), &["don't".to_string()]);
}

#[test]
fn builtin_simple_replace_ignores_correctly_punctuated_fiction() {
    for text in &[
        "\u{201c}I didn\u{2019}t know,\u{201d} she said\u{2014}then paused\u{2026}",
        "\u{2018}Doesn\u{2019}t matter,\u{2019} he said. \u{2018}It\u{2019}s Marta\u{2019}s coat.\u{2019}",
        "He opened the door\u{2014}then stopped. It was quiet\u{2014}too quiet.",
        "It wasn\u{2019}t Marta\u{2019}s coat, and it wasn\u{2019}t her hat.",
        "She waited\u{2026} and waited. Nobody came.",
        "She waited.\u{2028}Nothing came.",
        "\u{201c}We\u{2019}re not going,\u{201d} he said, \u{201c}and that\u{2019}s final.\u{201d}",
        "I won\u{2019}t go, and you don\u{2019}t have to either.",
    ] {
        let flags = all_simple_replace_flags(text);
        assert!(flags.is_empty(), "unexpected flags for {:?}: {:?}", text, flags);
    }
}

#[test]
fn builtin_simple_replace_ignores_correct_words_that_look_like_table_entries() {
    // "Its" is not "it's", "resume" is not a foreign word here, and a plain
    // "cafe" is not in the diacritics table (only "cafe au lait" and friends).
    for text in &[
        "Its cover was torn.",
        "Please resume the work.",
        "We drank coffee at the cafe.",
        "The frame was wrought of iron.",
    ] {
        let flags = all_simple_replace_flags(text);
        assert!(
            flags.is_empty(),
            "unexpected flags for {:?}: {:?}",
            text,
            flags
        );
    }
}

#[test]
fn grammar_indefinite_determiner_uses_the_article_word_lists() {
    // GRAMMAR/ESTABLISHED_FACT rewrites "an established fact" to "a fact" with
    // `<match postag="+INDT">`; the article comes from the build dir word lists.
    let replacements: Vec<String> = RULES
        .suggest("This is an established fact.", &*TOKENIZER)
        .iter()
        .flat_map(|suggestion| suggestion.replacements().iter().cloned())
        .collect();
    assert!(
        replacements
            .iter()
            .any(|replacement| replacement == "a fact"),
        "{:?}",
        replacements
    );
}

#[test]
fn grammar_indefinite_determiner_picks_an_from_a_lemma() {
    // ONE_PLURAL's `<match postag="NN|NN:.*\+INDT">` chooses "an" from the
    // lemma "hour" (a word list entry, not a spelling heuristic) and prefixes
    // the synthesized singular.
    let replacements: Vec<String> = RULES
        .suggest("I worked for one hours.", &*TOKENIZER)
        .iter()
        .flat_map(|suggestion| suggestion.replacements().iter().cloned())
        .collect();
    assert!(
        replacements
            .iter()
            .any(|replacement| replacement == "an hour"),
        "{:?}",
        replacements
    );
}

#[test]
fn grammar_include_skipped_adds_the_skipped_tokens() {
    // WRITE_IN_MY_OWN_PAGE's `<match no="2" include_skipped="all">` covers the
    // token ("that") and the token skipped by the pattern's `skip="1"`
    // ("piece"), so the suggestion is "Write that piece on my own page".
    let replacements: Vec<String> = RULES
        .suggest("Write that piece in my own page.", &*TOKENIZER)
        .iter()
        .flat_map(|suggestion| suggestion.replacements().iter().cloned())
        .collect();
    assert!(
        replacements
            .iter()
            .any(|replacement| replacement == "Write that piece on my own page"),
        "{:?}",
        replacements
    );
}

#[test]
fn speller_falls_back_to_the_default_variant_while_spellchecking_is_off() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(rules.spell().options().variant.is_none());
    assert_eq!(RULES.spell().speller().unwrap().variant().as_str(), "en_US");

    rules.add_words(["Zar\u{2019}khan"]);
    let text =
        "\u{201c}Teh colour\u{2014}it\u{2019}s gone\u{2026}\u{201d}\u{2028}O\u{2019}Brien said.";
    assert!(!rules
        .suggest(text, &TOKENIZER)
        .iter()
        .any(|x| x.source().starts_with("SPELLCHECK")));
    {
        let speller = rules.spell().speller().unwrap();
        assert_eq!(speller.variant().as_str(), "en_US");
        let misspelled: Vec<String> = token_texts(text)
            .into_iter()
            .filter(|word| !speller.is_correct(word))
            .collect();
        assert_eq!(misspelled, vec!["Teh", "colour"]);
        assert_eq!(speller.suggest("teh", 1), vec!["the"]);
    }

    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_GB").unwrap());
    {
        let speller = rules.spell().speller().unwrap();
        assert_eq!(speller.variant().as_str(), "en_GB");
        assert!(speller.is_correct("colour"));
        assert!(speller.is_correct("Zar\u{2019}khan"));
    }

    rules.spell_mut().options_mut().variant = None;
    let speller = rules.spell().speller().unwrap();
    assert_eq!(speller.variant().as_str(), "en_US");
    assert!(!speller.is_correct("colour"));
    assert!(speller.is_correct("Zar\u{2019}khan"));
}

#[test]
fn spell_option_changes_apply_while_the_variant_stays_the_same() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let text =
        "\u{201c}Teh colour\u{2014}it\u{2019}s gone\u{2026}\u{201d}\u{2028}O\u{2019}Brien said.";
    let en_us = rules.spell().variant("en_US").unwrap();

    for variant in [None, Some(en_us)] {
        rules.spell_mut().options_mut().variant = variant.clone();
        assert_eq!(
            rules.spell().speller().unwrap().suggest("teh", 10).len(),
            10
        );

        rules.spell_mut().options_mut().top_n = 1;
        assert_eq!(
            rules.spell().speller().unwrap().suggest("teh", 10),
            vec!["the"]
        );
        let spelling: Vec<_> = rules
            .suggest(text, &TOKENIZER)
            .into_iter()
            .filter(|x| x.source().starts_with("SPELLCHECK"))
            .collect();
        if variant.is_some() {
            assert_eq!(spelling.len(), 2);
            assert!(spelling.iter().all(|x| x.replacements().len() == 1));
        } else {
            assert!(spelling.is_empty());
        }

        rules.spell_mut().options_mut().top_n = 10;
    }
}

#[test]
fn speller_answers_single_word_queries_in_fiction_text() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    rules.add_words(["Zar\u{2019}khan", "Xylophonix"]);
    let speller = rules.spell().speller().unwrap();
    assert_eq!(speller.variant().as_str(), "en_US");

    let text = "\u{201c}I didn\u{2019}t know,\u{201d} she said\u{2026} \u{2018}Patenice\u{2014}always.\u{2019}\u{2028}Teh end, O\u{2019}Brien.";
    let misspelled: Vec<String> = token_texts(text)
        .into_iter()
        .filter(|word| !speller.is_correct(word))
        .collect();
    assert_eq!(misspelled, vec!["Patenice", "Teh"]);

    assert!(speller.is_correct("O\u{2019}Brien"));
    assert!(speller.is_correct("Zar\u{2019}khan"));
    assert!(speller.is_correct("Zar'khan"));
    assert!(speller.is_correct("\u{2028}"));
    assert!(speller.is_correct("well-known\u{2028}"));
    assert!(!speller.is_correct("Patenice\u{2028}"));

    assert!(speller.suggest("\u{2028}", 5).is_empty());
    assert_eq!(speller.suggest("teh", 1), vec!["the"]);
    assert!(speller
        .suggest("O\u{2019}Brein", 5)
        .contains(&"O'Brien".to_string()));
    assert!(speller
        .suggest("Xylophoni", 5)
        .contains(&"Xylophonix".to_string()));
    assert_eq!(speller.suggest("teh", 3).len(), 3);
    assert!(speller.suggest("teh", 0).is_empty());
}

#[test]
fn speller_accepts_single_word_whitelist_entries() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let en_us = rules.spell().variant("en_US").unwrap();
    {
        let mut options = rules.spell_mut().options_mut();
        options.variant = Some(en_us);
        options.whitelist.insert("Quillfeather".into());
        options.whitelist.insert("Vharn Keep".into());
    }
    let speller = rules.spell().speller().unwrap();

    assert!(speller.is_correct("Quillfeather"));
    assert!(speller.is_correct("Quillfeather\u{2028}"));
    assert!(!speller.is_correct("Vharn"));
}

#[test]
fn speller_checks_tokens_not_contractions() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let speller = rules.spell().speller().unwrap();

    let text = "\u{201c}I didn\u{2019}t\u{2026} I won\u{2019}t,\u{201d} she said\u{2014}O\u{2019}Brien\u{2019}s.\u{2028}";
    for sentence in TOKENIZER.pipe(text) {
        assert!(rules.spell().suggest(&sentence).is_empty());
        for token in sentence.tokens() {
            let word = token.word().text().as_str();
            assert!(
                token.ignore_spelling() || speller.is_correct(word),
                "{:?}",
                word
            );
        }
    }
    assert!(!speller.is_correct("didn\u{2019}t"));
    assert!(speller.is_correct("did") && speller.is_correct("n\u{2019}t"));
}
