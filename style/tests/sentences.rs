use nlprule::Tokenizer;
use writemorebetter_style::reports::sentence_length::{
    self as length, SentenceLengthBaseline, SentenceLengthOptions,
};
use writemorebetter_style::reports::sentence_starts::{
    self as starts, SentenceStartCategory, SentenceStartsBaseline, SentenceStartsOptions,
};
use writemorebetter_style::reports::sentence_table;
use writemorebetter_style::{analyze_document, DocumentOptions, StyleError};

fn category(
    group: &starts::SentenceStartsGroup,
    category: SentenceStartCategory,
) -> &starts::SentenceStartCategoryShare {
    group
        .categories
        .iter()
        .find(|share| share.category == category)
        .unwrap()
}

fn universal<'a>(
    group: &'a starts::SentenceStartsGroup,
    tag: &str,
) -> &'a starts::SentenceStartUniversalShare {
    group
        .universal
        .iter()
        .find(|share| share.tag == tag)
        .unwrap()
}

#[test]
fn hand_labeled_categories_and_unicode() {
    use SentenceStartCategory::*;
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let cases = [
        (
            Subject,
            [
                "She ran.",
                "The bird sang.",
                "Mara waited.",
                "My hands shook.",
                "They don’t know—yet…\u{2028}perhaps.",
            ],
        ),
        (
            Adverb,
            [
                "Quickly she ran.",
                "Slowly he rose.",
                "Silently they waited.",
                "Carefully she opened it.",
                "Suddenly he stopped.",
            ],
        ),
        (
            Gerund,
            [
                "Running down the road, he fell.",
                "Walking home, she sang.",
                "Looking back, he waved.",
                "Hoping for rain, she waited.",
                "Smiling broadly, he bowed.",
            ],
        ),
        (
            Infinitive,
            [
                "To run is fun.",
                "To walk was impossible.",
                "To see her was enough.",
                "To speak would be foolish.",
                "To leave now meant defeat.",
            ],
        ),
        (
            SubordinatingConjunction,
            [
                "Because she ran, he followed.",
                "Although it rained, he stayed.",
                "If you wait, I will come.",
                "While he slept, she worked.",
                "When she woke, he left.",
            ],
        ),
        (
            CoordinatingConjunction,
            [
                "And she ran.",
                "But he waited.",
                "Or we could leave.",
                "Yet she stayed.",
                "Nor did he complain.",
            ],
        ),
        (
            PastParticiple,
            [
                "Exhausted, she fell asleep.",
                "Surrounded by wolves, he froze.",
                "Hidden behind the door, she waited.",
                "Trapped in the room, he shouted.",
                "Driven by fear, she ran.",
            ],
        ),
        (
            Inversion,
            [
                "Did she leave?",
                "Was he ready?",
                "Could they wait?",
                "Had she forgotten?",
                "Is the door open?",
            ],
        ),
        (
            Introductory,
            [
                "However, she waited.",
                "In fact, he left.",
                "Of course, they knew.",
                "Yes, I know.",
                "Well, she tried.",
            ],
        ),
    ];
    let mut failures = Vec::new();
    for (expected, examples) in cases {
        for text in examples {
            for quoted in [false, true] {
                let input = if quoted {
                    format!("😀\n\n“{text}” —… don’t\u{2028}stop.")
                } else {
                    text.into()
                };
                let document = analyze_document(&tokenizer, &input, &DocumentOptions::new());
                let sentence = &document.paragraphs.last().unwrap().sentences[0];
                if starts::classify(sentence) != expected {
                    failures.push(format!(
                        "{input}: {:?} expected {:?}",
                        starts::classify(sentence),
                        expected
                    ));
                    continue;
                }
                let result =
                    starts::report(&document, &SentenceStartsOptions::new(), None).unwrap();
                let groups = &result.groups;
                assert_eq!(
                    groups.all.sentences,
                    groups.narration.sentences + groups.dialogue.sentences
                );
                if quoted {
                    assert!(groups.dialogue.sentences > 0, "{input}");
                }
                assert!(category(&groups.all, expected).share.percent.is_none());
                let span = result.starts.last().unwrap().opener.as_ref().unwrap();
                assert!(input.get(span.byte.clone()).is_some());
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn lengths_runs_thresholds_baselines_and_skips() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let text = "She ran. She left. She slept. She wept.\n\n“He ran. He left. He slept. He wept.”\n\n😀 They don’t know—yet…\u{2028}perhaps.";
    let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
    let options = SentenceLengthOptions::new()
        .with_long_sentences(true)
        .with_minimum_sentences(4)
        .with_long_sentence_words(2)
        .with_longest_per_chapter(20);
    let baseline: SentenceLengthBaseline = serde_json::from_str(
        r#"{"mean":3,"percentiles":{"50":3},"dialogue":{"mean":3,"percentiles":{"50":3}}}"#,
    )
    .unwrap();
    let result = length::report(&document, &options, Some(&baseline)).unwrap();
    let groups = &result.groups;
    assert_eq!(groups.narration.monotony_runs.len(), 1);
    assert!(groups.dialogue.monotony_runs.is_empty());
    assert!(groups
        .dialogue
        .histogram
        .iter()
        .any(|bucket| bucket.words == 2 && bucket.sentences == 4));
    assert!(groups.dialogue.long_sentences.is_empty());
    assert_eq!(groups.dialogue.mean_difference, Some(-1.0));
    let median = groups
        .dialogue
        .percentiles
        .iter()
        .find(|p| p.percentile == 50)
        .unwrap();
    assert_eq!(median.difference, Some(-1.0));
    let starts = starts::report(&document, &SentenceStartsOptions::new(), None).unwrap();
    assert_eq!(starts.groups.all.same_word_runs.len(), 1);
    for span in groups
        .all
        .monotony_runs
        .iter()
        .chain(&starts.groups.all.same_word_runs)
        .map(|run| &run.span)
    {
        let byte_text = text.get(span.byte.clone()).unwrap();
        let units: Vec<_> = text.encode_utf16().collect();
        assert_eq!(
            String::from_utf16(&units[span.utf16.clone()]).unwrap(),
            byte_text
        );
    }
    let compact = analyze_document(
        &tokenizer,
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert_eq!(
        length::report(&compact, &options, None)
            .unwrap()
            .groups
            .all
            .histogram,
        groups.all.histogram
    );
    assert_eq!(
        starts::report(&compact, &SentenceStartsOptions::new(), None),
        Err(StyleError::NeedsTokenDetails {
            report: "sentenceStarts".into()
        })
    );
    let skipped = analyze_document(
        &tokenizer,
        "Mara ran.",
        &DocumentOptions::new().with_skip_names(vec!["Mara".into()]),
    );
    assert_eq!(sentence_table(&skipped)[0].words, 1);
    let skip_length = length::report(&skipped, &options, None).unwrap();
    assert_eq!(skip_length.groups.all.sentences, 1);
    let skip_start = starts::report(&skipped, &SentenceStartsOptions::new(), None).unwrap();
    assert_eq!(skip_start.groups.all.sentences, 1);
    assert_eq!(skip_start.groups.all.excluded_sentences, 1);
    assert!(skip_start.starts.is_empty());
}

#[test]
fn fractional_ambiguity_and_empty_input() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let mut document = analyze_document(&tokenizer, "Water flows.", &DocumentOptions::new());
    let first = document.paragraphs[0].sentences[0]
        .tokens
        .iter_mut()
        .find(|t| t.is_word)
        .unwrap();
    let reading = first.readings[0].clone();
    first.readings = ["NN:UN", "VB", "VBP"]
        .iter()
        .map(|pos| {
            let mut r = reading.clone();
            r.pos = (*pos).into();
            r
        })
        .collect();
    let options = SentenceStartsOptions::new().with_minimum_sentences(1);
    let baseline: SentenceStartsBaseline =
        serde_json::from_str(r#"{"universalPercent":{"NOUN":20}}"#).unwrap();
    let report = starts::report(&document, &options, Some(&baseline)).unwrap();
    let all = &report.groups.all;
    assert_eq!(all.ambiguous_starts, 1);
    assert_eq!(universal(all, "NOUN").weight, 0.5);
    assert_eq!(universal(all, "VERB").weight, 0.5);
    assert_eq!(universal(all, "NOUN").difference, Some(30.0));
    let empty = analyze_document(&tokenizer, "", &DocumentOptions::new());
    assert!(starts::report(&empty, &options, None)
        .unwrap()
        .groups
        .all
        .universal
        .iter()
        .all(|c| c.percent.is_none()));
    assert!(length::report(
        &empty,
        &SentenceLengthOptions::new().with_long_sentences(true),
        None
    )
    .unwrap()
    .groups
    .all
    .mean
    .is_none());
}

#[test]
fn monotony_has_no_drift_and_reports_overlapping_maximal_windows() {
    fn document(lengths: &[usize]) -> writemorebetter_style::Document {
        let sentences: Vec<_> = lengths.iter().enumerate().map(|(i, n)| {
            let span = serde_json::json!({"byte":{"start":i,"end":i+1},"char":{"start":i,"end":i+1},"utf16":{"start":i,"end":i+1}});
            let token = serde_json::json!({"text":"a","span":span,"readings":[],"chunks":[],"is_whitespace":false,"is_punctuation":false,"is_word":true,"is_skipped":false});
            serde_json::json!({"text":"a","span":span,"tokens":vec![token;*n]})
        }).collect();
        serde_json::from_value(serde_json::json!({"paragraphs":[{"text":"","span":{"byte":{"start":0,"end":lengths.len()},"char":{"start":0,"end":lengths.len()},"utf16":{"start":0,"end":lengths.len()}},"sentences":sentences}],"dialogue":[],"lemma_index":{},"word_count":lengths.iter().sum::<usize>()})).unwrap()
    }
    for (lengths, expected) in [
        (vec![1, 2, 2, 3, 3, 4, 4], vec![0..5, 1..7]),
        (vec![2, 4, 6, 8, 10], vec![]),
        (vec![8, 12, 8, 12], vec![]),
        (vec![8, 10, 8, 10], vec![0..4]),
        (vec![1, 1, 1, 1, 6, 6, 6, 6], vec![0..4, 4..8]),
        (vec![2; 10_000], vec![0..10_000]),
    ] {
        let report = length::report(
            &document(&lengths),
            &SentenceLengthOptions::new().with_long_sentences(true),
            None,
        )
        .unwrap();
        let runs = &report.groups.all.monotony_runs;
        assert_eq!(
            runs.iter().map(|s| s.span.byte.clone()).collect::<Vec<_>>(),
            expected
        );
        assert_eq!(
            runs.iter().map(|s| s.sentences.clone()).collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn bundled_baselines_are_loadable_and_keep_missing_values_unavailable() {
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../data/baselines/sentence-starts.json")).unwrap();
    let parsed: SentenceStartsBaseline = serde_json::from_value(file["baseline"].clone()).unwrap();
    let starts = SentenceStartsBaseline::bundled().unwrap();
    assert_eq!(starts, parsed);
    starts.validate().unwrap();
    assert!((starts.universal_percent.values().sum::<f64>() - 100.0).abs() < 1e-9);
    assert!(starts.universal_percent.contains_key("."));
    let file: serde_json::Value =
        serde_json::from_str(include_str!("../data/baselines/sentence-length.json")).unwrap();
    let parsed: SentenceLengthBaseline = serde_json::from_value(file["baseline"].clone()).unwrap();
    let length = SentenceLengthBaseline::bundled().unwrap();
    assert_eq!(length, parsed);
    length.validate().unwrap();
    assert_eq!(length.percentiles.len(), 5);
}

#[test]
fn sample_floor_is_per_group_and_long_threshold_is_inclusive() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let text = "She ran. ".repeat(49);
    let small = analyze_document(&tokenizer, &text, &DocumentOptions::new());
    let report = starts::report(&small, &SentenceStartsOptions::new(), None).unwrap();
    assert_eq!(report.groups.narration.sentences, 49);
    assert!(
        category(&report.groups.narration, SentenceStartCategory::Subject)
            .share
            .percent
            .is_none()
    );
    let text = text + "She ran.\n\n“He ran. He left.”";
    let enough = analyze_document(&tokenizer, &text, &DocumentOptions::new());
    let report = starts::report(&enough, &SentenceStartsOptions::new(), None).unwrap();
    assert_eq!(
        category(&report.groups.narration, SentenceStartCategory::Subject)
            .share
            .percent,
        Some(100.0)
    );
    assert_eq!(
        category(&report.groups.narration, SentenceStartCategory::Subject)
            .share
            .count,
        50
    );
    assert_eq!(report.groups.dialogue.sentences, 2);
    assert!(
        category(&report.groups.dialogue, SentenceStartCategory::Subject)
            .share
            .percent
            .is_none()
    );
    let words = vec!["word"; 40].join(" ") + ".";
    let document = analyze_document(&tokenizer, &words, &DocumentOptions::new());
    assert_eq!(
        length::report(
            &document,
            &SentenceLengthOptions::new().with_long_sentences(true),
            None
        )
        .unwrap()
        .groups
        .all
        .long_sentences,
        vec![0]
    );
    let words = vec!["word"; 39].join(" ") + ".";
    let document = analyze_document(&tokenizer, &words, &DocumentOptions::new());
    assert!(length::report(
        &document,
        &SentenceLengthOptions::new().with_long_sentences(true),
        None
    )
    .unwrap()
    .groups
    .all
    .long_sentences
    .is_empty());
}

#[test]
fn reviewed_opening_functions() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("fixtures/sentence-start-review.json")).unwrap();
    let mut failures = Vec::new();
    for case in cases {
        let text = case["text"].as_str().unwrap();
        let expected: SentenceStartCategory =
            serde_json::from_value(case["category"].clone()).unwrap();
        for input in [
            text.to_owned(),
            format!("“{text}”\u{2028}She’s gone—again…"),
        ] {
            let document = analyze_document(&tokenizer, &input, &DocumentOptions::new());
            let sentences = &document.paragraphs[0].sentences;
            let category = starts::classify(&sentences[0]);
            if category != expected {
                failures.push(format!("{input}: {category:?} expected {expected:?}"));
            }
            if input != text {
                if sentences.len() != 2 {
                    failures.push(format!("{input}: {} sentences expected 2", sentences.len()));
                } else if starts::classify(&sentences[1]) != SentenceStartCategory::Subject {
                    failures.push(format!("{input}: trailing sentence not subject"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn universal_starts_skip_one_punctuation_after_quotes_and_reject_unknown_keys() {
    let tokenizer = Tokenizer::new("../storage/en_tokenizer.bin").unwrap();
    let options = SentenceStartsOptions::new();
    let plain = starts::report(
        &analyze_document(&tokenizer, "She ran.", &DocumentOptions::new()),
        &options,
        None,
    )
    .unwrap();
    for prefix in ["…", "—", "(", "*", "“—"] {
        let text = format!("{prefix}she ran.");
        let document = analyze_document(&tokenizer, &text, &DocumentOptions::new());
        let result = starts::report(&document, &options, None).unwrap();
        assert_eq!(
            result.starts[0].universal_candidates, plain.starts[0].universal_candidates,
            "{text}"
        );
        assert_eq!(result.groups.all.unknown_starts, 0, "{text}");
    }
    let document = analyze_document(&tokenizer, "She ran.", &DocumentOptions::new());
    for json in [
        r#"{"universalPercent":{"Noun":20}}"#,
        r#"{"universalPercent":{"NOUN":120}}"#,
        r#"{"categoryPercent":{"subject":-1}}"#,
    ] {
        let baseline: SentenceStartsBaseline = serde_json::from_str(json).unwrap();
        assert!(
            matches!(
                starts::report(&document, &options, Some(&baseline)),
                Err(StyleError::InvalidBaseline { .. })
            ),
            "{json}"
        );
    }
    assert!(serde_json::from_str::<SentenceStartsBaseline>(r#"{"typo":{}}"#).is_err());
    let baseline: SentenceLengthBaseline =
        serde_json::from_str(r#"{"percentiles":{"10":3}}"#).unwrap();
    assert!(matches!(
        length::report(
            &document,
            &SentenceLengthOptions::new().with_long_sentences(true),
            Some(&baseline)
        ),
        Err(StyleError::InvalidBaseline { .. })
    ));
}
