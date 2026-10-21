use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::hidden_verbs::{
    self, HiddenVerbPattern, HiddenVerbsOptions, HiddenVerbsReport,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report_with(text: &str, options: &HiddenVerbsOptions) -> HiddenVerbsReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    hidden_verbs::report(&document, options).unwrap()
}

fn report(text: &str) -> HiddenVerbsReport {
    report_with(text, &HiddenVerbsOptions::new())
}

fn found(text: &str) -> Vec<(String, String, HiddenVerbPattern)> {
    report(text)
        .matches
        .iter()
        .map(|m| {
            (
                text[m.span.byte.clone()].to_owned(),
                m.term.clone(),
                m.pattern,
            )
        })
        .collect()
}

#[test]
fn light_verbs_and_of_phrases() {
    use HiddenVerbPattern::*;
    for (text, span, term, pattern) in [
        (
            "She made a decision.",
            "made a decision",
            "decide",
            LightVerb,
        ),
        (
            "They made decisions quickly.",
            "made decisions",
            "decide",
            LightVerb,
        ),
        (
            "He gave an explanation.",
            "gave an explanation",
            "explain",
            LightVerb,
        ),
        (
            "He gave her an explanation.",
            "gave her an explanation",
            "explain",
            LightVerb,
        ),
        (
            "We had a long discussion.",
            "had a long discussion",
            "discuss",
            LightVerb,
        ),
        (
            "She had no intention of going.",
            "had no intention",
            "intend",
            LightVerb,
        ),
        (
            "They reached a conclusion.",
            "reached a conclusion",
            "conclude",
            LightVerb,
        ),
        (
            "She came to the realization that he lied.",
            "came to the realization",
            "realize",
            LightVerb,
        ),
        (
            "They carried out an investigation.",
            "carried out an investigation",
            "investigate",
            LightVerb,
        ),
        (
            "He made a full confession.",
            "made a full confession",
            "confess",
            LightVerb,
        ),
        (
            "The police conducted an examination.",
            "conducted an examination",
            "examine",
            LightVerb,
        ),
        (
            "She offered no resistance.",
            "offered no resistance",
            "resist",
            LightVerb,
        ),
        (
            "They waited for the arrival of the train.",
            "the arrival of",
            "arrive",
            OfPhrase,
        ),
        (
            "The destruction of the city took a week.",
            "The destruction of",
            "destroy",
            OfPhrase,
        ),
        (
            "He watched the slow movement of her hand.",
            "the slow movement of",
            "move",
            OfPhrase,
        ),
    ] {
        let result = found(text);
        assert!(
            result.contains(&(span.to_owned(), term.to_owned(), pattern)),
            "{text}: {result:?}"
        );
    }
}

#[test]
fn other_frames_and_senses_are_not_hidden_verbs() {
    for text in [
        // The noun is not after a light verb or before "of".
        "The decision was hers.",
        "Her arrival surprised him.",
        "After the discussion, they left.",
        // Excluded nouns: other senses, places, fixed phrases.
        "She paid attention to the road.",
        "In addition to the cake, there was tea.",
        "He made a reservation.",
        "The information of the day was dull.",
        "He took action.",
        // Causative and possessive readings.
        "It made his isolation more complete.",
        "She had his statement.",
        "He has expectations.",
        "I must have employment.",
        // A compound's modifier.
        "They took the examination room.",
        // LanguageTool's MAKE_DECISIONS_ABOUT flags this one.
        "The board will make decisions about it.",
        // A name, not the noun.
        "They met Mr. Decision.",
    ] {
        assert!(found(text).is_empty(), "{text}: {:?}", found(text));
    }
}

#[test]
fn of_phrases_option() {
    let text = "She made a decision after the arrival of the train.";
    assert_eq!(report(text).matches.len(), 2);
    let light = report_with(text, &HiddenVerbsOptions::new().with_of_phrases(false));
    let terms: Vec<_> = light.matches.iter().map(|m| m.term.as_str()).collect();
    assert_eq!(terms, ["decide"]);
}

#[test]
fn records_name_the_verb_and_noun() {
    let text = "She made a quick decision. They made the decision of a lifetime.";
    let result = report(text);
    assert_eq!(result.matches.len(), 2, "{:?}", result.matches);
    let first = &result.matches[0];
    assert_eq!(&text[first.noun.byte.clone()], "decision");
    assert_eq!(
        &text[first.light_verb.as_ref().unwrap().byte.clone()],
        "made"
    );
    assert_eq!(first.nominalization, "decision");
    assert_eq!(first.term, "decide");
    // A light-verb match claims its noun, so the of-phrase after it is not reported again.
    assert_eq!(result.matches[1].pattern, HiddenVerbPattern::LightVerb);
    assert_eq!(result.terms.len(), 1);
    assert_eq!(result.terms[0].count, 2);
    let of = report("The arrival of the train.");
    assert_eq!(of.matches[0].light_verb, None);
}

