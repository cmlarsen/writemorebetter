use std::sync::{Arc, OnceLock};

use nlprule::Tokenizer;
use writemorebetter_style::reports::pacing::{self, PacingOptions, PacingSignal};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, StyleDocument, StyleError};

const BACKSTORY: &str = "She remembered the old house and knew that her father had gone away. She had never been happy there.\nHe wondered why she had forgotten the garden. He had always believed that she had understood.\nShe recalled how he had been alone. She had imagined that he had known the truth.";

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report(text: &str) -> pacing::PacingReport {
    pacing::report(
        &analyze_document(tokenizer(), text, &DocumentOptions::new()),
        &PacingOptions::new(),
    )
    .unwrap()
}

#[test]
fn flags_backstory_but_not_action_dialogue_or_short_exposition() {
    let r = report(BACKSTORY);
    assert_eq!(r.slow_runs.len(), 1, "{r:#?}");
    assert_eq!(r.slow_runs[0].paragraphs, 0..3);
    assert_eq!(r.slow_runs[0].span.byte, 0..BACKSTORY.len());
    assert!(r.slow_runs[0]
        .triggered_signals
        .contains(&PacingSignal::PastPerfect));
    assert!(r.slow_runs[0]
        .triggered_signals
        .contains(&PacingSignal::Cognition));
    for text in [
        "She ran. The door slammed.\nHe jumped over the wall.\nThey fled into the trees.",
        "“I remembered that he had gone,” she said.\n“I knew that he had been there,” he replied.\n“I thought he had understood,” she said.",
        "The road led to the village.\nThe houses stood beside the river.",
        &BACKSTORY[..BACKSTORY.find('\n').unwrap()],
    ] {
        assert!(report(text).slow_runs.is_empty(), "{text}");
    }
}

#[test]
fn past_perfect_uses_pos_and_whitespace_bounded_adverbs() {
    let r = report("She had gone. She had never been alone. She'd gone. She’d already gone. She'd go. She'd put it away. She had a dog. She had, gone away.");
    assert_eq!(r.paragraphs[0].signals.past_perfect, 4, "{r:#?}");
    let nouns = report("Her thought was a memory. The wonder of it filled the room.");
    assert_eq!(nouns.paragraphs[0].signals.cognition, 0);
    for (text, verb_phrases, past_perfect) in [
        ("She had still not forgotten him.", 1, 1),
        ("She had back then known it.", 1, 1),
        ("She had still water.", 1, 0),
        ("He had back pain.", 1, 0),
        ("He had better go.", 1, 0),
        ("And the garden had been dug up.", 1, 1),
        ("She’d come home.", 1, 0),
    ] {
        let signals = &report(text).paragraphs[0].signals;
        assert_eq!(
            (signals.verb_phrases, signals.past_perfect),
            (verb_phrases, past_perfect),
            "{text}"
        );
    }
}

#[test]
fn appending_qualifying_paragraphs_keeps_a_run() {
    let a = "She had gone. He had left. They knew it. It was over.";
    let b = "She thought about the river and remembered the boats that drifted past the old mill, and she wondered whether he knew how often she walked there, sat by the water, watched the birds and understood the people who lived and worked along its banks.";
    for (count, expected) in [(0, 0..3), (1, 0..4), (2, 0..5), (3, 0..5)] {
        let text = [vec![a; 3], vec![b; count]].concat().join("\n");
        let r = report(&text);
        assert_eq!(r.paragraphs.len(), 3 + count);
        assert_eq!(r.slow_runs.len(), 1, "{count}");
        assert_eq!(r.slow_runs[0].paragraphs, expected);
    }
    let r = report(&[vec![a; 3], vec![b; 3]].concat().join("\n"));
    let sum = |f: fn(&pacing::PacingParagraph) -> usize| r.paragraphs.iter().map(f).sum::<usize>();
    let verbs = sum(|p| p.signals.verb_phrases) as f64;
    assert!(100.0 * (sum(|p| p.signals.past_perfect) as f64) / verbs < 15.0);
    assert!(sum(|p| p.signals.words.all) as f64 / (sum(|p| p.signals.sentences) as f64) < 18.0);
}

