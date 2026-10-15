use nlprule::Tokenizer;
use serde_json::Value;
use writemorebetter_style::reports::dialogue_tags::{
    dialogue_tags, non_speech_verbs, speech_verbs, DialogueTagBaseline, DialogueTagOptions,
    DialogueTagReport, VerbClass,
};
use writemorebetter_style::{analyze_document, Document, DocumentOptions, Span};

fn tokenizer() -> Tokenizer {
    Tokenizer::new("../storage/en_tokenizer.bin").unwrap()
}

fn surface(text: &str, span: &Span) -> String {
    let byte = text.get(span.byte.clone()).unwrap().to_owned();
    let chars: String = text
        .chars()
        .skip(span.char.start)
        .take(span.char.len())
        .collect();
    let utf16 = String::from_utf16(
        &text
            .encode_utf16()
            .skip(span.utf16.start)
            .take(span.utf16.len())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(byte, chars);
    assert_eq!(byte, utf16);
    byte
}

fn report(tokenizer: &Tokenizer, text: &str, names: Vec<String>) -> (Document, DialogueTagReport) {
    let document = analyze_document(
        tokenizer,
        text,
        &DocumentOptions::new().with_skip_names(names),
    );
    let report = dialogue_tags(&document, &DialogueTagOptions::new());
    (document, report)
}

fn quotes(text: &str, document: &Document, indexes: &[usize]) -> Vec<Value> {
    indexes
        .iter()
        .map(|&index| Value::from(surface(text, &document.dialogue[index])))
        .collect()
}

fn class(class: VerbClass) -> String {
    serde_json::to_value(class)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn annotated_passages() {
    let tokenizer = tokenizer();
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/dialogue-tags.json")).unwrap();
    for fixture in fixtures.as_array().unwrap() {
        let name = fixture["name"].as_str().unwrap();
        let text = fixture["text"].as_str().unwrap();
        let (document, report) = report(&tokenizer, text, Vec::new());
        let tags: Vec<Value> = report
            .tags
            .iter()
            .map(|tag| {
                let mut value = serde_json::json!({
                    "verb": surface(text, &tag.verb),
                    "lemma": tag.lemma,
                    "class": class(tag.class),
                    "position": serde_json::to_value(tag.position).unwrap(),
                    "span": surface(text, &tag.span),
                    "dialogue": quotes(text, &document, &tag.dialogue),
                });
                assert_eq!(tag.verb_text, surface(text, &tag.verb));
                if let Some(adverb) = &tag.adverb {
                    value["adverb"] = Value::from(surface(text, adverb));
                }
                value
            })
            .collect();
        let expected = fixture.get("tags").cloned().unwrap_or(Value::Array(vec![]));
        assert_eq!(Value::Array(tags), expected, "{name}: tags");
        let beats: Vec<Value> = report
            .beats
            .iter()
            .map(|beat| {
                serde_json::json!({
                    "span": surface(text, &beat.span),
                    "dialogue": quotes(text, &document, &beat.dialogue),
                })
            })
            .collect();
        let expected = fixture
            .get("beats")
            .cloned()
            .unwrap_or(Value::Array(vec![]));
        assert_eq!(Value::Array(beats), expected, "{name}: beats");
        let runs: Vec<Value> = report
            .untagged_runs
            .iter()
            .map(|run| {
                assert_eq!(run.dialogue.len(), run.exchanges);
                serde_json::json!({"exchanges": run.exchanges, "span": surface(text, &run.span)})
            })
            .collect();
        let expected = fixture.get("runs").cloned().unwrap_or(Value::Array(vec![]));
        assert_eq!(Value::Array(runs), expected, "{name}: runs");
        if let Some(expected) = fixture.get("untagged_exchanges") {
            assert_eq!(
                report.untagged_exchanges as u64,
                expected.as_u64().unwrap(),
                "{name}: untagged exchanges"
            );
        }
    }
}

#[test]
fn counts_and_percentages() {
    let tokenizer = tokenizer();
    let text =
        "“Go,” she said.\n“Why?” he asked softly.\n“Because,” she hissed.\n“Fine,” he shrugged.";
    let (_, report) = report(&tokenizer, text, Vec::new());
    assert_eq!(report.tags.len(), 4);
    assert_eq!(report.dialogue_spans, 4);
    assert_eq!(report.tagged_spans, 4);
    assert_eq!(report.exchanges, 4);
    assert_eq!(report.said_asked, 2);
    assert_eq!(report.with_adverb, 1);
    assert_eq!(report.non_speech, 1);
    assert_eq!(report.said_asked_percent, None);
    let others: Vec<(&str, usize)> = report
        .other_verbs
        .iter()
        .map(|verb| (verb.lemma.as_str(), verb.count))
        .collect();
    assert_eq!(others, [("hiss", 1), ("shrug", 1)]);
    let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
    let baseline = DialogueTagBaseline::new(80.0, 5.0, 1.0);
    let report = dialogue_tags(
        &document,
        &DialogueTagOptions::new()
            .with_min_tags(4)
            .with_baseline(baseline.clone()),
    );
    assert_eq!(report.said_asked_percent, Some(50.0));
    assert_eq!(report.adverb_percent, Some(25.0));
    assert_eq!(report.non_speech_percent, Some(25.0));
    assert_eq!(report.baseline, Some(baseline));
}

#[test]
fn skipped_names_are_subjects_not_verbs() {
    let tokenizer = tokenizer();
    let text = "“Go,” said Will.\n“Stay,” Hope pleaded.";
    let (_, report) = report(&tokenizer, text, vec!["Will".into(), "Hope".into()]);
    let verbs: Vec<(&str, &str)> = report
        .tags
        .iter()
        .map(|tag| (tag.verb_text.as_str(), tag.lemma.as_str()))
        .collect();
    assert_eq!(verbs, [("said", "say"), ("pleaded", "plead")]);
}

#[test]
fn untagged_run_threshold_is_configurable() {
    let tokenizer = tokenizer();
    let text = "“A.”\n“B.”\n“C.”";
    let document = analyze_document(&tokenizer, text, &DocumentOptions::new());
    let report = dialogue_tags(
        &document,
        &DialogueTagOptions::new().with_max_untagged_exchanges(2),
    );
    assert_eq!(report.untagged_runs.len(), 1);
    assert_eq!(report.untagged_runs[0].exchanges, 3);
    let report = dialogue_tags(&document, &DialogueTagOptions::new());
    assert!(report.untagged_runs.is_empty());
}

#[test]
fn continued_speech_is_one_exchange() {
    let tokenizer = tokenizer();
    let text = "“First part,\n“second part,” she said.";
    let (document, report) = report(&tokenizer, text, Vec::new());
    assert_eq!(document.dialogue.len(), 1);
    assert_eq!(report.exchanges, 1);
    assert_eq!(report.tags.len(), 1);
    assert_eq!(report.tags[0].dialogue, [0]);
}

#[test]
fn verb_lists_are_disjoint() {
    let speech: Vec<_> = speech_verbs().collect();
    assert!(speech.contains(&"say") && speech.contains(&"go on"));
    for verb in non_speech_verbs() {
        assert!(!speech.contains(&verb), "{verb}");
    }
}
