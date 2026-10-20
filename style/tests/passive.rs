use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::{
    passive::{self, PassiveAuxiliary, PassiveBaseline, PassiveOptions},
    PercentileBand, StyleResources,
};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}
fn document(text: &str) -> writemorebetter_style::Document {
    analyze_document(tokenizer(), text, &DocumentOptions::new())
}
fn passives(text: &str) -> passive::PassiveReport {
    passive::report(&document(text), &PassiveOptions::new(), None).unwrap()
}
fn matched(text: &str) -> Vec<String> {
    passives(text)
        .passives
        .iter()
        .map(|m| text[m.span.byte.clone()].to_owned())
        .collect()
}

#[test]
fn be_and_get_passives() {
    for (text, span, auxiliary, progressive, agent) in [
        (
            "He was followed.",
            "was followed",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "They were quickly killed.",
            "were quickly killed",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "It had never been opened.",
            "been opened",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "We were told nothing.",
            "were told",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "The book was written by her.",
            "was written",
            PassiveAuxiliary::Be,
            false,
            true,
        ),
        (
            "She was frightened by the noise.",
            "was frightened",
            PassiveAuxiliary::Be,
            false,
            true,
        ),
        (
            "Was he taken?",
            "Was he taken",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "It is used to cut rope.",
            "is used",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "The door will be closed.",
            "be closed",
            PassiveAuxiliary::Be,
            false,
            false,
        ),
        (
            "She got fired.",
            "got fired",
            PassiveAuxiliary::Get,
            false,
            false,
        ),
        (
            "He’ll get caught.",
            "get caught",
            PassiveAuxiliary::Get,
            false,
            false,
        ),
        (
            "They got hit by a truck.",
            "got hit",
            PassiveAuxiliary::Get,
            false,
            true,
        ),
    ] {
        let report = passives(text);
        assert_eq!(report.passives.len(), 1, "{text}");
        let found = &report.passives[0];
        assert_eq!(&text[found.span.byte.clone()], span, "{text}");
        assert_eq!(found.auxiliary, auxiliary, "{text}");
        assert_eq!(found.progressive, progressive, "{text}");
        assert_eq!(found.agent, agent, "{text}");
        assert!(text[found.participle.byte.clone()].len() > 2, "{text}");
        assert!(
            span.ends_with(&text[found.participle.byte.clone()]),
            "{text}"
        );
    }
}

#[test]
fn progressive_passives() {
    for (text, span, auxiliary) in [
        (
            "He was being followed.",
            "was being followed",
            PassiveAuxiliary::Be,
        ),
        (
            "They are being watched by the police.",
            "are being watched",
            PassiveAuxiliary::Be,
        ),
        (
            "The car is getting fixed.",
            "is getting fixed",
            PassiveAuxiliary::Get,
        ),
        (
            "Being followed, she ran.",
            "Being followed",
            PassiveAuxiliary::Be,
        ),
    ] {
        let report = passives(text);
        assert_eq!(report.passives.len(), 1, "{text}");
        assert_eq!(&text[report.passives[0].span.byte.clone()], span, "{text}");
        assert_eq!(report.passives[0].auxiliary, auxiliary, "{text}");
        assert_eq!(
            report.passives[0].progressive,
            !text.starts_with("Being"),
            "{text}"
        );
    }
}

#[test]
fn adjectival_participles_and_actives_do_not_count() {
    for text in [
        "I was tired.",
        "She was married to him.",
        "He was interested in art.",
        "She was very surprised.",
        "He was so tired—exhausted, really.",
        "He got lost.",
        "She got dressed.",
        "I got used to it.",
        "She was used to waiting.",
        "She was supposed to stay.",
        "He’s gone.",
        "It’s come to this.",
        "She was arrived at last.",
        "He was acquainted with her.",
        "He was much accustomed to it.",
        "I’m done.",
        "We were finished with it.",
        "The man’s broken arm hurt.",
        "He was a broken man.",
        "She’s taken the car.",
        "He has followed her.",
        "She was home.",
        "He was… followed.",
        "There was nothing left.",
        "The rest was complicated.",
        "It just got wet.",
        "All you can do is let him go.",
    ] {
        assert_eq!(matched(text), Vec::<String>::new(), "{text}");
    }
    // A by-agent turns a listed adjectival participle back into a passive; a degree word does not.
    assert_eq!(matched("She was worried by the news."), ["was worried"]);
    assert_eq!(
        matched("She was very worried by the news."),
        Vec::<String>::new()
    );
    // `done`/`finished` after a non-personal subject stay passive.
    assert_eq!(matched("It was done."), ["was done"]);
    assert_eq!(matched("I was given it."), ["was given"]);
    assert_eq!(matched("It was finished by noon."), ["was finished"]);
}

#[test]
fn typography_and_utf8_spans() {
    for (text, expected) in [
        ("She’s been seen.", vec!["’s been seen"]),
        ("It wasn’t stolen.", vec!["wasn’t stolen"]),
        ("It wasn't stolen.", vec!["wasn't stolen"]),
        ("He was watched—followed—by them.", vec!["was watched"]),
        ("He was followed… by them.", vec!["was followed"]),
        ("She was\u{2028}followed home.", vec!["was\u{2028}followed"]),
        ("“I was followed,” she said.", vec!["was followed"]),
        ("‘We were robbed,’ he said.", vec!["were robbed"]),
    ] {
        assert_eq!(matched(text), expected, "{text}");
    }
    let report = passives("He was followed… by them.");
    assert!(!report.passives[0].agent);
    let text = "“Ça va?” “I was followed—again.”";
    let report = passives(text);
    let span = &report.passives[0].span;
    let utf16: Vec<u16> = text.encode_utf16().collect();
    assert_eq!(
        String::from_utf16(&utf16[span.utf16.clone()]).unwrap(),
        "was followed"
    );
    assert_eq!(text[span.byte.clone()].chars().count(), span.char.len());
}

