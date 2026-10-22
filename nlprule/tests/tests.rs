use std::{
    collections::HashSet,
    convert::{TryFrom, TryInto},
};

use lazy_static::lazy_static;
use nlprule::{
    builtin::Builtin, profile::Profile, rule::id::Category, rule::id::Selector, types::Position,
    types::Suggestion, Error, Rules, Tokenizer,
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

#[test]
fn tokenizer_splits_mixed_case_negation_clitics() {
    assert_eq!(
        token_texts("\u{201c}We don\u{2019}T\u{2014}\u{201d} she said\u{2026}\u{2028}\u{201c}We DON\u{2019}T.\u{201d}"),
        vec![
            "\u{201c}", "We", "do", "n", "\u{2019}", "T", "\u{2014}", "\u{201d}", "she", "said",
            "\u{2026}", "\u{201c}", "We", "DO", "N'T", ".", "\u{201d}",
        ]
    );
    assert_eq!(
        token_texts("We don'T need this."),
        vec!["We", "do", "n", "'", "T", "need", "this", "."]
    );
}

#[test]
fn tokenizer_splits_hyphens_at_word_edges() {
    assert_eq!(
        token_texts("Germans didn't-->  \u{201c}Go-\u{201d} he said\u{2014}then -fine\u{2026}"),
        vec![
            "Germans", "didn", "'", "t", "-", "-", ">", "\u{201c}", "Go", "-", "\u{201d}", "he",
            "said", "\u{2014}", "then", "-", "fine", "\u{2026}",
        ]
    );
}

fn readings(text: &str, word: &str) -> Vec<String> {
    let sentence = TOKENIZER.pipe(text).next().unwrap();
    let token = sentence
        .tokens()
        .iter()
        .find(|token| token.word().text().as_str() == word)
        .unwrap();
    token
        .word()
        .tags()
        .iter()
        .filter(|data| !data.pos().as_str().is_empty())
        .map(|data| format!("{}/{}", data.lemma().as_str(), data.pos().as_str()))
        .collect()
}

#[test]
fn best_before_a_preposition_is_a_superlative_adjective() {
    for text in [
        "It may best to add this.",
        "It\u{2019}s best for him.",
        "\u{201c}It may best for him\u{2014}or for her\u{2026}\u{201d}",
        "It may best\u{2028}for him.",
    ] {
        assert_eq!(readings(text, "best"), vec!["well/JJS"], "{}", text);
    }

    for text in [
        "He brought out the best in people.",
        "The best for him is rest.",
    ] {
        assert_eq!(
            readings(text, "best"),
            vec![
                "best/NN:U",
                "best/VB",
                "best/VBP",
                "good/JJS",
                "well/JJS",
                "well/RBS"
            ],
            "{}",
            text
        );
    }
}

#[test]
fn default_disambiguation_keeps_the_last_reading_with_the_tag() {
    for text in [
        "His motives were unclear.",
        "\u{201c}His motives were unclear\u{2014}truly\u{2026}\u{201d}",
        "Mara\u{2019}s motives were unclear.",
        "His motives\u{2028}were unclear.",
    ] {
        assert_eq!(readings(text, "motives"), vec!["motif/NNS"], "{}", text);
    }

    assert_eq!(
        readings("\u{201c}Her motives?\u{201d} he asked.", "motives"),
        vec!["motive/NNS", "motif/NNS"]
    );
}

#[test]
fn lemma_only_removal_drops_the_instal_readings() {
    for (text, word, expected) in [
        ("Did you installed the new app?", "installed", "install/VBD"),
        (
            "\u{201c}We installed it\u{2014}twice\u{2026}\u{201d}",
            "installed",
            "install/VBD",
        ),
        ("They\u{2019}re installing it.", "installing", "install/VBG"),
        (
            "It was installed\u{2028}yesterday.",
            "installed",
            "install/VBN",
        ),
    ] {
        assert_eq!(readings(text, word), vec![expected], "{}", text);
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
fn sentencizer_splits_like_languagetool() {
    let segments = |text: &'static str| -> Vec<&'static str> {
        TOKENIZER
            .pipe(text)
            .map(|sentence| &text[sentence.span().byte().clone()])
            .collect()
    };
    assert_eq!(
        segments("\u{201c}Wait\u{2026} and then?\u{201d} Mr. Snyder asked\u{2014}quietly. \u{2026} and she\u{2019}d gone."),
        vec![
            "\u{201c}Wait\u{2026} and then?\u{201d} ",
            "Mr. Snyder asked\u{2014}quietly. ",
            "\u{2026} and she\u{2019}d gone.",
        ]
    );
    assert_eq!(
        segments("Thelen et. al. 2001 says it\u{2019}s fine\u{2014}mostly."),
        vec!["Thelen et. al. 2001 says it\u{2019}s fine\u{2014}mostly."]
    );
    assert_eq!(
        segments("We met at 8 a.m. on Friday\u{2028}or so I thought\u{2026} It wasn\u{2019}t."),
        vec![
            "We met at 8 a.m. on Friday\u{2028}or so I thought\u{2026} ",
            "It wasn\u{2019}t."
        ]
    );
    assert_eq!(
        segments("He said it.\u{2028}\u{201c}No,\u{201d} she whispered.\u{2028}The rain kept falling\u{2026} It always did."),
        vec![
            "He said it.\u{2028}\u{201c}No,\u{201d} she whispered.\u{2028}The rain kept falling\u{2026} ",
            "It always did."
        ]
    );
    assert_eq!(
        segments(
            "Mr.\u{a0}Smith left. Dr.\u{2028}Doe stayed. The end.\u{a0}The beginning\u{2014}maybe."
        ),
        vec![
            "Mr.\u{a0}Smith left. ",
            "Dr.\u{2028}Doe stayed. ",
            "The end.\u{a0}",
            "The beginning\u{2014}maybe."
        ]
    );
    assert_eq!(
        segments("\u{201c}I\u{2019}m done.\u{201d} She left."),
        vec!["\u{201c}I\u{2019}m done.\u{201d} ", "She left."]
    );
    assert_eq!(
        segments("\u{201c}Wait!\u{201d}  she cried\u{2014}then stopped\u{2026} \u{201c}Go.\u{201d}  He went."),
        vec![
            "\u{201c}Wait!\u{201d}  she cried\u{2014}then stopped\u{2026} ",
            "\u{201c}Go.\u{201d} ",
            " He went."
        ]
    );
    assert_eq!(
        segments("She\u{2019}d said \u{2018}no.\u{2019}  then left."),
        vec!["She\u{2019}d said \u{2018}no.\u{2019}  then left."]
    );
}

#[test]
fn sentencizer_keeps_honorifics_like_languagetool() {
    let segments = |text: &'static str| -> Vec<&'static str> {
        TOKENIZER
            .pipe(text)
            .map(|sentence| &text[sentence.span().byte().clone()])
            .collect()
    };
    for text in [
        "I had known Mr. Aduke for eleven years and did not know his first name.",
        "I saw Mrs. Okafor, Ms. Lin and Dr. Patel in St. Louis.",
        "I saw Prof. Hale, Sgt. Ross and Capt. Lee.",
        "He met Rev. Hale and Gen. Lee and Gov. Ray.",
        "I saw Mr. and Mrs. A. Okafor at the market.",
        "I saw Messrs. Aduke and Okafor.",
        "He met John Hale Jr. Then he left.",
        "\u{201c}Mr. Aduke,\u{201d} she said, \u{201c}come in.\u{201d}",
        "\u{201c}Mrs. Okafor!\u{201d} she called.",
        "He waited\u{2014}Mr. Aduke never came.",
        "\u{2014}Dr. Patel arrived late\u{2026}",
        "She\u{2019}d asked Ms. Lin\u{2019}s name twice.",
        "I saw Mr.\u{2028}Aduke at the market.",
        "I saw Mr.  Aduke at the market.",
    ] {
        assert_eq!(segments(text), vec![text]);
    }
    assert_eq!(
        segments("\u{201c}I know.\u{201d} Mr. Aduke nodded."),
        vec!["\u{201c}I know.\u{201d} ", "Mr. Aduke nodded."]
    );
    assert_eq!(
        segments("Well\u{2026} Mr. Aduke came anyway."),
        vec!["Well\u{2026} ", "Mr. Aduke came anyway."]
    );
    assert_eq!(
        segments("I saw Mr.\u{201d} Aduke laughed."),
        vec!["I saw Mr.\u{201d} ", "Aduke laughed."]
    );
    assert_eq!(
        segments("I saw mr. Aduke. MR. ADUKE came early."),
        vec!["I saw mr. ", "Aduke. ", "MR. ", "ADUKE came early."]
    );
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
            "BUILTIN/WHITESPACE_RULE/NO_SUCH_CASE".to_owned(),
            "BUILTIN/UPPERCASE_SENTENCE_START/INDENTATION".to_owned(),
        ],
    );
    let mut unmatched = rules.apply_profile(&profile);
    unmatched.sort();
    assert_eq!(
        unmatched,
        [
            "BUILTIN/NO_SUCH_BUILTIN",
            "BUILTIN/UPPERCASE_SENTENCE_START/INDENTATION",
            "BUILTIN/WHITESPACE_RULE/NO_SUCH_CASE",
            "NO_SUCH_CATEGORY"
        ]
    );
    let off_by_default = [
        "EMPTY_LINE",
        "TOO_LONG_SENTENCE",
        "TOO_LONG_PARAGRAPH",
        "EN_DASH_RULE",
        "EN_PLAIN_ENGLISH_REPLACE",
        "EN_REDUNDANCY_REPLACE",
        "PROFANITY",
        "EN_REPEATEDWORDS",
    ];
    assert!(rules.builtins().iter().all(|rule| rule.enabled()
        || off_by_default.contains(&rule.id())
        || rule.id().starts_with("TOO_OFTEN_USED_")));
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

/// Sources of the suggestions `rules` makes on `text`.
fn sources(rules: &Rules, text: &str) -> Vec<String> {
    rules
        .suggest(text, &*TOKENIZER)
        .iter()
        .map(|suggestion| suggestion.source().to_owned())
        .collect()
}

/// An off-by-default plain-English rule fires once enabled by `CATEGORY/RULE_ID`.
#[test]
fn enable_rules_enables_one_rule() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let text = "The plan, in my opinion, is sound.";
    let fires = |rules: &Rules| {
        sources(rules, text)
            .iter()
            .any(|source| source.starts_with("PLAIN_ENGLISH/IN_MY_OPINION"))
    };
    assert!(!fires(&rules));

    assert!(
        rules
            .enable_rules(&["PLAIN_ENGLISH/IN_MY_OPINION"])
            .unwrap()
            > 0
    );
    assert!(fires(&rules));
    // Enabling it again changes nothing.
    assert_eq!(
        rules
            .enable_rules(&["PLAIN_ENGLISH/IN_MY_OPINION"])
            .unwrap(),
        0
    );
}

/// A category selector enables every rule in the category.
#[test]
fn enable_rules_enables_a_category() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let category = Selector::try_from("PLAIN_ENGLISH").unwrap();
    assert!(rules.select(&category).any(|rule| !rule.enabled()));

    let enabled = rules.enable_rules(&["PLAIN_ENGLISH"]).unwrap();
    assert!(enabled > 1);
    assert!(rules.select(&category).all(|rule| rule.enabled()));
}

/// Unknown selectors are an error and nothing is enabled, including the valid ones.
#[test]
fn enable_rules_rejects_unknown_selectors() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let count = |rules: &Rules| {
        rules.rules().iter().filter(|rule| rule.enabled()).count()
            + rules
                .builtins()
                .iter()
                .filter(|rule| rule.enabled())
                .count()
    };
    let before = count(&rules);

    let err = rules
        .enable_rules(&[
            "PLAIN_ENGLISH",
            "NO_SUCH_CATEGORY",
            "BUILTIN/NO_SUCH_BUILTIN",
            // The case form only disables.
            "BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH",
        ])
        .unwrap_err();
    match err {
        Error::UnknownSelectors(unmatched) => assert_eq!(
            unmatched,
            [
                "NO_SUCH_CATEGORY",
                "BUILTIN/NO_SUCH_BUILTIN",
                "BUILTIN/TOO_OFTEN_USED_NOUN_EN/DIRECT_SPEECH"
            ]
        ),
        other => panic!("unexpected error: {}", other),
    }
    assert_eq!(count(&rules), before);
    // An empty list is fine and changes nothing.
    assert_eq!(rules.enable_rules::<&str>(&[]).unwrap(), 0);
}

/// Enabling after the fiction profile turns back on what the profile disabled.
#[test]
fn enable_rules_after_fiction_profile() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(rules.apply_profile(&Profile::fiction()).is_empty());
    let redundancy = Selector::try_from("REDUNDANCY").unwrap();
    assert!(rules.select(&redundancy).all(|rule| !rule.enabled()));

    assert!(rules.enable_rules(&["REDUNDANCY"]).unwrap() > 0);
    assert!(rules.select(&redundancy).all(|rule| rule.enabled()));
    // Other rules the profile disabled stay off.
    let quotes = Selector::try_from("TYPOGRAPHY/EN_QUOTES").unwrap();
    assert!(rules.select(&quotes).all(|rule| !rule.enabled()));
}

/// Builtins are enabled by `BUILTIN/ID`; a case the profile skipped stays skipped.
#[test]
fn enable_rules_enables_builtins() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(rules.apply_profile(&Profile::fiction()).is_empty());
    let enabled = |rules: &Rules, id: &str| {
        rules
            .builtins()
            .iter()
            .find(|rule| rule.id() == id)
            .unwrap()
            .enabled()
    };
    assert!(!enabled(&rules, "EN_REDUNDANCY_REPLACE"));
    assert!(!enabled(&rules, "TOO_OFTEN_USED_NOUN_EN"));
    assert_eq!(
        rules
            .enable_rules(&[
                "BUILTIN/EN_REDUNDANCY_REPLACE",
                "BUILTIN/TOO_OFTEN_USED_NOUN_EN"
            ])
            .unwrap(),
        2
    );
    assert!(enabled(&rules, "EN_REDUNDANCY_REPLACE"));
    assert!(enabled(&rules, "TOO_OFTEN_USED_NOUN_EN"));
}

/// Enabled rules report UTF-16-safe spans on fiction punctuation: curly quotes, curly
/// apostrophes, em dashes, ellipses and U+2028.
#[test]
fn enable_rules_fiction_punctuation() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules
        .enable_rules(&["PLAIN_ENGLISH", "STYLE/PASSIVE_VOICE", "REDUNDANCY"])
        .unwrap();
    let text = "\u{201c}It\u{2019}s, in my opinion, wrong\u{2026}\u{201d} she said\u{2014}quietly.\u{2028}The door was opened by Sam.";
    let suggestions = rules.suggest(text, &*TOKENIZER);
    let flagged = |prefix: &str| -> Vec<&str> {
        suggestions
            .iter()
            .filter(|s| s.source().starts_with(prefix))
            .map(|s| &text[s.span().byte().clone()])
            .collect()
    };
    // Exact spans pin the offsets after the curly quote and apostrophe, and the
    // passive after the ellipsis, the em dash and U+2028.
    assert_eq!(
        flagged("PLAIN_ENGLISH/IN_MY_OPINION/"),
        vec!["in my opinion"]
    );
    assert_eq!(flagged("STYLE/PASSIVE_VOICE/"), vec!["was opened"]);
    for suggestion in &suggestions {
        let chars: String = text.chars().collect::<Vec<_>>()[suggestion.span().char().clone()]
            .iter()
            .collect();
        assert_eq!(chars, &text[suggestion.span().byte().clone()]);
    }
}

/// Enabling rules adds their matches and leaves every other rule's matches
/// unchanged (#297).
#[test]
fn enable_rules_leaves_other_rules_matches_unchanged() {
    let selectors = ["PLAIN_ENGLISH", "REDUNDANCY", "STYLE/PASSIVE_VOICE"];
    let text = "\u{201c}It\u{2019}s, in my opinion, a absolute true fact,\u{201d} she said\u{2014}quietly. \
                Their going to the the store\u{2026} The door was opened by Sam.\u{2028}\
                He dont know what what to do. I seen it with my own eyes.";
    for profile in [None, Some(Profile::fiction())] {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        if let Some(profile) = &profile {
            assert!(rules.apply_profile(profile).is_empty());
        }
        let others = |rules: &Rules| -> Vec<(String, std::ops::Range<usize>)> {
            rules
                .suggest(text, &*TOKENIZER)
                .into_iter()
                .filter(|s| {
                    !["PLAIN_ENGLISH/", "REDUNDANCY/", "STYLE/PASSIVE_VOICE/"]
                        .iter()
                        .any(|prefix| s.source().starts_with(prefix))
                })
                .map(|s| (s.source().to_owned(), s.span().byte().clone()))
                .collect()
        };
        let before = others(&rules);
        assert!(before.len() >= 3, "{:?}", before);
        let total_before = rules.suggest(text, &*TOKENIZER).len();
        assert!(rules.enable_rules(&selectors).unwrap() > 0);
        assert_eq!(
            others(&rules),
            before,
            "profile {:?}",
            profile.map(|p| p.name)
        );
        assert!(rules.suggest(text, &*TOKENIZER).len() > total_before);
    }
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
fn spellchecker_ignores_words_with_digits() {
    for text in &[
        "\u{201c}Back in the 1860s\u{2014}or the \u{2019}80s\u{2014}nobody cared,\u{201d} she said.",
        "The 75mm gun fired at 3:21pm\u{2026} then at 8:12pm.\u{2028}It was the 22nd.",
        "He bought a 4k screen and a 443m cable for the FM-27s.",
    ] {
        assert_eq!(spelling_errors(text), Vec::<&str>::new(), "{}", text);
    }
    assert_eq!(
        spelling_errors("\u{201c}The 1940s were teh worst,\u{201d} he said\u{2014}75mm recieved."),
        vec!["teh", "recieved"]
    );
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

#[test]
fn spellchecker_accepts_all_caps_known_words() {
    assert_eq!(
        spelling_errors(
            "\u{201c}THE END,\u{201d} she wrote\u{2026} He barked HELLO\u{2014}LOUDLY.\u{2028}\u{2018}DON\u{2019}T MOVE,\u{2019} said O\u{2019}BRIEN in PARIS."
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        spelling_errors("\u{201c}THSI is WRONG\u{2014}TEH END\u{2026}\u{201d}\u{2028}"),
        vec!["THSI", "TEH"]
    );
}

#[test]
fn spellchecker_accepts_all_caps_names_from_the_global_list() {
    assert_eq!(
        spelling_errors(
            "\u{201c}MCCARTHY!\u{201d} he roared\u{2014}MCKINLEY\u{2019}S men ran\u{2026}\u{2028}\u{2018}MCDONALD\u{2019}S, YOUTUBE, POWERPOINT,\u{2019} she said."
        ),
        Vec::<&str>::new()
    );
    assert_eq!(
        spelling_errors(
            "\u{201c}MCDONALD!\u{201d} MACARTHUR\u{2014}EBAY\u{2026}\u{2028}\u{2018}MCCARTNEY.\u{2019}"
        ),
        vec!["MCDONALD", "MACARTHUR", "EBAY", "MCCARTNEY"]
    );
    assert_eq!(
        spelling_errors(
            "\u{201c}JOSQUIN DES PR\u{c9}S\u{2026}\u{201d} Josquin des Pr\u{e9}s sang\u{2014}DES PR\u{c9}S.\u{2028}Pr\u{e9}s."
        ),
        vec!["PR\u{c9}S", "Pr\u{e9}s"]
    );
}

#[test]
fn speller_accepts_and_suggests_words_from_the_global_list() {
    let speller = RULES.spell().speller().unwrap();
    for word in ["Josquin", "JOSQUIN", "fit4work", "Nestl\u{e9}"] {
        assert!(speller.is_correct(word), "{}", word);
    }
    assert!(!speller.is_correct("Pr\u{e9}s"));
    assert_eq!(speller.suggest("Josquinn", 5), vec!["Josquin"]);
    let nescio = speller.suggest("Nescio", 10);
    assert_eq!(
        nescio.last().map(String::as_str),
        Some("ESCO"),
        "{:?}",
        nescio
    );

    for variant in ["en_US", "en_GB"] {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        rules.spell_mut().options_mut().variant = Some(rules.spell().variant(variant).unwrap());
        let text = "\u{201c}Josquinn\u{2014}again\u{2026}\u{201d}\u{2028}\u{2018}Josquin\u{2019}s motet, fit4work, Nestl\u{e9},\u{2019} she said.";
        let flags: Vec<(&str, Vec<String>)> = rules
            .suggest(text, &*TOKENIZER)
            .into_iter()
            .filter(|x| x.source().starts_with("SPELLCHECK"))
            .map(|x| (&text[x.span().byte().clone()], x.replacements().to_vec()))
            .collect();
        assert_eq!(flags, vec![("Josquinn", vec!["Josquin".to_string()])]);
    }
}

#[test]
fn speller_folds_the_case_of_accented_letters() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(
        speller.suggest("\u{c9}CLAIRR", 5),
        vec!["\u{c9}CLAIR", "\u{c9}CLAIRS"]
    );
    assert_eq!(
        speller.suggest("\u{c9}clairr", 5),
        vec!["\u{c9}clair", "\u{c9}clairs"]
    );
    assert_eq!(speller.suggest("NA\u{cf}VEE", 1), vec!["NA\u{cf}VE"]);
    assert_eq!(speller.suggest("S\u{d8}RENN", 5), vec!["S\u{d8}REN"]);
    assert_eq!(
        speller.suggest("WH\u{152}L", 5),
        vec!["WHO", "WELL", "WHEN", "WHAT", "WHILE"]
    );
    assert!(speller
        .suggest("\u{5d7}\u{5d5}", 10)
        .iter()
        .all(|x| x.chars().count() > 1));
    let qx = speller.suggest("qx", 50);
    assert!(qx.contains(&"ox".to_string()), "{:?}", qx);
    assert!(!qx.contains(&"a".to_string()), "{:?}", qx);

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text = "\u{201c}S\u{d8}RENN\u{2014}wait\u{2026}\u{201d}\u{2028}\u{2018}Zo\u{eb} ate the \u{c9}clairr\u{2019}s icing,\u{2019} she said.";
    let first_replacements: Vec<(&str, String)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| (&text[x.span().byte().clone()], x.replacements()[0].clone()))
        .collect();
    assert_eq!(
        first_replacements,
        vec![
            ("S\u{d8}RENN", "S\u{d8}REN".to_string()),
            ("\u{c9}clairr", "\u{c9}clair".to_string()),
        ]
    );
}

#[test]
fn speller_suggests_the_name_first_for_a_lowercased_name() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(
        speller.suggest("s\u{f8}ren", 2),
        vec!["S\u{f8}ren", "siren"]
    );
    assert_eq!(speller.suggest("ren\u{e9}e", 1), vec!["Ren\u{e9}e"]);
    assert_eq!(
        speller.suggest("nestl\u{e9}", 2),
        vec!["Nestl\u{e9}", "nestle"]
    );
    assert_eq!(speller.suggest("paris", 2), vec!["Paris", "parts"]);

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text = "\u{201c}Wait\u{2014}s\u{f8}ren?\u{201d}\u{2028}\u{2018}zo\u{eb}\u{2019}s coat\u{2026}\u{2019} she said.";
    let flags: Vec<(&str, Vec<String>)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| {
            (
                &text[x.span().byte().clone()],
                x.replacements().iter().take(2).cloned().collect(),
            )
        })
        .collect();
    assert_eq!(
        flags,
        vec![
            ("s\u{f8}ren", strings(&["S\u{f8}ren", "siren"])),
            ("zo\u{eb}", strings(&["Zo\u{eb}", "zoo"])),
        ]
    );
}

