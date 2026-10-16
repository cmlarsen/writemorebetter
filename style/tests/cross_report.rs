use std::sync::OnceLock;

use nlprule::Tokenizer;
use serde_json::Value;
use writemorebetter_style::reports::{
    run, run_json, sentence_length::SentenceLengthOptions, sentence_starts::SentenceStartsOptions,
    to_utf16_json, StyleReports, StyleRequest, StyleResources,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Mode, StyleError};

const PASSAGES: &[(&str, &str, &[&str])] = &[
    (
        "mixed",
        "“I can’t go,” Mara said softly. “Not tonight.”\nShe smiled. Will didn’t answer; he was watching the door.\n“Yes” was the answer. He called it “grub” and ate.\nMara’s mother said, “Go home, Will.”",
        &["Mara", "Will"],
    ),
    (
        "name-starts",
        "Mara walked home. Mara’s dog barked. Will you come? I WILL NOT GO.\nMara.\n“Mara?”",
        &["Mara", "Will"],
    ),
    (
        "hard-wrapped",
        "The old man sat by the\r\nwindow and watched the rain.\r\n\r\n“It will rain,” he said, “as soon as\r\npossible.” He kicked the bucket.\r\n",
        &[],
    ),
    (
        "multiword",
        "Herman Melville wrote. By the way, he left. As soon as possible, we will go. Herman spoke.",
        &[],
    ),
    (
        "unicode",
        "He waited — and waited… “Fine,” she said.\u{2028}Then she left. ‘Tis true, ’twas late. Café naïve 😀 done.",
        &[],
    ),
    (
        "continued",
        "“First part of a long speech.\n“Second part,” she said. Then she was dead as a doornail.",
        &[],
    ),
    (
        "mid-sentence-speech",
        "He shouted “Run!” and the dog ran like the wind. She said that “honesty is the best policy” was a cliché.",
        &[],
    ),
];

fn tokenizer() -> &'static Tokenizer {
    static TOKENIZER: OnceLock<Tokenizer> = OnceLock::new();
    TOKENIZER.get_or_init(|| Tokenizer::new("../storage/en_tokenizer.bin").unwrap())
}

fn resources() -> &'static StyleResources {
    static RESOURCES: OnceLock<StyleResources> = OnceLock::new();
    RESOURCES.get_or_init(StyleResources::new)
}

fn request() -> StyleRequest {
    let mut request = StyleRequest::new();
    request.sentence_length = Some(SentenceLengthOptions::new().with_minimum_sentences(1));
    request.sentence_starts = Some(SentenceStartsOptions::new().with_minimum_sentences(1));
    request
}

fn check_spans(text: &str, value: &Value, path: &str, checked: &mut usize) {
    match value {
        Value::Object(fields) => {
            if let (Some(byte), Some(char), Some(utf16)) =
                (fields.get("byte"), fields.get("char"), fields.get("utf16"))
            {
                let range = |value: &Value| {
                    (
                        value["start"].as_u64().unwrap() as usize,
                        value["end"].as_u64().unwrap() as usize,
                    )
                };
                let (byte, char, utf16) = (range(byte), range(char), range(utf16));
                let prefix = |end: usize| text.get(..end).unwrap_or_else(|| panic!("{path}"));
                assert!(byte.0 <= byte.1, "{path}");
                assert!(text.get(byte.0..byte.1).is_some(), "{path}: char boundary");
                for (offset, scalar, unit) in [(byte.0, char.0, utf16.0), (byte.1, char.1, utf16.1)]
                {
                    assert_eq!(prefix(offset).chars().count(), scalar, "{path}: char");
                    assert_eq!(prefix(offset).encode_utf16().count(), unit, "{path}: utf16");
                }
                *checked += 1;
                return;
            }
            for (key, child) in fields {
                check_spans(text, child, &format!("{path}.{key}"), checked);
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                check_spans(text, child, &format!("{path}[{index}]"), checked);
            }
        }
        _ => {}
    }
}

fn project_utf16(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            if fields.len() == 3
                && fields.contains_key("byte")
                && fields.contains_key("char")
                && fields.contains_key("utf16")
            {
                *value = fields.remove("utf16").unwrap();
            } else {
                fields.values_mut().for_each(project_utf16);
            }
        }
        Value::Array(values) => values.iter_mut().for_each(project_utf16),
        _ => {}
    }
}

fn speech_words(document: &Document) -> usize {
    document
        .words()
        .filter(|token| {
            document.dialogue.iter().enumerate().any(|(index, span)| {
                document.is_speech(index) && span.byte.contains(&token.span.byte.start)
            })
        })
        .count()
}