#[test]
fn scenes_headings_dialogue_and_nonqualifying_paragraphs_end_runs() {
    let parts: Vec<_> = BACKSTORY.split('\n').collect();
    for separator in [
        "\n\n\n",
        "\n***\n",
        "\nChapter 2\n",
        "\nShe ran.\n",
        "\n“Go!”\n",
    ] {
        let text = format!("{}\n{}{}{}", parts[0], parts[1], separator, parts[2]);
        assert!(report(&text).slow_runs.is_empty(), "{separator:?}");
    }
    let text = format!("{BACKSTORY}\n\n\n{BACKSTORY}");
    let r = report(&text);
    assert_eq!(r.slow_runs.len(), 2);
    assert_eq!(r.slow_runs[1].paragraphs, 3..6);
}

#[test]
fn word_cutoff_is_strict_and_options_are_validated() {
    let four_hundred = format!("{} She.", vec!["She had remembered."; 133].join(" "));
    assert_eq!(report(&four_hundred).words.all, 400);
    assert!(report(&four_hundred).slow_runs.is_empty());
    assert_eq!(report(&(four_hundred + " She.")).slow_runs.len(), 1);
    let text = BACKSTORY.lines().next().unwrap();
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let words = document.word_count;
    for (threshold, expected) in [(words, 0), (words - 1, 1)] {
        let r = pacing::report(
            &document,
            &PacingOptions::new().with_more_than_words(threshold),
        )
        .unwrap();
        assert_eq!(r.slow_runs.len(), expected);
    }
    for options in [
        PacingOptions::new().with_minimum_paragraphs(0),
        PacingOptions::new().with_minimum_sentence_length(f64::NAN),
        PacingOptions::new().with_minimum_past_perfect_percent(101.0),
        PacingOptions::new().with_minimum_cognition_percent(-1.0),
    ] {
        assert!(pacing::report(&document, &options).is_err());
    }
}

#[test]
fn utf8_hard_wrap_names_and_retained_json() {
    let text = format!(
        "😀 She’d remembered—she had never been alone…\u{2028}She wondered why.\n{}",
        BACKSTORY
    );
    let document = analyze_document(
        tokenizer(),
        &text,
        &DocumentOptions::new().with_skip_names(vec!["She".into()]),
    );
    let r = pacing::report(&document, &PacingOptions::new()).unwrap();
    assert_eq!(r.words.all, document.word_count);
    for row in &r.paragraphs {
        let surface = text.get(row.span.byte.clone()).unwrap();
        assert_eq!(surface.encode_utf16().count(), row.span.utf16.len());
        assert_eq!(surface.chars().count(), row.span.char.len());
    }
    let wrapped = BACKSTORY.replace(". ", ".\n").replace('\n', "\r\n");
    let wrapped = wrapped
        .replace("\r\nHe wondered", "\r\n\r\nHe wondered")
        .replace("\r\nShe recalled", "\r\n\r\nShe recalled");
    let hard = analyze_document(
        tokenizer(),
        &wrapped,
        &DocumentOptions::new().with_hard_wrapped(true),
    );
    let hard_r = pacing::report(&hard, &PacingOptions::new()).unwrap();
    assert_eq!(hard_r.paragraphs.len(), 3);
    assert_eq!(hard_r.slow_runs.len(), 1);
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        &text,
        &DocumentOptions::new(),
    );
    let json: serde_json::Value =
        serde_json::from_str(&to_utf16_json(&retained.pacing(None).unwrap())).unwrap();
    assert_eq!(json["paragraphs"][0]["span"]["start"], 0);
    assert!(retained
        .pacing(Some("{\"minimum_cognition_percent\":10}"))
        .is_ok());
    assert!(retained.pacing(Some("{\"typo\":10}")).is_err());
    let compact = analyze_document(
        tokenizer(),
        &text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        pacing::report(&compact, &PacingOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    assert!(report("").paragraphs.is_empty());
}