#[test]
fn speller_suggests_dictionary_casing_for_mixed_case_words() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(speller.suggest("iPHONE", 2), vec!["iPhone", "phone"]);
    assert_eq!(
        speller.suggest("McKINLEY", 5),
        vec!["McKinley", "McKinney", "McKinsey", "McGinley", "Mckinney"]
    );

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text = "\u{201c}My iPHONE\u{2014}gone\u{2026}\u{201d}\u{2028}\u{2018}McKINLEY\u{2019}s fault.\u{2019}";
    let first_replacements: Vec<(&str, String)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| (&text[x.span().byte().clone()], x.replacements()[0].clone()))
        .collect();
    assert_eq!(
        first_replacements,
        vec![
            ("iPHONE", "iPhone".to_string()),
            ("McKINLEY", "McKinley".to_string()),
        ]
    );
}

#[test]
fn speller_ranks_edit_distance_before_frequency() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(speller.suggest("Hecla", 5), vec!["Hekla"]);
    assert_eq!(speller.suggest("Aduke", 1), vec!["Duke"]);
    assert!(speller.suggest("Wat", 10).contains(&"At".to_string()));
    assert!(speller.suggest("IPHONE", 10).contains(&"PHONE".to_string()));
    assert_eq!(
        speller.suggest("McCARTHY", 10),
        vec!["McCarthy", "McCarty", "McCartney", "McCarry"]
    );
    let anno = speller.suggest("Anno", 10);
    assert!(
        anno.iter()
            .all(|x| !["And", "An", "Any"].contains(&x.as_str())),
        "{:?}",
        anno
    );

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text = "\u{201c}Aduke\u{2019}s gone\u{2014}to Hecla\u{2026}\u{201d}\u{2028}\u{2018}Wat now?\u{2019}";
    let first_replacements: Vec<(&str, String)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| (&text[x.span().byte().clone()], x.replacements()[0].clone()))
        .collect();
    assert_eq!(
        first_replacements,
        vec![
            ("Aduke", "Duke".to_string()),
            ("Hecla", "Hekla".to_string()),
            ("Wat", "Was".to_string()),
        ]
    );
}

#[test]
fn speller_suggestions_restore_the_case_of_capitalized_words() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(
        speller.suggest("Teh", 5),
        vec!["The", "Ten", "Tea", "Tech", "Ted"]
    );
    assert!(speller.is_correct("HELLO"));
    assert!(speller.is_correct("PARIS"));
    assert!(!speller.is_correct("TEH"));

    let shouted = speller.suggest("TEDD", 5);
    assert!(shouted.contains(&"TED".to_string()), "{:?}", shouted);
    assert!(
        shouted.iter().all(|x| x.to_uppercase() == *x),
        "{:?}",
        shouted
    );

    let shouted = speller.suggest("WHTA", 10);
    assert_eq!(
        shouted.first().map(String::as_str),
        Some("WHAT"),
        "{:?}",
        shouted
    );
    assert_eq!(
        shouted.iter().collect::<HashSet<_>>().len(),
        shouted.len(),
        "{:?}",
        shouted
    );

    assert_eq!(speller.suggest("Iphone", 1), vec!["iPhone"]);
    assert_eq!(speller.suggest("MCDONALD", 1), vec!["McDonald"]);
    assert_eq!(speller.suggest("teh", 1), vec!["the"]);
}

#[test]
fn spellchecker_ignores_words_with_no_latin_letter() {
    for text in &[
        "\u{201c}\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442},\u{201d} he said\u{2014}\u{417}\u{434}\u{440}\u{430}\u{432}\u{441}\u{442}\u{432}\u{443}\u{439}\u{442}\u{435}\u{2026}",
        "The sign read \u{2018}\u{39f}\u{394}\u{39f}\u{3a3}\u{2019}.\u{2028}She wrote \u{6771}\u{4eac}.",
        "\u{6771}\u{4eac}\u{2019}s lights\u{2014}\u{3b1}\u{3b2}\u{3b3}\u{2026} gone.",
        "He wrote \u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}-\u{444}\u{440}\u{44d}\u{43d}\u{434}.\u{2029}\u{39f}\u{394}\u{39f}\u{3a3}\u{a0}again.",
    ] {
        assert_eq!(spelling_errors(text), Vec::<&str>::new(), "{}", text);
    }

    // A token mixing scripts is still checked: Greek capital tau + "he".
    assert_eq!(
        spelling_errors("\u{201c}\u{3a4}he end\u{2014}\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}\u{2026}\u{201d}"),
        vec!["\u{3a4}he"]
    );
    // The test looks at the whole token, not at its hyphen-separated parts: LanguageTool 6.6
    // flags `Привет-friend` although `friend` is known.
    assert_eq!(
        spelling_errors(
            "\u{201c}\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}-friend,\u{201d} he said."
        ),
        vec!["\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}-friend"]
    );

    let speller = RULES.spell().speller().unwrap();
    assert!(speller.is_correct("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"));
    assert!(speller.is_correct("\u{39f}\u{394}\u{39f}\u{3a3}"));
    assert!(speller.is_correct("\u{6771}\u{4eac}"));
    assert!(!speller.is_correct("\u{3a4}he"));
    assert!(!speller.is_correct("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}-friend"));
    assert_eq!(speller.suggest("\u{3a4}he", 3), vec!["The", "He", "She"]);
}

#[test]
fn speller_capitalizes_candidates_for_mixed_case_words() {
    let speller = RULES.spell().speller().unwrap();
    assert_eq!(
        speller.suggest("LeBRON", 5),
        vec!["LeBron", "Hebron", "Lebrun", "Learn", "Lesson"]
    );
    for (word, expected) in [
        ("PayPAL", &["PayPal", "Papal"][..]),
        ("XerXes", &["Xerxes", "Xeroxes", "Xeres"][..]),
        ("DeVITO", &["DeVito", "Device"][..]),
    ] {
        let suggestions = speller.suggest(word, 10);
        assert_eq!(&suggestions[..expected.len()], expected, "{}", word);
        assert!(
            suggestions
                .iter()
                .all(|x| x.starts_with(char::is_uppercase)),
            "{:?}",
            suggestions
        );
        assert_eq!(
            suggestions.iter().collect::<HashSet<_>>().len(),
            suggestions.len(),
            "{:?}",
            suggestions
        );
    }
    // A word starting lowercase keeps the candidates' case.
    assert_eq!(speller.suggest("iPHONE", 2), vec!["iPhone", "phone"]);

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text =
        "\u{201c}LeBRON\u{2019}s back\u{2014}PayPAL\u{2026}\u{201d}\u{2028}\u{2018}XerXes!\u{2019}";
    let first_replacements: Vec<(&str, String)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| (&text[x.span().byte().clone()], x.replacements()[0].clone()))
        .collect();
    assert_eq!(
        first_replacements,
        vec![
            ("LeBRON", "LeBron".to_string()),
            ("PayPAL", "PayPal".to_string()),
            ("XerXes", "Xerxes".to_string()),
        ]
    );
}

#[test]
fn spelling_suggestions_keep_the_case_of_shouted_dialogue() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    let text = "\u{201c}WHERE ARE YOU GOIGN?\u{201d} he shouted\u{2026}\u{2028}\u{2018}Teh END\u{2014}DON\u{2019}T BELEIVE IT.\u{2019}";

    let first_replacements: Vec<(&str, String)> = rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|x| x.source().starts_with("SPELLCHECK"))
        .map(|x| (&text[x.span().byte().clone()], x.replacements()[0].clone()))
        .collect();
    assert_eq!(
        first_replacements,
        vec![
            ("GOIGN", "GOING".to_string()),
            ("Teh", "The".to_string()),
            ("BELEIVE", "BELIEVE".to_string()),
        ]
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
    // Every entry accepts its all-caps form.
    assert!(spelling_errors_with_words(&["Xylophonix"], "the XYLOPHONIX").is_empty());
    assert!(spelling_errors_with_words(&["xylophonix"], "the XYLOPHONIX").is_empty());
    // A partly uppercased form is still flagged.
    assert_eq!(
        spelling_errors_with_words(&["Xylophonix"], "the XYLOPHONiX"),
        vec!["XYLOPHONiX"]
    );
}