#[test]
fn narration_and_dialogue_rates() {
    let text = "He was followed. She walked home. The door was closed by the wind. It rained.\n\n\
                “I was robbed,” she said. “We were seen.” “Nothing happened.”";
    let report = passive::report(
        &document(text),
        &PassiveOptions::new().with_minimum_sentences(1),
        None,
    )
    .unwrap();
    let modes: Vec<_> = report.passives.iter().map(|m| m.mode).collect();
    assert_eq!(
        modes,
        [
            Mode::Narration,
            Mode::Narration,
            Mode::Dialogue,
            Mode::Dialogue
        ]
    );
    assert_eq!(report.groups.narration.sentences, 4);
    assert_eq!(report.groups.dialogue.sentences, 3);
    assert_eq!(report.groups.all.passives, 4);
    assert_eq!(report.groups.narration.per_hundred_sentences, Some(50.0));
    assert!((report.groups.dialogue.per_hundred_sentences.unwrap() - 200.0 / 3.0).abs() < 1e-9);
    assert_eq!(report.passives[2].sentence, 4);
    assert!(report.groups.all.band.is_none() && !report.groups.all.highlighted);
}

#[test]
fn sample_floors_and_bands() {
    for (count, shown, low) in [
        (0, false, true),
        (49, false, true),
        (50, true, true),
        (200, true, false),
    ] {
        let doc = document(&"He was followed. ".repeat(count));
        let result = passive::report(&doc, &PassiveOptions::new(), None).unwrap();
        assert_eq!(result.groups.all.sentences, count);
        assert_eq!(result.groups.all.passives, count);
        assert_eq!(result.groups.all.per_hundred_sentences.is_some(), shown);
        assert_eq!(result.groups.all.low_confidence, low);
        assert!(result.groups.dialogue.per_hundred_sentences.is_none());
    }
    let doc = document(&"He was followed. She slept. ".repeat(30));
    let band = PassiveBaseline::new()
        .with_band(None, PercentileBand::new(5.0, 20.0))
        .with_band(Some(Mode::Narration), PercentileBand::new(40.0, 60.0));
    let result = passive::report(&doc, &PassiveOptions::new(), Some(&band)).unwrap();
    assert_eq!(result.groups.all.per_hundred_sentences, Some(50.0));
    assert!(result.groups.all.highlighted);
    assert!(!result.groups.narration.highlighted);
    assert_eq!(
        result.groups.narration.band,
        Some(PercentileBand::new(40.0, 60.0))
    );
    assert!(result.groups.dialogue.band.is_none() && !result.groups.dialogue.highlighted);
    let invalid = PassiveBaseline::new().with_band(None, PercentileBand::new(20.0, 5.0));
    assert!(matches!(
        passive::report(&doc, &PassiveOptions::new(), Some(&invalid)),
        Err(StyleError::InvalidBaseline { .. })
    ));
}

#[test]
fn options_baselines_and_retained_document() {
    let compact = analyze_document(
        tokenizer(),
        "He was followed.",
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        passive::report(&compact, &PassiveOptions::new(), None),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
    let shared = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        &"He was followed. She slept. ".repeat(30),
        &DocumentOptions::new(),
    );
    let bundled = shared.passive(None, None).unwrap();
    assert_eq!(
        bundled,
        passive::report(
            shared.document(),
            &PassiveOptions::new(),
            PassiveBaseline::bundled().as_ref()
        )
        .unwrap()
    );
    assert_eq!(bundled, shared.passive(None, Some("bundled")).unwrap());
    assert!(bundled.groups.narration.band.is_some());
    let none = shared.passive(None, Some("none")).unwrap();
    assert!(none.groups.all.band.is_none() && !none.groups.all.highlighted);
    let custom = shared
        .passive(
            Some(r#"{"minimum_sentences": 10}"#),
            Some(r#"{"bands": {"narration": {"p5": 1, "p95": 2}}}"#),
        )
        .unwrap();
    assert!(custom.groups.narration.highlighted);
    assert!(custom.groups.all.band.is_none());
    assert!(shared
        .passive(Some(r#"{"minimumSentences": 1}"#), None)
        .is_ok());
    assert!(matches!(
        shared.passive(Some(r#"{"unknown": 1}"#), None),
        Err(StyleError::InvalidOptions { .. })
    ));
    assert!(matches!(
        shared.passive(None, Some(r#"{"bands": {}, "extra": 1}"#)),
        Err(StyleError::InvalidBaseline { .. })
    ));
    assert!(matches!(
        shared.passive(
            None,
            Some(r#"{"bands": {"all": {"p5": 1, "p95": 2, "p50": 1}}}"#)
        ),
        Err(StyleError::InvalidBaseline { .. })
    ));
    let json = writemorebetter_style::reports::to_utf16_json(&bundled);
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        value["passives"][0]["span"],
        serde_json::json!({"start": 3, "end": 15})
    );
    assert_eq!(value["passives"][0]["auxiliary"], "be");
    assert!(value["groups"]["narration"]["perHundredSentences"].is_number());
}
