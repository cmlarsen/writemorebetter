//! The voice layer (#353). All fixture text is written for these tests (CC0).

use std::sync::{Arc, OnceLock};

use nlprule::Tokenizer;
use writemorebetter_style::reports::voices::{self, VoicesOptions};
use writemorebetter_style::reports::{DocumentStats, StyleResources};
use writemorebetter_style::{
    analyze_document, Document, DocumentOptions, MarkedSpan, MarkedVoice, Mode, OffsetUnits, Span,
    StyleDocument, Voice, VoiceShift, VoiceSource,
};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn document_with(text: &str, options: &DocumentOptions) -> Document {
    analyze_document(tokenizer(), text, options)
}

fn analyze(text: &str) -> Document {
    document_with(text, &DocumentOptions::new())
}

/// Checks a span's byte, char and UTF-16 ranges against the source and returns its text.
fn text_of(text: &str, span: &Span) -> String {
    let by_byte = text
        .get(span.byte.clone())
        .expect("byte range on char boundaries");
    let by_char: String = text
        .chars()
        .skip(span.char.start)
        .take(span.char.len())
        .collect();
    let by_utf16 = String::from_utf16(
        &text
            .encode_utf16()
            .skip(span.utf16.start)
            .take(span.utf16.len())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(by_byte, by_char, "char range of {by_byte:?}");
    assert_eq!(by_byte, by_utf16, "utf16 range of {by_byte:?}");
    by_byte.to_owned()
}

#[derive(Debug, PartialEq)]
struct Found {
    text: String,
    voice: Voice,
    source: VoiceSource,
    tag: Option<String>,
    shift: Option<VoiceShift>,
}

fn found_in(text: &str, document: &Document) -> Vec<Found> {
    for pair in document.voices.windows(2) {
        assert!(
            pair[0].span.byte.end <= pair[1].span.byte.start,
            "sorted, non-overlapping"
        );
    }
    document
        .voices
        .iter()
        .map(|span| {
            assert_ne!(span.voice, Voice::Narration);
            Found {
                text: text_of(text, &span.span),
                voice: span.voice,
                source: span.source,
                tag: span.tag.as_ref().map(|tag| text_of(text, tag)),
                shift: span.shift,
            }
        })
        .collect()
}

fn found(text: &str) -> Vec<Found> {
    found_in(text, &analyze(text))
}

fn tagged(text: &str, voice: Voice, tag: &str, shift: VoiceShift) -> Found {
    Found {
        text: text.to_owned(),
        voice,
        source: VoiceSource::Tag,
        tag: Some(tag.to_owned()),
        shift: Some(shift),
    }
}

fn thought(text: &str, tag: &str, shift: VoiceShift) -> Found {
    tagged(text, Voice::Thought, tag, shift)
}

#[test]
fn trailing_tag_with_curly_apostrophe() {
    let text = "I’m done with this, she thought, and reached for the door.";
    let document = analyze(text);
    // *I’m* is first person and present against a third-person past tag.
    assert_eq!(
        found_in(text, &document),
        [thought(
            "I’m done with this",
            "she thought",
            VoiceShift::Both
        )]
    );
    let reached = text.find("reached").unwrap();
    assert!(document.is_narration_at(reached));
    let tag = text.find("she thought").unwrap();
    assert!(document.is_narration_at(tag));
    assert!(document.in_voice_tag(tag));
    assert!(!document.in_voice_tag(reached));
    assert_eq!(document.voice_at(0), Voice::Thought);
}

#[test]
fn first_person_past_frame_needs_a_tense_shift() {
    assert_eq!(
        found("He’s lying, I thought."),
        [thought("He’s lying", "I thought", VoiceShift::Tense)]
    );
    assert_eq!(found("He was lying, I thought."), []);
    assert_eq!(
        found("He's lying, I thought.")[0].shift,
        Some(VoiceShift::Tense)
    );
}

#[test]
fn bare_tag_after_a_question() {
    let expected = [thought("Where is he?", "I wondered", VoiceShift::Tense)];
    assert_eq!(found("Where is he? I wondered."), expected);
    // U+2028 is a line break inside the paragraph; the span stops before it.
    assert_eq!(found("Where is he?\u{2028}I wondered."), expected);
    for separator in ["\n", "\r\n", "\u{2029}"] {
        assert_eq!(
            found(&format!("Where is he?{separator}I wondered.")),
            [],
            "{separator:?}"
        );
    }
}

#[test]
fn ellipsis_before_the_tag() {
    for text in [
        "Maybe it isn’t too late… she thought.",
        "Maybe it isn’t too late… She thought.",
    ] {
        let found = found(text);
        assert_eq!(found.len(), 1, "{text}");
        assert_eq!(found[0].text, "Maybe it isn’t too late…");
        assert_eq!(found[0].voice, Voice::Thought);
        assert_eq!(found[0].shift, Some(VoiceShift::Tense));
    }
}

#[test]
fn leading_tag() {
    assert_eq!(
        found("She thought, I can’t do this anymore."),
        [thought(
            "I can’t do this anymore.",
            "She thought",
            VoiceShift::Both
        )]
    );
    assert_eq!(found("She thought that I couldn’t do it."), []);
    assert_eq!(found("She wondered where he was."), []);
}

#[test]
fn split_tag_with_em_dashes_and_commas() {
    for (text, first, second) in [
        (
            "I have to get out—she thought—before he wakes.",
            "I have to get out",
            "before he wakes.",
        ),
        (
            "I have to get out, she thought, before he wakes.",
            "I have to get out",
            "before he wakes.",
        ),
    ] {
        let document = analyze(text);
        let found = found_in(text, &document);
        assert_eq!(
            found.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(),
            [first, second],
            "{text}"
        );
        assert!(found
            .iter()
            .all(|f| f.tag.as_deref() == Some("she thought")));
        assert!(document.is_narration_at(text.find("she").unwrap()));
    }
}

#[test]
fn reflexive_speech_verbs_tag_thought() {
    assert_eq!(
        found("You can do this, she told herself."),
        [thought(
            "You can do this",
            "she told herself",
            VoiceShift::Tense
        )]
    );
    assert_eq!(
        found("Keep moving, he muttered to himself."),
        [thought(
            "Keep moving",
            "he muttered to himself",
            VoiceShift::Tense
        )]
    );
}

#[test]
fn unquoted_speech_with_a_speech_tag() {
    assert_eq!(
        found("Keep moving, he muttered."),
        [tagged(
            "Keep moving",
            Voice::Speech,
            "he muttered",
            VoiceShift::Tense
        )]
    );
    assert_eq!(
        found("I’m leaving, he said."),
        [tagged(
            "I’m leaving",
            Voice::Speech,
            "he said",
            VoiceShift::Both
        )]
    );
    assert_eq!(found("She was late, he said."), []);
    // The bare anchor is left to quote detection for speech.
    assert_eq!(found("Where is he? he said."), []);
    let off = DocumentOptions::new().with_detect_unquoted_speech(false);
    for text in [
        "Keep moving, he muttered.",
        "I’m leaving, he said.",
        "She was late, he said.",
    ] {
        assert_eq!(found_in(text, &document_with(text, &off)), [], "{text}");
    }
}

#[test]
fn imperatives_but_not_noun_fragments() {
    assert_eq!(
        found("Get out now, she thought."),
        [thought("Get out now", "she thought", VoiceShift::Tense)]
    );
    assert_eq!(found("Run-down shack, she thought."), []);
}

#[test]
fn free_indirect_and_agreeing_clauses_stay_narration() {
    for text in [
        "She had to get out. Now. Before he woke.",
        "She was a fool, she thought.",
        "He would never forgive her, she realized.",
        "I’m late, she knew.",
        "I’m late, she felt.",
        "I walk to the door. It’s late, I think. I open it.",
        "It was, I suppose, inevitable.",
        "A thought struck her: I’m alone.",
        "Having thought it over, she left.",
        "Idiot, she thought.",
        "Too late, he realized.",
        "You’re late, you think.",
        "It’s late, it thought.",
    ] {
        assert_eq!(found(text), [], "{text}");
    }
}

#[test]
fn roman_numeral_i_is_not_first_person() {
    let found = found("Chapter I begins, she thought.");
    assert!(
        found
            .iter()
            .all(|f| !matches!(f.shift, Some(VoiceShift::Person | VoiceShift::Both))),
        "{found:?}"
    );
}

#[test]
fn expansion_takes_neighbouring_thought_sentences() {
    let text = "Where is he? Why doesn’t he call? I wondered. She set down the phone.";
    assert_eq!(
        found(text),
        [thought(
            "Where is he? Why doesn’t he call?",
            "I wondered",
            VoiceShift::Tense
        )]
    );
    assert_eq!(
        found("She waited. Where is he? I wondered."),
        [thought("Where is he?", "I wondered", VoiceShift::Tense)]
    );
    // No thought evidence, so a present-tense slip stays narration for #355 to see.
    let text = "The room is dark. Where is he? I wondered.";
    let document = analyze(text);
    assert_eq!(
        found_in(text, &document),
        [thought("Where is he?", "I wondered", VoiceShift::Tense)]
    );
    assert!(document.is_narration_at(0));
}

#[test]
fn expansion_is_capped() {
    let text = "Is it now? Is it here? Where is he? Why doesn’t he call? I wondered.";
    assert_eq!(
        found(text),
        [thought(
            "Is it here? Where is he? Why doesn’t he call?",
            "I wondered",
            VoiceShift::Tense
        )]
    );
}

#[test]
fn quoted_thought_is_relabelled_without_changing_mode() {
    let text = "“Fine,” he thought.";
    let document = analyze(text);
    assert_eq!(
        found_in(text, &document),
        [Found {
            text: "“Fine,”".into(),
            voice: Voice::Thought,
            source: VoiceSource::Quote,
            tag: None,
            shift: None,
        }]
    );
    assert_eq!(document.mode_at(1), Mode::Dialogue);
    assert!(document.speech[0]);
}

#[test]
fn curly_apostrophes_are_not_quotes() {
    let text = "‘Tis late, she thought.";
    let document = analyze(text);
    assert!(document.dialogue.is_empty());
    assert_eq!(
        found_in(text, &document),
        [thought("‘Tis late", "she thought", VoiceShift::Tense)]
    );
    let text = "She called it ‘grub,’ I’m sure, he thought.";
    let document = analyze(text);
    assert_eq!(
        document
            .dialogue
            .iter()
            .map(|span| text_of(text, span))
            .collect::<Vec<_>>(),
        ["‘grub,’"]
    );
    let found = found_in(text, &document);
    assert_eq!(found[0].voice, Voice::Quoted);
    assert_eq!(found[0].text, "‘grub,’");
    assert!(found[1..].iter().all(|f| f.source == VoiceSource::Tag));
}

#[test]
fn quoted_speech_and_unquoted_thought_side_by_side() {
    let text = "“I’m done,” she said. I’m done, she thought.";
    assert_eq!(
        found(text),
        [
            Found {
                text: "“I’m done,”".into(),
                voice: Voice::Speech,
                source: VoiceSource::Quote,
                tag: None,
                shift: None,
            },
            thought("I’m done", "she thought", VoiceShift::Both),
        ]
    );
}

#[test]
fn quoted_terms_are_not_narration_but_keep_their_mode() {
    let text = "She pressed “Send” and waited.";
    let document = analyze(text);
    let send = text.find("Send").unwrap();
    assert_eq!(document.voice_at(send), Voice::Quoted);
    assert!(!document.is_narration_at(send));
    assert_eq!(document.mode_at(send), Mode::Narration);
    assert!(document.is_narration_at(text.find("waited").unwrap()));
}

fn utf16_offset(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

fn char_offset(text: &str, byte: usize) -> usize {
    text[..byte].chars().count()
}

const LETTER: &str = "😀 She read the note. Dear Sam, I am gone. I’m done, she thought.";

fn letter_range() -> std::ops::Range<usize> {
    let start = LETTER.find("Dear").unwrap();
    start..LETTER.find(" I’m").unwrap()
}

#[test]
fn caller_spans_in_every_unit() {
    let bytes = letter_range();
    let convert = |units: OffsetUnits| {
        let (start, end) = match units {
            OffsetUnits::Byte => (bytes.start, bytes.end),
            OffsetUnits::Char => (
                char_offset(LETTER, bytes.start),
                char_offset(LETTER, bytes.end),
            ),
            _ => (
                utf16_offset(LETTER, bytes.start),
                utf16_offset(LETTER, bytes.end),
            ),
        };
        let options = DocumentOptions::new().with_marked_spans(
            units,
            vec![MarkedSpan::new(start..end, MarkedVoice::Embedded)],
        );
        document_with(LETTER, &options)
    };
    let documents = [
        convert(OffsetUnits::Byte),
        convert(OffsetUnits::Char),
        convert(OffsetUnits::Utf16),
    ];
    for document in &documents {
        assert!(document.ignored_marked_spans.is_empty());
        let found = found_in(LETTER, document);
        assert_eq!(found[0].text, "Dear Sam, I am gone.");
        assert_eq!(found[0].voice, Voice::Embedded);
        assert_eq!(found[0].source, VoiceSource::Caller);
        assert_eq!(document.voices[0].marked, Some(0));
        assert_eq!(
            found[1],
            thought("I’m done", "she thought", VoiceShift::Both)
        );
        assert_eq!(document.voices, documents[0].voices);
    }
}

#[test]
fn invalid_caller_spans_are_listed() {
    let length16 = LETTER.encode_utf16().count();
    let options = DocumentOptions::new().with_marked_spans(
        OffsetUnits::Utf16,
        vec![
            MarkedSpan::new(1..5, MarkedVoice::Excluded), // splits the emoji's surrogate pair
            MarkedSpan::new(3..length16 + 1, MarkedVoice::Excluded),
            MarkedSpan::new(4..4, MarkedVoice::Excluded),
            serde_json::from_str(r#"{"start":9,"end":4,"voice":"excluded"}"#).unwrap(),
            MarkedSpan::new(0..2, MarkedVoice::Excluded), // the whole emoji
        ],
    );
    let document = document_with(LETTER, &options);
    assert_eq!(document.ignored_marked_spans, [0, 1, 2, 3]);
    assert_eq!(text_of(LETTER, &document.voices[0].span), "😀");
    let options = DocumentOptions::new().with_marked_spans(
        OffsetUnits::Byte,
        vec![MarkedSpan::new(1..6, MarkedVoice::Excluded)],
    );
    assert_eq!(document_with(LETTER, &options).ignored_marked_spans, [0]);
    // Rust with no units means bytes.
    let mut options = DocumentOptions::new();
    options.marked_spans = vec![MarkedSpan::new(0..4, MarkedVoice::Excluded)];
    assert_eq!(
        text_of(LETTER, &document_with(LETTER, &options).voices[0].span),
        "😀"
    );
}

fn marked(text: &str, spans: Vec<(&str, MarkedVoice)>) -> Document {
    let spans = spans
        .into_iter()
        .map(|(needle, voice)| {
            let start = text.find(needle).unwrap();
            MarkedSpan::new(start..start + needle.len(), voice)
        })
        .collect();
    document_with(
        text,
        &DocumentOptions::new().with_marked_spans(OffsetUnits::Byte, spans),
    )
}

#[test]
fn caller_spans_win() {
    // Caller narration removes a detected thought.
    let text = "I’m done with this, she thought.";
    let document = marked(text, vec![("I’m done with this", MarkedVoice::Narration)]);
    assert_eq!(found_in(text, &document), []);
    // The innermost caller span wins; ties go to the later index.
    let text = "She read it twice. Dear Sam, I am gone. Do not follow me.";
    let document = marked(
        text,
        vec![
            (
                "Dear Sam, I am gone. Do not follow me.",
                MarkedVoice::Embedded,
            ),
            ("I am gone.", MarkedVoice::Thought),
            ("I am gone.", MarkedVoice::Quotation),
        ],
    );
    let found = found_in(text, &document);
    assert_eq!(
        found
            .iter()
            .map(|f| (f.text.as_str(), f.voice))
            .collect::<Vec<_>>(),
        [
            ("Dear Sam, ", Voice::Embedded),
            ("I am gone.", Voice::Quotation),
            (" Do not follow me.", Voice::Embedded),
        ]
    );
    assert_eq!(document.voices[1].marked, Some(2));
    assert_eq!(document.voices[0].marked, Some(0));
    // A caller span over a quote wins for its range.
    let text = "“Run now,” she said.";
    let document = marked(text, vec![("Run", MarkedVoice::Excluded)]);
    assert_eq!(
        found_in(text, &document)
            .iter()
            .map(|f| (f.text.as_str(), f.voice, f.source))
            .collect::<Vec<_>>(),
        [
            ("“", Voice::Speech, VoiceSource::Quote),
            ("Run", Voice::Excluded, VoiceSource::Caller),
            (" now,”", Voice::Speech, VoiceSource::Quote),
        ]
    );
    assert_eq!(document.mode_at(1), Mode::Dialogue);
}

#[test]
fn detection_never_crosses_a_caller_span() {
    let text = "I’m done with this, she thought.";
    let document = marked(text, vec![("she", MarkedVoice::Embedded)]);
    assert_eq!(
        found_in(text, &document)
            .iter()
            .map(|f| f.source)
            .collect::<Vec<_>>(),
        [VoiceSource::Caller]
    );
}

#[test]
fn detect_thought_off_keeps_caller_spans() {
    let text = "I’m done with this, she thought. Dear Sam.";
    let start = text.find("Dear").unwrap();
    let options = DocumentOptions::new()
        .with_detect_thought(false)
        .with_marked_spans(
            OffsetUnits::Byte,
            vec![MarkedSpan::new(start..text.len(), MarkedVoice::Embedded)],
        );
    let found = found_in(text, &document_with(text, &options));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].voice, Voice::Embedded);
}

#[test]
fn compact_documents_resolve_the_same_voices() {
    let text = "“Fine,” he thought. I’m done with this, she thought. Keep moving, he said.";
    let full = analyze(text);
    let compact = document_with(text, &DocumentOptions::new().with_token_details(false));
    assert_eq!(full.voices, compact.voices);
    assert_eq!(full.voices.len(), 3);
    let report = voices::report(&compact, &VoicesOptions::new()).unwrap();
    assert_eq!(report.spans, full.voices);
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert_eq!(retained.voices(None).unwrap(), report);
    assert!(retained.voices(Some("{\"typo\":1}")).is_err());
}

#[test]
fn narration_runs_skip_voices() {
    let text = "“Hi,” she says, and sits.";
    let document = analyze(text);
    let sentence = document.sentences().next().unwrap();
    let runs = document.narration_runs(sentence);
    assert_eq!(runs.len(), 1);
    let first = &sentence.tokens[runs[0].start].span;
    let last = &sentence.tokens[runs[0].end - 1].span;
    assert_eq!(
        text_of(text, &Span::covering(first, last)),
        "she says, and sits."
    );
    assert_eq!(
        document
            .sentence_narration_words(sentence)
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>(),
        ["she", "says", "and", "sits"]
    );
    assert!(!document.sentence_is_narration(sentence));
    let text = "I’m done, she thought, and left.";
    let document = analyze(text);
    let sentence = document.sentences().next().unwrap();
    let runs = document.narration_runs(sentence);
    let first = &sentence.tokens[runs[0].start].span;
    let last = &sentence.tokens[runs[0].end - 1].span;
    assert_eq!(
        text_of(text, &Span::covering(first, last)),
        "she thought, and left."
    );
}

#[test]
fn hard_wrapped_lines_join_a_tag_to_its_clause() {
    let text = "I have to go,\r\nshe thought.";
    let wrapped = document_with(text, &DocumentOptions::new().with_hard_wrapped(true));
    assert_eq!(
        found_in(text, &wrapped),
        [thought("I have to go", "she thought", VoiceShift::Both)]
    );
    assert_eq!(found(text), []);
}

const FIXTURES: &[&str] = &[
    "I’m done with this, she thought, and reached for the door.",
    "He’s lying, I thought.",
    "Where is he?\u{2028}I wondered.",
    "Maybe it isn’t too late… she thought.",
    "She thought, I can’t do this anymore.",
    "I have to get out—she thought—before he wakes.",
    "You can do this, she told herself.",
    "Keep moving, he muttered.",
    "“Fine,” he thought.",
    "‘Tis late, she thought.",
    "“I’m done,” she said. I’m done, she thought.",
    "She pressed “Send” and waited.",
    "Where is he? Why doesn’t he call? I wondered. She set down the phone.",
];

#[test]
fn stats_ignore_voices() {
    let off = DocumentOptions::new()
        .with_detect_thought(false)
        .with_detect_unquoted_speech(false);
    for text in FIXTURES {
        let on = serde_json::to_string(&DocumentStats::new(&analyze(text))).unwrap();
        let without =
            serde_json::to_string(&DocumentStats::new(&document_with(text, &off))).unwrap();
        assert_eq!(on, without, "{text}");
    }
}

#[test]
fn report_counts_add_up() {
    for text in FIXTURES {
        let document = analyze(text);
        let report = voices::report(&document, &VoicesOptions::new()).unwrap();
        let voice_words: usize = report.counts.values().map(|count| count.words).sum();
        assert_eq!(
            report.narration_words + voice_words,
            document.words().count(),
            "{text}"
        );
        assert_eq!(
            report
                .counts
                .values()
                .map(|count| count.spans)
                .sum::<usize>(),
            report.spans.len()
        );
        assert_eq!(report.sentences, document.sentences().count());
    }
    let text = "I’m done with this, she thought.";
    let report = voices::report(&analyze(text), &VoicesOptions::new()).unwrap();
    let json = writemorebetter_style::reports::to_utf16_json(&report);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["counts"]["thought"]["spans"], 1);
    assert_eq!(value["counts"]["thought"]["words"], 5); // I, ’m, done, with, this
    assert_eq!(
        value["spans"][0]["span"],
        serde_json::json!({"start": 0, "end": 18})
    );
    assert_eq!(
        value["spans"][0]["tag"],
        serde_json::json!({"start": 20, "end": 31})
    );
    assert_eq!(value["spans"][0]["source"], "tag");
    assert_eq!(value["spans"][0]["shift"], "both");
    assert_eq!(value["spans"][0]["marked"], serde_json::Value::Null);
    assert_eq!(value["narrationWords"], 2);
    assert_eq!(value["narrationSentences"], 0);
    assert_eq!(value["ignoredMarkedSpans"], serde_json::json!([]));
}