#[test]
fn user_dictionary_accepts_shouted_entries() {
    let words = ["Xylophonix", "James", "Port Royal", "Zar'khan"];
    for text in [
        "\u{201c}XYLOPHONIX!\u{201d} she screamed.",
        "XYLOPHONIX\u{2019}S wand\u{2014}gone\u{2026}",
        "Nothing.\u{2028}XYLOPHONIX'S wand. XYLOPHONIX's wand.",
        "\u{2018}JAMES\u{2019} coat,\u{2019} he said. JAMES' coat.",
        "\u{201c}PORT ROYAL burns!\u{201d}",
        "\u{201c}ZAR\u{2019}KHAN\u{2026}\u{201d} ZAR'KHAN\u{2014}",
    ] {
        assert!(
            spelling_errors_with_words(&words, text).is_empty(),
            "{}",
            text
        );
    }

    assert_eq!(
        spelling_errors_with_words(
            &words,
            "\u{201c}XYLOPHONIZ!\u{201d}\u{2014}PORT ROYEL\u{2026}"
        ),
        vec!["XYLOPHONIZ", "ROYEL"]
    );

    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());
    rules.add_words(words);
    assert_eq!(
        rules.spell().speller().unwrap().suggest("XYLOPHONIZ", 1),
        vec!["XYLOPHONIX"]
    );

    assert!(spelling_errors_with_words(
        &["Zorvath Josquin"],
        "Zorvath Josquin des Pr\u{e9}s sang\u{2014}\u{201c}ZORVATH JOSQUIN DES PR\u{c9}S\u{2026}\u{201d}\u{2028}"
    )
    .is_empty());

    rules.add_words(["NASA"]);
    let speller = rules.spell().speller().unwrap();
    for word in ["XYLOPHONIX\u{2019}S", "NASA'S", "NASA\u{2019}S", "NASA's"] {
        assert!(speller.is_correct(word), "{}", word);
    }
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
        .flat_map(|rule| {
            rule.builtin()
                .check(&sentence, RULES.spell().speller().as_ref())
        })
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
    // As in LanguageTool 6.6, U+2028 after a period does not end the sentence, so
    // "An year" is not at a sentence start.
    assert!(builtin_a_vs_an_flags("It was a one-time thing.\u{2028}An honest mistake.").is_empty());
    assert_eq!(
        builtin_a_vs_an_flags("It was a apple.\u{2028}An year passed."),
        vec![("a apple".to_string(), "an".to_string())]
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
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
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
fn builtin_comma_ignores_spaced_ellipsis() {
    for text in &[
        "\u{201c}So . . . how was the drive?\u{201d}",
        "She waited for an answer . . .",
        "The radio crackled. \u{201c} . . . day. Mayday.\u{201d}",
        "\u{201c}I just . . . couldn\u{2019}t\u{2014}not then.\u{201d}",
        "He stopped. . . . Then he left.",
        "Wait\u{2028}. . . and then what?",
        "\u{201c}Wait\u{00a0}.\u{00a0}.\u{00a0}. what?\u{201d}",
    ] {
        assert!(
            comma_flags(text).is_empty(),
            "{:?}: {:?}",
            text,
            comma_flags(text)
        );
    }
}

#[test]
fn builtin_comma_ignores_quote_spaced_from_ellipsis() {
    for text in &[
        "\u{201c}It will work. The trouble is . . . \u{201d}",
        "\"What on earth . . . \" Mara whispered.",
        "\u{201c}I thought . . . \u{201d} She stopped\u{2014}then went on.",
        "She said \u{201c} . . . and then nothing\u{2019}s left.\u{201d}",
        "\u{2018}Maybe . . . \u{2019}\u{2028}he said.",
        "\u{201c}Yeah . . . ,\u{201d} Alan said.",
        "\"Uh . . . , James\u{2014}no, Josh.\"",
        "\"He said, 'Not now . . . ' \" she told me.",
        "\u{201c}She called it \u{2018}the plan,\u{2019} \u{201d} he said.",
        "\" 'Crossing the line.' I like that!\"",
        "What sort of \u{201c} \u{2018}trouble\u{2019} \u{201d} sends you here?",
    ] {
        assert!(
            comma_flags(text).is_empty(),
            "{:?}: {:?}",
            text,
            comma_flags(text)
        );
    }
    assert_eq!(
        comma_flags("\"Well, I fixed the fence \"\n\"Then what?\""),
        vec![(
            " \"\n".to_string(),
            vec!["\" ".to_string(), " \"".to_string()]
        )]
    );
    assert_eq!(
        comma_flags("He said \u{201c} hello \u{201d} to me . . . twice."),
        vec![
            (
                " \u{201c} ".to_string(),
                vec!["\u{201c} ".to_string(), " \u{201c}".to_string()]
            ),
            (
                " \u{201d} ".to_string(),
                vec!["\u{201d} ".to_string(), " \u{201d}".to_string()]
            )
        ]
    );
}

#[test]
fn builtin_comma_flags_full_stop_next_to_short_dot_run() {
    assert_eq!(
        comma_flags("It ended . Then . . . nothing."),
        vec![(" .".to_string(), vec![".".to_string()])]
    );
    assert_eq!(
        comma_flags("It ended . . Then it began."),
        vec![
            (" .".to_string(), vec![".".to_string()]),
            (" .".to_string(), vec![".".to_string()])
        ]
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
    // U+2028 is whitespace inside a paragraph, so the whole paragraph shares state.
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
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
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
        .flat_map(|rule| {
            rule.builtin()
                .check_text(paragraph, RULES.spell().speller().as_ref())
        })
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
fn builtin_uppercase_sentence_start_ignores_lowercase_after_u2028() {
    // As in LanguageTool 6.6, U+2028 after a period does not end the sentence.
    assert!(builtin_uppercase_sentence_start_flags("Good.\u{2028}bad start here.").is_empty());
    assert_eq!(
        builtin_uppercase_sentence_start_flags("Good.\u{2028}Fine. bad start here."),
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
fn builtin_uppercase_sentence_start_ignores_lowercase_after_spaced_ellipsis() {
    for text in &[
        "\u{201c}So . . . how was the drive?\u{201d}",
        "\u{201c}Wait . . . and then what?\u{201d}",
        "He stopped. . . . and then he left.",
        "I just . . . couldn\u{2019}t\u{2014}not then.",
    ] {
        assert!(
            builtin_uppercase_sentence_start_flags(text).is_empty(),
            "{:?}",
            text
        );
    }
    assert_eq!(
        builtin_uppercase_sentence_start_flags("He waited . . . Then she came. bad start."),
        vec![("bad".to_string(), "Bad".to_string())]
    );
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
    // Both confusions are flagged in one paragraph, across a U+2028.
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

fn rule_flags(text: &str, rule: &str) -> Vec<(String, Vec<String>)> {
    rule_flags_with(&RULES, text, rule)
}

fn rule_flags_with(rules: &Rules, text: &str, rule: &str) -> Vec<(String, Vec<String>)> {
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source().starts_with(rule))
        .map(|suggestion| {
            let span = suggestion.span().char().clone();
            (
                text.chars().skip(span.start).take(span.len()).collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

/// When subrules of one rule group match the same span, the earliest subrule wins, as in
/// LanguageTool 6.6 (issue #190): subrule 0 of `EN_SPLIT_WORDS_HYPHEN`, not subrule 3.
#[test]
fn earliest_subrule_of_a_group_wins_the_same_span() {
    let text = "\u{201c}The break- up was painful\u{2026}\u{201d} she said\u{2014}it\u{2019}s over.\u{2028}The break- up hurt.";
    let flagged = (
        "break- up".to_owned(),
        vec![
            "break-up".to_owned(),
            "break - up".to_owned(),
            "breakup".to_owned(),
        ],
    );
    assert_eq!(
        rule_flags(text, "TYPOS/EN_SPLIT_WORDS_HYPHEN/0"),
        vec![flagged.clone(), flagged]
    );
    assert!(rule_flags(text, "TYPOS/EN_SPLIT_WORDS_HYPHEN/3").is_empty());
}

#[test]
fn apostrophe_rules_match_languagetool_on_fiction_text() {
    assert_eq!(
        rule_flags(
            "\u{201c}We don'T need this,\u{201d} she said\u{2026}",
            "TYPOS/APOS_RE/"
        ),
        vec![("don'T".to_string(), vec!["don't".to_string()])]
    );
    assert_eq!(
        rule_flags(
            "\u{201c}While were\u{2019}re at it\u{2014}\u{201d} he began.",
            "TYPOS/WERE_RE/"
        ),
        vec![(
            "were\u{2019}re".to_string(),
            vec!["we\u{2019}re".to_string()]
        )]
    );
    assert_eq!(
        rule_flags(
            "Here is a copy of the singed contract\u{2026} I\u{2019}m sure.",
            "GRAMMAR/SINGED_CONTRACT/"
        ),
        vec![("singed".to_string(), vec!["signed".to_string()])]
    );
    assert!(rule_flags(
        "Profit such as Adam Smith\u{2019}s where part of the undertaker\u{2019}s profits were for risk bearing.",
        "TYPOS/APOSTROPHE_PLURAL_/"
    )
    .is_empty());
    assert!(rule_flags(
        "\u{2026} the Germans didn\u{2019}t-->  The French armies were\u{2026}",
        "GRAMMAR/MISSING_APOSTROPHE_T/"
    )
    .is_empty());
}

#[test]
fn suggestions_at_sentence_start_keep_the_matched_case() {
    // LanguageTool capitalizes a suggestion only when the matched text starts
    // uppercase; sentence start alone does not (#191). `A_INFINITIVE` suggests
    // `\1 {suggestion}`. Checked against LanguageTool 6.6.
    let flags = |text: &str, matched: &str, article: &str| {
        assert_eq!(
            rule_flags(text, "GRAMMAR/A_INFINITIVE/"),
            vec![(
                matched.to_string(),
                ["competed", "complete", "compote"]
                    .iter()
                    .map(|word| format!("{} {}", article, word))
                    .collect::<Vec<_>>()
            )],
            "{:?}",
            text
        );
    };
    flags("a compete", "a compete", "a");
    flags("A compete", "A compete", "A");
    flags(
        "\u{201c}a compete,\u{201d} she said\u{2026}",
        "a compete",
        "a",
    );
    flags(
        "\u{2018}a compete\u{2019} she said\u{2026}",
        "a compete",
        "a",
    );
    flags("Wait\u{2014}a compete?", "a compete", "a");
    flags("Fine.\u{2028}a compete it was.", "a compete", "a");
    flags("Fine.\u{2028}A compete it was.", "A compete", "A");
}

#[test]
fn skip_continues_past_antipattern_immunized_token() {
    for (text, expected) in [
        ("He said \u{201c}a,\u{201c} b c\u{201c} d.", vec![16]),
        ("He said \u{201c}a b c\u{201c} d.", vec![14]),
        (
            "He said \u{201c}don\u{2019}t,\u{201c} b c\u{201c} d.",
            vec![20],
        ),
        ("He said \u{201c}a\u{2014}\u{201c} b c\u{201c} d.", vec![16]),
        ("He said \u{201c}a\u{2026}\u{201c} b c\u{201c} d.", vec![11]),
        ("He said \u{201c}a\u{2019}\u{201c} b c\u{201c} d.", vec![16]),
        (
            "She left.\u{2028}He said \u{201c}a,\u{201c} b c\u{201c} d.",
            vec![26],
        ),
        ("He said \u{201c}a,\u{201c} b,\u{201c} d.", vec![]),
    ] {
        let starts: Vec<usize> = RULES
            .suggest(text, &*TOKENIZER)
            .into_iter()
            .filter(|suggestion| suggestion.source().starts_with("TYPOGRAPHY/GERMAN_QUOTES/"))
            .map(|suggestion| suggestion.span().char().start)
            .collect();
        assert_eq!(starts, expected, "{:?}", text);
    }
}

#[test]
fn antipattern_on_adjacent_punctuation_does_not_block_match() {
    let flagged = vec![("singed".to_string(), vec!["signed".to_string()])];
    for text in [
        "\u{201c}Here is a copy of the singed contract.\u{201d} she said.",
        "\u{2014}Here is a copy of the singed contract.",
        "\u{2026}Here is a copy of the singed contract.\u{2026}",
        "She kept the singed contract.\u{2028}He didn\u{2019}t.",
    ] {
        assert_eq!(
            rule_flags(text, "GRAMMAR/SINGED_CONTRACT/"),
            flagged,
            "{:?}",
            text
        );
    }
    for text in [
        "\u{201c}I\u{2019}m sure it\u{2019}s the singed contract\u{2014}\u{201d} she began.",
        "\u{201c}It\u{2019}s the singed contract\u{2026}\u{201d}",
    ] {
        assert!(
            rule_flags(text, "GRAMMAR/SINGED_CONTRACT/").is_empty(),
            "{:?}",
            text
        );
    }
}

#[test]
fn opening_quote_antipattern_does_not_block_repeated_verbs() {
    let flagged = vec![("made make".to_string(), vec!["make".to_string()])];
    for text in [
        "\u{201c}To made make your text better.",
        "\u{201c} To made make your text better.",
        "To made make your text better.",
        "She said, \u{201c}To made make your text better.\u{201d}",
        "\u{2018}To made make your text better.\u{2019}",
        "\u{2014}To made make your text better.",
        "\u{2026}To made make your text better.",
        "She paused.\u{2028}\u{201c}To made make your text better.\u{201d}",
        "He didn\u{2019}t answer.\u{2028}To made make your text better\u{2026}",
    ] {
        assert_eq!(
            rule_flags(text, "GRAMMAR/REPEATED_VERBS/1"),
            flagged,
            "{:?}",
            text
        );
    }
    assert_eq!(
        rule_flags(
            "\u{201c}Sally, I want wanted to congratulate you.\u{201d}",
            "GRAMMAR/REPEATED_VERBS/0"
        ),
        vec![(
            "want wanted".to_string(),
            vec!["want".to_string(), "wanted".to_string()]
        )]
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

/// Chunk tags LanguageTool 6.6 assigns to these sentences. The chunker's beam search
/// must keep low-probability tags (`right/NN`, `that/WDT` here), extend the best
/// sequence first when no top tag is valid (`her/PRP`), and never continue a chunk
/// after `O` (the closing quote).
#[test]
fn chunker_matches_languagetool_beam_search() {
    let chunk_tags = |text: &str| -> Vec<(String, String)> {
        TOKENIZER
            .pipe(text)
            .flat_map(|sentence| sentence.into_iter())
            .filter(|token| !token.word().as_str().trim().is_empty())
            .map(|token| (token.word().as_str().to_string(), token.chunks().join("|")))
            .collect()
    };
    let expected = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(word, chunks)| (word.to_string(), chunks.to_string()))
            .collect()
    };
    let phrase = [
        ("I", "B-NP-singular|E-NP-singular"),
        ("miss", "B-VP"),
        ("the", "B-NP-singular"),
        ("button", "E-NP-singular"),
        ("on", "B-PP"),
        ("the", "B-NP-singular"),
        ("bottom", "I-NP-singular"),
        ("right", "E-NP-singular"),
        ("that", "B-NP-singular|E-NP-singular"),
        ("just", "B-ADVP"),
        ("go", "B-VP"),
    ];
    let errors = [
        ("'s", "B-NP-singular|E-NP-singular"),
        ("over", "B-PP"),
        ("all", "B-NP-plural"),
        ("the", "I-NP-plural"),
        ("errors", "E-NP-plural"),
    ];

    let mut straight = expected(&phrase);
    straight.extend(expected(&errors));
    straight.push((".".to_string(), "O".to_string()));
    assert_eq!(
        chunk_tags("I miss the button on the bottom right that just go's over all the errors."),
        straight
    );

    let mut quoted = expected(&[("\u{201c}", "B-ADVP")]);
    quoted.extend(expected(&phrase));
    quoted.extend(expected(&errors));
    quoted.extend(expected(&[
        ("\u{2026}", "B-PP"),
        ("\u{201d}", "B-NP-singular|E-NP-singular"),
        ("she", "B-NP-singular|E-NP-singular"),
        ("said", "B-VP"),
        (".", "O"),
    ]));
    assert_eq!(
        chunk_tags(
            "\u{201c}I miss the button on the bottom right that just go\u{2019}s over all the errors\u{2026}\u{201d} she said.\u{2028}"
        ),
        quoted
    );

    let told = chunk_tags(
        "I did not tell her that his recipe, or a version of it, was still in the till, because some gifts are for keeping.",
    );
    assert_eq!(
        told[3..6],
        expected(&[
            ("tell", "I-VP"),
            ("her", "B-NP-singular|E-NP-singular"),
            ("that", "B-PP"),
        ])[..]
    );

    let spain =
        chunk_tags("\u{201c}Spain\u{2014}a great whale stranded on the shores of Europe.\u{201d}");
    assert_eq!(
        spain[spain.len() - 3..],
        expected(&[
            ("Europe", "B-NP-singular|E-NP-singular"),
            (".", "O"),
            ("\u{201d}", "B-NP-singular|E-NP-singular"),
        ])[..]
    );
}

/// Chunk tags LanguageTool 6.6 assigns where its top two sequences are within 0.005
/// log-probability of each other (`nginx ,` and `fast/JJ` here), and where its tokenizer
/// keeps an opening quote attached to the next word (`‘No`, `“The`). Breaking these ties
/// like OpenNLP needs the models' parameters at full `f32` precision.
#[test]
fn chunker_breaks_near_ties_like_languagetool() {
    let chunk_tags = |text: &str| -> Vec<(String, String)> {
        TOKENIZER
            .pipe(text)
            .flat_map(|sentence| sentence.into_iter())
            .filter(|token| !token.word().as_str().trim().is_empty())
            .map(|token| (token.word().as_str().to_string(), token.chunks().join("|")))
            .collect()
    };
    let expected = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(word, chunks)| (word.to_string(), chunks.to_string()))
            .collect()
    };

    assert_eq!(
        chunk_tags("I have checked nginx, php, and mysql error logs and this error isn't there."),
        expected(&[
            ("I", "B-NP-singular|E-NP-singular"),
            ("have", "B-VP"),
            ("checked", "I-VP"),
            ("nginx", "B-NP-singular|E-NP-singular"),
            (",", "O"),
            ("php", "B-NP-singular|E-NP-singular"),
            (",", "O"),
            ("and", "O"),
            ("mysql", "B-NP-plural"),
            ("error", "I-NP-plural"),
            ("logs", "E-NP-plural"),
            ("and", "O"),
            ("this", "B-NP-singular"),
            ("error", "E-NP-singular"),
            ("is", "B-VP"),
            ("n't", "O"),
            ("there", "B-ADVP"),
            (".", "O"),
        ])
    );

    assert_eq!(
        chunk_tags(
            "\u{201c}I\u{2019}ve checked nginx, php, and mysql error logs\u{2014}and this error isn\u{2019}t there\u{2026}\u{201d}\u{2028}"
        ),
        expected(&[
            ("\u{201c}", "B-NP-singular|E-NP-singular"),
            ("I", "B-NP-singular|E-NP-singular"),
            ("'ve", "B-VP"),
            ("checked", "I-VP"),
            ("nginx", "B-NP-singular|E-NP-singular"),
            (",", "O"),
            ("php", "B-NP-singular|E-NP-singular"),
            (",", "O"),
            ("and", "O"),
            ("mysql", "B-NP-singular"),
            ("error", "E-NP-singular"),
            ("logs", ""),
            ("\u{2014}", ""),
            ("and", ""),
            ("this", "B-NP-singular"),
            ("error", "E-NP-singular"),
            ("is", "B-VP"),
            ("n't", "I-VP"),
            ("there", "I-VP"),
            ("\u{2026}", "I-VP"),
            ("\u{201d}", "B-NP-singular|E-NP-singular"),
        ])
    );

    assert_eq!(
        chunk_tags("The car is special, fast and nice to look at."),
        expected(&[
            ("The", "B-NP-singular"),
            ("car", "E-NP-singular"),
            ("is", "B-VP"),
            ("special", "B-ADJP"),
            (",", "O"),
            ("fast", "B-ADJP"),
            ("and", "I-ADJP"),
            ("nice", "I-ADJP"),
            ("to", "B-VP"),
            ("look", "I-VP"),
            ("at", "B-PP"),
            (".", "O"),
        ])
    );

    assert_eq!(
        chunk_tags("\u{201c}The car is special, fast and nice to look at\u{2026}\u{201d} she said.\u{2028}"),
        expected(&[
            ("\u{201c}", ""),
            ("The", ""),
            ("car", "E-NP-singular"),
            ("is", "B-VP"),
            ("special", "B-ADJP"),
            (",", "O"),
            ("fast", "B-ADJP"),
            ("and", "I-ADJP"),
            ("nice", "I-ADJP"),
            ("to", "B-VP"),
            ("look", "I-VP"),
            ("at", ""),
            ("\u{2026}", ""),
            ("\u{201d}", "B-NP-singular|E-NP-singular"),
            ("she", "B-NP-singular|E-NP-singular"),
            ("said", "B-VP"),
            (".", "O"),
        ])
    );

    assert_eq!(
        chunk_tags(
            "\u{2018}No sooner, sir?\u{2019}\u{2014}\u{2018}Soon enough for any honest man that goes a passenger.\u{2019}"
        ),
        expected(&[
            ("\u{2018}", ""),
            ("No", ""),
            ("sooner", "I-NP-singular"),
            (",", "I-NP-singular"),
            ("sir", "E-NP-singular"),
            ("?", "O"),
            ("\u{2019}", ""),
            ("\u{2014}", ""),
            ("\u{2018}", ""),
            ("Soon", ""),
            ("enough", "B-ADVP"),
            ("for", "B-PP"),
            ("any", "B-NP-singular"),
            ("honest", "I-NP-singular"),
            ("man", "E-NP-singular"),
            ("that", "B-NP-singular|E-NP-singular"),
            ("goes", "B-VP"),
            ("a", "B-NP-singular"),
            ("passenger", "E-NP-singular"),
            (".", "O"),
            ("\u{2019}", "O"),
        ])
    );
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
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
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
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
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

/// Runs EN_PLAIN_ENGLISH_REPLACE over `text` and returns the flagged text and
/// `text` with each replacement applied on its own.
fn plain_english_fixes(text: &str) -> Vec<(String, Vec<String>)> {
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    RULES
        .builtins()
        .iter()
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
        .filter(|suggestion| suggestion.source() == "BUILTIN/EN_PLAIN_ENGLISH_REPLACE")
        .map(|suggestion| {
            let span = suggestion.span().byte().clone();
            let chars: Vec<char> = text.chars().collect();
            let by_char: String = chars[suggestion.span().char().clone()].iter().collect();
            assert_eq!(by_char, text[span.clone()], "char and byte spans agree");
            let fixes = suggestion
                .replacements()
                .iter()
                .map(|replacement| {
                    format!(
                        "{}{}{}",
                        &text[..span.start],
                        replacement,
                        &text[span.end..]
                    )
                })
                .collect();
            (text[span].to_owned(), fixes)
        })
        .collect()
}

/// `[OMIT]` in the plain-English table means "delete the phrase". LanguageTool
/// 6.6 suggests the marker literally; the builtin deletes the phrase and tidies
/// the space and punctuation around it.
#[test]
fn builtin_plain_english_omit_deletes_the_phrase() {
    let fixes = |text: &str| plain_english_fixes(text);
    let owned = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();

    // Between paired commas: both commas go.
    assert_eq!(
        fixes("He was, as a matter of fact, late."),
        vec![(
            ", as a matter of fact,".to_owned(),
            owned(&["He was, in fact, late.", "He was late."])
        )]
    );
    // Mid-sentence before a word: the space after goes.
    assert_eq!(
        fixes("The plan as a whole failed."),
        vec![("as a whole ".to_owned(), owned(&["The plan failed."]))]
    );
    // At the sentence start: the comma goes and the next word is capitalized.
    assert_eq!(
        fixes("As a whole, the plan failed."),
        vec![("As a whole, t".to_owned(), owned(&["The plan failed."]))]
    );
    // Before the sentence end: the separator before goes.
    assert_eq!(
        fixes("She left early, as a matter of fact."),
        vec![(
            ", as a matter of fact".to_owned(),
            owned(&["She left early, in fact.", "She left early."])
        )]
    );
    // The offsets hold after an earlier sentence.
    assert_eq!(
        fixes("She left. The plan as a whole failed."),
        vec![(
            "as a whole ".to_owned(),
            owned(&["She left. The plan failed."])
        )]
    );
}

#[test]
fn builtin_plain_english_omit_handles_fiction_punctuation() {
    let fixes = |text: &str| -> Vec<String> {
        plain_english_fixes(text)
            .into_iter()
            .flat_map(|(_, fixes)| fixes)
            .collect()
    };
    // Curly quotes around dialogue, and the sentence start inside them.
    assert_eq!(
        fixes("\u{201c}As a matter of fact, I didn\u{2019}t,\u{201d} she said."),
        vec![
            "\u{201c}In fact, I didn\u{2019}t,\u{201d} she said.",
            "\u{201c}I didn\u{2019}t,\u{201d} she said.",
        ]
    );
    assert_eq!(
        fixes("\u{201c}It failed as a whole,\u{201d} she said."),
        Vec::<String>::new(),
        "no clean deletion before a comma mid-sentence"
    );
    assert_eq!(
        fixes("\u{201c}It failed as a whole.\u{201d}"),
        vec!["\u{201c}It failed.\u{201d}"]
    );
    // Curly apostrophes.
    assert_eq!(
        fixes("It\u{2019}s, as a matter of fact, Marta\u{2019}s coat."),
        vec![
            "It\u{2019}s, in fact, Marta\u{2019}s coat.",
            "It\u{2019}s Marta\u{2019}s coat."
        ]
    );
    // Closed-up em dashes leave one space; spaced ones leave the outer spaces.
    assert_eq!(
        fixes("The plan\u{2014}as a whole\u{2014}failed."),
        vec!["The plan failed."]
    );
    assert_eq!(
        fixes("The plan \u{2014} as a whole \u{2014} failed."),
        vec!["The plan failed."]
    );
    assert_eq!(
        fixes("She hesitated\u{2014}as a matter of fact."),
        vec!["She hesitated\u{2014}in fact.", "She hesitated."]
    );
    // Ellipses.
    assert_eq!(
        fixes("She was late, as a matter of fact\u{2026}"),
        vec!["She was late, in fact\u{2026}", "She was late\u{2026}"]
    );
    // A deletion never joins two lines across U+2028.
    assert_eq!(
        fixes("It failed as a whole\u{2028}and nobody came."),
        vec!["It failed\u{2028}and nobody came."]
    );
    // No clean deletion: only the other replacements stay.
    assert_eq!(
        fixes("The plan, as a matter of fact\u{2014}failed."),
        vec!["The plan, in fact\u{2014}failed."]
    );
}

#[test]
fn builtin_plain_english_never_suggests_the_omit_marker() {
    let text = "As a whole, the plan failed. He was, as a matter of fact, late. \
                The plan, as a whole\u{2014}failed. After all is said and done, we left.";
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let suggestions: Vec<_> = RULES
        .builtins()
        .iter()
        .flat_map(|rule| {
            rule.builtin()
                .check_text(&sentences, RULES.spell().speller().as_ref())
        })
        .filter(|suggestion| suggestion.source() == "BUILTIN/EN_PLAIN_ENGLISH_REPLACE")
        .collect();
    assert!(suggestions.len() >= 4);
    for suggestion in &suggestions {
        assert!(!suggestion.message().contains("[OMIT]"), "{:?}", suggestion);
        assert!(suggestion.message().contains("omit it"), "{:?}", suggestion);
        assert!(
            suggestion
                .replacements()
                .iter()
                .all(|r| !r.contains("[OMIT]")),
            "{:?}",
            suggestion
        );
    }
    let as_a_whole = suggestions
        .iter()
        .find(|suggestion| suggestion.message().starts_with("'As a whole'"))
        .unwrap();
    assert_eq!(
        as_a_whole.message(),
        "'As a whole' is a wordy or complex expression. In some cases, it might be \
         preferable to omit it."
    );
    let matter_of_fact = suggestions
        .iter()
        .find(|suggestion| suggestion.message().starts_with("'as a matter of fact'"))
        .unwrap();
    assert!(matter_of_fact
        .message()
        .ends_with("preferable to use in fact or omit it."));
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

fn flags_from(text: &str, source: &str) -> Vec<(String, Vec<String>)> {
    let chars: Vec<char> = text.chars().collect();
    RULES
        .suggest(text, &*TOKENIZER)
        .iter()
        .filter(|suggestion| suggestion.source() == source)
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

#[test]
fn or_group_takes_the_first_tokens_quantifier() {
    let source = "CONFUSED_WORDS/MULTIPLE_MULTIPLY/4";
    let multiply = vec![("multiple".to_string(), vec!["multiply".to_string()])];

    assert_eq!(
        flags_from(
            "\u{201c}Please multiple by five,\u{201d} she said\u{2026}",
            source
        ),
        multiply
    );
    assert_eq!(
        flags_from("She paused\u{2014}please multiple them by two.", source),
        multiply
    );
    assert_eq!(
        flags_from("He paused.\u{2028}Please multiple them by two.", source),
        multiply
    );
    assert!(flags_from("Please multiple 10 them by 5.", source).is_empty());
}

#[test]
fn token_match_synthesizes_the_referenced_verb() {
    assert_eq!(
        flags_from(
            "\u{201c}He\u{2019}s not going go to Boston,\u{201d} she said\u{2026}",
            "GRAMMAR/REPEATED_VERBS/0"
        ),
        vec![(
            "going go".to_string(),
            vec!["going".to_string(), "go".to_string()]
        )]
    );
    assert_eq!(
        flags_from(
            "She paused\u{2014}I want wanted to thank you.",
            "GRAMMAR/REPEATED_VERBS/0"
        ),
        vec![(
            "want wanted".to_string(),
            vec!["want".to_string(), "wanted".to_string()]
        )]
    );
    assert_eq!(
        flags_from(
            "He paused.\u{2028}He\u{2019}s not going go home.",
            "GRAMMAR/REPEATED_VERBS/0"
        ),
        vec![(
            "going go".to_string(),
            vec!["going".to_string(), "go".to_string()]
        )]
    );
    assert_eq!(
        flags_from(
            "\u{201c}Well, to made make it better\u{2026}\u{201d} she began.",
            "GRAMMAR/REPEATED_VERBS/1"
        ),
        vec![("made make".to_string(), vec!["make".to_string()])]
    );
    assert_eq!(
        flags_from(
            "He paused.\u{2028}To made make it better, try this.",
            "GRAMMAR/REPEATED_VERBS/1"
        ),
        vec![("made make".to_string(), vec!["make".to_string()])]
    );

    for text in [
        "\u{201c}Take Took Taken!\u{201d} she shouted.",
        "No one I know knows how to do that\u{2014}truly.",
    ] {
        assert!(
            RULES
                .suggest(text, &*TOKENIZER)
                .iter()
                .all(|suggestion| !suggestion.source().starts_with("GRAMMAR/REPEATED_VERBS/")),
            "{}",
            text
        );
    }
}

#[test]
fn raw_pos_text_only_pattern_matches() {
    let source = "TYPOS/CONTRACTION_CASE/0";

    assert_eq!(
        flags_from(
            "\u{201c}Maybe I'Ve seen this before,\u{201d} he said\u{2026}",
            source
        ),
        vec![("I'Ve".to_string(), vec!["I've".to_string()])]
    );
    assert_eq!(
        flags_from("She paused\u{2014}I'Ll do so tomorrow.", source),
        vec![("I'Ll".to_string(), vec!["I'll".to_string()])]
    );
    assert_eq!(
        flags_from("He paused.\u{2028}I'Ve seen this before.", source),
        vec![("I'Ve".to_string(), vec!["I've".to_string()])]
    );
    assert!(flags_from(
        "\u{201c}Maybe I\u{2019}Ve seen this before,\u{201d} he said.",
        source
    )
    .is_empty());
}

#[test]
fn tokenizer_splits_mixed_case_clitics_at_the_apostrophe() {
    assert_eq!(
        token_texts("They\u{2019}Re here\u{2014}they'RE here\u{2026} I'Ve seen it."),
        vec![
            "They", "\u{2019}", "Re", "here", "\u{2014}", "they", "'RE", "here", "\u{2026}", "I",
            "'", "Ve", "seen", "it", "."
        ]
    );
}

#[test]
fn token_match_without_synthesized_forms_matches_any_token() {
    for text in [
        "\u{201c}Methinks want wanted more,\u{201d} she said\u{2026}",
        "She paused\u{2014}methinks want wanted more.",
        "He paused.\u{2028}Methinks want wanted more.",
        "\u{201c}Thus spake want wanted the prophet,\u{201d} she said\u{2026}",
    ] {
        assert!(
            RULES
                .suggest(text, &*TOKENIZER)
                .iter()
                .all(|suggestion| !suggestion.source().starts_with("GRAMMAR/REPEATED_VERBS/")),
            "{}",
            text
        );
    }

    assert_eq!(
        flags_from(
            "He paused.\u{2028}I want wanted more.",
            "GRAMMAR/REPEATED_VERBS/0"
        ),
        vec![(
            "want wanted".to_string(),
            vec!["want".to_string(), "wanted".to_string()]
        )]
    );
}

#[test]
fn long_unterminated_sentence_checks_in_near_linear_time() {
    // A sentence with no terminator used to be checked in quadratic time: every candidate match
    // re-ran the rule's antipatterns over the whole sentence, every disambiguation change scanned
    // every token, and the chunker's beam search copied each hypothesis's history at every word.
    // This ~3,600-word sentence took ~36s before the fix and ~1s after with the app's profile and
    // speller; the bound is generous so the test stays reliable on slow machines.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let _ = rules.apply_profile(&Profile::fiction());
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_US").unwrap());

    let snippet =
        "“We’re not going—not yet…” she said, and the dog’s bark\u{2028}echoed across the \
                   old man’s field while they waited ";
    let text = snippet.repeat(200);
    assert_eq!(TOKENIZER.pipe(&text).count(), 1);

    let start = std::time::Instant::now();
    let _ = rules.suggest(&text, &*TOKENIZER);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(12),
        "checking a 200-copy unterminated sentence took {:?}",
        start.elapsed()
    );
}

#[test]
fn unbounded_skip_antipatterns_stay_fast_on_a_run_of_opening_quotes() {
    let text = "\u{201c}".repeat(2000);
    let start = std::time::Instant::now();
    let _ = RULES.suggest(&text, &*TOKENIZER);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(12),
        "checking 2,000 opening quotes took {:?}",
        start.elapsed()
    );
}

#[test]
fn long_unterminated_sentence_is_tagged_the_same_at_every_repeat() {
    let snippet = "“She didn’t know…,” he said — and then…nothing, the old man’s dog ran \u{2028}to the bank and ";
    let copies = 120;
    let text = snippet.repeat(copies);
    let sentences: Vec<_> = TOKENIZER.pipe(&text).collect();
    assert_eq!(sentences.len(), 1);

    let tokens: Vec<_> = sentences[0].iter().collect();
    assert_eq!(tokens.len() % copies, 0);
    let per_copy = tokens.len() / copies;
    let copy = |i: usize| {
        tokens[i * per_copy..(i + 1) * per_copy]
            .iter()
            .map(|token| {
                (
                    token.word().text().as_str().to_string(),
                    format!("{:?}", token.word().tags()),
                    token.chunks().to_vec(),
                )
            })
            .collect::<Vec<_>>()
    };

    let expected = copy(copies / 4);
    for i in copies / 4..copies * 3 / 4 {
        assert_eq!(copy(i), expected, "copy {}", i);
    }
}

#[test]
fn very_long_unterminated_sentence_tokenizes_in_near_linear_time() {
    let snippet = "“We’re not going—not yet…” she said, and the dogs’ barks\u{2028}echoed across \
                   the old man’s fields while they waited ";
    let text = snippet.repeat(3000);

    let start = std::time::Instant::now();
    assert_eq!(TOKENIZER.pipe(&text).count(), 1);
    assert!(
        start.elapsed() < std::time::Duration::from_secs(15),
        "tokenizing a 3,000-copy unterminated sentence took {:?}",
        start.elapsed()
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

fn look_slike_matches(rules: &Rules, text: &str) -> Vec<(std::ops::Range<usize>, Vec<String>)> {
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|s| s.source().ends_with("LOOK_SLIKE/0"))
        .map(|s| (s.span().char().clone(), s.replacements().to_vec()))
        .collect()
}

#[test]
fn raw_pos_rule_matches_pre_disambiguation_tags() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let selector: Selector = "typos/look_slike".try_into().unwrap();
    rules
        .rules_mut()
        .iter_mut()
        .filter(|rule| !selector.is_match(rule.id()))
        .for_each(|rule| rule.disable());
    rules
        .builtins_mut()
        .iter_mut()
        .for_each(|rule| rule.disable());

    assert!(look_slike_matches(&rules, "He always look sfor challenges. It’s fine.").is_empty());

    rules.select_mut(&selector).for_each(|x| x.enable());

    for (text, expected) in [
        (
            "He always look sfor challenges. It’s fine.",
            vec![(10..19, "looks for")],
        ),
        (
            "Dogs always look sfor food.\u{2028}Cats don’t.",
            vec![(12..21, "looks for")],
        ),
        (
            "The dog quickly look sfor the bone…",
            vec![(16..25, "looks for")],
        ),
        ("This sound slike an error.", vec![(5..16, "sounds like")]),
        ("She’d never look sfor trouble—never.", vec![]),
        ("“This sound slike an error,” she said—quietly…", vec![]),
    ] {
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(span, replacement)| (span, vec![replacement.to_string()]))
            .collect();
        assert_eq!(look_slike_matches(&rules, text), expected, "{}", text);
    }
}

fn sent_start_num_matches(rules: &Rules, text: &str) -> Vec<(std::ops::Range<usize>, Vec<String>)> {
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|s| s.source().ends_with("SENT_START_NUM/0"))
        .map(|s| (s.span().char().clone(), s.replacements().to_vec()))
        .collect()
}

#[test]
fn spell_number_synthesis_spells_out_a_sentence_start_number() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let selector: Selector = "style/sent_start_num".try_into().unwrap();
    rules.select_mut(&selector).for_each(|x| x.enable());

    for (text, expected) in [
        ("12 soldiers were killed!", vec![(0..2, "Twelve")]),
        (
            "42 soldiers’ horses were killed!",
            vec![(0..2, "Forty-two")],
        ),
        ("7 soldiers were killed—all of them!", vec![(0..1, "Seven")]),
        (
            "99 soldiers were killed\u{2028}yesterday!",
            vec![(0..2, "Ninety-nine")],
        ),
        (
            "She waited. 21 soldiers were killed!",
            vec![(12..14, "Twenty-one")],
        ),
        ("“12 soldiers were killed!”", vec![]),
        ("12 soldiers were killed…", vec![]),
        ("05 soldiers were killed!", vec![]),
        ("12 soldiers were killed in 1999!", vec![]),
    ] {
        let expected: Vec<_> = expected
            .into_iter()
            .map(|(span, replacement)| (span, vec![replacement.to_string()]))
            .collect();
        assert_eq!(sent_start_num_matches(&rules, text), expected, "{}", text);
    }
}

#[test]
fn pipe_keeps_the_raw_sentence() {
    let sentence = TOKENIZER
        .pipe("“This sound slike an error,” she said—quietly…")
        .next()
        .unwrap();
    let raw = sentence.raw().unwrap();
    let has_vb = |sentence: &nlprule::types::Sentence| {
        sentence
            .tokens()
            .iter()
            .find(|token| token.word().text().as_str() == "sound")
            .unwrap()
            .word()
            .tags()
            .iter()
            .any(|data| data.pos().as_str() == "VB")
    };

    assert_eq!(raw.tokens().len(), sentence.tokens().len());
    assert_eq!(raw.span(), sentence.span());
    assert!(has_vb(raw));
    assert!(!has_vb(&sentence));
    assert!(raw.raw().is_none());
}

fn false_friends(rules: &Rules, text: &str) -> Vec<(String, std::ops::Range<usize>, Vec<String>)> {
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source().starts_with("FALSE_FRIENDS/"))
        .map(|suggestion| {
            (
                suggestion.source().to_string(),
                suggestion.span().char().clone(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

const FALSE_FRIEND_TEXT: &str =
    "\u{201c}My handy is broken\u{2014}again\u{2026}\u{201d}\u{2028}It\u{2019}s the actual problem.";

#[test]
fn false_friends_need_a_mother_tongue() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert_eq!(rules.mother_tongue(), None);
    assert!(false_friends(&rules, FALSE_FRIEND_TEXT).is_empty());

    rules
        .select_mut(&Category::new("false_friends").into())
        .for_each(|rule| rule.enable());
    assert!(false_friends(&rules, FALSE_FRIEND_TEXT).is_empty());
}

#[test]
fn reordered_rules_still_run_without_a_mother_tongue() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let (id, text) = rules
        .rules()
        .iter()
        .filter(|rule| rule.enabled())
        .find_map(|rule| {
            rule.examples()
                .iter()
                .find(|example| example.suggestion().is_some())
                .map(|example| (rule.id().to_string(), example.text().to_string()))
        })
        .unwrap();
    let fires = |rules: &Rules| {
        rules
            .suggest(&text, &*TOKENIZER)
            .iter()
            .any(|suggestion| suggestion.source() == id)
    };
    assert!(fires(&rules));

    let first = rules
        .rules()
        .iter()
        .position(|rule| rule.mother_tongue().is_some())
        .unwrap();
    let middle = rules.rules().len() / 2;
    rules.rules_mut().rotate_left(first);
    rules.rules_mut().rotate_right(middle);
    assert!(fires(&rules));
}

#[test]
fn mother_tongue_selects_its_false_friends() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert_eq!(
        rules.mother_tongues(),
        vec!["ca", "de", "es", "fr", "ga", "gl", "it", "ja", "nl", "pl", "pt", "ru", "sv"]
    );

    rules.set_mother_tongue(Some("de-DE"));
    assert_eq!(rules.mother_tongue(), Some("de"));
    assert_eq!(
        false_friends(&rules, FALSE_FRIEND_TEXT),
        vec![
            (
                "FALSE_FRIENDS/HANDY/0".to_string(),
                4..9,
                vec!["cell phone".to_string(), "mobile phone".to_string()]
            ),
            (
                "FALSE_FRIENDS/ACTUAL/0".to_string(),
                37..43,
                vec![
                    "current".to_string(),
                    "(the) latest".to_string(),
                    "up-to-date".to_string()
                ]
            ),
        ]
    );

    rules.set_mother_tongue(Some("PL"));
    assert_eq!(
        false_friends(&rules, FALSE_FRIEND_TEXT),
        vec![(
            "FALSE_FRIENDS/ACTUAL/0".to_string(),
            37..43,
            vec![
                "current".to_string(),
                "(the) latest".to_string(),
                "up-to-date".to_string()
            ]
        )]
    );

    rules.set_mother_tongue(Some("en"));
    assert!(false_friends(&rules, FALSE_FRIEND_TEXT).is_empty());
    rules.set_mother_tongue(None);
    assert!(false_friends(&rules, FALSE_FRIEND_TEXT).is_empty());
}

#[test]
fn capitalized_suggestions_uppercase_their_first_letter() {
    // LanguageTool's `StringTools.uppercaseFirstChar` skips leading punctuation, so
    // a capitalized "Actual" gets "(The) latest", not "(the) latest" (#158).
    // Checked against LanguageTool 6.6 with `-m pl --level PICKY`.
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.set_mother_tongue(Some("pl"));
    let replacements = |text: &str| -> Vec<Vec<String>> {
        false_friends(&rules, text)
            .into_iter()
            .map(|(_, _, replacements)| replacements)
            .collect()
    };
    let capitalized = vec![vec![
        "Current".to_string(),
        "(The) latest".to_string(),
        "Up-to-date".to_string(),
    ]];
    let lowercase = vec![vec![
        "current".to_string(),
        "(the) latest".to_string(),
        "up-to-date".to_string(),
    ]];
    assert_eq!(replacements("Actual is what they said."), capitalized);
    assert_eq!(
        replacements("\u{201c}Actual,\u{201d} she said\u{2014}again."),
        capitalized
    );
    assert_eq!(
        replacements("Fine.\u{2028}Actual numbers matter."),
        capitalized
    );
    assert_eq!(
        replacements("It\u{2019}s the actual problem\u{2026}"),
        lowercase
    );
    // Sentence start alone does not capitalize a suggestion.
    assert_eq!(replacements("actual numbers matter."), lowercase);
    assert_eq!(
        replacements("Camera is what they said."),
        vec![vec!["(Motion-picture) camera".to_string()]]
    );
}

#[test]
fn false_friend_messages_are_localized() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.set_mother_tongue(Some("de"));
    let suggestions = rules.suggest("We will become friends.", &*TOKENIZER);
    let suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "FALSE_FRIENDS/BECOME/0")
        .expect("a false-friend match for \"become\"");
    assert_eq!(*suggestion.span().char(), 8..14);
    assert_eq!(suggestion.replacements(), ["get", "receive"]);
    let message = suggestion.message();
    assert!(message.contains("\"become\"") && message.contains("\"werden\""));
    assert!(message.contains("get, receive") && !message.contains(" means "));

    rules.set_mother_tongue(Some("pt"));
    let suggestions = rules.suggest("It\u{2019}s the actual problem.", &*TOKENIZER);
    let suggestion = suggestions
        .iter()
        .find(|suggestion| suggestion.source() == "FALSE_FRIENDS/ACTUAL/0")
        .expect("a false-friend match for \"actual\"");
    assert!(suggestion.message().contains(" means "));
}

#[test]
fn default_off_false_friends_stay_off() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.set_mother_tongue(Some("de"));
    assert!(false_friends(&rules, "Did he also work there?").is_empty());

    rules
        .select_mut(&"false_friends/also".try_into().unwrap())
        .for_each(|rule| rule.enable());
    assert_eq!(
        false_friends(&rules, "Did he also work there?"),
        vec![(
            "FALSE_FRIENDS/ALSO/0".to_string(),
            7..11,
            vec![
                "thus".to_string(),
                "therefore".to_string(),
                "so".to_string()
            ]
        )]
    );
}

/// A `scope="next"` exception on a token whose predecessor has no `skip` sees only the next
/// token's first reading, as in LanguageTool 6.6. Expected flags checked against LanguageTool
/// 6.6 (-l en-US).
#[test]
fn scope_next_exception_checks_only_the_first_reading() {
    let gets = |text: &str| vec![(text.to_string(), vec!["gets".to_string()])];
    for (text, flag) in [
        (
            "\u{201c}This vehicle get\u{2019}s used too much.\u{201d}",
            "get\u{2019}s",
        ),
        ("This vehicle get's used\u{2014}too much.", "get's"),
        ("The air get\u{2019}s colder\u{2026}", "get\u{2019}s"),
        (
            "She sighed.\u{2028}This vehicle get\u{2019}s used too much.",
            "get\u{2019}s",
        ),
        (
            "\u{2018}This vehicle get\u{2019}s broken again.\u{2019}",
            "get\u{2019}s",
        ),
        ("This vehicle get\u{2019}s driven too much.", "get\u{2019}s"),
    ] {
        assert_eq!(
            rule_flags(text, "TYPOS/VERB_APOSTROPHE_S/3"),
            gets(flag),
            "{:?}",
            text
        );
    }
    for text in [
        "This vehicle get\u{2019}s are cold.",
        "\u{2018}This vehicle get\u{2019}s fallen off.\u{2019}",
        "This vehicle get\u{2019}s washed every day.",
    ] {
        assert!(
            rule_flags(text, "TYPOS/VERB_APOSTROPHE_S/3").is_empty(),
            "{:?}",
            text
        );
    }
}

/// An untagged next token's first reading is `UNKNOWN`, not the empty reading every token
/// carries. Expected flags checked against LanguageTool 6.6 (-l en-US).
#[test]
fn scope_next_exception_sees_unknown_as_the_first_reading() {
    for text in [
        "You can get the third-party Homeseer kit.",
        "\u{201c}You can get the third-party Zorblax kit\u{2026}\u{201d}",
        "The third-party left.\u{2028}The third-party Homeseer kit\u{2026}",
    ] {
        assert!(
            rule_flags(text, "GRAMMAR/THE_THIRD_PARTY/").is_empty(),
            "{:?}",
            text
        );
    }
    assert_eq!(
        rule_flags(
            "\u{201c}The third-party wasn\u{2019}t informed\u{2014}again.\u{201d}",
            "GRAMMAR/THE_THIRD_PARTY/"
        ),
        vec![("third-party".to_string(), vec!["third party".to_string()])]
    );
}

/// A `scope="next"` exception on a token whose predecessor has a `skip` is not checked, as in
/// LanguageTool 6.6, so `TYPOGRAPHY/GERMAN_QUOTES/3` flags a closing quote even when a
/// lowercase word follows it without a space. Expected flags checked against LanguageTool 6.6
/// (-l en-US).
#[test]
fn scope_next_exception_after_a_skip_is_not_checked() {
    for text in [
        "\u{201e}Wait\u{2026}\u{201c}and then she ran.",
        "\u{201e}Don\u{2019}t\u{2026}\u{201c}and then she ran.",
        "\u{201e}Wait\u{2026}\u{201c}And then she ran.",
        "She sighed.\u{2028}\u{201e}Wait\u{2026}\u{201c}and then she ran.",
    ] {
        assert_eq!(
            rule_flags(text, "TYPOGRAPHY/GERMAN_QUOTES/3"),
            vec![("\u{201c}".to_string(), vec!["\u{201d}".to_string()])],
            "{:?}",
            text
        );
    }
    assert!(rule_flags(
        "\u{201e}Wait\u{2014}\u{201c}and then she ran.",
        "TYPOGRAPHY/GERMAN_QUOTES/3"
    )
    .is_empty());
}

#[test]
fn comma_suggestion_starts_the_match_at_the_previous_token() {
    assert_eq!(
        rule_flags(
            "Tim make his life worse\u{2014}she\u{2019}d said so\u{2026} \u{201c}Again.\u{201d}\u{2028}",
            "GRAMMAR/NNP_AGREEMENT/"
        ),
        vec![(
            " make".to_string(),
            vec![" makes".to_string(), " made".into(), ", make".into()]
        )]
    );
    assert_eq!(
        rule_flags(
            "\u{201c}There wasn\u{2019}t nobody there,\u{201d} he said\u{2014}twice\u{2026}",
            "GRAMMAR/DOUBLE_NEGATIVE/"
        ),
        vec![(
            " nobody".to_string(),
            vec![" anybody".to_string(), ", nobody".into()]
        )]
    );
    assert_eq!(
        rule_flags("Tim\u{2028}make his life worse.", "GRAMMAR/NNP_AGREEMENT/"),
        vec![(
            "\u{2028}make".to_string(),
            vec![" makes".to_string(), " made".into(), ", make".into()]
        )]
    );
}

fn word_tags_of(text: &str, word: &str) -> Vec<(String, String)> {
    let mut tags: Vec<_> = tags_of(text, word)
        .into_iter()
        .filter(|(_, pos)| !matches!(pos.as_str(), "" | "UNKNOWN" | "SENT_END"))
        .collect();
    tags.sort();
    tags.dedup();
    tags
}

#[test]
fn all_uppercase_multiword_names_get_the_phrase_reading() {
    for (text, first, rest, lemma) in [
        ("I met O'BRIEN today.", "O", "BRIEN", "O'Brien"),
        (
            "“I met O’BRIEN — again…”\u{2028}He left.",
            "O",
            "BRIEN",
            "O’Brien",
        ),
        ("‘L’ORÉAL,’ she said…", "L", "ORÉAL", "L’Oréal"),
        ("She used L'ORÉAL—twice.", "L", "ORÉAL", "L'Oréal"),
    ] {
        let expected = vec![(lemma.to_string(), "NNP".to_string())];
        assert_eq!(word_tags_of(text, first), expected, "{text}");
        assert_eq!(word_tags_of(text, rest), expected, "{text}");
    }
}

#[test]
fn single_token_multiword_names_keep_their_tagger_readings() {
    let tags = word_tags_of("“I met BAHÁ’Í today…”\u{2028}", "BAHÁ'Í");
    assert!(
        tags.contains(&("Bahá'í".to_string(), "NNP".to_string())),
        "{tags:?}"
    );
    assert!(
        tags.contains(&("Bahá'í".to_string(), "JJ".to_string())),
        "{tags:?}"
    );

    let tags = word_tags_of("I met O'Brien today — again…", "O'Brien");
    assert_eq!(tags, vec![("O'Brien".to_string(), "NNP".to_string())]);
}

#[test]
fn dm_d_is_one_token() {
    assert!(token_texts("He DM'D me — twice…").contains(&"DM'D".to_string()));
    assert!(token_texts("“She dm’d me.”\u{2028}").contains(&"dm'd".to_string()));
}

#[test]
fn multiword_entries_with_stray_whitespace_do_not_match() {
    let tags = word_tags_of("“My Advisor said no — twice…”\u{2028}", "Advisor");
    assert!(!tags.iter().any(|(_, pos)| pos == "NNP"), "{tags:?}");

    let text = "“I met James Garfield — again…”\u{2028}He left.";
    assert_eq!(
        word_tags_of(text, "James"),
        vec![("James".to_string(), "NNP".to_string())]
    );
    assert_eq!(
        word_tags_of(text, "Garfield"),
        vec![("Garfield".to_string(), "NNP".to_string())]
    );
}

fn strings(xs: &[&str]) -> Vec<String> {
    xs.iter().map(|x| x.to_string()).collect()
}

#[test]
fn number_in_word_filter_matches_languagetool_on_fiction_text() {
    let rule = "TYPOS/NUMBERS_IN_WORDS/";
    assert_eq!(
        rule_flags("Go0d morning!", rule),
        vec![("Go0d".into(), strings(&["Good", "God"]))]
    );
    assert_eq!(
        rule_flags("\u{201c}The w0rld\u{2014}it ended.\u{201d}", rule),
        vec![("w0rld".into(), strings(&["world"]))]
    );
    assert_eq!(
        rule_flags("He was a g00d man.\u{2028}Then he was g0ne\u{2026}", rule),
        vec![("g0ne".into(), strings(&["gone"]))]
    );
    let flagged = rule_flags("\u{201c}H3llo,\u{201d} she said\u{2026}", rule);
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].1.first().map(String::as_str), Some("Hello"));
}

#[test]
fn suppress_misspelled_filter_matches_languagetool_on_fiction_text() {
    let rule = "TYPOS/EN_SPLIT_WORDS_HYPHEN/";
    assert_eq!(
        rule_flags("The pia- nist played\u{2014}badly\u{2026}", rule),
        vec![("pia- nist".into(), strings(&["pianist"]))]
    );
    assert_eq!(
        rule_flags(
            "\u{2018}The seco- ndary school\u{2019}s gate.\u{2019}",
            rule
        ),
        vec![("seco- ndary".into(), strings(&["secondary"]))]
    );
    assert!(rule_flags("Her ex- husband was there\u{2026}\u{2028}He smiled.", rule).is_empty());
}

#[test]
fn find_suggestions_filter_matches_languagetool_on_fiction_text() {
    assert_eq!(
        rule_flags(
            "\u{201c}I can trait,\u{201d} she said\u{2014}quietly.",
            "GRAMMAR/PRP_MD_NN/"
        ),
        vec![("trait".into(), strings(&["trail", "train"]))]
    );
    assert_eq!(
        rule_flags("I can cheeseburger\u{2026}", "GRAMMAR/PRP_MD_NN/"),
        vec![("cheeseburger".into(), vec![])]
    );
    assert_eq!(
        rule_flags(
            "It was a compete disaster\u{2014}truly.",
            "GRAMMAR/A_INFINITIVE/"
        ),
        vec![(
            "a compete".into(),
            strings(&["a competed", "a complete", "a compote"])
        )]
    );
    assert_eq!(
        rule_flags(
            "\u{201c}Worth a send,\u{201d} she said\u{2026}",
            "GRAMMAR/A_INFINITIVE/"
        ),
        vec![(
            "a send".into(),
            strings(&["a Sand", "a Seed", "a sand", "a seed", "a sent"])
        )]
    );
}

#[test]
fn speller_backed_filters_consult_the_spellers_languagetool_uses() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.spell_mut().options_mut().variant = Some(rules.spell().variant("en_GB").unwrap());
    assert_eq!(
        rule_flags_with(
            &rules,
            "\u{201c}The c0lor faded\u{2014}slowly.\u{201d} \u{2018}Her h0nour\u{2026}\u{2019} he said.\u{2028}The flav0ur was odd.",
            "TYPOS/NUMBERS_IN_WORDS/"
        ),
        vec![
            ("c0lor".into(), strings(&["color"])),
            ("h0nour".into(), strings(&["hour"])),
            ("flav0ur".into(), strings(&["flavor"])),
        ]
    );
    assert_eq!(
        rule_flags_with(
            &rules,
            "\u{201c}I can trait,\u{201d} she said\u{2014}quietly.",
            "GRAMMAR/PRP_MD_NN/"
        ),
        vec![("trait".into(), strings(&["trail", "train"]))]
    );
    assert_eq!(
        rule_flags_with(
            &rules,
            "\u{201c}Worth a send,\u{201d} she said\u{2026}",
            "GRAMMAR/A_INFINITIVE/"
        ),
        vec![(
            "a send".into(),
            strings(&["a Sand", "a Seed", "a sand", "a seed", "a sent"])
        )]
    );
}

