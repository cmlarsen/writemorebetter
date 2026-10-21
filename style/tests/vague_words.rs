use nlprule::Tokenizer;
use std::sync::{Arc, OnceLock};
use writemorebetter_style::reports::vague_words::{
    self, VagueWordCategory, VagueWordsOptions, VagueWordsReport, COVERED,
};
use writemorebetter_style::reports::{to_utf16_json, StyleResources};
use writemorebetter_style::{analyze_document, DocumentOptions, Mode, StyleDocument, StyleError};

fn tokenizer() -> &'static Arc<Tokenizer> {
    static TOKENIZER: OnceLock<Arc<Tokenizer>> = OnceLock::new();
    TOKENIZER.get_or_init(|| Arc::new(Tokenizer::new("../storage/en_tokenizer.bin").unwrap()))
}

fn report_with(text: &str, options: &VagueWordsOptions) -> VagueWordsReport {
    let document = analyze_document(tokenizer(), text, &DocumentOptions::new());
    vague_words::report(&document, options).unwrap()
}

fn report(text: &str) -> VagueWordsReport {
    report_with(text, &VagueWordsOptions::new())
}

fn found(text: &str) -> Vec<(String, String)> {
    report(text)
        .matches
        .iter()
        .map(|m| (text[m.span.byte.clone()].to_owned(), m.term.clone()))
        .collect()
}

fn spans(text: &str) -> Vec<String> {
    found(text).into_iter().map(|(span, _)| span).collect()
}

#[test]
fn vague_words_by_category() {
    use VagueWordCategory::*;
    for (text, span, term, category) in [
        ("She was sort of tired.", "sort of", "sort of", Hedge),
        ("He felt kind of sick.", "kind of", "kind of", Hedge),
        ("The room was a little cold.", "a little", "a little", Hedge),
        ("She was a bit late.", "a bit", "a bit", Hedge),
        ("It was somewhat odd.", "somewhat", "somewhat", Hedge),
        ("In a way, it helped.", "In a way", "in a way", Hedge),
        ("He waited an hour or so.", "or so", "or so", Hedge),
        (
            "Bring a cake or something.",
            "or something",
            "or something",
            Hedge,
        ),
        ("She seemed to know him.", "seemed to", "seem to", Hedge),
        ("They had a lot of money.", "a lot", "a lot", Quantity),
        ("He owned lots of boats.", "lots of", "lots of", Quantity),
        (
            "She had a number of cats.",
            "a number of",
            "a number of",
            Quantity,
        ),
        ("He tried various doors.", "various", "various", Quantity),
        ("A thing moved in the dark.", "thing", "thing", Noun),
        ("She kept her things there.", "things", "thing", Noun),
        ("He sorted the stuff.", "stuff", "stuff", Noun),
        ("It was a nice day.", "nice", "nice", Adjective),
        ("The view was amazing.", "amazing", "amazing", Adjective),
        ("She was basically right.", "basically", "basically", Filler),
        ("He was probably asleep.", "probably", "probably", Filler),
        ("She began to run.", "began", "begin", Filler),
        ("He started walking.", "started", "start", Filler),
        ("It was so cold.", "so", "so", Filler),
        ("It was too late.", "too", "too", Filler),
        ("It was, like, huge.", "like", "like", Filler),
        ("Well, I suppose.", "Well", "well", Filler),
        ("He might be home.", "might", "might", Filler),
    ] {
        let result = report(text);
        let found: Vec<_> = result
            .matches
            .iter()
            .map(|m| (&text[m.span.byte.clone()], m.term.as_str(), m.category))
            .collect();
        assert!(found.contains(&(span, term, category)), "{text}: {found:?}");
    }
}

