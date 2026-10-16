use std::sync::OnceLock;

use nlprule::Tokenizer;
use writemorebetter_style::reports::{
    dialogue_tags, echoes, repeated_phrases, sentence_length, sentence_starts,
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
    let missing = sentence_length::SentenceLengthBaseline::new();
    let r = sentence_length::report(&d, &options, Some(&missing)).unwrap();
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
    for (separator, expected) in [
        ("\n", 1),
        ("\n\n", 1),
        ("\n\n\n", 0),
        ("\n“Go!”\n", 0),
        ("\n* * *\n", 0),
    ] {
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
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences, vec![97, 98, 99]);
    let d = document(&format!(
        "Chapter One\n{text}\nChapter Two\n{}.",
        vec!["word"; 40].join(" ")
    ));
    assert_eq!(
        sentence_length::report(
            &d,
            &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
            None
        )
        .unwrap()
        .groups
        .all
        .long_sentences
        .len(),
        4
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
    assert_eq!(r.showy_verbs.iter().map(|v| v.count).sum::<usize>(), 1);
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
            usize::from(showy && r.tags[0].class != dialogue_tags::DialogueTagVerbClass::NonSpeech),
            "{verb}"
        );
    }
    for n in [12, 13] {
        for separator in ["\n", "\n\n"] {
            let r = dialogue_tags::report(
                &document(&format!("“Wait.”{separator}").repeat(n)),
                &Default::default(),
                None,
            )
            .unwrap();
            assert_eq!(r.untagged_runs.len(), usize::from(n > 12));
        }
    }
    let r = dialogue_tags::report(
        &document("“Wait.”\nThe magistrate cut her short.\n“No.”"),
        &Default::default(),
        None,
    )
    .unwrap();
    assert_eq!(r.beats.len(), 1);
    assert_eq!(
        r.beats[0].quotes,
        vec![document("“Wait.”\nThe magistrate cut her short.\n“No.”").dialogue[1].clone()]
    );
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