#[test]
fn guards_from_published_prose_shapes() {
    for text in [
        // A narrator's aside inside a frame clause.
        "He grunted, or, rather, I might say, he barked.",
        // A second predicate or clause after a leading tag.
        "The miner grunted, put his bag down and sat.",
        "He promises fairly, and I hope he means it.",
        // A perfect modal and *must* carry no present tense.
        "They must have been together, he thought, for an hour.",
        "He must go soon, she thought.",
        // Hedges and naming.
        "It is late, they say.",
        "Look at the sailor, called the mincer.",
        // Joyce-style dash dialogue with no person or tense to shift.
        "―Yes, Stephen said.",
    ] {
        assert_eq!(found(text), [], "{text}");
    }
    assert_eq!(
        found("That is what Mrs. Ramsay told me, she thought."),
        [thought(
            "That is what Mrs. Ramsay told me",
            "she thought",
            VoiceShift::Both
        )]
    );
    assert_eq!(
        found("―I don’t know, Stephen said."),
        [tagged(
            "―I don’t know",
            Voice::Speech,
            "Stephen said",
            VoiceShift::Both
        )]
    );
    // A second tag in the sentence bounds the next clause.
    assert_eq!(
        found("So it ends then, thought Ann, so it ends, she thought.")
            .iter()
            .map(|f| f.text.as_str())
            .collect::<Vec<_>>(),
        ["So it ends then", "so it ends"]
    );
}
