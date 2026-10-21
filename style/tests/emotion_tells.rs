use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::emotion_tells::{
    self, EmotionTellPattern, EmotionTellsOptions, EmotionTellsReport,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report_with(text: &str, options: &EmotionTellsOptions) -> EmotionTellsReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    emotion_tells::report(&document, options).unwrap()
}

fn report(text: &str) -> EmotionTellsReport {
    report_with(text, &EmotionTellsOptions::new())
}

fn found(text: &str) -> Vec<(String, String, EmotionTellPattern)> {
    report(text)
        .tells
        .iter()
        .map(|tell| {
            (
                text[tell.span.byte.clone()].to_owned(),
                tell.word.clone(),
                tell.pattern,
            )
        })
        .collect()
}

#[test]
fn named_emotions_in_narration() {
    use EmotionTellPattern::*;
    for (text, span, word, pattern) in [
        ("She was afraid.", "was afraid", "afraid", Predicate),
        ("He felt angry.", "felt angry", "angry", Predicate),
        (
            "He grew anxious as the hours passed.",
            "grew anxious",
            "anxious",
            Predicate,
        ),
        (
            "They looked worried.",
            "looked worried",
            "worried",
            Predicate,
        ),
        (
            "She was terrified of the dark.",
            "was terrified",
            "terrified",
            Predicate,
        ),
        (
            "She was startled by the noise.",
            "was startled",
            "startle",
            Predicate,
        ),
        ("He seemed upset.", "seemed upset", "upset", Predicate),
        ("It made her angry.", "made her angry", "angry", Predicate),
        (
            "She was tired and afraid.",
            "was tired and afraid",
            "afraid",
            Predicate,
        ),
        (
            "She was in love with him.",
            "was in love",
            "love",
            Predicate,
        ),
        (
            "She felt guilty about it.",
            "felt guilty",
            "guilty",
            Predicate,
        ),
        (
            "She was afraid of heights.",
            "was afraid",
            "afraid",
            Predicate,
        ),
        (
            "She felt a surge of relief.",
            "felt a surge of relief",
            "relief",
            Object,
        ),
        (
            "She felt a sudden pang of guilt.",
            "felt a sudden pang of guilt",
            "guilt",
            Object,
        ),
        (
            "He was filled with dread.",
            "filled with dread",
            "dread",
            Object,
        ),
        ("Fear washed over her.", "Fear washed", "fear", Subject),
        (
            "Anger surged through him.",
            "Anger surged",
            "anger",
            Subject,
        ),
        (
            "A wave of panic rose in her chest.",
            "A wave of panic rose",
            "panic",
            Subject,
        ),
        // After *’d*, nlprule reads *felt* as the verb *to felt*.
        ("She’d felt angry.", "felt angry", "angry", Predicate),
        ("She'd felt angry.", "felt angry", "angry", Predicate),
        ("He’d felt fear.", "felt fear", "fear", Object),
        (
            "He’d felt a pang of guilt.",
            "felt a pang of guilt",
            "guilt",
            Object,
        ),
        // A noun followed by a verb opens a that-less clause; the adjective isn't attributive.
        (
            "She was afraid things would go wrong.",
            "was afraid",
            "afraid",
            Predicate,
        ),
        ("He was glad nobody saw.", "was glad", "glad", Predicate),
        (
            "She was a little afraid.",
            "was a little afraid",
            "afraid",
            Predicate,
        ),
        (
            "She looked a bit worried.",
            "looked a bit worried",
            "worried",
            Predicate,
        ),
        // Coordinated feelings on one verb are one tell covering both.
        (
            "She was angry and afraid.",
            "was angry and afraid",
            "angry",
            Predicate,
        ),
        ("He felt hurt.", "felt hurt", "hurt", Predicate),
        ("His pride was hurt.", "was hurt", "hurt", Predicate),
        (
            "She was a little relieved by his smile.",
            "was a little relieved",
            "relieve",
            Predicate,
        ),
        // First-person present narration, not a hedge.
        ("I am afraid.", "am afraid", "afraid", Predicate),
        (
            "I’m afraid. The door creaks open.",
            "’m afraid",
            "afraid",
            Predicate,
        ),
        // A relative *that* is not a dummy subject.
        (
            "The man that was afraid ran away.",
            "was afraid",
            "afraid",
            Predicate,
        ),
        (
            "The man who was afraid ran away.",
            "was afraid",
            "afraid",
            Predicate,
        ),
    ] {
        let tells = found(text);
        assert_eq!(tells.len(), 1, "{text}: {tells:?}");
        assert_eq!(
            tells[0],
            (span.to_owned(), word.to_owned(), pattern),
            "{text}"
        );
    }
}