#[test]
fn published_median_chapters_do_not_highlight_and_beyond_p95_does() {
    let calibration: serde_json::Value =
        serde_json::from_str(include_str!("../data/baselines/sentence-length.json")).unwrap();
    let starts_calibration: serde_json::Value =
        serde_json::from_str(include_str!("../data/baselines/sentence-starts.json")).unwrap();
    let lengths = sentence_length::SentenceLengthBaseline::bundled().unwrap();
    let starts = sentence_starts::SentenceStartsBaseline::bundled().unwrap();
    for mode in ["all", "narration", "dialogue"] {
        for extreme in [false, true] {
            let p = if extreme { "95" } else { "50" };
            let mean = calibration["calibration"][mode]["mean"]["percentiles"][p]
                .as_f64()
                .unwrap()
                + if extreme { 2.0 } else { 0.0 };
            let pron = starts_calibration["calibration"][mode]["universal"]["PRON"]["percentiles"]
                [p]
                .as_f64()
                .unwrap()
                + if extreme { 5.0 } else { 0.0 };
            let total = (mean * 200.0).round() as usize;
            let pron_count = (pron * 2.0).round() as usize;
            let text = (0..200)
                .map(|i| {
                    let count = total / 200 + usize::from(i < total % 200);
                    let sentence = format!(
                        "{} {}.",
                        if i < pron_count { "She" } else { "Trees" },
                        vec!["waited"; count - 1].join(" ")
                    );
                    if mode == "dialogue" {
                        format!("“{sentence}”")
                    } else {
                        sentence
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            let d = document(&text);
            let l = sentence_length::report(&d, &Default::default(), Some(&lengths)).unwrap();
            let s = sentence_starts::report(&d, &Default::default(), Some(&starts)).unwrap();
            let (l, s) = match mode {
                "narration" => (&l.groups.narration, &s.groups.narration),
                "dialogue" => (&l.groups.dialogue, &s.groups.dialogue),
                _ => (&l.groups.all, &s.groups.all),
            };
            assert_eq!(l.sentences, 200, "{mode}");
            assert_eq!(l.mean_highlighted, extreme, "length {mode}: {:?}", l.mean);
            let pron = s.universal.iter().find(|r| r.tag == "PRON").unwrap();
            assert_eq!(pron.highlighted, extreme, "starts {mode}: {pron:?}");
        }
    }
}

#[test]
fn reviewed_bookisms_are_showy_including_all_three_cried_occurrences() {
    for (source, verbs) in [
        ("Moby", vec!["demanded", "cried", "cried", "exclaimed"]),
        (
            "Psmith",
            vec![
                "urged",
                "explained",
                "explained",
                "inquired",
                "pleaded",
                "echoed",
            ],
        ),
        ("Links", vec!["cried", "inquired", "remarked"]),
    ] {
        for verb in verbs {
            let r = dialogue_tags::report(
                &document(&format!("“Fine,” he {verb}.")),
                &Default::default(),
                None,
            )
            .unwrap();
            assert_eq!(r.tags.len(), 1, "{source}: {verb}");
            assert!(r.tags[0].showy, "{source}: {verb}");
            assert_eq!(r.showy_verbs.iter().map(|v| v.count).sum::<usize>(), 1);
        }
    }
}

#[test]
fn phrase_suppressions_boundaries_and_automatic_roster_fallback() {
    for text in [
        "I got up. I got up again. I got up at last.",
        "She never came back. She never came back. She never came back.",
        "Piglet said that it was late. Piglet said that it was late. Piglet said that it was late.",
    ] {
        let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
        let target = if text.starts_with("I got") {
            "got up"
        } else if text.starts_with("She never") {
            "came back"
        } else {
            "Piglet said that"
        };
        assert!(
            r.phrases.iter().any(|p| p.count == 3
                && p.spans
                    .iter()
                    .all(|span| text.get(span.byte.clone()).unwrap().contains(target))),
            "{text}: {r:?}"
        );
    }
    for separator in [",", ";", "—", "…", "...", "”"] {
        let text = format!(
            "Bright{separator} silver. Bright{separator} silver. Bright{separator} silver."
        );
        let r = repeated_phrases::report(&document(&text), &Default::default()).unwrap();
        assert!(r.phrases.is_empty(), "{text}: {r:?}");
    }
    let text =
        "They saw Mara Jones waiting. They saw Mara Jones waiting. They saw Mara Jones waiting.";
    let options = repeated_phrases::RepeatedPhraseOptions::new().with_automatic_names(true);
    let r = repeated_phrases::report(&document(text), &options).unwrap();
    assert!(r
        .phrases
        .iter()
        .all(|p| !p.keys.iter().any(|k| k == "mara" || k == "jones")));
    let d = analyze_document(
        &Tokenizer::new("../storage/en_tokenizer.bin").unwrap(),
        text,
        &DocumentOptions::new().with_skip_names(vec!["Other".into()]),
    );
    let r = repeated_phrases::report(&d, &options).unwrap();
    assert!(r.phrases.iter().any(|p| p.keys.iter().any(|k| k == "mara")));
}

#[test]
fn headings_preserve_prose_signs_and_mark_markdown_chapters() {
    for text in [
        "Part civil servant, part spy.",
        "Part mild amusement, part dread.",
        "Book one was a disaster.",
        "BANG",
        "NO VACANCY",
        "I",
    ] {
        let d = document(text);
        assert_eq!(d.paragraphs.len(), 1, "{text}");
        assert!(d.word_count > 0, "{text}");
    }
    let d = document("CHAPTER 1. Loomings.\nShe waited.");
    assert!(d.paragraphs[0].chapter_start);
    assert_eq!(d.paragraphs.len(), 1);
    let d = document("## Chapter Two\nShe waited.");
    assert!(d.paragraphs[0].chapter_start);
    let d = document("“Where?” I asked.");
    assert_eq!(d.sentences().count(), 1);
}

#[test]
fn echo_ambiguity_titles_window_and_turns_have_positive_controls() {
    for text in [
        "The leaves fell. She left.",
        "He was better. She was well.",
        "He was good. She was best.",
        "The stranger arrived. It was strange.",
        "He uttered words. It was utter nonsense.",
        "The guard rested. A rest was needed.",
        "He was late. She arrived later.",
        "The makings were scattered. She was making bread.",
        "She liked it. It looked like rain.",
        "Good morning. He was better.",
    ] {
        let r = echoes::report(&document(text), &Default::default()).unwrap();
        assert!(r.echoes.is_empty(), "{text}: {r:?}");
    }
    for text in [
        "The ladies came in. The ladies sat down.",
        "I miss her. I miss him.",
        "He walked. She was walking.",
    ] {
        assert!(
            !echoes::report(&document(text), &Default::default())
                .unwrap()
                .echoes
                .is_empty(),
            "{text}"
        );
    }
    for text in [
        "Miss Jones came. Miss Smith left.",
        "The lantern lit the lantern.",
        "“The lantern.”\n“Another lantern.”",
    ] {
        assert!(
            echoes::report(&document(text), &Default::default())
                .unwrap()
                .echoes
                .is_empty(),
            "{text}"
        );
    }
}

#[test]
fn anaphora_is_suppressed_but_varied_second_words_still_flag() {
    for text in [
        "The clock started. The clock stopped. The clock ticked.",
        "Why wait? Why leave! Why stay?",
    ] {
        assert!(
            sentence_starts::report(&document(text), &Default::default(), None)
                .unwrap()
                .groups
                .all
                .same_word_runs
                .is_empty()
        );
    }
    assert_eq!(
        sentence_starts::report(
            &document("The thermos fell. The meeting ended. The broker left."),
            &Default::default(),
            None
        )
        .unwrap()
        .groups
        .all
        .same_word_runs
        .len(),
        1
    );
}

#[test]
fn long_sentence_options_exclude_quotes_blocks_and_preserve_percentile_option() {
    let short = "She waited. ".repeat(200);
    let long = format!("{}.", vec!["word"; 42].join(" "));
    let d = document(&format!("{short}{}", vec![long.clone(); 10].join("\n")));
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences.len(), 3);
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new()
            .with_long_sentences(true)
            .with_above_percentile(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences.len(), 10);
    let d = document(&format!("“{long}”\n> {long}\n    {long}\n{long}"));
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences.len(), 2);
}

#[test]
fn beats_introduce_following_speech_and_manner_does_not_modify_adjective() {
    for text in [
        "“You coming?” she asked.\nI nodded.",
        "“Where?” he asked.\nShe didn’t answer.",
        "“No.”\nShe did not interrupt.\n“Yes.”",
        "“No.”\nShe didn’t interrupt.\n“Yes.”",
    ] {
        assert!(
            dialogue_tags::report(&document(text), &Default::default(), None)
                .unwrap()
                .beats
                .is_empty(),
            "{text}"
        );
    }
    let text = "“Fine,” said Eve, politely uninterested.";
    assert_eq!(
        dialogue_tags::report(&document(text), &Default::default(), None)
            .unwrap()
            .with_adverb
            .count,
        0
    );
    assert!(
        serde_json::from_str::<sentence_length::SentenceLengthBaseline>(
            r#"{"narration":{"bands":{}}}"#
        )
        .is_err()
    );
    assert!(
        serde_json::from_str::<sentence_starts::SentenceStartsBaseline>(
            r#"{"dialogue":{"dialogue":{}}}"#
        )
        .is_err()
    );
}

#[test]
fn chapter_separators_keep_selection_independent_without_deleting_prose() {
    let long = (41..45)
        .map(|n| format!("{}.", vec!["word"; n].join(" ")))
        .collect::<Vec<_>>()
        .join("\n");
    for heading in [
        "Chapter 1: The Storm",
        "Chapter One: The Storm",
        "Chapter 1 — The Storm",
        "Part IV – The Storm",
        "CHAPTER 1. Loomings.",
    ] {
        let text = format!("{heading}\n{long}\n{heading}\n{long}");
        let d = document(&text);
        assert_eq!(
            d.paragraphs.iter().filter(|p| p.chapter_start).count(),
            2,
            "{heading}"
        );
        assert_eq!(d.sentences().count(), 8, "{heading}");
        let r = sentence_length::report(
            &d,
            &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
            None,
        )
        .unwrap();
        assert_eq!(r.groups.all.long_sentences.len(), 6, "{heading}");
    }
    for text in [
        "Part civil servant, part spy.",
        "Book one was a disaster.",
        "Part iv",
        "BANG",
        "NO VACANCY",
    ] {
        assert_eq!(document(text).paragraphs.len(), 1, "{text}");
    }
}

#[test]
fn abbreviations_do_not_split_phrases_or_make_honorifics_content() {
    let text = "She said Mrs. Dalloway. She said Mrs. Coates. She said Mrs. Bletchley.";
    let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
    assert!(r.phrases.is_empty(), "{r:?}");
    let text = "He saw Dr. Jones at the old mill. He saw Dr. Jones at the old mill. He saw Dr. Jones at the old mill.";
    let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
    assert!(
        r.phrases.iter().any(|p| p.count == 3
            && p.spans
                .iter()
                .all(|s| text.get(s.byte.clone()).unwrap().contains("Dr. Jones"))),
        "{r:?}"
    );
    for text in [
        "Bright... silver. Bright... silver. Bright... silver.",
        "Bright… silver. Bright… silver. Bright… silver.",
    ] {
        assert!(
            repeated_phrases::report(&document(text), &Default::default())
                .unwrap()
                .phrases
                .is_empty()
        );
    }
}

#[test]
fn frequent_shorter_phrases_survive_rare_longer_repeats() {
    let text = "The young man waited. The young man waited. The young man spoke. The young man ran. The bright young man sat. The bright young man sat.";
    let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
    let young = r
        .phrases
        .iter()
        .find(|p| p.labels.join(" ") == "the young man")
        .unwrap();
    assert_eq!(young.count, 4);
    assert!(r
        .phrases
        .iter()
        .any(|p| p.labels.join(" ").contains("bright young man")));

    let text = "The young man waited. The young man spoke. The young man ran. “Fine,” said the young man. “No,” said the young man.";
    let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
    assert!(
        r.phrases
            .iter()
            .any(|p| p.labels.join(" ") == "the young man" && p.count == 5),
        "{r:?}"
    );
    let text = "The old grey house stood. The old grey house fell. A grey house remained.";
    let r = repeated_phrases::report(&document(text), &Default::default()).unwrap();
    assert!(!r.phrases.iter().any(|p| p.labels.join(" ") == "grey house"));
}

#[test]
fn noun_plurals_and_gerunds_recover_without_ambiguous_family_joins() {
    for (singular, plural) in [
        ("bucket", "buckets"),
        ("blanket", "blankets"),
        ("ladder", "ladders"),
        ("horse", "horses"),
        ("window", "windows"),
        ("candle", "candles"),
        ("ground", "grounds"),
    ] {
        let text = format!("The {singular} stood. The {plural} fell.");
        let r = echoes::report(&document(&text), &Default::default()).unwrap();
        assert!(
            r.echoes
                .iter()
                .any(|e| text.get(e.earlier.byte.clone()) == Some(singular)
                    && text.get(e.later.byte.clone()) == Some(plural)),
            "{text}: {r:?}"
        );
    }
    let text = "He swung. They were swinging.";
    assert!(!echoes::report(&document(text), &Default::default())
        .unwrap()
        .echoes
        .is_empty());
    for text in [
        "The leaves fell. She left.",
        "He was better. She was well.",
        "He uttered words. It was utter nonsense.",
    ] {
        assert!(
            echoes::report(&document(text), &Default::default())
                .unwrap()
                .echoes
                .is_empty(),
            "{text}"
        );
    }
}

#[test]
fn own_narration_with_quotes_and_first_line_indents_remains_a_long_candidate() {
    let words = vec!["word"; 50].join(" ");
    let text = format!("She called it “the plan” and {words}.\n    She remembered {words}.\n“{words}.”\n> {words}.\nSACRED {} _December_ 31st 1839.", vec!["MEMORY"; 45].join(" "));
    let r = sentence_length::report(
        &document(&text),
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences, vec![0, 1]);
    let d = analyze_document(
        &Tokenizer::new("../storage/en_tokenizer.bin").unwrap(),
        &format!("    {words}\n    {words}."),
        &DocumentOptions::new().with_hard_wrapped(true),
    );
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert!(r.groups.all.long_sentences.is_empty());
}

#[test]
fn defaults_retain_statistics_and_monotony_but_lenses_are_explicit() {
    let text = format!(
        "{} {}.",
        "She waited. ".repeat(50),
        vec!["word"; 50].join(" ")
    );
    let d = document(&text);
    let r = sentence_length::report(&d, &Default::default(), None).unwrap();
    let group = &r.groups.all;
    assert!(group.mean.is_some());
    assert!(!group.monotony_runs.is_empty());
    assert!(group.long_sentences.is_empty());
    let r = sentence_length::report(
        &d,
        &sentence_length::SentenceLengthOptions::new().with_long_sentences(true),
        None,
    )
    .unwrap();
    assert_eq!(r.groups.all.long_sentences.len(), 1);
    assert!(repeated_phrases::report(&d, &Default::default()).is_ok());
    assert!(echoes::report(&d, &Default::default()).is_ok());
}

#[test]
fn inferred_names_do_not_mask_lowercase_homonyms() {
    let text = "They saw Rose waiting. They saw Rose waiting. The rose garden grew. The rose garden grew. The rose garden grew.";
    let r = repeated_phrases::report(
        &document(text),
        &repeated_phrases::RepeatedPhraseOptions::new().with_automatic_names(true),
    )
    .unwrap();
    assert!(
        r.phrases
            .iter()
            .any(|p| p.keys.iter().any(|k| k == "rose") && p.count == 3),
        "{r:?}"
    );
}