fn check(name: &str, text: &str, document: &Document, reports: &StyleReports) {
    let stats = &reports.stats;
    let words = &stats.words;
    assert_eq!(words.all, document.word_count, "{name}");
    assert_eq!(words.narration + words.dialogue, words.all, "{name}");
    assert_eq!(words.dialogue, speech_words(document), "{name}");

    let echoes = reports.echoes.as_ref().unwrap();
    assert_eq!(echoes.words, words.all, "{name}: echoes");
    let cliches = reports.cliches.as_ref().unwrap();
    assert_eq!(cliches.groups.all.words, words.all, "{name}: cliches");
    assert_eq!(
        cliches.groups.narration.words, words.narration,
        "{name}: cliches"
    );
    assert_eq!(
        cliches.groups.dialogue.words, words.dialogue,
        "{name}: cliches"
    );
    for found in &cliches.matches {
        assert_eq!(
            found.mode,
            document.mode_at(found.span.byte.start),
            "{name}"
        );
    }

    let sentences = &reports.sentences;
    assert_eq!(sentences.len(), stats.sentences.all, "{name}");
    assert_eq!(
        sentences.iter().map(|row| row.words).sum::<usize>(),
        words.all,
        "{name}: sentence words"
    );
    for (row, sentence) in sentences.iter().zip(document.sentences()) {
        assert_eq!(row.span, sentence.span, "{name}");
        assert_eq!(row.mode, document.sentence_mode(sentence), "{name}");
        let first = sentence.tokens.iter().find(|token| token.is_word);
        assert_eq!(
            row.mode,
            first.map_or(Mode::Narration, |token| document
                .mode_at(token.span.byte.start)),
            "{name}"
        );
    }
    for mode in [Mode::Narration, Mode::Dialogue] {
        assert_eq!(
            sentences.iter().filter(|row| row.mode == mode).count(),
            *stats.sentences.mode(mode),
            "{name}"
        );
    }

    let length = &reports.sentence_length.as_ref().unwrap().groups;
    let starts = reports.sentence_starts.as_ref().unwrap();
    for (length, starts, expected) in [
        (&length.all, &starts.groups.all, stats.sentences.all),
        (
            &length.narration,
            &starts.groups.narration,
            stats.sentences.narration,
        ),
        (
            &length.dialogue,
            &starts.groups.dialogue,
            stats.sentences.dialogue,
        ),
    ] {
        assert_eq!(length.sentences, expected, "{name}: length sentences");
        assert_eq!(starts.sentences, expected, "{name}: starts sentences");
        assert_eq!(
            length
                .histogram
                .iter()
                .map(|bucket| bucket.sentences)
                .sum::<usize>(),
            expected,
            "{name}"
        );
    }
    assert_eq!(
        starts.starts.len(),
        starts.groups.all.sentences - starts.groups.all.excluded_sentences,
        "{name}"
    );
    for start in &starts.starts {
        let row = &sentences[start.sentence];
        if let Some(opener) = &start.opener {
            assert!(row.span.byte.start <= opener.byte.start, "{name}");
            assert!(opener.byte.end <= row.span.byte.end, "{name}");
        }
    }
    for index in length.all.long_sentences.iter() {
        assert!(sentences[*index].words >= SentenceLengthOptions::new().long_sentence_words);
    }

    let tags = reports.dialogue_tags.as_ref().unwrap();
    assert_eq!(tags.dialogue_spans, stats.dialogue_spans, "{name}");
    assert_eq!(tags.speech_spans, stats.speech_spans, "{name}");
    assert_eq!(stats.dialogue_spans, document.dialogue.len(), "{name}");
    for quote in tags
        .tags
        .iter()
        .flat_map(|tag| &tag.quotes)
        .chain(tags.beats.iter().flat_map(|beat| &beat.quotes))
    {
        let index = document
            .dialogue
            .iter()
            .position(|span| span == quote)
            .unwrap();
        assert!(document.is_speech(index), "{name}");
    }

    let mut checked = 0;
    check_spans(
        text,
        &serde_json::to_value(reports).unwrap(),
        name,
        &mut checked,
    );
    assert!(checked > 0, "{name}");
}

#[test]
fn reports_agree_on_words_sentences_and_dialogue() {
    for (name, text, names) in PASSAGES {
        for hard_wrapped in [false, true] {
            for names in [
                Vec::new(),
                names.iter().map(|name| name.to_string()).collect(),
            ] {
                let options = DocumentOptions::new()
                    .with_hard_wrapped(hard_wrapped)
                    .with_skip_names(names);
                let document = analyze_document(tokenizer(), text, &options);
                let reports = run(&document, &request(), resources(), tokenizer()).unwrap();
                assert_eq!(reports.stats.hard_wrapped, hard_wrapped);
                assert_eq!(reports.stats.paragraphs, document.paragraphs.len());
                check(name, text, &document, &reports);

                let json = run_json(
                    tokenizer(),
                    resources(),
                    text,
                    Some(&serde_json::to_string(&request()).unwrap()),
                    &options,
                )
                .unwrap();
                let mut typed = serde_json::to_value(&reports).unwrap();
                project_utf16(&mut typed);
                assert_eq!(
                    serde_json::from_str::<Value>(&json).unwrap(),
                    serde_json::from_str::<Value>(&typed.to_string()).unwrap(),
                    "{name}: run_json shape"
                );
                assert_eq!(to_utf16_json(&reports), json, "{name}");

                let compact = analyze_document(
                    tokenizer(),
                    text,
                    &options.clone().with_token_details(false),
                );
                assert_eq!(compact.speech, document.speech, "{name}");
                assert_eq!(
                    run(&compact, &request(), resources(), tokenizer()),
                    Err(StyleError::NeedsTokenDetails {
                        report: "echoes".into()
                    })
                );
                let mut readings_free = StyleRequest::empty();
                readings_free.cliches = request().cliches;
                readings_free.sentence_length = request().sentence_length;
                let full = run(&document, &readings_free, resources(), tokenizer()).unwrap();
                let mut lean = run(&compact, &readings_free, resources(), tokenizer()).unwrap();
                assert!(!lean.stats.token_details);
                lean.stats.token_details = true;
                assert_eq!(lean, full, "{name}: compact");
            }
        }
    }
}