#[test]
fn literal_and_fixed_uses_are_not_vague() {
    for text in [
        // Types, sizes and amounts, not hedges.
        "What kind of man was he?",
        "It was a strange sort of creak.",
        "A little girl ran past.",
        "He ate a bit of bread.",
        "A little withered old man sat there.",
        "He talked in a way we never had.",
        "It mattered to a degree that surprised her.",
        "Or so he said.",
        "She seemed to him a stranger.",
        // Fixed phrases and other senses.
        "And so on.",
        "He did so gladly.",
        "She would rather not.",
        "Or rather, Sunday.",
        "It was as much as he had.",
        "It was the most expensive house.",
        "Like most old ships, it leaked.",
        "A giant in might.",
        "She looked like her mother.",
        "That's all right.",
        "He did it well.",
        "Me too.",
        "They flew to Nice in May.",
        "She stuffed the bag.",
        "He tried the door.",
    ] {
        assert!(spans(text).is_empty(), "{text}: {:?}", found(text));
    }
}

#[test]
fn words_other_checks_flag_are_left_out() {
    // TIRED_INTENSIFIERS (really, very, quite, extremely) and the overused-words fillers
    // (actually, just, simply) report these words already.
    assert_eq!(
        COVERED,
        [
            "really",
            "very",
            "quite",
            "extremely",
            "actually",
            "just",
            "simply"
        ]
    );
    assert!(
        spans("It was really very quite extremely cold. She actually just simply left.").is_empty()
    );
    let fillers = include_str!("../data/vague/fillers.txt");
    for word in COVERED {
        assert!(fillers.lines().any(|line| line == word), "{word}");
    }
}

#[test]
fn fillers_option_keeps_only_the_project_list() {
    let text = "Basically, it was a nice thing.";
    assert_eq!(spans(text), ["Basically", "nice", "thing"]);
    let without = report_with(text, &VagueWordsOptions::new().with_fillers(false));
    let terms: Vec<_> = without.matches.iter().map(|m| m.term.as_str()).collect();
    assert_eq!(terms, ["nice", "thing"]);
}

#[test]
fn narration_and_dialogue_are_counted_separately() {
    let text = "It was sort of cold. “Well, it’s kind of nice,” she said.";
    let result = report(text);
    let modes: Vec<_> = result
        .matches
        .iter()
        .map(|m| (&text[m.span.byte.clone()], m.mode))
        .collect();
    assert_eq!(
        modes,
        [
            ("sort of", Mode::Narration),
            ("Well", Mode::Dialogue),
            ("kind of", Mode::Dialogue),
            ("nice", Mode::Dialogue),
        ]
    );
    assert_eq!(result.groups.narration.matches, 1);
    assert_eq!(result.groups.dialogue.matches, 3);
    assert_eq!(result.groups.all.matches, 4);
    assert_eq!(
        result.groups.all.words,
        result.groups.narration.words + result.groups.dialogue.words
    );
    let rate = result.groups.narration.per_thousand_words.unwrap();
    assert!((rate - 1000.0 / result.groups.narration.words as f64).abs() < 1e-9);
    let sort = result.terms.iter().find(|t| t.term == "sort of").unwrap();
    assert_eq!((sort.count, sort.narration, sort.dialogue), (1, 1, 0));
    assert!(report("").matches.is_empty());
    assert_eq!(report("").groups.all.per_thousand_words, None);
}

#[test]
fn terms_are_sorted_by_count() {
    let result = report("A thing. Another thing. A nice thing. A nice day.");
    let terms: Vec<_> = result
        .terms
        .iter()
        .map(|t| (t.term.as_str(), t.count, t.category))
        .collect();
    assert_eq!(
        terms,
        [
            ("thing", 3, VagueWordCategory::Noun),
            ("nice", 2, VagueWordCategory::Adjective)
        ]
    );
}

#[test]
fn skipped_names_are_not_counted() {
    let text = "Nice smiled. Nice was a nice girl.";
    let options = DocumentOptions::new().with_skip_names(vec!["Nice".into()]);
    let document = analyze_document(tokenizer(), text, &options);
    let result = vague_words::report(&document, &VagueWordsOptions::new()).unwrap();
    let found: Vec<_> = result
        .matches
        .iter()
        .map(|m| &text[m.span.byte.clone()])
        .collect();
    assert_eq!(found, ["nice"]);
}