#[test]
fn tokenizer_keeps_g_dropped_words_whole() {
    assert_eq!(
        token_texts("\u{201c}How you doin\u{2019}?\u{201d} he asked\u{2026}\u{2028}\u{201c}Nothin\u{2019}\u{2014}just somethin' I was goin' on about.\u{201d}"),
        vec![
            "\u{201c}", "How", "you", "doin'", "?", "\u{201d}", "he", "asked", "\u{2026}",
            "\u{201c}", "Nothin'", "\u{2014}", "just", "somethin'", "I", "was", "goin'", "on",
            "about", ".", "\u{201d}",
        ]
    );
    assert_eq!(
        token_texts("DOIN' IT NOW\u{2014}comin'\u{2026}"),
        vec!["DOIN'", "IT", "NOW", "\u{2014}", "comin'", "\u{2026}"]
    );
}

#[test]
fn g_dropped_words_are_tagged_as_their_ing_form() {
    let pos_tags = |text: &str, word: &str| -> Vec<(String, String)> {
        tags_of(text, word)
            .into_iter()
            .filter(|(_, pos)| !pos.is_empty())
            .collect()
    };
    let doing = vec![
        ("doing".to_string(), "NN:UN".to_string()),
        ("do".to_string(), "VBG".to_string()),
    ];
    assert_eq!(pos_tags("how you doin'?", "doin'"), doing);
    assert_eq!(
        pos_tags("\u{201c}How you doin\u{2019}?\u{201d}", "doin'"),
        doing
    );
    assert_eq!(
        pos_tags("\u{2018}Doin\u{2019} fine\u{2014}\u{2019}", "Doin'"),
        doing
    );
    assert_eq!(pos_tags("DOIN' IT NOW\u{2026}", "DOIN'"), doing);
    assert!(pos_tags("He was goin' home.\u{2028}", "goin'")
        .contains(&("go".to_string(), "VBG".to_string())));
}