#[test]
fn one_dialogue_definition_for_every_report() {
    let text = PASSAGES[0].1;
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let reports = run(&document, &request(), resources(), tokenizer()).unwrap();
    let quoted = |quote: &str| {
        let index = document
            .dialogue
            .iter()
            .position(|span| &text[span.byte.clone()] == quote)
            .unwrap();
        document.is_speech(index)
    };
    assert!(quoted("“I can’t go,”"));
    assert!(!quoted("“Yes”"));
    assert!(!quoted("“grub”"));
    assert!(quoted("“Go home, Will.”"));
    let row = |prefix: &str| {
        reports
            .sentences
            .iter()
            .find(|row| text[row.span.byte.clone()].starts_with(prefix))
            .unwrap()
            .mode
    };
    assert_eq!(row("“Yes” was"), Mode::Narration);
    assert_eq!(row("“Not tonight"), Mode::Dialogue);
    assert_eq!(row("Mara’s mother"), Mode::Narration);
    assert_eq!(reports.stats.dialogue_spans, 5);
    assert_eq!(reports.stats.speech_spans, 3);

    let text = PASSAGES[6].1;
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    let reports = run(&document, &request(), resources(), tokenizer()).unwrap();
    assert_eq!(reports.stats.words.dialogue, 1);
    assert_eq!(reports.cliches.unwrap().groups.dialogue.words, 1);
}

#[test]
fn json_request_selects_reports_and_baselines() {
    let text = "“Go,” she said. She ran. She ran. She ran.";
    let options = DocumentOptions::new();
    let all: Value =
        serde_json::from_str(&run_json(tokenizer(), resources(), text, None, &options).unwrap())
            .unwrap();
    for key in [
        "stats",
        "sentences",
        "echoes",
        "repeatedPhrases",
        "cliches",
        "dialogueTags",
        "sentenceLength",
        "sentenceStarts",
    ] {
        assert!(all.get(key).is_some(), "{key}");
    }
    let percentile = &all["sentenceLength"]["groups"]["all"]["percentiles"][2];
    assert_eq!(percentile["percentile"], 50);
    assert!(percentile["value"].is_null());
    assert_eq!(
        all["sentences"][0]["span"],
        serde_json::json!({"start": 0, "end": 16})
    );

    let one = |request: &str| -> Result<Value, StyleError> {
        run_json(tokenizer(), resources(), text, Some(request), &options)
            .map(|json| serde_json::from_str(&json).unwrap())
    };
    let length = one(r#"{"sentenceLength":{"minimumSentences":1},"baselines":"bundled"}"#).unwrap();
    assert!(length.get("sentenceStarts").is_none());
    assert!(length.get("echoes").is_none());
    assert!(length["sentenceLength"]["groups"]["all"]["meanDifference"].is_number());
    let without = one(r#"{"sentence_length":{"minimum_sentences":1}}"#).unwrap();
    assert!(without["sentenceLength"]["groups"]["all"]["meanDifference"].is_null());
    let explicit = one(
        r#"{"sentenceLength":{"minimumSentences":1},"baselines":{"sentenceLength":{"mean":1}}}"#,
    )
    .unwrap();
    assert_eq!(
        explicit["sentenceLength"]["groups"]["all"]["meanDifference"],
        explicit["sentenceLength"]["groups"]["all"]["mean"]
            .as_f64()
            .unwrap()
            - 1.0
    );
    let none = one("{}").unwrap();
    assert!(none["sentences"].as_array().unwrap().is_empty());
    assert_eq!(none["stats"]["sentences"]["all"], 4);

    assert!(matches!(
        one(r#"{"sentenceLength":{"minimumSentneces":1}}"#),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        one(r#"{"typo":{}}"#),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        one(r#"{"baselines":"published"}"#),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        one(r#"{"baselines":{"sentenceStarts":{"universalPercent":{"Noun":1}}}}"#),
        Err(StyleError::InvalidBaseline { .. })
    ));
    let compact = DocumentOptions::new().with_token_details(false);
    assert_eq!(
        run_json(tokenizer(), resources(), text, None, &compact).unwrap(),
        run_json(tokenizer(), resources(), text, None, &options).unwrap()
    );
}
