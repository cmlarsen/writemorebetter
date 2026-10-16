use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::{
    dialogue_tags, echoes, repeated_phrases, sentence_length, sentence_starts, StyleRequest,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions};

fn document(text: &str) -> Document {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    analyze_document(
        TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap()),
        text,
        &DocumentOptions::new(),
    )
}

#[test]
fn bands_highlight_only_outside_the_matching_mode_and_validate() {
    let d = document("She waited. She left. “Go!”");
    let options = sentence_length::SentenceLengthOptions::new().with_minimum_sentences(1);
    let baseline = serde_json::from_value(serde_json::json!({
        "mean": 4, "narration": {"mean":2}, "dialogue":{"mean":1},
        "bands":{"all":{"mean":{"p5":2,"p95":3}},"narration":{"mean":{"p5":1,"p95":2}},"dialogue":{"mean":{"p5":2,"p95":3}}}
    })).unwrap();
    let r = sentence_length::report(&d, &options, Some(&baseline)).unwrap();
    assert!(r.groups.all.mean_highlighted);
    assert!(!r.groups.narration.mean_highlighted);
    assert!(r.groups.dialogue.mean_highlighted);
    assert_eq!(r.groups.narration.mean_difference, Some(0.0));
    assert_eq!(r.groups.dialogue.mean_difference, Some(0.0));
    let bundled = sentence_length::SentenceLengthBaseline::bundled().unwrap();
    let r = sentence_length::report(&d, &options, Some(&bundled)).unwrap();
    assert!(!r.groups.all.mean_highlighted);
    assert!(r.groups.narration.mean_difference.is_none());
    let baseline =
        serde_json::from_value(serde_json::json!({"bands":{"all":{"mean":{"p5":3,"p95":2}}}}))
            .unwrap();
    assert!(sentence_length::report(&d, &options, Some(&baseline)).is_err());

    let baseline = serde_json::from_value(serde_json::json!({"bands":{"narration":{"universalPercent":{"PRON":{"p5":20,"p95":80}}},"dialogue":{"universalPercent":{"VERB":{"p5":0,"p95":100}}}}})).unwrap();
    let r = sentence_starts::report(
        &d,
        &sentence_starts::SentenceStartsOptions::new().with_minimum_sentences(1),
        Some(&baseline),
    )
    .unwrap();
    assert!(
        r.groups
            .narration
            .universal
            .iter()
            .find(|v| v.tag == "PRON")
            .unwrap()
            .highlighted
    );
    assert!(!r.groups.dialogue.universal.iter().any(|v| v.highlighted));
}

#[test]
fn sample_floors_have_independent_confidence_labels() {
    for n in [49, 50, 199, 200] {
        let d = document(&"She waited. ".repeat(n));
        let l = sentence_length::report(&d, &Default::default(), None).unwrap();
        let s = sentence_starts::report(&d, &Default::default(), None).unwrap();
        assert_eq!(l.groups.all.mean.is_some(), n >= 50);
        assert_eq!(s.groups.all.categories[0].share.percent.is_some(), n >= 50);
        assert_eq!(l.groups.all.low_confidence, n < 200);
        assert_eq!(s.groups.all.low_confidence, n < 200);
    }
    for n in [14, 15, 49, 50] {
        let d = document(&"“Fine,” she said.\n".repeat(n));
        let r = dialogue_tags::report(&d, &Default::default(), None).unwrap();
        assert_eq!(r.said_asked.count, n);
        assert_eq!(r.said_asked.percent.is_some(), n >= 15);
        assert_eq!(r.low_confidence, n < 50);
    }
}

#[test]
fn narration_runs_cross_paragraphs_and_stop_at_scenes_and_dialogue() {
    for (separator, expected) in [("\n", 1), ("\n\n", 0), ("\n“Go!”\n", 0), ("\n* * *\n", 0)] {
        let text = ["She waited.", "She left.", "She slept.", "She wept."].join(separator);
        let d = document(&text);
        assert_eq!(
            sentence_length::report(&d, &Default::default(), None)
                .unwrap()
                .groups
                .all
                .monotony_runs
                .len(),
            expected,
            "{text}"
        );
        assert_eq!(
            sentence_starts::report(&d, &Default::default(), None)
                .unwrap()
                .groups
                .all
                .same_word_runs
                .len(),
            expected,
            "{text}"
        );
    }
    let d = document("“She waited. She left. She slept. She wept.”");
    assert!(sentence_length::report(&d, &Default::default(), None)
        .unwrap()
        .groups
        .all
        .monotony_runs
        .is_empty());
    assert!(sentence_starts::report(&d, &Default::default(), None)
        .unwrap()
        .groups
        .all
        .same_word_runs
        .is_empty());
}

#[test]
fn echoes_are_opt_in_suppressed_and_never_chain_through_a_third_word() {
    assert!(StyleRequest::new().echoes.is_none());
    let d = document("“Well, good night,” she said. He said good night. They looked up, down, out, back, off, over. Up, down, out, back, off, over. Mr. Jones saw Mrs. Jones. Better.");
    let r = echoes::report(&d, &Default::default()).unwrap();
    for echo in r.echoes {
        let earlier = &d.paragraphs[0].text[echo.earlier.byte.clone()];
        let later = &d.paragraphs[0].text[echo.later.byte.clone()];
        assert!(
            !["said", "up", "down", "out", "back", "off", "over", "Mr.", "Mrs."].contains(&later)
        );
        assert!(!(earlier == "Well" && later == "good"));
    }
}