#[test]
fn idioms_metaphors_and_other_senses_are_not_tells() {
    for text in [
        // Hedges and formulas.
        "I’m afraid I can’t help you.",
        "I'm afraid I can't help you.",
        "I’m afraid the door is locked.",
        "I am afraid that it is too late.",
        "I’m sorry.",
        "I’d be glad to help.",
        "She was pleased to call it a garden.",
        "He was happy to say that it was done.",
        // Other senses.
        "As far as she was concerned, it was over.",
        "Where their two selves only were concerned, nothing changed.",
        "The jury decided he was guilty.",
        "She was moved to the hospital.",
        "He was badly hurt in the crash.",
        "She was hurt in the fall.",
        "I had more, I am afraid, than I should.",
        "He was content to wait.",
        "She was content with less.",
        // Attributive and non-predicate uses.
        "She has a happy face.",
        "He kissed his mother’s troubled forehead.",
        "Her fear of the dark was legendary.",
        "The fear spread through the city.",
        "It was sad.",
        "That was sad.",
        // Feelings named by verbs, and sensations, are not named emotions.
        "She feared him.",
        "He loved her.",
        "He felt the warmth of the fire.",
        "A chill ran down her spine.",
    ] {
        assert!(found(text).is_empty(), "{text}: {:?}", found(text));
    }
}

#[test]
fn dialogue_is_skipped_unless_included() {
    for text in [
        "“I’m afraid of heights,” he said.",
        "\"I was so angry,\" she said.",
        "“I’m afraid so,” she said.",
    ] {
        assert!(found(text).is_empty(), "{text}");
    }
    let options = EmotionTellsOptions::new().with_include_dialogue(true);
    let text = "“I was so angry,” she said. He felt calm.";
    let result = report_with(text, &options);
    let modes: Vec<_> = result.tells.iter().map(|tell| tell.mode).collect();
    assert_eq!(modes, [Mode::Dialogue, Mode::Narration]);
    assert_eq!(result.groups.dialogue.tells, 1);
    assert_eq!(result.groups.narration.tells, 1);
    assert_eq!(result.groups.all.tells, 2);
    // The idioms stay excluded inside dialogue.
    assert!(report_with("“I’m afraid so,” she said.", &options)
        .tells
        .is_empty());
    // An untagged quotation is still a character's words.
    let untagged = "Mrs. Folliott looked up. “It is charity. I should be quite miserable.”";
    assert!(report(untagged).tells.is_empty());
    assert_eq!(report_with(untagged, &options).tells.len(), 1);
}