#[test]
fn closing_quote_after_a_word_ending_in_in_stays_a_token() {
    assert_eq!(
        token_texts("\u{2018}A cabin\u{2019} \u{2014} \u{2018}a sin\u{2019}\u{2026} \u{2018}the robin\u{2019} \u{2028}'so thin' 'a win'"),
        vec![
            "\u{2018}", "A", "cabin", "\u{2019}", "\u{2014}", "\u{2018}", "a", "sin", "\u{2019}",
            "\u{2026}", "\u{2018}", "the", "robin", "\u{2019}", "'", "so", "thin", "'", "'", "a",
            "win", "'",
        ]
    );
    assert_eq!(
        token_texts("\u{2018}goin\u{2019} home,\u{2019} she said."),
        vec!["\u{2018}", "goin'", "home", ",", "\u{2019}", "she", "said", "."]
    );
    for text in [
        "\u{2018}It was a sin\u{2019} \u{2014} he said\u{2026}",
        "\u{2018}The robin\u{2019} sang.\u{2028}",
        "'It was a sin' he said.",
        "\u{2018}Doin\u{2019} fine,\u{2019} she said.",
        "\u{2018}goin\u{2019} home,\u{2019} she said.",
    ] {
        assert_eq!(builtin_unpaired_flags(text), vec![], "{}", text);
    }
}

#[test]
fn g_dropped_word_before_a_closing_quote_is_not_a_quote() {
    for text in [
        "'I ain't doin'.'",
        "\u{2018}I ain\u{2019}t doin\u{2019}.\u{2019}",
        "\u{2018}I\u{2019}m doin\u{2019} fine. You keep goin\u{2019}.\u{2019}\u{2028}",
        "She said, \u{2018}He\u{2019}s just talkin\u{2019}\u{2026}\u{2019}",
        "\u{201c}She said \u{2018}I ain\u{2019}t doin\u{2019} it\u{2019} and left.\u{201d}",
        "\"She said 'I ain't doin' it' and left.\"",
        "\u{201c}\u{2018}Nothin\u{2019},\u{2019} she said.\u{201d}",
        "'Keep goin',' he said.",
        "\u{2018}Lookin\u{2019} good,\u{2019} said Tom\u{2014}\u{2018}real good.\u{2019}",
        "\u{2018}Rock \u{2019}n\u{2019} roll,\u{2019} he said.",
        "'Rock 'n' roll,' he said.",
    ] {
        assert_eq!(builtin_unpaired_flags(text), vec![], "{}", text);
    }
    assert_eq!(
        builtin_unpaired_flags("\u{2018}I said goin\u{2019}\u{2019} and left."),
        vec![quote_flag("\u{2019}")]
    );
}

#[test]
fn g_dropped_word_after_an_opening_straight_quote_stays_whole() {
    assert_eq!(
        token_texts("'goin' home,' she said."),
        vec!["'", "goin'", "home", ",", "'", "she", "said", "."]
    );
    assert_eq!(
        token_texts("'Nothin',' she said.\u{2028}'Comin' through!'"),
        vec!["'", "Nothin'", ",", "'", "she", "said", ".", "'", "Comin'", "through", "!", "'",]
    );
    assert_eq!(
        token_texts("\u{201c}'Goin\u{2019} home,' she said\u{2014}'DOIN' IT\u{2026}'\u{201d}"),
        vec![
            "\u{201c}", "'", "Goin'", "home", ",", "'", "she", "said", "\u{2014}", "'", "DOIN'",
            "IT", "\u{2026}", "'", "\u{201d}",
        ]
    );
    assert_eq!(
        token_texts("\"'Nothin'.' He left.\""),
        vec!["\"", "'", "Nothin'", ".", "'", "He", "left", ".", "\""]
    );
    assert_eq!(
        token_texts("'hello' \u{2014} 'a sin' \u{2026} 'Robin' 'cabin'\u{2028}'sin'"),
        vec![
            "'", "hello", "'", "\u{2014}", "'", "a", "sin", "'", "\u{2026}", "'", "Robin", "'",
            "'", "cabin", "'", "'", "sin", "'",
        ]
    );
    assert_eq!(
        token_texts("'Rock 'n' roll,' he said. \u{2018}Comin\u{2019}?\u{2019}"),
        vec![
            "'", "Rock", "'n'", "roll", ",", "'", "he", "said", ".", "\u{2018}", "Comin'", "?",
            "\u{2019}",
        ]
    );
    assert_eq!(
        token_texts("'Tis nothin', 'tis.\u{2028}I'll get 'em. 'Cause \u{2019}Twas cold."),
        vec![
            "'", "T", "is", "nothin'", ",", "'", "t", "is", ".", "I", "'ll", "get", "'", "em", ".",
            "'", "Cause", "\u{2019}", "T", "was", "cold", ".",
        ]
    );
    // With no later apostrophe to close the quote, the word's own closes it.
    assert_eq!(
        token_texts("She said 'goin'."),
        vec!["She", "said", "'", "goin", "'", "."]
    );
}

#[test]
fn quoted_name_ending_in_in_is_not_a_g_dropped_word() {
    assert_eq!(
        token_texts("His name was 'Corin'. Call him 'Rin'."),
        vec!["His", "name", "was", "'", "Corin", "'", ".", "Call", "him", "'", "Rin", "'", ".",]
    );
    assert_eq!(
        token_texts(
            "\u{201c}His name was 'Corin'\u{2014}or \u{2018}Rin\u{2019}\u{2026}\u{201d}\u{2028}"
        ),
        vec![
            "\u{201c}", "His", "name", "was", "'", "Corin", "'", "\u{2014}", "or", "\u{2018}",
            "Rin", "\u{2019}", "\u{2026}", "\u{201d}",
        ]
    );
    assert_eq!(
        token_texts("'Corin' was goin' home. 'Corin' and 'Darin' came."),
        vec![
            "'", "Corin", "'", "was", "goin'", "home", ".", "'", "Corin", "'", "and", "'", "Darin",
            "'", "came", ".",
        ]
    );
    assert_eq!(
        token_texts("I met Rin' and Strin' there."),
        vec!["I", "met", "Rin", "'", "and", "Strin", "'", "there", "."]
    );
    // A later quotation or a plural possessive does not close the first quote.
    for text in [
        "His name was 'Corin' and hers was 'Mary'.",
        "'Corin' said 'hi'.",
        "Call him 'Corin', not 'Rob'.",
        "They said 'Corin' to the boys' mother.",
        "'Corin'\u{2014}'Rin'\u{2026}",
    ] {
        let tokens = token_texts(text);
        assert!(
            tokens.iter().any(|t| t == "Corin") && !tokens.iter().any(|t| t == "Corin'"),
            "{text}: {tokens:?}"
        );
    }
    assert_eq!(
        spelling_errors("His name was 'Corin' and hers was 'Mary'."),
        vec!["Corin"]
    );
    assert!(tags_of("His name was 'Corin'.", "Corin")
        .iter()
        .all(|(lemma, pos)| lemma == "Corin" && (pos.is_empty() || pos == "UNKNOWN")));
    assert!(tags_of("Call him \u{2018}Rin\u{2019}.", "Rin")
        .iter()
        .all(|(lemma, pos)| lemma == "Rin" && (pos.is_empty() || pos == "UNKNOWN")));
    assert_eq!(
        spelling_errors("His name was 'Corin'. Call him \u{2018}Rin\u{2019}."),
        vec!["Corin", "Rin"]
    );
}

#[test]
fn g_dropped_words_with_a_vowel_before_in_are_still_tagged() {
    let has_lemma = |text: &str, word: &str, lemma: &str| {
        tags_of(text, word)
            .iter()
            .any(|(l, pos)| l == lemma && !pos.is_empty())
    };
    assert!(has_lemma("'Mornin',' she said.", "Mornin'", "morning"));
    assert!(has_lemma(
        "\u{2018}Evenin\u{2019}\u{2014}\u{2019}",
        "Evenin'",
        "evening"
    ));
    assert!(has_lemma(
        "Somethin' or nothin'\u{2026}\u{2028}",
        "nothin'",
        "nothing"
    ));
    assert!(has_lemma("Thanks, darlin'.", "darlin'", "darling"));
    assert!(has_lemma("'Doin' fine,' he said.", "Doin'", "do"));
    assert!(has_lemma("He was flyin' high.", "flyin'", "fly"));
    // An aphetic word inside the quotation does not open a new one.
    assert!(has_lemma("'Comin' for 'em,' he said.", "Comin'", "come"));
    assert!(has_lemma("'Doin' it 'cause I can.'", "Doin'", "do"));
    // A closing quote is found past U+2028 and a no-break space.
    assert!(has_lemma(
        "'Mornin'\u{2028}to you,' she said.",
        "Mornin'",
        "morning"
    ));
    assert!(has_lemma("'Lyin'\u{a0}again!'", "Lyin'", "lie"));
}

#[test]
fn g_dropped_word_after_an_opening_straight_quote_is_tagged() {
    let has_tag = |text: &str, word: &str, lemma: &str, pos: &str| {
        tags_of(text, word).contains(&(lemma.to_string(), pos.to_string()))
    };
    assert!(has_tag("'goin' home,' she said.", "goin'", "go", "VBG"));
    assert!(has_tag(
        "\u{201c}'Comin' through!'\u{201d}",
        "Comin'",
        "come",
        "VBG"
    ));
    assert!(has_tag(
        "'Doin\u{2019} fine\u{2014}'\u{2028}",
        "Doin'",
        "do",
        "VBG"
    ));
    assert!(has_tag("'DOIN' IT\u{2026}'", "DOIN'", "do", "VBG"));
    assert!(tags_of("'Nothin',' she said.", "Nothin'")
        .iter()
        .any(|(lemma, pos)| lemma == "nothing" && !pos.is_empty()));
}

#[test]
fn g_dropped_word_after_an_opening_straight_quote_has_no_flags() {
    for text in [
        "'goin' home,' she said.",
        "'Nothin',' she said.",
        "'Comin' through!'",
        "\u{201c}'Goin\u{2019} home,' she said\u{2014}'doin' it\u{2026}'\u{201d}\u{2028}",
        "\"'Nothin'.' He left.\"",
        "She looked up. 'Somethin' wrong?'",
        "'Hello,' she said. 'Comin'?'",
    ] {
        assert_eq!(builtin_unpaired_flags(text), vec![], "{}", text);
        assert_eq!(spelling_errors(text), Vec::<&str>::new(), "{}", text);
    }
}

#[test]
fn spellchecker_accepts_g_dropped_words() {
    for text in &[
        "\u{201c}How you doin\u{2019}?\u{201d} he asked\u{2026}",
        "\u{201c}Nothin\u{2019}\u{2014}just somethin' I was goin' on about.\u{201d}\u{2028}",
    ] {
        assert_eq!(spelling_errors(text), Vec::<&str>::new(), "{}", text);
    }
}

lazy_static! {
    static ref PICKY_RULES: Rules = {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        for rule in rules.builtins_mut() {
            if matches!(rule.id(), "TOO_LONG_SENTENCE" | "TOO_LONG_PARAGRAPH") {
                rule.enable();
            }
        }
        rules
    };
}

/// Checks `text` with `rules` and returns the flagged text and replacements of
/// every suggestion from the builtin `id`.
fn suggest_flags(rules: &Rules, id: &str, text: &str) -> Vec<(String, Vec<String>)> {
    let chars: Vec<char> = text.chars().collect();
    let source = format!("BUILTIN/{}", id);
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source() == source)
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

/// Runs the builtin rule `id` over `text` paragraph by paragraph, as
/// [`Rules::suggest`] does, and returns the flagged text and all replacements
/// of every suggestion.
fn builtin_flags(id: &str, text: &str) -> Vec<(String, Vec<String>)> {
    let rule = RULES
        .builtins()
        .iter()
        .find(|rule| rule.id() == id)
        .unwrap();
    let chars: Vec<char> = text.chars().collect();

    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let speller = RULES.spell().speller();
    let mut suggestions = Vec::new();
    let mut start = 0;
    for (i, sentence) in sentences.iter().enumerate() {
        let ends_text = i + 1 == sentences.len();
        if sentence.text().contains('\n') || ends_text {
            suggestions.extend(rule.builtin().check_paragraph(
                &sentences[start..=i],
                &sentences[start.saturating_sub(2)..start],
                ends_text,
                speller.as_ref(),
            ));
            start = i + 1;
        }
    }

    suggestions
        .into_iter()
        .map(|suggestion| {
            (
                chars[suggestion.span().char().clone()].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

fn flag(text: &str, replacements: &[&str]) -> (String, Vec<String>) {
    (
        text.to_string(),
        replacements.iter().map(|x| x.to_string()).collect(),
    )
}

#[test]
fn builtin_double_punctuation_flags_two_dots_and_commas() {
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "He left.. Then he came back."),
        vec![flag("..", &[".", "\u{2026}"])]
    );
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "One ,, two ,,, three."),
        vec![flag(",,", &[","])]
    );
}

#[test]
fn builtin_double_punctuation_ignores_ellipses() {
    for text in [
        "Wait... what? Wait\u{2026}. what?",
        "Really?.. Yes. Stop!.. Now. And\u{2026}. then",
        "\u{201c}Wait\u{2026}\u{201d} she said\u{2026}. \u{201c}Now?!\u{201d}",
        "Path ../foo and ..\\bar here.",
        "\u{201c}Go!\u{201d} \u{201c}Why?\u{201d} \u{2018}Fine,\u{2019} he said\u{2014}softly.",
    ] {
        assert_eq!(builtin_flags("DOUBLE_PUNCTUATION", text), vec![], "{text}");
    }
}

