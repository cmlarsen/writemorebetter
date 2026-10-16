use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::echoes::{self, EchoOptions, EchoReport};
use writemorebetter_style::reports::repeated_phrases::{
    self, RepeatedPhraseOptions, RepeatedPhraseReport,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Span, StyleError};

fn echoes(document: &Document, options: &EchoOptions) -> EchoReport {
    echoes::report(document, options).unwrap()
}

fn repeated_phrases(document: &Document, options: &RepeatedPhraseOptions) -> RepeatedPhraseReport {
    repeated_phrases::report(document, options).unwrap()
}

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn document(text: &str, names: &[&str]) -> Document {
    let options =
        DocumentOptions::new().with_skip_names(names.iter().map(|name| name.to_string()).collect());
    analyze_document(tokenizer(), text, &options)
}

fn slice(text: &str, span: &Span) -> String {
    let byte = text.get(span.byte.clone()).unwrap().to_owned();
    let chars: String = text
        .chars()
        .skip(span.char.start)
        .take(span.char.len())
        .collect();
    let utf16 = String::from_utf16(
        &text
            .encode_utf16()
            .skip(span.utf16.start)
            .take(span.utf16.len())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(byte, chars);
    assert_eq!(byte, utf16);
    byte
}

fn echo_pairs(text: &str, report: &EchoReport) -> Vec<(String, String, String)> {
    report
        .echoes
        .iter()
        .map(|echo| {
            (
                echo.label.clone(),
                slice(text, &echo.earlier),
                slice(text, &echo.later),
            )
        })
        .collect()
}

fn echo_text(text: &str, names: &[&str], options: &EchoOptions) -> Vec<(String, String, String)> {
    echo_pairs(text, &echoes(&document(text, names), options))
}

fn triple(lemma: &str, earlier: &str, later: &str) -> (String, String, String) {
    (lemma.into(), earlier.into(), later.into())
}

fn phrase_texts(text: &str, report: &RepeatedPhraseReport) -> Vec<(String, Vec<String>)> {
    report
        .phrases
        .iter()
        .map(|phrase| {
            assert_eq!(phrase.count, phrase.spans.len());
            (
                phrase.labels.join(" "),
                phrase.spans.iter().map(|span| slice(text, span)).collect(),
            )
        })
        .collect()
}

fn phrases_with(
    text: &str,
    names: &[&str],
    options: &RepeatedPhraseOptions,
) -> Vec<(String, Vec<String>)> {
    phrase_texts(text, &repeated_phrases(&document(text, names), options))
}

fn phrases_of(text: &str, names: &[&str]) -> Vec<(String, Vec<String>)> {
    phrases_with(text, names, &RepeatedPhraseOptions::new())
}

#[test]
fn echoes_match_by_lemma_and_report_both_spans() {
    let text = "She walked to the window. Later he was walking home.";
    let report = echoes(&document(text, &[]), &EchoOptions::new());
    assert_eq!(
        echo_pairs(text, &report),
        vec![triple("walk", "walked", "walking")]
    );
    assert_eq!(report.echoes[0].distance, 7);
    assert_eq!(report.words, 10);
    assert_eq!(report.per_thousand_words, 100.0);
}

#[test]
fn exact_form_matching_is_optional() {
    let text = "She walked to the window. He was walking. They walked.";
    let options = EchoOptions::new().with_match_forms(false);
    assert_eq!(
        echo_text(text, &[], &options),
        vec![triple("walked", "walked", "walked")]
    );
    assert_eq!(
        echo_text(text, &[], &EchoOptions::new()),
        vec![
            triple("walk", "walked", "walking"),
            triple("walk", "walking", "walked")
        ]
    );
}

#[test]
fn window_is_counted_in_words_and_inclusive() {
    let at = |fillers: usize| format!("The lantern. {}The lantern.", "It was. ".repeat(fillers));
    let echoes_at = |fillers: usize, options: &EchoOptions| {
        echoes(&document(&at(fillers), &[]), options).echoes
    };
    let default = EchoOptions::new();
    assert_eq!(echoes_at(14, &default)[0].distance, 30);
    assert!(echoes_at(15, &default).is_empty());
    let narrow = EchoOptions::new().with_window(10);
    assert_eq!(echoes_at(4, &narrow)[0].distance, 10);
    assert!(echoes_at(5, &narrow).is_empty());
    assert!(echoes_at(0, &EchoOptions::new().with_window(0)).is_empty());
}

#[test]
fn windows_cross_sentence_and_paragraph_boundaries() {
    let text = "The lantern swung.\nShe raised the lantern.";
    assert_eq!(
        echo_text(text, &[], &EchoOptions::new()),
        vec![triple("lantern", "lantern", "lantern")]
    );
}

#[test]
fn function_words_and_auxiliaries_do_not_echo() {
    let text = "She was not there, and she was not here. It is his, and it was his. \
                The one he had was the one she had. They would do it, and they did.";
    assert!(echo_text(text, &[], &EchoOptions::new()).is_empty());
    assert!(
        repeated_phrases(&document(text, &[]), &RepeatedPhraseOptions::new())
            .phrases
            .is_empty()
    );
}

#[test]
fn each_echo_links_to_the_nearest_earlier_occurrence() {
    let text = "Rain fell. Rain fell harder. Rain.";
    let report = echoes(&document(text, &[]), &EchoOptions::new());
    let distances: Vec<_> = report.echoes.iter().map(|echo| echo.distance).collect();
    assert_eq!(distances, vec![2, 2, 3]);
    assert_eq!(
        echo_pairs(text, &report),
        vec![
            triple("rain", "Rain", "Rain"),
            triple("fell", "fell", "fell"),
            triple("rain", "Rain", "Rain"),
        ]
    );
    assert_eq!(report.echoes[0].later, report.echoes[2].earlier);
}

#[test]
fn an_ambiguous_token_echoes_once_and_never_itself() {
    let text = "The light in the room lit her face.";
    let document = document(text, &[]);
    let lit = document.paragraphs[0].sentences[0]
        .tokens
        .iter()
        .find(|token| token.text == "lit")
        .unwrap();
    let mut lemmas: Vec<_> = lit.readings.iter().map(|reading| &reading.lemma).collect();
    lemmas.dedup();
    assert!(lemmas.len() > 1, "fixture needs an ambiguous token");
    let report = echoes(&document, &EchoOptions::new());
    assert_eq!(echo_pairs(text, &report), vec![]);
    for alone in ["It lit.", "The better.", "She left."] {
        assert!(echo_text(alone, &[], &EchoOptions::new()).is_empty());
    }
}

#[test]
fn skipped_names_neither_echo_nor_count() {
    let text = "Mara waited. She saw Mara’s reflection.";
    let skipped = echoes(&document(text, &["Mara"]), &EchoOptions::new());
    assert!(skipped.echoes.is_empty());
    assert_eq!(skipped.words, 4);
    assert_eq!(
        echo_text(text, &[], &EchoOptions::new()),
        vec![triple("mara", "Mara", "Mara")]
    );
}

#[test]
fn echo_spans_survive_fiction_typography() {
    let text = "“The lantern’s glow—” she said… “Lanterns!”\u{2028}The lantern’s gone.";
    assert_eq!(
        echo_text(text, &[], &EchoOptions::new()),
        vec![
            triple("lantern", "lantern", "Lanterns"),
            triple("lantern", "Lanterns", "lantern"),
        ]
    );
    let text = "“I can’t—I won’t…” She wouldn’t.\u{2028}“Don’t.”";
    assert!(echo_text(text, &[], &EchoOptions::new()).is_empty());
}

#[test]
fn empty_document_has_zero_density() {
    let report = echoes(&document("", &[]), &EchoOptions::new());
    assert!(report.echoes.is_empty());
    assert_eq!(report.words, 0);
    assert_eq!(report.per_thousand_words, 0.0);
    assert!(
        repeated_phrases(&document("…", &[]), &RepeatedPhraseOptions::new())
            .phrases
            .is_empty()
    );
}

#[test]
fn repeated_phrases_stop_at_internal_punctuation() {
    assert!(phrases_of(
        "He turned to the window. She turns, to the window. Nothing else.",
        &[]
    )
    .is_empty());
}

#[test]
fn shorter_phrases_need_a_clear_frequency_advantage() {
    let text = "The old grey house stood. The old grey house fell. A grey house remained.";
    assert_eq!(
        phrases_of(text, &[]),
        vec![(
            "the old grey house".into(),
            vec!["The old grey house".into(), "The old grey house".into()]
        ),]
    );
}

#[test]
fn overlapping_phrases_merge_and_stay_inside_sentences() {
    let text =
        "We sailed across the cold dark sea at night. We sailed across the cold dark sea at dawn.";
    let phrases = phrases_of(text, &[]);
    let lengths: Vec<_> = phrases
        .iter()
        .map(|(lemmas, _)| lemmas.split(' ').count())
        .collect();
    assert_eq!(lengths, vec![8]);
    assert_eq!(
        phrases
            .iter()
            .map(|(lemmas, _)| lemmas.as_str())
            .collect::<Vec<_>>(),
        vec!["we sail across the cold dark sea at"]
    );
    let text = "The ship sank. Fast. The ship. Sank fast.";
    assert_eq!(
        phrases_with(
            text,
            &[],
            &RepeatedPhraseOptions::new().with_minimum_content_words(1)
        ),
        vec![(
            "the ship".into(),
            vec!["The ship".into(), "The ship".into()]
        )]
    );
}

#[test]
fn stopword_only_ngrams_are_dropped() {
    assert!(phrases_of("It was in the morning. It was in the night.", &[]).is_empty());
    let text = "It was in the end of the day. It was in the end of the night.";
    assert!(phrases_of(text, &[]).is_empty());
    let phrases = phrases_with(
        text,
        &[],
        &RepeatedPhraseOptions::new().with_minimum_content_words(1),
    );
    assert_eq!(
        phrases,
        vec![(
            "it was in the end of the".into(),
            vec![
                "It was in the end of the".into(),
                "It was in the end of the".into()
            ]
        )]
    );
}

#[test]
fn skipped_names_are_masked_inside_phrases() {
    let text = "Then Mara walked home. Then Jon walked home.";
    assert_eq!(
        phrases_of(text, &["Mara", "Jon"]),
        vec![(
            "then <name> walk home".into(),
            vec![
                "Then Mara walked home".into(),
                "Then Jon walked home".into()
            ]
        )]
    );
}

#[test]
fn phrase_spans_survive_fiction_typography() {
    let text = "“The brass gate’s hinge—” she said… “the brass gate’s hinge!”\u{2028}Then silence.";
    assert_eq!(
        phrases_of(text, &[]),
        vec![(
            "the brass gate 's hinge".into(),
            vec![
                "The brass gate’s hinge".into(),
                "the brass gate’s hinge".into()
            ]
        )]
    );
}

#[test]
fn one_spelling_keeps_one_key_when_context_changes_its_readings() {
    let text = "I thought so. The thought was his. A crooked fence. The crooked, old fence.";
    let document = document(text, &[]);
    let lemma_sets: Vec<Vec<String>> = document
        .paragraphs
        .iter()
        .flat_map(|paragraph| &paragraph.sentences)
        .flat_map(|sentence| &sentence.tokens)
        .filter(|token| token.text == "thought" || token.text == "crooked")
        .map(|token| {
            let mut lemmas: Vec<_> = token.readings.iter().map(|r| r.lemma.clone()).collect();
            lemmas.sort();
            lemmas.dedup();
            lemmas
        })
        .collect();
    assert!(
        lemma_sets[0] != lemma_sets[1] || lemma_sets[2] != lemma_sets[3],
        "fixture needs context-dependent readings: {lemma_sets:?}"
    );
    let pairs = echo_pairs(text, &echoes(&document, &EchoOptions::new()));
    assert!(pairs.contains(&triple("thought", "thought", "thought")));
    assert!(pairs
        .iter()
        .any(|(_, earlier, later)| earlier == "crooked" && later == "crooked"));
}

#[test]
fn phrases_need_two_content_words_by_default() {
    let text = "The door opened. She shut the door. An open door, an open door.";
    assert_eq!(
        phrases_of(text, &[]),
        vec![(
            "an open door".into(),
            vec!["An open door".into(), "an open door".into()]
        )]
    );
    let issue_definition = RepeatedPhraseOptions::new().with_minimum_content_words(1);
    let phrases = phrases_with(text, &[], &issue_definition);
    assert!(phrases.contains(&(
        "the door".into(),
        vec!["The door".into(), "the door".into()]
    )));
    let lengths = RepeatedPhraseOptions::new().with_length(2, 2);
    let text = "The old grey house stood. The old grey house fell.";
    assert_eq!(
        phrases_with(text, &[], &lengths),
        vec![(
            "old grey house".into(),
            vec!["old grey house".into(), "old grey house".into()]
        )]
    );
    assert!(phrases_with(text, &[], &RepeatedPhraseOptions::new().with_length(3, 2)).is_empty());
}

#[test]
fn noun_plurals_echo_even_when_the_singular_is_also_a_verb_form() {
    let cases = [
        (
            "I thought so. A thought struck him. His thoughts wandered.",
            "thought",
            "thoughts",
            "thought",
        ),
        (
            "The wound bled. Her wounds healed.",
            "wound",
            "wounds",
            "wound",
        ),
    ];
    for (text, earlier, later, key) in cases {
        let pairs = echo_text(text, &[], &EchoOptions::new());
        assert!(
            pairs.contains(&triple(key, earlier, later)),
            "{text}: {pairs:?}"
        );
    }
}

#[test]
fn frequent_content_words_with_stray_function_readings_still_echo() {
    for (text, word) in [
        ("I need water. You need rest.", "need"),
        ("He will come back. They come and go.", "come"),
        ("The last train left. The last bus.", "last"),
    ] {
        let pairs = echo_text(text, &[], &EchoOptions::new());
        assert!(
            pairs
                .iter()
                .any(|(_, earlier, later)| earlier.to_lowercase() == word && later == word),
            "{text}: {pairs:?}"
        );
    }
}

#[test]
fn one_spelling_gets_one_function_word_decision_per_document() {
    let text = "The heads turned. Two heads nodded.";
    let pairs = echo_text(text, &[], &EchoOptions::new());
    assert!(
        pairs.contains(&triple("head", "heads", "heads")),
        "{pairs:?}"
    );
    for text in [
        "Two men came. Two boats sailed.",
        "Through the door, through the window.",
        "Nothing moved. Nothing at all.",
    ] {
        let pairs = echo_text(text, &[], &EchoOptions::new());
        assert!(
            !pairs
                .iter()
                .any(|(key, ..)| ["two", "through", "nothing"].contains(&key.as_str())),
            "{text}: {pairs:?}"
        );
    }
}

#[test]
fn words_with_a_listed_lemma_reading_still_echo_when_their_content_reading_is_unlisted() {
    for (text, word) in [
        (
            "The evening was cold. Every evening she walked. That evening he slept.",
            "evening",
        ),
        (
            "She was willing. He was willing too. They were willing.",
            "willing",
        ),
    ] {
        let pairs = echo_text(text, &[], &EchoOptions::new());
        assert_eq!(
            pairs
                .iter()
                .filter(|(_, earlier, later)| earlier.to_lowercase() == word && later == word)
                .count(),
            2,
            "{text}: {pairs:?}"
        );
    }
    for text in ["He’d, she’d, we’d.", "Being late, being tired."] {
        assert!(echo_text(text, &[], &EchoOptions::new()).is_empty());
    }
}

#[test]
fn oversized_phrase_lengths_are_clamped_to_the_longest_sentence() {
    let text = "The old grey house stood. The old grey house stood.";
    let default = phrases_of(text, &[]);
    for max_length in [6, 1_000_000, usize::MAX] {
        let options = RepeatedPhraseOptions::new().with_length(2, max_length);
        assert_eq!(phrases_with(text, &[], &options), default);
    }
    let options = RepeatedPhraseOptions::new().with_length(usize::MAX, usize::MAX);
    assert!(phrases_with(text, &[], &options).is_empty());
}

#[test]
fn keys_name_the_family_and_labels_name_the_matched_words() {
    let text = "He wound the clock. The wind rose. Her wounds healed.";
    let report = echoes(&document(text, &[]), &EchoOptions::new());
    assert!(report.echoes.is_empty(), "{:?}", report.echoes);
    assert!(!report
        .echoes
        .iter()
        .any(|echo| slice(text, &echo.earlier) == "wind" && slice(text, &echo.later) == "wounds"));
    for echo in &report.echoes {
        let earlier = slice(text, &echo.earlier).to_lowercase();
        let later = slice(text, &echo.later).to_lowercase();
        assert!(
            echo.label == earlier
                || echo.label == later
                || ["wind", "wound"].contains(&echo.label.as_str()),
            "{echo:?}"
        );
    }

    let text = "It was good. He did well. It was better. It was good.";
    let document = document(text, &[]);
    let report = echoes(&document, &EchoOptions::new());
    for echo in &report.echoes {
        let earlier = slice(text, &echo.earlier).to_lowercase();
        let later = slice(text, &echo.later).to_lowercase();
        if earlier != later {
            assert!(
                echo.label == earlier || echo.label == later,
                "label must name a matched word: {echo:?}"
            );
        }
    }
    let phrases = repeated_phrases(
        &document,
        &RepeatedPhraseOptions::new().with_minimum_content_words(1),
    );
    for phrase in &phrases.phrases {
        assert_eq!(phrase.keys.len(), phrase.labels.len());
    }
}

#[test]
fn compact_documents_are_rejected_instead_of_degrading() {
    let compact = analyze_document(
        tokenizer(),
        "The lamp lit the lamp.",
        &DocumentOptions::new().with_token_details(false),
    );
    assert_eq!(
        echoes::report(&compact, &EchoOptions::new()),
        Err(StyleError::NeedsTokenDetails {
            report: "echoes".into()
        })
    );
    assert_eq!(
        repeated_phrases::report(&compact, &RepeatedPhraseOptions::new()),
        Err(StyleError::NeedsTokenDetails {
            report: "repeatedPhrases".into()
        })
    );
}
