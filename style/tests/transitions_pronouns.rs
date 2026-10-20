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
        ("Thus, the plan failed.", "thus", TransitionType::Cause),
        ("And thus she left.", "and", TransitionType::Addition),
        ("Besides, nobody saw.", "besides", TransitionType::Addition),
        (
            "He was, however, late.",
            "however",
            TransitionType::Contrast,
        ),
        ("She stayed, however.", "however", TransitionType::Contrast),
        ("Instead, she stayed.", "instead", TransitionType::Contrast),
        (
            "But, besides the money, he had nothing.",
            "but",
            TransitionType::Contrast,
        ),
    ] {
        let result = transitions::report(&document(text), &TransitionsOptions::new()).unwrap();
        let expected = if text.starts_with("And thus") { 2 } else { 1 };
        assert_eq!(result.matches.len(), expected, "{text}");
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
        "She had thus broken it.",
        "He stood thus for an hour.",
        "Thus far nothing has happened.",
        "The idea, however wild, worked.",
        "Well, then, however the captains may grumble, we sail.",
        "He chose tea instead of coffee.",
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
    let text = "She was home. She was tired.\n\nShe was done. A bird sang.\n“I am here. I can wait. I will stay.”";
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
    let reported = document("She walked home. She slept.\n\nShe woke early.");
    assert_eq!(
        writemorebetter_style::reports::sentence_starts::report(
            &reported,
            &Default::default(),
            None
        )
        .unwrap()
        .groups
        .all
        .same_word_runs[0]
            .sentences,
        0..3
    );
    let lens = pronoun_starts::report(&reported, &Default::default()).unwrap();
    assert!(lens.groups.all.runs.is_empty());
    assert_eq!(lens.groups.all.pronoun_starts.count, 3);
}
#[test]
fn dialogue_turns_reset_runs_and_repeats() {
    let options = PronounStartsOptions::new().with_minimum_sentences(1);
    let speakers = document("“She left,” I said.\n\n“She did,” he said.\n\n“She will,” I said.");
    let result = pronoun_starts::report(&speakers, &options).unwrap();
    assert_eq!(result.starts.len(), 3);
    assert!(result.groups.all.runs.is_empty());
    let speech = document("“She left. She cried. She’s gone,” I said.");
    assert_eq!(
        pronoun_starts::report(&speech, &options)
            .unwrap()
            .groups
            .dialogue
            .runs[0]
            .sentences,
        0..3
    );
    let turns =
        document("“But I can’t,” she said.\n\n“But you must,” he said.\n\n“But why?” she asked.");
    let result = transitions::report(&turns, &TransitionsOptions::new()).unwrap();
    assert_eq!(result.matches.len(), 3);
    assert!(result.repeats.is_empty());
    let single = document("“But I can’t. But I won’t.”");
    assert_eq!(
        transitions::report(&single, &TransitionsOptions::new())
            .unwrap()
            .repeats
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
    let names = DocumentOptions::new().with_skip_names(vec!["Mary".to_owned()]);
    let named = analyze_document(
        tokenizer(),
        "Mary, however, stayed. Therefore, she left.",
        &names,
    );
    let result = transitions::report(&named, &Default::default()).unwrap();
    assert_eq!(result.matches.len(), 2);
    assert!(!result.matches[0].opening);
    assert_eq!(result.groups.all.sentences, 2);
    let chain = analyze_document(
        tokenizer(),
        "However, he left. Mary sat. However, he came back.",
        &names,
    );
    assert_eq!(
        transitions::report(&chain, &Default::default())
            .unwrap()
            .repeats
            .len(),
        1
    );
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
        "One evening she left. One day he came. One morning it rained.",
        "What’s more, she paid.",
        "What's more, she paid.",
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
        ("One cannot know.", "one"),
    ] {
        let result = pronoun_starts::report(&document(text), &Default::default()).unwrap();
        assert_eq!(result.starts.len(), 1, "{text}");
        assert_eq!(result.starts[0].pronoun, word);
    }
}

#[test]
fn condition_transitions_and_ambiguity() {
    for (text, phrase) in [
        ("If she waits, I’ll stay.", "if"),
        ("I’ll go unless she stays.", "unless"),
        ("Provided that she agrees, we’ll go.", "provided that"),
        ("As long as he waits, we’ll stay.", "as long as"),
        ("He stayed as long as she watched.", "as long as"),
        ("He left provided that she agreed.", "provided that"),
        ("In case she calls, I’ll stay.", "in case"),
        ("“If she’s ready”—he paused…\u{2028}we’ll go.", "if"),
    ] {
        let result = transitions::report(&document(text), &Default::default()).unwrap();
        let found = result
            .matches
            .iter()
            .find(|item| item.phrase == phrase && item.kind == TransitionType::Condition);
        assert!(found.is_some(), "{text}");
        assert!(text.get(found.unwrap().span.byte.clone()).is_some());
    }
    for text in [
        "While she waited, he slept.",
        "He looked as if he had seen a ghost.",
        "She asked if I was ready.",
        "I wonder if it will rain.",
        "The rope was as long as the mast.",
        "The food provided that night was cold.",
        "“I can’t,” she asked if he’d go—slowly…\u{2028}Maybe.",
    ] {
        let result = transitions::report(&document(text), &Default::default()).unwrap();
        assert!(
            !result
                .matches
                .iter()
                .any(|item| item.kind == TransitionType::Condition),
            "{text}"
        );
    }
}

#[test]
fn pronoun_words_count_possessives_and_split_speech() {
    let text = "“I’ll keep my coat,” she said—her own was gone…\u{2028}He gave his coat to her.";
    let report = pronoun_starts::report(&document(text), &Default::default()).unwrap();
    let all = &report.groups.all;
    assert!(all.words > 0);
    assert_eq!(
        all.pronoun_words.count,
        all.word_pronouns
            .iter()
            .map(|row| row.share.count)
            .sum::<usize>()
    );
    assert!(all
        .word_pronouns
        .iter()
        .any(|row| row.pronoun == "my" && row.share.count == 1));
    assert!(all
        .word_pronouns
        .iter()
        .any(|row| row.pronoun == "his" && row.share.count == 1));
    assert_eq!(
        all.words,
        report.groups.narration.words + report.groups.dialogue.words
    );
    assert_eq!(
        all.pronoun_words.count,
        report.groups.narration.pronoun_words.count + report.groups.dialogue.pronoun_words.count
    );
    assert!(all.pronoun_words.percent.unwrap() > 0.0);
    let nouns = pronoun_starts::report(
        &document("The US army marched. The coal mine closed."),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(nouns.groups.all.pronoun_words.count, 0);
    let archaic = pronoun_starts::report(
        &document("Thou art mad, and thy ship is lost; ye shall see thee and thine."),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(archaic.groups.all.pronoun_words.count, 5);
    assert!(!archaic
        .groups
        .all
        .word_pronouns
        .iter()
        .any(|row| row.pronoun == "whose"));
}
