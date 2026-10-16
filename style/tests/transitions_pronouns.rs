use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::{
    pronoun_starts::{self, PronounStartsOptions},
    transitions::{self, TransitionType, TransitionsOptions},
    StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}
fn document(text: &str) -> writemorebetter_style::Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}
#[test]
fn labeled_transitions() {
    for (text, phrase, kind) in [
        ("However, she stayed.", "however", TransitionType::Contrast),
        (
            "On the other hand, he stayed.",
            "on the other hand",
            TransitionType::Contrast,
        ),
        (
            "As a result, we left.",
            "as a result",
            TransitionType::Cause,
        ),
        (
            "She left because it rained.",
            "because",
            TransitionType::Cause,
        ),
        (
            "What’s more, she paid.",
            "what's more",
            TransitionType::Addition,
        ),
        (
            "What's more, she paid.",
            "what's more",
            TransitionType::Addition,
        ),
        ("Meanwhile, he waited.", "meanwhile", TransitionType::Time),
        ("Then, he left.", "then", TransitionType::Time),
        (
            "In addition, she paid.",
            "in addition",
            TransitionType::Addition,
        ),
        (
            "For example, birds fly.",
            "for example",
            TransitionType::Example,
        ),
        (
            "She chose, for instance, a blue coat.",
            "for instance",
            TransitionType::Example,
        ),
    ] {
        let result = transitions::report(&document(text), &TransitionsOptions::new()).unwrap();
        assert_eq!(result.matches.len(), 1, "{text}");
        assert_eq!(result.matches[0].phrase, phrase);
        assert_eq!(result.matches[0].kind, kind);
        assert_eq!(
            text.get(result.matches[0].span.byte.clone())
                .unwrap()
                .to_lowercase()
                .replace('’', "'"),
            phrase
        );
    }
    for text in [
        "She was so tired.",
        "So tired was he.",
        "She then left.",
        "Then he left.",
        "However hard she tried, she failed.",
        "He stood besides me.",
        "The cat and dog slept.",
        "She held a result sheet.",
        "She raised her other hand.",
        "They chose an example.",
        "In a sudden addition, he arrived.",
        "The clock showed the time.",
        "For, example birds fly.",
        "As—a result, we left.",
    ] {
        assert!(
            transitions::report(&document(text), &Default::default())
                .unwrap()
                .matches
                .is_empty(),
            "{text}"
        );
    }
}
#[test]
fn pronoun_runs_cross_paragraphs_and_include_dialogue() {
    let text = "She walked home. She slept.\n\nShe woke early. A bird sang.\n“I am here. I can wait. I will stay.”";
    let result = pronoun_starts::report(
        &document(text),
        &PronounStartsOptions::new().with_minimum_sentences(1),
    )
    .unwrap();
    assert_eq!(result.groups.narration.runs.len(), 1);
    assert_eq!(result.groups.narration.runs[0].sentences, 0..3);
    assert_eq!(result.groups.dialogue.runs.len(), 1);
    assert_eq!(result.groups.all.pronoun_starts.count, 6);
    assert_eq!(result.groups.narration.pronoun_starts.percent, Some(75.0));
    assert!(result.groups.all.low_confidence);
    let same = document("She waited. She waited. She waited.");
    assert!(writemorebetter_style::reports::sentence_starts::report(
        &same,
        &Default::default(),
        None
    )
    .unwrap()
    .groups
    .all
    .same_word_runs
    .is_empty());
    assert_eq!(
        pronoun_starts::report(&same, &Default::default())
            .unwrap()
            .groups
            .all
            .runs
            .len(),
        1
    );
}
#[test]
fn utf8_spans_and_repeats() {
    let text = "“AS A RESULT, she’s tired—still… fine.”\u{2028}As a result, she left. As a result, she slept.";
    let doc = document(text);
    let report =
        transitions::report(&doc, &TransitionsOptions::new().with_minimum_sentences(1)).unwrap();
    assert_eq!(report.matches.len(), 3);
    assert_eq!(report.repeats.len(), 1);
    for row in &report.matches {
        let found = text.get(row.span.byte.clone()).unwrap();
        assert_eq!(found.to_lowercase(), "as a result");
        assert_eq!(
            text[..row.span.byte.start].chars().count(),
            row.span.char.start
        );
        assert_eq!(
            text[..row.span.byte.start].encode_utf16().count(),
            row.span.utf16.start
        );
    }
    let pronouns = pronoun_starts::report(
        &document("“She’s tired—still… fine.”\u{2028}She slept. She woke."),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(pronouns.starts.len(), 3);
    assert!(pronouns.groups.all.pronoun_starts.percent.is_none());
}
#[test]
fn windows_and_deduplication() {
    let doc =
        document("Therefore, she left; therefore, he stayed. A bird sang. Therefore, she slept.");
    let report =
        transitions::report(&doc, &TransitionsOptions::new().with_minimum_sentences(1)).unwrap();
    assert_eq!(report.groups.all.containing.count, 2);
    assert_eq!(
        report
            .groups
            .all
            .types
            .iter()
            .find(|t| t.kind == TransitionType::Cause)
            .unwrap()
            .containing
            .count,
        2
    );
    assert_eq!(report.repeats.len(), 2);
    let compact = analyze_document(
        tokenizer(),
        "Therefore, she left; therefore, he stayed. A bird sang. Therefore, she slept.",
        &DocumentOptions::new().with_token_details(false),
    );
    assert_eq!(
        transitions::report(
            &compact,
            &TransitionsOptions::new().with_minimum_sentences(1)
        )
        .unwrap(),
        report
    );
    let close =
        transitions::report(&doc, &TransitionsOptions::new().with_repeat_window(1)).unwrap();
    assert_eq!(close.repeats.len(), 1);
    assert!(transitions::report(&doc, &TransitionsOptions::new().with_repeat_window(0)).is_err());
}
#[test]
fn boundaries_exclusions_and_shared_methods() {
    for text in [
        "She slept. She woke.\n\n***\n\nShe left.",
        "She slept. She woke. He left. She slept.",
    ] {
        assert!(pronoun_starts::report(&document(text), &Default::default())
            .unwrap()
            .groups
            .all
            .runs
            .is_empty());
    }
    let options = DocumentOptions::new().with_skip_names(vec!["She".to_owned()]);
    let doc = analyze_document(tokenizer(), "She slept. She woke. She left.", &options);
    assert_eq!(
        pronoun_starts::report(&doc, &Default::default())
            .unwrap()
            .groups
            .all
            .excluded_sentences,
        3
    );
    let compact = analyze_document(
        tokenizer(),
        "She slept.",
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        pronoun_starts::report(&compact, &Default::default()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    assert_eq!(
        transitions::report(&compact, &Default::default()).unwrap(),
        transitions::report(&document("She slept."), &Default::default()).unwrap()
    );
    let shared = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        "Therefore, she slept.",
        &DocumentOptions::new(),
    );
    assert_eq!(
        shared.transitions(None).unwrap(),
        transitions::report(shared.document(), &Default::default()).unwrap()
    );
    assert!(shared.pronoun_starts(Some("{\"unknown\":1}")).is_err());
    assert!(shared.transitions(Some("{\"unknown\":1}")).is_err());
}

#[test]
fn sample_floors_and_empty_documents() {
    for (count, shown, low) in [
        (0, false, true),
        (49, false, true),
        (50, true, true),
        (199, true, true),
        (200, true, false),
    ] {
        let doc = document(&"She slept. ".repeat(count));
        let result = pronoun_starts::report(&doc, &Default::default()).unwrap();
        assert_eq!(result.groups.all.pronoun_starts.percent.is_some(), shown);
        assert_eq!(result.groups.all.low_confidence, low);
        assert_eq!(result.groups.all.pronoun_starts.count, count);
        let result = transitions::report(&doc, &Default::default()).unwrap();
        assert_eq!(result.groups.all.containing.percent.is_some(), shown);
        assert_eq!(result.groups.all.low_confidence, low);
        assert!(result.groups.dialogue.containing.percent.is_none());
    }
    let doc = document("As\u{2028}a result, she slept.");
    assert_eq!(
        transitions::report(&doc, &Default::default())
            .unwrap()
            .matches
            .len(),
        1
    );
}

#[test]
fn pronouns_distinguish_determiners_and_independent_uses() {
    for text in [
        "Her coat was red.",
        "His coat was blue.",
        "This book was red.",
        "What color was it?",
        "That she left was clear.",
    ] {
        assert!(
            pronoun_starts::report(&document(text), &Default::default())
                .unwrap()
                .starts
                .is_empty(),
            "{text}"
        );
    }
    for (text, word) in [
        ("Hers was blue.", "hers"),
        ("Nobody left.", "nobody"),
        ("Someone came.", "someone"),
        ("This was easy.", "this"),
        ("Who came?", "who"),
        ("What she said was clear.", "what"),
        ("His was blue.", "his"),
    ] {
        let result = pronoun_starts::report(&document(text), &Default::default()).unwrap();
        assert_eq!(result.starts.len(), 1, "{text}");
        assert_eq!(result.starts[0].pronoun, word);
    }
}