#[test]
fn repeated_phrases_merge_and_require_frequency_or_proximity() {
    let far = format!(
        "The brass gate creaked. {} The brass gate creaked.",
        "They waited. ".repeat(151)
    );
    let d = document(&far);
    let r = repeated_phrases::report(&d, &Default::default()).unwrap();
    assert!(!r
        .phrases
        .iter()
        .any(|p| p.labels.iter().any(|s| s == "brass")));
    let d = document(&(far + " The brass gate creaked."));
    assert!(repeated_phrases::report(&d, &Default::default())
        .unwrap()
        .phrases
        .iter()
        .any(|p| p.count == 3 && p.labels.iter().any(|s| s == "brass")));
    let d = document("She gently closed the old brass gate behind her. She gently closed the old brass gate behind her.");
    let r = repeated_phrases::report(&d, &Default::default()).unwrap();
    assert_eq!(r.phrases.len(), 1);
    assert!(r.phrases[0].keys.len() > 5);
}

#[test]
fn long_sentences_use_rank_and_the_writers_own_distribution_per_chapter() {
    let text = (1..=100)
        .map(|n| format!("{}.", vec!["word"; n].join(" ")))
        .collect::<Vec<_>>()
        .join("\n");
    let d = document(&text);
    let r = sentence_length::report(&d, &Default::default(), None).unwrap();
    assert_eq!(r.groups.all.long_sentences, vec![95, 96, 97, 98, 99]);
    let d = document(&format!(
        "Chapter One\n{text}\nChapter Two\n{}.",
        vec!["word"; 40].join(" ")
    ));
    assert_eq!(
        sentence_length::report(&d, &Default::default(), None)
            .unwrap()
            .groups
            .all
            .long_sentences
            .len(),
        6
    );
}

#[test]
fn speech_attributions_and_tag_manner_spans() {
    let text = "“Fine,” said Psmith, pained. “No,” roared the examining magistrate. “Yes,” she said quite calmly. “Maybe.” Poirot reflected for a minute. “Fine,” she smiled politely.";
    let d = document(text);
    let r = dialogue_tags::report(&d, &Default::default(), None).unwrap();
    let spans = r
        .tags
        .iter()
        .map(|t| text.get(t.span.byte.clone()).unwrap())
        .collect::<Vec<_>>();
    assert!(spans.contains(&"said Psmith"), "{spans:?}");
    assert!(
        spans.contains(&"roared the examining magistrate"),
        "{spans:?}"
    );
    assert_eq!(r.with_adverb.count, 1);
    assert_eq!(r.showy_verbs.iter().map(|v| v.count).sum::<usize>(), 2);
    assert!(r.neutral_verbs.iter().any(|v| v.lemma == "say"));
    assert!(!r.tags.iter().any(|v| v.lemma == "reflect"));
    for text in [
        "“There she blows,” was sung out.",
        "“He’ll be here afore long,” was the answer.",
        "“He pays reg’lar,” was the rejoinder.",
        "A cry of “Bulkington! Where is he?”",
        "Then “Fine,” she said.",
    ] {
        let d = document(text);
        assert_eq!(d.speech, vec![true], "{text}");
    }
    let d = document("Chapter One\nShe waited…\u{2028}She didn’t leave—yet.\n* * *\n“She left.”");
    assert_eq!(d.sentences().count(), 3);
    assert_eq!(d.paragraphs.len(), 2);
    assert!(d.paragraphs[0].chapter_start);
    assert!(d.paragraphs[1].scene_break_before);
}

#[test]
fn neutral_counts_showy_verbs_and_untagged_threshold() {
    for (verb, showy) in [
        ("said", false),
        ("asked", false),
        ("replied", false),
        ("answered", false),
        ("whispered", false),
        ("shouted", false),
        ("muttered", false),
        ("chortled", true),
        ("ejaculated", true),
        ("hissed", true),
        ("laughed", true),
        ("sighed", true),
    ] {
        let text = format!("“Fine,” he {verb}.");
        let r = dialogue_tags::report(&document(&text), &Default::default(), None).unwrap();
        assert_eq!(r.tags.len(), 1, "{verb}");
        assert_eq!(r.tags[0].showy, showy, "{verb}");
        assert_eq!(
            r.showy_verbs.iter().map(|v| v.count).sum::<usize>(),
            usize::from(showy),
            "{verb}"
        );
    }
    for n in [12, 13] {
        let r = dialogue_tags::report(&document(&"“Wait.”\n".repeat(n)), &Default::default(), None)
            .unwrap();
        assert_eq!(r.untagged_runs.len(), usize::from(n > 12));
    }
    let r = dialogue_tags::report(
        &document("“Wait.”\nThe magistrate cut her short."),
        &Default::default(),
        None,
    )
    .unwrap();
    assert_eq!(r.beats.len(), 1);
}

#[test]
fn skipped_surnames_cover_honorifics_and_ordinary_prose_is_not_a_heading() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let d = analyze_document(
        &tokenizer,
        "“Mr. Jones,” Dr. Jones said. Monsieur Jones asked.",
        &DocumentOptions::new().with_skip_names(vec!["Jones".into()]),
    );
    assert_eq!(d.word_count, 2);
    assert!(d
        .words()
        .all(|t| ["said", "asked"].contains(&t.text.as_str())));
    let d = document("Part of it was missing.\nBook the next train.\nChapter One\nShe left.");
    assert_eq!(d.paragraphs.len(), 3);
    assert_eq!(d.sentences().count(), 3);
}