#[test]
fn builtin_double_punctuation_flags_fiction_punctuation() {
    let dots = || vec![flag("..", &[".", "\u{2026}"])];
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "\u{201c}Wait..\u{201d} she said."),
        dots()
    );
    assert_eq!(
        builtin_flags(
            "DOUBLE_PUNCTUATION",
            "\u{201c}What?\u{201d} he asked.. Then."
        ),
        dots()
    );
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "She paused\u{2014}.. then went on."),
        dots()
    );
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "It\u{2019}s fine,, isn\u{2019}t it?"),
        vec![flag(",,", &[","])]
    );
}

#[test]
fn builtin_multiple_whitespace_flags_repeated_spaces() {
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Two  spaces here."),
        vec![flag("  ", &[" "])]
    );
    assert_eq!(
        builtin_flags(
            "WHITESPACE_RULE",
            "Tab\there and tab \t there and \t  mixed."
        ),
        vec![flag("\t ", &["\t"]), flag("\t  ", &["\t"])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Trailing spaces.   Then more."),
        vec![flag("  ", &[" "])]
    );
}

#[test]
fn builtin_multiple_whitespace_handles_unicode_spaces() {
    assert_eq!(
        builtin_flags(
            "WHITESPACE_RULE",
            "NBSP\u{a0}\u{a0}twice and nbsp \u{a0}mixed."
        ),
        vec![flag("\u{a0}\u{a0}", &["\u{a0}"]), flag(" \u{a0}", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Line\u{2028}sep  here.\u{2028} Then."),
        vec![flag("  ", &[" "]), flag("\u{2028} ", &["\u{2028}"])]
    );
    assert_eq!(
        builtin_flags(
            "WHITESPACE_RULE",
            "Em\u{2003}\u{2003}spaces and thin\u{2009} \u{2009}ones."
        ),
        vec![
            flag("\u{2003}\u{2003}", &["\u{2003}"]),
            flag("\u{2009} \u{2009}", &["\u{2009}"])
        ]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Zero\u{200b} width and \u{200b}  space."),
        vec![flag("  ", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Unit\u{1f}\u{1f}separators here."),
        vec![flag("\u{1f}\u{1f}", &["\u{1f}"])]
    );
    assert_eq!(
        builtin_flags(
            "WHITESPACE_RULE",
            "A\u{2007}\u{2007}figure and A \u{202f} narrow."
        ),
        vec![flag(" \u{202f} ", &[" "])]
    );
}

#[test]
fn builtin_multiple_whitespace_in_fiction_prose() {
    assert_eq!(
        builtin_flags(
            "WHITESPACE_RULE",
            "\u{201c}Hi,\u{201d}  she said \u{2014}  softly\u{2026}  then left."
        ),
        vec![flag("  ", &[" "]), flag("  ", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Sentence one.  Sentence two."),
        vec![]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Wait\u{2026} what\u{2026} ok."),
        vec![]
    );
}

#[test]
fn builtin_sentence_whitespace_flags_missing_space() {
    assert_eq!(
        builtin_flags("SENTENCE_WHITESPACE", "Hello world.This is it."),
        vec![flag("This", &[" This"])]
    );
    assert_eq!(
        builtin_flags("SENTENCE_WHITESPACE", "Hello!How are you?Fine."),
        vec![flag("How", &[" How"]), flag("Fine", &[" Fine"])]
    );
    assert_eq!(
        builtin_flags(
            "SENTENCE_WHITESPACE",
            "He came.Then he left. It was 5.Then 6."
        ),
        vec![flag("Then", &[" Then"]), flag("Then", &[" Then"])]
    );
}

#[test]
fn builtin_sentence_whitespace_ignores_dialogue_punctuation() {
    for text in [
        "\u{201c}Wait!\u{201d}she said.",
        "Done.\u{201c}Next,\u{201d} he said.",
        "\u{201c}No!\u{201d} she said.\u{201c}Yes.\u{201d}",
        "A sentence ends here.\u{2028}Next one.",
        "One.\u{a0}Two.",
        "Sentence one.  Sentence two.\n\nSentence three\u{2014}and four\u{2026}",
    ] {
        assert_eq!(builtin_flags("SENTENCE_WHITESPACE", text), vec![], "{text}");
    }
}

#[test]
fn builtin_empty_line_flags_extra_empty_lines() {
    assert_eq!(
        builtin_flags("EMPTY_LINE", "Para one.\n\n\n\nPara two."),
        vec![flag(".", &[])]
    );
    assert_eq!(
        builtin_flags(
            "EMPTY_LINE",
            "\u{201c}Para one\u{2026}\u{201d}\n\n\n\n\nPara two."
        ),
        vec![flag("\u{201d}", &[])]
    );
    assert_eq!(
        builtin_flags("EMPTY_LINE", "Para one.\n\n\nPara two."),
        vec![]
    );
    assert_eq!(
        builtin_flags("EMPTY_LINE", "Para one.\n \n\n\nPara two."),
        vec![]
    );
}

#[test]
fn builtin_empty_line_is_off_by_default() {
    let enabled = |id: &str| {
        RULES
            .builtins()
            .iter()
            .find(|rule| rule.id() == id)
            .unwrap()
            .enabled()
    };
    assert!(!enabled("EMPTY_LINE"));
    assert!(enabled("DOUBLE_PUNCTUATION"));
    assert!(enabled("WHITESPACE_RULE"));
    assert!(enabled("SENTENCE_WHITESPACE"));
}

#[test]
fn builtin_multiple_whitespace_skips_indentation_after_a_line_break() {
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Line one\n   indented line."),
        vec![]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Line one\n\t  indented line."),
        vec![flag("  ", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Line one\r\n   indented line."),
        vec![flag("   ", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "Line one.\n   Indented line."),
        vec![flag("   ", &[" "])]
    );
    assert_eq!(
        builtin_flags("WHITESPACE_RULE", "  Leading spaces here."),
        vec![flag("  ", &[" "])]
    );
}

#[test]
fn builtin_sentence_whitespace_looks_across_paragraphs() {
    assert_eq!(
        builtin_flags("SENTENCE_WHITESPACE", "Line one\nLine two.Next one."),
        vec![flag("Next", &[" Next"])]
    );
    assert_eq!(
        builtin_flags("SENTENCE_WHITESPACE", "Wait!\u{201d}He left."),
        vec![flag("He", &[" He"])]
    );
}

/// A U+2029 or U+2028 after a sentence is a break, not a missing space (#193). LanguageTool 6.6
/// flags the sentence after a U+2029; LanguageTool fixed that after 6.6.
#[test]
fn builtin_sentence_whitespace_treats_unicode_line_breaks_as_whitespace() {
    for text in [
        "He left.\u{2029}Then.",
        "He left.\u{2029}\u{201c}No\u{2014}don\u{2019}t,\u{201d} she said.",
        "\u{201c}Wait\u{2026}\u{201d}\u{2029}He didn\u{2019}t.",
        "Fine\u{2014}fine.\u{2029}\u{2029}Next.",
        "One.\u{2028}Two.\u{2029}Three\u{2026}\u{2028}Four.",
        "He left.\u{2029}  Then\u{2014}nothing.",
    ] {
        assert_eq!(
            builtin_flags("SENTENCE_WHITESPACE", text),
            vec![],
            "{text:?}"
        );
    }
    assert_eq!(
        builtin_flags(
            "SENTENCE_WHITESPACE",
            "He left.\u{2029}\u{201c}No.\u{201d}Then\u{2014}he\u{2019}d gone.\u{2028}Fine."
        ),
        vec![flag("Then", &[" Then"])]
    );
}

#[test]
fn builtin_sentence_whitespace_skips_a_sentence_opening_with_a_line_break() {
    use nlprule::builtin::en::SentenceWhitespace;
    // The splitter keeps a line break at the end of the previous sentence, so
    // pair a previous sentence that has no trailing whitespace with one that
    // opens with the break, as LanguageTool's `isLineBreakToken` check covers.
    let previous: Vec<_> = TOKENIZER.pipe("\u{201c}He left\u{2026}\u{201d}").collect();
    assert_eq!(previous.len(), 1);
    for (text, opener) in [
        ("Then.\u{2029}\u{2029}Now\u{2014}gone.", "\u{2029}"),
        ("Then.\r\rShe didn\u{2019}t.", "\r"),
    ] {
        let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
        let next = &sentences[1];
        assert!(next.text().starts_with(opener), "{:?}", next.text());
        assert_eq!(
            SentenceWhitespace.check_with_previous(&sentences[1..], previous.last()),
            vec![],
            "{text:?}"
        );
    }
    // Without the leading break the same pairing is flagged.
    let sentences: Vec<_> = TOKENIZER.pipe("Then.").collect();
    assert_eq!(
        SentenceWhitespace
            .check_with_previous(&sentences, previous.last())
            .len(),
        1
    );
}

#[test]
fn builtin_empty_line_ignores_the_end_of_the_text() {
    assert_eq!(builtin_flags("EMPTY_LINE", "Para one.\n\n\n\n"), vec![]);
    assert_eq!(
        builtin_flags("EMPTY_LINE", "Para one\n\n\n\nPara two.\n\n\n\nPara three."),
        vec![flag("one", &[]), flag(".", &[])]
    );
}

#[test]
fn builtin_double_punctuation_at_sentence_end() {
    let dots = || vec![flag("..", &[".", "\u{2026}"])];
    assert_eq!(builtin_flags("DOUBLE_PUNCTUATION", "He left.."), dots());
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "He left..\u{201d} she said."),
        dots()
    );
    assert_eq!(
        builtin_flags("DOUBLE_PUNCTUATION", "He left..\u{2028}Then he came back."),
        dots()
    );
    assert_eq!(builtin_flags("DOUBLE_PUNCTUATION", "Hey!.."), vec![]);
}

#[test]
fn builtin_empty_line_runs_in_suggest_when_enabled() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    for rule in rules.builtins_mut() {
        if rule.id() == "EMPTY_LINE" {
            rule.enable();
        }
    }
    let empty_lines = |text: &str| {
        rules
            .suggest(text, &TOKENIZER)
            .into_iter()
            .filter(|suggestion| suggestion.source() == "BUILTIN/EMPTY_LINE")
            .map(|suggestion| suggestion.span().char().clone())
            .collect::<Vec<_>>()
    };

    assert_eq!(
        empty_lines("\u{201c}Go.\u{201d}\n\n\n\nShe went\u{2014}slowly\u{2026}\n\n\n\n"),
        vec![4..5]
    );
}

/// Flags of `WHITESPACE_RULE` from [`Rules::suggest`] with the profile `name`. The grammar
/// rules are off, because `TYPOGRAPHY/CONSECUTIVE_SPACES` claims some of the same spans.
fn whitespace_flags(name: &str, text: &str) -> Vec<String> {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    assert!(rules
        .apply_profile(&Profile::by_name(name).unwrap())
        .is_empty());
    rules.rules_mut().iter_mut().for_each(|rule| rule.disable());
    let chars: Vec<char> = text.chars().collect();
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/WHITESPACE_RULE")
        .map(|suggestion| chars[suggestion.span().char().clone()].iter().collect())
        .collect()
}

/// Runs the long-sentence builtin directly, so that no other rule's
/// overlapping match hides it, and returns the flagged texts.
fn long_sentence_flags(text: &str) -> Vec<String> {
    builtin_flags("TOO_LONG_SENTENCE", text)
        .into_iter()
        .map(|(flagged, _)| flagged)
        .collect()
}

/// `n` distinct words, so that no repeated-word rule matches.
fn distinct_words(n: usize) -> String {
    (0..n)
        .map(|i| format!("k{}", i))
        .collect::<Vec<_>>()
        .join(" ")
}

fn pronoun_flag(word: &str, pronoun: &str) -> (String, Vec<String>) {
    (
        word.to_owned(),
        vec![
            format!("Furthermore, {}", pronoun),
            format!("Likewise, {}", pronoun),
            format!("Not only that, but {}", pronoun),
        ],
    )
}

#[test]
fn word_repeat_beginning_flags_the_third_sentence() {
    let flags = |text| suggest_flags(&RULES, "ENGLISH_WORD_REPEAT_BEGINNING_RULE", text);
    assert_eq!(
        flags("He ran home. He sat down. He left the room."),
        vec![pronoun_flag("He", "he")]
    );
    assert_eq!(
        flags("I ran home. I sat down. I left the room."),
        vec![pronoun_flag("I", "I")]
    );
    assert_eq!(
        flags("Then we ran. Then we sat. Then we left."),
        vec![("Then".to_owned(), vec![])]
    );
    assert_eq!(
        flags("She ran home. She sat down.\nShe left the room. She slept well."),
        vec![pronoun_flag("She", "she"), pronoun_flag("She", "she")]
    );
    assert_eq!(
        flags("He\u{2019}s here now. He\u{2019}s there now. He\u{2019}s gone now."),
        vec![pronoun_flag("He", "he")]
    );
}

#[test]
fn word_repeat_beginning_looks_across_paragraphs() {
    assert_eq!(
        suggest_flags(
            &RULES,
            "ENGLISH_WORD_REPEAT_BEGINNING_RULE",
            "He ran home.\n\nHe sat down.\n\nHe left the room."
        ),
        vec![pronoun_flag("He", "he")]
    );
    for paragraph in ["He ran home.", "He sat down.", "He left the room."] {
        assert!(suggest_flags(&RULES, "ENGLISH_WORD_REPEAT_BEGINNING_RULE", paragraph).is_empty());
    }
}

#[test]
fn word_repeat_beginning_counts_sentences_after_a_line_separator() {
    let text = "He ran home.\u{2028}He sat down. He left the room. He slept.";
    let flags = suggest_flags(&RULES, "ENGLISH_WORD_REPEAT_BEGINNING_RULE", text);
    assert_eq!(flags, vec![pronoun_flag("He", "he")]);
    let start = RULES
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .find(|x| x.source() == "BUILTIN/ENGLISH_WORD_REPEAT_BEGINNING_RULE")
        .unwrap()
        .span()
        .char()
        .start;
    assert_eq!(start, text.chars().count() - "He slept.".len());
}

#[test]
fn word_repeat_beginning_flags_a_repeated_adverb() {
    assert_eq!(
        suggest_flags(
            &RULES,
            "ENGLISH_WORD_REPEAT_BEGINNING_RULE",
            "Moreover, the street is residential. Moreover, it was named after a poet."
        ),
        vec![(
            "Moreover".to_owned(),
            [
                "Besides",
                "Also",
                "Additionally",
                "Furthermore",
                "In addition",
                "As well as"
            ]
            .iter()
            .map(|x| x.to_string())
            .collect()
        )]
    );
}

#[test]
fn word_repeat_beginning_counts_the_final_whitespace_of_the_text() {
    let flags = |text| suggest_flags(&RULES, "ENGLISH_WORD_REPEAT_BEGINNING_RULE", text);
    assert!(flags("He ran. He sat. He.").is_empty());
    assert_eq!(
        flags("He ran. He sat. He. "),
        vec![pronoun_flag("He", "he")]
    );
}

#[test]
fn word_repeat_beginning_ignores_fiction_punctuation_and_articles() {
    for text in [
        "\u{201c}He ran home.\u{201d} He sat down. He left the room.",
        "\u{201c}He ran home,\u{201d} she said. \u{201c}He sat down,\u{201d} she said. \u{201c}He left,\u{201d} she said.",
        "He ran home. He sat\u{2028}down. He left the room.",
        "He \u{2014} sat down. He \u{2014} ran off. He \u{2014} left us.",
        "We ran home\u{2026} We sat down\u{2026} We left the room.",
        "The cat sat. The dog ran. The bird flew.",
    ] {
        assert_eq!(
            suggest_flags(&RULES, "ENGLISH_WORD_REPEAT_BEGINNING_RULE", text),
            vec![],
            "{}",
            text
        );
    }
}

#[test]
fn fiction_profile_disables_word_repeat_beginning() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.apply_profile(&Profile::fiction());
    assert!(suggest_flags(
        &rules,
        "ENGLISH_WORD_REPEAT_BEGINNING_RULE",
        "He ran home. He sat down. He left the room."
    )
    .is_empty());
}

#[test]
fn fiction_profile_keeps_long_sentence_and_paragraph_off() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    rules.apply_profile(&Profile::fiction());
    for rule in rules.builtins() {
        if matches!(rule.id(), "TOO_LONG_SENTENCE" | "TOO_LONG_PARAGRAPH") {
            assert!(!rule.enabled(), "{}", rule.id());
        }
    }
}

#[test]
fn long_sentence_and_paragraph_are_off_by_default() {
    let text = format!("The {} sat.", distinct_words(250));
    assert!(suggest_flags(&RULES, "TOO_LONG_SENTENCE", &text).is_empty());
    assert!(suggest_flags(&RULES, "TOO_LONG_PARAGRAPH", &text).is_empty());
}

#[test]
fn long_sentence_flags_more_than_forty_words() {
    let text = format!("The {} sat.", distinct_words(39));
    assert_eq!(long_sentence_flags(&text), vec![text.clone()]);
    assert!(long_sentence_flags(&format!("The {} sat.", distinct_words(38))).is_empty());

    let text = format!("The {} sat\u{2026}", distinct_words(39));
    assert_eq!(
        long_sentence_flags(&text),
        vec![text.trim_end_matches('\u{2026}').to_owned()]
    );
}

#[test]
fn long_sentence_reports_a_sentence_once() {
    let text = format!("The {} sat.", distinct_words(90));
    assert_eq!(long_sentence_flags(&text), vec![text.clone()]);
}

#[test]
fn long_sentence_counts_words_between_separators() {
    let words = distinct_words(39);
    let (first, rest) = words.split_at(words.find(" k20").unwrap());
    for (separator, flagged) in [(":", false), ("\n", false), ("\u{2028}", true), (" ", true)] {
        let text = format!("The {}{}{} sat.", first, separator, rest.trim_start());
        assert_eq!(
            long_sentence_flags(&text).len(),
            usize::from(flagged),
            "{:?}",
            text
        );
    }
}

#[test]
fn long_sentence_skips_quoted_words_and_asides() {
    let quoted = |open: &str, close: &str, before: usize| {
        format!(
            "The {} {}q1 q2{} {} sat.",
            distinct_words(before),
            open,
            close,
            distinct_words(19)
        )
    };
    assert!(long_sentence_flags(&quoted("\u{201c}", "\u{201d}", 19)).is_empty());
    assert_eq!(
        long_sentence_flags(&quoted("\u{201c}", "\u{201d}", 20)).len(),
        1
    );
    assert!(long_sentence_flags(&quoted("\u{2014} ", " \u{2014}", 19)).is_empty());
    assert!(long_sentence_flags(&quoted("(", ")", 19)).is_empty());
    assert!(long_sentence_flags(&format!("The {} 12 sat.", distinct_words(38))).is_empty());
    assert_eq!(
        long_sentence_flags(&format!("The {} don\u{2019}t sat.", distinct_words(37))).len(),
        1
    );
}

#[test]
fn long_sentence_skips_dialogue() {
    assert!(long_sentence_flags(&format!("The {} sat!\u{201d}", distinct_words(45))).is_empty());
    assert!(long_sentence_flags(&format!(
        "She asked \u{201c}why?\u{201d} and the {} sat.",
        distinct_words(45)
    ))
    .is_empty());
}

#[test]
fn long_sentence_yields_to_an_overlapping_match() {
    let text = format!("The {} sat.", distinct_words(39));
    assert_eq!(
        suggest_flags(&PICKY_RULES, "TOO_LONG_SENTENCE", &text).len(),
        1
    );
    let text = format!("The the {} sat.", distinct_words(39));
    assert!(suggest_flags(&PICKY_RULES, "TOO_LONG_SENTENCE", &text).is_empty());
}

/// `n` words in sentences of ten, numbered from `w0`.
fn paragraph_of(n: usize) -> String {
    (0..n)
        .collect::<Vec<_>>()
        .chunks(10)
        .map(|chunk| {
            let words: Vec<String> = chunk.iter().map(|i| format!("w{}", i)).collect();
            format!("A{}.", words.join(" "))
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn long_paragraph_flags_more_than_225_words() {
    let flags = |text: &str| suggest_flags(&PICKY_RULES, "TOO_LONG_PARAGRAPH", text);
    let flagged = vec![("w218 w219".to_owned(), vec![])];
    assert_eq!(flags(&paragraph_of(226)), flagged);
    assert!(flags(&paragraph_of(225)).is_empty());
    assert_eq!(
        flags(&format!(
            "Short para.\n\n{}\n\nShort para.",
            paragraph_of(230)
        )),
        flagged
    );
    assert_eq!(
        flags(&format!("{}\r\n\r\nShort para.", paragraph_of(230))),
        flagged
    );
    assert_eq!(
        flags(&format!("{}\u{2028}{}", paragraph_of(230), paragraph_of(5))),
        flagged
    );
}

#[test]
fn long_paragraph_skips_paragraphs_with_line_breaks() {
    let flags = |text: &str| suggest_flags(&PICKY_RULES, "TOO_LONG_PARAGRAPH", text);
    assert!(flags(&format!("{}\nNext line.", paragraph_of(230))).is_empty());
    assert!(flags(&format!("Intro line.\n{}", paragraph_of(230))).is_empty());
}

#[test]
fn long_paragraph_does_not_count_fiction_punctuation() {
    let flags = |text: &str| suggest_flags(&PICKY_RULES, "TOO_LONG_PARAGRAPH", text);
    let dialogue = " She said, \u{201c}hi\u{201d} \u{2014} then\u{2026} don\u{2019}t.";
    assert!(flags(&format!("{}{}", paragraph_of(219), dialogue)).is_empty());
    assert_eq!(
        flags(&format!("{}{}", paragraph_of(220), dialogue)),
        vec![("w218 w219".to_owned(), vec![])]
    );
}

#[test]
fn fiction_profile_skips_whitespace_indentation() {
    let text = "    \u{201c}Go,\u{201d} she said.\n    He went\u{2014}slowly\u{2026}\n\n  \u{2018}Fine.\u{2019}\r\n    It\u{2019}s late.\n\t  Verse one,\u{2028}   verse two.  Then  stop.";

    assert_eq!(
        whitespace_flags("default", text),
        vec!["    ", "    ", "  ", "    ", "\t  ", "\u{2028}   ", "  "]
    );
    assert_eq!(whitespace_flags("fiction", text), vec!["  "]);
}

#[test]
fn fiction_profile_keeps_other_whitespace_flags() {
    assert_eq!(
        whitespace_flags(
            "fiction",
            "\u{201c}Hi,\u{201d}  she said \u{2014}  softly\u{2026}\n  Then\u{a0}\u{a0}left \u{2028}  here."
        ),
        vec!["  ", "  ", "\u{a0}\u{a0}"]
    );
}

/// Under the fiction profile every double space between words gets the builtin's message;
/// LanguageTool's `CONSECUTIVE_SPACES` says the same thing in other words and is disabled.
#[test]
fn fiction_profile_reports_double_spaces_once() {
    let text = "\u{201c}It was a  cold night\u{2014}the  coldest\u{2026}\u{201d} She didn\u{2019}t  move.\u{2028}He  waited.";
    let flags = |name: &str| -> Vec<(String, String, String)> {
        let mut rules = Rules::new(RULES_PATH).unwrap();
        assert!(rules
            .apply_profile(&Profile::by_name(name).unwrap())
            .is_empty());
        let chars: Vec<char> = text.chars().collect();
        rules
            .suggest(text, &TOKENIZER)
            .into_iter()
            .map(|suggestion| {
                (
                    chars[suggestion.span().char().clone()].iter().collect(),
                    suggestion.source().to_owned(),
                    suggestion.message().to_owned(),
                )
            })
            .filter(|(flagged, _, _): &(String, _, _)| flagged.trim().is_empty())
            .collect()
    };

    let builtin = |flagged: &str| {
        (
            flagged.to_owned(),
            "BUILTIN/WHITESPACE_RULE".to_owned(),
            "Possible typo: repeated spaces".to_owned(),
        )
    };
    assert_eq!(flags("fiction"), vec![builtin("  "); 4]);
    assert!(flags("default")
        .iter()
        .any(|(_, source, _)| source == "TYPOGRAPHY/CONSECUTIVE_SPACES/0"));
}

fn builtin_enabled(id: &str) -> bool {
    RULES
        .builtins()
        .iter()
        .find(|rule| rule.id() == id)
        .unwrap()
        .enabled()
}

#[test]
fn builtins_from_issue_81_follow_languagetool_defaults() {
    assert!(builtin_enabled("EN_COMPOUNDS"));
    assert!(builtin_enabled("EN_SPECIFIC_CASE"));
    assert!(!builtin_enabled("EN_DASH_RULE"));
    assert!(!builtin_enabled("EN_PLAIN_ENGLISH_REPLACE"));
    assert!(!builtin_enabled("EN_REDUNDANCY_REPLACE"));
    assert!(!builtin_enabled("PROFANITY"));
}

#[test]
fn builtin_compounds_flags_separated_compounds() {
    assert_eq!(
        builtin_flags(
            "EN_COMPOUNDS",
            "\u{201c}I need a part time job,\u{201d} she said.\n\nHe sent an e mail\u{2026} then a back fire."
        ),
        vec![
            flag("part time", &["part-time"]),
            flag("e mail", &["e-mail"]),
            flag("back fire", &["backfire"]),
        ]
    );
    assert_eq!(
        builtin_flags(
            "EN_COMPOUNDS",
            "She\u{2019}d a part time job\u{2014}and an e mail.\u{2028}Part time work is fine."
        ),
        vec![
            flag("part time", &["part-time"]),
            flag("e mail", &["e-mail"])
        ]
    );
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "Part time work is fine. PART TIME too."),
        vec![flag("Part time", &["Part-time"])]
    );
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "The part-time job and the back-fire."),
        vec![flag("back-fire", &["backfire"])]
    );
}

#[test]
fn builtin_compounds_reports_sub_rule_ids_and_messages() {
    let suggestions = RULES.suggest("He sent me an e mail\u{2026}", &TOKENIZER);
    let compound = suggestions
        .iter()
        .find(|suggestion| suggestion.source().starts_with("BUILTIN/EN_COMPOUNDS"))
        .expect("the compound is flagged");
    assert_eq!(compound.source(), "BUILTIN/EN_COMPOUNDS_E_MAIL");
    assert_eq!(compound.replacements(), &["e-mail".to_string()]);
    assert_eq!(
        compound.message(),
        "This word is normally spelled with a hyphen."
    );
}

/// A sentence-initial compound starts at its first word, not at the whitespace before it
/// (LanguageTool 6.6 covers the whitespace), so accepting the suggestion keeps indentation.
#[test]
fn builtin_compounds_span_starts_at_the_first_word() {
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "He left.\n  Part time work."),
        vec![flag("Part time", &["Part-time"])]
    );
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "He left.  Part time work."),
        vec![flag("Part time", &["Part-time"])]
    );
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "He left.\n  e mail me now."),
        vec![flag("e mail", &["e-mail"])]
    );
    assert_eq!(
        builtin_flags(
            "EN_COMPOUNDS",
            "\u{201c}He left.\u{201d}\n\t  E mail her\u{2026} it\u{2019}s a back fire\u{2014}really.\u{2028}"
        ),
        vec![flag("E mail", &["E-mail"]), flag("back fire", &["backfire"])]
    );
    assert_eq!(
        builtin_flags(
            "EN_COMPOUNDS",
            "\u{201c}Go.\u{201d}\n\u{a0}\u{a0}E mail me\u{2026}"
        ),
        vec![flag("E mail", &["E-mail"])]
    );
    assert_eq!(
        builtin_flags("EN_COMPOUNDS", "\u{3000}E mail me."),
        vec![flag("E mail", &["E-mail"])]
    );

    let text = "\u{201c}Go.\u{201d}\n  E mail me\u{2014}now\u{2026}";
    let compound = RULES
        .suggest(text, &TOKENIZER)
        .into_iter()
        .find(|suggestion| suggestion.source().starts_with("BUILTIN/EN_COMPOUNDS"))
        .expect("the compound is flagged");
    let span = compound.span().byte().clone();
    let fixed = format!(
        "{}{}{}",
        &text[..span.start],
        compound.replacements()[0],
        &text[span.end..]
    );
    assert_eq!(fixed, "\u{201c}Go.\u{201d}\n  E-mail me\u{2014}now\u{2026}");
}