#[test]
fn narration_and_dialogue_are_counted_separately() {
    let text = "She made a decision. “We had a discussion,” he said.";
    let result = report(text);
    let modes: Vec<_> = result
        .matches
        .iter()
        .map(|m| (&text[m.span.byte.clone()], m.mode))
        .collect();
    assert_eq!(
        modes,
        [
            ("made a decision", Mode::Narration),
            ("had a discussion", Mode::Dialogue)
        ]
    );
    assert_eq!(result.groups.narration.matches, 1);
    assert_eq!(result.groups.dialogue.matches, 1);
    assert_eq!(result.groups.all.matches, 2);
    let discuss = result.terms.iter().find(|t| t.term == "discuss").unwrap();
    assert_eq!((discuss.narration, discuss.dialogue), (0, 1));
    let rate = result.groups.dialogue.per_thousand_words.unwrap();
    assert!((rate - 1000.0 / result.groups.dialogue.words as f64).abs() < 1e-9);
    assert!(report("").matches.is_empty());
    assert_eq!(report("").groups.all.per_thousand_words, None);
}

#[test]
fn utf8_spans_around_typographic_punctuation() {
    for (text, expected) in [
        ("“She’d made a decision,” he said.", vec!["made a decision"]),
        (
            "He gave an explanation—then left.",
            vec!["gave an explanation"],
        ),
        (
            "They waited… the arrival of the train.",
            vec!["the arrival of"],
        ),
        (
            "She stood still.\u{2028}She made a decision.",
            vec!["made a decision"],
        ),
        ("Zoë’s departure—sudden—hurt.", vec![]),
        (
            "😀 They reached a conclusion.",
            vec!["reached a conclusion"],
        ),
    ] {
        let result = report(text);
        let found: Vec<_> = result
            .matches
            .iter()
            .map(|m| {
                assert!(text.is_char_boundary(m.span.byte.start));
                assert!(text.is_char_boundary(m.span.byte.end));
                let chars: String = text
                    .chars()
                    .skip(m.span.char.start)
                    .take(m.span.char.end - m.span.char.start)
                    .collect();
                assert_eq!(chars, text[m.span.byte.clone()]);
                assert!(
                    m.span.byte.start <= m.noun.byte.start && m.noun.byte.end <= m.span.byte.end
                );
                &text[m.span.byte.clone()]
            })
            .collect();
        assert_eq!(found, expected, "{text}");
    }
}

#[test]
fn options_bindings_and_errors() {
    // Astral characters make UTF-16 offsets differ from char offsets.
    let text = "😀 “Hi,” she said. 😀 Zoë made a decision.";
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let json: serde_json::Value =
        serde_json::from_str(&to_utf16_json(&retained.hidden_verbs(None).unwrap())).unwrap();
    let start = text.encode_utf16().count() - "made a decision.".encode_utf16().count();
    assert_eq!(json["matches"][0]["span"]["start"], start);
    assert_eq!(
        json["matches"][0]["span"]["end"],
        start + "made a decision".len()
    );
    assert_eq!(json["matches"][0]["lightVerb"]["start"], start);
    assert_eq!(json["matches"][0]["pattern"], "lightVerb");
    assert_eq!(json["matches"][0]["term"], "decide");
    assert_eq!(json["terms"][0]["nominalization"], "decision");
    assert!(retained.hidden_verbs(Some("{\"ofPhrases\":false}")).is_ok());
    assert!(retained
        .hidden_verbs(Some("{\"of_phrases\":false}"))
        .is_ok());
    assert!(matches!(
        retained.hidden_verbs(Some("{\"typo\":true}")),
        Err(StyleError::InvalidOptions { .. })
    ));
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        hidden_verbs::report(&compact, &HiddenVerbsOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn bundled_table_carries_the_wordnet_notice() {
    let data = include_str!("../data/hidden-verbs/nominalizations.tsv");
    assert!(data.contains("WordNet 3.0 Copyright 2006 by Princeton University"));
    assert!(data.contains("style/data/licenses/WordNet-LICENSE"));
    let rows: Vec<_> = data.lines().filter(|line| !line.starts_with('#')).collect();
    assert!(rows.len() > 250 && rows.len() < 500, "{}", rows.len());
    let emotions = include_str!("../data/emotions/emotions.tsv");
    for row in &rows {
        let (noun, verb) = row.split_once('\t').expect("noun and verb");
        assert!(noun.chars().all(|c| c.is_ascii_lowercase()), "{row}");
        assert!(verb.chars().all(|c| c.is_ascii_lowercase()), "{row}");
        assert!(
            ["tion", "sion", "ment", "ance", "ence", "al", "ure"]
                .iter()
                .any(|suffix| noun.ends_with(suffix)),
            "{row}"
        );
        assert!(
            !emotions
                .lines()
                .any(|line| line.split('\t').next() == Some(noun)),
            "{noun} is in the emotion lexicon"
        );
    }
    let mut sorted = rows.clone();
    sorted.sort();
    assert_eq!(rows, sorted);
    let notice = include_str!("../../NOTICE");
    assert!(notice.contains("style/data/hidden-verbs/nominalizations.tsv"));
}