#[test]
fn utf8_spans_around_typographic_punctuation() {
    for (text, expected) in [
        ("“It’s sort of cold,” she said.", vec!["sort of"]),
        ("She’d begun to run—then stopped.", vec!["begun"]),
        (
            "It was a little cold… or something.",
            vec!["a little", "or something"],
        ),
        (
            "She stood still.\u{2028}It was kind of nice.",
            vec!["kind of", "nice"],
        ),
        ("Zoë’s things—all of them—were there.", vec!["things"]),
        ("😀 It was too late.", vec!["too"]),
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
                &text[m.span.byte.clone()]
            })
            .collect();
        assert_eq!(found, expected, "{text}");
    }
}

#[test]
fn options_bindings_and_errors() {
    // Astral characters make UTF-16 offsets differ from char offsets.
    let text = "😀 “Hi,” she said. 😀 Zoë was kind of tired.";
    let retained = StyleDocument::new(
        tokenizer().clone(),
        Arc::new(StyleResources::new()),
        text,
        &DocumentOptions::new(),
    );
    let json: serde_json::Value =
        serde_json::from_str(&to_utf16_json(&retained.vague_words(None).unwrap())).unwrap();
    let start = text.encode_utf16().count() - "kind of tired.".encode_utf16().count();
    assert_eq!(json["matches"][0]["span"]["start"], start);
    assert_eq!(json["matches"][0]["span"]["end"], start + "kind of".len());
    assert_eq!(json["matches"][0]["category"], "hedge");
    assert_eq!(json["matches"][0]["mode"], "narration");
    assert_eq!(json["terms"][0]["term"], "kind of");
    assert!(json["groups"]["narration"]["perThousandWords"].is_number());
    assert!(retained.vague_words(Some("{\"fillers\":false}")).is_ok());
    assert!(matches!(
        retained.vague_words(Some("{\"typo\":true}")),
        Err(StyleError::InvalidOptions { .. })
    ));
    let compact = analyze_document(
        tokenizer(),
        text,
        &DocumentOptions::new().with_token_details(false),
    );
    assert!(matches!(
        vague_words::report(&compact, &VagueWordsOptions::new()),
        Err(StyleError::NeedsTokenDetails { .. })
    ));
}

#[test]
fn bundled_lists_carry_their_sources() {
    let fillers = include_str!("../data/vague/fillers.txt");
    assert!(fillers.contains("Titus Wormer"));
    assert!(fillers.contains("mikpanko/expresso"));
    assert!(fillers.contains("style/data/licenses/fillers.txt"));
    let rows: Vec<_> = fillers.lines().filter(|l| !l.starts_with('#')).collect();
    assert_eq!(rows.len(), 80);
    for row in &rows {
        assert!(
            !row.is_empty() && row.chars().all(|c| c.is_ascii_lowercase()),
            "{row}"
        );
    }
    // The three words/fillers entries expresso does not back are not vendored.
    for word in ["amazing", "stuff", "things"] {
        assert!(!rows.contains(&word), "{word}");
    }
    let license = include_str!("../data/licenses/fillers.txt");
    assert!(license.contains("Copyright (c) 2014 Titus Wormer"));
    assert_eq!(license.matches("Permission is hereby granted").count(), 2);
    let list = include_str!("../data/vague/vague-words.tsv");
    for row in list.lines().filter(|l| !l.starts_with('#')) {
        let (category, phrase) = row.split_once('\t').expect("category and phrase");
        assert!(
            ["hedge", "quantity", "noun", "adjective"].contains(&category),
            "{row}"
        );
        assert!(!phrase.is_empty() && phrase == phrase.to_lowercase());
        assert!(!rows.contains(&phrase), "{phrase} is also a filler");
    }
}