/// `Builtin::check` and `Builtin::check_text` filter suggestions through the speller they are
/// given, as `Rules::suggest` does: LanguageTool 6.6 suggests only `Yo-jong` here.
#[test]
fn builtin_check_filters_compounds_with_the_speller() {
    let text = "\u{201c}Kim Yo jong spoke\u{2026}\u{201d}";
    let builtin = RULES
        .builtins()
        .iter()
        .find(|rule| rule.id() == "EN_COMPOUNDS")
        .unwrap()
        .builtin();
    let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
    let replacements = |suggestions: Vec<Suggestion>| -> Vec<Vec<String>> {
        suggestions
            .iter()
            .map(|suggestion| suggestion.replacements().to_vec())
            .collect()
    };
    let speller = RULES.spell().speller();

    assert_eq!(
        replacements(builtin.check(&sentences[0], speller.as_ref())),
        vec![vec!["Yo-jong".to_string()]]
    );
    assert_eq!(
        replacements(builtin.check_text(&sentences, speller.as_ref())),
        vec![vec!["Yo-jong".to_string()]]
    );
    assert_eq!(
        replacements(
            RULES
                .suggest(text, &TOKENIZER)
                .into_iter()
                .filter(|suggestion| suggestion.source().starts_with("BUILTIN/EN_COMPOUNDS"))
                .collect()
        ),
        vec![vec!["Yo-jong".to_string()]]
    );
    assert_eq!(
        replacements(builtin.check_text(&sentences, None)),
        vec![vec!["Yo-jong".to_string(), "Yojong".to_string()]]
    );
}

#[test]
fn builtin_compounds_skips_antipatterns() {
    assert_eq!(
        builtin_flags(
            "EN_COMPOUNDS",
            "We well received your email.\n\nHow well received was it?\n\nThey\u{2019}re re married."
        ),
        vec![]
    );
}

#[test]
fn builtin_specific_case_flags_proper_nouns() {
    let flag_harry = || flag("Harry potter", &["Harry Potter"]);
    assert_eq!(
        builtin_flags(
            "EN_SPECIFIC_CASE",
            "She said \u{201c}harry potter\u{201d} twice\u{2026} and Harry potter\u{2019}s wand."
        ),
        vec![flag("harry potter", &["Harry Potter"]), flag_harry()]
    );
    assert_eq!(
        builtin_flags(
            "EN_SPECIFIC_CASE",
            "\u{201c}Harry potter,\u{201d} she said\u{2014}then the empire state building.\u{2028}HARRY POTTER is fine."
        ),
        vec![
            flag_harry(),
            flag("empire state building", &["Empire State Building"])
        ]
    );
}

#[test]
fn builtin_dash_flags_compounds_with_dashes() {
    assert_eq!(
        builtin_flags(
            "EN_DASH_RULE",
            "He wore a T\u{2014}shirt\u{2026} and a \u{201c}T \u{2013} shirt\u{201d}.\u{2028}The well\u{2014}being of all."
        ),
        vec![
            flag("T\u{2014}shirt", &["T-shirt"]),
            flag("T \u{2013} shirt", &["T-shirt"]),
            flag("well\u{2014}being", &["well-being"]),
        ]
    );
    assert_eq!(
        builtin_flags(
            "EN_DASH_RULE",
            "She\u{2019}d gone\u{2014}well, being late\u{2014}and an E\u{2014}mail."
        ),
        vec![]
    );
}

#[test]
fn builtin_plain_english_and_redundancy_flag_phrases() {
    assert_eq!(
        builtin_flags(
            "EN_PLAIN_ENGLISH_REPLACE",
            "It was a fatal outcome, she said\u{2014}in close proximity to the door\u{2026}"
        ),
        vec![
            flag("fatal outcome", &["death"]),
            flag("in close proximity to", &["close", "near"]),
        ]
    );
    assert_eq!(
        builtin_flags(
            "EN_PLAIN_ENGLISH_REPLACE",
            "\u{201c}She\u{2019}d a fatal outcome,\u{201d} he said.\u{2028}Fatal outcome."
        ),
        vec![
            flag("fatal outcome", &["death"]),
            flag("Fatal outcome", &["Death"])
        ]
    );
    assert_eq!(
        builtin_flags(
            "EN_REDUNDANCY_REPLACE",
            "\u{201c}A tuna fish,\u{201d} she\u{2019}d said.\u{2028}Tuna fish again."
        ),
        vec![flag("tuna fish", &["tuna"]), flag("Tuna fish", &["Tuna"])]
    );
    assert_eq!(
        builtin_flags(
            "EN_REDUNDANCY_REPLACE",
            "A tuna fish\u{2014}no\u{2026} a tuna fish."
        ),
        vec![flag("tuna fish", &["tuna"]), flag("tuna fish", &["tuna"])]
    );
}

#[test]
fn builtin_profanity_flags_without_suggestions() {
    assert_eq!(
        builtin_flags(
            "PROFANITY",
            "\u{201c}What a twat,\u{201d} he said\u{2026} arsehole\u{2014}really.\u{2028}Twat."
        ),
        vec![flag("twat", &[]), flag("arsehole", &[]), flag("Twat", &[])]
    );
    assert_eq!(
        builtin_flags("PROFANITY", "She\u{2019}s a twat\u{2019}s friend."),
        vec![flag("twat", &[])]
    );
}

fn typo_or_of_matches(rules: &Rules, text: &str) -> Vec<(std::ops::Range<usize>, String)> {
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|s| s.source() == "TYPOS/TYPO_OR_OF/1")
        .map(|s| {
            let chars: Vec<char> = text.chars().collect();
            (
                s.span().char().clone(),
                chars[s.span().char().clone()].iter().collect(),
            )
        })
        .collect()
}

#[test]
fn reference_with_literal_suffix_matches_reference_plus_suffix() {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let selector: Selector = "typos/typo_or_of".try_into().unwrap();
    rules.select_mut(&selector).for_each(|x| x.enable());

    for text in [
        "The 1’s are the active couple or actives in the middle; the 2’s wait.",
        "“The active couple or actives in the middle—wait…”\u{2028}She nodded.",
        "The active couple or Actives in the middle wait.",
        "The ACTIVE couple or actives in the middle wait.",
    ] {
        assert!(typo_or_of_matches(&rules, text).is_empty(), "{}", text);
    }

    for text in [
        "Below is a list or guidelines on how to handle it.",
        "“The active couple or dancers in the middle—wait…”\u{2028}She nodded.",
    ] {
        let matches = typo_or_of_matches(&rules, text);
        assert_eq!(matches.len(), 1, "{}", text);
        assert_eq!(matches[0].1, "or", "{}", text);
    }
}

fn rule_matches(selector: &str, text: &str) -> Vec<(String, std::ops::Range<usize>, Vec<String>)> {
    let mut rules = Rules::new(RULES_PATH).unwrap();
    let selector: Selector = selector.try_into().unwrap();
    rules.rules_mut().iter_mut().for_each(|rule| rule.disable());
    rules
        .builtins_mut()
        .iter_mut()
        .for_each(|rule| rule.disable());
    rules.select_mut(&selector).for_each(|x| x.enable());
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .map(|s| {
            (
                s.source().to_string(),
                s.span().char().clone(),
                s.replacements().to_vec(),
            )
        })
        .collect()
}

#[test]
fn apostrophe_type_filter_reads_the_raw_apostrophe() {
    let text = "An actress\u{2019}s role. An actress's role.\u{2028}\u{201c}She didn\u{2019}t \
                know\u{2026},\u{201d} he said \u{2014} the old man\u{2019}s dog ran to the bank \
                and \u{2018}that\u{2019}s that\u{2019} \u{2014} I can\u{2019}t.";
    let spans = |selector: &str| -> Vec<(std::ops::Range<usize>, Vec<String>)> {
        rule_matches(selector, text)
            .into_iter()
            .map(|(_, span, replacements)| (span, replacements))
            .collect()
    };

    assert_eq!(
        spans("typography/typewriter_apostrophe"),
        vec![
            (10..12, vec!["'s".to_string()]),
            (79..81, vec!["'s".to_string()]),
            (111..113, vec!["'s".to_string()]),
        ]
    );
    assert_eq!(
        spans("typography/typographical_apostrophe"),
        vec![(29..31, vec!["\u{2019}s".to_string()])]
    );
}

#[test]
fn apostrophe_type_filter_on_fiction_clitics() {
    let text = "\u{201c}Give \u{2019}em hell,\u{201d} said James\u{2019} brother\u{2014}rock \
                \u{2019}n\u{2019} roll\u{2026}\u{2028}I\u{2019}ll go. Rock 'n' roll.";
    let spans = |selector: &str| -> Vec<(std::ops::Range<usize>, Vec<String>)> {
        rule_matches(selector, text)
            .into_iter()
            .map(|(_, span, replacements)| (span, replacements))
            .collect()
    };

    assert_eq!(
        spans("typography/typewriter_apostrophe"),
        vec![
            (42..45, vec!["'n'".to_string()]),
            (53..56, vec!["'ll".to_string()]),
        ]
    );
    assert_eq!(
        spans("typography/typographical_apostrophe"),
        vec![(66..69, vec!["\u{2019}n\u{2019}".to_string()])]
    );
}

#[test]
fn consecutive_spaces_match_like_languagetool() {
    let spans = |selector: &str, text: &str| -> Vec<std::ops::Range<usize>> {
        rule_matches(selector, text)
            .into_iter()
            .map(|(_, span, _)| span)
            .collect()
    };

    assert_eq!(
        spans(
            "typography/consecutive_spaces",
            "This is a  sentence with two spaces.  Next."
        ),
        vec![9..11]
    );
    assert_eq!(
        spans("typography/consecutive_spaces", "This  is  a  test."),
        vec![4..6, 8..10]
    );
    assert_eq!(
        spans(
            "typography/consecutive_spaces",
            "\u{201c}Wait.\u{201d}  She left \u{2014} then  came back\u{2026}  \u{201c}Fine,\u{201d} he said.\u{2028}"
        ),
        vec![24..26]
    );
    assert_eq!(
        spans(
            "typography/consecutive_spaces_single_char",
            "This  is  a  test."
        ),
        vec![11..13]
    );
}

fn repetition_flags(rules: &Rules, group: &str, text: &str) -> Vec<std::ops::Range<usize>> {
    let group = format!("REPETITIONS_STYLE/{}/", group);
    rules
        .suggest(text, &*TOKENIZER)
        .into_iter()
        .filter(|suggestion| suggestion.source().starts_with(&group))
        .map(|suggestion| suggestion.span().char().clone())
        .collect()
}

#[test]
fn repetition_rule_needs_a_previous_match() {
    let flags = |text: &str| repetition_flags(&RULES, "REP_THANK_YOU_FOR", text);

    assert_eq!(
        flags("Thank you for the tea."),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        flags("Thank you for the tea. Thank you for the cake."),
        vec![23..36]
    );
    assert_eq!(
        flags("Thank you for the tea.\n\nThank you for the cake."),
        vec![24..37]
    );
}

#[test]
fn repetition_rule_counts_tokens_between_matches() {
    let flags = |text: &str| repetition_flags(&RULES, "REP_THANK_YOU_FOR", text);
    let filler = |words| format!("{}. ", vec!["Dogs"; words].join(" "));

    assert_eq!(
        flags(&format!(
            "Thank you for the tea. {}Thank you for the cake.",
            filler(52)
        )),
        vec![284..297]
    );
    assert_eq!(
        flags(&format!(
            "Thank you for the tea. {}Thank you for the cake.",
            filler(53)
        )),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        flags(&format!(
            "Thank you for the tea. {}Thank you for the cake. Thank you for the pie.",
            filler(53)
        )),
        vec![313..326]
    );
}

#[test]
fn repetition_rule_state_is_per_suggest_call() {
    let flags = |text: &str| repetition_flags(&RULES, "REP_THANK_YOU_FOR", text);

    assert_eq!(
        flags("Thank you for the tea."),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        flags("Thank you for the cake."),
        Vec::<std::ops::Range<usize>>::new()
    );
}

#[test]
fn repetition_rule_handles_fiction_punctuation() {
    let flags = |text: &str| repetition_flags(&RULES, "REP_THANK_YOU_FOR", text);

    assert_eq!(
        flags(
            "Thank you for the tea\u{2026} Thank you for the cake.\u{2028}Thank you for the pie."
        ),
        vec![23..36, 47..60]
    );
    assert_eq!(
        flags("Thank you for the tea. It\u{2019}s late. Thank you for the cake."),
        vec![34..47]
    );
    assert_eq!(
        flags("\u{201c}Thank you for the tea.\u{201d} \u{201c}Thank you for the cake.\u{201d}"),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        flags("Thank you for the tea\u{2014}thank you for the cake."),
        Vec::<std::ops::Range<usize>>::new()
    );
}

#[test]
fn repetition_rule_counts_sentence_start_when_whitespace_leads() {
    let flags = |text: &str| repetition_flags(&RULES, "REP_THANK_YOU_FOR", text);
    let filler = |words, end| format!("{}{}", vec!["Dogs"; words].join(" "), end);

    assert_eq!(
        flags(&format!(
            "  Thank you for the tea. {}Thank you for the cake.",
            filler(53, ". ")
        )),
        vec![291..304]
    );
    assert_eq!(
        flags(&format!(
            "Thank you for the tea. \u{2028}{}Thank you for the cake.",
            filler(52, ". \u{2028}")
        )),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        flags(&format!(
            "Thank you for the tea. \u{2028}{}Thank you for the cake.",
            filler(51, ". \u{2028}")
        )),
        vec![281..294]
    );
}

