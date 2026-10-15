use nlprule::Tokenizer;
use std::sync::OnceLock;
use writemorebetter_style::reports::sentences::{self, Context, Options, Start};
use writemorebetter_style::{analyze_document, Document};

fn doc(paragraphs: &[&str]) -> Document {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    analyze_document(
        TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap()),
        paragraphs,
    )
}

#[test]
fn hand_labeled_sentence_openings_five_per_category() {
    let cases = [
        (
            Start::Subject,
            [
                "She closed the door.",
                "The old dog slept.",
                "Mara waited outside.",
                "A cold wind blew.",
                "“They found the key,” he said.",
            ],
        ),
        (
            Start::Adverb,
            [
                "Quietly, she closed the door.",
                "Slowly, he turned around.",
                "Carefully, Mara opened the box.",
                "Suddenly, the lights went out.",
                "“Perhaps we should leave,” she said.",
            ],
        ),
        (
            Start::Gerund,
            [
                "Running through the rain, she slipped.",
                "Walking home, he found a coin.",
                "Looking back, Mara smiled.",
                "Waiting outside, she grew cold.",
                "“Hoping for rain, I stayed,” he said.",
            ],
        ),
        (
            Start::Infinitive,
            [
                "To escape, she climbed the wall.",
                "To find the key, he searched the room.",
                "To stay warm, Mara lit a fire.",
                "To understand, she listened carefully.",
                "“To win, we must work together,” she said.",
            ],
        ),
        (
            Start::SubordinatingConjunction,
            [
                "Although it rained, she went outside.",
                "Because he was tired, he stopped.",
                "If Mara calls, tell her to wait.",
                "Unless the rain stops, we will stay.",
                "“While you sleep, I will watch,” he said.",
            ],
        ),
        (
            Start::CoordinatingConjunction,
            [
                "And she opened the door.",
                "But he stayed behind.",
                "Yet Mara refused to leave.",
                "Or we could wait here.",
                "“So we waited,” she said.",
            ],
        ),
        (
            Start::PastParticiple,
            [
                "Exhausted by the journey, she slept.",
                "Trapped in the cellar, he waited.",
                "Surrounded by wolves, Mara froze.",
                "Shaken by the news, she sat down.",
                "“Hidden behind the wall, I waited,” he said.",
            ],
        ),
        (
            Start::Inversion,
            [
                "Did she close the door?",
                "Was he waiting outside?",
                "Can Mara find the key?",
                "Had they left already?",
                "“Will you stay?” she asked.",
            ],
        ),
        (
            Start::Introductory,
            [
                "However, she refused to leave.",
                "For example, he could wait outside.",
                "In fact, Mara knew the answer.",
                "As a result, the door stayed shut.",
                "“Of course, I will stay,” she said.",
            ],
        ),
    ];
    let mut failures = Vec::new();
    for (expected, texts) in cases {
        for text in texts {
            let document = doc(&[text]);
            let sentence = &document.paragraphs[0].sentences[0];
            let actual = sentences::classify(sentence);
            if actual != expected {
                failures.push(format!(
                    "{text}: {actual:?}, expected {expected:?}; {:?}",
                    sentence
                        .tokens
                        .iter()
                        .filter(|t| !t.is_whitespace)
                        .take(3)
                        .map(|t| (&t.text, &t.readings))
                        .collect::<Vec<_>>()
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn histogram_contractions_unicode_and_context_counts() {
    let document = doc(&[
        "😀 “I’m goin’—home…” she said.\u{2028}",
        "The well-known dog slept.",
    ]);
    let report = sentences::report(&document, &Options::default()).unwrap();
    assert_eq!(report.sentences[0].words, 5); // I'm / goin' / home / she / said
    assert_eq!(report.sentences[0].dialogue_words, 3);
    assert_eq!(report.sentences[0].narration_words, 2);
    assert!(report.sentences[0].mixed);
    assert_eq!(report.sentences[0].context, Context::Dialogue);
    assert_eq!(report.sentences[1].words, 4);
    assert_eq!(report.lengths.dialogue.histogram[&5].count, 1);
    assert_eq!(report.lengths.narration.histogram[&4].count, 1);
    assert!(report.starts.dialogue.categories[&Start::Subject]
        .distribution
        .share
        .is_none());
}

#[test]
fn shares_and_baselines_use_context_sample_sizes() {
    let document = doc(&["She waited.", "“He stayed.”"]);
    let mut options = Options {
        minimum_sentences: 1,
        ..Options::default()
    };
    options
        .start_baseline
        .narration
        .insert(Start::Subject, 0.75);
    options.start_baseline.dialogue.insert(Start::Subject, 0.75);
    options.length_baseline.narration.insert(2, 0.5);
    options.length_baseline.narration.insert(3, 0.5);
    let report = sentences::report(&document, &options).unwrap();
    for start in [&report.starts.narration, &report.starts.dialogue] {
        let dist = &start.categories[&Start::Subject].distribution;
        assert_eq!(dist.share, Some(1.0));
        assert_eq!(dist.difference, Some(0.25));
    }
    assert_eq!(report.lengths.narration.histogram[&2].difference, Some(0.5));
    assert_eq!(report.lengths.narration.histogram[&3].share, Some(0.0));
    options.minimum_sentences = 2;
    let report = sentences::report(&document, &options).unwrap();
    assert!(report.lengths.narration.histogram[&2].share.is_none());
    assert!(report.starts.dialogue.categories[&Start::Subject]
        .distribution
        .difference
        .is_none());
}

#[test]
fn consecutive_runs_cross_paragraphs_but_stop_at_context_changes() {
    let document = doc(&[
        "She waited outside.",
        "She watched the door.",
        "She held her breath.",
        "She found the key.",
        "“She left.”",
        "She came home.",
    ]);
    let report = sentences::report(&document, &Options::default()).unwrap();
    assert_eq!(report.starts.narration.repeated_opening_runs.len(), 1);
    let span = &report.starts.narration.repeated_opening_runs[0];
    assert_eq!(
        &document.text[span.byte.clone()],
        "She waited outside.\nShe watched the door.\nShe held her breath.\nShe found the key."
    );
    assert_eq!(
        report.lengths.narration.monotonous_runs,
        report.starts.narration.repeated_opening_runs
    );
    assert!(report.lengths.dialogue.monotonous_runs.is_empty());
}

#[test]
fn long_sentence_threshold_and_invalid_options() {
    let document = doc(&["She waited outside the door.", "Leave now."]);
    let options = Options {
        long_sentence_words: 4,
        minimum_sentences: 0,
        ..Options::default()
    };
    let report = sentences::report(&document, &options).unwrap();
    assert_eq!(report.lengths.narration.long_sentences.len(), 1);
    assert_eq!(report.sentences[1].start, Start::Unclassified);
    let empty = sentences::report(&doc(&[]), &options).unwrap();
    assert!(empty.starts.narration.categories[&Start::Subject]
        .distribution
        .share
        .is_none());
    for value in [f64::NAN, -0.1, 1.1] {
        let mut invalid = options.clone();
        invalid
            .start_baseline
            .narration
            .insert(Start::Subject, value);
        assert!(sentences::report(&document, &invalid).is_err());
    }
    let mut invalid = options.clone();
    invalid.start_baseline.narration.insert(Start::Subject, 0.8);
    invalid.start_baseline.narration.insert(Start::Adverb, 0.3);
    assert!(sentences::report(&document, &invalid).is_err());
}