#[test]
fn utf8_text_keeps_spans_on_char_boundaries() {
    for (text, expected) in [
        (
            "“Go,” she said. She was afraid—then angry.",
            vec!["was afraid"],
        ),
        ("She’d been afraid of him for years.", vec!["been afraid"]),
        ("He was angry… then sad.", vec!["was angry"]),
        (
            "She was worried—terribly worried—about him.",
            vec!["was worried"],
        ),
        (
            "She stood still.\u{2028}She felt a surge of relief.",
            vec!["felt a surge of relief"],
        ),
        ("Ça va. Zoë felt angry.", vec!["felt angry"]),
        (
            "She stood still.\u{2028}Fear washed over her. Anger surged through him.",
            vec!["Fear washed", "Anger surged"],
        ),
        ("She’d felt angry—then afraid…", vec!["felt angry"]),
    ] {
        let result = report(text);
        let spans: Vec<_> = result
            .tells
            .iter()
            .map(|tell| {
                assert!(text.is_char_boundary(tell.span.byte.start));
                assert!(text.is_char_boundary(tell.span.byte.end));
                let chars: String = text
                    .chars()
                    .skip(tell.span.char.start)
                    .take(tell.span.char.end - tell.span.char.start)
                    .collect();
                assert_eq!(chars, text[tell.span.byte.clone()]);
                assert!(
                    tell.span.byte.start <= tell.emotion.byte.start
                        && tell.emotion.byte.end <= tell.span.byte.end
                );
                &text[tell.span.byte.clone()]
            })
            .collect();
        assert_eq!(spans, expected, "{text}");
    }
}

#[test]
fn groups_count_words_and_rates() {
    let result = report("She was afraid. He walked home slowly.");
    assert_eq!(result.groups.narration.tells, 1);
    assert_eq!(result.groups.narration.words, 7);
    assert_eq!(result.groups.all.words, 7);
    assert_eq!(result.groups.dialogue.words, 0);
    assert_eq!(result.groups.dialogue.per_thousand_words, None);
    let rate = result.groups.narration.per_thousand_words.unwrap();
    assert!((rate - 1000.0 / 7.0).abs() < 1e-9);
    assert!(report("").tells.is_empty());
    assert_eq!(report("").groups.all.per_thousand_words, None);
}

#[test]
fn options_bindings_and_errors() {
    // Astral characters make UTF-16 offsets differ from char offsets.
    let text = "😀 “Hi,” she said. 😀 Zoë was afraid.";
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let json: serde_json::Value =
        serde_json::from_str(&to_utf16_json(&retained.emotion_tells(None).unwrap())).unwrap();
    let start = text.encode_utf16().count() - "was afraid.".encode_utf16().count();
    assert_eq!(json["tells"][0]["span"]["start"], start);
    assert_eq!(json["tells"][0]["span"]["end"], start + "was afraid".len());
    assert_eq!(json["tells"][0]["pattern"], "predicate");
    assert_eq!(json["tells"][0]["mode"], "narration");
    assert!(json["groups"]["narration"]["perThousandWords"].is_number());
    assert!(retained
        .emotion_tells(Some("{\"includeDialogue\":true}"))
        .is_ok());
    assert!(retained
        .emotion_tells(Some("{\"include_dialogue\":true}"))
        .is_ok());
    assert!(matches!(
        retained.emotion_tells(Some("{\"typo\":true}")),
        Err(StyleError::InvalidOptions { .. })
    ));
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        emotion_tells::report(&compact, &EmotionTellsOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn bundled_lexicon_carries_the_wordnet_notice() {
    let data = include_str!("../data/emotions/emotions.tsv");
    assert!(data.contains("WordNet 3.0 Copyright 2006 by Princeton University"));
    assert!(data.contains("style/data/licenses/WordNet-LICENSE"));
    let license = include_str!("../data/licenses/WordNet-LICENSE");
    assert!(license.contains("WordNet Release 3.0"));
    assert!(license.contains("Permission to use, copy, modify and distribute"));
    let rows: Vec<_> = data.lines().filter(|line| !line.starts_with('#')).collect();
    assert!(rows.len() > 500);
    for row in &rows {
        let (word, kinds) = row.split_once('\t').expect("word and kinds");
        assert!(!word.is_empty() && word == word.to_lowercase());
        assert!(
            !kinds.is_empty() && kinds.chars().all(|c| "anv".contains(c)),
            "{row}"
        );
    }
    let mut sorted = rows.clone();
    sorted.sort();
    assert_eq!(rows, sorted);
}