#[test]
fn repetition_rule_state_spans_batches() {
    let text = format!(
        "Thank you for the tea.\n\n{}Thank you for the cake.",
        vec!["Dogs"; 52].join(" ") + ".\n\n"
    );
    let mut rules = RULES.clone();
    rules.set_batch_size(1);

    assert_eq!(
        repetition_flags(&rules, "REP_THANK_YOU_FOR", &text),
        vec![286..299]
    );
    assert_eq!(
        repetition_flags(&RULES, "REP_THANK_YOU_FOR", &text),
        vec![286..299]
    );
}

#[test]
fn repetition_rule_needs_min_prev_matches() {
    let mut rules = RULES.clone();
    rules
        .select_mut(&"repetitions_style/rep_passive_voice".try_into().unwrap())
        .for_each(|rule| rule.enable());
    let sentence = "It is also thought that tests are satisfactory. ";

    assert_eq!(
        repetition_flags(&rules, "REP_PASSIVE_VOICE", &sentence.repeat(4)),
        Vec::<std::ops::Range<usize>>::new()
    );
    assert_eq!(
        repetition_flags(&rules, "REP_PASSIVE_VOICE", &sentence.repeat(5)),
        vec![195..210]
    );
    assert_eq!(
        repetition_flags(
            &rules,
            "REP_PASSIVE_VOICE",
            &"It was announced that we won. ".repeat(5)
        ),
        vec![123..136]
    );
}

lazy_static! {
    static ref WORD_STATISTICS_RULES: Rules = {
        let mut rules = RULES.clone();
        rules.rules_mut().iter_mut().for_each(|rule| rule.disable());
        for rule in rules.builtins_mut() {
            if rule.id() == "EN_REPEATEDWORDS" || rule.id().starts_with("TOO_OFTEN_USED_") {
                rule.enable();
            } else {
                rule.disable();
            }
        }
        rules
    };
}

/// `(source, start char, matched text, replacements)` of the word-statistics builtins.
fn word_statistics_flags(rules: &Rules, text: &str) -> Vec<(String, usize, String, Vec<String>)> {
    let chars: Vec<char> = text.chars().collect();
    rules
        .suggest(text, &TOKENIZER)
        .into_iter()
        .map(|suggestion| {
            let span = suggestion.span().char().clone();
            (
                suggestion
                    .source()
                    .trim_start_matches("BUILTIN/")
                    .to_owned(),
                span.start,
                chars[span].iter().collect(),
                suggestion.replacements().to_vec(),
            )
        })
        .collect()
}

fn repeated(
    id: &str,
    start: usize,
    text: &str,
    replacements: &[&str],
) -> (String, usize, String, Vec<String>) {
    (
        format!("EN_REPEATEDWORDS_{}", id),
        start,
        text.to_owned(),
        replacements.iter().map(|x| (*x).to_owned()).collect(),
    )
}

#[test]
fn word_statistics_builtins_are_off_by_default() {
    let text = "He definitely ran. She definitely left. ".repeat(60);
    assert!(RULES.suggest(&text, &TOKENIZER).iter().all(|suggestion| {
        !suggestion.source().starts_with("BUILTIN/EN_REPEATEDWORDS")
            && !suggestion.source().starts_with("BUILTIN/TOO_OFTEN_USED_")
    }));
}

#[test]
fn repeated_words_flags_a_word_from_an_earlier_sentence() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);
    let definitely = ["certainly", "absolutely", "undoubtedly"];

    assert_eq!(
        flags("He definitely ran. She definitely left."),
        vec![repeated("DEFINITELY", 23, "definitely", &definitely)]
    );
    assert_eq!(
        flags("He definitely ran. Definitely she left."),
        vec![repeated(
            "DEFINITELY",
            19,
            "Definitely",
            &["Certainly", "Absolutely", "Undoubtedly"]
        )]
    );
    assert_eq!(
        flags("He definitely ran\u{2014}she definitely left. He definitely won. Ok."),
        vec![repeated("DEFINITELY", 42, "definitely", &definitely)]
    );
    assert_eq!(flags("He definitely ran. She DEFINITELY left."), vec![]);
    assert_eq!(flags("He definitely ran. She Definitely left."), vec![]);
    assert_eq!(flags("He definitely ran definitely. She left."), vec![]);
}

#[test]
fn repeated_words_inflects_the_synonyms() {
    assert_eq!(
        word_statistics_flags(
            &WORD_STATISTICS_RULES,
            "He chose it. She chooses it. They will choose it. We have chosen it."
        ),
        vec![
            repeated("CHOOSE", 17, "chooses", &["decides", "selects", "picks"]),
            repeated("CHOOSE", 39, "choose", &["decide", "select", "pick"]),
            repeated("CHOOSE", 58, "chosen", &["decided", "selected", "picked"]),
        ]
    );
}

#[test]
fn repeated_words_skips_sentences_that_do_not_end_in_a_full_stop() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);

    assert_eq!(flags("He definitely ran. She definitely left.\n"), vec![]);
    assert_eq!(flags("He definitely ran. She definitely left. "), vec![]);
    assert_eq!(
        flags("\u{201c}He definitely ran.\u{201d} She definitely left. Ok."),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran. \u{201c}She definitely left.\u{201d} Ok."),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran\u{2026} She definitely left. Ok."),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran\u{2028}She definitely left. Ok."),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran. She definitely left.\u{a0}"),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran. She definitely left.\u{202f}"),
        vec![]
    );
    assert_eq!(
        flags("He definitely ran.\u{a0}She definitely left. Ok."),
        vec![repeated(
            "DEFINITELY",
            23,
            "definitely",
            &["certainly", "absolutely", "undoubtedly"]
        )]
    );
    assert_eq!(
        flags("He definitely ran.\n\nShe definitely left. Ok."),
        vec![repeated(
            "DEFINITELY",
            24,
            "definitely",
            &["certainly", "absolutely", "undoubtedly"]
        )]
    );
}

#[test]
fn repeated_words_handles_dialogue_and_curly_apostrophes() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);

    assert_eq!(
        flags("He definitely ran. \u{201c}Definitely,\u{201d} she said. Ok."),
        vec![repeated(
            "DEFINITELY",
            20,
            "Definitely",
            &["Certainly", "Absolutely", "Undoubtedly"]
        )]
    );
    assert_eq!(
        flags("He\u{2019}s definitely gone. She\u{2019}s definitely here. Ok."),
        vec![repeated(
            "DEFINITELY",
            28,
            "definitely",
            &["certainly", "absolutely", "undoubtedly"]
        )]
    );
}

#[test]
fn repeated_words_respects_antipatterns_and_constraints() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);
    let nice = [
        "pleasant",
        "impressive",
        "enjoyable",
        "delightful",
        "charming",
    ];

    assert_eq!(
        flags("It was a nice day. It was nice. It was nice."),
        vec![repeated("NICE", 39, "nice", &nice)]
    );
    assert_eq!(
        flags("No problem, I said. The problem is there. Ok."),
        vec![]
    );
    assert_eq!(
        flags("We solve the problem. The problem is there. Ok."),
        vec![]
    );
    assert_eq!(
        flags("We need to go. We need to stop. We need help. We need help. Ok."),
        vec![repeated("NEED", 49, "need", &["require"])]
    );
    assert_eq!(
        flags("Literally. It is literally true. Take it literally. Ok."),
        vec![]
    );
    assert_eq!(
        flags("Several men came. Several hundred men came. Several women came. Ok."),
        vec![repeated(
            "SEVERAL",
            44,
            "Several",
            &["Many", "Numerous", "Various", "Countless"]
        )]
    );
}

#[test]
fn repeated_words_looks_back_150_words() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);
    let text = |dogs: usize, tail: &str| {
        format!(
            "He definitely ran. Dogs {}{}. She definitely left.",
            vec!["dogs"; dogs].join(" "),
            tail
        )
    };

    assert_eq!(flags(&text(144, "")).len(), 1);
    assert_eq!(flags(&text(145, "")), vec![]);
    assert_eq!(flags(&text(143, " need to")).len(), 1);
}

#[test]
fn repeated_words_state_is_per_suggest_call() {
    let flags = |text: &str| word_statistics_flags(&WORD_STATISTICS_RULES, text);

    assert_eq!(flags("He definitely ran."), vec![]);
    assert_eq!(flags("She definitely left."), vec![]);
}

const STATISTICS_NOUNS: &str = "dog tree river car book chair garden street city lamp bottle cup \
    plate shirt horse apple kitchen window door table house phone mountain forest island bridge \
    castle candle blanket pillow spoon fork knife basket ladder carpet curtain mirror wallet pocket \
    jacket helmet hammer engine wheel tunnel valley meadow village harbor lantern barrel saddle \
    anchor violin piano guitar trumpet tomato potato carrot onion banana lemon cherry peach pear \
    melon cabbage pepper garlic biscuit pastry sandwich elephant giraffe monkey tiger rabbit turtle \
    squirrel donkey camel dolphin parrot pigeon sparrow eagle owl spider beetle ant wasp nephew niece \
    uncle aunt cousin grandmother teacher doctor farmer";

fn cats_and_nouns(cat: &str, others: usize) -> String {
    cats_and_nouns_joined(cat, others, " ")
}

fn cats_and_nouns_joined(cat: &str, others: usize, separator: &str) -> String {
    let mut sentences = vec![cat.to_owned(); 5];
    sentences.extend(
        STATISTICS_NOUNS
            .split_whitespace()
            .take(others)
            .map(|noun| format!("I saw the {}.", noun)),
    );
    sentences.join(separator)
}

/// `(start char, matched text)` of the matches of the builtin `id`.
fn too_often_used(rules: &Rules, id: &str, text: &str) -> Vec<(usize, String)> {
    word_statistics_flags(rules, text)
        .into_iter()
        .filter(|(source, ..)| source == id)
        .map(|(_, start, matched, _)| (start, matched))
        .collect()
}

fn too_often_used_nouns(rules: &Rules, text: &str) -> Vec<usize> {
    word_statistics_flags(rules, text)
        .into_iter()
        .filter(|(source, ..)| source == "TOO_OFTEN_USED_NOUN_EN")
        .map(|(_, start, matched, replacements)| {
            assert_eq!(matched, "cat");
            assert!(replacements.is_empty());
            start
        })
        .collect()
}

#[test]
fn too_often_used_noun_needs_five_percent_of_100_nouns() {
    let flags = |text: &str| too_often_used_nouns(&WORD_STATISTICS_RULES, text);

    assert_eq!(
        flags(&cats_and_nouns("I saw the cat.", 95)),
        vec![10, 25, 40, 55, 70]
    );
    assert_eq!(
        flags(&cats_and_nouns("I saw the cat.", 96)),
        Vec::<usize>::new()
    );
    assert_eq!(
        flags(&cats_and_nouns("I saw the cat.", 94)),
        Vec::<usize>::new()
    );

    let text = cats_and_nouns("I saw the cat.", 95).replacen(
        "I saw the cat.",
        "I saw the cat\u{2014}the cat\u{2026}",
        1,
    );
    assert_eq!(flags(&text), vec![10, 18, 33, 48, 63, 78]);
}

#[test]
fn too_often_used_counts_dialogue_unless_the_case_is_disabled() {
    let dialogue = cats_and_nouns("\u{201c}I saw the cat,\u{201d} she said.", 95);
    assert_eq!(
        too_often_used_nouns(&WORD_STATISTICS_RULES, &dialogue),
        vec![11, 38, 65, 92, 119]
    );

    let mut rules = WORD_STATISTICS_RULES.clone();
    for rule in rules.builtins_mut() {
        if rule.id().starts_with("TOO_OFTEN_USED_") {
            assert!(rule.disable_case("DIRECT_SPEECH"));
        }
    }
    assert_eq!(too_often_used_nouns(&rules, &dialogue), Vec::<usize>::new());

    let narration = cats_and_nouns("I saw the cat.", 95);
    assert_eq!(
        too_often_used_nouns(&rules, &narration),
        vec![10, 25, 40, 55, 70]
    );
}

#[test]
fn too_often_used_min_percent_is_configurable() {
    let with_percent = |percent: usize| {
        let mut rules = WORD_STATISTICS_RULES.clone();
        rules.set_too_often_used_percent(percent).unwrap();
        rules
    };
    // 5 cats of 100 nouns, or 5% each.
    let text = cats_and_nouns("I saw the cat.", 95);

    assert_eq!(
        too_often_used_nouns(&with_percent(5), &text),
        vec![10, 25, 40, 55, 70]
    );
    assert_eq!(
        too_often_used_nouns(&with_percent(6), &text),
        Vec::<usize>::new()
    );
    assert_eq!(
        too_often_used(&with_percent(1), "TOO_OFTEN_USED_NOUN_EN", &text).len(),
        100
    );
    // "saw" is every verb.
    assert_eq!(
        too_often_used(&with_percent(100), "TOO_OFTEN_USED_VERB_EN", &text).len(),
        100
    );

    let rules = with_percent(4);
    assert!(rules
        .suggest(&text, &TOKENIZER)
        .iter()
        .filter(|suggestion| suggestion.source() == "BUILTIN/TOO_OFTEN_USED_NOUN_EN")
        .all(|suggestion| suggestion.message().contains("more than 4% times")));

    // 10 cats of 105 nouns (9.5%), with dialogue, an em dash, an ellipsis, a curly apostrophe
    // and a line separator.
    let text = cats_and_nouns(
        "\u{201c}He\u{2019}d seen the cat\u{2014}the cat\u{2026}\u{201d}\u{2028}She said so.",
        95,
    );
    let cats = |rules: &Rules| {
        let flags = too_often_used(rules, "TOO_OFTEN_USED_NOUN_EN", &text);
        assert!(flags.iter().all(|(_, matched)| matched == "cat"));
        flags.len()
    };
    assert_eq!(cats(&with_percent(9)), 10);
    assert_eq!(cats(&with_percent(10)), 0);
}

#[test]
fn too_often_used_min_percent_is_per_rule_and_checked() {
    let text = cats_and_nouns("I saw the cat.", 95);
    let mut rules = WORD_STATISTICS_RULES.clone();
    for rule in rules.builtins_mut() {
        if rule.id() == "TOO_OFTEN_USED_NOUN_EN" {
            match rule.builtin_mut() {
                Builtin::TooOftenUsed(rule) => {
                    assert_eq!(rule.min_percent(), 5);
                    rule.set_min_percent(6).unwrap();
                    assert_eq!(rule.min_percent(), 6);
                }
                other => panic!("unexpected builtin: {:?}", other),
            }
        }
    }
    assert_eq!(too_often_used_nouns(&rules, &text), Vec::<usize>::new());
    assert_eq!(
        too_often_used(&rules, "TOO_OFTEN_USED_VERB_EN", &text).len(),
        100
    );

    for percent in [0, 101] {
        assert!(matches!(
            rules.set_too_often_used_percent(percent),
            Err(Error::OptionOutOfRange { value, min: 1, max: 100, .. }) if value == percent
        ));
    }
    let mut percents: Vec<(&str, usize)> = rules
        .builtins()
        .iter()
        .filter_map(|rule| match rule.builtin() {
            Builtin::TooOftenUsed(builtin) => Some((rule.id(), builtin.min_percent())),
            _ => None,
        })
        .collect();
    percents.sort_unstable();
    assert_eq!(
        percents,
        vec![
            ("TOO_OFTEN_USED_ADJECTIVE_EN", 5),
            ("TOO_OFTEN_USED_NOUN_EN", 6),
            ("TOO_OFTEN_USED_VERB_EN", 5)
        ]
    );
}

#[test]
fn too_often_used_gives_way_to_overlapping_suggestions() {
    let mut rules = WORD_STATISTICS_RULES.clone();
    for rule in rules.builtins_mut() {
        if rule.id() == "ENGLISH_WORD_REPEAT_RULE" {
            rule.enable();
        }
    }
    let text = cats_and_nouns("I saw the cat.", 95).replacen("the cat.", "the cat cat.", 1);
    let flags = word_statistics_flags(&rules, &text);

    assert!(flags
        .iter()
        .any(|(source, start, ..)| source == "ENGLISH_WORD_REPEAT_RULE" && *start == 10));
    assert!(flags
        .iter()
        .all(|(source, start, ..)| source != "TOO_OFTEN_USED_NOUN_EN" || *start > 17));
}

#[test]
fn too_often_used_verb_counts_across_line_separators_and_apostrophes() {
    let verbs = |text: &str| too_often_used(&WORD_STATISTICS_RULES, "TOO_OFTEN_USED_VERB_EN", text);

    let flags = verbs(&cats_and_nouns_joined("I saw the cat.", 95, "\u{2028}"));
    assert_eq!(flags.len(), 100);
    assert!(flags.iter().all(|(_, matched)| matched == "saw"));
    assert_eq!(
        flags[..3]
            .iter()
            .map(|(start, _)| *start)
            .collect::<Vec<_>>(),
        vec![2, 17, 32]
    );

    let flags = verbs(&cats_and_nouns(
        "He\u{2019}s seen the cat\u{2019}s cat.",
        95,
    ));
    assert_eq!(flags.len(), 100);
    assert_eq!(flags[0], (5, "seen".to_owned()));
    assert_eq!(flags[5], (127, "saw".to_owned()));
}

#[test]
fn too_often_used_direct_speech_handles_straight_quotes_and_line_separators() {
    let mut rules = WORD_STATISTICS_RULES.clone();
    for rule in rules.builtins_mut() {
        if rule.id().starts_with("TOO_OFTEN_USED_") {
            assert!(rule.disable_case("DIRECT_SPEECH"));
        }
    }
    let said = |text: &str| -> Vec<usize> {
        too_often_used(&rules, "TOO_OFTEN_USED_VERB_EN", text)
            .into_iter()
            .filter(|(_, matched)| matched == "said")
            .map(|(start, _)| start)
            .collect()
    };
    let saw = |text: &str| {
        too_often_used(&rules, "TOO_OFTEN_USED_VERB_EN", text)
            .into_iter()
            .filter(|(_, matched)| matched == "saw")
            .count()
    };

    let text = cats_and_nouns("\"I saw the cat,\" she said.", 95);
    assert_eq!(said(&text), vec![21, 48, 75, 102, 129]);
    assert_eq!(saw(&text), 95);

    let text = cats_and_nouns("She said \"I saw the cat.\"", 95);
    assert_eq!(said(&text), vec![4, 30, 56, 82, 108]);
    assert_eq!(saw(&text), 95);

    let text = cats_and_nouns_joined("\u{201c}I saw the cat,\u{201d} she said.", 95, "\u{2028}");
    assert_eq!(said(&text), vec![21, 48, 75, 102, 129]);
    assert_eq!(saw(&text), 95);
}

#[test]
fn word_statistics_builtins_check_text_as_a_whole() {
    let builtin = |id: &str| {
        RULES
            .builtins()
            .iter()
            .find(|rule| rule.id() == id)
            .unwrap()
            .builtin()
    };
    let starts = |id: &str, text: &str| -> Vec<usize> {
        let sentences: Vec<_> = TOKENIZER.pipe(text).collect();
        builtin(id)
            .check_text(&sentences, RULES.spell().speller().as_ref())
            .iter()
            .map(|suggestion| suggestion.span().char().start)
            .collect()
    };

    assert_eq!(
        starts(
            "TOO_OFTEN_USED_NOUN_EN",
            &cats_and_nouns("I saw the cat.", 95)
        ),
        vec![10, 25, 40, 55, 70]
    );
    assert_eq!(
        starts(
            "EN_REPEATEDWORDS",
            "He definitely ran. She definitely left."
        ),
        vec![23]
    );
    assert_eq!(
        starts(
            "EN_REPEATEDWORDS",
            "He definitely ran. She definitely left. "
        ),
        Vec::<usize>::new()
    );
}

/// `unmatched_selectors` reports unknown selectors without changing any rule, and accepts the
/// builtin case form only when asked to.
#[test]
fn unmatched_selectors_checks_without_changing_rules() {
    let rules = Rules::new(RULES_PATH).unwrap();
    let selectors = [
        "STYLE/SERIAL_COMMA_ON",
        "BUILTIN/WHITESPACE_RULE",
        "BUILTIN/WHITESPACE_RULE/INDENTATION",
        "BUILTIN/WHITESPACE_RULE/NO_SUCH_CASE",
        "NO_SUCH_CATEGORY",
    ];
    assert_eq!(
        rules.unmatched_selectors(&selectors, true),
        ["BUILTIN/WHITESPACE_RULE/NO_SUCH_CASE", "NO_SUCH_CATEGORY"]
    );
    assert_eq!(
        rules.unmatched_selectors(&selectors, false),
        [
            "BUILTIN/WHITESPACE_RULE/INDENTATION",
            "BUILTIN/WHITESPACE_RULE/NO_SUCH_CASE",
            "NO_SUCH_CATEGORY"
        ]
    );
    let serial = Selector::try_from("STYLE/SERIAL_COMMA_ON").unwrap();
    assert!(rules.select(&serial).all(|rule| !rule.enabled()));
}
