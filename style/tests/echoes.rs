use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::echoes::{echoes, EchoOptions, EchoReport};
use writemorebetter_style::reports::phrases::{repeated_phrases, PhraseReport};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Span};

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
                echo.lemma.clone(),
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

fn phrase_texts(text: &str, report: &PhraseReport) -> Vec<(String, Vec<String>)> {
    report
        .phrases
        .iter()
        .map(|phrase| {
            assert_eq!(phrase.count, phrase.spans.len());
            (
                phrase.lemmas.join(" "),
                phrase.spans.iter().map(|span| slice(text, span)).collect(),
            )
        })
        .collect()
}

fn phrases_of(text: &str, names: &[&str]) -> Vec<(String, Vec<String>)> {
    phrase_texts(text, &repeated_phrases(&document(text, names)))
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
    assert_eq!(report.word_count, 10);
    assert_eq!(report.density_per_1000, 100.0);
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
    let at = |fillers: usize| format!("The lantern {}the lantern.", "it was ".repeat(fillers));
    let echoes_at = |fillers: usize, options: &EchoOptions| {
        echoes(&document(&at(fillers), &[]), options).echoes
    };
    let default = EchoOptions::new();
    assert_eq!(echoes_at(24, &default)[0].distance, 50);
    assert!(echoes_at(25, &default).is_empty());
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
    assert!(repeated_phrases(&document(text, &[])).phrases.is_empty());
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
            triple("fall", "fell", "fell"),
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
    assert_eq!(
        echo_pairs(text, &report),
        vec![triple("light", "light", "lit")]
    );
    for alone in ["It lit.", "The better.", "She left."] {
        assert!(echo_text(alone, &[], &EchoOptions::new()).is_empty());
    }
}

#[test]
fn skipped_names_neither_echo_nor_count() {
    let text = "Mara saw Mara’s reflection.";
    let skipped = echoes(&document(text, &["Mara"]), &EchoOptions::new());
    assert!(skipped.echoes.is_empty());
    assert_eq!(skipped.word_count, 2);
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
    assert_eq!(report.word_count, 0);
    assert_eq!(report.density_per_1000, 0.0);
    assert!(repeated_phrases(&document("…", &[])).phrases.is_empty());
}

#[test]
fn repeated_phrases_match_lemmas_and_ignore_punctuation() {
    let text = "He turned to the window. She turns, to the window. Nothing else.";
    assert_eq!(
        phrases_of(text, &[]),
        vec![(
            "turn to the window".into(),
            vec!["turned to the window".into(), "turns, to the window".into()]
        )]
    );
}

#[test]
fn shorter_phrases_inside_a_longer_repeat_are_dropped_unless_they_also_occur_alone() {
    let text = "The old grey house stood. The old grey house fell. A grey house remained.";
    assert_eq!(
        phrases_of(text, &[]),
        vec![
            (
                "grey house".into(),
                vec![
                    "grey house".into(),
                    "grey house".into(),
                    "grey house".into()
                ]
            ),
            (
                "the old grey house".into(),
                vec!["The old grey house".into(), "The old grey house".into()]
            ),
        ]
    );
}

#[test]
fn phrases_are_capped_at_five_tokens_and_stay_inside_sentences() {
    let text =
        "We sailed across the cold dark sea at night. We sailed across the cold dark sea at dawn.";
    let phrases = phrases_of(text, &[]);
    let lengths: Vec<_> = phrases
        .iter()
        .map(|(lemmas, _)| lemmas.split(' ').count())
        .collect();
    assert!(lengths.iter().all(|length| *length == 5));
    assert_eq!(
        phrases
            .iter()
            .map(|(lemmas, _)| lemmas.as_str())
            .collect::<Vec<_>>(),
        vec![
            "we sail across the cold",
            "sail across the cold dark",
            "across the cold dark sea",
            "the cold dark sea at",
        ]
    );
    let text = "The ship sank. Fast. The ship. Sank fast.";
    assert_eq!(
        phrases_of(text, &[]),
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
    let phrases = phrases_of(text, &[]);
    assert_eq!(
        phrases,
        vec![
            (
                "it was in the end".into(),
                vec!["It was in the end".into(), "It was in the end".into()]
            ),
            (
                "was in the end of".into(),
                vec!["was in the end of".into(), "was in the end of".into()]
            ),
            (
                "in the end of the".into(),
                vec!["in the end of the".into(), "in the end of the".into()]
            ),
        ]
    );
}

#[test]
fn skipped_names_break_phrases() {
    let text = "Then Mara walked home. Then Jon walked home.";
    assert_eq!(
        phrases_of(text, &["Mara", "Jon"]),
        vec![(
            "walk home".into(),
            vec!["walked home".into(), "walked home".into()]
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
    assert!(pairs.contains(&triple("think", "thought", "thought")));
    assert!(pairs
        .iter()
        .any(|(_, earlier, later)| earlier == "crooked" && later == "crooked"));
}
